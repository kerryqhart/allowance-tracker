use crate::backend::sync::lgs_client::{DaemonState, LgsClient};
use crate::backend::sync::paths::SyncPaths;
use anyhow::{Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// lgs shells out to `git` for every repository operation, and spawns
/// `git http-backend` to serve the remote. Bundling lgs does NOT remove this
/// dependency — on a Mac without Xcode Command Line Tools, /usr/bin/git is a
/// stub that opens a dialog and exits non-zero.
pub fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub const GIT_MISSING_MESSAGE: &str = "Sync needs Apple's Command Line Tools, which include git. \
Open Terminal and run: xcode-select --install";

pub fn copy_binary(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {parent:?}"))?;
    }
    // Write to a temp name in the SAME directory as the destination (not a
    // system temp dir), so the rename below is same-filesystem and therefore
    // atomic — a crash mid-copy cannot leave a truncated binary that launchd
    // would happily keep executing.
    let tmp = dst.with_extension("tmp");
    std::fs::copy(src, &tmp).with_context(|| format!("copying {src:?} -> {tmp:?}"))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
        .with_context(|| format!("setting permissions on {tmp:?}"))?;
    // If the rename fails, don't leave the `.tmp` staging file behind — but
    // keep the rename's own error as the reported cause, not a cleanup error.
    std::fs::rename(&tmp, dst)
        .map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            err
        })
        .with_context(|| format!("renaming {tmp:?} into {dst:?}"))?;
    Ok(())
}

/// Resolve the bundled binary and copy it to the stable path.
pub fn ensure_lgs_binary(env: &SyncPaths) -> Result<PathBuf> {
    let bundled = bundled_lgs_path()?;
    copy_binary(&bundled, &env.lgs_binary)?;
    Ok(env.lgs_binary.clone())
}

/// Where the bundled `lgs` binary might be, checked in order.
///
/// Exactly where `cargo-bundle` places a `resources` entry that lives
/// outside the crate directory (like `../target/release/lgs`) was not
/// verified against a real produced `.app` — if `cargo-bundle` strips
/// resource paths relative to a common ancestor instead of flattening them,
/// `lgs` could land at `Contents/Resources/target/release/lgs` rather than
/// the flat `Contents/Resources/lgs` a single-candidate resolver would
/// assume. Rather than bet on one layout, this checks several plausible
/// ones, so a packaging surprise degrades to "found it in a different spot"
/// instead of "release builds are silently broken while every test stays
/// green".
fn bundled_lgs_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        // .app/Contents/MacOS/<exe> -> .app/Contents/Resources/lgs
        exe_dir.join("../Resources/lgs"),
        // In case cargo-bundle preserves the `target/release/` portion of a
        // resource path that lives outside the crate directory.
        exe_dir.join("../Resources/target/release/lgs"),
        // Dev builds: target/<profile>/lgs, produced by build.rs next to the
        // binary itself.
        exe_dir.join("lgs"),
    ]
}

fn bundled_lgs_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolving current exe")?;
    let exe_dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let candidates = bundled_lgs_candidates(exe_dir);

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    let tried = candidates
        .iter()
        .map(|p| format!("{p:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!("bundled lgs binary not found; tried: {tried}")
}

/// Whether this Mac's `lgs` daemon is one the app installed, or one it found
/// already running and adopted.
///
/// This is the record that keeps "adopt, don't reinstall" from becoming a
/// trap: the bundled `lgs` CLI advances with every app release, an adopted
/// daemon never does, so `outdated` would become the permanent steady state
/// for it — and because the app refuses to report a project as backed up
/// while `outdated` holds (see [`crate::backend::sync::lgs_client::StatusReport::durability_data_is_fresh`]),
/// the app would never report a project as backed up again. Recording
/// ownership is what lets the app upgrade a daemon it installed while never
/// touching one it did not.
///
/// `#[serde(default)]` on every field using this type keeps an existing
/// `sync_state.yaml` (written before this field existed) loading as
/// `installed_by_app: false` — the safe default, since a daemon this app has
/// no memory of installing must be treated as adopted, not owned.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DaemonOwnership {
    pub installed_by_app: bool,
}

