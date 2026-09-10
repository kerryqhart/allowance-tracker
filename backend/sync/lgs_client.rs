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
//! this exact shape (see `local-git-sync/src/cli.rs` around line 1788), and
//! the real fixture captured for this file already contains a `durability`
//! state (`"stranded"`) this module does not model. A closed enum would fail
//! the whole parse the moment lgs reports a state this app does not know
//! about yet; modeling the field as a bare `String` would lose the ability to
//! match on the states this app *does* understand. `#[serde(other)]` is the
//! only shape that tolerates the future without going loose today.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurabilityState {
    BackedUp,
    Pending,
    NotBackedUp,
    Diverged,
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

#[derive(Debug, Clone)]
pub struct StatusReport {
    pub daemon: DaemonInfo,
    pub cloud_root: Option<PathBuf>,
    pub cloud_root_exists: bool,
    pub projects: Vec<ProjectReport>,
    pub adoptable: Vec<String>,
}

impl StatusReport {
    /// Durability is only trustworthy from a daemon we can actually talk to.
    ///
    /// When the daemon is `outdated` (or `down`/`error`/anything unrecognized),
    /// the durability values on each project were read from disk and may be
    /// stale — so the app must not tell the user a project is backed up.
    /// Reporting nothing is safer than reporting a stale "backed up ✓".
    pub fn can_claim_durability(&self) -> bool {
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
        adoptable: raw.adoptable.into_iter().map(|a| a.name).collect(),
    })
}

pub struct LgsClient {
    binary: PathBuf,
}

impl LgsClient {
    pub fn new(binary: PathBuf) -> Self {
        Self { binary }
    }

    pub fn run(&self, args: &[&str]) -> Result<String> {
        let out = Command::new(&self.binary)
            .args(args)
            .output()
            .with_context(|| format!("running {:?} {:?}", self.binary, args))?;
        if !out.status.success() {
            bail!(
                "lgs {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
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

    #[test]
    fn parses_the_real_fixture() {
        let report = parse_status(FIXTURE).unwrap();
        assert!(report.cloud_root.is_some());
        assert!(!report.projects.is_empty());
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
            !report.can_claim_durability(),
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

    /// `can_claim_durability()` is the gate on whether the app tells the user
    /// their data is safe: it must be true only when the daemon is `Ok`, and
    /// false for every other state — including ones this module does not
    /// recognize yet.
    #[test]
    fn can_claim_durability_is_true_only_for_ok_daemon() {
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
                report.can_claim_durability(),
                expected,
                "for daemon state {state:?}"
            );
        }
    }
}
