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
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use git2::{Oid, Repository};

use allowance_core::merge::{merge, Decision};
use allowance_core::row::{Provenance, Sided, TxRow};
use shared::ChildId;

use crate::backend::storage::csv::CsvConnection;
use crate::backend::storage::git::{ensure_lgs_remote, fetch_lgs, push_lgs, GitManager};
use crate::backend::sync::lgs_client::{LgsClient, ProjectReport, StatusReport};

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
    /// We were ahead of the peer's last published tip, but the project is
    /// archived (`ProjectReport::archived`) — lgs refuses every push
    /// against an archived project with a 403, and that refusal is
    /// permanent until a human runs `lgs unarchive`. Nothing was pushed:
    /// review finding Important-1 is that without this check, every single
    /// tick retried the push, got the 403, and tried again next tick —
    /// forever, for as long as any local edit existed, with no durable
    /// notice ever reaching the user. The caller surfaces one notice
    /// (`SyncMessage::ArchivedProjectSkipped`) instead of retrying.
    ArchivedSkipped,
}

// --- Task 20: "Check sync" ---------------------------------------------
//
// Health *reporting* (`fetch_status`/`cycle`) proves nothing about whether
// the loop actually works end to end — a daemon that answers `status --json`
// says nothing about whether THIS child's commits can actually get out and
// back. `check_sync` exercises the real path a user's edit takes: write,
// commit, push, fetch, read back what came out the other side, then clean
// up after itself. Every stage is named so a failure reads as "Push failed:
// <reason>", never "something went wrong."

/// One stage of [`ChildSyncEngine::check_sync`], in the order they run.
/// `Ord` follows this declaration order deliberately — [`check_sync_stages`]
/// and every caller that compares stages with `<=`/`>` (see this module's
/// tests) depend on "later in the enum" meaning "later in the run." Keep
/// this list and `check_sync_stages` in lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// `lgs status --json` answered at all.
    DaemonReachable,
    /// This child has a registered lgs project, and its clone URL (and this
    /// child's local repo) were both resolved.
    RemoteResolved,
    /// The sentinel scratch file was written into the working tree.
    WriteSentinel,
    /// The sentinel was staged and committed.
    Commit,
    /// The commit was pushed to the `lgs` remote.
    Push,
    /// The authoritative peer ref (`refs/remotes/lgs-auth/*`) was observed
    /// to advance to the pushed commit — bounded-polled, never assumed on
    /// the first fetch, because `lgs sync` only acknowledges the nudge (see
    /// [`LgsClient::sync`]).
    Fetch,
    /// The sentinel's content was read back from that authoritative ref's
    /// tree and matched exactly what was written.
    ReadBack,
    /// The sentinel was removed and that removal committed (and, once it
    /// was ever pushed, pushed too) — leaving the repo exactly as it was
    /// before this check ran.
    Cleanup,
}

/// [`Stage`]'s canonical run order, as a `Vec` so a caller (and this
/// module's own tests) can assert the full sequence without re-typing it.
pub fn check_sync_stages() -> Vec<Stage> {
    use Stage::*;
    vec![DaemonReachable, RemoteResolved, WriteSentinel, Commit, Push, Fetch, ReadBack, Cleanup]
}

/// What one stage of [`ChildSyncEngine::check_sync`] found. `detail` is
/// always plain, human-readable text — naming the stage is only half the
/// point; the other half is that a non-technical user (or this app's own
/// logs) can show `detail` verbatim and it says something useful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageResult {
    pub stage: Stage,
    pub ok: bool,
    pub detail: String,
}

impl StageResult {
    fn pass(stage: Stage, detail: impl Into<String>) -> Self {
        Self { stage, ok: true, detail: detail.into() }
    }
    fn fail(stage: Stage, detail: impl Into<String>) -> Self {
        Self { stage, ok: false, detail: detail.into() }
    }
}

/// The scratch file `check_sync` round-trips through the whole loop.
///
/// Deliberately NOT one of `paths::FILES_THIS_APP_OWNS` — seeded into an
/// unrelated commit via that list's callers would both corrupt this app's
/// real owned files' history and make the sentinel invisible to the staging
/// that check_sync itself does (which stages this name explicitly, never
/// `add_all`). The leading dot also keeps it out of anything that globs
/// only "real" data files.
pub const SYNC_CHECK_SENTINEL: &str = ".sync-check";

