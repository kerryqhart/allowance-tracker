# Child Registry Design

**Date:** 2026-08-28
**Author:** Kerry (with Claude)
**Status:** Draft, revised after reviewer panel (Deiko, Greg, Ted)
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

The underlying mistake is that *location* was modelled as a property derived from *identity*. A child folder is already fully self-contained — it holds `child.yaml`, `allowance_config.yaml`, `transactions.csv`, `goals.csv`, `parental_control_attempts.csv`, and its own `.git`. Nothing outside it is child-specific. The redirect stub exists purely because discovery is defined as "scan a fixed directory," so an out-of-tree child needs a forwarding address left in-tree.

## Goals

- Register a child by pointing at its folder. Adding an existing child on a new machine is a file-picker away.
- Make the registry the single source of truth for which children this machine knows about and where they live.
- Resolve `child_id → path` by map lookup instead of a directory scan with a YAML parse per entry, per call.
- Degrade legibly when a registered path is unavailable — never silently drop a child, never beachball on a cold iCloud folder, and **never fabricate a child folder from a registry entry**.
- Migrate the existing install automatically, with no user action and no data movement.

## Non-goals

- **Remote bootstrap.** Teaching the client to `GET /entities/child` and reconstruct a child from sync events is explicitly out of scope. iCloud carries the data. This is a deliberate decision not to make first-run depend on the remote service.
- Changing the sync protocol, watermarks, or event model — **except** for gating which children are polled (see *Sync contract*).
- Moving machine-local config out of `~/Documents/Allowance Tracker` into `~/Library/Application Support`. Considered and rejected: the registry holds machine-specific absolute paths and can never be synced, so the separation buys nothing today and costs a second migration.
- Extracting `backend/` into its own crate. Wanted, and deferred — see *Deferred work*.
- Fixing desktop-to-desktop sync propagation, or the shared-`.git` hazard. Both are named in *Deferred work* with successor specs.
- Multi-user or multi-parent accounts.
- Windows/Linux path handling beyond what exists (the dataless-file probe is macOS-specific and degrades to a no-op elsewhere).

## Background

### Three path conventions that happen to coincide

Path resolution is currently inconsistent, and the inconsistency is invisible only because of an accident.

`create_child` sets `child.id = generate_safe_directory_name(name)` (`backend/domain/child_service.rs:59-67`), so `Keiko Hart` becomes id `keiko_hart` stored in folder `keiko_hart`. `load_child_from_directory` then *enforces* that the id equals the containing directory name, failing hard otherwise (`child_repository.rs:120-127`).

On top of that identity, three different resolvers coexist:

| Caller | Resolves via |
|---|---|
| `GoalRepository` | passes `child_id` straight in as the directory name (`goal_repository.rs:72`) |
| `TransactionRepository` | re-derives `generate_safe_directory_name(child.name)` from the loaded child (`transaction_repository.rs:185-205`) |
| `ChildRepository`, `AllowanceRepository`, `ParentalControlRepository` | `find_child_directory_by_id`, a full base-dir scan parsing every `child.yaml` (`connection.rs:658-698`) |

These agree only because id, folder name, and sanitized display name are currently the same string. **Renaming a child breaks that.** Verified empirically during review — store child `keiko_hart` / "Keiko Hart", store one transaction, rename the display name to "Keiko Smith", list again:

```
before rename: 1 transaction
after rename:  0 transactions
base dir contents: ["keiko_hart", "keiko_smith"]
```

No error is raised. `ensure_transactions_file_exists` calls `create_dir_all` (`connection.rs:118-135`), so the failed resolution **manufactures a second child directory** and returns an empty list. Goals and allowance config keep resolving correctly, so the child appears half-erased.

Two consequences. The regression pin must assert **directory-set equality**, not merely that transactions resolve — otherwise a future regression that resolves to a different-but-consistent wrong folder passes. And if this bug has ever fired on the real install, the base directory already holds an orphan folder with real transactions and no `child.yaml`; migration must report it rather than walk past it.

The registry removes the class of bug by construction — one resolver, keyed on the immutable id.

### iCloud and dataless files

Verified on macOS 26 (Darwin 25.2.0):

