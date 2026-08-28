# [2B] Critique — Greg Grubberstone

**Spec reviewed:** `/Users/kerryhart/Code/allowance-tracker/docs/superpowers/specs/2026-08-28-child-registry-design.md`
**Reviewer:** Greg Grubberstone, Senior Engineer
**Date:** 2026-08-28

---

## Overall Verdict

**Approve with changes.**

The direction is right and the net line count goes down. Three path conventions collapse to one, `.allowance_redirect` dies, `find_child_directory_by_id`'s O(children) scan-and-parse-per-call dies, and ~700 lines of conflict-resolution machinery dies with them. I have no argument with the shape of the change. I have arguments with four specific pieces of it, one of which will not compile as written.

## Top Concerns

### Concern 1: The `ChildRegistry` API cannot be shared, and the spec never says how it will be

**What I see:** `ChildRegistry` exposes `entries(&self) -> &[RegistryEntry]` and `path_for(&self, &str) -> Option<&Path>` — borrowing accessors — and `CsvConnection` "gains the registry." But `CsvConnection` is constructed once as `Arc<CsvConnection>` (`backend/mod.rs:66`) and handed to seven services, is itself `Clone`, and stores its one piece of mutable state as `Arc<Mutex<PathBuf>>` (`connection.rs:13`). Meanwhile `register` / `repoint` / `deregister` are `&mut self` and get called from the Settings UI.

**Why it matters:** You cannot return `&Path` out of a `MutexGuard`. The moment the registry lives behind a lock inside a shared `CsvConnection`, `path_for` and `entries` stop compiling and get "fixed" in the worst way — by cloning a `Vec<RegistryEntry>` on every call, or by leaking the guard through the API. The spec's whole performance claim ("map lookup instead of a directory scan") quietly dies there. This is the load-bearing unanswered question in the design and it is invisible until phase 3.

**Recommendation:** Make the registry a copy-on-write snapshot, not a mutable object behind a lock.

```rust
// in CsvConnection
registry: Mutex<Arc<ChildRegistry>>,

pub fn registry(&self) -> Arc<ChildRegistry> { self.registry.lock().unwrap().clone() }
pub fn child_dir(&self, child_id: &str) -> Result<PathBuf> { … }
pub fn update_registry(&self, f: impl FnOnce(&mut ChildRegistry)) -> Result<()> { … } // clone, mutate, persist, swap
```

Readers clone one `Arc` and then use `entries()` / `path_for()` with borrows, exactly as specified. Writers rebuild and swap. The lock is held for a pointer copy, never across I/O. This also gives the roster worker a stable snapshot for free, which is precisely the "registry mutation mid-load does not produce a stale roster" test you already wrote down.

While you are in there: `base_directory: Arc<Mutex<PathBuf>>` exists only because `relocate_child_data_directory` and `revert_child_data_directory` mutated it. Both are being deleted. Collapse it to a plain `PathBuf`, delete the six `lock().unwrap_or_else(|e| e.into_inner())` poison dances, and let `base_directory()` return `&Path`.

### Concern 2: `FileAvailability` is a trait with one implementation, existing to pin one assertion

**What I see:** A `Send + Sync` trait with a single method, one production impl, one test fake. Its stated purpose is so "tests can simulate dataless files, missing paths, and I/O errors."

**Why it matters:** Two of those three need no injection at all — `TempDir` produces missing paths and unreadable files just fine. The trait exists for exactly one case: asserting that a dataless entry reports `Downloading` before `Available`. That is a trait, a trait object, a fake type, and a plumbing parameter through the worker, to pin one ordering assertion. This is the "trait with one impl" anti-pattern in *Rust Design Patterns*, and it is the kind of thing that is still here in two years with a comment nobody understands.

**Recommendation:** Two options, either is fine, both are smaller.

1. **Preferred — split the decision from the I/O.** Make the classification a pure function and test *that* directly with synthesized inputs:
   ```rust
   fn classify(id: &str, meta: io::Result<fs::Metadata>, yaml: io::Result<String>) -> ChildStatus
   ```
   Every `UnavailableReason` becomes a two-line unit test with no filesystem, no fake, no trait. The worker is then a thin shell that calls `stat`, calls `read`, and hands the results to `classify`. Functional core, imperative shell. This is also the only version where `IdMismatch` and `ParseFailed` are tested without contorting fixtures.