/// What to do about the daemon, decided from its reported state and whether
/// this app owns it.
#[derive(Debug, PartialEq, Eq)]
pub enum DaemonAction {
    /// Healthy; nothing to do.
    None,
    /// Outdated, but ours — restart it to pick up the bundled binary.
    Restart,
    /// Outdated, in error, or in an unrecognized state, and not ours (or we
    /// cannot tell) — tell the user rather than act on a daemon we don't
    /// manage.
    ReportSkew,
    /// No daemon running at all — install ours and take ownership.
    InstallAndOwn,
}

/// Only a daemon this app installed should ever be upgraded.
pub fn should_upgrade(o: &DaemonOwnership) -> bool {
    o.installed_by_app
}

/// Decide what to do about the daemon from its reported state and ownership.
///
/// `Down` always installs — there is nothing running to disturb. `Outdated`
/// is the case ownership exists for: restart it if we installed it, report
/// skew otherwise. `Error` and `Unknown` both route to `ReportSkew` — when we
/// cannot tell what is going on, we report rather than act.
pub fn plan_daemon_action(state: DaemonState, owner: &DaemonOwnership) -> DaemonAction {
    match state {
        DaemonState::Ok => DaemonAction::None,
        DaemonState::Down => DaemonAction::InstallAndOwn,
        // Someone else's daemon is not ours to restart or overwrite.
        DaemonState::Outdated if owner.installed_by_app => DaemonAction::Restart,
        DaemonState::Outdated => DaemonAction::ReportSkew,
        DaemonState::Error | DaemonState::Unknown => DaemonAction::ReportSkew,
    }
}

/// What happened when [`ensure_daemon`] resolved the daemon's state.
#[derive(Debug, PartialEq, Eq)]
pub enum DaemonOutcome {
    /// Already `Ok`; nothing was done.
    Healthy,
    /// Was outdated and ours; restarted to pick up the bundled binary.
    Restarted,
    /// Nothing was running; installed and took ownership.
    InstalledAndOwned,
    /// Outdated/errored/unrecognized and not ours (or we cannot tell) — the
    /// daemon's own message, relayed verbatim per the design constraint that
    /// an `outdated` daemon's `message` is never re-worded.
    Skewed(String),
}

/// Composition point for [`plan_daemon_action`] + [`install_and_start`]:
/// query the daemon's current state, decide what (if anything) to do about
/// it, and do it — never reinstalling or repointing a daemon this app did
/// not install (see [`plan_daemon_action`]'s doc comment).
///
/// Callers that get back [`DaemonOutcome::InstalledAndOwned`] must persist
/// `installed_by_app: true` into their `DaemonOwnership` record — this
/// function only decides and acts, it does not own persistence.
///
/// Not exercised in this module's own tests beyond the state-decision table
/// already covered by [`plan_daemon_action`]'s tests: the `Restart` and
/// `InstallAndOwn` branches call [`install_and_start`], which runs real
/// `launchctl` commands against the single system-wide
/// `com.local-git-sync.daemon` — invoking that from an automated test would
/// risk disturbing a real, already-running daemon backing up real projects,
/// which is exactly the hazard `install_and_start`'s own tests already avoid
/// for the same reason. Verify those two branches by hand against a
/// throwaway `HOME`.
pub fn ensure_daemon(lgs: &LgsClient, ownership: &DaemonOwnership) -> Result<DaemonOutcome> {
    let status = lgs.status().context("running `lgs status --json`")?;
    match plan_daemon_action(status.daemon.state, ownership) {
        DaemonAction::None => Ok(DaemonOutcome::Healthy),
        DaemonAction::Restart => {
            install_and_start(lgs)?;
            Ok(DaemonOutcome::Restarted)
        }
        DaemonAction::InstallAndOwn => {
            install_and_start(lgs)?;
            Ok(DaemonOutcome::InstalledAndOwned)
        }
        DaemonAction::ReportSkew => Ok(DaemonOutcome::Skewed(status.daemon.message)),
    }
}