```
SF_DATALESS  0x40000000  /* file is dataless object */    — sys/stat.h:359
find ~/Library/Mobile Documents/com~apple~CloudDocs -name "*.icloud"  →  0 results
```

The legacy `.icloud` bplist placeholder is not what modern iCloud Drive produces. An un-downloaded file is an APFS **dataless file**: present at its real path with real size and mtime, flagged `SF_DATALESS`. `stat()` reads its metadata *without* materializing it; the first `read()` traps to `fileproviderd` and blocks until the bytes arrive, or fails with an I/O error when offline.

Three consequences drive the design:

1. **Reading is the download trigger.** No `startDownloadingUbiquitousItem`, no objc2 FFI, no `brctl` (whose `download` subcommand no longer exists — the surviving verbs are diagnose/log/dump/status/accounts/quota/monitor).
2. **A blocking read on the UI thread is the hazard.** Children are loaded synchronously from render paths today (`child_selector.rs:38`). Pointed at a cold iCloud folder, egui freezes mid-frame — and hangs indefinitely when offline.
3. **`stat` is a free availability check.** It tells us which case we are in before committing to a block, and it is the same primitive that makes `child_dir` safe (below).

## Design

### `children.yaml`

Lives beside `global_config.yaml` in `~/Documents/Allowance Tracker/`, alongside the other machine-local state (`sync_state.yaml`, `sync_retry_queue.yaml`).

```yaml
version: 1
children:
  - id: keiko_hart
    path: /Users/kerryhart/Library/Mobile Documents/com~apple~CloudDocs/HartRoot/Parent Portal/Allowance Tracker/keiko_hart
    label: Keiko Hart
```

- `id` — the child's immutable identity, matching `child.yaml`'s `id`. Also the sync-service partition key. The registry never invents it; it is read from `child.yaml` at registration.
- `path` — absolute path to the self-contained child folder. Machine-specific by nature.
- `label` — a **display cache only**, refreshed when it changes and persisted once at the end of a roster walk. It exists so the picker can show "Keiko Hart" rather than a raw path while a folder is downloading or unavailable. Never authoritative; `child.yaml` always wins.

Written atomically (temp file + rename).

### `ChildId`

A transparent newtype in `shared`, landed as its own phase **before** the resolver cutover:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChildId(String);
```

with `AsRef<str>` and `From<&str>`. Today `AllowanceRepository` and `ParentalControlRepository` thread `child_directory: &str`, `TransactionRepository` threads `child_name: &str`, and `GoalRepository` threads `child_id: &str`. The cutover makes all three mean the same thing, and every one of those swaps compiles either way. The newtype makes the compiler the guard for a change whose failure mode is silently writing to the wrong directory.

### `ChildRegistry`

A new module, `backend/storage/csv/child_registry.rs`, owning load/save/mutate of `children.yaml`.

```rust
pub struct ChildRegistry { /* entries */ }

impl ChildRegistry {
    pub fn entries(&self) -> &[RegistryEntry];
    pub fn path_for(&self, id: &ChildId) -> Option<&Path>;
    pub fn register(&mut self, entry: RegistryEntry) -> Result<()>;
    pub fn deregister(&mut self, id: &ChildId) -> Result<()>;
    pub fn repoint(&mut self, id: &ChildId, new_path: PathBuf) -> Result<()>;
    pub fn set_label(&mut self, id: &ChildId, label: &str);
}
```

Registration is **rejected** when the id is already registered, or when the path is already registered under a different id. The error names the incumbent entry. No silent dedup — two folders claiming one child, or one folder claimed by two ids, is a situation the user must resolve.

**Sharing.** `CsvConnection` is `Clone`, held by six repositories, and shared as `Arc` across seven services (`backend/mod.rs:66`). Borrowing accessors cannot return out of a `MutexGuard`, so the registry is held copy-on-write:

```rust
// in CsvConnection
registry: Mutex<Arc<ChildRegistry>>,

