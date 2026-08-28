# [3D] Reply to Ted Thornberry's Critique

**Spec:** `docs/superpowers/specs/2026-08-28-child-registry-design.md`
**Reviewer addressed:** Ted Thornberry
**Reply date:** 2026-08-28

---

## Overall Response

Six of seven concerns accepted. Concern 5 (crate extraction before Phase 3) is a partial rejection with a substituted remedy, on grounds explained below. Your empirical work — running the suite and demonstrating that a green test pins the live defect — changed the plan more than any other single contribution to this review.

## Point-by-Point

### Concern 1: The existing repository suite is a false safety net for Phase 3

**Verdict:** Accepted.

**Our response:** You are right and the spec was wrong to cite the existing suite as the safety net. That sentence is removed from Risks.

The finding that nine of ten `transaction_repository` tests create no child and route through the `unknown_child_*` fallback at `:196` — and that the tenth, `test_delete_transaction`, passes *while exercising the bug* by using `id: "child::test_123"` with name "Test Child" — is the most useful thing anyone on this panel produced, because it is the difference between a plan that would have shipped a silent regression and one that will not. That `goal_repository.rs` has zero tests while being one of the five repositories Phase 3 rewrites compounds it.

Adopted in full:
- Characterization tests at the `CsvConnection::child_dir` seam for all five repositories, landed **before** Phase 3 touches anything: store through the repository, assert the bytes appear under the registered path and nowhere else.
- `test_utils.rs` fixtures inverted so `id ≠ generate_safe_directory_name(name)` is the **default**, not the exception. As you say, `TestHelper::create_test_child_with_name` deliberately setting `id = safe_name` is precisely the accident that hides the bug.

Also removed: the citation of `data_directory_service.rs:651+` as the pattern to follow, since Phase 5 deletes that code.

### Concern 2: The rename claim is true, and the real behaviour is worse than stated

**Verdict:** Accepted.

**Our response:** Thank you for verifying empirically rather than reading. The probe result —

```
TED before rename: 1
TED after rename: 0 transactions
TED base dir contents: ["keiko_hart", "keiko_smith"]
```

— is now in the spec's Background section, because "silently manufactures a second directory and returns an empty list, while goals and allowance config keep resolving correctly" is a far more precise statement of the defect than the spec's original "would begin resolving to a folder that does not exist." The child appears half-erased. That is the detail that makes the bug legible.

Both consequences adopted:
- The regression pin now asserts **directory-set equality** before and after, not merely that transactions still resolve. Your reasoning is correct: without it, a future regression that resolves to a different-but-consistent wrong folder passes.
- A migration fixture is added for a folder holding `transactions.csv` with no `child.yaml`, and migration must **report** such folders in the startup banner rather than skipping them silently. If this bug has ever fired on the real install, that orphan is holding real transactions, and the migration as originally specced would have walked past it without a word.

Confirmed independently before accepting: the working tree is clean and the real install shows no stray directory.

### Concern 3: The migration cannot be verified before it touches the real install

**Verdict:** Accepted.

**Our response:** "Inert is not the same as verifiable" is the correction the sequencing needed. All four recommendations adopted:

1. Golden fixture replicating the real install's tree — names and file presence, contents scrubbed — asserting the exact `children.yaml` produced.
2. Phase 2 writes `children.yaml.proposed` as a dry run for eyeballing. This is what makes Phase 2 genuinely verifiable rather than merely harmless.
3. "Nothing moved" asserted as a recursive checksum of the whole tree before and after, not per-file existence. Agreed that this is the assertion that catches the unanticipated mistake.
4. All six additional fixtures added: redirect to a missing path; redirect to a folder with no `child.yaml`; two folders with the same id; folder name ≠ yaml id; symlinked child folder; base dir with loose files.

Your observation that the real base directory also holds `sync_state.yaml`, `sync_retry_queue.yaml`, `parental_control_attempts.csv`, `global_config.yaml`, `.DS_Store`, `archive/`, `global/`, and stub folders carrying `.git` — none of it in a fixture — is why the golden fixture is worth more than the four imagined shapes.

### Concern 4: The `FileAvailability` seam is in the right module but too narrow

**Verdict:** Accepted, merged with Greg's opposing recommendation.

**Our response:** You are right that the ordering test as written cannot do what it claims: with only `probe` faked, subsequent reads hit an already-materialized temp file and return instantly, so there is no window in which `Downloading` is observable. The test would have raced or asserted nothing.

Greg argued the opposite direction — that the trait is over-abstraction and classification should be a pure function. Both are right about different halves, and the design takes both. Classification becomes `classify(id, meta, yaml) -> ChildStatus`, a pure function testable with synthesized inputs and no filesystem. The trait survives narrowed to the **read** side, so a fake can block on a test-released barrier and make `Downloading → Available` deterministic.

Also adopted: a numbered manual-verification checklist for what remains untested by construction — that `SF_DATALESS` is read off the right metadata call, and that a `read()` actually triggers materialization — each with its expected observation on the real machine. You are right that leaving it implied means it never happens.

