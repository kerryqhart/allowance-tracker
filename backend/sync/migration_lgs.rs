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
//!   working exactly as it did before the migration was attempted — the old
//!   folder is the fallback for as long as it isn't deleted.
//! - **Adopt before init.** Two machines can both decide to migrate the same
//!   child on different days. If this machine's `lgs status` already shows
//!   the project as `adoptable` (created and pushed by the other machine),
//!   this module runs `lgs restore` and builds on top of that history
//!   instead of `git init`-ing an unrelated root. Without this check, the
//!   second machine's fresh history has no common ancestor with the first
//!   machine's, and a later merge treats the empty root as the common base —
//!   which resurrects any row the first machine had already deleted.
//!
//! Mirrors the plan/report/run split in
//! `backend::storage::csv::migration::{plan_migration, MigrationReport}`:
//! [`plan_lgs_migration`] only reads (`lgs status`'s already-fetched result,
//! nothing more) and decides; [`run_lgs_migration`] is the only thing that
//! writes, executes strictly in [`LgsMigrationPlan::steps`] order, and
//! produces an [`LgsMigrationReport`] that always says whether it failed and
//! where.
//!
//! `plan_lgs_migration` takes a slice of registry entries because that is the
//! shape the caller already holds (`ChildRegistry::entries()`), but this
//! module plans and runs one migration at a time — the one real install this
//! ships for has exactly one child. A future multi-child rollout would call
//! this once per child rather than teaching `Step` to carry per-child payload
//! for steps that don't need one.

use crate::backend::storage::csv::{ChildRegistry, RegistryEntry, REGISTRY_FILENAME};
use crate::backend::storage::git::{ensure_lgs_remote, push_lgs, GitManager};
use crate::backend::sync::child_sync::current_branch;
use crate::backend::sync::lgs_client::LgsClient;
use crate::backend::sync::paths::{is_cloud_synced, Reason, SyncPaths};
pub use crate::backend::sync::lgs_client::StatusReport;
use crate::backend::{NoticeSeverity, StartupNotice};
use anyhow::{Context, Result};
use shared::ChildId;
use std::fs;
use std::path::PathBuf;

/// Files this app owns and copies byte-for-byte during migration.
/// `transactions.csv` is deliberately NOT in this list — it is read, parsed,
/// and re-rendered by [`Step::Canonicalize`] instead of copied verbatim,
/// since the whole point of that step is to rewrite it canonically. This
/// mirrors (but does not import, across the frontend/backend boundary) the
/// `FILES_THIS_APP_OWNS` list in `egui-frontend/src/ui/app_coordinator.rs`;
/// keep the two in sync if either changes.
const DATA_FILES: &[&str] = &["child.yaml", "allowance_config.yaml", "goals.csv"];
const TRANSACTIONS_FILE: &str = "transactions.csv";

fn project_name(id: &ChildId) -> String {
    format!("allowance-{}", id.as_str())
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
    /// Nothing adoptable exists yet: `git init` a fresh root, to be `lgs
    /// add`ed below.
    InitAndPush { name: String },
    /// Copy `child.yaml`, `allowance_config.yaml`, `goals.csv` verbatim.
    /// Never copies `.git`.
    CopyDataFiles,
    /// Read `old_dir/transactions.csv` through the canonical codec and write
    /// it back out through `render_transactions` — canonical row order and
    /// two-decimal money, at the new location.
    Canonicalize,
    /// Commit the copied + canonicalized files. A no-op (not an error) if
    /// staging produced no change.
    Commit,
    /// Register the fresh repo with lgs. Only present when the plan's first
    /// step is [`Step::InitAndPush`] — an adopted project is already
    /// registered.
    LgsAdd,
    /// Point the repo's `lgs` remote at the right URL: read from the
    /// restored repo's `origin` when adopting, or resolved via a fresh `lgs
    /// status` lookup (the URL is assigned by the daemon and cannot be
    /// known before `lgs add` runs) when this is a fresh project.
    EnsureLgsRemote,
    /// Push the current branch to the `lgs` remote.
    Push,
    /// Repoint `children.yaml` at the new folder. Always last: see the
    /// module doc comment.
    RepointRegistry,
}