/// The first-run sequence's gate: `lgs` shells out to `git` for every
/// repository operation (init, restore, commit, push, fetch — all of it),
/// so bundling the `lgs` binary does not remove the dependency on a working
/// `git`. Checked and refused FIRST, before anything else in first run
/// touches disk or the daemon — a missing git surfacing instead as an
/// opaque failure three steps into `lgs init` would leave a user with no
/// idea what to fix. `git_available` is injected (rather than this function
/// calling [`git_is_available`] itself) so a test can drive the refusal
/// path without needing to actually uninstall git.
///
/// On success, runs `lgs init --cloud-root <cloud_root>` — the one step of
/// first run that is safe to unit-test against a fake `lgs` binary, since it
/// has no real side effects beyond the (fake) process call itself. The
/// daemon adopt-or-install step and child registration are separate,
/// later steps of first run (see [`ensure_daemon`] and
/// `crate::backend::sync::migration_lgs::adopt_child`), deliberately not
/// folded into this function so a git-missing refusal can never have
/// already run `lgs init` by the time the caller sees it.
pub fn run_first_run(lgs: &LgsClient, cloud_root: &Path, git_available: bool) -> Result<()> {
    anyhow::ensure!(git_available, "{GIT_MISSING_MESSAGE}");
    let cloud_root_str = cloud_root
        .to_str()
        .context("the cloud root path is not valid UTF-8")?;
    lgs.init(cloud_root_str).context("running `lgs init --cloud-root`")
}

/// How long a single `id`/`launchctl` call is given before it's treated as
/// hung and killed.
///
/// Review Important-4: these used to run via bare `Command::output()`, which
/// has no bound at all — and `install_and_start` is reachable from
/// `AllowanceTrackerApp::new`, on the main thread, before the first frame
/// ever renders. A wedged `launchctl` (stuck talking to a launchd that is
/// itself unhealthy, which is not a hypothetical on a machine whose own
/// daemon is exactly what this call is trying to fix) would hang the app on
/// launch with a blank window and no way for the user to even see what's
/// wrong. 10s matches [`crate::backend::sync::lgs_client::LgsClient::run`]'s
/// own bound for the same class of call — a local process talking to a
/// local daemon/launchd, never a network round-trip.
const SYSTEM_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Run `cmd`, killing it and returning an error if it does not complete
/// within `timeout`. The same spawn+poll+kill technique
/// `LgsClient::run_with_timeout` already uses for the same reason:
/// `Command::output()` blocks with no built-in bound, and reading
/// stdout/stderr only after the child exits risks the classic pipe deadlock,
/// so both pipes are drained on their own threads concurrently with the wait
/// loop.
fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<std::process::Output> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {cmd:?}"))?;

    let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().with_context(|| format!("waiting for {cmd:?}"))? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!(
                    "{cmd:?} did not complete within {timeout:?} and was killed — it may be hung"
                );
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };

    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    Ok(std::process::Output { status, stdout, stderr })
}

