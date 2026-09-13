//! Fixture captured from lgs at commit a2eb77f578d981b249e1cbc309a93dc51c7671d2.
//!
//! # Design
//!
//! No trait behind [`LgsClient`] — there is one implementation and one
//! caller, so a trait here would be an abstraction with no second
//! implementor. The testable seam is the pure [`parse_status`]; [`LgsClient`]
//! itself is a thin wrapper that shells out to the `lgs` binary and hands the
//! text to the parser.
//!
//! The status/durability enums below carry `#[serde(other)] Unknown`
//! catch-all variants. This is required, not defensive padding: lgs's own
//! test suite feeds a literal `"a_variant_from_the_future"` value through
//! this exact shape (see `local-git-sync/src/cli.rs` around line 1788).
//!
//! `DurabilityState` models all seven of lgs's current
//! `DurabilityHealth` wire states (`local-git-sync/src/durability/health.rs`),
//! not just the four originally guessed at — the real fixture captured for
//! this file already contains `"stranded"`, and `Stranded`,
//! `WorkingRepoUnreadable`, and `BareRepoUnreadable` are all `Severity::Red`
//! in lgs's own model. Leaving those three unmodeled would have made "your
//! data is not backed up anywhere else" indistinguishable from "lgs added a
//! state after we shipped" — exactly the distinction this app must not blur.
//! `#[serde(other)] Unknown` stays as the catch-all for values lgs adds
//! *after* this file was written; [`ProjectReport::is_confirmed_backed_up`]
//! treats `Unknown` the same as every Red state: not safe to report as
//! backed up.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurabilityState {
    /// The remote holds every branch pushed here.
    BackedUp,
    /// A publish is in flight, or the cloud drive is offline. Transient.
    Pending,
    /// Nothing on the remote covers this project.
    NotBackedUp,
    /// A branch moved here and on the remote independently.
    Diverged,
    /// Commits exist in the working repo that have never reached the bare —
    /// they exist on exactly one disk. Red in lgs's own model.
    Stranded,
    /// This machine's working repo could not be measured. Red.
    WorkingRepoUnreadable,
    /// This machine's own bare copy of the project could not be read. Red.
    BareRepoUnreadable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DaemonState {
    #[default]
    Ok,
    Down,
    Outdated,
    Error,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DaemonInfo {
    #[serde(default)]
    pub state: DaemonState,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ProjectReport {
    pub name: String,
    pub clone_url: String,
    pub working_repo_path: PathBuf,
    pub durability_state: DurabilityState,
    pub durability_label: Option<String>,
    pub failed_sync_attempts: usize,
    pub archived: bool,
}

impl ProjectReport {
    /// True only when lgs affirmatively says this project is backed up.
    /// Every other state — including `Unknown` — returns false: a state we
    /// cannot interpret must never be rendered as safe.
    ///
    /// Three of the states this module cannot interpret today (`Stranded`,
    /// `WorkingRepoUnreadable`, `BareRepoUnreadable`) are `Severity::Red` in
    /// lgs's own model, and `Unknown` covers states lgs adds after this file
    /// was written — which could be Red too. Defaulting an uninterpretable
    /// state to "safe" would be exactly the wrong direction, so this checks
    /// for the one state known to be safe rather than excluding the states
    /// known to be unsafe.
    pub fn is_confirmed_backed_up(&self) -> bool {
        matches!(self.durability_state, DurabilityState::BackedUp)
    }
}

/// One entry in `lgs status --json`'s `adoptable` list: a project this
/// machine has not registered, discovered by scanning the cloud root
/// directly (`local-git-sync`'s `discover_adoptable`).
///
/// `archived` and `note` mirror lgs's own `AdoptableEntry` shape
/// (`local-git-sync/src/ipc.rs`) — an archived project is still offered here
/// (never hidden), and `note` carries the human-written reason, when there is
/// one, so onboarding can label rather than silently skip it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AdoptableEntry {
    pub name: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StatusReport {
    pub daemon: DaemonInfo,
    pub cloud_root: Option<PathBuf>,
    pub cloud_root_exists: bool,
    pub projects: Vec<ProjectReport>,
    pub adoptable: Vec<AdoptableEntry>,
}

impl StatusReport {
    /// Whether the durability data attached to every project in this report
    /// is fresh enough to trust at all — NOT a per-project safety answer.
    ///
    /// This is report-wide: it says nothing about whether any *given*
    /// project is safe, only whether the daemon that produced these
    /// durability values was itself in a state where they can be trusted.
    /// When the daemon is `outdated` (or `down`/`error`/anything
    /// unrecognized), the durability values on each project were read from
    /// disk and may be stale. For "is this specific project backed up",
    /// use [`ProjectReport::is_confirmed_backed_up`] instead — do not use
    /// this method as a per-project stand-in for it.
    pub fn durability_data_is_fresh(&self) -> bool {
        matches!(self.daemon.state, DaemonState::Ok)
    }
    pub fn project(&self, name: &str) -> Option<&ProjectReport> {
        self.projects.iter().find(|p| p.name == name)
    }
}

pub fn parse_status(json: &str) -> Result<StatusReport> {
    #[derive(Deserialize)]
    struct RawDurability {
        state: DurabilityState,
    }
    #[derive(Deserialize)]
    struct RawProject {
        name: String,
        clone_url: String,
        working_repo_path: PathBuf,
        #[serde(default)]
        durability: Option<RawDurability>,
        #[serde(default)]
        durability_label: Option<String>,
        #[serde(default)]
        failed_sync_attempts: Option<usize>,
        #[serde(default)]
        archived: bool,
    }
    #[derive(Deserialize)]
    struct RawAdoptable {
        name: String,
        #[serde(default)]
        archived: bool,
        #[serde(default)]
        note: Option<String>,
    }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        daemon: DaemonInfo,
        #[serde(default)]
        cloud_root: Option<PathBuf>,
        #[serde(default)]
        cloud_root_exists: bool,
        #[serde(default)]
        projects: Vec<RawProject>,
        #[serde(default)]
        adoptable: Vec<RawAdoptable>,
    }

    let raw: Raw = serde_json::from_str(json).context("parsing `lgs status --json`")?;
    Ok(StatusReport {
        daemon: raw.daemon,
        cloud_root: raw.cloud_root,
        cloud_root_exists: raw.cloud_root_exists,
        projects: raw
            .projects
            .into_iter()
            .map(|p| ProjectReport {
                name: p.name,
                clone_url: p.clone_url,
                working_repo_path: p.working_repo_path,
                durability_state: p
                    .durability
                    .map(|d| d.state)
                    .unwrap_or(DurabilityState::Unknown),
                durability_label: p.durability_label,
                failed_sync_attempts: p.failed_sync_attempts.unwrap_or(0),
                archived: p.archived,
            })
            .collect(),
        adoptable: raw
            .adoptable
            .into_iter()
            .map(|a| AdoptableEntry { name: a.name, archived: a.archived, note: a.note })
            .collect(),
    })
}

/// How long [`LgsClient::run`] waits for `lgs` before killing it and
/// returning an error. Review Critical-2: `Command::output()` has no
/// built-in bound, so a hung daemon or a stalled local IPC call used to
/// block the call forever — and `run_child_sync_cycles` calls this
/// synchronously on the background sync thread, so an unbounded `run` would
/// have blocked the AWS poll, the 30s timer, AND shutdown, indefinitely.
/// 10 seconds is generous for what is always a local process talking to a
/// daemon on the same machine.
const LGS_RUN_TIMEOUT: Duration = Duration::from_secs(10);

pub struct LgsClient {
    binary: PathBuf,
}

impl LgsClient {
    pub fn new(binary: PathBuf) -> Self {
        Self { binary }
    }

    pub fn run(&self, args: &[&str]) -> Result<String> {
        self.run_with_timeout(args, LGS_RUN_TIMEOUT)
    }

    /// The actual implementation behind [`Self::run`], with the timeout
    /// exposed so tests can drive the expiry path in milliseconds instead
    /// of waiting out the real 10s bound.
    ///
    /// Spawns rather than uses `Command::output()` so a bound can be placed
    /// on the wait. stdout/stderr are drained on their own threads
    /// concurrently with the wait loop below — reading them only after the
    /// process exits would risk the classic pipe deadlock (the child blocks
    /// writing to a full OS pipe buffer; the parent blocks waiting for it to
    /// exit without ever reading that pipe).
    fn run_with_timeout(&self, args: &[&str], timeout: Duration) -> Result<String> {
        let mut child = Command::new(&self.binary)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning {:?} {:?}", self.binary, args))?;

        let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
        let stdout_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stdout_pipe.read_to_string(&mut buf);
            buf
        });
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr_pipe.read_to_string(&mut buf);
            buf
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child
                .try_wait()
                .with_context(|| format!("waiting for {:?} {:?}", self.binary, args))?
            {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(
                        "lgs {:?} did not complete within {:?} and was killed — the daemon may be \
                         hung or unreachable",
                        args,
                        timeout
                    );
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };

        // Best-effort: a reader thread panicking would be a bug in this
        // function, not in `lgs`, but must not propagate as a panic from
        // here — fall back to an empty capture rather than unwrap.
        let stdout = stdout_thread.join().unwrap_or_default();
        let stderr = stderr_thread.join().unwrap_or_default();

        if !status.success() {
            bail!("lgs {:?} failed: {}", args, stderr.trim());
        }
        Ok(stdout)
    }

    pub fn status(&self) -> Result<StatusReport> {
        parse_status(&self.run(&["status", "--json"])?)
    }
    pub fn add(&self, path: &str, name: &str) -> Result<()> {
        self.run(&["add", path, "--name", name]).map(|_| ())
    }
    pub fn restore(&self, name: &str, path: &str) -> Result<String> {
        self.run(&["restore", name, path])
    }
    pub fn projects(&self) -> Result<String> {
        self.run(&["projects", "--json"])
    }
    pub fn init(&self, cloud_root: &str) -> Result<()> {
        self.run(&["init", "--cloud-root", cloud_root]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../egui-frontend/tests/fixtures/lgs_status.json");

    /// Review Critical-2: `LgsClient::run` must not block forever on a
    /// hung/stalled `lgs` process. Drives a fake script that sleeps far
    /// longer than a short test timeout, and asserts both that it errors
    /// out (rather than hanging) and that it does so close to the timeout,
    /// not close to the script's full sleep duration — proving the process
    /// was actually killed, not merely that `run` gave up waiting on it.
    #[test]
    fn run_times_out_and_kills_a_hung_process_instead_of_blocking_forever() {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("lgs");
        std::fs::write(&script_path, "#!/bin/sh\nsleep 5\necho too-late\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let client = LgsClient::new(script_path);
        let start = std::time::Instant::now();
        let result = client.run_with_timeout(&["status", "--json"], Duration::from_millis(200));
        let elapsed = start.elapsed();

        let err = result.expect_err("a hung process must be reported as an error, not hang the caller");
        assert!(
            format!("{err}").contains("did not complete"),
            "error must say the process was killed for timing out: {err}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "run_with_timeout must return close to its 200ms bound, not wait out the process's \
             full 5s sleep: took {elapsed:?}"
        );
    }

    /// The ordinary success path still works with the spawn+timeout
    /// implementation (not just the old `Command::output()` one) — output
    /// is captured correctly and a fast, well-behaved process is not
    /// mistaken for a hang.
    #[test]
    fn run_captures_stdout_for_a_well_behaved_process() {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("lgs");
        std::fs::write(&script_path, "#!/bin/sh\necho '{\"projects\":[]}'\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let client = LgsClient::new(script_path);
        let out = client.run(&["status", "--json"]).unwrap();
        assert_eq!(out.trim(), r#"{"projects":[]}"#);
    }

    #[test]
    fn parses_the_real_fixture() {
        let report = parse_status(FIXTURE).unwrap();
        assert!(report.cloud_root.is_some());
        assert!(!report.projects.is_empty());

        // Pins a real-world find: at capture time, this project's
        // `durability.state` was `"stranded"` (commits pushed only locally,
        // never reached the remote) — not one of the four states originally
        // modeled. This assertion is the regression guard that turns that
        // discovery into a permanent check rather than leaving it in prose.
        let project = report
            .project("allowance-tracker")
            .expect("fixture must contain the allowance-tracker project");
        assert_eq!(project.durability_state, DurabilityState::Stranded);
        assert!(!project.is_confirmed_backed_up());
    }

    #[test]
    fn an_unknown_durability_state_does_not_fail_the_parse() {
        // lgs's own tests feed "a_variant_from_the_future". A String field would
        // model this loosely; a closed enum would reject the whole document.
        let json = r#"{"daemon_running":true,"cloud_root":"/tmp","cloud_root_exists":true,
          "port":8418,"projects":[{"name":"p","working_repo_path":"/tmp/p",
          "clone_url":"http://localhost:8418/p.git","archived":false,
          "durability":{"state":"a_variant_from_the_future","generation":1}}]}"#;
        let report = parse_status(json).unwrap();
        assert_eq!(report.projects[0].durability_state, DurabilityState::Unknown);
    }

    #[test]
    fn outdated_daemon_is_reported_not_swallowed() {
        let json = r#"{"daemon_running":true,"cloud_root":"/tmp","cloud_root_exists":true,
          "port":8418,"projects":[],
          "daemon":{"state":"outdated","message":"restart the service to pick up the new binary"}}"#;
        let report = parse_status(json).unwrap();
        assert_eq!(report.daemon.state, DaemonState::Outdated);
        assert!(report.daemon.message.contains("restart the service"));
        assert!(
            !report.durability_data_is_fresh(),
            "a skewed daemon reads durability from disk; we must not claim backed-up"
        );
    }

    /// A syntactically broken (or truncated, e.g. by a killed `lgs` process)
    /// JSON payload must return an `Err`, not panic. `serde_json::from_str`
    /// already does this; this test pins that `parse_status` propagates it
    /// rather than unwrapping internally.
    #[test]
    fn malformed_json_returns_err_not_panic() {
        let truncated = r#"{"daemon_running":true,"cloud_root":"/tmp","cloud_root_exists":tru"#;
        let result = parse_status(truncated);
        assert!(result.is_err(), "expected an Err for truncated JSON, got {result:?}");
    }

    /// A project entry with only the fields lgs guarantees present (name,
    /// working_repo_path, clone_url, archived) — no durability, no
    /// durability_label, no failed_sync_attempts — must still parse. These
    /// are exactly the optional fields `#[serde(default)]` exists to cover.
    #[test]
    fn a_project_missing_every_optional_field_still_parses() {
        let json = r#"{"daemon_running":true,"cloud_root":"/tmp","cloud_root_exists":true,
          "port":8418,"projects":[{"name":"bare","working_repo_path":"/tmp/bare",
          "clone_url":"http://localhost:8418/bare.git","archived":false}]}"#;
        let report = parse_status(json).unwrap();
        let project = &report.projects[0];
        assert_eq!(project.name, "bare");
        assert_eq!(project.working_repo_path, PathBuf::from("/tmp/bare"));
        assert_eq!(project.clone_url, "http://localhost:8418/bare.git");
        assert!(!project.archived);
        assert_eq!(project.durability_state, DurabilityState::Unknown);
        assert_eq!(project.durability_label, None);
        assert_eq!(project.failed_sync_attempts, 0);
    }

    /// `durability_data_is_fresh()` is the gate on whether the app trusts the
    /// durability values in this report at all: it must be true only when
    /// the daemon is `Ok`, and false for every other state — including ones
    /// this module does not recognize yet.
    #[test]
    fn durability_data_is_fresh_is_true_only_for_ok_daemon() {
        let cases: Vec<(DaemonState, bool)> = vec![
            (DaemonState::Ok, true),
            (DaemonState::Down, false),
            (DaemonState::Outdated, false),
            (DaemonState::Error, false),
            (DaemonState::Unknown, false),
        ];
        for (state, expected) in cases {
            let report = StatusReport {
                daemon: DaemonInfo { state, message: String::new() },
                cloud_root: None,
                cloud_root_exists: false,
                projects: Vec::new(),
                adoptable: Vec::new(),
            };
            assert_eq!(
                report.durability_data_is_fresh(),
                expected,
                "for daemon state {state:?}"
            );
        }
    }

    /// `is_confirmed_backed_up()` is the per-project safety answer: true only
    /// for `BackedUp`, false for every other state — including three states
    /// (`Stranded`, `WorkingRepoUnreadable`, `BareRepoUnreadable`) that are
    /// `Severity::Red` in lgs's own model, and false for `Unknown`, which
    /// could be Red too since it covers states lgs adds after this file was
    /// written.
    #[test]
    fn is_confirmed_backed_up_is_true_only_for_backed_up() {
        fn project_with(state: DurabilityState) -> ProjectReport {
            ProjectReport {
                name: "p".to_string(),
                clone_url: "http://localhost:8418/p.git".to_string(),
                working_repo_path: PathBuf::from("/tmp/p"),
                durability_state: state,
                durability_label: None,
                failed_sync_attempts: 0,
                archived: false,
            }
        }

        let cases: Vec<(DurabilityState, bool)> = vec![
            (DurabilityState::BackedUp, true),
            (DurabilityState::Pending, false),
            (DurabilityState::NotBackedUp, false),
            (DurabilityState::Diverged, false),
            (DurabilityState::Stranded, false),
            (DurabilityState::WorkingRepoUnreadable, false),
            (DurabilityState::BareRepoUnreadable, false),
            (DurabilityState::Unknown, false),
        ];
        for (state, expected) in cases {
            assert_eq!(
                project_with(state).is_confirmed_backed_up(),
                expected,
                "for durability state {state:?}"
            );
        }
    }
}
