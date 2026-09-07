# Desktop-to-desktop sync over lgs

**Date:** 2026-08-29 (revised 2026-09-06 after reviewer panel)
**Status:** design, revised post-review; one blocking spike before planning
**Supersedes the diagnosis in:** `docs/superpowers/NEXT-desktop-sync-bug.md`
**Follow-on spec (not this one):** AWS sync correctness — see [Deferred](#deferred-spec-2--aws-sync-correctness)
**Reviews:** `reviews/2026-08-29-lgs-desktop-sync-design/` (Deiko, Greg, Ted)

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

- **Two** Macs converge on the same child data, without a terminal.
- No silent data loss and no silently wrong balances, including on concurrent edits.
- Adding a child requires no manual setup step on any machine.
- A child's data is an independently owned unit — restorable, archivable, and
  shareable on its own.

Three or more machines is explicitly **not** claimed. Every property below is
two-way, and "edit beats delete" is not obviously confluent across three peers
with repeated merges. Widening the goal requires proving confluence first.

## Non-goals

- Fixing the AWS sync path. It keeps running as the MCP window; its defects are
  [deferred to spec 2](#deferred-spec-2--aws-sync-correctness).
- Preserving the existing per-child git history. Explicitly out of scope by decision.
- Real-time sync. Convergence is bounded by the lgs daemon's poll interval plus
  cloud-drive latency, on both machines.
- Syncing machine-local state (registry, watermarks, retry queue, parental-control attempts).
- Extracting all of `backend/` into its own crate. Only the merge and the pure
  balance arithmetic move; see [Testing](#testing).

## Blocking spike before planning

**Does this build's libgit2 speak smart-HTTP to a local lgs daemon?**

`git2 = { version = "0.19", default-features = false }`
(`egui-frontend/Cargo.toml:48`) — no `https`, no `ssh`. lgs serves
`http://localhost:<port>/<name>.git`. Every transport claim below depends on
clone, fetch, **and push** (`receive-pack`) working under those flags.

Run it, and write the result into this section before planning. If it fails, the
two exits are: enable `https` and accept the build weight, or shell out to `git`
for transport and drop the remaining zero-dependency language.

## Architecture

Each child is one git repo, registered as one lgs project.

```
Machine A working repo  --git push-->  A's bare  --lgs daemon-->  cloud root
                                                                       |
                                                                  lgs daemon
                                                                       v
Machine B working repo  <--git fetch-- B's bare  <-----------------  cloud root
```

The critical mechanical fact, verified in `local-git-sync/src/daemon/sync.rs`
(`sync_project`): **the lgs daemon syncs the bare repo to and from the cloud,
and never touches the working repo.** Layer 1 reconciles cloud into bare; then
`advance_with` publishes bare to cloud. Moving commits between the working repo
and the bare is `git push` / `git fetch`, and nothing in lgs does it. That is the
app's job, and it is why this spec requires app changes rather than
configuration.

### Component boundaries

| Unit | Responsibility | Depends on |
|---|---|---|
| `LgsClient` | `parse_status(&str)` (pure) + `run(&[&str])` (spawns the bundled binary) | bundled binary path, daemon socket |
| `GitManager` (extended) | `clone`, `fetch`, `push`, `merge_commit`, `merge_base`; injectable time source | git2 |
| `ChildSyncEngine` | fetch → classify → resolve provenance → merge → apply → push | `GitManager`, merge crate |
| `merge` (new crate) | `(base, Sided<Row>, Sided<Row>) → merged` | **nothing — no filesystem in its dependency graph** |
| `balance` (same crate) | pure `recompute(rows)` / `validate(rows)` | nothing |
| Migration | Relocate a cloud-drive child into an lgs-backed repo | `LgsClient`, `GitManager` |

No trait behind `LgsClient` — one implementation, one caller. The testable seam
is the pure parser, not an interface.

`merge` and `balance` live in a **separate crate with no GUI or filesystem
dependencies**, so "the merge does no I/O" is a compile error to violate rather
than a paragraph in a spec. This is where the correctness risk lives.

## Repo layout

Child repos live at:

```
~/Library/Application Support/Allowance Tracker/children/<child_id>/
```

Not `~/Documents`. Documents is not iCloud-synced on the current machine — `~/Documents`
is a real directory, not a symlink into the iCloud container, which is the reliable
signal — but `FXICloudDriveDocuments = 1` is sitting in Finder prefs, leaving the setup
one toggle away from the corruption lgs's README warns about. Application Support is
never cloud-synced.

Because this hides the data from Finder, the app provides a **Reveal in Finder** action.

**The `.allowance_redirect` mechanism is retired.** It exists so a child folder
can point at a cloud-drive location; lgs replaces that entirely. Registry paths
become real working repos.

The `Availability` / `Downloading` model (`backend/domain/child_availability.rs`)
is **narrowed, not retired**: the dataless branch stays reachable while migrated
and un-migrated children coexist, and is removed once no child resolves to a
cloud-drive path.

### The cloud-path guard

The single most important safety rule in this document. It is a **pure predicate
over injected paths**, not scattered `if`s reading `dirs::home_dir()` internally:

```rust
fn is_cloud_synced(candidate: &Path, env: &SyncPaths) -> Option<Reason>
```

Rejects: the configured lgs cloud root, anything under
`~/Library/Mobile Documents/`, and `~/Documents` when Desktop & Documents sync is
genuinely on (detected by `~/Documents` being a symlink — not by the Finder pref,
which is stale on this very machine). The `Reason` names which rule fired.
Table-driven test per reason plus the accept case.

One `SyncPaths { data_dir, children_root, lgs_binary, cloud_root }` is threaded
from startup. A guard that reads the environment internally cannot be unit
tested, so it gets verified once by hand and then silently regresses.

**Machine-local, never synced,** staying in `~/Documents/Allowance Tracker/`:
`children.yaml` (paths are machine-specific), `global_config.yaml`,
`sync_state.yaml` (AWS watermarks), `sync_retry_queue.yaml`, and
`parental_control_attempts.csv`.

> Flagged, not decided: failed-PIN attempts are therefore per-machine. If they
> should be visible on both Macs, that is a separate change.

## lgs integration

### The binary is bundled, then copied out

The app's build compiles `lgs` from a **pinned git dependency on a specific lgs
commit** and copies the binary into `Contents/Resources/` (via the existing
`[package.metadata.bundle]` `resources` list).

On first run the app copies it to a stable path:

```
~/Library/Application Support/Allowance Tracker/bin/lgs
```

This matters because `service::install()` writes the plist from
`std::env::current_exe()` (`service.rs:129`). A plist pointing inside the `.app`
breaks the moment the app is moved to `/Applications`, renamed, or updated —
launchd then retries a missing path forever under `KeepAlive`, and sync stops
with no signal.

### `git` is a runtime prerequisite

**An earlier draft of this spec claimed the app has "no command-line
dependencies at all". That was false.** lgs is a `git`-CLI wrapper:
`repo/mod.rs:25,41,66`, `durability/bundles.rs`, `durability/manifest.rs`,
`daemon/sync.rs` and `cli.rs:724` are all bare `Command::new("git")`, and the
smart-HTTP endpoint spawns `git http-backend`. `lgs restore` shells out to
`git clone`. Bundling lgs bundles something that needs git.

On a Mac without Xcode Command Line Tools, `/usr/bin/git` is a stub that raises a
GUI dialog and exits non-zero. The app therefore **detects git at startup and
says so in plain language** when it is missing. We do not bundle a git.

git2 being statically linked covers only *our* calls, not lgs's.

### Daemon: adopt, upgrade only what we own

On startup the app checks for a running daemon.

- **Daemon present** → adopt it. Never reinstall, never overwrite its plist.
- **No daemon** → install, and **record that we own it**.

The app upgrades **only a daemon it installed**. This closes a trap that the
naive "adopt, don't reinstall" rule creates: the bundled CLI advances with every
app release, an adopted daemon never does, `outdated` becomes the steady state,
and since we refuse to claim backed-up while `outdated` holds, the app would
never report a project as backed up again.

For an adopted daemon we carry a **documented minimum version** and a handshake.
Below the floor, the app says what to upgrade and stops claiming durability — it
does not silently reinstall over someone else's install.

Installing means writing the plist *and* running `launchctl bootstrap
gui/<uid>` + `kickstart` ourselves. `cli::install_service` (`cli.rs:1034-1053`)
writes the plist and then **prints** those commands for a human to run; nothing
loads and nothing runs until the next login. Relying on it would hand a
non-technical user a plist, no daemon, no clone URL, and a terminal command as
the remedy — at the first step of first run.

**lgs-side dependency**, named explicitly because this spec cannot be delivered
without it: idempotent `install-service` (today `service::install()`
unconditionally overwrites the plist under a shared launchd label), and a defined
upgrade path for an app-installed daemon.

### Degraded mode is a feature, and we claim it

With no daemon, or no cloud drive, **commits still land locally and the app is
fully usable.** Only cross-machine replication stalls. Nothing is lost; the
unpushed commits carry forward.

### First run

1. Folder picker for the cloud root → `lgs init --cloud-root <path>`.
2. git check; daemon adopt-or-install as above.
3. Register or adopt children.

The cloud drive client (Proton Drive, iCloud) remains a prerequisite the app
cannot install. When `cloud_root_exists` is false, say so plainly.

### Health must be surfaced, not swallowed

The app **relays `daemon.message` verbatim** and does not claim a project is
backed up while `outdated` holds. Health is modelled as an enum with
`#[serde(other)] Unknown` — required, not optional: lgs's own tests feed
`"a_variant_from_the_future"` (`cli.rs:1788`).

`durability` and `failed_sync_attempts` are separate signals, reported
separately. A project can be `backed_up` with a non-zero failure count. A
non-zero count is expected for roughly 40 minutes after a reboot while the Proton
mount warms up.

### The remote URL is re-resolved, never frozen

`clone_url` is `http://localhost:<port>/<name>.git` — it contains the daemon's
port, which is configurable. Writing it into `.git/config` once at migration
means a port change breaks push forever with a confusing error. The app
re-resolves from `lgs status --json` on startup and reconciles the remote through
one shared function:

```rust
fn ensure_lgs_remote(repo: &Repository, url: &str)
```

used by **both** migration and onboarding, so exactly one remote name exists in
the system.

### Project naming

`allowance-<child_id>` — e.g. `allowance-keiko_hart`. lgs project names are a
flat global namespace shared with unrelated projects, so the prefix makes
provenance obvious in `lgs status` and cannot collide later.

## Money

`amount` and `balance` become `Money(i64)` — integer cents — at the domain
boundary, with `From`/`TryFrom` conversions and a **canonical 2-decimal
renderer** replacing `f64::to_string()`.

`f64` money is not merely a style objection here. Float addition is not
associative, so two machines applying the same row set in different orders
produce byte-different balance strings; `validate_all_balances` compares with a
`0.001` epsilon and would pass both. The convergence property would then be
tested by something that cannot detect the failure it exists to catch — and the
predictable repair is to widen the epsilon.

Canonical rendering also removes a whole class of byte-divergence independent of
the arithmetic.

**Blast radius, stated because it is not small.** The type that changes is the
**domain** `Transaction` (`backend/domain/models/transaction.rs`), and it is not
only the CSV model: `read_entity_for_sync` (`app_coordinator.rs:512-526`)
serializes it directly as the AWS wire payload, which the MCP Lambda in the
zephytop-brain stack then reads. The serde representation must therefore stay
**wire-compatible** — same JSON shape, still accepting values already stored in
DynamoDB — so no cross-repo change is forced. Internal representation changes;
the wire does not. A round-trip test against a captured production payload
enforces this.

`validate_all_balances` currently returns `Ok(Vec<String>)`, so it returns `Ok`
when balances are wrong and a `?` at the call site swallows it. It becomes
`Result<(), Vec<BalanceMismatch>>`.

## Canonical form

Two machines must agree on **bytes**, not merely on values. Three separate
mechanisms are required:

1. **Total row order `(date, id)`**, applied on **every write**, not only after a
   merge. Today file order is insertion order — `store_transaction` pushes to the
   end — so two machines holding the same rows can write different files.
2. **The same total order in the balance accumulation.** `recalculate_balances_from_date`
   sorts by date with **no id tiebreak**; Rust's sort is stable, so same-timestamp
   rows get balances assigned in file read order, which is per-machine insertion
   order. Same rows, different money, and `validate_all_balances` passes on both
   machines.
3. **No `Utc::now()` fallback.** `parse_date_string` (`transaction_repository.rs:128-130`)
   turns an unparseable date into the current time. One malformed row then makes
   read-modify-write non-idempotent: the file changes on every cycle and both
   machines re-merge forever. A parse failure is now a **hard error** before a
   merge, never a fresh timestamp. Date-only values must not resolve through
   `chrono::Local` either — a date-only row would otherwise parse differently in
   two timezones.

One CSV codec, not two. The merge reads git **blobs** (in memory, no path) while
all parsing today lives inside path-bound repositories — which invites a second
parser that drifts. Extract free functions `parse_transactions(&str)` and
`render_transactions(&[Transaction])`; the repository calls them.

## Sync lifecycle

### Thread ownership

The existing sync thread touches **zero** repository files by design —
`sync_manager.rs:37-40` states "UI owns all repo I/O", and the thread does
synchronous channel round-trips precisely to avoid it. That invariant is
preserved rather than replaced:

| Work | Thread | Touches |
|---|---|---|
| `fetch`, `push` | background | `.git` objects and refs only; libgit2 locks refs |
| provenance walk, `merge`, `recompute` | background | nothing — pure CPU |
| write merged files, commit | **UI thread** | working tree |

All working-tree mutation lands on the UI thread in the existing
`handle_sync_messages` drain, via one new
`SyncMessage::ApplyMerge { child_id, merged }`.

No mutex, no in-flight flag, no second concurrency rule. This is fewer moving
parts than "serialize through the same path", which named no mechanism.

### Push

The commit already happens on every write (`commit_file_change`). A push to the
`lgs` remote follows it.

**No retry queue is needed.** Unlike the AWS path — which requires
`sync_retry_queue.yaml` because an unsent event exists only in memory — an
unpushed git commit *is* the queue: durable on disk, carried by the next
successful push. This includes a push interrupted mid-stream by `lgs restart`.

### Fetch — the refspec matters

**This is the detail that decides whether the design works at all.**

`reconcile` is documented as *"Never moves a head backward or over a
divergence"* (`engine.rs:399`). Trace the concurrent case: A commits `a1` and
publishes; B has committed `b1`. B's daemon reconciles, finds neither is an
ancestor of the other, marks the branch diverged, and **leaves `refs/heads/main`
in B's bare at `b1`**.

A fetch of `refs/heads/*` therefore returns *B's own tip*. The app classifies
"up to date", never merges, and `advance_once` refuses to publish forever. Both
machines stall permanently and invisibly — the exact outcome this spec exists to
prevent.

lgs already solved this. `reconcile` writes the authoritative tip to
`refs/lgs-auth/heads/<branch>` **before** the divergence check, and the objects
are already imported. So the refspec is:

```
+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*
+refs/heads/*:refs/remotes/lgs/*
```

and **the merge input is `refs/remotes/lgs-auth/main`**, not
`refs/remotes/lgs/main`.

### Classify

Triggered on startup, on window focus, and on a 30-second timer — the same
triggers the AWS poll loop already uses.

| State | Action |
|---|---|
| Up to date | nothing |
| Fast-forward | check out (UI thread), reload, repaint |
| Diverged | merge, below |

Push rejection because the remote moved between fetch and push retries within a
**bounded** cap per cycle, then leaves the child for the next scheduled pull
rather than spinning.

### Crash mid-merge

If the app dies after writing merged files but before the merge commit, startup
finds a dirty tree on a diverged branch. It **discards the working-tree changes
and re-runs the merge** — safe precisely because the merge is deterministic.

## The merge

### A textual merge is disqualified

`Transaction` carries a running `balance` ("account balance after this
transaction", `shared/src/lib.rs:24-38`). If both machines add a transaction and
git merges the CSVs as text, the merged file has correct rows and **silently
wrong balances** for every row after the insertion point, and git reports
success. This rules textual merge out; it is not a preference.

### Signature — provenance is passed in, not looked up

```rust
merge(base, ours: Sided<Row>, theirs: Sided<Row>) -> Merged
```

`Sided` pairs each row with `(committer_epoch, commit_oid)`, **resolved by the
caller**. The caller does the git walk; the merge stays a total function over
data.

An earlier draft said "later committer timestamp wins" without saying later than
*what*. The two readings differ: tip-of-side lets B editing an unrelated *goal*
win a `transactions.csv` row conflict, while per-row provenance would require
commit history inside `merge` and break its no-I/O boundary. Passing provenance
explicitly resolves both.

### `balance` is excluded from comparison

Rows compare on intrinsic fields only: `id`, `child_id`, `date`, `description`,
`amount`, `type`. `balance` is regenerated output.

Without this, a single non-tail insert on machine A rewrites the balance column
of every subsequent row, so most of the file reads `changed` against the base and
the conflict rule fires across rows whose only difference is a derived column
about to be recomputed anyway.

**With `balance` excluded, genuine `changed/changed` conflicts become rare — and
that is what makes the crude resolution rule acceptable.**

### Row resolution

| In base | In ours | In theirs | Result |
|---|---|---|---|
| — | yes | — | add — keep |
| — | — | yes | add — keep |
| — | yes | yes, identical | one row — keep either |
| — | yes | yes, **differing** | **keep both**, re-key one deterministically |
| yes | gone | unchanged | delete — drop |
| yes | unchanged | gone | delete — drop |
| yes | gone | changed | keep the change (edit beats delete) |
| yes | changed | gone | keep the change (edit beats delete) |
| yes | changed | changed | resolve by rule below |

**Base-absent add/add and base-present edit/edit are different situations.** An
edit/edit is one row edited twice — pick one. An add/add is *two distinct rows
that collided on a key*, and picking one destroys a real transaction. Re-keying
appends the short commit hash of the side being re-keyed, so both machines choose
the same loser.

> **Correction to the review finding.** Greg's critique cited
> `shared/src/lib.rs:581` (`transaction::{type}::{millis}`, no suffix). That
> generator is **dead** — it is referenced only by its own unit tests. The live
> one is `DomainTransaction::generate_id`
> (`backend/domain/models/transaction.rs:29`), used by `transaction_service.rs:137,462`
> and `balance_service.rs:334`, and it already appends a 4-hex suffix:
> `in-1625846400123-af3c`.
>
> The concern survives in weakened form. `generate_random_suffix` is **not
> random** — it is `SystemTime::now().as_nanos() % 16^4`
> (`models/transaction.rs:49-56`), so it is a second clock reading, not entropy,
> and two machines are correlated in exactly the way the suffix is meant to
> break. The collision is far less likely than "same millisecond", but the
> consequence is unchanged: two real transactions silently become one.
>
> So the table split above is the load-bearing fix and stays. The generator
> change is narrowed to *making the suffix actually random*, and the dead
> `shared::Transaction::generate_id` is deleted rather than aligned, so no future
> reader mistakes it for the live path.

`Goal` id generation is checked for the same defect.

**When there is no merge base**, treat the base as empty and union both sides.
With no common ancestor nothing can be shown to have been deleted, so nothing is
dropped. (Migration now adopts an existing lgs project rather than creating an
unrelated root, so this case is rarer than in the first draft — see
[Migration](#migration).)

### The resolution rule

**Later committer timestamp wins; ties broken by commit hash**, read from the
provenance passed in.

"Prefer ours" would diverge *permanently*: A merges with ours=A, B with ours=B,
and the machines settle on different answers forever.

Stated honestly: this is **clock-skew-sensitive last-writer-wins at branch
granularity**. A Mac running 20 seconds fast wins contested rows. That is
acceptable only because excluding `balance` makes genuine conflicts rare.

### Then recompute

Pure functions, on the in-memory row set:

```rust
fn recompute_running_balances(rows: &mut [Transaction]);
fn validate(rows: &[Transaction]) -> Vec<BalanceMismatch>;
```

**Not** `recalculate_balances_from_date`. That goes through
`update_transaction_balances`, which calls `find_child_id_for_transaction` **per
row** (`transaction_repository.rs:384-392`), each parsing every child's entire
CSV — a 500-row merge becomes 500 × N full parses, on a 30-second timer. It also
fires a `SyncEvent` per changed row into the AWS notifier
(`balance_service.rs:85-96`), amplifying every merge into a burst of HTTP PUTs on
both machines.

`recalculate_balances_from_date` is refactored to call the same pure function, so
there is one implementation of the arithmetic — the original goal, reached
without the I/O. The merge path builds `BalanceService` with
`.with_sync_notifier(None)`.

**A validation failure aborts the merge before commit.** Money that fails its own
check does not reach the remote.

The result is committed as **one** two-parent merge commit whose tree is already
canonical and rebalanced. A separate recompute commit would produce a tree that
never satisfies the convergence property.

### No conflict UI for git merges

Every case resolves by rule, preserving the no-terminal goal. Merge decisions —
including which version was discarded and why — are logged to the app log with
the child id and both commit oids, retained for the life of the log file.

The AWS `SyncMessage::ConflictDetected` path is untouched; it belongs to the
deferred AWS spec.

## Two transports over one dataset

AWS keeps writing the same CSVs as the MCP window. Serialization alone is not
sufficient — three consequences need naming:

1. **Independent commits per machine.** `ApplyRemoteEntity` runs on the UI thread
   and writes through repositories that call `commit_file_change`. One MCP write
   produces a *separate commit on each machine* for the same logical change, so
   divergence is the steady state whenever MCP is active. **Git commits are
   suppressed on the `ApplyRemoteEntity` path**; the merge/recompute produces one
   commit instead.
2. **Write amplification.** Merge-driven recalculation is run with the notifier
   suppressed, so a merge does not push a burst of `Updated` events at AWS.
3. **Resurrection.** A row deleted on A and correctly dropped by the merge on B
   can be re-created on both by an AWS replay, then committed and pushed — and
   with no base entry the next merge reads it as an add and keeps it. A deleted
   transaction comes back and stays. The deferred AWS spec owns the real fix
   (event-log wipe plus acknowledged apply); until then this is a **known,
   documented** hole, not an unnoticed one.

## Migration

One-time, in-app, automatic, for any child whose registry path is a redirect stub
or sits inside a cloud-synced location:

1. **Check `lgs projects --json` for an existing `allowance-<child_id>`.** If it
   exists, `lgs restore` it and adopt — do not `git init` a fresh root. This is
   the two-machine ordering case: A migrates Monday, B on Friday; without the
   check B creates an unrelated history and any row A deleted in that window
   resurrects through the empty-base union.
2. Otherwise copy **data files only** — `child.yaml`, `allowance_config.yaml`,
   `goals.csv`, `transactions.csv` — to the Application Support path. The `.git`
   is deliberately left behind.
3. `git init`, canonicalize (order + money rendering), initial commit.
4. `lgs add`, resolve `clone_url`, `ensure_lgs_remote`, push.
5. **Repoint `children.yaml` last**, only after every preceding step succeeded.

Step 5 being last is the safety story: copy-then-repoint, never move. If any step
fails the registry is unchanged and the app keeps working off the old path
exactly as today.

**The old folder is never deleted.** It remains a frozen backup, and the app
raises a `StartupNotice` saying where it is.

This reuses the existing `plan_migration` / `MigrationReport` / `SkippedFolder`
discipline (`csv/migration.rs:44-93`) rather than inventing a second one.

## Onboarding the second machine

1. App finds no registered children.
2. Reads the adoptable list from `lgs projects --json`, filtered to `allowance-*`.
3. Shows them as a checklist.
4. Per ticked child: `lgs restore <name> <dir>` — **which already clones**
   (`ensure_working_copy`, `cli.rs:700-733`) — then `ensure_lgs_remote` to
   normalize the remote name from `origin`, then write the `children.yaml` entry.

No second clone. `lgs restore` refuses a non-empty non-repo directory, so a git2
clone after it is either dead code or a collision.

**Edge case:** if the project is already registered here, `lgs restore` refuses
by design. Fall through to clone-or-pull; do not treat the refusal as an error.

**Archived projects** stay archived and read-only after restore, and pushes are
refused with a 403. Relay that rather than retrying; `lgs unarchive` is the fix.

## Check sync

A user-facing action that verifies the whole loop on demand: write a sentinel to
a scratch file in the child repo, commit, push, fetch, read back — reporting each
stage pass/fail **by name**.

This is the manual checklist's happy path, automated, one click, and usable by a
non-technical person over the phone. Given the no-terminal goal, health
*reporting* without a way to *exercise* the loop is half an answer.

## Testing

The valuable seam: **the sync logic does not care that the remote is lgs.** It
pushes and fetches a git URL, so most tests point at a plain bare repo on disk
and need no daemon.

The exception is divergence. A plain bare neither accepts nor rejects the way lgs
does, so the diverged case needs the real thing — which is why the harness below
matters.

Existing fixtures are extended, not duplicated: `egui-frontend/Cargo.toml:58`
already has `tempfile`, and `backend/storage/csv/test_utils.rs` has a real
`TestHelper` / `TestEnvironment` harness. (An earlier draft claimed the workspace
had no dev-dependencies. It was wrong.) `proptest` is added.

### Properties — over bytes, not values

| Property | What it establishes |
|---|---|
| `merge(A,B) == merge(B,A)` | symmetry — permanent divergence cannot recur |
| `merge(A,A) == canonicalize(A)` | idempotence |
| **`merge(merge(A,B), B) == merge(A,B)`** | **fixed point — the re-merge loop cannot happen** |
| `render(parse(x)) == x` over a corpus incl. the real production `transactions.csv` | byte round-trip stability |
| `validate(recompute(rows))` is empty | no silently wrong money |

Symmetry alone is necessary and **not sufficient**: two machines can converge on
values and diverge on bytes forever. The fixed-point property is the one that
proves termination.

### Tests

| Test | What it establishes |
|---|---|
| Merge unit tests over `(base, ours, theirs)` | every row of the resolution table |
| Add/add with differing content | **both rows survive** — the id-collision data-loss case |
| Unrelated-later-commit test | B commits a goal edit after A's transaction edit; A's row wins — distinguishes tip-of-side from provenance |
| `TZ=UTC` vs `TZ=America/Los_Angeles` | identical bytes — timezone determinism |
| Committer-timestamp tie | exercises tiebreak-by-hash (needs the injectable clock) |
| Two-bare diverged integration | the refspec fix — a plain bare would pass and miss it |
| Convergence via `tree_checksum` (`csv/checksum.rs:17`) | identical **trees**, the strong assertion |
| Cloud-path guard, table-driven per `Reason` | the most important safety rule is covered |
| "No retry queue": kill remote, 5 writes, restore, assert all land in order | the claim is tested, not asserted |
| AWS/git serialization | testable now that all mutation is behind `ApplyMerge` |
| Crash mid-merge | dirty tree on a diverged branch recovers |
| Schema skew | a version marker refuses a merge from a newer schema — `parse_transaction_type` otherwise falls through to *derivation* (`transaction_repository.rs:97`), so an older app would silently downgrade a row type and push it |
| Migration failure at each step | registry unchanged, old data intact, app still works |
| `parse_status` against lgs's blessed fixture | parsing, and that `outdated` surfaces rather than being swallowed |

### `TwoMachineHarness`

Two machines do not need two Macs. `lgs::paths` resolves everything from `$HOME`
(`local-git-sync/src/paths.rs:6`), and lgs's own `spawn_test_daemon` already
supports two daemons sharing one `TempDir` cloud root. A full
A→bare→cloud→bare→B loop runs offline in CI with the real bundled binary, under
two `HOME`s.

One fixture — `machine_a()`, `machine_b()`, `cloud()`, `sync_both_ways()` — so
every convergence and onboarding case is three lines. Moved into CI:
`install-service` idempotence, `restore` refusal fall-through, archived-project
403, adoptable filtering.

Recorded `--json` fixtures are kept for **parse tests only**, sourced from lgs's
blessed `current-status-json.json` with the lgs commit pinned and CI failing on
drift. A hand-retyped fixture is a second representation of a contract: it keeps
passing after lgs changes `ProjectJson`, and the real binary breaks in the field.

### Manual acceptance checklist

What genuinely needs hardware: **Proton File Provider materialization,
launchd-at-login, and real clock skew.** That is a checklist someone will run.

It must record that our cold-start path inherits an unproven assumption —
`local-git-sync/docs/bootstrap-install-acceptance-checklist.md` still reads
"Run 1 — not yet performed", and its item 1 (File Provider materialization) is
described there as the weakest assumption the whole design leans on.

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

Closing it means making apply acknowledged rather than fire-and-forget — a change
to the thread protocol, in its own spec. That spec also owns the resurrection
channel named above.

Also deferred there: the event log may be wiped (approved), but wiping it alone
strands new devices — the server's upsert short-circuits on byte-identical
content and emits no event (`sync-service/src/storage/dynamo.rs`), so a
re-backfill produces nothing to replay, and `list_entities` is not on the
`RemoteStorage` trait (`backend/storage/remote.rs:5-12`). Under this spec that no
longer blocks desktop-to-desktop sync, which is why it is deferred rather than
solved here.

## Housekeeping noted in passing

`~/Library/Mobile Documents/com~apple~CloudDocs/Desktop` and `Documents` are
hand-made symlinks pointing back at the real local folders. iCloud Drive does
not traverse symlinks, so they sync nothing, but they would resolve to whatever
local `/Users/<name>/Documents` exists on another machine. Unrelated to this
spec; worth deleting.

## Review Change Appendix

Changes prompted by the reviewer panel (full memos in
`reviews/2026-08-29-lgs-desktop-sync-design/`):

- **Fetch `refs/lgs-auth/heads/*`, not `refs/heads/*`** (Deiko): accepted —
  verified against `engine.rs:399`; without it both machines stall silently and
  invisibly on the first concurrent edit. The single most consequential finding.
- **Retracted "no command-line dependencies at all"; `git` declared a
  prerequisite** (Deiko, Greg): accepted — lgs shells out to `git` throughout.
- **App runs `launchctl bootstrap`/`kickstart`; binary copied out of the bundle**
  (Deiko): accepted — `install-service` only prints instructions, and the plist
  uses `current_exe()`.
- **Upgrade only a daemon we installed** (Deiko): accepted — "adopt, don't
  reinstall" alone would make `outdated` permanent, so the app would never report
  backed-up again.
- **Canonical `(date, id)` order on every write; ordered recompute** (Deiko, Ted):
  accepted — file order was insertion order and the balance sort had no tiebreak.
- **`balance` excluded from conflict comparison** (Deiko): accepted, and his
  corollary — that this makes genuine conflicts rare, which is what justifies the
  crude rule — is now the spec's stated rationale.
- **"Two transports over one dataset" section** (Deiko): accepted, including the
  resurrection channel as a documented hole.
- **lgs-side work named as an explicit dependency** (Deiko): accepted.
- **Background thread does fetch/push only; all working-tree mutation on the UI
  thread via `ApplyMerge`** (Greg): accepted — preserves the existing "UI owns
  repo I/O" invariant instead of adding a mutex. Fewer moving parts than the
  original.
- **git2-over-HTTP promoted to a blocking spike** (Greg): accepted —
  `default-features = false` may not speak smart-HTTP push.
- **Remote URL re-resolved, not frozen** (Greg, Deiko): accepted — `clone_url`
  carries the port.
- **Onboarding double-clone removed; one `ensure_lgs_remote`** (Greg): accepted —
  `lgs restore` already clones and names the remote `origin`.
- **add/add split from edit/edit; id generator gains a random suffix** (Greg):
  accepted — same-millisecond ids across machines would have silently destroyed a
  real transaction, against this spec's own goal.
- **Pure `recompute_running_balances`, not `recalculate_balances_from_date`**
  (Greg, Ted): accepted — the latter is quadratic I/O and fires an AWS event per
  row.
- **`Money(i64)` newtype with canonical 2-decimal rendering** (Greg): accepted,
  and taken further than proposed — Greg suggested integer cents in the proptest
  strategy as a minimum; the author chose the newtype. Wire compatibility with
  the MCP Lambda is required.
- **`validate_all_balances` returns a `Result`** (Greg): accepted — it returned
  `Ok` when balances were wrong.
- **One CSV codec: `parse_transactions` / `render_transactions`** (Greg):
  accepted.
- **No trait behind `LgsClient`; health enum with `#[serde(other)]`; one open
  `Repository` per cycle; reuse `plan_migration` and `tree_checksum`; guard as
  one typed function** (Greg): all accepted.
- **Provenance passed into `merge` explicitly via `Sided<Row>`** (Ted): accepted
  — "later committer timestamp" was undefined, and per-row provenance would have
  broken the no-I/O boundary.
- **Four properties over bytes, incl. fixed point** (Ted): accepted — symmetry
  alone permits converging on values while diverging on bytes forever.
- **`Utc::now()` date fallback removed** (Ted): accepted — it made
  read-modify-write non-idempotent, so one malformed row caused an infinite
  re-merge.
- **`SyncPaths` injection; guard as a pure predicate; injectable clock** (Ted):
  accepted — the most important safety rule was untestable as written.
- **`TwoMachineHarness` in CI; manual checklist shrunk to File Provider,
  launchd, clock skew** (Ted): accepted — lgs's own harness already runs two
  daemons over one cloud root.
- **Test table gaps closed** (Ted): accepted — guard, retry-queue claim,
  AWS/git serialization, crash mid-merge, schema skew.
- **"Check sync" action** (Ted): accepted.
- **Merge + pure balance arithmetic extracted to their own crate** (Ted):
  **compromise** — Ted argued for extracting all of `backend/`; the author scoped
  it to the merge and balance modules, putting the compile-error boundary where
  the risk is. Cost: other backend tests still compile wgpu.
- **Goal narrowed to two machines** (Ted): accepted — every property is 2-way and
  "edit beats delete" is not obviously confluent across three peers.
- **Corrected "the workspace has no dev-dependencies"** (Ted): factually wrong;
  `tempfile` and a real `TestHelper` fixture already exist.
