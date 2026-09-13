//! `ChildSyncEngine`: one sync cycle for one child.
//!
//! # Thread ownership
//!
//! `sync_manager.rs:36-38` states the architecture: "UI owns all repo I/O".
//! [`ChildSyncEngine::cycle`] runs on the background thread and is the half
//! that preserves that invariant — it only ever touches `.git` objects and
//! refs (`fetch_lgs`, and reading git **blobs** for provenance/merge input,
//! never the working tree) plus pure CPU (`allowance_core::merge`). It never
//! writes a file. When histories have diverged, `cycle` returns the computed
//! merge as data; applying it — writing `transactions.csv`, creating the
//! merge commit, and pushing — is `SyncMessage::ApplyMerge`'s job, handled on
//! the UI thread in `app_coordinator.rs`.
//!
//! # The refspec is the whole design
//!
//! `refs/remotes/lgs-auth/main` (landed by [`fetch_lgs`]), never
//! `refs/remotes/lgs/main` — see the doc comment on
//! `storage::git::LGS_AUTH_REFSPEC` for why. `classify` and `cycle` both key
//! off this.
//!
//! # Scope: `transactions.csv` only
//!
//! `allowance_core::merge` models `TxRow`, not a goal row — `goals.csv` is a
//! known, recorded gap (see the project's SDD ledger, "SPEC-COVERAGE GAP
//! FOUND" at Task 6). This module never invents a goal merge. What it DOES
//! do is make sure a diverged `goals.csv` is never silently resolved by
//! picking a side: [`goals_diverged`] detects the divergence and the
//! `ApplyMerge` handler on the UI thread logs a loud warning naming both
//! commits before it commits (see `app_coordinator.rs`) — the local copy is
//! left exactly as it is (a real merge commit still needs *some* tree, and
//! the working tree it stages already holds the local copy; the point is
//! that this is surfaced, not that it is technically avoidable).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use git2::{Oid, Repository};

use allowance_core::merge::{merge, Decision};
use allowance_core::row::{Provenance, Sided, TxRow};
use shared::ChildId;

use crate::backend::storage::csv::CsvConnection;
use crate::backend::storage::git::{ensure_lgs_remote, fetch_lgs, push_lgs};
use crate::backend::sync::lgs_client::LgsClient;

/// The ref the merge input is read from. See the module doc and
/// `storage::git::LGS_AUTH_REFSPEC` for why this, and never
/// `refs/remotes/lgs/main`.
const LGS_AUTH_MAIN: &str = "refs/remotes/lgs-auth/main";

/// The filename this module refuses to merge — see the module doc's
/// "Scope" section.
const GOALS_FILE: &str = "goals.csv";
const TRANSACTIONS_FILE: &str = "transactions.csv";

/// How many times [`cycle_against`]'s `Ahead` push, and `apply_merge`'s
/// post-merge push (`app_coordinator.rs`), each retry before giving up for
/// this cycle. See [`push_with_retry_inner`] for why bounded rather than
/// unbounded.
const PUSH_RETRY_MAX: u8 = 3;

/// The outcome of comparing `ours`, the authoritative peer tip (`auth`), and
/// their merge base.
///
/// Pure and total over three already-resolved commit identifiers (in
/// practice, hex OIDs) — deliberately separable from [`ChildSyncEngine::cycle`]
/// so the branch decision is testable without a repository at all. `cycle`
/// resolves `ours`/`auth`/`base` via git2 (including the ancestry walk — see
/// its doc comment for why comparing to `base` is equivalent to
/// `graph_descendant_of`) and then calls this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cycle {
    UpToDate,
    FastForward,
    /// We are strictly ahead of `auth` (`auth` is an ancestor of `ours`).
    /// There is nothing to merge — the correct action is to push and stop.
    ///
    /// This is NOT a rare edge case. lgs's `reconcile` mirrors
    /// `refs/lgs-auth/heads/<branch>` **unconditionally on every daemon
    /// tick** (`local-git-sync/src/durability/engine.rs:458`) — `auth` is
    /// simply "the last tip the daemon has published," not "the tip at the
    /// last genuine divergence." Any ordinary local commit — a user adding
    /// one transaction — puts `ours` ahead of whatever `auth` currently
    /// reads. Treating that as `Diverged` would run an empty merge (the
    /// `theirs` side is unchanged from `base`, so `merge` resolves to
    /// `ours` verbatim) and create a new merge commit that leaves us one
    /// commit further ahead — which classifies as `Diverged` again next
    /// cycle, forever, for as long as a push is failing, the daemon is
    /// down, or the cloud round-trip lags. That is an unbounded stream of
    /// empty merge commits into the user's allowance history with no
    /// brake except lgs eventually republishing our tip.
    Ahead,
    Diverged,
}

