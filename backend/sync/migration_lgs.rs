//! # One-shot migration into an lgs-backed repo
//!
//! Moves one child's data out of the legacy iCloud-backed folder and into a
//! `.git` repository under `~/Library/Application Support/Allowance
//! Tracker/children/<id>/`, tracked by `lgs` instead of iCloud. This is the
//! migration that touches a real family's real financial history, so the
//! design is deliberately conservative:
//!
//! - **Copy, never move.** The old folder (and its `.git`, if it has one) is
//!   never deleted, written to, or even opened for writing. Every step here
//!   only ever *reads* `old_dir`.
//! - **The registry is repointed last.** [`Step::RepointRegistry`] is always
//!   the final step in the plan, and [`run_lgs_migration`] executes steps in
//!   order, stopping at the first failure. Nothing after a failed step runs,
//!   which means `children.yaml` still names the old folder and the app keeps
//!   working exactly as it did before the migration was attempted.
//! - **Adopt before init.** Two machines can both decide to migrate the same
//!   child on different days. If this machine's `lgs status` already shows
//!   the project as `adoptable` (created and pushed by the other machine),
//!   this module runs `lgs restore` and builds on top of that history
//!   instead of `git init`-ing an unrelated root.
//! - **Never overwrite an adopted (or resumed) history with a stale local
//!   copy.** [`run_copy_data_files`] and [`run_canonicalize`] never blindly
//!   replace whatever already sits at `new_dir` — that is exactly how an
//!   earlier version of this module lost data (see the task-18 review's
//!   Critical-1: cloning a peer's history and then overwriting it with this
//!   machine's stale iCloud copy silently deletes every row the peer wrote
//!   after it migrated, because the deletion has real ancestry and
//!   fast-forwards cleanly). `transactions.csv` is instead reconciled with
//!   `allowance_core::merge::merge(None, ours, theirs)` — the same
//!   property-tested three-way merge the ordinary sync path uses, with an
//!   empty base. An empty base means no common ancestor, which means
//!   nothing can be shown to have been *deleted* on either side, so the
//!   union keeps every row from both. `child.yaml`, `allowance_config.yaml`,
//!   and `goals.csv` are simpler (`allowance_core::merge` does not model
//!   them): whatever already exists at `new_dir` is kept outright, and a
//!   [`StartupNotice`] names any file where the two copies actually differ,
//!   rather than silently preferring one.
//! - **Re-runnable after a partial failure.** [`plan_lgs_migration`] checks
//!   `status.projects`, not only `status.adoptable`: a project already
//!   registered with lgs (because an earlier attempt got as far as
//!   `Step::LgsAdd` before failing later) is not `lgs add`ed a second time —
//!   that call fails permanently, with no path back, for an already-named
//!   project. Because Copy/Canonicalize/Commit are all safe to re-run (the
//!   union above is idempotent, and `commit_if_changed` is a no-op when
//!   nothing changed), a retry after a failure at *any* step converges to
//!   the same result rather than getting stuck.
//!
//! Mirrors the plan/report/run split in
//! `backend::storage::csv::migration::{plan_migration, MigrationReport}`:
//! [`plan_lgs_migration`] only reads and decides; [`run_lgs_migration`] is
//! the only thing that writes, executes strictly in
//! [`LgsMigrationPlan::steps`] order, and produces an [`LgsMigrationReport`]
//! that always says whether it failed and where.
//!
//! `plan_lgs_migration` takes one [`RegistryEntry`] because this module
//! plans and runs one migration at a time — the one real install this ships
//! for has exactly one child. A future multi-child rollout would call this
//! once per child.

use crate::backend::storage::csv::{ChildRegistry, RegistryEntry};
use crate::backend::storage::git::{clone_repo, ensure_lgs_remote, fetch_lgs, push_lgs, GitManager};
use crate::backend::sync::child_sync::{current_branch, provenance};
use crate::backend::sync::lgs_client::{LgsClient, StatusReport};
use crate::backend::sync::paths::{is_cloud_synced, Reason, SyncPaths, FILES_THIS_APP_OWNS};
use crate::backend::{NoticeSeverity, StartupNotice};
use allowance_core::merge::{merge, Decision};
use allowance_core::row::{Provenance, Sided};
use anyhow::{Context, Result};
use shared::ChildId;
use std::fs;
use std::path::{Path, PathBuf};

const TRANSACTIONS_FILE: &str = "transactions.csv";

fn project_name(id: &ChildId) -> String {
    format!("{PROJECT_PREFIX}{}", id.as_str())
}

/// One step of an [`LgsMigrationPlan`]. Deliberately carries no per-step
/// payload beyond the two variants that need to name which project to
/// restore or create — every other step's inputs live on the
/// [`LgsMigrationPlan`] itself, not duplicated onto each `Step`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Adopt an already-existing lgs project via `lgs restore` rather than
    /// starting an unrelated history.
    RestoreExisting { name: String },
    /// Nothing adoptable exists yet: `git init` a fresh root (idempotent —
    /// also the branch taken to *resume* a fresh migration that already got
    /// this far in an earlier, failed attempt).
    InitAndPush { name: String },
    /// Bring `child.yaml`, `allowance_config.yaml`, `goals.csv` into
    /// `new_dir`. Never overwrites a file already there — see the module
    /// doc comment. Never copies `.git`.
    CopyDataFiles,
    /// Reconcile `old_dir/transactions.csv` with whatever is already at
    /// `new_dir/transactions.csv` (if anything) through
    /// `allowance_core::merge::merge` and write the result back out
    /// through `render_transactions` — canonical row order, two-decimal
    /// money, and no dropped rows.
    Canonicalize,
    /// Commit the copied + canonicalized files. A no-op (not an error) if
    /// staging produced no change.
    Commit,
    /// Register the fresh repo with lgs. Present only when the project is
    /// neither adoptable nor already registered — an adopted project is
    /// already registered by `lgs restore`, and a resumed one was already
    /// registered by an earlier attempt's successful `lgs add`.
    LgsAdd,
    /// Point the repo's `lgs` remote at the right URL: read from the
    /// restored repo's `origin` when adopting, or resolved via a fresh `lgs
    /// status` lookup otherwise (self-healing a stale URL either way — see
    /// `ensure_lgs_remote`'s own doc comment).
    EnsureLgsRemote,
    /// Push the current branch to the `lgs` remote.
    Push,
    /// Repoint `children.yaml` at the new folder. Always last: see the
    /// module doc comment.
    RepointRegistry,
}

/// The decided, not-yet-executed shape of one child's migration.
#[derive(Debug)]
pub struct LgsMigrationPlan {
    pub child_id: ChildId,
    /// The pre-migration folder. Read-only for the entire lifetime of this
    /// plan and its execution — never written to, never deleted.
    pub old_dir: PathBuf,
    /// `~/Library/Application Support/Allowance Tracker/children/<id>`.
    pub new_dir: PathBuf,
    pub project_name: String,
    /// Directory holding `children.yaml` (`SyncPaths::data_dir`).
    pub data_dir: PathBuf,
    pub lgs_binary: PathBuf,
    pub steps: Vec<Step>,
    /// `Some` when [`is_cloud_synced`] rejected `new_dir` (or `new_dir`
    /// could not even be canonicalized enough to check). When set, `steps`
    /// is empty and [`run_lgs_migration`] does nothing but report the
    /// refusal.
    pub blocked: Option<Reason>,
}

/// Outcome of [`run_lgs_migration`].
#[derive(Debug, Default)]
pub struct LgsMigrationReport {
    pub completed_steps: Vec<Step>,
    pub failed_step: Option<Step>,
    pub error: Option<String>,
    /// How many transaction rows needed legacy f64-precision rounding
    /// (`Money::parse_rounding`) while being canonicalized, summed across
    /// whatever was already at `new_dir` and `old_dir`. The rewrite is
    /// correct, not a value change — but the user's financial data is
    /// affected and must be told, not left to notice a diff.
    pub legacy_precision_rows_rounded: usize,
    /// Non-trivial choices `allowance_core::merge::merge` made while
    /// reconciling `transactions.csv` — see `Decision`'s own doc comment.
    /// Empty on the ordinary first-time path, where there is nothing at
    /// `new_dir` yet to reconcile against.
    pub merge_decisions: Vec<Decision>,
    /// Names of owned files (other than `transactions.csv`, which is always
    /// merged, not compared) where what was already at `new_dir` differed
    /// from `old_dir`'s copy. The `new_dir` version was kept in every case;
    /// this is not a failure, only something to look at by hand.
    pub discrepancies: Vec<String>,
    /// User-visible outcomes. On success, always names the old folder's
    /// path so the user knows it is being kept as a backup, not lost.
    pub notices: Vec<StartupNotice>,
}

impl LgsMigrationReport {
    pub fn failed(&self) -> bool {
        self.failed_step.is_some() || self.error.is_some()
    }
}