/// The decided, not-yet-executed shape of one child's migration.
///
/// Carries everything [`run_lgs_migration`] needs to execute — including the
/// path to the (possibly fake, in tests) `lgs` binary — so that function's
/// signature can stay `run_lgs_migration(plan) -> LgsMigrationReport` with no
/// extra collaborators to thread through.
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
    /// `Some` when [`is_cloud_synced`] rejected `new_dir`. When set, `steps`
    /// is empty and [`run_lgs_migration`] does nothing but report the
    /// refusal — this is the safety gate the module doc comment describes,
    /// and it fires before any decision about adopting vs. initializing.
    pub blocked: Option<Reason>,
}

/// Outcome of [`run_lgs_migration`]. Always tells the caller whether it
/// failed and, if so, at which step — never leaves that to be inferred from
/// a bare `Result`, because "which step" is exactly what determines whether
/// the old folder and the registry are still intact (answer: always yes,
/// but the report says why nothing further happened).
#[derive(Debug, Default)]
pub struct LgsMigrationReport {
    pub completed_steps: Vec<Step>,
    pub failed_step: Option<Step>,
    pub error: Option<String>,
    /// How many transaction rows needed legacy f64-precision rounding
    /// (`Money::parse_rounding`) while being canonicalized. The rewrite is
    /// correct, not a value change — but the user is financially affected
    /// data and must be told, not left to notice a diff. See
    /// `ParsedTransactions::rows_rounded`.
    pub legacy_precision_rows_rounded: usize,
    /// User-visible outcomes. On success, always names the old folder's
    /// path so the user knows it is being kept as a backup, not lost.
    pub notices: Vec<StartupNotice>,
}

impl LgsMigrationReport {
    pub fn failed(&self) -> bool {
        self.failed_step.is_some() || self.error.is_some()
    }
}

