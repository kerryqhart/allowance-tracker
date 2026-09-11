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
//!   1. its own `HOME` (a fresh `TempDir`) — structurally guaranteed by
//!      [`Machine::lgs_command`]: it is the *only* place that constructs a
//!      `Command` for the `lgs` binary, and it always sets `HOME`. Every
//!      other function in this file that runs `lgs` (the daemon spawn, the
//!      readiness check, `Machine::run`) goes through it, so there is no
//!      call site left that could forget the override.
//!   2. its own cloud root (a fresh `TempDir`, set via `lgs init --cloud-root`
//!      before the daemon ever starts), and
//!   3. its own port — see [`MACHINE_A_PORT`]/[`MACHINE_B_PORT`] below.
//!
//! No command here ever runs without an overridden `HOME`, and nothing here
//! calls `install-service`, `uninstall-service`, `restart`, or any
//! `launchctl`/service-manager path. Every spawned daemon is killed (not
//! merely dropped) by [`Machine`]'s `Drop` impl — armed on the very next line
//! after `spawn()` returns (see [`Machine::start`]), so a panic anywhere
//! afterward, including during the readiness wait, still kills the process
//! instead of leaking one holding a port.

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

/// Deliberately far outside every bounded wait this harness uses (60s in
/// `two_machine_sync.rs`). Written into each machine's `config.toml` before
/// its daemon ever starts, so the daemon's own ambient background sync loop
/// (`run_sync_loop`, which ticks every `poll_interval_secs`) cannot fire
/// during a test run. Without this, a test that waits up to 60s for
/// `refs/lgs-auth/*` to converge cannot tell "the explicit `lgs sync` calls
/// made this happen" from "we waited long enough for the daemon's own
/// 30-second-default poll loop to do it instead" — the same commit would
/// converge either way, for a different reason than the test names.
///
/// Do not "tidy" this back down to a small number: doing so silently
/// reintroduces exactly that ambiguity.
const NO_AMBIENT_TICKS_POLL_INTERVAL_SECS: u64 = 3600;

/// Two lgs daemons under two HOMEs sharing one cloud root — the full
/// A -> bare -> cloud -> bare -> B loop, offline, in CI.
pub struct TwoMachineHarness {
    _cloud: TempDir,
    #[allow(dead_code)] // part of the harness's public surface; not every caller needs it
    cloud_root: PathBuf,
    pub a: Machine,
    pub b: Machine,
}