pub fn registry(&self) -> Arc<ChildRegistry>;                                  // snapshot
pub fn update_registry(&self, f: impl FnOnce(&mut ChildRegistry)) -> Result<()>; // clone, mutate, persist, swap
```

Readers clone one `Arc` and then use the borrowing accessors freely. Writers rebuild and swap. The lock is held for a pointer copy and never across I/O. This also hands the roster worker a stable snapshot, which is what makes "a registry mutation mid-load does not produce a stale roster" true rather than hoped-for; worker result messages carry a generation counter so a stale batch is discarded.

`ChildRegistry::load` returns `Result`. The caller implementing log-and-banner policy is `app_state.rs`, following the `sync_state.yaml` precedent at `:97-103`. Naming the caller matters: unnamed policy lands as `unwrap_or_default()` and the banner never gets built.

### `CsvConnection` becomes a resolver

`CsvConnection` keeps the base directory — now a plain `PathBuf`, since `Arc<Mutex<PathBuf>>` existed only for `relocate`/`revert`, both deleted — and gains the registry. All directory-name parameters across the storage layer become `&ChildId`, and every path lookup routes through one method:

```rust
pub fn child_dir(&self, id: &ChildId) -> Result<PathBuf>;
```

**`child_dir` verifies before it returns.** It resolves the path from the registry and performs one `stat` of `child.yaml`, returning `Err(ChildUnavailable)` when it is absent. A `stat` does not materialize a dataless file, so this is free on the iCloud path.

This check is the design's central invariant, and the reason must be recorded so a future reader does not remove it as redundant: **the registry converts "child not found" — a safe, self-limiting failure — into "child found at a path we will happily create."** Under the scan-based world, a child whose folder was gone was never discovered, so nothing asked for its path. Under a registry the child is known whether or not the folder is there. Combined with `ensure_transactions_file_exists` calling `create_dir_all`, an unavailable child would get a fabricated folder, a $0.00 balance, and writes landing in the phantom directory and pushed to sync as truth.

Accordingly, **`create_dir_all` is removed from `ensure_transactions_file_exists`.** Creating a child folder is a registration-time act, never a side effect of a read.

Renamed while being touched, per Rust API Guidelines (C-GETTER): `get_child_directory` → `child_dir`, `get_transactions_file_path` → `transactions_path`, `get_goals_file_path` → `goals_path`.

Deleted outright: redirect-following in `get_child_directory`, `find_child_directory_by_id`, `relocate_child_data_directory`, `revert_child_data_directory`, `commit_redirect_file`, the id-equals-directory-name check (`child_repository.rs:120-127` — it is incompatible with registering a folder whose basename is not the id), `ChildRepository::set_active_child_directory` (see *Config ownership*), and the dead `new_default` (`connection.rs:33-70`, called from nowhere).

`generate_safe_directory_name` survives only where it belongs: minting an id and choosing a folder name for a *newly created* child. It is no longer a resolver.

The five repositories are **not** a mechanical cutover. Two things change shape:

- **Infallibility flips.** `get_child_directory` returned `PathBuf`; `child_dir` returns `Result`. Infallible helpers — `get_child_yaml_path` (`child_repository.rs:41`), `get_allowance_config_path` (`allowance_repository.rs:68`) — become fallible and propagate `?` upward. This is the true blast radius.
- **Write-before-register.** `store_child` resolves through `child_dir`, which fails for an unregistered child. Creation therefore has a mandatory ordering, specified below.

`TransactionRepository::get_child_directory_name` and its `unknown_child_*` fallback are deleted; the latent rename bug goes with them.

### Registry and child lifecycle

| Path | Ordering |
|---|---|
| **Create new child** | mkdir → register → write `child.yaml` |
| **Add existing child** | validate `child.yaml` → register (id and path both checked for collision) |
| **Local delete** | deregister → remove folder |
| **Remote-driven delete** | **deregister only — never remove the folder** |

The last row is a behaviour change with teeth. `app_coordinator.rs:613` currently routes a remote `Child` delete into `delete_child` → `remove_dir_all` (`child_repository.rs:251-262`). On a second machine that is a normal UI path on Machine A destroying the shared iCloud folder out from under Machine B. A sync event must never delete a folder it does not own.

A sync-applied rename marks the roster entry stale so the worker refreshes the cached `label`. Rebuilding the roster only "when the registry changes" would miss it, because a rename does not change the registry.

### Config ownership

`global_config.yaml` currently has two writers with incompatible schemas: `ChildRepository::set_active_child_directory` writes a two-key mapping (`child_repository.rs:198-215`), while `GlobalConfigRepository::save_global_config` writes a four-field struct whose `created_at`/`updated_at` are non-`Option` and whose loader hard-errors on a missing field. Migration is the moment that latent split becomes a startup failure.

`GlobalConfigRepository` becomes the **sole owner**; the `ChildRepository` writer is deleted. The key `active_child_directory` becomes `active_child_id`; the reader accepts the old key for one release and rewrites on next save.

### Availability model

```rust
pub enum ChildStatus {
    Available(Child),
    Downloading,
    Unavailable(UnavailableReason),
}