/// Decide what a migration for `entry` would do. Reads only — `status` is
/// the caller's already-fetched `lgs status --json`, and this function
/// performs no I/O of its own beyond the cheap, best-effort filesystem
/// probes described below.
pub fn plan_lgs_migration(entry: &RegistryEntry, paths: &SyncPaths, status: &StatusReport) -> LgsMigrationPlan {
    let child_id = entry.id.clone();
    let name = project_name(&child_id);
    let old_dir = entry.path.clone();
    let new_dir = paths.children_root.join(child_id.as_str());

    let documents_is_symlink = fs::symlink_metadata(paths.home.join("Documents"))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    // `is_cloud_synced`'s own doc comment states plainly that resolving
    // symlinks in the candidate is the CALLER's job, not something the pure
    // guard can do. `new_dir` does not exist yet at plan time (it is
    // created during migration), so a plain `fs::canonicalize` on it would
    // almost always fail with `NotFound` — `canonicalize_as_far_as_possible`
    // resolves whatever ancestor of it actually exists (e.g. a symlinked
    // `children_root`) and re-appends the not-yet-existing tail. A
    // candidate that cannot be verified at all fails closed, exactly like
    // `is_cloud_synced`'s own `NotNormalisable` case.
    let blocked = match canonicalize_as_far_as_possible(&new_dir) {
        Some(canonical) => is_cloud_synced(&canonical, paths, documents_is_symlink),
        None => Some(Reason::NotNormalisable),
    };

    if let Some(reason) = blocked {
        return LgsMigrationPlan {
            child_id,
            old_dir,
            new_dir,
            project_name: name,
            data_dir: paths.data_dir.clone(),
            lgs_binary: paths.lgs_binary.clone(),
            steps: Vec::new(),
            blocked: Some(reason),
        };
    }

    let adopt = status.adoptable.iter().any(|a| a.name == name);
    // Critical-2 fix: a project already registered with lgs (typically
    // because an earlier attempt at THIS SAME migration got as far as
    // `Step::LgsAdd` before failing at a later step) must never be
    // `lgs add`ed again — that call fails permanently for an
    // already-existing name, stranding the user with no way to finish.
    let already_registered = status.project(&name).is_some();

    let mut steps = Vec::new();
    if adopt {
        steps.push(Step::RestoreExisting { name: name.clone() });
    } else {
        steps.push(Step::InitAndPush { name: name.clone() });
    }
    steps.push(Step::CopyDataFiles);
    steps.push(Step::Canonicalize);
    steps.push(Step::Commit);
    if !adopt && !already_registered {
        steps.push(Step::LgsAdd);
    }
    steps.push(Step::EnsureLgsRemote);
    steps.push(Step::Push);
    steps.push(Step::RepointRegistry);

    LgsMigrationPlan {
        child_id,
        old_dir,
        new_dir,
        project_name: name,
        data_dir: paths.data_dir.clone(),
        lgs_binary: paths.lgs_binary.clone(),
        steps,
        blocked: None,
    }
}

/// Resolve symlinks in `candidate` as far as the filesystem allows: walk up
/// to the nearest ancestor that actually exists, canonicalize THAT, then
/// re-append the (necessarily still-nonexistent) tail untouched.
///
/// Returns `None` only when nothing on the path exists at all, which does
/// not happen on a real filesystem (`/` always exists) but is handled
/// explicitly rather than assumed, matching `is_cloud_synced`'s own
/// fail-closed posture for a path it cannot reason about.
fn canonicalize_as_far_as_possible(candidate: &Path) -> Option<PathBuf> {
    let mut existing = candidate;
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    while !existing.exists() {
        tail.push(existing.file_name()?);
        existing = existing.parent()?;
    }
    let mut resolved = fs::canonicalize(existing).ok()?;
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    Some(resolved)
}

/// Execute `plan`'s steps strictly in order, stopping at the first failure.
///
/// Every step before [`Step::RepointRegistry`] only touches `new_dir` (never
/// deleting anything already there) or reads `old_dir`. `children.yaml` is
/// never opened for writing until every earlier step has already succeeded
/// — so a failure anywhere leaves both the registry and the old folder
/// exactly as they were.
pub fn run_lgs_migration(plan: LgsMigrationPlan) -> LgsMigrationReport {
    let mut report = LgsMigrationReport::default();

    if let Some(reason) = plan.blocked {
        report.error = Some(format!(
            "refusing to migrate {} into {}: {}",
            plan.child_id,
            plan.new_dir.display(),
            reason.message()
        ));
        return report;
    }

    let lgs = LgsClient::new(plan.lgs_binary.clone());
    let git = GitManager::new();
    let adopting = matches!(plan.steps.first(), Some(Step::RestoreExisting { .. }));

    for step in plan.steps.clone() {
        let outcome: Result<()> = match &step {
            Step::RestoreExisting { name } => run_restore_existing(&plan, &lgs, name),
            Step::InitAndPush { .. } => run_init(&plan, &git),
            Step::CopyDataFiles => run_copy_data_files(&plan).map(|discrepancies| {
                report.discrepancies = discrepancies;
            }),
            Step::Canonicalize => run_canonicalize(&plan).map(|(rows_rounded, decisions)| {
                report.legacy_precision_rows_rounded = rows_rounded;
                report.merge_decisions = decisions;
            }),
            Step::Commit => run_commit(&plan, &git),
            Step::LgsAdd => run_lgs_add(&plan, &lgs),
            Step::EnsureLgsRemote => run_ensure_remote(&plan, &lgs, adopting),
            Step::Push => run_push(&plan),
            Step::RepointRegistry => run_repoint_registry(&plan),
        };

        match outcome {
            Ok(()) => report.completed_steps.push(step),
            Err(e) => {
                report.error = Some(e.to_string());
                report.failed_step = Some(step);
                return report;
            }
        }
    }

    if !report.discrepancies.is_empty() {
        report.notices.push(StartupNotice {
            severity: NoticeSeverity::Warning,
            title: format!("{}'s migrated data disagrees with the old folder in some files", plan.child_id),
            details: report
                .discrepancies
                .iter()
                .map(|f| {
                    format!(
                        "{f} differs between the migrated copy and the old folder's copy at {}. \
                         The migrated version was kept; review {f} by hand if the old folder's \
                         changes need to be reconciled.",
                        plan.old_dir.join(f).display()
                    )
                })
                .collect(),
        });
    }
    if !report.merge_decisions.is_empty() {
        log::warn!(
            "migrating {}: reconciling transactions.csv made {} non-trivial decision(s): {:?}",
            plan.child_id,
            report.merge_decisions.len(),
            report.merge_decisions
        );
    }

    let mut details = vec![format!(
        "The old folder was kept as a backup and was not deleted: {}",
        plan.old_dir.display()
    )];
    if report.legacy_precision_rows_rounded > 0 {
        details.push(format!(
            "{} row(s) had legacy floating-point precision corrected to the nearest cent as \
             part of the move (e.g. \"14.620000000000001\") — not a change in value.",
            report.legacy_precision_rows_rounded
        ));
    }
    report.notices.push(StartupNotice {
        severity: NoticeSeverity::Warning,
        title: format!("{}'s data now syncs through lgs", plan.child_id),
        details,
    });

    report
}

fn run_restore_existing(plan: &LgsMigrationPlan, lgs: &LgsClient, name: &str) -> Result<()> {
    if let Some(parent) = plan.new_dir.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let path = plan
        .new_dir
        .to_str()
        .context("the new child directory path is not valid UTF-8")?;
    lgs.restore(name, path).map(|_| ())
}

fn run_init(plan: &LgsMigrationPlan, git: &GitManager) -> Result<()> {
    fs::create_dir_all(&plan.new_dir)
        .with_context(|| format!("creating {}", plan.new_dir.display()))?;
    // `Repository::init` on an already-initialized repo is a safe no-op —
    // this is also the branch a RESUME of a fresh migration takes, and it
    // must not disturb whatever `new_dir` already has.
    git.init_repo(&plan.new_dir)
}

fn read_optional(path: &Path) -> Result<String> {
    if path.exists() {
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
    } else {
        Ok(String::new())
    }
}

/// Bring `child.yaml`, `allowance_config.yaml`, `goals.csv` into `new_dir`.
/// Never overwrites a file already there (an adopted history's file, or
/// this same migration's own prior partial attempt) — only speaks up, via
/// the returned discrepancy names, when the two versions actually differ.
/// `transactions.csv` is deliberately excluded: it is always reconciled by
/// [`run_canonicalize`], never a plain copy.
fn run_copy_data_files(plan: &LgsMigrationPlan) -> Result<Vec<String>> {
    let mut discrepancies = Vec::new();
    for name in FILES_THIS_APP_OWNS.iter().filter(|f| **f != TRANSACTIONS_FILE) {
        let src = plan.old_dir.join(name);
        let dst = plan.new_dir.join(name);

        if dst.exists() {
            if src.exists() {
                let existing = fs::read(&dst).with_context(|| format!("reading {}", dst.display()))?;
                let local = fs::read(&src).with_context(|| format!("reading {}", src.display()))?;
                if existing != local {
                    discrepancies.push(name.to_string());
                }
            }
            continue;
        }

        if !src.exists() {
            // Not every child has every file yet (e.g. no goal was ever
            // created) — absence is not an error.
            continue;
        }
        fs::copy(&src, &dst)
            .with_context(|| format!("copying {} to {}", src.display(), dst.display()))?;
    }
    Ok(discrepancies)
}