/// Classify a sync cycle from three optional, already-resolved commit
/// identifiers: our local tip, the authoritative peer tip (`None` when no
/// `refs/lgs-auth/*` ref has landed yet — nothing to reconcile against), and
/// their merge base (`None` when there is no common ancestor).
///
/// - No known peer tip at all -> `UpToDate` (nothing to compare against).
/// - We have no history yet but the peer does -> `FastForward` (adopt their
///   tip outright).
/// - Same commit on both sides -> `UpToDate`.
/// - We have not moved since the merge base (the peer is strictly ahead) ->
///   `FastForward`. This is equivalent to `auth` being a descendant of
///   `ours`: `git merge-base(ours, auth) == ours` exactly when `ours` is an
///   ancestor of `auth`.
/// - The peer has not moved since the merge base (we are strictly ahead) ->
///   `Ahead`. Equivalent to `ours` being a descendant of `auth`:
///   `git merge-base(ours, auth) == auth` exactly when `auth` is an
///   ancestor of `ours`. See [`Cycle::Ahead`] for why this must be its own
///   arm rather than falling into `Diverged`.
/// - Anything else — neither side is an ancestor of the other — ->
///   `Diverged`, a genuine three-way merge.
pub fn classify(ours: Option<&str>, auth: Option<&str>, base: Option<&str>) -> Cycle {
    match (ours, auth) {
        (_, None) => Cycle::UpToDate,
        (None, Some(_)) => Cycle::FastForward,
        (Some(o), Some(a)) if o == a => Cycle::UpToDate,
        (Some(o), Some(_)) if base == Some(o) => Cycle::FastForward,
        (Some(_), Some(a)) if base == Some(a) => Cycle::Ahead,
        _ => Cycle::Diverged,
    }
}

/// What one call to [`ChildSyncEngine::cycle`] found.
#[derive(Debug, Clone)]
pub enum CycleOutcome {
    /// Local tip already matches the authoritative peer tip.
    UpToDate,
    /// The peer is strictly ahead. The caller should check out `to` (UI
    /// thread) and reload — no merge needed.
    FastForward { to: String },
    /// We are strictly ahead of the peer's last published tip. Nothing to
    /// merge — `cycle_against` has already pushed our tip (push is a
    /// background-thread operation, same as fetch) before returning this.
    /// See [`Cycle::Ahead`] for why this must never be treated as
    /// `Diverged`.
    Ahead,
    /// Histories diverged. The merge already ran (pure CPU, done here on the
    /// background thread); `rows`/`decisions` are the result and `parents`
    /// names the two commits the caller must pass to `commit_merge`.
    Merged {
        rows: Vec<TxRow>,
        parents: (String, String),
        decisions: Vec<Decision>,
        /// `true` when `goals.csv` differs between `ours` and `theirs` at
        /// the diverged tips. See the module doc's "Scope" section — this
        /// is informational only; the transactions merge proceeds either
        /// way, but the caller must not treat this cycle as a full,
        /// silent, everything-synced success when it is `true`.
        goals_diverged: bool,
    },
}

/// One sync cycle for one child. See the module doc for thread ownership.
pub struct ChildSyncEngine {
    lgs: LgsClient,
    connection: Arc<CsvConnection>,
}

impl ChildSyncEngine {
    pub fn new(lgs: LgsClient, connection: Arc<CsvConnection>) -> Self {
        Self { lgs, connection }
    }

    /// The child's working repo. Each child directory is its own git repo —
    /// see `storage::git`'s module doc.
    fn work_dir(&self, child_id: &ChildId) -> Result<PathBuf> {
        self.connection.child_dir(child_id)
    }

    /// Resolve this child's lgs clone URL from `lgs status --json`, never
    /// frozen — see the design spec's "remote URL is re-resolved, never
    /// frozen" and `storage::git::ensure_lgs_remote`'s doc comment. Project
    /// naming convention: `allowance-<child_id>`.
    fn clone_url(&self, child_id: &ChildId) -> Result<String> {
        let project_name = format!("allowance-{child_id}");
        let status = self.lgs.status().context("running `lgs status --json`")?;
        let project = status.project(&project_name).ok_or_else(|| {
            anyhow::anyhow!(
                "lgs has no project named '{project_name}' — has child '{child_id}' been \
                 registered with lgs yet?"
            )
        })?;
        Ok(project.clone_url.clone())
    }

    /// Fetch, classify, and (when diverged) merge. Never touches the
    /// working tree. Resolves the remote via `lgs status --json`, then
    /// delegates to [`Self::cycle_against`] — the actual mechanics, which
    /// take the remote URL directly and so can be driven in tests against a
    /// local bare repo in a tempdir instead of the real lgs daemon (the
    /// whole point of the refspec-based design is that this code does not
    /// care that the remote is lgs).
    pub fn cycle(&self, child_id: &ChildId) -> Result<CycleOutcome> {
        let work_dir = self.work_dir(child_id)?;
        let clone_url = self.clone_url(child_id)?;
        Self::cycle_against(&work_dir, &clone_url)
    }

