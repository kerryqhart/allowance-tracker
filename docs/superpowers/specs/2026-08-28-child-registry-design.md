# Child Registry Design

**Date:** 2026-08-28
**Author:** Kerry (with Claude)
**Status:** Draft, pending implementation
**Repos touched:** `allowance-tracker` (backend + egui-frontend)
**Related:** [Bidirectional Sync](2026-04-17-bidirectional-sync-design.md), [Initial Sync Backfill](2026-04-04-initial-sync-backfill-design.md)

## Problem

Setting up the app on a second Mac is impossible through the UI.

Children are discovered by scanning the immediate subdirectories of a hardcoded base directory, `~/Documents/Allowance Tracker`, for folders containing `child.yaml` (`backend/storage/csv/child_repository.rs:50-97`). A child whose data lives outside that base — the normal case once the data is in iCloud Drive — is represented by a stub directory whose only content is `.allowance_redirect`, a file holding the real absolute path (`backend/storage/csv/connection.rs:75-100`).

On a fresh machine the base directory is empty, so:

- No children are discovered.
- **Settings → Data directory** is the only UI that can point at an existing folder, and it resolves its target through the *active child* (`backend/domain/data_directory_service.rs:41-46`). With no children there is no active child, so the modal cannot function. This is the chicken-and-egg the user hit.
- The workaround of creating a placeholder child and then repointing it fails anyway: `relocate_child_data_directory` refuses a non-empty target (`connection.rs:212-218`), and the target is the fully-populated iCloud folder.
- Sync cannot bootstrap either. `poll_remote` asks the UI thread for the list of *local* child IDs and polls only those (`backend/domain/sync_thread.rs:270-283`); zero local children means zero polls, forever. **Settings → Initial sync** is push-only backfill (`egui-frontend/src/ui/components/settings/backfill_modal.rs:38-104`) despite its name.

The only way onto a new machine today is hand-creating a stub directory, an `.allowance_redirect` file, and a `global_config.yaml` in Finder.

The underlying mistake is that *location* was modelled as a property derived from *identity*. A child folder is already fully self-contained — it holds `child.yaml`, `allowance_config.yaml`, `transactions.csv`, `goals.csv`, and its own `.git`. Nothing outside it is child-specific. The redirect stub exists purely because discovery is defined as "scan a fixed directory," so an out-of-tree child needs a forwarding address left in-tree.

## Goals

- Register a child by pointing at its folder. Adding an existing child on a new machine is a file-picker away.
- Make the registry the single source of truth for which children this machine knows about and where they live.
- Resolve `child_id → path` by map lookup instead of a directory scan with a YAML parse per entry, per call.
- Degrade legibly when a registered path is unavailable — never silently drop a child, never beachball on a cold iCloud folder.
- Migrate the existing install automatically, with no user action and no data movement.

## Non-goals

- **Remote bootstrap.** Teaching the client to `GET /entities/child` and reconstruct a child from sync events is explicitly out of scope. iCloud carries the data; sync-service stays a live-updates channel between machines that already have the child registered. This is a deliberate decision not to make first-run depend on the remote service.
- Changing the sync protocol, watermarks, or event model.
- Moving machine-local config out of `~/Documents/Allowance Tracker` into `~/Library/Application Support`. Considered and rejected: the registry holds machine-specific absolute paths and can never be synced, so the separation buys nothing today and costs a second migration.
- Multi-user or multi-parent accounts.
- Windows/Linux path handling beyond what exists (the dataless-file probe is macOS-specific and degrades to a no-op elsewhere).

## Background: three path conventions that happen to coincide

Path resolution is currently inconsistent, and the inconsistency is invisible only because of an accident.

`create_child` sets `child.id = generate_safe_directory_name(name)` (`backend/domain/child_service.rs:59-67`), so `Keiko Hart` becomes id `keiko_hart` stored in folder `keiko_hart`. `load_child_from_directory` then *enforces* that the id equals the containing directory name, failing hard otherwise (`child_repository.rs:120-127`).

On top of that identity, three different resolvers coexist:

| Caller | Resolves via |
|---|---|
| `GoalRepository` | passes `child_id` straight in as the directory name (`goal_repository.rs:72`) |
| `TransactionRepository` | re-derives `generate_safe_directory_name(child.name)` from the loaded child (`transaction_repository.rs:185-205`) |
| `ChildRepository`, `AllowanceRepository`, `ParentalControlRepository` | `find_child_directory_by_id`, a full base-dir scan parsing every `child.yaml` (`connection.rs:658-698`) |

These agree only because id, folder name, and sanitized display name are currently the same string. **Renaming a child breaks that**: `child.yaml` keeps the original id, but `TransactionRepository` would begin resolving to a folder derived from the *new* name, writing transactions to a directory that does not exist. This is a latent bug the registry removes by construction — one resolver, keyed on the immutable id.