/// Reconcile `old_dir/transactions.csv` with whatever is already at
/// `new_dir/transactions.csv` (an adopted history, a resumed attempt's own
/// prior commit, or nothing at all on the ordinary first-time path) through
/// `allowance_core::merge::merge` with an empty base, and write the result
/// back canonically. See the module doc comment for why this must always be
/// a merge and never a blind overwrite.
///
/// Returns the total legacy-precision row count (summed across both sides)
/// and the merge's non-trivial decisions, if any.
fn run_canonicalize(plan: &LgsMigrationPlan) -> Result<(usize, Vec<Decision>)> {
    let old_path = plan.old_dir.join(TRANSACTIONS_FILE);
    let old_parsed = allowance_core::codec::parse_transactions(&read_optional(&old_path)?)
        .with_context(|| format!("parsing {}", old_path.display()))?;

    let new_path = plan.new_dir.join(TRANSACTIONS_FILE);
    let existing_parsed = allowance_core::codec::parse_transactions(&read_optional(&new_path)?)
        .with_context(|| format!("parsing {}", new_path.display()))?;

    // `ours`'s provenance is the new location's real commit when one
    // exists (an adopted history, or a resumed attempt's own prior
    // commit); a distinct synthetic sentinel otherwise (nothing committed
    // there yet — the ordinary first-time path, where `existing_parsed` is
    // empty anyway and never reaches the tiebreak that provenance feeds).
    // `theirs` (the old folder, never a git commit) gets its OWN distinct
    // sentinel — `merge`'s `wins()` asserts the two sides' provenance are
    // never equal, so even the fully-synthetic case (both sides sentinel)
    // must not collide.
    let existing_provenance = match git2::Repository::open(&plan.new_dir) {
        Ok(repo) => {
            let head_oid = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).map(|c| c.id());
            match head_oid {
                Some(oid) => provenance(&repo, oid)?,
                None => Provenance { committer_epoch: 0, commit_oid: [0u8; 20] },
            }
        }
        Err(_) => Provenance { committer_epoch: 0, commit_oid: [0u8; 20] },
    };

    let ours = Sided { rows: existing_parsed.rows, provenance: existing_provenance };
    let theirs = Sided {
        rows: old_parsed.rows,
        provenance: Provenance { committer_epoch: -1, commit_oid: [1u8; 20] },
    };

    let outcome = merge(None, &ours, &theirs);
    let rendered = allowance_core::codec::render_transactions(&outcome.rows);
    fs::write(&new_path, rendered).with_context(|| format!("writing {}", new_path.display()))?;

    Ok((old_parsed.rows_rounded + existing_parsed.rows_rounded, outcome.decisions))
}

fn run_commit(plan: &LgsMigrationPlan, git: &GitManager) -> Result<()> {
    for name in FILES_THIS_APP_OWNS {
        if plan.new_dir.join(name).exists() {
            git.add_file(&plan.new_dir, name)?;
        }
    }
    git.commit_if_changed(&plan.new_dir, "Migrate to lgs-backed repo")
        .map(|_| ())
}

fn run_lgs_add(plan: &LgsMigrationPlan, lgs: &LgsClient) -> Result<()> {
    let path = plan
        .new_dir
        .to_str()
        .context("the new child directory path is not valid UTF-8")?;
    lgs.add(path, &plan.project_name)
}

fn run_ensure_remote(plan: &LgsMigrationPlan, lgs: &LgsClient, adopting: bool) -> Result<()> {
    let repo = git2::Repository::open(&plan.new_dir)
        .with_context(|| format!("opening {}", plan.new_dir.display()))?;

    let url = if adopting {
        repo.find_remote("origin")
            .context("the restored repo has no `origin` remote")?
            .url()
            .context("the restored repo's `origin` remote has no URL")?
            .to_string()
    } else {
        let fresh_status = lgs
            .status()
            .context("querying `lgs status` to resolve the project's clone_url")?;
        fresh_status
            .project(&plan.project_name)
            .with_context(|| {
                format!(
                    "'{}' did not appear in `lgs status` — has it been `lgs add`ed yet?",
                    plan.project_name
                )
            })?
            .clone_url
            .clone()
    };

    ensure_lgs_remote(&repo, &url)
}

fn run_push(plan: &LgsMigrationPlan) -> Result<()> {
    let repo = git2::Repository::open(&plan.new_dir)
        .with_context(|| format!("opening {}", plan.new_dir.display()))?;
    let branch = current_branch(&repo)?;
    push_lgs(&repo, &branch)
}

fn run_repoint_registry(plan: &LgsMigrationPlan) -> Result<()> {
    let mut registry = ChildRegistry::load(&plan.data_dir)?;
    registry.repoint(&plan.child_id, plan.new_dir.clone())?;
    registry.save(&plan.data_dir)
}

// ============================================================================
// Task 19: onboarding a second machine.
//
// Everything below reads a project this machine has never seen — either
// `lgs status --json`'s `adoptable` list (discovered by scanning the cloud
// root; nothing here has been registered on THIS machine yet) or a project
// this machine already registered independently, on a day it was set up
// without any peer's history to adopt. Both are legitimate "second machine"
// shapes and neither is an error.
// ============================================================================

/// One `allowance-<id>` project this machine has not adopted yet, ready to
/// show as a checklist row.
///
/// `archived` and `note` are carried through, never dropped — an archived
/// project is a real project (its data still exists and is still readable),
/// just read-only going forward (see the module doc's push-refusal note), so
/// onboarding labels it rather than hiding it from the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptableChild {
    pub child_id: String,
    pub project_name: String,
    pub archived: bool,
    pub note: Option<String>,
    /// `Some(reason)` when THIS machine could not read the project's
    /// archive record at all (lgs's own `archive_unreadable` reason
    /// string) — `archived` above is then a default (`false`), not an
    /// observation. Review Important-2: must never be dropped and must
    /// never be treated as "confirmed not archived" — the record this
    /// binary could not read might well be an archived one. Same principle
    /// as `ProjectReport::is_confirmed_backed_up` treating `Unknown` as
    /// unsafe rather than defaulting it to safe. See
    /// [`Self::should_be_labelled_archived`].
    pub archive_status_unknown: Option<String>,
}

impl AdoptableChild {
    /// True whenever this row must NOT be presented as an ordinary,
    /// definitely-not-archived project: either lgs affirmatively says it is
    /// archived, or this machine could not read the archive record at all
    /// and so has no basis for saying it isn't. Prefer this over reading
    /// `archived` alone when deciding how to label a row.
    pub fn should_be_labelled_archived(&self) -> bool {
        self.archived || self.archive_status_unknown.is_some()
    }
}

/// lgs project names for this app are always `allowance-<child_id>` — see
/// [`project_name`].
const PROJECT_PREFIX: &str = "allowance-";

/// Filter `status`'s adoptable list down to this app's own projects, and
/// recover each one's `child_id` from its project name.
///
/// A cloud root shared with other, unrelated lgs projects (the ordinary case
/// — this app is not the only thing a user backs up with lgs) must not
/// surface those as if they were children to onboard.
pub fn adoptable_children(status: &StatusReport) -> Vec<AdoptableChild> {
    status
        .adoptable
        .iter()
        .filter_map(|entry| {
            entry.name.strip_prefix(PROJECT_PREFIX).map(|child_id| AdoptableChild {
                child_id: child_id.to_string(),
                project_name: entry.name.clone(),
                archived: entry.archived,
                note: entry.note.clone(),
                archive_status_unknown: entry.archive_unreadable.clone(),
            })
        })
        .collect()
}

/// What [`LgsClient::restore`]'s result means for onboarding.
#[derive(Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// `restore` cloned the project; the working copy is at the requested
    /// path with lgs's own remote (`origin`) already configured.
    Restored,
    /// `restore` refused because this machine already has a project by this
    /// name registered — the "both machines set up independently" case, not
    /// an error. The caller falls through to a plain clone-or-pull instead.
    AlreadyPresentUsePull,
    /// A genuine failure — anything `restore` can fail with other than the
    /// already-registered refusal above.
    Failed(String),
}