    fn cycle_against(work_dir: &Path, clone_url: &str) -> Result<CycleOutcome> {
        let repo = Repository::open(work_dir)
            .with_context(|| format!("opening child repo at {}", work_dir.display()))?;
        ensure_lgs_remote(&repo, clone_url)?;
        fetch_lgs(&repo)?;

        let ours_oid = repo
            .head()
            .context("child repo has no HEAD — it must already have at least one commit")?
            .peel_to_commit()?
            .id();

        let auth_oid = match repo.find_reference(LGS_AUTH_MAIN) {
            Ok(r) => Some(r.peel_to_commit()?.id()),
            Err(_) => None,
        };
        let base_oid = auth_oid.and_then(|auth| repo.merge_base(ours_oid, auth).ok());

        let ours_str = ours_oid.to_string();
        let auth_str = auth_oid.map(|o| o.to_string());
        let base_str = base_oid.map(|o| o.to_string());

        match classify(Some(&ours_str), auth_str.as_deref(), base_str.as_deref()) {
            Cycle::UpToDate => Ok(CycleOutcome::UpToDate),
            Cycle::FastForward => {
                let auth_oid = auth_oid.expect("Cycle::FastForward implies classify saw Some(auth)");
                Ok(CycleOutcome::FastForward { to: auth_oid.to_string() })
            }
            Cycle::Ahead => {
                // Nothing to merge — push our tip and stop. Push is a
                // background-thread operation (same as fetch; see the
                // module doc's thread-ownership table), so this does not
                // violate "UI owns all working-tree I/O": nothing here
                // touches a file, only `.git` refs/objects over the wire.
                let branch = current_branch(&repo)?;
                push_with_retry(&repo, &branch, PUSH_RETRY_MAX).with_context(|| {
                    format!(
                        "pushing branch '{branch}' after determining we are ahead of the peer's \
                         last published tip (retried up to {PUSH_RETRY_MAX} times)"
                    )
                })?;
                Ok(CycleOutcome::Ahead)
            }
            Cycle::Diverged => {
                let auth_oid = auth_oid.expect("Cycle::Diverged implies classify saw Some(auth)");

                // Read all three sides as blobs and resolve provenance here,
                // so the merge itself never walks history.
                let base = base_oid.map(|o| read_rows(&repo, o)).transpose()?;
                let ours = Sided {
                    rows: read_rows(&repo, ours_oid)?,
                    provenance: provenance(&repo, ours_oid)?,
                };
                let theirs = Sided {
                    rows: read_rows(&repo, auth_oid)?,
                    provenance: provenance(&repo, auth_oid)?,
                };

                let diverged = goals_diverged(&repo, ours_oid, auth_oid)?;
                if diverged {
                    log::warn!(
                        "goals.csv diverged between the local tip ({ours_oid}) and the \
                         authoritative peer tip ({auth_oid}); allowance_core::merge does not \
                         model goals, so this cycle's transactions merge proceeds but goals.csv \
                         is left exactly as it is locally. Any goal edits made on the other \
                         machine are NOT reflected here and must be reconciled by hand."
                    );
                }

                let outcome = merge(base.as_deref(), &ours, &theirs);
                Ok(CycleOutcome::Merged {
                    rows: outcome.rows,
                    parents: (ours_oid.to_string(), auth_oid.to_string()),
                    decisions: outcome.decisions,
                    goals_diverged: diverged,
                })
            }
        }
    }
}

/// The branch currently checked out, by shorthand name (e.g. `"main"`).
/// Shared by `cycle_against`'s `Cycle::Ahead` push and by
/// `app_coordinator::apply_merge`'s post-merge push, so both resolve the
/// branch to push the same way rather than each hardcoding `"main"`.
pub(crate) fn current_branch(repo: &Repository) -> Result<String> {
    let head = repo.head().context("resolving current branch")?;
    head.shorthand()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("HEAD is not a valid UTF-8 branch name"))
}

/// Outcome of [`recover_if_dirty`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovered {
    /// The working tree already matched HEAD — nothing to recover.
    Clean,
    /// The working tree was dirty and has been hard-reset back to HEAD. The
    /// caller must treat this exactly like starting fresh: re-fetch,
    /// re-classify, and (if still diverged) re-merge and re-apply.
    DiscardedAndReMerged,
}