### Concern 5: No CI, and no crate boundary for the code under test

**Verdict:** Compromise — CI accepted, crate extraction deferred.

**Our response:** The diagnosis is correct on every fact. There is no `.github/`, `backend` is `#[path = "../../backend/mod.rs"]` inside the egui crate, and a YAML registry test links `eframe`, `wgpu`, `image`, `git2`, `lettre`, and `reqwest`.

**Accepted:** a CI job lands with Phase 1, running the existing suite on the existing layout.

**Deferred:** extraction before Phase 3. We checked the coupling rather than taking the recommendation on faith, and it cuts against the "cheap now, expensive after Phase 3" argument. Only four files under `backend/` mention egui at all, and **all four are comments** — `backend/mod.rs:1`, `storage/traits.rs:19`, `domain/sync_manager.rs:107`, `storage/git/mod.rs:16`. There is no code-level dependency. Extraction is therefore a mechanical rename of 202 `crate::backend::` references plus a `Cargo.toml`, with no architectural untangling, and it will be about as cheap after this work as before it.

The specific mechanism you gave — "before five repositories bind to it" — does not hold either: those five already bind to `CsvConnection`, and the registry adds no new cross-boundary coupling.

What remains true is the long-term drift argument, and the inner-loop cost. On the latter, your own measurement works against urgency: 206 tests in 5.4s warm is not a loop that is hurting. The cost is cold builds and future CI, which the CI job surfaces without a workspace restructure riding inside a data-integrity fix.

Extraction is recorded as a follow-up with its own diff. If it slips more than one cycle, the drift argument wins and it should be escalated.

### Concern 6: The sync thread's contract with the roster is unspecified and untested

**Verdict:** Accepted.

**Our response:** The contract is now explicit: **sync polls `Available` children only.** Pinned with the test you specified — seed a roster of one `Available` and one `Unavailable` entry, assert `GetChildIdsRequest` returns exactly one id. Every `list_children` caller is enumerated in the spec (`header.rs:121`, `child_selector.rs:38`, `backfill_modal.rs:16` and `:62`, `app_coordinator.rs:417`), not just the one originally named.

Your causal chain was exactly right and Deiko reached it independently: an unavailable child reported as local gets polled, the apply path calls `ensure_transactions_file_exists`, which `create_dir_all`s a fresh `transactions.csv` into a folder iCloud is still pulling down — a data-conflict generator on precisely the first-run scenario this spec exists to fix. Removing `create_dir_all` from the read path (Deiko's Concern 1) closes the other half.

One thing you could not have known, found while verifying a related claim: desktop-to-desktop sync does not currently work at all. Every desktop sends `X-Sync-Source: local`, the server stamps those events `SyncSource::Local`, and `poll_child` skips them while advancing the watermark past them. `Local` means "a desktop wrote this," not "I wrote this." This is tracked as immediate follow-up work, separate from this spec.

### Concern 7: `global_config.yaml` has two writers with incompatible schemas

**Verdict:** Accepted.

**Our response:** `GlobalConfigRepository` is named the sole owner in the spec, and `ChildRepository::set_active_child_directory` (`child_repository.rs:198-215`) is removed as a second writer. The table-driven test you specified is added, covering every historical shape — two-key, four-field, `active_child_directory` only, `active_child_id` only, both — asserting the resolved active child for each.

You are right that migration is the moment this latent split becomes a startup failure, and that six existing tests reading only what their own repository wrote is not coverage of a cross-writer format.

## Questions Answered

### Q: Where does migration run relative to `Backend::with_data_dir`?

A: Inside it, before any repository is constructed. Now stated, and it is the end-to-end seam the tests use.

### Q: What is the rollback story? Does the previous build still start?

A: No — once `active_child_id` is written, the prior build resolves no active child, though it does not crash. Documented as a known limitation with the mitigation being a tagged pre-migration build. A test asserts the pre-migration file is preserved as `global_config.yaml.pre-registry`.

### Q: Two registry entries with different ids but the same path?

A: Rejected at registration, same as duplicate ids. Added — the spec previously covered only duplicate ids.

### Q: How does a test assert atomic-write behaviour when the rename fails?

A: Through the same widened folder-source seam from Concern 4, which now covers writes. Without it the assertion was untestable, as you note.

### Q: When `Move data…` fails mid-copy, what asserts the source is byte-identical?

A: Content, via the recursive checksum helper introduced for Concern 3. Existence-only was the weaker assertion and the spec now says so.

## Closing

Concerns 1, 2, 3, 6, and 7 are accepted and are spec text. Concern 4 is merged with Greg's opposing recommendation into a smaller and more capable seam. Concern 5 is split: CI now, extraction deferred with a stated escalation trigger.

Your closing position — do not enter Phase 3 on the strength of the current suite — is adopted as a hard gate. The characterization tests land first.