/// `lgs restore`'s refusal for an already-registered project is a plain
/// string match against its error message — `local-git-sync/src/cli.rs`
/// gives it no distinct error type, so this is the only seam available.
/// Matched narrowly (`"already exists"`) rather than the whole message, so
/// small wording changes upstream don't silently start treating this as a
/// hard failure.
pub fn interpret_restore_result(result: Result<String>) -> RestoreOutcome {
    match result {
        Ok(_) => RestoreOutcome::Restored,
        Err(e) => {
            let message = e.to_string();
            if message.contains("already exists") {
                RestoreOutcome::AlreadyPresentUsePull
            } else {
                RestoreOutcome::Failed(message)
            }
        }
    }
}

/// Adopt one `allowance-<id>` project onto this machine.
///
/// `project_name` is the full lgs project name (e.g. `allowance-keiko_hart`),
/// as listed by [`adoptable_children`] or already present in
/// `status.projects`.
///
/// - **Ordinary adopt**: `lgs restore` clones the project (it refuses a
///   non-empty non-repo directory itself — see `ensure_working_copy`,
///   `local-git-sync/src/cli.rs:700-733` — so this never clones a second
///   time). The clone's remote is named `origin`; [`ensure_lgs_remote`]
///   renames it to `lgs` so one remote name exists in the system regardless
///   of whether a child arrived via onboarding or via migration.
/// - **Already registered here**: `restore`'s refusal is not an error (see
///   [`RestoreOutcome::AlreadyPresentUsePull`]) — both machines were set up
///   independently. Falls through to a plain clone-or-pull: clone if nothing
///   is at the target directory yet, otherwise fetch and let the ordinary
///   sync engine (`ChildSyncEngine`) reconcile on its next cycle rather than
///   merging here.
///
/// Registers the child in `children.yaml` if it is not there already; never
/// touches an existing entry.
pub fn adopt_child(lgs: &LgsClient, project_name: &str, paths: &SyncPaths) -> Result<()> {
    let child_id_str = project_name
        .strip_prefix(PROJECT_PREFIX)
        .with_context(|| format!("'{project_name}' is not an allowance-tracker project"))?;
    let child_id = ChildId::from(child_id_str);
    let new_dir = paths.children_root.join(child_id.as_str());

    let restore_path = new_dir
        .to_str()
        .context("the child directory path is not valid UTF-8")?;
    let outcome = interpret_restore_result(lgs.restore(project_name, restore_path));

    match outcome {
        RestoreOutcome::Restored => {
            let repo = git2::Repository::open(&new_dir)
                .with_context(|| format!("opening {}", new_dir.display()))?;
            let url = repo
                .find_remote("origin")
                .context("the restored repo has no `origin` remote")?
                .url()
                .context("the restored repo's `origin` remote has no URL")?
                .to_string();
            ensure_lgs_remote(&repo, &url)?;
        }
        RestoreOutcome::AlreadyPresentUsePull => {
            let status = lgs
                .status()
                .context("running `lgs status --json` to resolve the clone_url")?;
            let url = status
                .project(project_name)
                .with_context(|| {
                    format!("'{project_name}' is registered here but not reported by `lgs status`")
                })?
                .clone_url
                .clone();

            if git2::Repository::open(&new_dir).is_ok() {
                let repo = git2::Repository::open(&new_dir)?;
                ensure_lgs_remote(&repo, &url)?;
                // Land the peer's tip so the ordinary sync engine has
                // something to reconcile against on its next cycle — never
                // merge or check out here, that is `ChildSyncEngine`'s job.
                fetch_lgs(&repo)?;
            } else {
                if let Some(parent) = new_dir.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                let repo = clone_repo(&url, &new_dir)?;
                ensure_lgs_remote(&repo, &url)?;
            }
        }
        RestoreOutcome::Failed(message) => {
            anyhow::bail!("{message}");
        }
    }

    let mut registry = ChildRegistry::load(&paths.data_dir)?;
    if registry.path_for(&child_id).is_none() {
        registry.register(RegistryEntry {
            id: child_id.clone(),
            path: new_dir.clone(),
            label: child_id.as_str().to_string(),
        })?;
        registry.save(&paths.data_dir)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::sync::lgs_client::{DaemonInfo, DurabilityState, ProjectReport};
    use crate::backend::storage::csv::REGISTRY_FILENAME;
    use tempfile::TempDir;

    fn child(id: &str) -> RegistryEntry {
        RegistryEntry {
            id: ChildId::from(id),
            path: PathBuf::from(format!("/fake/old/{id}")),
            label: id.to_string(),
        }
    }

    fn paths() -> SyncPaths {
        SyncPaths {
            data_dir: PathBuf::from("/fake/data"),
            children_root: PathBuf::from("/fake/children"),
            lgs_binary: PathBuf::from("/fake/bin/lgs"),
            cloud_root: None,
            home: PathBuf::from("/fake/home"),
        }
    }

    fn status_with_adoptable(names: &[&str]) -> StatusReport {
        StatusReport {
            daemon: DaemonInfo::default(),
            cloud_root: None,
            cloud_root_exists: false,
            projects: Vec::new(),
            adoptable: names
                .iter()
                .map(|s| crate::backend::sync::lgs_client::AdoptableEntry {
                    name: s.to_string(),
                    archived: false,
                    note: None,
                    archive_unreadable: None,
                })
                .collect(),
        }
    }

    fn status_with_projects(entries: &[(&str, &str)]) -> StatusReport {
        StatusReport {
            daemon: DaemonInfo::default(),
            cloud_root: None,
            cloud_root_exists: false,
            projects: entries
                .iter()
                .map(|(name, clone_url)| ProjectReport {
                    name: name.to_string(),
                    clone_url: clone_url.to_string(),
                    working_repo_path: PathBuf::from("/tmp/x"),
                    durability_state: DurabilityState::Unknown,
                    durability_label: None,
                    failed_sync_attempts: 0,
                    archived: false,
                })
                .collect(),
            adoptable: Vec::new(),
        }
    }

    // --- The four tests specified in the task brief. ---

    #[test]
    fn adopts_an_existing_project_instead_of_creating_an_unrelated_root() {
        // A migrates Monday, B on Friday. Without this check B does its own
        // `git init`, the histories are unrelated, and any row A deleted in
        // that window resurrects through the empty-base union.
        let status = status_with_adoptable(&["allowance-keiko_hart"]);
        let plan = plan_lgs_migration(&child("keiko_hart"), &paths(), &status);
        assert_eq!(plan.steps[0], Step::RestoreExisting { name: "allowance-keiko_hart".into() });
    }

    #[test]
    fn creates_a_fresh_repo_when_nothing_is_adoptable() {
        let plan = plan_lgs_migration(&child("keiko_hart"), &paths(), &status_with_adoptable(&[]));
        assert_eq!(plan.steps[0], Step::InitAndPush { name: "allowance-keiko_hart".into() });
    }

    #[test]
    fn registry_is_repointed_only_after_every_other_step_succeeds() {
        let plan = plan_lgs_migration(&child("keiko_hart"), &paths(), &status_with_adoptable(&[]));
        assert_eq!(*plan.steps.last().unwrap(), Step::RepointRegistry);
    }

    #[test]
    fn a_failure_leaves_the_registry_and_the_old_folder_untouched() {
        let env = TestEnvironment::new().unwrap();
        let before_registry = fs::read_to_string(env.registry_path()).unwrap();
        let before_old = snapshot_old_dir(&env);

        let report = run_lgs_migration(plan_that_fails_at_push(&env));

        assert!(report.failed());
        // Pin exactly where this fails, not merely that it fails somewhere
        // — a bug that degrades this into failing at an earlier step must
        // not pass silently.
        assert_eq!(report.failed_step, Some(Step::Push));
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before_registry);
        assert_eq!(snapshot_old_dir(&env), before_old, "the old folder's bytes must be unchanged");
    }

    // --- Supporting fixture. ---

    /// A tempdir-backed fixture standing in for the real machine: an old
    /// (pre-migration) child folder with real-shaped data (including one
    /// legacy f64-precision row), a `children.yaml` registry pointing at it,
    /// and a separate root standing in for `~/Library/Application
    /// Support/.../children`. Nothing here is the user's real data.
    struct TestEnvironment {
        base: TempDir,
        children_root: TempDir,
        old: TempDir,
        scratch: TempDir,
    }

    impl TestEnvironment {
        fn new() -> Result<Self> {
            let base = TempDir::new()?;
            let children_root = TempDir::new()?;
            let old = TempDir::new()?;
            let scratch = TempDir::new()?;

            let old_child_dir = old.path().join("keiko_hart");
            fs::create_dir_all(&old_child_dir)?;
            fs::write(
                old_child_dir.join("child.yaml"),
                "id: keiko_hart\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
                 created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n",
            )?;
            fs::write(old_child_dir.join("allowance_config.yaml"), "amount: 5.0\n")?;
            fs::write(old_child_dir.join("goals.csv"), "id,child_id,description\n")?;
            // One row carries legacy f64-precision noise on `amount`,
            // matching what was actually found in the real
            // transactions.csv this migration is written for (there,
            // it was 57 of 93 rows). `balance` is written as the correct
            // running total for a single row (its own amount) so that
            // `merge`'s unconditional `recompute_running_balances` (see
            // `run_canonicalize`'s doc comment — Canonicalize is always a
            // merge, even with nothing to merge against) reproduces the
            // exact same value rather than changing it — this fixture is
            // about precision-rounding, not about exercising balance
            // recomputation across multiple rows.
            fs::write(
                old_child_dir.join("transactions.csv"),
                "id,child_id,date,description,amount,balance,type\n\
                 tx-1,keiko_hart,2024-01-01T00:00:00Z,Allowance,14.620000000000001,14.62,allowance\n",
            )?;

            let mut registry = ChildRegistry::default();
            registry.register(RegistryEntry {
                id: ChildId::from("keiko_hart"),
                path: old_child_dir.clone(),
                label: "Keiko Hart".to_string(),
            })?;
            registry.save(base.path())?;

            Ok(Self { base, children_root, old, scratch })
        }

        fn base_dir(&self) -> PathBuf {
            self.base.path().to_path_buf()
        }

        fn scratch_dir(&self) -> &Path {
            self.scratch.path()
        }

        fn registry_path(&self) -> PathBuf {
            self.base.path().join(REGISTRY_FILENAME)
        }

        fn old_child_dir(&self) -> PathBuf {
            self.old.path().join("keiko_hart")
        }

        fn entry(&self) -> RegistryEntry {
            RegistryEntry {
                id: ChildId::from("keiko_hart"),
                path: self.old_child_dir(),
                label: "Keiko Hart".to_string(),
            }
        }

        /// A `SyncPaths` whose `home` deliberately does not exist on disk
        /// (so the `~/Documents` symlink probe just resolves to `false`)
        /// and is nowhere near iCloud, so it never trips the cloud-sync
        /// guard on its own.
        fn paths(&self, lgs_binary: PathBuf) -> SyncPaths {
            SyncPaths {
                data_dir: self.base_dir(),
                children_root: self.children_root.path().to_path_buf(),
                lgs_binary,
                cloud_root: None,
                home: self.scratch.path().join("not_a_real_home"),
            }
        }
    }

    /// Read every plain FILE directly inside `env.old_child_dir()`, sorted
    /// by name, as raw bytes — a content-level pin, not merely an
    /// existence check. A bug that truncated or rewrote a file in place
    /// would pass an `.exists()` check but must fail this one.
    ///
    /// Skips directories rather than `fs::read`ing (and panicking on) them.
    /// This fixture has no subdirectory today, but the REAL folder this
    /// migration reads from does — it has a `.git` — so a helper that
    /// blows up the moment one appears is a trap for whoever writes the
    /// next test against a more realistic fixture.
    fn snapshot_old_dir(env: &TestEnvironment) -> Vec<(String, Vec<u8>)> {
        let dir = env.old_child_dir();
        let mut names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names.into_iter().map(|n| (n.clone(), fs::read(dir.join(&n)).unwrap())).collect()
    }

    /// Write an executable fake `lgs` binary at `dir/lgs` whose body is
    /// `body` (a `/bin/sh` script). Mirrors the same technique
    /// `lgs_client.rs`'s own tests use to exercise `LgsClient` without ever
    /// invoking the real `lgs`.
    fn fake_lgs_script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("lgs");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        path
    }

    /// A fake `lgs status --json` response naming one project with the
    /// given `clone_url`. `add` always succeeds.
    fn status_json(clone_url: &str) -> String {
        format!(
            "{{\"daemon_running\":true,\"cloud_root\":null,\"cloud_root_exists\":false,\
             \"projects\":[{{\"name\":\"allowance-keiko_hart\",\
             \"working_repo_path\":\"/tmp/x\",\"clone_url\":\"{clone_url}\",\
             \"archived\":false}}],\"adoptable\":[]}}"
        )
    }

    fn plan_that_fails_at_push(env: &TestEnvironment) -> LgsMigrationPlan {
        // `add` succeeds, `status` hands back a clone_url that cannot
        // actually be pushed to — real git2, no fake at the push layer,
        // just an unreachable local path. This fails exactly at `Push`,
        // after every earlier step (including `EnsureLgsRemote`, which
        // merely records the URL) has already succeeded.
        let json = status_json("/nonexistent/bogus-remote.git");
        let script = fake_lgs_script(
            env.scratch_dir(),
            &format!(
                "case \"$1\" in\n  add) exit 0 ;;\n  status) cat <<'JSON'\n{json}\nJSON\n  ;;\nesac\n"
            ),
        );
        let paths = env.paths(script);
        plan_lgs_migration(&env.entry(), &paths, &status_with_adoptable(&[]))
    }

    #[test]
    fn a_failure_at_lgs_add_leaves_the_registry_and_the_old_folder_untouched() {
        let env = TestEnvironment::new().unwrap();
        let script = fake_lgs_script(
            env.scratch_dir(),
            "case \"$1\" in\n  add) echo boom >&2; exit 1 ;;\nesac\n",
        );
        let paths = env.paths(script);
        let plan = plan_lgs_migration(&env.entry(), &paths, &status_with_adoptable(&[]));

        let before_registry = fs::read_to_string(env.registry_path()).unwrap();
        let before_old = snapshot_old_dir(&env);

        let report = run_lgs_migration(plan);

        assert!(report.failed());
        assert_eq!(report.failed_step, Some(Step::LgsAdd));
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before_registry);
        assert_eq!(snapshot_old_dir(&env), before_old);
    }

    #[test]
    fn a_failure_at_the_very_first_step_leaves_the_registry_and_the_old_folder_untouched() {
        let env = TestEnvironment::new().unwrap();
        let mut paths = env.paths(PathBuf::from("/fake/bin/lgs-not-used"));
        // `children_root` sits underneath a plain FILE, not a directory, so
        // `fs::create_dir_all` inside `InitAndPush` fails deterministically
        // before the (unused, hence the bogus binary path above) `lgs`
        // binary is ever invoked.
        let blocker = env.scratch_dir().join("blocker_file");
        fs::write(&blocker, "x").unwrap();
        paths.children_root = blocker.join("children");

        let plan = plan_lgs_migration(&env.entry(), &paths, &status_with_adoptable(&[]));
        let before_registry = fs::read_to_string(env.registry_path()).unwrap();
        let before_old = snapshot_old_dir(&env);

        let report = run_lgs_migration(plan);

        assert!(report.failed());
        assert_eq!(
            report.failed_step,
            Some(Step::InitAndPush { name: "allowance-keiko_hart".into() })
        );
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before_registry);
        assert_eq!(snapshot_old_dir(&env), before_old);
    }

    #[test]
    fn refuses_a_target_inside_a_cloud_synced_path() {
        let env = TestEnvironment::new().unwrap();
        let cloud_root = TempDir::new().unwrap();
        // Canonicalize the tempdir path before using it as `cloud_root`:
        // on macOS, `/var/folders/...` (what `TempDir` hands back) is
        // itself a symlink to `/private/var/folders/...`, and the
        // production fix under test canonicalizes the CANDIDATE before
        // comparing — so the reference point must be in the same resolved
        // form for the comparison to land, exactly as it would in
        // production if `SyncPaths::cloud_root` is already realpath'd.
        let cloud_root_canonical = cloud_root.path().canonicalize().unwrap();

        let mut paths = env.paths(PathBuf::from("/fake/bin/lgs-not-used"));
        paths.children_root = cloud_root_canonical.join("children");
        paths.cloud_root = Some(cloud_root_canonical);

        let plan = plan_lgs_migration(&env.entry(), &paths, &status_with_adoptable(&[]));
        assert!(plan.steps.is_empty(), "a refused target must plan no steps to run");
        assert_eq!(plan.blocked, Some(Reason::InsideCloudRoot));

        let before_registry = fs::read_to_string(env.registry_path()).unwrap();
        let before_old = snapshot_old_dir(&env);

        let report = run_lgs_migration(plan);

        assert!(report.failed());
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before_registry);
        assert_eq!(snapshot_old_dir(&env), before_old);
    }

    #[test]
    fn reports_the_legacy_precision_count_and_completes_successfully() {
        let env = TestEnvironment::new().unwrap();
        let bare_dir = TempDir::new().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();

        let json = status_json(&bare_dir.path().to_string_lossy());
        let script = fake_lgs_script(
            env.scratch_dir(),
            &format!(
                "case \"$1\" in\n  add) exit 0 ;;\n  status) cat <<'JSON'\n{json}\nJSON\n  ;;\nesac\n"
            ),
        );
        let paths = env.paths(script);
        let new_dir = paths.children_root.join("keiko_hart");
        let plan = plan_lgs_migration(&env.entry(), &paths, &status_with_adoptable(&[]));

        let report = run_lgs_migration(plan);

        assert!(!report.failed(), "expected success, got: {:?}", report.error);
        assert_eq!(report.legacy_precision_rows_rounded, 1);
        assert!(
            report
                .notices
                .iter()
                .any(|n| n.details.iter().any(|d| d.contains("1 row"))),
            "the legacy-precision count must reach a StartupNotice: {:?}",
            report.notices
        );
        assert!(
            report
                .notices
                .iter()
                .any(|n| n.details.iter().any(|d| d.contains(&env.old_child_dir().display().to_string()))),
            "the notice must name the old folder's path: {:?}",
            report.notices
        );

        let registry = ChildRegistry::load(&env.base_dir()).unwrap();
        assert_eq!(registry.path_for(&ChildId::from("keiko_hart")), Some(new_dir.as_path()));

        let migrated = fs::read_to_string(new_dir.join("transactions.csv")).unwrap();
        assert!(migrated.contains("14.62"), "got: {migrated}");
        assert!(!migrated.contains("14.620000000000001"), "got: {migrated}");

        assert!(env.old_child_dir().exists());
        let original = fs::read_to_string(env.old_child_dir().join("transactions.csv")).unwrap();
        assert!(original.contains("14.620000000000001"));
    }

    // --- Critical-2: re-runnable after a partial failure. ---

    #[test]
    fn resumes_after_a_failure_at_lgs_add() {
        let env = TestEnvironment::new().unwrap();

        // Attempt 1: `add` fails outright.
        let script1 = fake_lgs_script(
            env.scratch_dir(),
            "case \"$1\" in\n  add) echo boom >&2; exit 1 ;;\nesac\n",
        );
        let paths1 = env.paths(script1);
        let plan1 = plan_lgs_migration(&env.entry(), &paths1, &status_with_adoptable(&[]));
        let report1 = run_lgs_migration(plan1);
        assert!(report1.failed());
        assert_eq!(report1.failed_step, Some(Step::LgsAdd));

        // Attempt 2 (resume): a fresh `lgs status` now shows the project
        // already registered — as it would be if `add` actually landed on
        // the daemon despite this process seeing a failure. The retry must
        // skip `LgsAdd` (calling it again fails permanently) and push
        // straight through.
        let bare_dir = TempDir::new().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();
        let clone_url = bare_dir.path().to_string_lossy().to_string();
        let script2 = fake_lgs_script(
            env.scratch_dir(),
            &format!(
                "case \"$1\" in\n  status) cat <<'JSON'\n{}\nJSON\n  ;;\nesac\n",
                status_json(&clone_url)
            ),
        );
        let paths2 = env.paths(script2);
        let status2 = status_with_projects(&[("allowance-keiko_hart", &clone_url)]);
        let plan2 = plan_lgs_migration(&env.entry(), &paths2, &status2);
        assert!(
            !plan2.steps.contains(&Step::LgsAdd),
            "a project already in `lgs status` must not be `lgs add`ed again: {:?}",
            plan2.steps
        );

        let report2 = run_lgs_migration(plan2);
        assert!(!report2.failed(), "resume must succeed: {:?}", report2.error);

        let new_dir = paths2.children_root.join("keiko_hart");
        let registry = ChildRegistry::load(&env.base_dir()).unwrap();
        assert_eq!(registry.path_for(&ChildId::from("keiko_hart")), Some(new_dir.as_path()));
    }

    #[test]
    fn resumes_after_a_failure_at_ensure_remote() {
        let env = TestEnvironment::new().unwrap();

        // Attempt 1: `add` succeeds, but `status` (needed to resolve the
        // clone_url) fails.
        let script1 = fake_lgs_script(
            env.scratch_dir(),
            "case \"$1\" in\n  add) exit 0 ;;\n  status) echo boom >&2; exit 1 ;;\nesac\n",
        );
        let paths1 = env.paths(script1);
        let plan1 = plan_lgs_migration(&env.entry(), &paths1, &status_with_adoptable(&[]));
        let report1 = run_lgs_migration(plan1);
        assert!(report1.failed());
        assert_eq!(report1.failed_step, Some(Step::EnsureLgsRemote));

        // Attempt 2: `add` already ran, so `status` now shows the project
        // registered, and this time actually resolves — a real bare repo.
        let bare_dir = TempDir::new().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();
        let clone_url = bare_dir.path().to_string_lossy().to_string();
        let script2 = fake_lgs_script(
            env.scratch_dir(),
            &format!(
                "case \"$1\" in\n  status) cat <<'JSON'\n{}\nJSON\n  ;;\nesac\n",
                status_json(&clone_url)
            ),
        );
        let paths2 = env.paths(script2);
        let status2 = status_with_projects(&[("allowance-keiko_hart", &clone_url)]);
        let plan2 = plan_lgs_migration(&env.entry(), &paths2, &status2);
        assert!(!plan2.steps.contains(&Step::LgsAdd));

        let report2 = run_lgs_migration(plan2);
        assert!(!report2.failed(), "resume must succeed: {:?}", report2.error);

        let new_dir = paths2.children_root.join("keiko_hart");
        let registry = ChildRegistry::load(&env.base_dir()).unwrap();
        assert_eq!(registry.path_for(&ChildId::from("keiko_hart")), Some(new_dir.as_path()));
    }

    #[test]
    fn resumes_after_a_failure_at_push() {
        let env = TestEnvironment::new().unwrap();

        // Attempt 1: `add` and `status` both succeed, but the resolved
        // clone_url is bogus, so `Push` fails.
        let json1 = status_json("/nonexistent/bogus-remote.git");
        let script1 = fake_lgs_script(
            env.scratch_dir(),
            &format!(
                "case \"$1\" in\n  add) exit 0 ;;\n  status) cat <<'JSON'\n{json1}\nJSON\n  ;;\nesac\n"
            ),
        );
        let paths1 = env.paths(script1);
        let plan1 = plan_lgs_migration(&env.entry(), &paths1, &status_with_adoptable(&[]));
        let report1 = run_lgs_migration(plan1);
        assert!(report1.failed());
        assert_eq!(report1.failed_step, Some(Step::Push));

        // Attempt 2: the daemon now reports a real, reachable clone_url —
        // `ensure_lgs_remote` self-heals the stale URL from attempt 1 — and
        // the project is already registered, so `LgsAdd` is skipped.
        let bare_dir = TempDir::new().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();
        let clone_url = bare_dir.path().to_string_lossy().to_string();
        let script2 = fake_lgs_script(
            env.scratch_dir(),
            &format!(
                "case \"$1\" in\n  status) cat <<'JSON'\n{}\nJSON\n  ;;\nesac\n",
                status_json(&clone_url)
            ),
        );
        let paths2 = env.paths(script2);
        let status2 = status_with_projects(&[("allowance-keiko_hart", &clone_url)]);
        let plan2 = plan_lgs_migration(&env.entry(), &paths2, &status2);
        assert!(!plan2.steps.contains(&Step::LgsAdd));

        let report2 = run_lgs_migration(plan2);
        assert!(!report2.failed(), "resume must succeed: {:?}", report2.error);

        let new_dir = paths2.children_root.join("keiko_hart");
        let registry = ChildRegistry::load(&env.base_dir()).unwrap();
        assert_eq!(registry.path_for(&ChildId::from("keiko_hart")), Some(new_dir.as_path()));
    }

    // --- Critical-1: adopting must merge, never overwrite. ---

    /// Seed a bare repo with two commits, as if machine A had already
    /// migrated this child (commit 1: `tx-a1`) and then, independently,
    /// written and pushed one more transaction after migrating (commit 2:
    /// `tx-a2`). Returns the branch name actually used, so the caller does
    /// not have to guess "main" vs "master".
    fn seed_bare_with_machine_as_history(bare_path: &Path) -> String {
        let work = TempDir::new().unwrap();
        let repo = git2::Repository::init(work.path()).unwrap();
        let git = GitManager::new();

        fs::write(
            work.path().join("child.yaml"),
            "id: keiko_hart\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
             created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n",
        )
        .unwrap();
        fs::write(work.path().join("allowance_config.yaml"), "amount: 5.0\n").unwrap();
        fs::write(work.path().join("goals.csv"), "id,child_id,description\n").unwrap();
        fs::write(
            work.path().join("transactions.csv"),
            "id,child_id,date,description,amount,balance,type\n\
             tx-a1,keiko_hart,2024-01-01T00:00:00Z,A's first row,5.00,5.00,allowance\n",
        )
        .unwrap();
        git.add_all(work.path()).unwrap();
        git.commit(work.path(), "A: initial migration").unwrap();

        // The row A wrote AFTER migrating — this is the row a broken
        // overwrite-on-adopt would silently delete.
        fs::write(
            work.path().join("transactions.csv"),
            "id,child_id,date,description,amount,balance,type\n\
             tx-a1,keiko_hart,2024-01-01T00:00:00Z,A's first row,5.00,5.00,allowance\n\
             tx-a2,keiko_hart,2024-01-02T00:00:00Z,A's row after migrating,3.00,8.00,allowance\n",
        )
        .unwrap();
        git.add_all(work.path()).unwrap();
        git.commit(work.path(), "A: a row written after migrating").unwrap();

        repo.remote("origin", bare_path.to_str().unwrap()).unwrap();
        let branch = current_branch(&repo).unwrap();
        let mut remote = repo.find_remote("origin").unwrap();
        remote
            .push(&[format!("refs/heads/{branch}:refs/heads/{branch}")], None)
            .unwrap();
        branch
    }

    /// Read `path`'s content out of `repo_dir`'s HEAD commit tree — the
    /// COMMITTED content, not the working-tree file. Round-2 review,
    /// Minor-1: asserting only the working-tree file would still pass if a
    /// bug staged nothing at all (`Push` on an unchanged branch is a
    /// harmless no-op), which is exactly the failure mode the adopt test
    /// exists to catch.
    fn read_committed_file(repo_dir: &Path, path: &str) -> String {
        let repo = git2::Repository::open(repo_dir).unwrap();
        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = commit.tree().unwrap();
        let entry = tree.get_path(Path::new(path)).unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        String::from_utf8(blob.content().to_vec()).unwrap()
    }

    /// Same, but reads out of a BARE repo's named branch tip — proving the
    /// content actually reached the remote (was pushed), not merely
    /// committed in the local working repo. `branch` is the shorthand name
    /// (e.g. `"main"`).
    fn read_pushed_file(bare_dir: &Path, branch: &str, path: &str) -> String {
        let repo = git2::Repository::open_bare(bare_dir).unwrap();
        let reference = repo.find_reference(&format!("refs/heads/{branch}")).unwrap();
        let commit = reference.peel_to_commit().unwrap();
        let tree = commit.tree().unwrap();
        let entry = tree.get_path(Path::new(path)).unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        String::from_utf8(blob.content().to_vec()).unwrap()
    }

    #[test]
    fn adopting_merges_the_restored_history_with_the_stale_local_copy_instead_of_overwriting_it() {
        let env = TestEnvironment::new().unwrap();

        // B's own stale local row, distinct from anything A has, plus a
        // goals.csv that deliberately DIFFERS from what A's restored
        // history has — proving the discrepancy is reported, not silently
        // resolved by picking B's copy.
        fs::write(
            env.old_child_dir().join("transactions.csv"),
            "id,child_id,date,description,amount,balance,type\n\
             tx-b1,keiko_hart,2024-01-03T00:00:00Z,B's own row,2.00,2.00,allowance\n",
        )
        .unwrap();
        fs::write(env.old_child_dir().join("goals.csv"), "id,child_id,description\nb-goal,keiko_hart,Bike\n").unwrap();

        let bare_dir = TempDir::new().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();
        let branch = seed_bare_with_machine_as_history(bare_dir.path());

        let script = fake_lgs_script(
            env.scratch_dir(),
            &format!(
                "case \"$1\" in\n  restore) git clone --quiet \"{}\" \"$3\" ;;\nesac\n",
                bare_dir.path().display()
            ),
        );
        let paths = env.paths(script);
        let new_dir = paths.children_root.join("keiko_hart");
        let status = status_with_adoptable(&["allowance-keiko_hart"]);
        let plan = plan_lgs_migration(&env.entry(), &paths, &status);
        assert_eq!(plan.steps[0], Step::RestoreExisting { name: "allowance-keiko_hart".into() });

        let report = run_lgs_migration(plan);
        assert!(!report.failed(), "expected success, got: {:?}", report.error);

        // Both the row A wrote after migrating, and the row only B has,
        // survive — nothing was dropped by the adoption. Asserted against
        // the COMMITTED tree (`HEAD:transactions.csv`), not the
        // working-tree file: a bug that staged nothing would leave `Push`
        // a harmless no-op on an unchanged branch, and a working-tree-only
        // assertion would not catch that. Also asserted against the BARE
        // repo's own branch tip, proving the rows actually reached the
        // remote — "pushed" is the property that makes them durable on
        // the other machine, which is the entire point of this test.
        let committed = read_committed_file(&new_dir, "transactions.csv");
        assert!(committed.contains("tx-a1"), "got: {committed}");
        assert!(committed.contains("tx-a2"), "A's post-migration row must survive adoption: {committed}");
        assert!(committed.contains("tx-b1"), "B's own row must survive adoption: {committed}");

        let pushed = read_pushed_file(bare_dir.path(), &branch, "transactions.csv");
        assert!(pushed.contains("tx-a1"), "got: {pushed}");
        assert!(pushed.contains("tx-a2"), "A's post-migration row must reach the remote: {pushed}");
        assert!(pushed.contains("tx-b1"), "B's own row must reach the remote: {pushed}");

        // goals.csv differed between the restored history and B's stale
        // copy; the restored (already-synced) version was kept — checked
        // committed AND pushed, same reasoning as above — and the
        // discrepancy was surfaced rather than silently resolved.
        let committed_goals = read_committed_file(&new_dir, "goals.csv");
        assert!(!committed_goals.contains("Bike"), "B's stale goals.csv must not silently win: {committed_goals}");
        let pushed_goals = read_pushed_file(bare_dir.path(), &branch, "goals.csv");
        assert!(!pushed_goals.contains("Bike"), "B's stale goals.csv must not reach the remote: {pushed_goals}");
        assert!(report.discrepancies.contains(&"goals.csv".to_string()), "got: {:?}", report.discrepancies);
        assert!(
            report
                .notices
                .iter()
                .any(|n| n.details.iter().any(|d| d.contains("goals.csv"))),
            "the discrepancy must reach a StartupNotice: {:?}",
            report.notices
        );

        // The old folder itself is still exactly what it was — read-only
        // throughout.
        assert!(env.old_child_dir().exists());
        let original = fs::read_to_string(env.old_child_dir().join("transactions.csv")).unwrap();
        assert!(original.contains("tx-b1"));
        assert!(!original.contains("tx-a1"), "the old folder must never be written into");
    }
}

