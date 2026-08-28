# [2D] Critique — Ted Thornberry

**Spec reviewed:** `docs/superpowers/specs/2026-08-28-child-registry-design.md`
**Reviewer:** Ted Thornberry, Principal Software Test Engineer
**Date:** 2026-08-28

---

## Overall Verdict

**Approve with changes.**

The design is more testable than what it replaces — one resolver keyed on an immutable id, an injected availability probe, a phase boundary that keeps the migration inert. I have no argument with the architecture. My argument is with the test plan, which is thinnest exactly where the blast radius is largest, and with the claim that the existing suite is a safety net. It is not. I ran it.

## Top Concerns

### Concern 1: The existing repository suite is a false safety net for Phase 3

**What I see:** Risks says "the rename test and the existing repository test suites are the safety net." I ran the suite (206 tests, 5.4s warm, all green) and then read what it actually covers.

- `transaction_repository.rs` has 10 tests. Nine of them (`setup_test_repo`) create **no child at all**, so every call routes through the `unknown_child_*` fallback branch at `transaction_repository.rs:196`. They never exercise id→directory resolution.
- The tenth, `test_delete_transaction`, uses `setup_test_child` with `id: "child::test_123"` and `name: "Test Child"`. The folder on disk is `child::test_123`; reads and writes go to `test_child`. The test passes **while exercising the bug**. It is a regression test that pins the defect.
- `goal_repository.rs` has **zero** tests. It is one of the five repositories Phase 3 rewrites.
- The four tests in `data_directory_service.rs:651+` that the spec cites as the pattern to follow are tests of `relocate` / `revert` / redirect — all code this spec deletes in Phase 5. They will be gone.

**Why it matters:** Phase 3 rewrites path resolution in five repositories and you would still be at 206/206 green if it silently resolved every child to the wrong folder, because nothing currently asserts that a write lands in the folder the id names.

**Recommendation:** Before Phase 3 touches anything, land characterization tests at the `CsvConnection::child_dir` seam for all five repositories: store through the repository, assert the bytes appear under `base/<registered path>` and nowhere else. Fix the fixtures while you are there — make `id ≠ generate_safe_directory_name(name)` the **default** in `test_utils.rs`, not the exception. Today `TestHelper::create_test_child_with_name` deliberately sets `id = safe_name`, which is precisely the accident that hides the bug.

### Concern 2: The rename-regression claim is true — and the real behaviour is worse than the spec says

**What I see:** I verified the claim empirically by temporarily adding a probe test to `transaction_repository.rs` (since reverted; `git status` is clean). Store child `keiko_hart` / "Keiko Hart", store one transaction, rename the display name to "Keiko Smith", list again:

```
TED before rename: 1
TED after rename: 0 transactions
TED base dir contents: ["keiko_hart", "keiko_smith"]
```

So the claim holds. But note what happened: no error, no exception. `ensure_transactions_file_exists` (`connection.rs:118-135`) calls `create_dir_all`, so the failed resolution **manufactures a second child directory** and returns an empty list. Goals and allowance config keep resolving correctly, so the child appears half-erased.

**Why it matters:** Two consequences the spec misses. (a) A pin that only asserts "transactions still resolve" is too weak — the failure mode is silent directory creation, so the test must also assert **no new directory appeared under the base dir**, or a future regression that resolves to a *different* wrong-but-consistent folder will pass. (b) If this bug has ever fired on the real install, the base directory already contains an orphan folder holding real transactions and no `child.yaml`. The migration as specced scans for "folders that yield a readable `child.yaml`" and will **silently skip it**, abandoning that data with no diagnostic.

**Recommendation:** Strengthen the pin to assert directory-set equality before and after. Add a migration fixture: folder with `transactions.csv` and no `child.yaml`. Migration must report it in the startup banner, not skip it in silence.

### Concern 3: The migration cannot be verified before it touches the one real install

**What I see:** Four fixtures (in-tree, redirected, both, empty). Phase 2 is described as "observable but inert, and can be verified against the real install without risk."

**Why it matters:** Inert is not the same as verifiable. There is no described mechanism for *seeing* what migration produced before Phase 3 makes it authoritative, and the four fixtures are the four shapes the author imagined, not the shape actually on disk. The real base directory also holds `sync_state.yaml`, `sync_retry_queue.yaml`, `parental_control_attempts.csv`, `global_config.yaml`, `.DS_Store`, and stub folders carrying `.git`. None of that is in a fixture.

**Recommendation:**
1. Check in a **golden fixture** replicating the real install's directory tree (names and file presence; contents can be scrubbed). Assert the exact `children.yaml` produced, byte for byte.
2. Give Phase 2 a **dry-run report** — log or write `children.yaml.proposed` and have the user eyeball it. That is what makes Phase 2 genuinely verifiable rather than merely harmless.
3. Assert "no data moved" as a **recursive checksum of the whole tree** taken before and after, not a per-file existence check. That is the assertion that catches the mistake you did not anticipate.
4. Add fixtures for: redirect pointing at a missing path; redirect pointing at a folder with no `child.yaml`; two folders whose `child.yaml` carry the same id; a folder whose name ≠ its yaml id; a symlinked child folder; base dir containing loose files.

### Concern 4: The `FileAvailability` seam is in the right module but too narrow

**What I see:** The trait abstracts `probe(&Path) -> Availability` — the `stat`. Step 3, the prefetch, is a plain `read` with no seam. The test plan says a "dataless fixture asserts the entry reports `Downloading` *before* the prefetch completes and `Available` after."

