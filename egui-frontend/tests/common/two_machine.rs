// egui-frontend/tests/common/two_machine.rs
//! Two lgs daemons under two `HOME`s sharing one cloud root — the full
//! A -> bare -> cloud -> bare -> B loop, offline, in CI. `lgs::paths`
//! resolves every path it touches from `$HOME`, so two temp homes plus one
//! shared temp cloud root is enough to simulate two Macs on one machine.
//!
//! # Safety
//!
//! Every daemon this harness spawns is isolated on all three axes a real
//! daemon needs isolating on:
//!   1. its own `HOME` (a fresh `TempDir`, via `Command::env("HOME", ..)`),
//!   2. its own cloud root (a fresh `TempDir`, set via `lgs init --cloud-root`
//!      before the daemon ever starts), and
//!   3. its own port — see [`MACHINE_A_PORT`]/[`MACHINE_B_PORT`] below.
//!
//! No command here ever runs without an overridden `HOME`, and nothing here
//! calls `install-service`, `uninstall-service`, `restart`, or any
//! `launchctl`/service-manager path. Every spawned daemon is killed (not
//! merely dropped) by [`Machine`]'s `Drop` impl, so a panicking test cannot
//! leak a process holding one of these ports.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use tempfile::TempDir;

/// Ports for the two test daemons. Never 8418 — that is the user's real,
/// live lgs daemon backing up real projects, and binding it here would
/// collide with it — and never each other's, since two daemons cannot share
/// a port. Chosen well away from 8418 so a stray real service is unlikely to
/// already be sitting on them.
const MACHINE_A_PORT: u16 = 18418;
const MACHINE_B_PORT: u16 = 18419;

/// Two lgs daemons under two HOMEs sharing one cloud root — the full
/// A -> bare -> cloud -> bare -> B loop, offline, in CI.
pub struct TwoMachineHarness {
    _cloud: TempDir,
    #[allow(dead_code)] // part of the harness's public surface; not every caller needs it
    cloud_root: PathBuf,
    pub a: Machine,
    pub b: Machine,
    lgs_binary: PathBuf,
}

pub struct Machine {
    _home: TempDir,
    pub home: PathBuf,
    pub work: PathBuf,
    pub port: u16,
    /// The running `lgs daemon` subprocess for this machine. `Some` from
    /// construction until `Drop` kills it; never left dangling in between —
    /// there is no code path that takes this without immediately killing
    /// what it held.
    daemon: Option<Child>,
}

impl TwoMachineHarness {
    pub fn new(lgs_binary: PathBuf) -> Self {
        let cloud = TempDir::new().expect("tempdir for shared cloud root");
        let cloud_root = cloud.path().to_path_buf();
        let a = Machine::start("a", MACHINE_A_PORT, &cloud_root, &lgs_binary);
        let b = Machine::start("b", MACHINE_B_PORT, &cloud_root, &lgs_binary);
        Self {
            _cloud: cloud,
            cloud_root,
            a,
            b,
            lgs_binary,
        }
    }

    /// Run `lgs <args>` against machine `m`'s daemon. `HOME` is always
    /// overridden to `m.home` — never the ambient process environment — so
    /// this can never reach the user's real config or real daemon.
    pub fn lgs(&self, m: &Machine, args: &[&str]) -> String {
        run_lgs(&self.lgs_binary, &m.home, args)
    }

    /// Publish A, ingest into B, publish B, ingest into A.
    pub fn sync_both_ways(&self, project: &str) {
        self.lgs(&self.a, &["sync", project]);
        self.lgs(&self.b, &["sync", project]);
        self.lgs(&self.a, &["sync", project]);
    }

    #[allow(dead_code)] // part of the harness's public surface; not every caller needs it
    pub fn cloud(&self) -> &Path {
        &self.cloud_root
    }
}