/// Decide what a migration for the first entry in `children` would do.
/// Reads only — `status` is the caller's already-fetched `lgs status
/// --json`, and this function performs no I/O of its own beyond the cheap,
/// best-effort `~/Documents` symlink check `is_cloud_synced` needs (see its
/// doc comment for why that observation is the caller's job, not that
/// function's).
pub fn plan_lgs_migration(
    children: &[RegistryEntry],
    paths: &SyncPaths,
    status: &StatusReport,
) -> LgsMigrationPlan {
    let entry = children
        .first()
        .expect("plan_lgs_migration requires at least one child entry");

    let child_id = entry.id.clone();
    let name = project_name(&child_id);
    let old_dir = entry.path.clone();
    let new_dir = paths.children_root.join(child_id.as_str());

    // The absolute safety gate: refuse a target this migration's own guard
    // would flag, before deciding anything else about adopt-vs-init.
    let documents_is_symlink = fs::symlink_metadata(paths.home.join("Documents"))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if let Some(reason) = is_cloud_synced(&new_dir, paths, documents_is_symlink) {
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

    let adopt = status.adoptable.iter().any(|n| n == &name);

    let mut steps = Vec::new();
    if adopt {
        steps.push(Step::RestoreExisting { name: name.clone() });
    } else {
        steps.push(Step::InitAndPush { name: name.clone() });
    }
    steps.push(Step::CopyDataFiles);
    steps.push(Step::Canonicalize);
    steps.push(Step::Commit);
    if !adopt {
        // An adopted project is already registered with lgs; only a fresh
        // one needs `lgs add`.
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

/// Execute `plan`'s steps strictly in order, stopping at the first failure.
///
/// Every step before [`Step::RepointRegistry`] only touches `new_dir` (a
/// brand-new location) or reads `old_dir`. `children.yaml` is never opened
/// for writing until every earlier step has already succeeded — so a
/// failure anywhere leaves both the registry and the old folder exactly as
/// they were.
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
        let outcome = match &step {
            Step::RestoreExisting { name } => run_restore_existing(&plan, &lgs, name),
            Step::InitAndPush { .. } => run_init(&plan, &git),
            Step::CopyDataFiles => run_copy_data_files(&plan),
            Step::Canonicalize => run_canonicalize(&plan).map(|rows_rounded| {
                report.legacy_precision_rows_rounded = rows_rounded;
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
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
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
    git.init_repo(&plan.new_dir)
}

fn run_copy_data_files(plan: &LgsMigrationPlan) -> Result<()> {
    for name in DATA_FILES {
        let src = plan.old_dir.join(name);
        if !src.exists() {
            // Not every child has every file yet (e.g. no goal was ever
            // created) — absence is not an error.
            continue;
        }
        let dst = plan.new_dir.join(name);
        fs::copy(&src, &dst)
            .with_context(|| format!("copying {} to {}", src.display(), dst.display()))?;
    }
    Ok(())
}

fn run_canonicalize(plan: &LgsMigrationPlan) -> Result<usize> {
    let src = plan.old_dir.join(TRANSACTIONS_FILE);
    let text = if src.exists() {
        fs::read_to_string(&src).with_context(|| format!("reading {}", src.display()))?
    } else {
        String::new()
    };

    let parsed = allowance_core::codec::parse_transactions(&text)
        .with_context(|| format!("parsing {}", src.display()))?;
    let rendered = allowance_core::codec::render_transactions(&parsed.rows);

    let dst = plan.new_dir.join(TRANSACTIONS_FILE);
    fs::write(&dst, rendered).with_context(|| format!("writing {}", dst.display()))?;

    Ok(parsed.rows_rounded)
}

fn run_commit(plan: &LgsMigrationPlan, git: &GitManager) -> Result<()> {
    git.add_all(&plan.new_dir)?;
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
        // `lgs restore` clones with the remote named `origin`; that URL is
        // already correct, no need to ask lgs again.
        repo.find_remote("origin")
            .context("the restored repo has no `origin` remote")?
            .url()
            .context("the restored repo's `origin` remote has no URL")?
            .to_string()
    } else {
        // The URL for a brand-new project is assigned by the daemon and
        // cannot be known before `lgs add` ran — a fresh `lgs status` call
        // is the only way to learn it.
        let fresh_status = lgs
            .status()
            .context("querying `lgs status` to resolve the new project's clone_url")?;
        fresh_status
            .project(&plan.project_name)
            .with_context(|| {
                format!(
                    "`lgs add` succeeded but '{}' did not appear in `lgs status`",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::sync::lgs_client::DaemonInfo;
    use std::path::Path;
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
            adoptable: names.iter().map(|s| s.to_string()).collect(),
        }
    }

    // --- The four tests specified in the task brief, verbatim. ---

    #[test]
    fn adopts_an_existing_project_instead_of_creating_an_unrelated_root() {
        // A migrates Monday, B on Friday. Without this check B does its own
        // `git init`, the histories are unrelated, and any row A deleted in
        // that window resurrects through the empty-base union.
        let status = status_with_adoptable(&["allowance-keiko_hart"]);
        let plan = plan_lgs_migration(&[child("keiko_hart")], &paths(), &status);
        assert_eq!(plan.steps[0], Step::RestoreExisting { name: "allowance-keiko_hart".into() });
    }

    #[test]
    fn creates_a_fresh_repo_when_nothing_is_adoptable() {
        let plan = plan_lgs_migration(&[child("keiko_hart")], &paths(), &status_with_adoptable(&[]));
        assert_eq!(plan.steps[0], Step::InitAndPush { name: "allowance-keiko_hart".into() });
    }

    #[test]
    fn registry_is_repointed_only_after_every_other_step_succeeds() {
        let plan = plan_lgs_migration(&[child("keiko_hart")], &paths(), &status_with_adoptable(&[]));
        assert_eq!(*plan.steps.last().unwrap(), Step::RepointRegistry);
    }

    #[test]
    fn a_failure_leaves_the_registry_and_the_old_folder_untouched() {
        let env = TestEnvironment::new().unwrap();
        let before = fs::read_to_string(env.registry_path()).unwrap();
        let report = run_lgs_migration(plan_that_fails_at_push(&env));
        assert!(report.failed());
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before);
        assert!(env.old_child_dir().exists(), "the old folder is never deleted");
    }

    // --- Supporting fixture for the execution-level tests. ---

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
            // One row carries legacy f64-precision noise, matching what was
            // actually found in the real transactions.csv this migration is
            // written for.
            fs::write(
                old_child_dir.join("transactions.csv"),
                "id,child_id,date,description,amount,balance,type\n\
                 tx-1,keiko_hart,2024-01-01T00:00:00Z,Allowance,5.00,14.620000000000001,allowance\n",
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
        /// (so the `~/Documents` symlink probe just resolves to `false`) and
        /// is nowhere near iCloud, so it never trips the cloud-sync guard on
        /// its own.
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

    /// A fake `lgs status --json` response naming one project with the given
    /// `clone_url`. `add` always succeeds.
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
        plan_lgs_migration(&[env.entry()], &paths, &status_with_adoptable(&[]))
    }

    #[test]
    fn a_failure_at_lgs_add_leaves_the_registry_and_the_old_folder_untouched() {
        let env = TestEnvironment::new().unwrap();
        let script = fake_lgs_script(
            env.scratch_dir(),
            "case \"$1\" in\n  add) echo boom >&2; exit 1 ;;\nesac\n",
        );
        let paths = env.paths(script);
        let plan = plan_lgs_migration(&[env.entry()], &paths, &status_with_adoptable(&[]));

        let before = fs::read_to_string(env.registry_path()).unwrap();
        let report = run_lgs_migration(plan);

        assert!(report.failed());
        assert_eq!(report.failed_step, Some(Step::LgsAdd));
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before);
        assert!(env.old_child_dir().exists());
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

        let plan = plan_lgs_migration(&[env.entry()], &paths, &status_with_adoptable(&[]));
        let before = fs::read_to_string(env.registry_path()).unwrap();
        let report = run_lgs_migration(plan);

        assert!(report.failed());
        assert_eq!(
            report.failed_step,
            Some(Step::InitAndPush { name: "allowance-keiko_hart".into() })
        );
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before);
        assert!(env.old_child_dir().exists());
    }

    #[test]
    fn refuses_a_target_inside_a_cloud_synced_path() {
        let env = TestEnvironment::new().unwrap();
        let cloud_root = TempDir::new().unwrap();
        let mut paths = env.paths(PathBuf::from("/fake/bin/lgs-not-used"));
        paths.children_root = cloud_root.path().join("children");
        paths.cloud_root = Some(cloud_root.path().to_path_buf());

        let plan = plan_lgs_migration(&[env.entry()], &paths, &status_with_adoptable(&[]));
        assert!(plan.steps.is_empty(), "a refused target must plan no steps to run");
        assert_eq!(plan.blocked, Some(Reason::InsideCloudRoot));

        let before = fs::read_to_string(env.registry_path()).unwrap();
        let report = run_lgs_migration(plan);

        assert!(report.failed());
        assert_eq!(fs::read_to_string(env.registry_path()).unwrap(), before);
        assert!(env.old_child_dir().exists());
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
        let plan = plan_lgs_migration(&[env.entry()], &paths, &status_with_adoptable(&[]));

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

        // The registry now points at the new folder — only reachable
        // because RepointRegistry was the last step to run, i.e. every
        // earlier step succeeded.
        let registry = ChildRegistry::load(&env.base_dir()).unwrap();
        assert_eq!(registry.path_for(&ChildId::from("keiko_hart")), Some(new_dir.as_path()));

        // The canonicalized file at the new location has real, 2-decimal
        // money, not the legacy f64 noise.
        let migrated = fs::read_to_string(new_dir.join("transactions.csv")).unwrap();
        assert!(migrated.contains("14.62"), "got: {migrated}");
        assert!(!migrated.contains("14.620000000000001"), "got: {migrated}");

        // The old folder is untouched — still there, still holding the
        // original (unrounded) bytes.
        assert!(env.old_child_dir().exists());
        let original = fs::read_to_string(env.old_child_dir().join("transactions.csv")).unwrap();
        assert!(original.contains("14.620000000000001"));
    }
}