/// `lgs install-service` writes the plist and then PRINTS the launchctl
/// commands for a human to run — nothing loads and nothing starts until the
/// next login. We run them ourselves, or first run hands a non-technical user
/// a plist, no daemon, no clone URL, and a terminal command as the remedy.
pub fn install_and_start(lgs: &LgsClient) -> Result<()> {
    lgs.run(&["install-service"])?;
    let uid = run_command_with_timeout(Command::new("id").arg("-u"), SYSTEM_COMMAND_TIMEOUT)
        .context("running id -u")?;
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let plist = dirs::home_dir()
        .unwrap_or_default()
        .join("Library/LaunchAgents/com.local-git-sync.daemon.plist");
    let _ = run_command_with_timeout(
        Command::new("launchctl").args(["bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()]),
        SYSTEM_COMMAND_TIMEOUT,
    );
    let st = run_command_with_timeout(
        Command::new("launchctl").args(["kickstart", &format!("gui/{uid}/com.local-git-sync.daemon")]),
        SYSTEM_COMMAND_TIMEOUT,
    )
    .context("running launchctl kickstart")?;
    anyhow::ensure!(
        st.status.success(),
        "launchctl kickstart failed: {}",
        String::from_utf8_lossy(&st.stderr)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn copies_the_binary_out_of_the_bundle_and_is_idempotent() {
        // The plist points at whatever path we install from
        // (lgs service.rs uses current_exe()), so it must be a stable location
        // outside the .app — otherwise moving the app to /Applications leaves
        // launchd retrying a missing path forever under KeepAlive.
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"#!/bin/sh\nexit 0\n").unwrap();

        let dst = dst_dir.path().join("bin").join("lgs");
        copy_binary(&src, &dst).unwrap();
        assert!(dst.exists());

        copy_binary(&src, &dst).unwrap();
        assert!(dst.exists(), "re-running must not fail or corrupt the target");
    }

    #[test]
    fn copied_binary_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"x").unwrap();
        let dst = dst_dir.path().join("lgs");
        copy_binary(&src, &dst).unwrap();
        let mode = std::fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "must be executable by owner/group/other");
    }

    /// A stale binary surviving an app update is the exact failure mode this
    /// design exists to avoid: the destination must end up with the NEW
    /// bytes, not silently keep whatever was there before.
    #[test]
    fn copy_replaces_an_existing_older_binary() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"new version bytes").unwrap();

        let dst = dst_dir.path().join("lgs");
        std::fs::write(&dst, b"old stale version").unwrap();

        copy_binary(&src, &dst).unwrap();

        let contents = std::fs::read(&dst).unwrap();
        assert_eq!(
            contents, b"new version bytes",
            "the old binary must be replaced, not kept"
        );
    }

    /// The destination's parent directory (e.g. the `bin/` in `.../bin/lgs`)
    /// does not exist yet on a first run — `ensure_lgs_binary` must create it
    /// rather than fail.
    #[test]
    fn copy_creates_missing_parent_directory() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"binary").unwrap();

        let dst = dst_dir.path().join("does").join("not").join("exist").join("lgs");
        assert!(!dst.parent().unwrap().exists());

        copy_binary(&src, &dst).unwrap();

        assert!(dst.exists());
    }

    /// The atomic write-then-rename must not leave its `.tmp` staging file
    /// behind after a successful copy — the destination directory should
    /// contain only the final binary.
    #[test]
    fn no_temp_artifact_is_left_behind_after_a_successful_copy() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"binary").unwrap();

        let dst = dst_dir.path().join("bin").join("lgs");
        copy_binary(&src, &dst).unwrap();

        let entries: Vec<_> = std::fs::read_dir(dst.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            entries,
            vec![std::ffi::OsString::from("lgs")],
            "no temp artifact should remain: {entries:?}"
        );
    }

    /// This asserts the local dev environment (git is present on this
    /// machine) rather than simulating git's absence — that would require
    /// manipulating PATH, out of scope here. The point is still to fail if
    /// the function ever panics instead of returning a plain bool, e.g. on a
    /// broken pipe or an unexpected exit status.
    #[test]
    fn git_is_available_does_not_panic() {
        let available: bool = git_is_available();
        assert!(available, "git is expected to be present on this dev machine");
    }

    // --- Review Important-4: `run_command_with_timeout` — the same bound
    // `LgsClient::run` already has, now covering the `id`/`launchctl` calls
    // `install_and_start` makes on the main thread before the first frame.

    #[test]
    fn run_command_with_timeout_kills_a_hung_process_instead_of_blocking_forever() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 5; echo too-late"]);
        let start = Instant::now();
        let result = run_command_with_timeout(&mut cmd, Duration::from_millis(200));
        let elapsed = start.elapsed();

        let err = result.expect_err("a hung process must be reported as an error, not hang the caller");
        assert!(
            format!("{err}").contains("did not complete"),
            "error must say the process was killed for timing out: {err}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "must return close to the 200ms bound, not wait out the process's full 5s sleep: \
             took {elapsed:?}"
        );
    }

    #[test]
    fn run_command_with_timeout_captures_output_for_a_well_behaved_process() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo hello"]);
        let output = run_command_with_timeout(&mut cmd, Duration::from_secs(5)).unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
    }

    /// A failed rename must not leave the `.tmp` staging file behind. Force
    /// the rename to fail by making `dst`'s parent a location where `dst`
    /// itself is a directory — `rename(tmp_file, existing_dir)` fails on
    /// every POSIX system — then check the parent holds nothing named after
    /// the temp file's extension.
    #[test]
    fn failed_rename_does_not_leave_a_temp_artifact_behind() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"binary").unwrap();

        // `dst` is itself a directory, so `fs::rename(tmp, dst)` fails
        // (cannot rename a file onto a directory).
        let dst = dst_dir.path().join("lgs");
        std::fs::create_dir(&dst).unwrap();

        let result = copy_binary(&src, &dst);
        assert!(result.is_err(), "renaming a file onto a directory must fail");

        let tmp = dst.with_extension("tmp");
        assert!(!tmp.exists(), "the .tmp staging file must be cleaned up on failure");
    }

    /// If none of the candidate locations have `lgs`, the error must name
    /// every path tried — a packaging mistake (e.g. cargo-bundle placing the
    /// binary somewhere other than the flat `Contents/Resources/lgs` this
    /// resolver expects) must be diagnosable from the message alone.
    #[test]
    fn bundled_lgs_missing_error_names_every_candidate_tried() {
        let exe = std::env::current_exe().unwrap();
        let exe_dir = exe.parent().unwrap().to_path_buf();
        let expected_candidates = bundled_lgs_candidates(&exe_dir);

        // None of the candidates exist under the test binary's own directory
        // (target/debug/deps/...), so this exercises the real not-found path
        // rather than a synthetic one.
        for c in &expected_candidates {
            assert!(!c.exists(), "test assumption violated: {c:?} unexpectedly exists");
        }

        let err = bundled_lgs_path().expect_err("no candidate should exist here");
        let message = format!("{err}");
        for c in &expected_candidates {
            let formatted = format!("{c:?}");
            assert!(
                message.contains(&formatted),
                "error message must name {formatted}; got: {message}"
            );
        }
    }

    #[test]
    fn an_adopted_daemon_is_never_upgraded() {
        // "Adopt, don't reinstall" alone strands the daemon: the bundled CLI
        // advances every release, `outdated` becomes permanent, and because we
        // refuse to claim backed-up while outdated holds, the app would never
        // report a project as backed up again. So we only upgrade what we own.
        let mut ownership = DaemonOwnership { installed_by_app: false };
        assert!(!should_upgrade(&ownership));
        ownership.installed_by_app = true;
        assert!(should_upgrade(&ownership));
    }

    #[test]
    fn an_adopted_daemon_below_the_floor_is_reported_not_replaced() {
        let action = plan_daemon_action(DaemonState::Outdated, &DaemonOwnership { installed_by_app: false });
        assert_eq!(action, DaemonAction::ReportSkew);
    }

    #[test]
    fn our_own_outdated_daemon_gets_restarted() {
        let action = plan_daemon_action(DaemonState::Outdated, &DaemonOwnership { installed_by_app: true });
        assert_eq!(action, DaemonAction::Restart);
    }

    #[test]
    fn no_daemon_means_install_and_take_ownership() {
        let action = plan_daemon_action(DaemonState::Down, &DaemonOwnership { installed_by_app: false });
        assert_eq!(action, DaemonAction::InstallAndOwn);
    }

    /// The full decision table: all five [`DaemonState`] variants crossed with
    /// both ownership values. This is a decision table, not a handful of
    /// spot checks, and deserves to be tested as one — every cell pinned so a
    /// future edit to `plan_daemon_action`'s `match` cannot silently change a
    /// cell nothing else here happens to exercise.
    #[test]
    fn daemon_action_decision_table_covers_every_state_and_ownership() {
        let owned = DaemonOwnership { installed_by_app: true };
        let adopted = DaemonOwnership { installed_by_app: false };

        let cases: Vec<(DaemonState, &DaemonOwnership, DaemonAction)> = vec![
            // state              owner     expected action
            (DaemonState::Ok, &owned, DaemonAction::None),
            (DaemonState::Ok, &adopted, DaemonAction::None),
            (DaemonState::Down, &owned, DaemonAction::InstallAndOwn),
            (DaemonState::Down, &adopted, DaemonAction::InstallAndOwn),
            (DaemonState::Outdated, &owned, DaemonAction::Restart),
            (DaemonState::Outdated, &adopted, DaemonAction::ReportSkew),
            (DaemonState::Error, &owned, DaemonAction::ReportSkew),
            (DaemonState::Error, &adopted, DaemonAction::ReportSkew),
            (DaemonState::Unknown, &owned, DaemonAction::ReportSkew),
            (DaemonState::Unknown, &adopted, DaemonAction::ReportSkew),
        ];

        assert_eq!(cases.len(), 10, "must cover all 5 states x 2 ownership values");

        for (state, owner, expected) in cases {
            let actual = plan_daemon_action(state, owner);
            assert_eq!(
                actual, expected,
                "state={state:?} installed_by_app={} expected={expected:?} actual={actual:?}",
                owner.installed_by_app
            );
        }
    }

    fn fake_lgs_script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("lgs");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    // --- `ensure_daemon`: only the branches that never touch a real daemon. ---
    // See `ensure_daemon`'s doc comment for why `Restart`/`InstallAndOwn` are
    // not exercised here.

    #[test]
    fn ensure_daemon_does_nothing_when_already_healthy() {
        let dir = TempDir::new().unwrap();
        let script = fake_lgs_script(
            dir.path(),
            "case \"$1\" in\n  status) echo '{\"daemon\":{\"state\":\"ok\"},\"projects\":[],\"adoptable\":[]}' ;;\nesac\n",
        );
        let lgs = LgsClient::new(script);
        let outcome = ensure_daemon(&lgs, &DaemonOwnership { installed_by_app: false }).unwrap();
        assert_eq!(outcome, DaemonOutcome::Healthy);
    }

    #[test]
    fn ensure_daemon_reports_skew_verbatim_for_an_adopted_outdated_daemon() {
        let dir = TempDir::new().unwrap();
        let script = fake_lgs_script(
            dir.path(),
            "case \"$1\" in\n  status) echo '{\"daemon\":{\"state\":\"outdated\",\"message\":\"restart the service to pick up the new binary\"},\"projects\":[],\"adoptable\":[]}' ;;\nesac\n",
        );
        let lgs = LgsClient::new(script);
        let outcome = ensure_daemon(&lgs, &DaemonOwnership { installed_by_app: false }).unwrap();
        assert_eq!(
            outcome,
            DaemonOutcome::Skewed("restart the service to pick up the new binary".to_string())
        );
    }

    // --- `run_first_run`: the git-availability gate. ---

    #[test]
    fn first_run_refuses_before_touching_lgs_when_git_is_absent() {
        // A binary path that does not exist: if `run_first_run` invoked it
        // before checking `git_available`, the failure would be a spawn
        // error naming this bogus path, not `GIT_MISSING_MESSAGE` — so this
        // also proves the gate runs FIRST, not merely that it can fail.
        let lgs = LgsClient::new(PathBuf::from("/nonexistent/lgs-should-not-be-invoked"));
        let cloud_root = PathBuf::from("/tmp/wherever");
        let err = run_first_run(&lgs, &cloud_root, false).unwrap_err();
        assert_eq!(err.to_string(), GIT_MISSING_MESSAGE);
    }

    #[test]
    fn first_run_runs_lgs_init_when_git_is_available() {
        let dir = TempDir::new().unwrap();
        let script = fake_lgs_script(dir.path(), "case \"$1\" in\n  init) exit 0 ;;\nesac\n");
        let lgs = LgsClient::new(script);
        let cloud_root = TempDir::new().unwrap();
        run_first_run(&lgs, cloud_root.path(), true).unwrap();
    }

    #[test]
    fn first_run_surfaces_a_genuine_lgs_init_failure() {
        let dir = TempDir::new().unwrap();
        let script = fake_lgs_script(
            dir.path(),
            "case \"$1\" in\n  init) echo 'cloud root is not empty' >&2; exit 1 ;;\nesac\n",
        );
        let lgs = LgsClient::new(script);
        let cloud_root = TempDir::new().unwrap();
        let err = run_first_run(&lgs, cloud_root.path(), true).unwrap_err();
        let full_message = err.chain().map(|c| c.to_string()).collect::<Vec<_>>().join(": ");
        assert!(
            full_message.contains("cloud root is not empty"),
            "got: {full_message}"
        );
    }
}
