# lgs desktop sync — manual acceptance checklist

**Covers:** the desktop-to-desktop sync path built in
`docs/superpowers/plans/2026-09-07-lgs-desktop-sync.md` — bootstrap
(`backend/sync/bootstrap.rs`), the daemon adopt-or-install decision
(`plan_daemon_action`/`ensure_daemon`), `GitManager`'s remote operations
(`backend/storage/git/mod.rs`), `ChildSyncEngine` (`backend/sync/child_sync.rs`),
and "Check sync".
**Design:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md`.

These are the checks CI genuinely cannot perform. Everything else — the
merge, the codec, the cloud-path guard, migration, check-sync stage
sequencing — is already exercised by the test suite; see "What CI already
covers" below before re-testing any of it by hand. **Two-machine
convergence is the exception** — see the correction in that section below
before assuming it is covered.

Mirrors the structure and discipline of
`local-git-sync/docs/bootstrap-install-acceptance-checklist.md`, including
its append-only run history.

## When to re-run

Any change to:

- `backend/sync/bootstrap.rs` (`ensure_daemon`, `plan_daemon_action`,
  `install_and_start`, `bundled_lgs_candidates`/`bundled_lgs_path`)
- `backend/storage/git/mod.rs`'s remote operations (`fetch_lgs`, `push_lgs`,
  `fetch_lgs_refspec`, `push_lgs_refspec`, and their deadline/callback wiring)
- `backend/sync/child_sync.rs`'s `check_sync`/`check_sync_against`
- `egui-frontend/Cargo.toml`'s `[package.metadata.bundle]` or `build.rs`
- launchd plist generation/paths, or anything in `local-git-sync` itself that
  this design leans on (the Proton File Provider assumption in particular)

This is deliberately a trigger rather than an enforced gate, for the same
reason `local-git-sync`'s own checklist gives: a commit hook here would
produce habitual `--no-verify` on a single-author repo with no PR flow,
degrading the hooks that already run.

**Append results — never overwrite.** The run history is the point.

## Inherited assumption, stated plainly

`local-git-sync`'s own bootstrap checklist still reads "Run 1 — not yet
performed" as of this writing, and its item 1 — Proton File Provider
materialization — is described there as **the weakest assumption its whole
design leans on**. This design's cold-start path (first run on a fresh
machine, reading a project out of a cloud-drive-backed bundle that this
machine's Proton client has never touched) inherits that exact assumption
unproven. Item 1 below is the same open question, asked again at this
design's own entry point rather than assumed answered by reference. Until
one of the two checklists actually records a real run, neither design has
verified the ground its cold start stands on.

## The checks

### 1. Proton File Provider materialization

**The same weakest-assumption question as `local-git-sync`'s own checklist,
asked at this design's entry point.** Must be run on a machine (or fresh
user account) whose Proton client has synced the cloud root but has
**never** read the bundle(s) this app's first run needs.

- [ ] Confirm (e.g. via Finder's cloud-status icon, or `ls -la` showing a
      dataless/placeholder file) that the target bundle is genuinely
      un-materialized before starting — not merely "not recently opened."
- [ ] Trigger this app's cold-start read against that bundle (first run /
      restore onboarding a second machine).
- [ ] Record which of these actually happened:
  - reads **block** until the data is available (first run is slow but
    eventually succeeds)
  - reads return **partial** data (the read appears to succeed but the
    content is truncated/incomplete — the dangerous case, since nothing
    obviously fails)
  - reads **fail outright** and require an explicit materialization step
    (e.g. opening the file in Finder, or `brctl download`) before this
    app's read can succeed
- [ ] If the read blocks, record how long (rough order of magnitude — seconds
      vs. minutes) and whether the app's UI stays responsive or appears
      hung during that wait.

**PASS** looks like: the cold-start read completed with fully correct,
verified content (compare byte-for-byte against the source machine's copy,
not just "the app didn't crash"), and the observed behaviour (block /
partial / explicit-call) is recorded verbatim, not summarized as "it
worked." A PASS here answers the question; it does not make the assumption
disappear for future changes to this path.

**Known failure mode:** if reads return partial data silently, the app could
construct a `TxRow` set from a truncated CSV and proceed as if it were
complete — the merge and balance-validation properties do not protect
against this because CI never sees a partially-materialized file; they only
run on faithfully-materialized data. Since this hazard is exactly what
`local-git-sync`'s own checklist warns about, and since neither design
inherits that answer safely, if this design's own materialization triggers
through a different code path than `local-git-sync`'s bootstrap stub does,
that difference should be named explicitly when recording the result.

### 2. launchd at login

- [ ] Reboot the test machine. Confirm the `com.local-git-sync.daemon` is
      running after login **without anyone touching it** — no manual
      `launchctl kickstart`, no opening the app first. `RunAtLoad` fires at
      login; it does not fire on a manual `bootstrap`, so this specifically
      tests the unattended path a non-technical user actually experiences.
- [ ] Kill the login-started daemon (`kill <pid>`, found via
      `pgrep -fl "lgs daemon"` or equivalent). Confirm launchd respawns it
      (`KeepAlive`). **Allow ~10 seconds for throttling — a 5-second wait
      proves nothing** (a same-second respawn can hide a KeepAlive
      misconfiguration that only shows up once launchd's throttle window
      kicks in).
- [ ] Confirm the respawned process actually serves requests (e.g.
      `lgs status --json` against it succeeds), not merely that a process
      with the right name exists — a registered-but-unresponsive process is
      the exact false positive `local-git-sync`'s own launchd investigation
      warned about.

**PASS** looks like: after reboot, the daemon is confirmed serving requests
before you touch anything; after a kill, the daemon is confirmed serving
requests again within the ~10s throttling window, with the *same* plist
still loaded (not a second competing instance).

**Known failure mode:** a plist that loads but whose program path is wrong
(see item 4) will show as "loaded" in `launchctl print` while never
successfully running — don't stop at "loaded," confirm "running and
answering."

### 3. Real clock skew

- [ ] Set the two test Macs' system clocks about one minute apart (System
      Settings → General → Date & Time, or `sudo sntp` pointed at nothing —
      whatever this OS version allows; do not use a fake/injected clock,
      the point is the real OS clock feeding real commit timestamps).
- [ ] On each machine, make a conflicting edit to the *same* transaction
      (e.g. both machines edit the same row's description or amount) while
      offline from each other's edit, then let both machines sync.
- [ ] Confirm the edit from whichever machine's commit carries the **later
      committer timestamp** wins the conflict, per the merge's tie-break
      rule (`committer_epoch` wins; ties break on `commit_oid`) — on
      **both** machines, not just the one that "should" win.
- [ ] Confirm the two machines converge to **byte-identical** CSV content
      afterward (diff the rendered files, not just "the balance looks
      right").

**PASS** looks like: both machines show the same winning edit, and a byte
diff of the transactions file on both machines is empty. This exercises
real OS clocks feeding real `git2::Signature` committer times into the
merge's `committer_epoch` comparison — the merge logic itself is already
proven by CI's property tests (symmetry, idempotence, fixed point) using
synthetic epochs; what CI cannot exercise is two real, independently-clocked
machines actually producing skewed real timestamps and the merge correctly
picking the later one from real data.

**Known failure mode:** if the two machines' clocks are skewed in the
*other* direction from what you expect (double-check which one you set
ahead), you can mistake "the wrong machine's edit won because of a test
setup error" for "the merge got it backward" — verify which machine's clock
you advanced before drawing a conclusion.

### 4. The app-moved case

- [ ] Run the app for the first time from wherever it was downloaded to
      (e.g. `~/Downloads/Allowance Tracker.app`), completing first-run setup
      including daemon install.
- [ ] Quit the app. Move the `.app` bundle to `/Applications` (drag-and-drop
      or `mv`, whichever a real user would do).
- [ ] Confirm the daemon still starts correctly afterward — reboot, or kill
      and let launchd respawn it (see item 2), and confirm it serves
      requests.

**PASS** looks like: the daemon runs and serves requests after the move,
with no reinstall or re-run of first-run setup. This specifically confirms
that the installed launchd plist's program path points at the binary this
app copied out to a stable location
(`~/Library/Application Support/Allowance Tracker/bin/lgs`), not at a path
inside the `.app` bundle — `install_and_start` (`backend/sync/bootstrap.rs`)
installs against `current_exe()`'s copied-out path for exactly this reason,
but nothing in CI moves a real `.app` around to prove the plist doesn't
silently point into the (now-relocated) bundle instead.

**Known failure mode:** if the plist ever regresses to pointing inside the
bundle, this failure is invisible until the app is moved — first run and
every subsequent boot on the original location will look completely healthy.

### 5. cargo-bundle resource layout

- [ ] Run a real release build with `cargo bundle` (with `LGS_BINARY` set to
      a real pinned `lgs` binary), producing an actual `.app`.
- [ ] Inspect `Contents/Resources/` in the produced bundle and confirm which
      of the three candidate paths `bundled_lgs_path()`
      (`backend/sync/bootstrap.rs`) actually finds the binary at:
      1. `Contents/Resources/lgs`
      2. `Contents/Resources/target/release/lgs`
      3. the dev-build sibling path (`<exe dir>/lgs` — not expected in a
         release bundle, but confirm it isn't accidentally the one that
         resolves)
- [ ] Confirm the app's first run finds and copies out the binary
      successfully from whichever candidate matched.

**PASS** looks like: the produced `.app` contains the binary at exactly one
of the three candidates, first run finds it without hitting the "bundled lgs
binary not found; tried: ..." error path, and you record which candidate it
was.

This is a **confirmation, not a risk** — the resolver already tries all
three in order and is unit-tested against synthetic layouts for each, per
Task 11's review response. Nobody has run a real `cargo bundle` against this
project yet, so this is the first time the assumption meets reality; a
surprising answer here would be worth a comment in the code once confirmed,
even though no code change should be *needed* regardless of which candidate
wins.

### 6. Daemon install on a machine with no existing daemon

- [ ] On a machine (or fresh user account) that has never run `local-git-sync`
      before — no `com.local-git-sync.daemon` plist, no daemon process —
      run this app's first-run flow end to end.
- [ ] Confirm `ensure_daemon`'s `InstallAndOwn` branch runs
      `install_and_start` successfully: `lgs install-service` writes the
      plist, `launchctl bootstrap` loads it, `launchctl kickstart` starts
      it, and the daemon is actually serving requests afterward (not merely
      registered — see item 2's same warning).
- [ ] Confirm `DaemonOwnership.installed_by_app` is persisted as `true`
      afterward (readable from the app's saved sync state), so a later
      outdated-daemon check knows this app owns it and may restart it.
- [ ] On a second run of first-run setup (or a restart of the app) against
      the same now-installed daemon, confirm it is recognized as healthy
      and *not* reinstalled a second time.

**PASS** looks like: a machine with genuinely no prior `local-git-sync`
daemon goes from nothing to a running, request-serving daemon through this
app's own first-run flow alone, with no manual `launchctl` or `lgs
install-service` run by hand — and the ownership flag is set afterward.

This is the **only path a non-technical user's actual first run takes** —
every other daemon-state test in this project (the decision-table unit
tests in `backend/sync/bootstrap.rs`) exercises `plan_daemon_action`'s pure
logic, not `install_and_start`'s real `lgs install-service` +
`launchctl bootstrap`/`kickstart` sequence, which is deliberately untested
in CI because it would touch a real launchd (see `ensure_daemon`'s own doc
comment). It has never been run end to end before this checklist item is
performed for the first time.

**Known failure mode:** `install_and_start` runs three sequential
subprocesses (`lgs install-service`, `launchctl bootstrap`, `launchctl
kickstart`); a partial failure partway through (e.g. `bootstrap` succeeds
but `kickstart` fails) can leave a loaded-but-never-started job — check
`launchctl print gui/$UID/com.local-git-sync.daemon` distinguishes this from
a clean success, not just that the command returned exit 0.

### 7. A wedged daemon during CONNECT

**A known open gap, not a hypothetical — confirm the observed behaviour
rather than assuming it.** `fetch_lgs`/`push_lgs`
(`backend/storage/git/mod.rs`) bound an in-flight transfer with a captured
deadline checked from inside libgit2's progress callbacks
(`transfer_progress`, `push_negotiation`, `sideband_progress`). But
`transfer_progress` only fires once the indexer starts receiving objects,
and `push_negotiation` only fires once, after the server's ref
advertisement — neither callback exists yet if the daemon is wedged
*before* that point (mid-TCP-connect, or hung mid-HTTP-handshake before
sending any advertisement). Confirm this really is unbounded on both sides.

- [ ] Simulate a daemon that accepts a connection but never responds (e.g.
      point the `lgs` remote at a bare `nc -l <port>` listener, or a process
      that `SIGSTOP`s the real daemon right after it accepts the socket but
      before it writes anything). Do this against a disposable local bare
      repo / throwaway daemon, never the real one.
- [ ] Attempt a `fetch_lgs` against it. Record whether the call ever
      returns (and if so, after how long) or hangs indefinitely.
- [ ] Attempt a `push_lgs` against it (before any advertisement is sent
      back). Record the same.

**PASS** here means confirming the documented behaviour, not fixing it: a
hang before the first callback fires blocks the calling thread — the
background sync thread in production — indefinitely, with no 30-second
`NETWORK_TIMEOUT` protection despite the constant's name suggesting
otherwise. Record how long you actually waited before giving up (you do not
need to wait forever to confirm "no callback fired and no timeout tripped
within N minutes" is different from "eventually resolves").

**Known failure mode:** because this runs on the app's one background sync
thread (`run_child_sync_cycles`), a wedge here — not just during an
in-flight transfer, but during the earlier connect/advertise phase — takes
the AWS poll and the 30-second sync timer down with it, per the existing
doc comment in `backend/storage/git/mod.rs` around `NETWORK_TIMEOUT`. This
checklist item exists to turn "the comment says this is possible" into "here
is what actually happens and how long it actually hangs."

### 8. No stray `refs/sync-check/*` ref after "Check sync"

- [ ] Open Settings → "Sync with another Mac" for a child that has a real,
      registered lgs project. Click "Check sync" and let it complete
      (either a full pass, or any failure — both paths matter here).
- [ ] Inspect the remote directly (e.g. `git ls-remote <lgs-remote-url>` or
      equivalent) and confirm no `refs/sync-check/probe` ref remains after
      a **successful** run — Cleanup deletes it both locally and on the
      remote.
- [ ] Confirm `lgs status --json` (or the app's own status display) still
      reports the project as backed up afterward — "Check sync" must not
      leave the project looking unhealthy or out of sync as a side effect
      of probing it.
- [ ] Repeat at least once after deliberately interrupting a run partway
      (e.g. disconnect from the daemon between Push and Fetch, if you can
      arrange it, or just run it while the daemon is briefly unreachable)
      and confirm that even in the worst case, any leftover
      `refs/sync-check/probe` ref is inert — check that a subsequent normal
      "Check sync" run cleans it up via its own forced push (self-healing),
      rather than requiring manual remote surgery.

**PASS** looks like: after a clean run, `refs/sync-check/probe` does not
exist on the remote and the project's normal backed-up status is unchanged.
After a deliberately-interrupted run, at worst a `refs/sync-check/probe` ref
is left (never a branch ref, never anything under `refs/heads/*` or
`refs/lgs-auth/*`), and a follow-up "Check sync" run overwrites/removes it
via its own forced push.

**Known failure mode:** the design was revised specifically to keep
"Check sync" off the child's real branch after an earlier version left
permanent add/remove commit pairs in the family's actual financial history
on every click. This item exists to confirm that revision holds in
practice, on a real daemon and a real remote, not just in the local-bare-
repo integration tests that exercise the same code path in CI.

## What CI already covers

Do **not** re-test these by hand — they have real, passing automated
coverage and a manual re-check adds risk (real data, real daemons) without
adding confidence:

- **The merge's five properties** (`allowance-core/tests/properties.rs`):
  symmetry, idempotence, the fixed point (proves re-merging converges),
  byte-stable round-trip, and post-merge balance validity — all via
  property-based tests over synthetic rows and provenance, plus a
  timezone-determinism test.
- **The codec round-trip**: parsing and rendering transactions is proven
  byte-stable and timezone-independent by the same property test file.
- **The cloud-path guard table** (`backend/sync/paths.rs` and its tests):
  the predicate that stops this app from ever writing inside a
  cloud-drive-backed path outside the one directory lgs owns.
- **Migration failure-at-each-step** (Task 18's tests): adopt-or-init,
  copy-then-repoint, and registry-written-last are each exercised with an
  injected failure at every step, confirming resumability.
- **"Check sync" stage sequencing** (`backend/sync/child_sync.rs`'s
  integration tests): the eight-stage sequence, its stop-at-first-failure
  behavior, and its cleanup-on-every-failure-path behavior are all proven
  against a fake `lgs` script and a local bare repo — everything except
  what a *real* daemon and a *real* remote do, which is item 8 above.

**Two-machine convergence is NOT covered — correction (2026-09-19):**
`two_machine_sync` (`egui-frontend/tests/two_machine_sync.rs:103`) and
`codec_real_data` (`egui-frontend/tests/codec_real_data.rs:18`) are both
`#[ignore]`d, and `.github/workflows/ci.yml` runs plain
`cargo test --workspace` — no `--ignored`, no `LGS_BINARY` — so **neither
has ever run in CI.** Do not treat the two-machine path as covered. Even
when run by hand, that harness commits `ledger.txt` through raw
`GitManager` calls — it never drives `ChildSyncEngine` or `apply_merge`, so
no automated test anywhere converges two app instances over real financial
data through the real code. That is the structural reason both dirty-tree
defects shipped.

Adding a CI job that installs `lgs` and runs
`cargo test --workspace -- --ignored` is tracked as follow-up 2 in the
dirty-tree resolution spec. An ignored test that never runs is
documentation, not coverage.

## Known unverifiable limitations

Unlike "The checks" above, these cannot be turned into a hand-test run at
all — there is no procedure that would confirm or refute them, so they are
recorded as accepted risk rather than given `- [ ]` steps and a "PASS looks
like" criterion.

### Power-loss durability of `atomic::write`

**Not verifiable in CI, and deliberately not claimed.** `atomic::write`
guarantees that no reader observes a partial file (from `rename(2)`). It does
NOT guarantee the most recent write survives a power cut: `fsync(2)` on macOS
does not flush the drive's volatile cache, and `F_FULLFSYNC` was rejected as
too expensive per write. Killing the process with SIGKILL proves nothing
either — the page cache outlives the process. Accepted unverified.

## Run history

Append one block per run. Do not edit earlier blocks.

### Run 1 — not yet performed

| Field | Value |
|---|---|
| Date | — |
| Performed by | — |
| Machine A | — |
| Machine B | — |
| macOS version(s) | — |
| Proton client version | — |
| Cloud root | — |
| Commit | — |

**Observed File Provider behaviour (item 1):** _not yet recorded_

**Results (items 1-8):** _not yet performed_

**Follow-ups filed:** _none_