/// Recover a child repo whose working tree is dirty for a reason that must
/// never happen in ordinary operation: a crash between `apply_merge`
/// writing `transactions.csv` and it creating the follow-up merge commit
/// (`app_coordinator.rs`). Call this before writing anything, so a leftover
/// half-written file from a previous crash can never be mistaken for
/// legitimate content or committed alongside a new merge's tree.
///
/// # Why discarding — never salvaging — the dirty state is safe
///
/// The instinct on finding unexpected uncommitted content is to inspect and
/// try to recover it. Do not do that here. `allowance_core::merge` is a
/// pure, deterministic function of `(base, ours, theirs)`: `ours` is still
/// exactly `HEAD` (a dirty working tree never moves `HEAD`), and `theirs`
/// is still sitting in the object database whether or not this attempt to
/// apply it survives. So re-running the *same* cycle from those two inputs
/// reproduces byte-for-byte the same rows the crashed attempt was in the
/// middle of writing — there is nothing recoverable on disk that a clean
/// re-run cannot recompute exactly. What IS on disk after an unclean
/// shutdown, by contrast, could be a torn write (a partial `fs::write`), or
/// — if the crash landed mid-stage — an index that disagrees with either
/// the old committed content or the new merged content. Treating that as
/// salvageable risks committing exactly the kind of corrupted or
/// half-applied row this whole design exists to prevent. Discard it and let
/// the deterministic merge regenerate it; do not replace this function with
/// an attempt to inspect or keep any part of the dirty state.
pub fn recover_if_dirty(repo: &Repository) -> Result<Recovered> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false);
    if repo
        .statuses(Some(&mut opts))
        .context("checking child repo status before applying a merge")?
        .is_empty()
    {
        return Ok(Recovered::Clean);
    }
    let head = repo
        .head()
        .context("resolving HEAD to recover a dirty working tree")?
        .peel_to_commit()
        .context("peeling HEAD to a commit to recover a dirty working tree")?;
    repo.reset(head.as_object(), git2::ResetType::Hard, None)
        .context("hard-resetting a dirty working tree back to HEAD")?;
    Ok(Recovered::DiscardedAndReMerged)
}