2. **If you insist on an injectable probe**, use the idiom this codebase already established rather than inventing a second one: `pub type WakeUi = Arc<dyn Fn() + Send + Sync>` (`backend/domain/sync_manager.rs:112`). A one-method trait *is* a closure. `type ProbeFs = Arc<dyn Fn(&Path) -> io::Result<Availability> + Send + Sync>` costs one line and no new type.

The `mpsc` + `WakeUi` roster loading itself is the right call — same primitive as the sync thread, no new concurrency model, no async runtime dragged in for four file reads. No complaint there.

### Concern 3: Phase 3 is described as "mechanical." It is not, and two blockers are unstated

**What I see:** "The five repositories change mechanically: `get_child_directory(name)` … take a `child_id` and delegate to `child_dir`."

**Why it matters:** Three things break that the spec does not mention.

- **Infallibility flips.** `get_child_directory` returns `PathBuf`. `child_dir` returns `Result<PathBuf>`. Call sites like `AllowanceRepository::get_allowance_config_path` (`allowance_repository.rs:68`) and `ChildRepository::get_child_yaml_path` (`child_repository.rs:41`) are infallible helpers today; they all become fallible and the `?` propagates up. Mechanical, but a wider blast radius than "delegate to `child_dir`" suggests.
- **Write-before-register.** `ChildStorage::store_child` calls `save_child_to_directory(child, &child.id)`, which creates the directory. Under a registry, `child_dir(id)` fails for a child that is not yet registered — so creating a child now has a mandatory ordering (register, then write) that nothing in the spec establishes and nothing in the type system enforces. Say who registers, and when, or `create_child` breaks on day one. Every `TestHelper`-based repository test (`test_utils.rs`) hits this too.
- **The id-equals-directory-name check contradicts Add-existing.** `load_child_from_directory` hard-fails when `child.yaml`'s id differs from the containing directory name (`child_repository.rs:120-127`). "Add existing child…" points `rfd` at an arbitrary folder. The instant a user picks a folder whose basename is not the id, that check fires and the child is unloadable. The spec's own migration section insists the id comes "from the YAML (not inferred from the folder name)" — correct, and it means that check must be deleted in phase 3. It isn't in the deletion list.

**Recommendation:** Add the id/dirname check to the phase-3 deletion list explicitly. Specify the create-child ordering. And drop the word "mechanical" — it sets the wrong expectation for the phase you yourself flagged as highest-risk.

### Concern 4: The roster does not fix the freeze unless the render paths stop calling `list_children`

**What I see:** "Render paths must never touch the filesystem. The app holds a roster in UI state."

**Why it matters:** `header.rs:121` calls `child_service.list_children()` inside `render_child_dropdown_with_generalized_component` — every frame that dropdown is drawn. `child_selector.rs:38` does the same. `backfill_modal.rs:16,62` too. Today each of those is a full base-dir scan with a YAML parse per child. If `ChildStorage::list_children` keeps reading `child.yaml` files and the header keeps calling it per frame, the registry makes it cheaper but the cold-iCloud freeze stays exactly where it is — which is the bug this whole change exists to fix.

**Recommendation:** State in the spec what `ChildStorage::list_children` becomes after the cutover (registry-backed? still parsing YAML?), and name the four render-path call sites that must read `ChildRoster` instead. Right now the roster is described as existing, not as being the only thing the UI reads. Those are very different changes.

### Concern 5: `child_id: &str` — the compiler is doing none of the work in the riskiest phase

**What I see:** "Every `&str` directory-name parameter across the storage layer is replaced by `child_id`." Same type, different meaning. The guard is a single regression test.

**Why it matters:** `AllowanceRepository` and `ParentalControlRepository` currently thread a parameter named `child_directory: &str`; `TransactionRepository` threads `child_name: &str`; `GoalRepository` threads `child_id: &str`. Phase 3 makes all three mean the same thing. Every one of those swaps compiles either way. You are betting a wide mechanical refactor across five repositories on one test and careful reading.

