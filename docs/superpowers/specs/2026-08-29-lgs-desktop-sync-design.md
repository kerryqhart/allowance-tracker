# Desktop-to-desktop sync over lgs

**Date:** 2026-08-29
**Status:** design, approved in brainstorm; not yet planned
**Supersedes the diagnosis in:** `docs/superpowers/NEXT-desktop-sync-bug.md`
**Follow-on spec (not this one):** AWS sync correctness — see [Deferred](#deferred-spec-2--aws-sync-correctness)

## Why this exists

Two Macs running this app do not see each other's edits.

`NEXT-desktop-sync-bug.md` diagnosed that as a defect in the AWS sync protocol:
`SyncSource` has two values (`shared/src/sync.rs:56-60`), every desktop stamps
its own writes `X-Sync-Source: local` (`backend/storage/http_remote.rs:64,104`),
the server records that verbatim (`sync-service/src/routes/entities.rs:30-33`),
and every desktop then skips those events on pull (`SyncEngine::poll_child`).
The proposed fix was to give the protocol a device identity.

**That diagnosis rests on a premise that is wrong.** AWS was never intended to
carry desktop-to-desktop traffic. It is the MCP server's window into the data.
Machine-to-machine replication was always meant to run over a cloud-drive path.

The real transport is already configured and already broken:

```yaml
# ~/Documents/Allowance Tracker/children.yaml
- id: keiko_hart
  path: /Users/kerryhart/Library/Mobile Documents/com~apple~CloudDocs/
        HartRoot/Parent Portal/Allowance Tracker/keiko_hart
```

That folder contains `child.yaml`, `allowance_config.yaml`, `goals.csv`,
`transactions.csv` — **and a live `.git`**. `GitManager` has no remote
operations at all (`backend/storage/git/mod.rs` exposes only `init`, `add`,
`commit`), and the repo has no remotes. So iCloud is replicating a working git
repository as loose files between two machines, with no ordering guarantee and
no merge. A partially-delivered `.git` is a corrupt repo, and two machines
committing to one branch produce histories git could merge but iCloud cannot.

lgs's own documentation names this exact failure
(`local-git-sync/scripts/cloud-root-README.md`):

> Do not put working repos under `~/Documents` or `~/Desktop` while macOS
> "Desktop & Documents Folders" syncing is on. iCloud writes conflict copies
> *inside* `.git`, which can corrupt the index, packs, or refs.

This spec replaces cloud-drive-as-transport with lgs.

## Goals

- Two or more Macs converge on the same child data, without a terminal.
- No silent data loss and no silently wrong balances, including on concurrent edits.
- Adding a child requires no manual setup step on any machine.
- A child's data is an independently owned unit — restorable, archivable, and
  shareable on its own.

## Non-goals

- Fixing the AWS sync path. It keeps running as the MCP window; its defects are
  [deferred to spec 2](#deferred-spec-2--aws-sync-correctness).
- Preserving the existing per-child git history. Explicitly out of scope by decision.
- Real-time sync. Convergence is bounded by the lgs daemon's poll interval plus
  cloud-drive latency, on both machines.
- Syncing machine-local state (registry, watermarks, retry queue, parental-control attempts).

## Architecture

Each child is one git repo, registered as one lgs project.

```
Machine A working repo  --git push-->  A's bare  --lgs daemon-->  cloud root
                                                                       |
                                                                  lgs daemon
                                                                       v
Machine B working repo  <--git pull--  B's bare  <-----------------  cloud root
```

The critical mechanical fact, verified in `local-git-sync/src/daemon/sync.rs`
(`sync_project`): **the lgs daemon syncs the bare repo to and from the cloud,
and never touches the working repo.** Layer 1 reconciles cloud into bare; then
`advance_with` publishes bare to cloud. Moving commits between the working repo
and the bare is `git push` / `git pull`, and nothing in lgs does it. That is the
app's job, and it is why this spec requires app changes rather than
configuration.

### Component boundaries

| Unit | Responsibility | Depends on |
|---|---|---|
| `LgsClient` | Wraps the bundled `lgs` binary: `init`, `install-service`, `add`, `restore`, `status --json`, `projects --json` | bundled binary path, daemon socket |
| `GitManager` (extended) | Adds `clone`, `fetch`, `push`, `merge_commit`, `merge_base` | git2 |
| `ChildSyncEngine` | Pull → classify (up-to-date / fast-forward / diverged) → merge → push | `GitManager`, merge module |
| `merge` (pure) | `(base, ours, theirs) → merged` for each file type | nothing — no I/O |
| Migration | Relocate a cloud-drive child into an lgs-backed repo | `LgsClient`, `GitManager` |

`merge` having no I/O is deliberate: it is where the correctness risk lives, and
it must be exhaustively testable without a filesystem, a daemon, or a network.

## Repo layout

Child repos live at:

```
~/Library/Application Support/Allowance Tracker/children/<child_id>/
```

Not `~/Documents`. Documents is not iCloud-synced on the current machine — `~/Documents`
is a real directory, not a symlink into the iCloud container, which is the reliable
signal — but `FXICloudDriveDocuments = 1` is set in Finder prefs, leaving the setup one
toggle away from the corruption lgs's README warns about. Application Support is never
cloud-synced.

Because this hides the data from Finder, the app provides a **Reveal in Finder** action.

**The `.allowance_redirect` mechanism is retired.** It exists so a child folder
can point at a cloud-drive location; lgs replaces that entirely. Registry paths
become real working repos.

**Guard:** the app refuses to register or migrate a child repo inside a
cloud-synced path — the configured lgs cloud root, anything under
`~/Library/Mobile Documents/`, or `~/Documents` when Desktop & Documents sync is
genuinely on (detected by `~/Documents` being a symlink, not by the Finder pref,
which is unreliable). Two replication systems over one set of bytes is the
failure this spec exists to end.

**Machine-local, never synced,** staying in `~/Documents/Allowance Tracker/`:
`children.yaml` (paths are machine-specific), `global_config.yaml`,
`sync_state.yaml` (AWS watermarks), `sync_retry_queue.yaml`, and
`parental_control_attempts.csv`.

> Flagged, not decided: failed-PIN attempts are therefore per-machine. If they
> should be visible on both Macs, that is a separate change.

## lgs integration

### The binary is bundled

The app's build compiles `lgs` and copies the binary into `Contents/Resources/`.
The app resolves it by absolute path inside its own bundle.

This is what makes the install path viable for a non-technical person. lgs's
documented setup — Xcode Command Line Tools, a Rust toolchain from rustup.rs,
`python3 bootstrap-lgs.py`, then `./scripts/install.sh` — is a developer machine
setup and is not a path anyone else will complete. Bundling removes all of it.
It also removes the `PATH` problem: a GUI `.app` launched from Finder gets a
minimal `PATH` that excludes `~/.cargo/bin` and `/usr/local/bin`, so "on PATH"
was never true for this app.

`GitManager` uses git2 — libgit2, statically linked (`backend/storage/git/mod.rs:34`)
— not the `git` binary. **With a bundled lgs, the app has no command-line
dependencies at all.**

The CLI is the integration surface rather than the `lgs` Rust library, even
though a `[lib]` target exists and `add` is a thin IPC call
(`local-git-sync/src/cli.rs:661-663`). The `--json` output is a documented
contract; `ProjectJson` carries an explicit comment saying so. The Rust API
makes no such promise, sits at `0.1.0`, and would couple this app's build to a
sibling checkout.

### Daemon: adopt, don't reinstall

On startup the app checks for a running daemon.

- **Daemon present** → use it. Do not install a service, do not start anything.
- **No daemon** → run `lgs install-service` (launchd, starts at login).

An already-installed service must make `install-service` a detected no-op.

### First run

1. Folder picker for the cloud root → `lgs init --cloud-root <path>`.
2. Daemon check as above.
3. Register or adopt children.

The cloud drive client (Proton Drive, iCloud) remains a prerequisite the app
cannot install. When `cloud_root_exists` is false, say so plainly rather than
failing obscurely.

### Health must be surfaced, not swallowed

The bundled CLI talks to whatever daemon is running, which may be a different
version — the `outdated` state. The app **relays `daemon.message` verbatim** and
does not claim a project is backed up while it holds. Reporting durability from
a skewed daemon is how a "backed up" claim becomes untrue.

`durability` and `failed_sync_attempts` are separate signals and are reported
separately. A project can be `backed_up` with a non-zero failure count; that
means the data is safe and there is nothing new to send. A non-zero count is
also expected for roughly 40 minutes after a reboot while the Proton mount
warms up.

### Project naming

`allowance-<child_id>` — e.g. `allowance-keiko_hart`. lgs project names are a
flat global namespace shared with unrelated projects, so the prefix makes
provenance obvious in `lgs status` and cannot collide later.

## Sync lifecycle

### Push

The commit already happens on every write (`commit_file_change`). A push to the
`lgs` remote follows it.

**No retry queue is needed.** Unlike the AWS path — which requires
`sync_retry_queue.yaml` because an unsent event exists only in memory — an
unpushed git commit *is* the queue: durable on disk, carried by the next
successful push. Push failure is logged and retried on the next cycle.

### Pull

Triggered on startup, on window focus, and on a 30-second timer — the same
triggers and interval the AWS poll loop already uses in
`backend/domain/sync_thread.rs`. After fetching:

| State | Action |
|---|---|
| Up to date | nothing |
| Fast-forward | check out, reload in-memory state, repaint |
| Diverged | three-way semantic merge, below |

### Ordering constraints

1. **No write may proceed while a merge is in flight.** Sync work runs off the
   UI thread and lands through the existing `SyncMessage` channel, so results
   apply at a frame boundary like every other async result.
2. **The AWS sync writes the same CSVs.** It is still running as the MCP window,
   so `ApplyRemoteEntity` and a git merge can race on one file. They serialize
   through the same path.

## The merge

### A textual merge is disqualified

`Transaction` carries a running `balance` — "account balance after this
transaction" (`shared/src/lib.rs:24-38`). There is no `updated_at`.

If both machines add a transaction and git merges the CSVs as text, the merged
file has correct rows and **silently wrong balances** for every row after the
insertion point, and git reports success. This is not a preference for semantic
merge; it rules textual merge out.

### Algorithm

On divergence, using the git merge base:

**Row files — `transactions.csv`, `goals.csv`.** Merge rows by `id` against the base:

| In base | In ours | In theirs | Result |
|---|---|---|---|
| — | yes | — | add — keep |
| — | — | yes | add — keep |
| yes | gone | unchanged | delete — drop |
| yes | unchanged | gone | delete — drop |
| yes | gone | changed | keep the change (edit beats delete) |
| yes | changed | gone | keep the change (edit beats delete) |
| yes | changed | changed | resolve by rule below |
| — | yes | yes | resolve by rule below |

Distinguishing a real delete from a not-yet-seen add is precisely why this is
three-way against the merge base rather than a two-way union.

**When there is no merge base**, treat the base as empty and union both sides.
This is a real case, not a theoretical one: if both machines migrate the same
child independently before either has synced, each creates its own `git init`
and the two histories are unrelated. The empty-base union is the only safe
reading — with no common ancestor, nothing can be shown to have been deleted, so
nothing is dropped.

**Document files — `child.yaml`, `allowance_config.yaml`.** Small whole-file
documents; the same resolution rule applies to the file.

### The resolution rule must be symmetric

**Later committer timestamp wins; ties broken by commit hash.**

"Prefer ours" would diverge *permanently*: A merges with ours=A, B merges with
ours=B, and the two machines settle on different answers forever. The rule above
yields an identical result on both machines regardless of which side each is
standing on. It keys off commit metadata because `Transaction` has no
`updated_at` to use instead.

### Then recompute balances

After the row merge, run `recalculate_balances_from_date`
(`backend/domain/balance_service.rs:42`) from the earliest changed date, then
assert with `validate_all_balances` (`:216`). Both already exist; the merge
reuses them rather than reimplementing the arithmetic.

Commit the result as a true merge commit — two parents, via git2 — and push.

### No conflict UI

Every case resolves by rule, which is what preserves the no-terminal goal.
Discarded versions are logged so a surprising outcome stays auditable.

## Migration

One-time, in-app, automatic, for any child whose registry path is a redirect
stub or sits inside a cloud-synced location:

1. Copy **data files only** — `child.yaml`, `allowance_config.yaml`,
   `goals.csv`, `transactions.csv` — to the Application Support path. The `.git`
   is deliberately left behind.
2. `git init`, initial commit.
3. `lgs add`, read `clone_url` from `lgs status --json`, `git remote add lgs`, push.
4. **Repoint `children.yaml` last**, only after every preceding step succeeded.

Step 4 being last is the safety story: this is copy-then-repoint, never move. If
any step fails the registry is unchanged and the app keeps working off the old
path exactly as today.

**The old folder is never deleted.** It remains a frozen backup, and the app
raises a `StartupNotice` saying where it is.

## Onboarding another machine

1. App finds no registered children.
2. Reads the adoptable list from `lgs projects --json`, filtered to `allowance-*`.
3. Shows them as a checklist.
4. Per ticked child: `lgs restore <name> <dir>`, clone via git2, write the
   `children.yaml` entry.

**Edge case:** if the project is already registered on this machine, `lgs
restore` refuses by design — the "both machines set up independently" case. Fall
through to clone-or-pull; do not treat the refusal as an error.

**Archived projects** stay archived and read-only after restore, and pushes are
refused with a 403. Relay that rather than retrying; `lgs unarchive` is the fix.

## Testing

The valuable seam: **the sync logic does not care that the remote is lgs.** It
pushes and pulls a git URL, so tests point at a plain bare repo on disk and need
no daemon, no cloud root, and no network.

| Test | What it establishes |
|---|---|
| Merge unit tests over `(base, ours, theirs)` | every row in the table above, add/add, edit/edit, delete/edit, delete-in-base |
| **Property: `merge(A,B) == merge(B,A)`** | the permanent-divergence bug cannot recur |
| Property: `validate_all_balances` passes after any merge | no silently wrong money |
| Convergence: two repos, randomized interleaved edits, sync both ways, assert identical trees | the end-to-end claim, entirely offline |
| Migration: fails at each step in turn | registry unchanged, old data intact, app still works |
| `LgsClient` against recorded `--json` fixtures | parsing, and that `outdated` is surfaced not swallowed |

The symmetry property is the one that matters most: it is exactly the class of
bug that passes every example test and fails in the field. `proptest` must be
added — the workspace has no dev-dependencies today.

**A manual acceptance checklist** covers what needs real hardware: two Macs, a
real Proton mount, a real login. It must record that our cold-start path
inherits an unproven assumption — `local-git-sync/docs/bootstrap-install-acceptance-checklist.md`
still reads "Run 1 — not yet performed", and its item 1 (Proton File Provider
materialization) is described there as the weakest assumption the whole design
leans on.

## Deferred: spec 2 — AWS sync correctness

Once desktops replicate over lgs, AWS is purely the MCP server's window. That
changes the answer to the original question: skipping desktop-authored events on
pull is approximately correct under this architecture, and **device identity may
not be needed at all.**

What remains genuinely broken, independent of transport:

**The watermark advances past events that were never applied.** `poll_child`
bumps `max_sequence` for every event it sees. `poll_remote` then fires
`ApplyRemoteEntity` at the UI thread over a channel and never learns whether it
landed; if `remote.get_entity` fails (`backend/domain/sync_thread.rs:323`) the
event is dropped with only an `Error` message, and the watermark has already
moved past it. An MCP-added expense can be lost silently and can never be
re-fetched.

Closing it means making apply acknowledged rather than fire-and-forget. That is
a change to the thread protocol and belongs in its own spec.

Also deferred there: the event log may be wiped (approved in brainstorm), but
note that wiping it alone strands new devices — the server's upsert
short-circuits on byte-identical content and emits no event
(`sync-service/src/storage/dynamo.rs`), so a re-backfill produces nothing to
replay, and `list_entities` is not on the `RemoteStorage` trait
(`backend/storage/remote.rs:5-12`). Under this spec that no longer blocks
desktop-to-desktop sync, which is why it is deferred rather than solved here.

## Housekeeping noted in passing

`~/Library/Mobile Documents/com~apple~CloudDocs/Desktop` and `Documents` are
hand-made symlinks pointing back at the real local folders. iCloud Drive does
not traverse symlinks, so they sync nothing, but they would resolve to whatever
local `/Users/<name>/Documents` exists on another machine. Unrelated to this
spec; worth deleting.