pub enum UnavailableReason {
    PathMissing,
    NotAChildFolder,
    IdMismatch { found: String },
    ReadFailed(String),
    ParseFailed(String),
}
```

Both derive `Clone, PartialEq` for roster diffing. Error payloads are stringified **at the worker boundary** — deliberately, because `ChildStatus` crosses an `mpsc` channel into UI state and must be `Clone`, while `io::Error` is not. Recorded so it is not "fixed" later.

**A registry entry is never dropped automatically.** An unloadable child stays in the list with its cached label, its path, and the reason. It cannot be made active. The user gets *Retry*, *Locate…* (repoint, keeping the id), and *Remove from this machine* (deregister only — the folder is never touched).

This is the behaviour that matters most on a fresh install: while iCloud is still pulling the folder down, the child must read as "Downloading from iCloud…", not vanish. A silently-empty picker is indistinguishable from the bug this change exists to fix.

### Classification and the availability seam

Classification is a **pure function**, not a trait:

```rust
fn classify(id: &ChildId, meta: io::Result<Metadata>, yaml: io::Result<String>) -> ChildStatus
```

Five of six statuses become two-line unit tests with no filesystem and no fake, and it is the only shape in which `IdMismatch` and `ParseFailed` are testable without contorting fixtures. Functional core, imperative shell.

A narrow seam survives on the **read** side, because the one thing a pure function cannot pin is the `Downloading → Available` ordering — a fake that only replaces `stat` leaves the subsequent reads hitting an already-materialized temp file, so the ordering test would race or assert nothing:

```rust
pub trait ChildFolderSource: Send + Sync {
    fn probe(&self, path: &Path) -> Result<Availability>;   // Materialized | Dataless | Missing
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;     // fake can block on a test-released barrier
}
```

Production uses the `stat`/`fs::read` implementation. On non-macOS, `probe` reports every existing file as materialized.

### Loading off the UI thread

Render paths must never touch the filesystem. The app holds a roster in UI state:

```rust
pub struct RosterEntry { pub entry: RegistryEntry, pub status: ChildStatus }
pub struct ChildRoster { entries: Vec<RosterEntry> }
```

On startup, and whenever the registry changes, a worker thread walks a registry snapshot and reports results over an `mpsc` channel, waking the UI with `ctx.request_repaint()` — the `WakeUi` pattern already established for the sync thread (`app_state.rs:150-157`), so no new concurrency primitive.

For each entry the worker:

1. `stat`s `child.yaml`. Missing folder → `PathMissing`; missing file → `NotAChildFolder`.
2. Checks `SF_DATALESS` via `std::os::darwin::fs::MetadataExt::st_flags()` — verified to compile on rustc 1.93, no new dependency. If set, reports `Downloading` immediately so the UI can paint, then proceeds.
3. **Prefetches the folder**: `child.yaml`, `allowance_config.yaml`, `transactions.csv`, `goals.csv`, `parental_control_attempts.csv`. On a dataless folder these reads block and materialize the files. Only when all five resolve does the entry become `Available`.

Step 3 is what makes the async worth doing. Making *only* discovery asynchronous would be a half-measure — if `child.yaml` is dataless then `transactions.csv` certainly is, and the freeze would move from the picker to the first calendar render.

`.git` is deliberately **not** prefetched: paging an entire object store to make one write fast is the wrong trade. The consequence is stated plainly — the first write to a cold child pays a git cost, because every write calls `GitManager::commit_file_change` (`git/mod.rs:164-201`) synchronously on the UI thread.

**`list_children` becomes registry-backed** and stops parsing `child.yaml` on the read path. Every current caller must read the roster instead, and all of them are in scope: `header.rs:121` (called per frame while the dropdown draws), `child_selector.rs:38`, `backfill_modal.rs:16` and `:62`, and `app_coordinator.rs:417`. Leaving any of them on a filesystem-touching path leaves the freeze exactly where it is.

**Startup allowance issuance moves behind the roster.** `check_and_issue_pending_allowances` currently runs at `app_state.rs:122`, on the main thread, before the first frame — a synchronous read-then-write of the active child's folder. On a cold iCloud folder that is a bouncing Dock icon with no window at all, strictly worse than the mid-frame freeze this design eliminates, because there is not even a label to read. It is now triggered by the roster reporting `Available` for the active child, consuming the same completion message. `app_coordinator.rs:642` is likewise gated.

### Sync contract

**Sync polls `Available` children only.** `GetChildIdsRequest` (`sync_thread.rs:270-283`, answered at `app_coordinator.rs:417`) is filtered on roster status. A `Downloading` or `Unavailable` child is neither polled nor pushed.

Without this, an unavailable child gets polled, the apply path calls `ensure_transactions_file_exists`, and a fresh `transactions.csv` is written into a folder iCloud is still pulling down — a data-conflict generator on precisely the first-run scenario this design exists to fix. Removing `create_dir_all` closes the other half.

A newly registered child begins at watermark 0, so its first poll requests the full remote history. This is acceptable only because polling cannot begin until the folder is `Available`, which is exactly why the gate is load-bearing rather than cosmetic.

`GetChildIdsRequest` itself is **kept**. An `Arc`-shareable registry snapshot would let the sync thread read ids directly and delete the round-trip plus its watermark-derived fallback — but polling is gated on roster *status*, which lives in UI state, not the registry. Revisit when status moves somewhere the sync thread can see it.

### Migration

Runs inside `Backend::with_data_dir` (`backend/mod.rs:57`), before any repository is constructed — which is also the end-to-end test seam. Triggered by the absence of `children.yaml`.

1. Scan the base directory the legacy way: each subdirectory, following `.allowance_redirect` if present.
2. For every folder that yields a readable `child.yaml`, write an entry with the id **read from the YAML** (never inferred from the folder name) and the resolved absolute path.
3. **Report, do not skip**, any folder that holds `transactions.csv` or `goals.csv` but no `child.yaml`. These are orphans from the rename bug and may hold real data. They surface in the startup banner.
4. Migrate `global_config.yaml`: resolve `active_child_directory` to its child's id, write `active_child_id`, and preserve the original as `global_config.yaml.pre-registry`.
5. Write `children.yaml` atomically.

No data is moved, copied, or deleted. Redirect stub directories are **left on disk** — they become inert, and each carries a `.git` history worth preserving. A later release can offer cleanup; this one will not delete a user's directories as a side effect of an upgrade.

Migration is idempotent by construction: skipped entirely once `children.yaml` exists. If it finds nothing, it writes an empty registry and the user goes to **Add existing child…**.

**Rollback.** Once `active_child_id` is written, a pre-migration build resolves no active child (it does not crash). Mitigation is a tagged pre-migration build plus the preserved `global_config.yaml.pre-registry`. Documented as a known limitation; single-operator deployment makes it acceptable.

**Downgrade.** A `version: 2` registry read by a `version: 1` binary refuses to load and shows the banner. Re-adding children on the older binary would produce duplicates on return. Known limitation.

### UI: Children, replacing Data directory

**Settings → Data directory** is replaced by **Settings → Children**, listing every registry entry with its status.

| Operation | Behaviour |
|---|---|
| **Add existing child…** | `rfd` folder picker. Validates `child.yaml` present and parseable, id not already registered, path not already registered. Registers path + id + label. |
| **Create new child…** | Existing form. Creates `~/Documents/Allowance Tracker/<id>/`, registers, then writes `child.yaml`. The base dir remains the default home for new children — it is simply no longer scanned. |
| **Move data…** | Copy folder to target, verify by content checksum, update registry path, delete source. **Refuses a non-empty target.** |
| **Remove from this machine** | Deregisters. Never deletes data. Confirmation names the path being left behind. |

Deleted with the old modal: `ConflictResolution::{OverwriteTarget, UseTargetData, Cancel}` (`shared/src/lib.rs:407-414`), `check_relocation_conflicts`, `relocate_with_conflict_resolution`, `return_to_default_location`, `archive_current_data`, and `render_conflict_resolution_content` (`data_directory_modal.rs:221-278`).

That machinery answers "the target already contains data — overwrite, adopt, or cancel?" Under a registry the question dissolves: adopting a populated folder *is* Add-existing, and it is non-destructive. Archiving guarded an overwrite the new model never performs. Roughly 700 lines of service code plus its `shared` types go with it.

Note that `ConflictResolution::UseTargetData` was already close to what a new machine needs. It was unreachable only because the modal required an active child to exist first.

## Error handling

- **Malformed `children.yaml`** — do not silently reset. Log the parse error, surface a banner naming the file, start with an empty roster. The file is hand-editable by design and a typo must be diagnosable (precedent: `app_state.rs:97-103`).
- **Unknown `version`** — refuse to load rather than guess; same banner.
- **Registry write failure** — the in-memory mutation is rolled back and the operation reports failure. Temp-file-plus-rename prevents a half-written registry.
- **`child.yaml` id ≠ registry id** — `IdMismatch`, surfaced with both ids. Needs a human.
- **Offline with dataless files** — reads fail, entry becomes `ReadFailed`, Retry offered. The app stays fully usable for any child already materialized.

## Testing

**Gate: the existing suite is not a safety net, and Phase 3 does not begin on it.** Verified during review — nine of ten `transaction_repository` tests create no child and route through the `unknown_child_*` fallback; the tenth (`test_delete_transaction`) uses `id: "child::test_123"` with name "Test Child" and **passes while exercising the rename bug**. `goal_repository.rs` has zero tests. The `data_directory_service.rs:651+` tests are of code Phase 5 deletes.

**Characterization tests, landed before the cutover.** For all five repositories, at the `child_dir` seam: store through the repository, assert the bytes appear under the registered path and nowhere else. `test_utils.rs` fixtures are inverted so `id ≠ generate_safe_directory_name(name)` is the **default** — today `TestHelper::create_test_child_with_name` sets `id = safe_name`, which is the accident that hides the bug.

**`ChildRegistry`** — round-trip; rejects duplicate id and duplicate path, naming the incumbent; `repoint` preserves id and label; `deregister` leaves the folder untouched; malformed YAML and unknown version surface errors rather than resetting; a failed rename leaves the prior file intact.

**Migration** — a **golden fixture** replicating the real install's tree (names and file presence, contents scrubbed), asserting the exact `children.yaml` produced. "Nothing moved" asserted as a **recursive checksum of the whole tree** before and after, not per-file existence. Fixtures: in-tree child; redirected child; both; empty base dir; redirect to a missing path; redirect to a folder with no `child.yaml`; two folders with the same id; folder name ≠ yaml id; symlinked child folder; base dir with loose files; **orphan folder with `transactions.csv` and no `child.yaml`** (must be reported, not skipped). Assert ids come from `child.yaml`, that `active_child_directory` converts, and that a second run is a no-op.

**Availability** — `classify` unit-tested per variant with synthesized inputs. The `Downloading → Available` ordering pinned through the `ChildFolderSource` fake blocking on a test-released barrier.

**Resolution regression** — a renamed child still resolves to its original folder for transactions, goals, and allowance config, **and the base directory's set of subdirectories is unchanged**. Fails on `main`.

**Sync contract** — a roster of one `Available` and one `Unavailable` entry; assert `GetChildIdsRequest` returns exactly one id.

**Config** — table-driven over every historical `global_config.yaml` shape (two-key, four-field, `active_child_directory` only, `active_child_id` only, both), asserting the resolved active child.

**Move** — refuses a non-empty target; a mid-copy failure leaves the source **byte-identical** (content, not existence) and the registry unchanged.

**Roster** — every entry reported exactly once; a registry mutation mid-load discards the stale generation.

### Untested by construction — manual checklist

Two facts cannot be exercised in CI and are verified once, by hand, on the real machine:

1. `SF_DATALESS` is read off the correct metadata call. *Expected observation:* evict a child folder via Finder, confirm the app shows "Downloading from iCloud…" rather than an error.
2. A `read()` actually triggers materialization. *Expected observation:* after eviction, opening the child resolves to `Available` with correct data and no manual download.

## Sequencing

Each phase compiles and passes tests on its own.

1. `ChildRegistry` + `children.yaml` format, unit-tested in isolation. **Plus:** a CI job running the existing suite on the existing layout. Nothing consumes the registry yet.
2. `ChildId` newtype threaded through the storage layer. Compiler-driven, no behaviour change.
3. Migration, writing the registry at startup, **plus a dry-run `children.yaml.proposed`** so the output can be eyeballed against the real install before anything depends on it. The legacy scan remains authoritative — inert *and* verifiable, not merely harmless.
4. Characterization tests at the `child_dir` seam for all five repositories. **Hard gate before Phase 5.**
5. `CsvConnection::child_dir` (with its `stat` check) and the five repositories cut over. Legacy scan, redirect handling, `find_child_directory_by_id`, the id/dirname check, and `create_dir_all`-on-read all deleted. Highest-risk phase; the characterization tests and the rename pin are the guard.
6. `classify`, `ChildFolderSource`, the roster, off-thread loading with prefetch, the `list_children` caller cutover, allowance issuance re-sequencing, and the sync `Available` gate.
7. Children UI; delete the Data directory modal, the conflict-resolution service paths, and the orphaned `shared` types.

## Deferred work

Three items are out of scope here and tracked as successors.

- **Desktop-to-desktop sync does not propagate.** Every desktop sends `X-Sync-Source: local` (`http_remote.rs:64,104`); the server stamps those events `SyncSource::Local` (`entities.rs:30-33`); `poll_child` skips them (`sync_manager.rs:190`) while advancing the watermark past them first (`:182-185`). `SyncSource` is a two-value enum with no device identity (`shared/src/sync.rs:57-60`), so `Local` means "a desktop wrote this," not "I wrote this" — Machine B skips 100% of Machine A's events and can never re-fetch them. Only header-less writes (the MCP server) reach a desktop. This also contradicts the original design, whose pull flow has no source filter and relies on `event_id` dedup (`2026-04-17-bidirectional-sync-design.md:91-108`). Fix shape: a device id in the event, skipping on `origin_device == me`. **Follow-up immediately after this work lands.**
- **Two machines sharing one iCloud-hosted `.git`.** Every write calls `GitManager::commit_file_change`. Concurrent commits into a file-synced repository produce `index.lock` collisions, conflicted copies inside `.git/objects`, and divergent refs with no merge. The hazard is not introduced here — the current install already keeps `.git` in iCloud — but this design makes the second machine real. Fix shape: a bare remote with per-machine working clones. Its own migration spec.
- **Extracting `backend/` into its own crate.** Registry and migration tests currently link `eframe`, `wgpu`, `image`, `git2`, `lettre`, and `reqwest` to assert something about a YAML file, and there is no compilation unit a dependent could bind to. Deferred rather than done first: all four `backend/` files that mention egui do so **only in comments**, so extraction is a mechanical rename of 202 `crate::backend::` references with no architectural untangling — about as cheap after this work as before. The CI job in Phase 1 covers the immediate need. Escalate if it slips more than one cycle.

## Risks

- **Phase 5 is a wide change** across five repositories with three prior conventions. The characterization tests (Phase 4) and the `ChildId` newtype (Phase 2) are the guard, and both land first by design. Resolver call sites are confined to ten files, all but two in `backend/storage/csv/`.
- **Prefetch latency on first launch.** A cold multi-megabyte folder could sit in `Downloading` for a noticeable stretch. Acceptable — visible, honest, and bounded, where the status quo is a frozen window.
- **First write to a cold child pays a git cost**, since `.git` is not prefetched.
- **`st_flags` portability.** Confined to the macOS `ChildFolderSource` implementation.
- **Leaving redirect stubs behind** means the base dir keeps folders that no longer mean anything, which may confuse someone reading it in Finder. Judged safer than deleting directories during an upgrade.

## Review Change Appendix

Changes prompted by the reviewer panel (see `reviews/2026-08-28-child-registry-design/` for full memos):

- **`child_dir` now `stat`s `child.yaml` and errors when absent; `create_dir_all` removed from the read path** (prompted by Deiko, corroborated by Ted): the registry turned "child not found" into "child found at a path we will create," which would fabricate a folder, show $0.00, and push phantom writes to sync — accepted, and made the design's central invariant.
- **Registry held copy-on-write as `Mutex<Arc<ChildRegistry>>` with `snapshot()`** (prompted by Greg, independently by Deiko): the specified borrowing API could not return out of a `MutexGuard` — accepted in Greg's shape over Deiko's `RwLock`, since a snapshot survives a long roster walk.
- **`ChildId` newtype added as its own phase** (prompted by Greg): offered as optional, taken — makes the compiler rather than one test the guard for a refactor whose failure mode is silent wrong-directory writes.
- **Phase "mechanical" claim withdrawn; infallibility flip, write-before-register ordering, and deletion of the id/dirname check all documented** (prompted by Greg, corroborated by Deiko and Ted): the id/dirname check directly blocks "Add existing child…" and was missing from the deletion list — accepted.
- **All five `list_children` callers enumerated; `list_children` specified as registry-backed** (prompted by Greg, corroborated by Ted): the roster does not fix the freeze while `header.rs:121` still hits the filesystem per frame — accepted.
- **Startup allowance issuance moved behind the roster** (prompted by Deiko): running it at `app_state.rs:122` on a cold folder is a bouncing Dock icon with no window — worse than the freeze being fixed — accepted.
- **Sync gated to `Available` children; watermark-0 replay justified by that gate** (prompted by Deiko and Ted): polling an unavailable child writes into a folder iCloud is mid-download — accepted.
- **Registry/child lifecycle subsection added; remote-driven delete deregisters only** (prompted by Deiko): `app_coordinator.rs:613` routes a remote delete into `remove_dir_all` on the shared iCloud folder — accepted.
- **Characterization tests at the `child_dir` seam made a hard gate before the cutover; `test_utils.rs` fixtures inverted so `id ≠ safe_name` is the default** (prompted by Ted): he ran the suite and demonstrated `test_delete_transaction` passes while exercising the live bug — accepted, and the "existing suite is the safety net" claim removed from Risks.
- **Rename pin strengthened to assert directory-set equality; orphan-folder fixture added and migration must report orphans** (prompted by Ted): the empirical probe showed the failure silently manufactures a second directory — accepted.
- **Golden migration fixture, dry-run `children.yaml.proposed`, and whole-tree recursive checksum** (prompted by Ted): "inert is not the same as verifiable" — accepted.
- **`FileAvailability` split into a pure `classify` function plus a narrowed read-side `ChildFolderSource`** (prompted by Greg and Ted in opposite directions): Greg called it over-abstraction, Ted called it too narrow to pin the ordering it existed for — compromise taking both, smaller than the original and more capable.
- **`global_config.yaml` given a sole owner; second writer deleted; table-driven format test added** (prompted by Ted): migration is when the latent schema split becomes a startup failure — accepted.
- **`parental_control_attempts.csv` added to prefetch; `.git` explicitly not prefetched, with the first-write cost stated** (prompted by Deiko): accepted; the `.git` multi-machine hazard is recorded in Deferred work rather than solved here.
- **Idiom pass: C-GETTER renames, named `RosterEntry`, stringly-error rationale recorded, `set_label` batched, `ChildRegistry::load` policy caller named** (prompted by Greg): accepted, all six.
- **Migration seam, rollback, downgrade, and duplicate-path rejection specified** (prompted by Ted and Deiko's open questions): accepted.
- **Crate extraction deferred with CI substituted** (prompted by Ted): partial rejection — all four `backend/` files mentioning egui do so only in comments, so extraction stays a mechanical 202-reference rename and is not made cheaper by doing it first; a CI job lands in Phase 1 instead, with an escalation trigger.
- **Desktop-to-desktop sync defect recorded in Deferred work** (found while verifying Deiko's Concern 2, which incorrectly claimed the client sends no `X-Sync-Source`): the header *is* sent, and because every desktop sends it, no desktop applies any other desktop's events — scheduled as immediate follow-up, and the Non-goals sentence claiming sync is a live-updates channel between machines was corrected.