/// Attempt `attempt` up to `max` times, returning at the first success.
///
/// Deliberately bounded, and deliberately with no sleep or backoff between
/// attempts. `lgs` mirrors `refs/lgs-auth/*` on every daemon tick
/// independently of this process (see [`Cycle::Ahead`]'s doc comment), so a
/// push rejected because the remote moved under us is not a condition that
/// retrying *this* call harder is likely to fix — it needs a fresh
/// fetch+classify, which is the next scheduled cycle's job. Retrying
/// without a bound would spin this call forever whenever a push keeps
/// losing that race, or whenever the daemon is simply down.
pub fn push_with_retry_inner<F>(max: u8, mut attempt: F) -> Result<()>
where
    F: FnMut() -> Result<()>,
{
    let mut last = None;
    for _ in 0..max {
        match attempt() {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    // Leaving it for the next scheduled cycle is correct, not a data-loss
    // risk: the commit this push is trying to publish is already durable on
    // this machine's disk (in the repo's object database), so nothing is
    // lost by stopping here. The next cycle's fetch+classify will see us
    // `Ahead` (or fast-forwardable) again and try the push again.
    Err(last.unwrap_or_else(|| anyhow!("push failed")))
}

/// Push `branch` to the `lgs` remote, retrying up to `max` times before
/// giving up for this cycle. See [`push_with_retry_inner`] for why this is
/// bounded rather than unbounded or backed off with a sleep.
pub fn push_with_retry(repo: &Repository, branch: &str, max: u8) -> Result<()> {
    push_with_retry_inner(max, || push_lgs(repo, branch))
}

/// Read `transactions.csv` as of `oid`, parsed. An absent file (a commit
/// that predates the file, or an unrelated root) reads as no rows — the
/// merge already treats a `None` base as "union both sides", and an empty
/// side behaves the same way through the ordinary add path.
fn read_rows(repo: &Repository, oid: Oid) -> Result<Vec<TxRow>> {
    match read_blob_at(repo, oid, TRANSACTIONS_FILE)? {
        None => Ok(Vec::new()),
        Some(bytes) => {
            let text = std::str::from_utf8(&bytes).with_context(|| {
                format!("{TRANSACTIONS_FILE} at commit {oid} is not valid UTF-8")
            })?;
            let parsed = allowance_core::codec::parse_transactions(text)
                .with_context(|| format!("parsing {TRANSACTIONS_FILE} at commit {oid}"))?;
            Ok(parsed.rows)
        }
    }
}

/// `true` when `goals.csv` differs (by raw bytes, including "present on one
/// side, absent on the other") between the two diverged tips. Deliberately
/// byte-level, not parsed — allowance-core models no goal row, so this
/// module has no basis to compare goals except "did the file change at
/// all". See the module doc's "Scope" section for what happens next.
pub(crate) fn goals_diverged(repo: &Repository, ours: Oid, theirs: Oid) -> Result<bool> {
    let ours_goals = read_blob_at(repo, ours, GOALS_FILE)?;
    let theirs_goals = read_blob_at(repo, theirs, GOALS_FILE)?;
    Ok(ours_goals != theirs_goals)
}

/// Read a file's raw bytes out of a commit's tree. `Ok(None)` when the path
/// is not present at that commit (never an error — an older commit
/// predating the file, or a project with no goals.csv yet, is normal).
fn read_blob_at(repo: &Repository, oid: Oid, path: &str) -> Result<Option<Vec<u8>>> {
    let commit = repo.find_commit(oid).with_context(|| format!("resolving commit {oid}"))?;
    let tree = commit.tree().with_context(|| format!("reading tree of commit {oid}"))?;
    match tree.get_path(Path::new(path)) {
        Ok(entry) => {
            let blob = repo
                .find_blob(entry.id())
                .with_context(|| format!("reading blob for {path} at commit {oid}"))?;
            Ok(Some(blob.content().to_vec()))
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Resolve a commit's provenance for the merge: its own committer timestamp
/// and its own oid. Per-side and real — never a shared placeholder. `merge`'s
/// `wins()` has `debug_assert!(ours != theirs)`; synthesising one
/// `Provenance` for both sides would panic in debug builds the moment an
/// add/add collision needs a tiebreak.
fn provenance(repo: &Repository, oid: Oid) -> Result<Provenance> {
    let commit = repo.find_commit(oid).with_context(|| format!("resolving commit {oid}"))?;
    let committer_epoch = commit.committer().when().seconds();
    let raw = oid.as_bytes();
    if raw.len() != 20 {
        // `Provenance::commit_oid` is a fixed 20-byte SHA-1. A SHA-256
        // repository's `Oid` is 32 bytes; `copy_from_slice` panics on a
        // length mismatch rather than truncating, so this must be checked
        // and reported, not assumed.
        anyhow::bail!(
            "commit {oid} has a {}-byte oid, not the 20-byte SHA-1 this app's Provenance type \
             assumes — SHA-256 repositories are not supported",
            raw.len()
        );
    }
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(raw);
    Ok(Provenance { committer_epoch, commit_oid: bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::git::{clone_repo, GitManager};

    // --- classify: pure, no repository ------------------------------------

    #[test]
    fn classifies_up_to_date_fast_forward_and_diverged() {
        assert_eq!(classify(Some("x"), Some("x"), Some("x")), Cycle::UpToDate);
        // ours is an ancestor of the auth tip -> fast-forward.
        assert_eq!(classify(Some("a"), Some("b"), Some("a")), Cycle::FastForward);
        // neither is an ancestor of the other -> merge.
        assert_eq!(classify(Some("a"), Some("b"), Some("base")), Cycle::Diverged);
    }

    #[test]
    fn classify_with_no_known_peer_tip_is_up_to_date() {
        assert_eq!(classify(Some("a"), None, None), Cycle::UpToDate);
    }

    #[test]
    fn classify_with_no_local_history_adopts_the_peer_outright() {
        assert_eq!(classify(None, Some("a"), None), Cycle::FastForward);
    }

    /// Regression for Critical-1: lgs mirrors `refs/lgs-auth/*`
    /// unconditionally on every daemon tick, so `auth` is just "the peer's
    /// last published tip," not "the tip at a genuine divergence." Being
    /// strictly ahead of it — the ordinary state after any local commit —
    /// must classify as `Ahead`, never `Diverged`: `Diverged` would drive an
    /// empty merge commit every cycle, unboundedly, while a push is
    /// pending.
    #[test]
    fn classify_ahead_and_fast_forward_are_true_mirror_images() {
        // Peer ahead of us (auth is a descendant of ours) -> FastForward.
        assert_eq!(classify(Some("a"), Some("b"), Some("a")), Cycle::FastForward);
        // We are ahead of the peer (ours is a descendant of auth) -> Ahead,
        // NOT Diverged.
        assert_eq!(classify(Some("b"), Some("a"), Some("a")), Cycle::Ahead);
    }

    #[test]
    fn classify_diverged_requires_that_neither_side_is_an_ancestor_of_the_other() {
        // base differs from BOTH ours and auth -> a genuine divergence.
        assert_eq!(classify(Some("a"), Some("b"), Some("base")), Cycle::Diverged);
    }

    // --- cycle_against: real repos, local bare remotes only ---------------
    //
    // No `lgs` binary and no daemon anywhere in these tests — `cycle_against`
    // takes the remote URL directly, and a local bare repo in a tempdir is
    // indistinguishable to git2 from an lgs-backed one. This is the seam the
    // whole refspec design is built to make testable offline.

    /// Commit directly against a repository's object database (no working
    /// tree or index needed), so this also works against a bare repo — used
    /// to plant the "peer's" commits without ever checking them out.
    fn commit_with_files(
        repo: &Repository,
        message: &str,
        parents: &[&git2::Commit],
        files: &[(&str, &str)],
        timestamp: i64,
    ) -> Oid {
        let sig = git2::Signature::new("Test", "test@example.com", &git2::Time::new(timestamp, 0))
            .unwrap();
        let mut builder = repo.treebuilder(None).unwrap();
        for (name, content) in files {
            let blob_id = repo.blob(content.as_bytes()).unwrap();
            builder.insert(name, blob_id, 0o100644).unwrap();
        }
        let tree_id = builder.write().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(None, &sig, &sig, message, &tree, parents).unwrap()
    }

    const TX_A: &str = "id,child_id,date,description,amount,balance,type\n\
in-1-a,keiko,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n";
    const TX_OURS: &str = "id,child_id,date,description,amount,balance,type\n\
in-1-a,keiko,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n\
ex-2-a,keiko,2026-01-02T00:00:00+00:00,Slime,-5.00,5.00,expense\n";
    const TX_THEIRS: &str = "id,child_id,date,description,amount,balance,type\n\
in-1-a,keiko,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n\
ex-3-a,keiko,2026-01-03T00:00:00+00:00,Book,-3.00,7.00,expense\n";
    const GOALS_A: &str = "id,child_id,description,target\ng1,keiko,Bike,100.00\n";
    const GOALS_THEIRS: &str = "id,child_id,description,target\ng1,keiko,Skateboard,60.00\n";

    /// Sets up: a bare "cloud" repo with a base commit, a local clone (the
    /// "work_dir"), the `lgs` remote wired via `ensure_lgs_remote`. Returns
    /// (bare, base_oid, work_dir tempdir, work repo path as String).
    fn setup_base() -> (tempfile::TempDir, Oid, tempfile::TempDir) {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = Repository::init_bare(bare_dir.path()).unwrap();
        let base_oid = commit_with_files(
            &bare,
            "base",
            &[],
            &[(TRANSACTIONS_FILE, TX_A), (GOALS_FILE, GOALS_A)],
            1_700_000_000,
        );
        bare.reference("refs/heads/main", base_oid, true, "init").unwrap();
        bare.set_head("refs/heads/main").unwrap();

        let work_dir = tempfile::tempdir().unwrap();
        let work_path = work_dir.path().join("work");
        let work_repo = clone_repo(bare_dir.path().to_str().unwrap(), &work_path).unwrap();
        ensure_lgs_remote(&work_repo, bare_dir.path().to_str().unwrap()).unwrap();

        (bare_dir, base_oid, work_dir)
    }

    #[test]
    fn up_to_date_when_no_auth_ref_has_landed() {
        let (bare_dir, _base_oid, work_dir) = setup_base();
        let outcome = ChildSyncEngine::cycle_against(
            &work_dir.path().join("work"),
            bare_dir.path().to_str().unwrap(),
        )
        .unwrap();
        assert!(matches!(outcome, CycleOutcome::UpToDate));
    }

    #[test]
    fn fast_forwards_when_the_peer_is_strictly_ahead() {
        let (bare_dir, base_oid, work_dir) = setup_base();
        let bare = Repository::open_bare(bare_dir.path()).unwrap();
        let base_commit = bare.find_commit(base_oid).unwrap();
        let ahead_oid = commit_with_files(
            &bare,
            "peer advanced",
            &[&base_commit],
            &[(TRANSACTIONS_FILE, TX_THEIRS), (GOALS_FILE, GOALS_A)],
            1_700_000_100,
        );
        bare.reference("refs/lgs-auth/heads/main", ahead_oid, true, "auth tip").unwrap();

        let outcome = ChildSyncEngine::cycle_against(
            &work_dir.path().join("work"),
            bare_dir.path().to_str().unwrap(),
        )
        .unwrap();
        match outcome {
            CycleOutcome::FastForward { to } => assert_eq!(to, ahead_oid.to_string()),
            other => panic!("expected FastForward, got {other:?}"),
        }
    }

    /// Regression for Critical-1. Simulates the ordinary, expected case:
    /// `auth` still reads as our OWN prior tip (lgs's unconditional mirror
    /// has not yet caught up to a local commit we just made). This must
    /// push and report `Ahead` — never run a merge, and never leave the
    /// local repo diverged-looking forever.
    #[test]
    fn pushes_and_reports_ahead_when_we_are_strictly_ahead_of_the_published_tip() {
        let (bare_dir, base_oid, work_dir) = setup_base();
        let work_path = work_dir.path().join("work");

        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_OURS).unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_050);
        gm.add_all(&work_path).unwrap();
        let ours_oid_str = gm.commit(&work_path, "ours edit").unwrap();

        // `auth` mirrors our OLD tip (base_oid) -- exactly what an
        // unconditional-mirror daemon would show right after we advanced
        // past it locally.
        let bare = Repository::open_bare(bare_dir.path()).unwrap();
        bare.reference("refs/lgs-auth/heads/main", base_oid, true, "auth mirrors our old tip")
            .unwrap();

        let outcome = ChildSyncEngine::cycle_against(&work_path, bare_dir.path().to_str().unwrap())
            .unwrap();
        assert!(matches!(outcome, CycleOutcome::Ahead), "expected Ahead, got {outcome:?}");

        // The push must have actually landed on the bare's real branch ref
        // (not the auth mirror) -- proving this is push-and-stop, not a
        // no-op that merely returns a status.
        assert_eq!(
            bare.find_reference("refs/heads/main").unwrap().target().unwrap().to_string(),
            ours_oid_str
        );

        // No merge commit was fabricated: our local HEAD is still exactly
        // the plain commit we made, not a two-parent merge.
        let work_repo = Repository::open(&work_path).unwrap();
        let head_commit = work_repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head_commit.id().to_string(), ours_oid_str);
        assert_eq!(head_commit.parent_count(), 1);
    }

    #[test]
    fn merges_on_divergence_and_flags_a_diverged_goals_csv() {
        let (bare_dir, base_oid, work_dir) = setup_base();
        let work_path = work_dir.path().join("work");

        // Diverge OURS: a local commit past base, with an edited goals.csv.
        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_OURS).unwrap();
        std::fs::write(work_path.join(GOALS_FILE), GOALS_A).unwrap(); // unchanged locally
        let gm = GitManager::with_clock(|| 1_700_000_050);
        gm.add_all(&work_path).unwrap();
        let ours_oid_str = gm.commit(&work_path, "ours edit").unwrap();

        // Diverge THEIRS directly on the bare's ODB (never touches the
        // working tree), with a DIFFERENT goals.csv.
        let bare = Repository::open_bare(bare_dir.path()).unwrap();
        let base_commit = bare.find_commit(base_oid).unwrap();
        let theirs_oid = commit_with_files(
            &bare,
            "their edit",
            &[&base_commit],
            &[(TRANSACTIONS_FILE, TX_THEIRS), (GOALS_FILE, GOALS_THEIRS)],
            1_700_000_100,
        );
        bare.reference("refs/lgs-auth/heads/main", theirs_oid, true, "auth tip").unwrap();

        let outcome =
            ChildSyncEngine::cycle_against(&work_path, bare_dir.path().to_str().unwrap()).unwrap();

        match outcome {
            CycleOutcome::Merged { rows, parents, goals_diverged, .. } => {
                assert_eq!(parents, (ours_oid_str, theirs_oid.to_string()));
                let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
                assert!(ids.contains(&"in-1-a"), "the unchanged allowance row must survive");
                assert!(ids.contains(&"ex-2-a"), "our add must survive");
                assert!(ids.contains(&"ex-3-a"), "their add must survive");
                assert!(goals_diverged, "goals.csv genuinely differs between the two tips");
            }
            other => panic!("expected Merged, got {other:?}"),
        }
    }

    #[test]
    fn merges_on_divergence_without_flagging_an_unchanged_goals_csv() {
        let (bare_dir, base_oid, work_dir) = setup_base();
        let work_path = work_dir.path().join("work");

        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_OURS).unwrap();
        // goals.csv left byte-identical to base on disk.
        let gm = GitManager::with_clock(|| 1_700_000_050);
        gm.add_all(&work_path).unwrap();
        gm.commit(&work_path, "ours edit").unwrap();

        let bare = Repository::open_bare(bare_dir.path()).unwrap();
        let base_commit = bare.find_commit(base_oid).unwrap();
        let theirs_oid = commit_with_files(
            &bare,
            "their edit",
            &[&base_commit],
            &[(TRANSACTIONS_FILE, TX_THEIRS), (GOALS_FILE, GOALS_A)],
            1_700_000_100,
        );
        bare.reference("refs/lgs-auth/heads/main", theirs_oid, true, "auth tip").unwrap();

        let outcome =
            ChildSyncEngine::cycle_against(&work_path, bare_dir.path().to_str().unwrap()).unwrap();

        match outcome {
            CycleOutcome::Merged { goals_diverged, .. } => {
                assert!(!goals_diverged, "goals.csv is identical on both sides");
            }
            other => panic!("expected Merged, got {other:?}"),
        }
    }

    #[test]
    fn provenance_is_resolved_per_side_and_is_never_a_shared_placeholder() {
        let (bare_dir, base_oid, work_dir) = setup_base();
        let work_path = work_dir.path().join("work");

        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_OURS).unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_050);
        gm.add_all(&work_path).unwrap();
        gm.commit(&work_path, "ours edit").unwrap();

        let bare = Repository::open_bare(bare_dir.path()).unwrap();
        let base_commit = bare.find_commit(base_oid).unwrap();
        let theirs_oid = commit_with_files(
            &bare,
            "their edit",
            &[&base_commit],
            &[(TRANSACTIONS_FILE, TX_THEIRS), (GOALS_FILE, GOALS_A)],
            1_700_000_999, // deliberately different from ours' 1_700_000_050
        );
        bare.reference("refs/lgs-auth/heads/main", theirs_oid, true, "auth tip").unwrap();

        // Reopen and resolve provenance directly, mirroring what
        // `cycle_against` does internally, to pin that the two sides differ
        // (a shared placeholder would trip `merge`'s `debug_assert!(ours !=
        // theirs)` the moment an add/add collision needed a tiebreak).
        let work_repo = Repository::open(&work_path).unwrap();
        fetch_lgs(&work_repo).unwrap(); // land theirs_oid locally so it can be resolved below
        let ours_oid = work_repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_prov = provenance(&work_repo, ours_oid).unwrap();
        let theirs_prov = provenance(&work_repo, theirs_oid).unwrap();
        assert_ne!(ours_prov, theirs_prov);
        assert_eq!(theirs_prov.committer_epoch, 1_700_000_999);
    }

    // --- recover_if_dirty and push_with_retry ------------------------------

    /// A standalone repo (no remote) with one committed `transactions.csv`.
    fn repo_with_commit() -> (Repository, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join(TRANSACTIONS_FILE), TX_A).unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_000);
        gm.add_all(dir.path()).unwrap();
        gm.commit(dir.path(), "init").unwrap();
        (repo, dir)
    }

    #[test]
    fn a_clean_tree_is_reported_clean_and_left_untouched() {
        let (repo, _dir) = repo_with_commit();
        assert_eq!(recover_if_dirty(&repo).unwrap(), Recovered::Clean);
        let text = std::fs::read_to_string(repo.workdir().unwrap().join(TRANSACTIONS_FILE)).unwrap();
        assert_eq!(text, TX_A, "a clean tree must not be touched");
    }

    /// Crash between writing merged CSVs and the merge commit. Re-running is
    /// safe precisely because the merge is deterministic — see
    /// `recover_if_dirty`'s doc comment for why discarding (never
    /// salvaging) the dirty content is the correct move here.
    #[test]
    fn a_dirty_tree_on_a_diverged_branch_is_discarded_and_re_merged() {
        let (repo, _dir) = repo_with_commit();
        std::fs::write(repo.workdir().unwrap().join(TRANSACTIONS_FILE), "garbage").unwrap();
        let recovered = recover_if_dirty(&repo).unwrap();
        assert_eq!(recovered, Recovered::DiscardedAndReMerged);
        let text = std::fs::read_to_string(repo.workdir().unwrap().join(TRANSACTIONS_FILE)).unwrap();
        assert_ne!(text, "garbage");
        assert_eq!(text, TX_A, "must be reset to exactly the last committed content");
    }

    /// An untracked file (never committed, so never part of HEAD) does not
    /// count as "dirty" here — `StatusOptions::include_untracked(false)`
    /// deliberately excludes it. Only a modification to tracked content
    /// (the crash-mid-write scenario this function exists for) triggers a
    /// reset.
    #[test]
    fn an_untracked_file_alone_is_not_treated_as_dirty() {
        let (repo, dir) = repo_with_commit();
        std::fs::write(dir.path().join("some_other_file.txt"), "not part of any commit").unwrap();
        assert_eq!(recover_if_dirty(&repo).unwrap(), Recovered::Clean);
        assert!(dir.path().join("some_other_file.txt").exists(), "untracked files are left alone");
    }

    #[test]
    fn push_retry_is_bounded() {
        let attempts = std::cell::Cell::new(0);
        let result = push_with_retry_inner(3, || {
            attempts.set(attempts.get() + 1);
            Err(anyhow!("moved"))
        });
        assert!(result.is_err());
        assert_eq!(attempts.get(), 3, "must not spin forever when the remote keeps moving");
    }

    #[test]
    fn push_retry_stops_at_the_first_success() {
        let attempts = std::cell::Cell::new(0);
        let result = push_with_retry_inner(3, || {
            attempts.set(attempts.get() + 1);
            if attempts.get() < 2 {
                Err(anyhow!("transient"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_ok());
        assert_eq!(attempts.get(), 2, "must not keep retrying once it has succeeded");
    }
}