impl Machine {
    /// Build a machine's `HOME`/work dir, write a config pinned to its own
    /// port and the shared cloud root, then start (and wait for) its daemon.
    ///
    /// Ordering matters for safety: the port is written into `config.toml`
    /// *before* `lgs daemon` ever runs, so the daemon's very first bind is
    /// already on `port`, never on the real default 8418.
    fn start(tag: &str, port: u16, cloud_root: &Path, lgs_binary: &Path) -> Self {
        let home_dir = TempDir::new().expect("tempdir for machine home");
        let home = home_dir.path().to_path_buf();
        let work = home.join(format!("work-{tag}"));
        std::fs::create_dir_all(&work).expect("create work dir");

        // `lgs init` creates config.toml (generated machine_id + cloud_root)
        // without starting a daemon (local-git-sync/src/init.rs) — safe to
        // run before anything is listening. It always leaves `port` at the
        // built-in default (8418), so that gets patched directly into the
        // file next, still with no daemon running.
        //
        // `lgs config port <n>` is deliberately NOT used here: `cli::set_config`
        // sends `port` over the daemon's IPC socket (`call()`), which requires
        // an *already-running* daemon — meaning this daemon's first bind would
        // have to happen on the default port 8418 before it could ever be told
        // to use a different one. That is exactly the collision this harness
        // must never risk, so the port is written into config.toml up front
        // instead, before `lgs daemon` is ever spawned.
        run_lgs(
            lgs_binary,
            &home,
            &["init", "--cloud-root", &cloud_root.to_string_lossy()],
        );
        set_config_port(&home, port);

        let daemon = Command::new(lgs_binary)
            .arg("daemon")
            .env("HOME", &home)
            .spawn()
            .unwrap_or_else(|e| panic!("spawn lgs daemon for machine {tag}: {e}"));

        wait_for_daemon_ready(lgs_binary, &home, tag);

        Self {
            _home: home_dir,
            home,
            work,
            port,
            daemon: Some(daemon),
        }
    }

    /// The HTTP clone URL this machine's own daemon serves `project` at.
    pub fn clone_url(&self, project: &str) -> String {
        format!("http://localhost:{}/{project}.git", self.port)
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        // A leaked daemon here would hold its port and linger as a stray
        // process on the user's machine — the harness's one hard safety
        // requirement. `Child::kill` (SIGKILL) rather than a graceful IPC
        // shutdown is deliberate and fine here: this daemon's entire state
        // lives under `_home`/the shared cloud `TempDir`, both of which are
        // about to be deleted anyway, unlike a real, persistent daemon.
        if let Some(mut child) = self.daemon.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Run `lgs <args>` with `HOME` overridden to `home` — never the ambient
/// environment. Every lgs invocation in this harness goes through this one
/// function (directly, or via `TwoMachineHarness::lgs`), so "HOME is always
/// overridden" is one property to check, not one per call site.
fn run_lgs(lgs_binary: &Path, home: &Path, args: &[&str]) -> String {
    let out = Command::new(lgs_binary)
        .args(args)
        .env("HOME", home)
        .output()
        .unwrap_or_else(|e| panic!("running lgs {args:?} (HOME={home:?}): {e}"));
    assert!(
        out.status.success(),
        "lgs {:?} (HOME={:?}) failed: {}",
        args,
        home,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Patch `port` directly into a freshly-`lgs init`'d config.toml. Only safe
/// because nothing has read or bound the file yet — this writes the same
/// field `lgs config port` would eventually set, just earlier, and without
/// needing a live daemon to send it to (see `Machine::start`'s comment).
fn set_config_port(home: &Path, port: u16) {
    let config_path = home.join(".config").join("lgs").join("config.toml");
    let text = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("reading {config_path:?}: {e}"));
    let mut value: toml::Value =
        toml::from_str(&text).unwrap_or_else(|e| panic!("parsing {config_path:?}: {e}"));
    value
        .as_table_mut()
        .expect("config.toml must parse as a TOML table")
        .insert("port".to_string(), toml::Value::Integer(port as i64));
    let rewritten = toml::to_string_pretty(&value).expect("serializing config.toml");
    std::fs::write(&config_path, rewritten)
        .unwrap_or_else(|e| panic!("writing {config_path:?}: {e}"));
}

/// Bounded wait for a freshly spawned daemon to actually be listening.
/// Delegates the bound to lgs's own `status --wait`, which polls `Ping`
/// against the daemon's socket for up to 10s (`ipc::READY_TIMEOUT`) rather
/// than sleeping blindly — see `local-git-sync/src/ipc.rs::wait_until_ready`.
fn wait_for_daemon_ready(lgs_binary: &Path, home: &Path, tag: &str) {
    let out = Command::new(lgs_binary)
        .args(["status", "--wait", "--json"])
        .env("HOME", home)
        .output()
        .unwrap_or_else(|e| panic!("running `lgs status --wait` for machine {tag}: {e}"));
    assert!(
        out.status.success(),
        "lgs daemon for machine {tag} did not become ready within 10s: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