**Why it matters:** That test cannot do what it claims. With only `probe` faked, the subsequent reads hit a real, already-materialized temp file and return instantly. There is no window in which `Downloading` is observable, so the test either races or asserts nothing. The genuinely hazardous behaviours — a read that blocks for 40 seconds, a read that fails with `ErrorKind::Other` because the device is offline, a file evicted between probe and read, a partial prefetch where `child.yaml` materializes and `transactions.csv` does not — are all on the read side and all unreachable through this seam.

**Recommendation:** Widen the seam to the whole folder source, e.g. `trait ChildFolderSource { fn probe(&self, p: &Path) -> Result<Availability>; fn read(&self, p: &Path) -> io::Result<Vec<u8>>; }`. Then a fake can block on a barrier the test releases, and the `Downloading → Available` ordering becomes deterministic instead of hopeful. Also state plainly in the spec what remains **untested by construction**: that `SF_DATALESS` is read off the right metadata call and that a `read()` actually triggers materialization. That is one manual verification on the real machine. Write it down as a numbered checklist item with an expected observation, rather than leaving it implied.

### Concern 5: No CI, and no crate boundary for the code under test

**What I see:** There is no `.github/` directory and no workflow of any kind. `backend/` is not a crate — it is `#[path = "../../backend/mod.rs"] pub mod backend;` inside `egui-frontend/src/lib.rs`.

**Why it matters:** Every registry and migration test compiles and links `eframe`, `wgpu`, `image`, `git2`, `lettre`, and `reqwest` to assert something about a YAML file. Warm that is 5 seconds; cold it is minutes, and a headless runner with wgpu is a reliability problem you will meet the day you add CI. Worse for this spec specifically: there is no compilation unit a dependent could bind to. "How does anything else test against the registry?" has no answer today.

**Recommendation:** Extract `backend` — or at minimum `child_registry` plus its migration — into its own crate as part of Phase 1, **before** five repositories bind to it. Land one CI job running `cargo test -p <backend-crate>` at the same time. This is cheap now and expensive after Phase 3.

### Concern 6: The sync thread's contract with the roster is unspecified and untested

**What I see:** `poll_remote` (`sync_thread.rs:270-291`) asks the UI thread for local child IDs; the handler at `app_coordinator.rs:417` answers with `list_children()`. The spec redefines discovery but never says what `list_children` returns once the registry exists, nor what the sync thread should be told about a `Downloading` or `Unavailable` child.

**Why it matters:** If an unavailable child is reported as local, sync will poll it and apply events. The apply path calls `ensure_transactions_file_exists`, which `create_dir_all`s — writing a fresh `transactions.csv` into a folder iCloud is still pulling down. That is a data-conflict generator on exactly the first-run scenario this spec exists to fix. Note `list_children` is also called from render paths the spec does not mention: `header.rs:121` and `backfill_modal.rs:16,62`.

**Recommendation:** State the contract explicitly — sync polls `Available` children only — and pin it with a test that seeds a roster of one Available and one Unavailable entry and asserts `GetChildIdsRequest` returns exactly one id. Enumerate every `list_children` caller in the spec, not just `child_selector.rs`.

### Concern 7: `global_config.yaml` has two writers with incompatible schemas

**What I see:** `ChildRepository::set_active_child_directory` (`child_repository.rs:198-215`) writes a two-key mapping. `GlobalConfigRepository::save_global_config` writes a four-field struct whose `created_at`/`updated_at` are non-`Option`, and `load_or_create_global_config` hard-errors on a missing field. The migration rewrites this file. The six existing `global_config_repository` tests only ever read what that same repository wrote.

**Why it matters:** The migration is the moment this latent schema split becomes a startup failure, and nothing in the test plan covers the cross-writer case or the "old key for one release" compatibility window.

**Recommendation:** Name one owner of `global_config.yaml` in the spec. Add a table-driven test that reads every historical shape — two-key, four-field, `active_child_directory` only, `active_child_id` only, both — and asserts the resolved active child for each.

## Questions the spec does not answer

- Where exactly does migration run relative to `Backend::with_data_dir` (`backend/mod.rs:57`)? That constructor is the natural end-to-end seam and the spec never claims it.
- What is the rollback story? Once migration rewrites `global_config.yaml`, does the previous build still start? Is there a test for that?
- What happens when two registry entries name different ids but the same path? `register` rejects duplicate ids; it says nothing about duplicate paths.
- How does a test assert "atomic write leaves the prior file intact when the rename fails"? Forcing a rename failure needs a seam that does not exist yet.
- When `Move data…` fails mid-copy, what asserts the source is byte-identical — existence, or content?

## What I thought was well-handled

Reading the id from `child.yaml` rather than inferring it from the folder name, with a fixture where the two deliberately differ, is exactly right and is the single highest-value assertion in the plan. Phase 2 being inert is good sequencing instinct. "Never drop a registry entry automatically" is a behavioural invariant, not an implementation detail, and it is stated as one. The reasoning for prefetching the whole folder rather than just `child.yaml` — that a half-async load only relocates the freeze — is the kind of thinking I usually have to drag out of people. And migration idempotence by construction beats idempotence by assertion.

## Closing

The architecture is sound and materially more testable than what it replaces. Proceed — but do not proceed into Phase 3 on the strength of the current suite, because I have demonstrated it is green while the bug is live. Land the characterization tests, the golden migration fixture, the widened availability seam, and the crate extraction first. Those four are a few days of work and they are the difference between a safety net and a decoration.