/// Task 19: onboarding a second machine. Module name deliberately contains
/// "onboard" so `cargo test -p allowance-tracker-egui onboard` selects
/// exactly this set.
#[cfg(test)]
mod onboarding_tests {
    use super::*;
    use crate::backend::sync::lgs_client::{AdoptableEntry, DaemonInfo};
    use std::path::Path;
    use tempfile::TempDir;

    fn status_with_adoptable_entries(entries: Vec<AdoptableEntry>) -> StatusReport {
        StatusReport {
            daemon: DaemonInfo::default(),
            cloud_root: None,
            cloud_root_exists: false,
            projects: Vec::new(),
            adoptable: entries,
        }
    }

    fn status_with_adoptable(names: &[&str]) -> StatusReport {
        status_with_adoptable_entries(
            names
                .iter()
                .map(|n| AdoptableEntry {
                    name: n.to_string(),
                    archived: false,
                    note: None,
                    archive_unreadable: None,
                })
                .collect(),
        )
    }

    fn status_with_archived_adoptable(name: &str, note: &str) -> StatusReport {
        status_with_adoptable_entries(vec![AdoptableEntry {
            name: name.to_string(),
            archived: true,
            note: Some(note.to_string()),
            archive_unreadable: None,
        }])
    }

    fn status_with_unreadable_archive_adoptable(name: &str, reason: &str) -> StatusReport {
        status_with_adoptable_entries(vec![AdoptableEntry {
            name: name.to_string(),
            archived: false,
            note: None,
            archive_unreadable: Some(reason.to_string()),
        }])
    }