/// Total time [`check_sync_against`]'s `Fetch`/`ReadBack` stages will wait,
/// combined, for the authoritative peer ref to catch up to the commit this
/// same run just pushed. `lgs sync` is ACK-ONLY (see [`LgsClient::sync`]) —
/// this is what stands between "the daemon said ok" and "the daemon actually
/// finished," so it must be bounded generously rather than assumed instant,
/// but bounded all the same so a wedged daemon fails this stage rather than
/// hanging the button that triggered it.
const CHECK_SYNC_POLL_TIMEOUT: Duration = Duration::from_secs(30);
/// How long to sleep between poll attempts within that bound.
const CHECK_SYNC_POLL_INTERVAL: Duration = Duration::from_millis(500);

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

    /// Resolve this child's `ProjectReport` — clone URL AND `archived` —
    /// from an ALREADY-fetched `StatusReport` rather than shelling out
    /// again. See [`Self::fetch_status`] and [`Self::cycle_with_status`]'s
    /// doc comments — a caller syncing several children in one tick fetches
    /// status once and reuses it, rather than running `lgs status --json`
    /// once per child per tick. Project naming convention:
    /// `allowance-<child_id>`.
    ///
    /// `archived` travels with the clone URL, not as a separate lookup,
    /// because [`cycle_against`] needs both from the exact same
    /// `ProjectReport` — resolving them independently would risk reading
    /// `archived` from a stale/different fetch than the URL it's paired
    /// with.
    fn project_from_status<'a>(status: &'a StatusReport, child_id: &ChildId) -> Result<&'a ProjectReport> {
        let project_name = format!("allowance-{child_id}");
        status.project(&project_name).ok_or_else(|| {
            anyhow::anyhow!(
                "lgs has no project named '{project_name}' — has child '{child_id}' been \
                 registered with lgs yet?"
            )
        })
    }

    /// Run `lgs status --json` once. Exposed so a caller syncing several
    /// children in a single tick (`run_child_sync_cycles` in
    /// `sync_thread.rs`) can fetch it ONCE per tick and pass the same
    /// report to [`Self::cycle_with_status`] for every child, instead of
    /// this engine shelling out to `lgs` once per child per tick — the
    /// daemon has one answer to "what are my projects" regardless of which
    /// child is asking, and `lgs status --json` is not free (it is itself a
    /// process spawn with its own timeout — see `LgsClient::run`).
    pub fn fetch_status(&self) -> Result<StatusReport> {
        self.lgs.status().context("running `lgs status --json`")
    }

    /// Fetch, classify, and (when diverged) merge. Never touches the
    /// working tree. Resolves the remote via `lgs status --json`, then
    /// delegates to [`Self::cycle_against`] — the actual mechanics, which
    /// take the remote URL directly and so can be driven in tests against a
    /// local bare repo in a tempdir instead of the real lgs daemon (the
    /// whole point of the refspec-based design is that this code does not
    /// care that the remote is lgs).
    ///
    /// Calls `lgs status --json` itself, once, on every invocation. A
    /// caller cycling several children in one tick should prefer
    /// [`Self::cycle_with_status`] with a status fetched once for the whole
    /// tick instead of calling this once per child.
    pub fn cycle(&self, child_id: &ChildId) -> Result<CycleOutcome> {
        let work_dir = self.work_dir(child_id)?;
        let status = self.fetch_status()?;
        let project = Self::project_from_status(&status, child_id)?;
        Self::cycle_against(&work_dir, &project.clone_url, project.archived)
    }

    /// Same as [`Self::cycle`], but resolves the project from an
    /// already-fetched `StatusReport` (see [`Self::fetch_status`]) instead
    /// of shelling out to `lgs status --json` again.
    pub fn cycle_with_status(&self, child_id: &ChildId, status: &StatusReport) -> Result<CycleOutcome> {
        let work_dir = self.work_dir(child_id)?;
        let project = Self::project_from_status(status, child_id)?;
        Self::cycle_against(&work_dir, &project.clone_url, project.archived)
    }

    /// Exercise the whole lgs loop for `child_id`, on demand, and report
    /// each stage by name — see this module's "Task 20" section for why a
    /// health report alone (`fetch_status`/`cycle`) is not enough. `git` is
    /// taken as a parameter rather than held on `self`, matching every other
    /// repo-mutating call in this app (constructed at the call site — see
    /// `app_coordinator.rs`) rather than stored as engine state.
    pub fn check_sync(&self, git: &GitManager, child_id: &ChildId) -> Vec<StageResult> {
        let project_name = format!("allowance-{child_id}");
        match self.work_dir(child_id) {
            Ok(work_dir) => check_sync_against(
                &self.lgs,
                git,
                &work_dir,
                &project_name,
                CHECK_SYNC_POLL_TIMEOUT,
                CHECK_SYNC_POLL_INTERVAL,
            ),
            Err(e) => {
                // The daemon check is independent of this child's local
                // folder, so it is still worth running and reporting even
                // when the folder itself cannot be resolved.
                match self.lgs.status() {
                    Ok(_) => vec![
                        StageResult::pass(Stage::DaemonReachable, "lgs responded to `lgs status --json`"),
                        StageResult::fail(
                            Stage::RemoteResolved,
                            format!("resolving this child's local repo folder: {e}"),
                        ),
                    ],
                    Err(daemon_err) => vec![StageResult::fail(
                        Stage::DaemonReachable,
                        format!("running `lgs status --json`: {daemon_err}"),
                    )],
                }
            }
        }
    }

    fn cycle_against(work_dir: &Path, clone_url: &str, archived: bool) -> Result<CycleOutcome> {
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
            Cycle::Ahead if archived => {
                // Review Important-1: an archived project refuses every
                // push with a 403, permanently — there is nothing to
                // retry into existence. Skip the push entirely rather than
                // attempting (and re-attempting, forever, every tick) a
                // push that cannot ever succeed.
                Ok(CycleOutcome::ArchivedSkipped)
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

/// The actual mechanics behind [`ChildSyncEngine::check_sync`], taking the
/// remote URL directly (via `lgs.status()`, resolved by `project_name`) so
/// this can be driven in tests against a local bare repo and a fake `lgs`
/// script instead of the real daemon — same shape as [`cycle_against`]/
/// [`ChildSyncEngine::cycle`].
///
/// `poll_timeout`/`poll_interval` govern the `Fetch` stage's bounded wait
/// for the authoritative ref to catch up (see [`CHECK_SYNC_POLL_TIMEOUT`]'s
/// doc comment); exposed here so a test can drive the timeout path in
/// milliseconds rather than waiting out the real 30s bound.
///
/// Stops at the first failing stage — no stage after it is ever appended to
/// the returned `Vec`. On a failure, a best-effort attempt is made to undo
/// whatever this run had already done (see [`recover_after_failure`]); if
/// that cleanup attempt itself fails, that failure is folded into the
/// ALREADY-failing stage's own `detail` — never reported as a separate,
/// later `Stage` entry.
fn check_sync_against(
    lgs: &LgsClient,
    git: &GitManager,
    work_dir: &Path,
    project_name: &str,
    poll_timeout: Duration,
    poll_interval: Duration,
) -> Vec<StageResult> {
    let mut results = Vec::new();

    // --- Stage 1: DaemonReachable -------------------------------------
    let status = match lgs.status() {
        Ok(s) => s,
        Err(e) => {
            results.push(StageResult::fail(
                Stage::DaemonReachable,
                format!("running `lgs status --json`: {e}"),
            ));
            return results;
        }
    };
    results.push(StageResult::pass(Stage::DaemonReachable, "lgs responded to `lgs status --json`"));

    // --- Stage 2: RemoteResolved ---------------------------------------
    let clone_url = match status.project(project_name) {
        Some(p) => p.clone_url.clone(),
        None => {
            results.push(StageResult::fail(
                Stage::RemoteResolved,
                format!(
                    "lgs has no project named '{project_name}' — has this child been registered \
                     with lgs yet?"
                ),
            ));
            return results;
        }
    };
    let repo = match Repository::open(work_dir) {
        Ok(r) => r,
        Err(e) => {
            results.push(StageResult::fail(
                Stage::RemoteResolved,
                format!("opening this child's local git repo at {}: {e}", work_dir.display()),
            ));
            return results;
        }
    };
    if let Err(e) = ensure_lgs_remote(&repo, &clone_url) {
        results.push(StageResult::fail(
            Stage::RemoteResolved,
            format!("pointing the local 'lgs' remote at {clone_url}: {e}"),
        ));
        return results;
    }
    let branch = match current_branch(&repo) {
        Ok(b) => b,
        Err(e) => {
            results.push(StageResult::fail(
                Stage::RemoteResolved,
                format!("determining the checked-out branch: {e}"),
            ));
            return results;
        }
    };
    results.push(StageResult::pass(
        Stage::RemoteResolved,
        format!("resolved lgs project '{project_name}' -> {clone_url}"),
    ));

    // --- Stage 3: WriteSentinel -----------------------------------------
    let sentinel_path = work_dir.join(SYNC_CHECK_SENTINEL);
    let token = format!("check-sync {}\n", chrono::Utc::now().to_rfc3339());
    if let Err(e) = std::fs::write(&sentinel_path, &token) {
        let mut fail = StageResult::fail(
            Stage::WriteSentinel,
            format!("writing {SYNC_CHECK_SENTINEL} into the working tree: {e}"),
        );
        recover_after_failure(&repo, work_dir, git, &branch, false, &mut fail);
        results.push(fail);
        return results;
    }
    results.push(StageResult::pass(Stage::WriteSentinel, format!("wrote {SYNC_CHECK_SENTINEL}")));

    // --- Stage 4: Commit --------------------------------------------------
    if let Err(e) = git.add_file(work_dir, SYNC_CHECK_SENTINEL) {
        let mut fail = StageResult::fail(Stage::Commit, format!("staging {SYNC_CHECK_SENTINEL}: {e}"));
        recover_after_failure(&repo, work_dir, git, &branch, false, &mut fail);
        results.push(fail);
        return results;
    }
    let sentinel_oid_str = match git.commit(work_dir, "chore(sync-check): add sentinel") {
        Ok(oid) => oid,
        Err(e) => {
            let mut fail =
                StageResult::fail(Stage::Commit, format!("committing {SYNC_CHECK_SENTINEL}: {e}"));
            recover_after_failure(&repo, work_dir, git, &branch, false, &mut fail);
            results.push(fail);
            return results;
        }
    };
    let sentinel_oid = match Oid::from_str(&sentinel_oid_str) {
        Ok(oid) => oid,
        Err(e) => {
            let mut fail = StageResult::fail(
                Stage::Commit,
                format!("parsing the sentinel commit id '{sentinel_oid_str}': {e}"),
            );
            recover_after_failure(&repo, work_dir, git, &branch, false, &mut fail);
            results.push(fail);
            return results;
        }
    };
    results.push(StageResult::pass(
        Stage::Commit,
        format!("committed {SYNC_CHECK_SENTINEL} as {sentinel_oid_str}"),
    ));

    // --- Stage 5: Push -----------------------------------------------------
    if let Err(e) = push_with_retry(&repo, &branch, PUSH_RETRY_MAX) {
        let mut fail = StageResult::fail(
            Stage::Push,
            format!("pushing branch '{branch}' (retried up to {PUSH_RETRY_MAX} times): {e}"),
        );
        recover_after_failure(&repo, work_dir, git, &branch, false, &mut fail);
        results.push(fail);
        return results;
    }
    results.push(StageResult::pass(Stage::Push, format!("pushed branch '{branch}' with the sentinel commit")));

    // --- Stage 6: Fetch (bounded poll — lgs sync is ack-only) -------------
    let deadline = Instant::now() + poll_timeout;
    // The initial `None` is never read if the very first iteration's fetch
    // succeeds — that is the intended, common case, not a bug.
    #[allow(unused_assignments)]
    let mut last_fetch_err: Option<String> = None;
    let mut landed = false;
    loop {
        // Ack-only nudge — ignored on error: even if this particular call
        // fails, the daemon's own ambient tick might still land the
        // reconcile before the deadline, so it is still worth fetching.
        let _ = lgs.sync(project_name);
        match fetch_lgs(&repo) {
            Ok(()) => {
                last_fetch_err = None;
                if let Ok(reference) = repo.find_reference(LGS_AUTH_MAIN) {
                    if let Ok(commit) = reference.peel_to_commit() {
                        if commit.id() == sentinel_oid {
                            landed = true;
                        }
                    }
                }
            }
            Err(e) => last_fetch_err = Some(e.to_string()),
        }
        if landed || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(poll_interval);
    }
    if !landed {
        let detail = match last_fetch_err {
            Some(e) => format!(
                "fetching from lgs kept failing (last error: {e}) after waiting up to {poll_timeout:?} \
                 for the sentinel commit to become authoritative"
            ),
            None => format!(
                "waited {poll_timeout:?} but the authoritative peer ref never advanced to the pushed \
                 sentinel commit — lgs's reconcile may not have run yet"
            ),
        };
        let mut fail = StageResult::fail(Stage::Fetch, detail);
        recover_after_failure(&repo, work_dir, git, &branch, true, &mut fail);
        results.push(fail);
        return results;
    }
    results.push(StageResult::pass(
        Stage::Fetch,
        format!("the authoritative peer ref advanced to the pushed sentinel commit within {poll_timeout:?}"),
    ));

    // --- Stage 7: ReadBack -------------------------------------------------
    let auth_commit_id = repo
        .find_reference(LGS_AUTH_MAIN)
        .and_then(|r| r.peel_to_commit())
        .map(|c| c.id());
    let read_back = auth_commit_id
        .map_err(|e| format!("resolving the authoritative ref's commit: {e}"))
        .and_then(|oid| {
            read_blob_at(&repo, oid, SYNC_CHECK_SENTINEL)
                .map_err(|e| format!("reading {SYNC_CHECK_SENTINEL} back from commit {oid}: {e}"))
        });
    match read_back {
        Ok(Some(bytes)) if bytes == token.as_bytes() => {
            results.push(StageResult::pass(
                Stage::ReadBack,
                "read the sentinel back from the authoritative ref and its content matched exactly",
            ));
        }
        Ok(Some(_)) => {
            let mut fail = StageResult::fail(
                Stage::ReadBack,
                "the sentinel read back from the authoritative ref does not match what was written",
            );
            recover_after_failure(&repo, work_dir, git, &branch, true, &mut fail);
            results.push(fail);
            return results;
        }
        Ok(None) => {
            let mut fail = StageResult::fail(
                Stage::ReadBack,
                format!("{SYNC_CHECK_SENTINEL} is missing from the authoritative ref's tree"),
            );
            recover_after_failure(&repo, work_dir, git, &branch, true, &mut fail);
            results.push(fail);
            return results;
        }
        Err(e) => {
            let mut fail = StageResult::fail(Stage::ReadBack, e);
            recover_after_failure(&repo, work_dir, git, &branch, true, &mut fail);
            results.push(fail);
            return results;
        }
    }

    // --- Stage 8: Cleanup ---------------------------------------------------
    match do_cleanup(&repo, work_dir, git, &branch, true) {
        Ok(()) => {
            results.push(StageResult::pass(
                Stage::Cleanup,
                format!("removed {SYNC_CHECK_SENTINEL} and pushed its removal"),
            ));
        }
        Err(e) => {
            results.push(StageResult::fail(
                Stage::Cleanup,
                format!(
                    "{e} — the repo may be left with a leftover {SYNC_CHECK_SENTINEL} commit and \
                     needs manual attention"
                ),
            ));
        }
    }

    results
}

/// Best-effort: undo whatever local (and, once `push_succeeded`, remote)
/// state an earlier stage's failure left behind, folding the outcome into
/// the ALREADY-failing `failing` stage's own `detail` rather than ever
/// appending a separate, later `Stage` entry — a stage after the one that
/// failed must never be reported at all (see this module's tests).
/// `push_succeeded` says whether the sentinel commit reached the remote
/// before the failure: if it did, undoing it locally is not enough — the
/// removal must be pushed too, or the remote stays permanently polluted.
fn recover_after_failure(
    repo: &Repository,
    work_dir: &Path,
    git: &GitManager,
    branch: &str,
    push_succeeded: bool,
    failing: &mut StageResult,
) {
    if let Err(e) = do_cleanup(repo, work_dir, git, branch, push_succeeded) {
        failing.detail = format!(
            "{}; additionally, cleaning up the sentinel afterward failed too: {e} — the repo may be \
             left with a leftover {SYNC_CHECK_SENTINEL} commit and needs manual attention",
            failing.detail
        );
    }
}

/// Remove the sentinel file if present, unstage it if tracked, commit the
/// removal if that actually changed anything relative to `HEAD`, and (only
/// when `push`) push that removal commit. Shared by the terminal `Cleanup`
/// stage and by [`recover_after_failure`] — both need exactly this, differing
/// only in whether a push has ever actually happened yet to need undoing.
fn do_cleanup(repo: &Repository, work_dir: &Path, git: &GitManager, branch: &str, push: bool) -> Result<()> {
    let sentinel_path = work_dir.join(SYNC_CHECK_SENTINEL);
    if sentinel_path.exists() {
        std::fs::remove_file(&sentinel_path)
            .with_context(|| format!("removing {SYNC_CHECK_SENTINEL} from the working tree"))?;
    }
    git.remove_file(work_dir, SYNC_CHECK_SENTINEL)
        .with_context(|| format!("unstaging {SYNC_CHECK_SENTINEL}"))?;
    match git
        .commit_if_changed(work_dir, "chore(sync-check): remove sentinel")
        .with_context(|| format!("committing the removal of {SYNC_CHECK_SENTINEL}"))?
    {
        Some(_) if push => {
            push_with_retry(repo, branch, PUSH_RETRY_MAX)
                .with_context(|| format!("pushing the removal of {SYNC_CHECK_SENTINEL}"))?;
            Ok(())
        }
        Some(_) | None => Ok(()),
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

/// Name of the crash-recovery marker file. Lives under the repo's `.git`
/// directory (`repo.path()`), never in the working tree — it must never
/// show up as an untracked file in `git status`, and it must survive
/// exactly the crash window it exists to detect (a `git reset --hard` of
/// the *working tree* does not touch `.git` itself). Content is the two
/// parent oids `apply_merge` computed its merge against
/// (`"{ours}\n{theirs}\n"`), for diagnostics; only its presence or absence
/// is load-bearing.
///
/// # Why a marker, not mere dirtiness, is the trigger
///
/// An earlier version of [`recover_if_dirty`] discarded on ANY dirty
/// tracked-file state. That is wrong: this design's own AWS-coexistence
/// path deliberately writes `transactions.csv` WITHOUT committing
/// (`app_coordinator.rs`'s `apply_remote_entity` -> `upsert_transaction_from_sync`,
/// so that one MCP-server write does not fabricate an independent git
/// commit on every machine — see that function's doc comment). A dirty
/// working tree is therefore a NORMAL STEADY STATE for this app, not a
/// crash signature, and hard-resetting on sight would silently destroy
/// legitimate, not-yet-committed remote rows the AWS transport just wrote —
/// a data-loss bug strictly worse than the crash this function exists to
/// recover from. The marker makes the trigger explicit instead of inferred:
/// only "a merge was in progress and never finished" (this file present)
/// means "safe to discard," never "something happened to modify a file."
pub const MERGE_IN_PROGRESS_MARKER: &str = "lgs-merge-in-progress";

/// Write the crash-recovery marker recording `(ours, theirs)`. Callers
/// (`apply_merge` in `app_coordinator.rs`) must call this BEFORE the first
/// working-tree byte changes, so the marker's presence unambiguously means
/// "a merge write started and never reached its commit."
///
/// Not fatal if this fails (logged by the caller, not by this function):
/// losing the marker only means a subsequent crash's dirty tree will be
/// left alone rather than auto-recovered — the ordinary "next successful
/// apply_merge overwrites transactions.csv anyway" path still heals it, just
/// without the explicit fast-path.
pub fn write_merge_marker(repo: &Repository, ours: &str, theirs: &str) -> Result<()> {
    let path = repo.path().join(MERGE_IN_PROGRESS_MARKER);
    std::fs::write(&path, format!("{ours}\n{theirs}\n"))
        .with_context(|| format!("writing crash-recovery marker at {}", path.display()))
}

/// Remove the crash-recovery marker after a merge commit has been created
/// successfully (or after [`recover_if_dirty`] has already discarded the
/// dirty state it described). Never an error when no marker exists.
pub fn clear_merge_marker(repo: &Repository) -> Result<()> {
    let path = repo.path().join(MERGE_IN_PROGRESS_MARKER);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing crash-recovery marker at {}", path.display())),
    }
}

fn merge_marker_present(repo: &Repository) -> bool {
    repo.path().join(MERGE_IN_PROGRESS_MARKER).exists()
}

/// Outcome of [`recover_if_dirty`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovered {
    /// No recovery action was taken — either the tree was already clean, or
    /// it was dirty but with no [`MERGE_IN_PROGRESS_MARKER`] present, which
    /// means this is an ordinary dirty state (see that constant's doc
    /// comment for why that is normal here), not a crash to recover from.
    /// Left exactly as it was found.
    Clean,
    /// The marker was present, so the dirty tree it described was a crash
    /// mid-merge: hard-reset back to HEAD and the marker removed. The
    /// caller must treat this exactly like starting fresh: re-fetch,
    /// re-classify, and (if still diverged) re-merge and re-apply.
    DiscardedAndReMerged,
}

/// Recover a child repo from a crash between `apply_merge` writing
/// `transactions.csv` and it creating the follow-up merge commit
/// (`app_coordinator.rs`) — but ONLY when [`MERGE_IN_PROGRESS_MARKER`] says
/// that is actually what happened. Call this before writing anything, so a
/// leftover half-written file from a previous crash can never be mistaken
/// for legitimate content or committed alongside a new merge's tree.
///
/// # Why discarding — never salvaging — the dirty state is safe (once the
/// marker confirms it really is a crash)
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
///
/// This must never fire on dirtiness alone — see [`MERGE_IN_PROGRESS_MARKER`]'s
/// doc comment for the data-loss bug that caused.
pub fn recover_if_dirty(repo: &Repository) -> Result<Recovered> {
    if !merge_marker_present(repo) {
        return Ok(Recovered::Clean);
    }

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false);
    if repo
        .statuses(Some(&mut opts))
        .context("checking child repo status before applying a merge")?
        .is_empty()
    {
        // The marker survived (e.g. a crash landed after the commit but
        // before the marker was cleared) but there is nothing left to
        // discard — just clean up the now-stale marker.
        clear_merge_marker(repo)?;
        return Ok(Recovered::Clean);
    }

    let head = repo
        .head()
        .context("resolving HEAD to recover a dirty working tree")?
        .peel_to_commit()
        .context("peeling HEAD to a commit to recover a dirty working tree")?;
    repo.reset(head.as_object(), git2::ResetType::Hard, None)
        .context("hard-resetting a dirty working tree back to HEAD")?;
    clear_merge_marker(repo)?;
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
///
/// `pub(crate)`: `backend::sync::migration_lgs`'s adopt-branch merge reuses
/// this rather than re-deriving a commit's `Provenance` a second time.
pub(crate) fn provenance(repo: &Repository, oid: Oid) -> Result<Provenance> {
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
            false,
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
            false,
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

        let outcome = ChildSyncEngine::cycle_against(&work_path, bare_dir.path().to_str().unwrap(), false)
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

    /// Review Important-1: an archived project must never have its push
    /// attempted — not once, and not retried. Same "ahead" setup as
    /// `pushes_and_reports_ahead_when_we_are_strictly_ahead_of_the_published_tip`,
    /// but `archived: true` this time. Asserts BOTH that the outcome is
    /// `ArchivedSkipped` AND that the bare's real branch ref never moved —
    /// proving this is skip-before-attempting, not attempt-then-swallow-the-403.
    #[test]
    fn an_archived_project_that_is_ahead_skips_the_push_instead_of_retrying_forever() {
        let (bare_dir, base_oid, work_dir) = setup_base();
        let work_path = work_dir.path().join("work");

        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_OURS).unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_050);
        gm.add_all(&work_path).unwrap();
        gm.commit(&work_path, "ours edit").unwrap();

        let bare = Repository::open_bare(bare_dir.path()).unwrap();
        bare.reference("refs/lgs-auth/heads/main", base_oid, true, "auth mirrors our old tip")
            .unwrap();

        let outcome = ChildSyncEngine::cycle_against(&work_path, bare_dir.path().to_str().unwrap(), true)
            .unwrap();
        assert!(
            matches!(outcome, CycleOutcome::ArchivedSkipped),
            "expected ArchivedSkipped, got {outcome:?}"
        );

        // The real branch ref must still be exactly where it started —
        // nothing was pushed, not even an attempt that a fake 403 happened
        // to swallow.
        assert_eq!(
            bare.find_reference("refs/heads/main").unwrap().target().unwrap(),
            base_oid,
            "an archived project's push must never be attempted, so the remote ref must not move"
        );
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
            ChildSyncEngine::cycle_against(&work_path, bare_dir.path().to_str().unwrap(), false).unwrap();

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
            ChildSyncEngine::cycle_against(&work_path, bare_dir.path().to_str().unwrap(), false).unwrap();

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

    /// Task 17 Important-3 regression: a dirty working tree is a NORMAL
    /// STEADY STATE for this app (the AWS transport writes
    /// `transactions.csv` without committing — see
    /// `MERGE_IN_PROGRESS_MARKER`'s doc comment), not necessarily a crash.
    /// Without the marker, `recover_if_dirty` must leave it alone — an
    /// earlier version that discarded on dirtiness alone would have hard-
    /// reset away legitimate, not-yet-committed remote rows here.
    #[test]
    fn a_dirty_tree_with_no_marker_is_left_untouched() {
        let (repo, _dir) = repo_with_commit();
        std::fs::write(repo.workdir().unwrap().join(TRANSACTIONS_FILE), "uncommitted AWS row").unwrap();
        assert!(!merge_marker_present(&repo), "precondition: no marker written");
        assert_eq!(recover_if_dirty(&repo).unwrap(), Recovered::Clean);
        let text = std::fs::read_to_string(repo.workdir().unwrap().join(TRANSACTIONS_FILE)).unwrap();
        assert_eq!(
            text, "uncommitted AWS row",
            "an ordinary dirty tree with no crash marker must never be touched"
        );
    }

    /// Crash between writing merged CSVs and the merge commit, WITH the
    /// crash-recovery marker present (as `apply_merge` writes it before
    /// touching the working tree). Re-running is safe precisely because the
    /// merge is deterministic — see `recover_if_dirty`'s doc comment for why
    /// discarding (never salvaging) the dirty content is the correct move
    /// once the marker confirms this really is a crash.
    #[test]
    fn a_dirty_tree_with_the_marker_present_is_discarded_and_the_marker_cleared() {
        let (repo, _dir) = repo_with_commit();
        write_merge_marker(&repo, "ours-oid", "theirs-oid").unwrap();
        std::fs::write(repo.workdir().unwrap().join(TRANSACTIONS_FILE), "garbage").unwrap();

        let recovered = recover_if_dirty(&repo).unwrap();
        assert_eq!(recovered, Recovered::DiscardedAndReMerged);

        let text = std::fs::read_to_string(repo.workdir().unwrap().join(TRANSACTIONS_FILE)).unwrap();
        assert_ne!(text, "garbage");
        assert_eq!(text, TX_A, "must be reset to exactly the last committed content");
        assert!(!merge_marker_present(&repo), "the marker must be cleared once recovery ran");
    }

    /// The marker can outlive its crash window (e.g. the crash landed right
    /// after the commit but before `clear_merge_marker` ran) with nothing
    /// left dirty to discard. `recover_if_dirty` must still clean up the
    /// stale marker rather than leaving it to wrongly trigger a discard on
    /// some LATER, unrelated dirty state.
    #[test]
    fn a_stale_marker_with_nothing_dirty_is_cleaned_up_without_a_discard() {
        let (repo, _dir) = repo_with_commit();
        write_merge_marker(&repo, "ours-oid", "theirs-oid").unwrap();

        assert_eq!(recover_if_dirty(&repo).unwrap(), Recovered::Clean);
        assert!(!merge_marker_present(&repo), "a stale marker must still be cleared");
        let text = std::fs::read_to_string(repo.workdir().unwrap().join(TRANSACTIONS_FILE)).unwrap();
        assert_eq!(text, TX_A, "nothing was dirty, so nothing should have changed");
    }

    /// An untracked file (never committed, so never part of HEAD) does not
    /// count as "dirty" here even with the marker present —
    /// `StatusOptions::include_untracked(false)` deliberately excludes it.
    #[test]
    fn an_untracked_file_alone_is_not_treated_as_dirty() {
        let (repo, dir) = repo_with_commit();
        write_merge_marker(&repo, "ours-oid", "theirs-oid").unwrap();
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

    // --- Task 20: check_sync -----------------------------------------------

    /// Pure sequencing test harness for the "every stage is named" and "stop
    /// at the first failure" contracts, independent of any real repo or
    /// `lgs` process: for each stage in order, calls `should_pass(stage)`
    /// and stops at the first `false`. The real mechanics
    /// (`check_sync_against`) have their own tests below, against local
    /// bare repos and a fake `lgs` script.
    fn run_check_with(should_pass: impl Fn(Stage) -> bool) -> Vec<StageResult> {
        let mut results = Vec::new();
        for stage in check_sync_stages() {
            if should_pass(stage) {
                results.push(StageResult::pass(stage, "ok"));
            } else {
                results.push(StageResult::fail(stage, "injected failure"));
                break;
            }
        }
        results
    }

    #[test]
    fn every_stage_is_named_so_a_failure_says_which_one_broke() {
        let stages = check_sync_stages();
        assert_eq!(
            stages,
            vec![
                Stage::DaemonReachable,
                Stage::RemoteResolved,
                Stage::WriteSentinel,
                Stage::Commit,
                Stage::Push,
                Stage::Fetch,
                Stage::ReadBack,
                Stage::Cleanup,
            ]
        );
    }

    #[test]
    fn a_failure_stops_at_the_failing_stage_and_reports_it() {
        let results = run_check_with(|stage| stage != Stage::Push);
        let failed: Vec<&StageResult> = results.iter().filter(|r| !r.ok).collect();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].stage, Stage::Push);
        assert!(results.iter().all(|r| r.stage <= Stage::Push), "must not continue past a failure");
    }

    /// A bare "cloud" repo with a base commit, plus a local clone (the
    /// child's `work_dir`) — no `lgs` remote wired yet; `check_sync_against`
    /// wires it itself from the fake script's `status --json` clone_url,
    /// exactly like production.
    fn setup_check_sync_repo() -> (tempfile::TempDir, PathBuf, tempfile::TempDir, PathBuf) {
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = Repository::init_bare(bare_dir.path()).unwrap();
        let base_oid =
            commit_with_files(&bare, "base", &[], &[(TRANSACTIONS_FILE, TX_A)], 1_700_000_000);
        bare.reference("refs/heads/main", base_oid, true, "init").unwrap();
        bare.set_head("refs/heads/main").unwrap();
        let bare_path = bare_dir.path().to_path_buf();

        let work_dir = tempfile::tempdir().unwrap();
        let work_path = work_dir.path().join("work");
        clone_repo(bare_path.to_str().unwrap(), &work_path).unwrap();

        (bare_dir, bare_path, work_dir, work_path)
    }

    /// Writes a fake `lgs` executable (a shell script, reached only by an
    /// explicit tempdir path — never on `PATH`) that answers `status --json`
    /// with one project pointing at `bare_path`, and answers `sync <name>`
    /// one of two ways:
    ///
    /// - `reconcile_on_sync = true`: mirrors `refs/lgs-auth/heads/main` to
    ///   whatever `refs/heads/main` currently is on the bare — a faithful
    ///   stand-in for "the daemon's reconcile landed," used by the
    ///   happy-path test.
    /// - `reconcile_on_sync = false`: advances the bare's `refs/heads/main`
    ///   to an unrelated new commit EVERY time it's called, and never
    ///   touches `refs/lgs-auth/heads/main` at all — simulates a peer
    ///   advancing the branch while this run's reconcile never lands,
    ///   which both makes `Fetch` time out AND makes a later cleanup push
    ///   a genuine non-fast-forward. Used by the "cleanup itself fails"
    ///   test.
    fn write_fake_lgs(script_dir: &Path, project_name: &str, bare_path: &Path, reconcile_on_sync: bool) -> PathBuf {
        let script_path = script_dir.join("lgs");
        let bare = bare_path.display();
        let status_json = format!(
            "{{\"daemon\":{{\"state\":\"ok\"}},\"cloud_root\":\"/tmp\",\"cloud_root_exists\":true,\"projects\":[{{\"name\":\"{project_name}\",\"clone_url\":\"{bare}\",\"working_repo_path\":\"/tmp\",\"archived\":false}}]}}"
        );
        let sync_body = if reconcile_on_sync {
            format!(
                "    HEAD=$(git --git-dir=\"{bare}\" rev-parse refs/heads/main 2>/dev/null) || exit 0\n    git --git-dir=\"{bare}\" update-ref refs/lgs-auth/heads/main \"$HEAD\"\n"
            )
        } else {
            format!(
                "    export GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=test@example.com GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=test@example.com\n    TREE=$(git --git-dir=\"{bare}\" rev-parse 'refs/heads/main^{{tree}}') || exit 0\n    PARENT=$(git --git-dir=\"{bare}\" rev-parse refs/heads/main) || exit 0\n    NEW=$(printf 'advance after %s' \"$PARENT\" | git --git-dir=\"{bare}\" commit-tree \"$TREE\" -p \"$PARENT\") || exit 0\n    git --git-dir=\"{bare}\" update-ref refs/heads/main \"$NEW\"\n"
            )
        };
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  status)\n    cat <<'JSON'\n{status_json}\nJSON\n    ;;\n  sync)\n{sync_body}    ;;\n  *)\n    exit 1\n    ;;\nesac\n"
        );
        std::fs::write(&script_path, script).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }
        script_path
    }

    #[test]
    fn check_sync_end_to_end_succeeds_and_leaves_the_repo_clean() {
        let (_bare_dir, bare_path, _work_dir, work_path) = setup_check_sync_repo();
        let script_dir = tempfile::tempdir().unwrap();
        let script_path = write_fake_lgs(script_dir.path(), "allowance-keiko", &bare_path, true);

        let lgs = LgsClient::new(script_path);
        let git = GitManager::new();

        let results = check_sync_against(
            &lgs,
            &git,
            &work_path,
            "allowance-keiko",
            Duration::from_secs(5),
            Duration::from_millis(50),
        );

        assert_eq!(
            results.len(),
            check_sync_stages().len(),
            "every stage must have run and passed: {results:?}"
        );
        for r in &results {
            assert!(r.ok, "stage {:?} unexpectedly failed: {}", r.stage, r.detail);
        }

        // Locally: no sentinel on disk, and HEAD's tree has no sentinel entry.
        assert!(!work_path.join(SYNC_CHECK_SENTINEL).exists());
        let repo = Repository::open(&work_path).unwrap();
        let head_tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
        assert!(head_tree.get_name(SYNC_CHECK_SENTINEL).is_none(), "HEAD must have no sentinel");

        // And the remote: cleanup's removal was actually pushed, not just
        // committed locally.
        let bare = Repository::open_bare(&bare_path).unwrap();
        let bare_tip_oid = bare.find_reference("refs/heads/main").unwrap().target().unwrap();
        let bare_tip = bare.find_commit(bare_tip_oid).unwrap();
        assert!(
            bare_tip.tree().unwrap().get_name(SYNC_CHECK_SENTINEL).is_none(),
            "the remote must be clean too — cleanup's removal must have been pushed"
        );
    }

    #[test]
    fn check_sync_stops_at_push_failure_and_cleans_up_locally() {
        let work_dir = tempfile::tempdir().unwrap();
        let work_path = work_dir.path().join("work");
        std::fs::create_dir_all(&work_path).unwrap();
        Repository::init(&work_path).unwrap();
        let gm = GitManager::with_clock(|| 1_700_000_000);
        std::fs::write(work_path.join(TRANSACTIONS_FILE), TX_A).unwrap();
        gm.add_all(&work_path).unwrap();
        gm.commit(&work_path, "init").unwrap();

        let script_dir = tempfile::tempdir().unwrap();
        // A clone_url that isn't a git repo at all — push fails outright, no
        // real remote plumbing needed for this test.
        let bogus_remote = script_dir.path().join("does-not-exist.git");
        let script_path = write_fake_lgs(script_dir.path(), "allowance-keiko", &bogus_remote, true);

        let lgs = LgsClient::new(script_path);
        let git = GitManager::new();

        let results = check_sync_against(
            &lgs,
            &git,
            &work_path,
            "allowance-keiko",
            Duration::from_secs(2),
            Duration::from_millis(50),
        );

        assert!(
            results.iter().all(|r| r.stage <= Stage::Push),
            "must not report any stage after Push: {results:?}"
        );
        let last = results.last().expect("at least one stage must have run");
        assert_eq!(last.stage, Stage::Push);
        assert!(!last.ok, "push against a nonexistent remote must fail");
        assert!(
            !last.detail.contains("cleaning up the sentinel afterward failed"),
            "local-only cleanup must succeed here: {}",
            last.detail
        );

        assert!(!work_path.join(SYNC_CHECK_SENTINEL).exists(), "sentinel must be removed from disk");
        let repo = Repository::open(&work_path).unwrap();
        let head_tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
        assert!(head_tree.get_name(SYNC_CHECK_SENTINEL).is_none(), "HEAD must have no sentinel");
    }

    /// Review-anticipating regression: a failure that happens AFTER the
    /// sentinel was already pushed (here, `Fetch` timing out) must still
    /// attempt to clean up — including pushing the removal, since the
    /// remote is already polluted at that point — and if THAT cleanup
    /// attempt also fails, it must say so rather than reporting the
    /// failing stage as if cleanup were silent. The fake script here
    /// advances the bare's `refs/heads/main` independently on every `sync`
    /// call (never landing `refs/lgs-auth/*`), which both starves `Fetch`
    /// and makes the eventual cleanup push a genuine non-fast-forward.
    #[test]
    fn check_sync_reports_when_cleanup_itself_fails_after_a_later_failure() {
        let (_bare_dir, bare_path, _work_dir, work_path) = setup_check_sync_repo();
        let script_dir = tempfile::tempdir().unwrap();
        let script_path = write_fake_lgs(script_dir.path(), "allowance-keiko", &bare_path, false);

        let lgs = LgsClient::new(script_path);
        let git = GitManager::new();

        let results = check_sync_against(
            &lgs,
            &git,
            &work_path,
            "allowance-keiko",
            Duration::from_millis(400),
            Duration::from_millis(50),
        );

        assert!(
            results.iter().all(|r| r.stage <= Stage::Fetch),
            "must not report any stage after Fetch: {results:?}"
        );
        let last = results.last().expect("at least one stage must have run");
        assert_eq!(last.stage, Stage::Fetch);
        assert!(!last.ok, "the authoritative ref never advances in this scenario, so Fetch must fail");
        assert!(
            last.detail.contains("cleaning up the sentinel afterward failed too"),
            "the recovery push must fail too (non-fast-forward) and be reported: {}",
            last.detail
        );

        // The local half of cleanup must still have succeeded even though
        // the push of the removal failed.
        assert!(!work_path.join(SYNC_CHECK_SENTINEL).exists());
        let repo = Repository::open(&work_path).unwrap();
        let head_tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
        assert!(head_tree.get_name(SYNC_CHECK_SENTINEL).is_none());
    }
}