pub struct Machine {
    _home: TempDir,
    pub home: PathBuf,
    pub work: PathBuf,
    pub port: u16,
    lgs_binary: PathBuf,
    /// The running `lgs daemon` subprocess for this machine. `Some` from the
    /// line right after `spawn()` in `Machine::start` until `Drop` kills it;
    /// never left dangling in between — there is no code path that takes
    /// this without immediately killing what it held.
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
        }
    }

    /// Run `lgs <args>` against machine `m`'s daemon. `HOME` is always
    /// overridden to `m.home` — never the ambient process environment — so
    /// this can never reach the user's real config or real daemon.
    pub fn lgs(&self, m: &Machine, args: &[&str]) -> String {
        m.run(args)
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
    /// port (and a poll interval that keeps the daemon's ambient sync loop
    /// out of the test window — see [`NO_AMBIENT_TICKS_POLL_INTERVAL_SECS`]),
    /// then start (and wait for) its daemon.
    ///
    /// Two ordering guarantees here, both load-bearing:
    ///   - the port is written into `config.toml` *before* `lgs daemon` ever
    ///     runs, so the daemon's very first bind is already on `port`, never
    ///     on the real default 8418;
    ///   - the spawned `Child` is stored into the already-constructed,
    ///     `Drop`-armed `Machine` on the line immediately after `spawn()`
    ///     returns, with no fallible operation in between. A panic during
    ///     the readiness wait that follows therefore still unwinds through a
    ///     `Machine` whose `Drop` owns and kills the process, rather than
    ///     leaking a `Child` that was never attached to anything with a
    ///     destructor.
    fn start(tag: &str, port: u16, cloud_root: &Path, lgs_binary: &Path) -> Self {
        let home_dir = TempDir::new().expect("tempdir for machine home");
        let home = home_dir.path().to_path_buf();
        let work = home.join(format!("work-{tag}"));
        std::fs::create_dir_all(&work).expect("create work dir");

        let mut m = Self {
            _home: home_dir,
            home,
            work,
            port,
            lgs_binary: lgs_binary.to_path_buf(),
            daemon: None,
        };

        // `lgs init` creates config.toml (generated machine_id + cloud_root)
        // without starting a daemon (local-git-sync/src/init.rs) — safe to
        // run before anything is listening. It always leaves `port` and
        // `poll_interval_secs` at their built-in defaults (8418, 30), so
        // both get patched directly into the file next, still with no
        // daemon running.
        //
        // `lgs config port <n>` is deliberately NOT used here: `cli::set_config`
        // sends `port` over the daemon's IPC socket (`call()`), which requires
        // an *already-running* daemon — meaning this daemon's first bind would
        // have to happen on the default port 8418 before it could ever be told
        // to use a different one. That is exactly the collision this harness
        // must never risk, so the port (and poll interval) are written into
        // config.toml up front instead, before `lgs daemon` is ever spawned.
        m.run(&["init", "--cloud-root", &cloud_root.to_string_lossy()]);
        m.patch_config();

        let child = m
            .lgs_command()
            .arg("daemon")
            .spawn()
            .unwrap_or_else(|e| panic!("spawn lgs daemon for machine {tag}: {e}"));
        // See the doc comment above: attaching the Child here, immediately,
        // is what makes the Drop guarantee hold through the readiness wait
        // below rather than only after `start` returns.
        m.daemon = Some(child);

        m.wait_ready(tag);

        m
    }

    /// Build a `Command` for the `lgs` binary with `HOME` already set to this
    /// machine's home. The *only* place in this file that constructs such a
    /// `Command` — every call site (`run`, the daemon spawn, and the
    /// readiness check) goes through this, so "HOME is always overridden" is
    /// a structural property of the code, not a claim that has to be
    /// re-verified against every call site by hand.
    fn lgs_command(&self) -> Command {
        let mut c = Command::new(&self.lgs_binary);
        c.env("HOME", &self.home);
        c
    }

    /// Run `lgs <args>` against this machine's daemon, asserting success.
    fn run(&self, args: &[&str]) -> String {
        let out = self
            .lgs_command()
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("running lgs {args:?} (HOME={:?}): {e}", self.home));
        assert!(
            out.status.success(),
            "lgs {:?} (HOME={:?}) failed: {}",
            args,
            self.home,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// Patch `port` and `poll_interval_secs` directly into a freshly-`lgs
    /// init`'d config.toml. Only safe because nothing has read or bound the
    /// file yet — this writes the same `port` field `lgs config port` would
    /// eventually set, just earlier, and without needing a live daemon to
    /// send it to (see `Machine::start`'s comment); `poll_interval_secs` has
    /// no daemon-mediated path at all in this harness, since it only matters
    /// before the daemon's sync loop ever starts ticking.
    fn patch_config(&self) {
        let config_path = self.home.join(".config").join("lgs").join("config.toml");
        let text = std::fs::read_to_string(&config_path)
            .unwrap_or_else(|e| panic!("reading {config_path:?}: {e}"));
        let mut value: toml::Value =
            toml::from_str(&text).unwrap_or_else(|e| panic!("parsing {config_path:?}: {e}"));
        let table = value
            .as_table_mut()
            .expect("config.toml must parse as a TOML table");
        table.insert("port".to_string(), toml::Value::Integer(self.port as i64));
        table.insert(
            "poll_interval_secs".to_string(),
            toml::Value::Integer(NO_AMBIENT_TICKS_POLL_INTERVAL_SECS as i64),
        );
        let rewritten = toml::to_string_pretty(&value).expect("serializing config.toml");
        std::fs::write(&config_path, rewritten)
            .unwrap_or_else(|e| panic!("writing {config_path:?}: {e}"));
    }

    /// Bounded wait for a freshly spawned daemon to actually be listening.
    /// Delegates the bound to lgs's own `status --wait`, which polls `Ping`
    /// against the daemon's socket for up to 10s (`ipc::READY_TIMEOUT`) rather
    /// than sleeping blindly — see `local-git-sync/src/ipc.rs::wait_until_ready`.
    fn wait_ready(&self, tag: &str) {
        let out = self
            .lgs_command()
            .args(["status", "--wait", "--json"])
            .output()
            .unwrap_or_else(|e| panic!("running `lgs status --wait` for machine {tag}: {e}"));
        assert!(
            out.status.success(),
            "lgs daemon for machine {tag} did not become ready within 10s: {}",
            String::from_utf8_lossy(&out.stderr)
        );
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