    // --- The three tests specified in the task brief. ---

    #[test]
    fn lists_only_allowance_projects() {
        let status = status_with_adoptable(&["allowance-keiko_hart", "weathertop-data"]);
        let found = adoptable_children(&status);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].child_id, "keiko_hart");
    }

    #[test]
    fn an_archived_project_is_labelled_rather_than_hidden() {
        let status = status_with_archived_adoptable("allowance-keiko_hart", "finished with this child");
        let found = adoptable_children(&status);
        assert!(found[0].archived);
        assert_eq!(found[0].note.as_deref(), Some("finished with this child"));
        assert!(found[0].should_be_labelled_archived());
    }

    /// Review Important-2: an entry lgs could not read the archive record
    /// for must carry that fact through — `archived` alone defaults to
    /// `false` here (mirroring lgs's own default-when-unreadable), which
    /// would otherwise render as "confirmed not archived." It must not:
    /// `should_be_labelled_archived` is the safe read, and it must be
    /// `true` here exactly as it is for a genuinely archived row.
    #[test]
    fn an_unreadable_archive_record_is_carried_through_and_labelled_as_unknown_not_safe() {
        let status = status_with_unreadable_archive_adoptable(
            "allowance-keiko_hart",
            "permission denied reading archive/state.yaml",
        );
        let found = adoptable_children(&status);
        assert!(!found[0].archived, "archived itself is only ever a default here, never an observation");
        assert_eq!(
            found[0].archive_status_unknown.as_deref(),
            Some("permission denied reading archive/state.yaml")
        );
        assert!(
            found[0].should_be_labelled_archived(),
            "an unreadable archive record must never present as a safe, ordinary project"
        );
    }

    #[test]
    fn a_refusal_because_it_is_already_registered_falls_through_to_pull() {
        let outcome = interpret_restore_result(Err(anyhow::anyhow!(
            "project 'allowance-keiko_hart' already exists"
        )));
        assert_eq!(outcome, RestoreOutcome::AlreadyPresentUsePull);
    }

    // --- Supplementary coverage for `adopt_child` and `interpret_restore_result`. ---

    #[test]
    fn a_genuine_restore_failure_is_reported_as_failed() {
        let outcome = interpret_restore_result(Err(anyhow::anyhow!("daemon unreachable")));
        assert_eq!(outcome, RestoreOutcome::Failed("daemon unreachable".to_string()));
    }

    #[test]
    fn a_successful_restore_is_reported_as_restored() {
        let outcome = interpret_restore_result(Ok("cloned".to_string()));
        assert_eq!(outcome, RestoreOutcome::Restored);
    }

    fn fake_lgs_script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("lgs");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn status_json(project_name: &str, clone_url: &str) -> String {
        format!(
            "{{\"daemon_running\":true,\"cloud_root\":null,\"cloud_root_exists\":false,\
             \"projects\":[{{\"name\":\"{project_name}\",\
             \"working_repo_path\":\"/tmp/x\",\"clone_url\":\"{clone_url}\",\
             \"archived\":false}}],\"adoptable\":[]}}"
        )
    }

    fn onboarding_paths(scratch: &Path, children_root: PathBuf, data_dir: PathBuf, lgs_binary: PathBuf) -> SyncPaths {
        SyncPaths {
            data_dir,
            children_root,
            lgs_binary,
            cloud_root: None,
            home: scratch.join("not_a_real_home"),
        }
    }

    fn seed_bare_repo() -> (TempDir, String) {
        let bare_dir = TempDir::new().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let work = TempDir::new().unwrap();
        let repo = git2::Repository::init(work.path()).unwrap();
        fs::write(work.path().join("child.yaml"), "id: keiko_hart\nname: Keiko Hart\n").unwrap();
        let git = GitManager::new();
        git.add_all(work.path()).unwrap();
        let _ = bare; // keep the bare repo alive via bare_dir; commit lands via push below.
        git.commit(work.path(), "seed").unwrap();
        repo.remote("origin", bare_dir.path().to_str().unwrap()).unwrap();
        let branch = current_branch(&repo).unwrap();
        let mut remote = repo.find_remote("origin").unwrap();
        remote.push(&[format!("refs/heads/{branch}:refs/heads/{branch}")], None).unwrap();
        (bare_dir, branch)
    }

    /// Ordinary adopt: `restore` succeeds (clones), and the clone's `origin`
    /// remote is renamed to `lgs` — no second clone is ever attempted.
    #[test]
    fn adopt_child_restores_and_renames_origin_to_lgs() {
        let (bare_dir, _branch) = seed_bare_repo();
        let scratch = TempDir::new().unwrap();
        let children_root = TempDir::new().unwrap();
        let data_dir = TempDir::new().unwrap();

        let script = fake_lgs_script(
            scratch.path(),
            &format!(
                "case \"$1\" in\n  restore) git clone --quiet \"{}\" \"$3\" ;;\nesac\n",
                bare_dir.path().display()
            ),
        );
        let paths = onboarding_paths(
            scratch.path(),
            children_root.path().to_path_buf(),
            data_dir.path().to_path_buf(),
            script,
        );
        let lgs = LgsClient::new(paths.lgs_binary.clone());

        adopt_child(&lgs, "allowance-keiko_hart", &paths).unwrap();

        let new_dir = children_root.path().join("keiko_hart");
        let repo = git2::Repository::open(&new_dir).unwrap();
        assert!(repo.find_remote("lgs").is_ok(), "origin must be renamed to lgs");
        assert!(repo.find_remote("origin").is_err(), "origin must not remain once renamed");

        let registry = ChildRegistry::load(data_dir.path()).unwrap();
        assert_eq!(registry.path_for(&ChildId::from("keiko_hart")), Some(new_dir.as_path()));
    }

    /// Both machines set up independently: `restore` refuses because
    /// `allowance-keiko_hart` is already registered here, but nothing is at
    /// the target directory yet. `adopt_child` must not error — it clones
    /// directly instead.
    #[test]
    fn adopt_child_falls_through_to_a_direct_clone_when_already_registered_and_nothing_local() {
        let (bare_dir, _branch) = seed_bare_repo();
        let clone_url = bare_dir.path().to_string_lossy().to_string();
        let scratch = TempDir::new().unwrap();
        let children_root = TempDir::new().unwrap();
        let data_dir = TempDir::new().unwrap();

        let script = fake_lgs_script(
            scratch.path(),
            &format!(
                "case \"$1\" in\n  restore) echo \"project 'allowance-keiko_hart' already exists\" >&2; exit 1 ;;\n  status) cat <<'JSON'\n{}\nJSON\n  ;;\nesac\n",
                status_json("allowance-keiko_hart", &clone_url)
            ),
        );
        let paths = onboarding_paths(
            scratch.path(),
            children_root.path().to_path_buf(),
            data_dir.path().to_path_buf(),
            script,
        );
        let lgs = LgsClient::new(paths.lgs_binary.clone());

        adopt_child(&lgs, "allowance-keiko_hart", &paths).unwrap();

        let new_dir = children_root.path().join("keiko_hart");
        assert!(new_dir.join("child.yaml").exists(), "the direct clone must have landed the working tree");
        let repo = git2::Repository::open(&new_dir).unwrap();
        assert!(repo.find_remote("lgs").is_ok());

        let registry = ChildRegistry::load(data_dir.path()).unwrap();
        assert_eq!(registry.path_for(&ChildId::from("keiko_hart")), Some(new_dir.as_path()));
    }

    /// Same refusal, but this machine already has a working copy at the
    /// target (a prior partial onboarding attempt, or a hand-made clone) —
    /// `adopt_child` must fetch rather than clone over it, and must not
    /// disturb the registry entry if one already exists.
    #[test]
    fn adopt_child_falls_through_to_a_fetch_when_already_registered_and_a_local_copy_exists() {
        let (bare_dir, _branch) = seed_bare_repo();
        let clone_url = bare_dir.path().to_string_lossy().to_string();
        let scratch = TempDir::new().unwrap();
        let children_root = TempDir::new().unwrap();
        let data_dir = TempDir::new().unwrap();

        let new_dir = children_root.path().join("keiko_hart");
        clone_repo(&clone_url, &new_dir).unwrap();
        {
            let mut registry = ChildRegistry::load(data_dir.path()).unwrap();
            registry
                .register(RegistryEntry {
                    id: ChildId::from("keiko_hart"),
                    path: new_dir.clone(),
                    label: "Keiko Hart".to_string(),
                })
                .unwrap();
            registry.save(data_dir.path()).unwrap();
        }

        let script = fake_lgs_script(
            scratch.path(),
            &format!(
                "case \"$1\" in\n  restore) echo \"project 'allowance-keiko_hart' already exists\" >&2; exit 1 ;;\n  status) cat <<'JSON'\n{}\nJSON\n  ;;\nesac\n",
                status_json("allowance-keiko_hart", &clone_url)
            ),
        );
        let paths = onboarding_paths(
            scratch.path(),
            children_root.path().to_path_buf(),
            data_dir.path().to_path_buf(),
            script,
        );
        let lgs = LgsClient::new(paths.lgs_binary.clone());

        adopt_child(&lgs, "allowance-keiko_hart", &paths).unwrap();

        let repo = git2::Repository::open(&new_dir).unwrap();
        assert!(repo.find_remote("lgs").is_ok());

        let registry = ChildRegistry::load(data_dir.path()).unwrap();
        assert_eq!(
            registry.path_for(&ChildId::from("keiko_hart")),
            Some(new_dir.as_path()),
            "an existing registry entry must not be disturbed"
        );
    }

    /// A genuine restore failure (not the already-registered refusal) must
    /// surface as an error, not be silently swallowed.
    #[test]
    fn adopt_child_surfaces_a_genuine_restore_failure() {
        let scratch = TempDir::new().unwrap();
        let children_root = TempDir::new().unwrap();
        let data_dir = TempDir::new().unwrap();

        let script = fake_lgs_script(
            scratch.path(),
            "case \"$1\" in\n  restore) echo \"daemon unreachable\" >&2; exit 1 ;;\nesac\n",
        );
        let paths = onboarding_paths(
            scratch.path(),
            children_root.path().to_path_buf(),
            data_dir.path().to_path_buf(),
            script,
        );
        let lgs = LgsClient::new(paths.lgs_binary.clone());

        let err = adopt_child(&lgs, "allowance-keiko_hart", &paths).unwrap_err();
        assert!(err.to_string().contains("daemon unreachable"), "got: {err}");
    }
}