### iCloud and dataless files

Verified on macOS 26 (Darwin 25.2.0):

```
SF_DATALESS  0x40000000  /* file is dataless object */    — sys/stat.h:359
find ~/Library/Mobile Documents/com~apple~CloudDocs -name "*.icloud"  →  0 results
```

The legacy `.icloud` bplist placeholder is not what modern iCloud Drive produces. An un-downloaded file is an APFS **dataless file**: present at its real path with real size and mtime, flagged `SF_DATALESS`. `stat()` reads its metadata *without* materializing it; the first `read()` traps to `fileproviderd` and blocks until the bytes arrive, or fails with an I/O error when offline.

Two consequences drive the design:

1. **Reading is the download trigger.** No `startDownloadingUbiquitousItem`, no objc2 FFI, no `brctl` (whose `download` subcommand no longer exists — the surviving verbs are diagnose/log/dump/status/accounts/quota/monitor).
2. **A blocking read on the UI thread is the hazard.** Children are loaded synchronously from render paths today (`child_selector.rs:36`). Pointed at a cold iCloud folder, egui freezes mid-frame — and hangs indefinitely when offline.

`stat` before `read` lets us know which case we are in before committing to a block.

## Design

### `children.yaml`

Lives beside `global_config.yaml` in `~/Documents/Allowance Tracker/`, alongside the other machine-local state (`sync_state.yaml`, `sync_retry_queue.yaml`, `parental_control_attempts.csv`).

```yaml
version: 1
children:
  - id: keiko_hart
    path: /Users/kerryhart/Library/Mobile Documents/com~apple~CloudDocs/HartRoot/Parent Portal/Allowance Tracker/keiko_hart
    label: Keiko Hart
```

- `id` — the child's immutable identity, matching `child.yaml`'s `id`. Also the sync-service partition key. The registry never invents it; it is read from `child.yaml` at registration.
- `path` — absolute path to the self-contained child folder. Machine-specific by nature.
- `label` — a **display cache only**, refreshed on every successful load. It exists so the picker can show "Keiko Hart" rather than a raw path while a folder is still downloading or unavailable. Never authoritative; `child.yaml` always wins.

Written atomically (temp file + rename), matching the existing convention in `set_active_child_directory` (`child_repository.rs:208-211`).

`global_config.yaml`'s `active_child_directory` becomes `active_child_id`. The reader accepts the old key for one release and rewrites on next save.

### `ChildRegistry`

A new module, `backend/storage/csv/child_registry.rs`, owning load/save/mutate of `children.yaml`. Its whole interface:

```rust
pub struct ChildRegistry { /* path + entries */ }

impl ChildRegistry {
    pub fn load(base_dir: &Path) -> Result<Self>;
    pub fn entries(&self) -> &[RegistryEntry];
    pub fn path_for(&self, child_id: &str) -> Option<&Path>;
    pub fn register(&mut self, entry: RegistryEntry) -> Result<()>;   // rejects duplicate id
    pub fn deregister(&mut self, child_id: &str) -> Result<()>;
    pub fn repoint(&mut self, child_id: &str, new_path: PathBuf) -> Result<()>;
    pub fn set_label(&mut self, child_id: &str, label: &str) -> Result<()>;
}
```

Registering an id already present is **rejected** with an error naming the existing entry's path. No silent dedup — two folders claiming the same child is a situation the user must resolve, not one the app should paper over.

### `CsvConnection` becomes a resolver

`CsvConnection` keeps the base directory (for machine-local files and as the default parent for new children) and gains the registry. Every `&str` directory-name parameter across the storage layer is replaced by `child_id`, and every path lookup routes through one method:

```rust
pub fn child_dir(&self, child_id: &str) -> Result<PathBuf>;   // registry lookup, no I/O
```

Deleted outright: `get_child_directory`'s redirect-following, `find_child_directory_by_id`, `relocate_child_data_directory`, `revert_child_data_directory`, `commit_redirect_file`, and the dead `new_default` (`connection.rs:33-70` — reads a base-level `.allowance_redirect` but is called from nowhere; `Backend::with_data_dir` goes straight to `CsvConnection::new`).

`generate_safe_directory_name` survives, but only where it belongs: minting an id and choosing a folder name for a *newly created* child. It is no longer a resolver.

The five repositories change mechanically: `get_child_directory(name)` / `get_goals_file_path(name)` / `get_transactions_file_path(name)` take a `child_id` and delegate to `child_dir`. `TransactionRepository::get_child_directory_name` and its name-derived fallback are deleted — the latent rename bug goes with them.

### Availability model