**Recommendation:** Land a transparent `ChildId(String)` newtype in `shared` as its own phase before phase 3 — `#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`, `AsRef<str>`, `From<&str>`/`TryFrom`. It is compiler-driven and boring, and it converts "the rename test is the guard" into "the type system is the guard" for a change whose failure mode is silently writing to the wrong directory. If you decide the newtype is too wide for this release, fine — but say so in the spec as a considered rejection, not by omission.

### Concern 6: Smaller idiom items, batched

- **C-GETTER.** You are renaming these functions anyway. `get_child_directory` → `child_dir`, `get_transactions_file_path` → `transactions_path`, `get_goals_file_path` → `goals_path`. Rust API Guidelines: getters do not carry a `get_` prefix. Free win during a rename you are already doing.
- **`ChildRoster { entries: Vec<(RegistryEntry, ChildStatus)> }`.** Anonymous tuple in a struct that will be pattern-matched in UI code. Give it a name — `struct RosterEntry { entry: RegistryEntry, status: ChildStatus }`. `.0` and `.1` in render code age badly.
- **`ReadFailed(String)` / `ParseFailed(String)`.** Stringly-typed error payloads are normally a smell, but here `ChildStatus` crosses an `mpsc` boundary into UI state and wants `Clone`, and `io::Error` is not `Clone`. Stringifying at the worker boundary is the right call — say that in the spec so the next reader doesn't "fix" it, and derive `Clone, PartialEq` so the roster diffing is easy.
- **`set_label` on every successful load.** That is N atomic writes (temp file + rename) on every startup and every Retry. Refresh labels in memory during the roster walk and persist once at the end, or only when a label actually changed.
- **`ChildRegistry::load` vs. the banner.** `Result<Self>` gives the caller an error, but the spec wants "log it, show a banner, start empty." Name the caller that implements that policy (`app_state.rs`, following the `sync_state.yaml` precedent at :97-103), otherwise it lands as a `unwrap_or_default()` and the banner never gets built.

## Questions the spec does not answer

- Where does the registry physically live — inside `CsvConnection`, or alongside it — and who is allowed to mutate it? (See Concern 1.)
- After the cutover, does `ChildStorage::list_children` read the registry, or does it still walk `child.yaml` files? The answer determines whether Concern 4 is real.
- `poll_remote` asks the UI thread for child ids over `SyncMessage::GetChildIdsRequest` with a 5-second timeout (`sync_thread.rs:283`), and the UI answers with a full `list_children()` scan (`app_coordinator.rs:417`). Once a registry snapshot is `Arc`-shareable, the sync thread can just read it and that whole request/response round-trip plus its timeout fallback deletes. Is that in phase 3, deliberately deferred, or unnoticed? I'd take the deletion; either way, decide it on purpose.
- `child_selector.rs.bak` is checked into the repo. Unrelated to this design, but it is sitting next to code you are about to change.

## What I thought was well-handled

The latent rename bug in `TransactionRepository::get_child_directory_name` is a genuine find, correctly diagnosed, and correctly pinned with a regression test that fails on `main`. That paragraph alone justifies the change.

The dataless-file research is the best part of the spec. Verifying that `.icloud` placeholders don't exist on modern macOS, that `brctl download` is gone, and that `stat` before `read` is the whole mechanism — that killed an objc2 FFI dependency before anyone wrote it. And the reasoning for prefetching *all four files* rather than just `child.yaml` is exactly right: half-async would have moved the freeze from the picker to the first calendar render and called it done.

Migration is non-destructive, idempotent by construction, and phase 2 lands it inert so it can be verified against the real install with nothing depending on it. That is how you sequence a risky data migration.

And the deletion list is real: redirect files, `find_child_directory_by_id`, `new_default`, `relocate`/`revert`, the whole `ConflictResolution` branch and its `shared` types. This proposal removes more than it adds. I do not get to say that often.

## Closing

Fix Concern 1 before anyone writes code — the specified `ChildRegistry` API does not survive contact with `Arc<CsvConnection>`, and finding that out in phase 3 will produce a bad `clone()`-everything patch under deadline. Fold in Concerns 3 and 4 as spec text, shrink the `FileAvailability` trait to a pure `classify` function, and this is ready to build.