Loading a registered child yields one of four states:

```rust
pub enum ChildStatus {
    Available(Child),
    Downloading,                  // SF_DATALESS set — materialization in progress
    Unavailable(UnavailableReason),
}

pub enum UnavailableReason {
    PathMissing,                  // folder gone, drive unmounted, foreign home dir
    NotAChildFolder,              // no child.yaml present
    IdMismatch { found: String }, // child.yaml's id ≠ registry id
    ReadFailed(String),           // I/O error — offline, daemon gave up, permissions
    ParseFailed(String),          // malformed child.yaml
}
```

**A registry entry is never dropped automatically.** An unloadable child stays in the list, rendered with its cached label, its path, and the reason. It cannot be made active. The user gets *Retry*, *Locate…* (repoint the entry at a new path, keeping the id), and *Remove from this machine* (deregister only — the folder is never touched).

This is the behaviour that matters most on a fresh install: while iCloud is still pulling the folder down, the child must read as "Downloading from iCloud…", not vanish. A silently-empty picker is indistinguishable from the bug this whole change exists to fix.

### Loading off the UI thread

Render paths must never touch the filesystem. The app holds a roster in UI state:

```rust
pub struct ChildRoster { entries: Vec<(RegistryEntry, ChildStatus)> }
```

On startup, and whenever the registry changes, a worker thread walks the entries and reports results over an `mpsc` channel, waking the UI with `ctx.request_repaint()`. This reuses the `WakeUi` pattern already established for the sync thread (`egui-frontend/src/ui/app_state.rs:150-157`), so it introduces no new concurrency primitive.

For each entry the worker:

1. `stat`s `child.yaml`. Missing folder → `PathMissing`; missing file → `NotAChildFolder`.
2. Checks `SF_DATALESS` via `std::os::darwin::fs::MetadataExt::st_flags()` — no new dependency. If set, reports `Downloading` immediately so the UI can paint, then proceeds.
3. **Prefetches the whole folder**: reads `child.yaml`, `allowance_config.yaml`, `transactions.csv`, `goals.csv`. On a dataless folder these reads block and materialize the files. Only when all four are resolved does the entry become `Available`.

Step 3 is what makes the async worth doing. Making *only* child discovery asynchronous would be a half-measure — if `child.yaml` is dataless then `transactions.csv` certainly is, and the freeze would simply move from the picker to the first calendar render. Prefetching means that by the time a child is selectable, every synchronous repository read downstream hits a warm local file, and the rest of the storage layer needs no async changes at all.

Files can be evicted again later; a subsequent blocking read is possible but rare, and *Retry* covers it. Making the entire storage layer async to close that gap is not justified.

The availability probe sits behind a small trait so tests can simulate dataless files, missing paths, and I/O errors without an iCloud account:

```rust
pub enum Availability { Materialized, Dataless, Missing }

pub trait FileAvailability: Send + Sync {
    fn probe(&self, path: &Path) -> Result<Availability>;
}
```

Production uses the `stat`-based implementation; tests inject a fake. On non-macOS the real implementation reports every existing file as materialized.

### Migration

Runs once, at startup, before any child is loaded. Triggered by the absence of `children.yaml`.

1. Scan the base directory the legacy way: each subdirectory, following `.allowance_redirect` if present.
2. For every folder that yields a readable `child.yaml`, write a registry entry with the id **read from the YAML** (not inferred from the folder name) and the resolved absolute path.
3. Migrate `global_config.yaml`: resolve `active_child_directory` to its child's id, write `active_child_id`.
4. Write `children.yaml` atomically.

No data is moved, copied, or deleted. Redirect stub directories are **left on disk** — they become inert, and each carries a `.git` history worth preserving. A later release can offer to clean them up; this one will not delete a user's directories as a side effect of an upgrade.

Migration is idempotent by construction: it is skipped entirely once `children.yaml` exists. If it finds nothing (the fresh-machine case) it writes an empty registry, and the user goes to **Add existing child…**.

For the install this was designed against, migration produces exactly the entry shown in the `children.yaml` example above, derived from `keiko_hart/.allowance_redirect`.

### UI: Children, replacing Data directory

**Settings → Data directory** is replaced by **Settings → Children**, listing every registry entry with its status. Four operations:

| Operation | Behaviour |
|---|---|
| **Add existing child…** | `rfd` folder picker. Validates `child.yaml` is present and parseable, and that its id is not already registered. Registers path + id + label. |
| **Create new child…** | Existing create-child form. Creates `~/Documents/Allowance Tracker/<id>/`, writes `child.yaml`, registers it. The base dir remains the default home for new children — it is simply no longer scanned. |
| **Move data…** | Copy folder to target, verify, update registry path, delete source. **Refuses a non-empty target** — Add-existing is the right tool for a folder that already holds data. |
| **Remove from this machine** | Deregisters. Never deletes data. Confirmation names the path being left behind. |

Deleted along with the old modal: the conflict-resolution branch (`ConflictResolution::{OverwriteTarget, UseTargetData, Cancel}` in `shared/src/lib.rs:407-414`), `check_relocation_conflicts`, `relocate_with_conflict_resolution`, `return_to_default_location`, `archive_current_data`, and the `render_conflict_resolution_content` UI (`data_directory_modal.rs:221-278`).

That machinery exists to answer "the target already contains data — overwrite it, adopt it, or cancel?" Under a registry the question dissolves: adopting a populated folder *is* Add-existing, and it is non-destructive. Archiving guarded against an overwrite the new model never performs. Roughly 700 lines of service code plus its `shared` request/response types go with it.

Note that `ConflictResolution::UseTargetData` — "switch to the data at the target location" — was already very close to what a new machine needs. It was unreachable only because the modal required an active child to exist first. The capability was there; the entry point was not.

## Error handling

- **Malformed `children.yaml`** — do not silently reset. Log the parse error, surface a banner naming the file, and start with an empty roster. The file is hand-editable by design and a typo must be diagnosable, matching the precedent set for `sync_state.yaml` (`app_state.rs:97-103`).
- **Unknown `version`** — refuse to load rather than guess, same banner treatment.
- **Registry write failure** — the in-memory mutation is rolled back and the operation reports failure. A half-written registry is prevented by the temp-file-plus-rename.
- **`child.yaml` id ≠ registry id** — `IdMismatch`, surfaced with both ids. This is the hand-edited or duplicated-folder case and needs a human.
- **Offline with dataless files** — reads fail with an I/O error, entry becomes `ReadFailed`, Retry is offered. The app remains fully usable for any child already materialized.

## Testing

Registry and migration are pure filesystem logic and get direct unit coverage against `TempDir`, following the existing pattern in `data_directory_service.rs:651-745`.

**`ChildRegistry`** — round-trip; rejects duplicate id, naming the incumbent; `repoint` preserves id and label; `deregister` leaves the folder untouched; malformed YAML and unknown version both surface errors rather than resetting; atomic write leaves the prior file intact when the rename fails.

**Migration** — four fixtures: in-tree child only; redirected child only (the shipped install); both together; empty base dir. Assert entries carry the id *from `child.yaml`*, not the folder name — a fixture where the two deliberately differ pins this. Assert `active_child_directory` converts to `active_child_id`, that no source file is moved or deleted, and that a second run is a no-op.

**Availability** — via the injected `FileAvailability` fake, one test per `UnavailableReason` plus `Downloading` and `Available`. A dataless fixture asserts the entry reports `Downloading` *before* the prefetch completes and `Available` after, which is the ordering the UI depends on.

**Resolution** — a child whose display name is renamed still resolves to its original folder for transactions, goals, and allowance config. This test fails on `main` and is the regression pin for the latent bug in `TransactionRepository`.

**Move** — refuses a non-empty target and leaves both sides untouched; a mid-copy failure leaves the source intact and the registry unchanged.

**Roster** — the worker reports every entry exactly once; a registry mutation mid-load does not produce a stale roster.

## Sequencing

Each phase compiles and passes tests on its own.

1. `ChildRegistry` + `children.yaml` format, unit-tested in isolation. Nothing consumes it.
2. Migration, writing the registry at startup. Still nothing consumes it — the legacy scan remains authoritative, so this phase is observable but inert, and can be verified against the real install without risk.
3. `CsvConnection::child_dir` and the five repositories cut over to registry lookup. Legacy scan, redirect handling, and `find_child_directory_by_id` deleted. This is the phase that must be reviewed most carefully; the rename-regression test is the guard.
4. `FileAvailability` probe, the roster, and off-thread loading with prefetch.
5. Children UI; delete the Data directory modal, the conflict-resolution service paths, and the orphaned `shared` types.

## Risks

- **Phase 3 is a wide mechanical change** across five repositories with three different prior conventions. The rename test and the existing repository test suites are the safety net; the phase boundary keeps it separable from the UI work.
- **Prefetch latency on first launch.** A cold multi-megabyte folder could sit in `Downloading` for a noticeable stretch. Acceptable — it is visible, honest, and bounded, where the status quo is a frozen window.
- **`st_flags` portability.** Confined to the macOS `FileAvailability` implementation; other platforms compile to a no-op that reports files as materialized.
- **Leaving redirect stubs behind** means the base dir keeps folders that no longer mean anything, which could confuse a user reading it in Finder. Judged safer than deleting directories during an upgrade.
