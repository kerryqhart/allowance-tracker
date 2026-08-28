# [3A] Reply to Deiko Deshimaru's Critique

**Spec:** `docs/superpowers/specs/2026-08-28-child-registry-design.md`
**Reviewer addressed:** Deiko Deshimaru
**Reply date:** 2026-08-28

---

## Overall Response

Concerns 1, 3, and 5 are accepted in full and were the most valuable findings on the panel — the perimeter really was unguarded. Concern 2 is accepted in its recommendation but its stated mechanism is wrong, and correcting it uncovered a larger, separate defect. Concern 4 splits: the prefetch omission is accepted, the `.git` question is recorded as a named risk rather than solved here.

## Point-by-Point

### Concern 1: `child_dir` is a pure lookup and the write paths will fabricate a folder

**Verdict:** Accepted.

**Our response:** This was the single most important finding of the review and it is now the design's central invariant. `child_dir` performs one `stat` of `child.yaml` and returns `Err(ChildUnavailable)` when it is absent — a `stat` does not materialize a dataless file, so this costs nothing on the iCloud path. `create_dir_all` is removed from `ensure_transactions_file_exists`; folder creation is a registration-time act only. Added to the Testing section: a registered child whose folder was deleted must cause every read and write path to error and create nothing on disk.

Your framing — that the registry converts "child not found," a safe self-limiting failure, into "child found at a path we will happily create" — is now quoted in the spec's Availability section, because it is the reason the check exists and future readers will otherwise remove the `stat` as redundant.

One calibration for the record: the 90-days-of-allowances tail is narrower than stated. `get_pending_allowance_dates` returns empty when there is no active allowance config (`allowance_service.rs:358-366`), and a fabricated folder has no `allowance_config.yaml`. The cascade requires `allowance_config.yaml` readable while `transactions.csv` is not — reachable under partial eviction, but not the common case. This does not weaken the concern: the primary harm is a $0.00 balance displayed for a child whose real data is intact elsewhere, followed by writes landing in the fabricated folder and being pushed to sync as truth.

### Concern 2: the "no remote bootstrap" boundary does not hold once a child is registered

**Verdict:** Accepted (recommendation), with a correction to the mechanism.

**Our response:** Both recommendations are adopted: `GetChildIdsRequest` is now answered only with children whose roster status is `Available`, and the watermark policy for newly registered children is stated explicitly in the spec rather than left to fall out of `unwrap_or(&0)`.

The correction: the client *does* send `X-Sync-Source` — `http_remote.rs:64` and `:104` both send `local`. That shipped with the push-events-removal work, so the `source == Local` skip is not inert.

Chasing that turned up something larger, which no memo caught. Because *every* desktop client sends `X-Sync-Source: local`, the server stamps all desktop-originated events `SyncSource::Local` (`entities.rs:30-33`), and `poll_child` skips them (`sync_manager.rs:190`) while advancing the watermark past them first (`:182-185`). `SyncSource` is a two-value enum with no device identity (`shared/src/sync.rs:57-60`), so `Local` means "written by a desktop app," not "written by me." **Desktop-to-desktop sync therefore does not work at all** — Machine B skips 100% of Machine A's events and can never re-fetch them. Only header-less writes (the MCP server) propagate to a desktop.

Note this also contradicts the original bidirectional design, whose pull flow has no source filter and relies on `event_id` dedup for echo suppression (`2026-04-17-bidirectional-sync-design.md:91-108`).

Consequences for this spec: the Non-goals sentence claiming sync is "a live-updates channel between machines" has been corrected to state what is actually true today. The defect is tracked as immediate follow-up work after this lands, not folded in — it is independent of the registry and would double this spec's surface.

Your race scenario is narrowed but not eliminated by this: a newly registered child replaying from watermark 0 skips all desktop-originated history and replays only MCP-originated events. Gating on `Available` is still the load-bearing fix, and it is in.

### Concern 3: startup allowance issuance runs on the main thread before the roster exists

**Verdict:** Accepted.

**Our response:** Correct, and it would have produced a worse first-run than the freeze the spec set out to fix — a bouncing Dock icon with no window and no "Downloading from iCloud…" label to explain it. Allowance issuance is now triggered by the roster reporting `Available` for the active child, consuming the same `mpsc` completion message the roster already sends. The `app_state.rs:122` call site moves; `app_coordinator.rs:642` is gated on roster status.

### Concern 4: the prefetch list is incomplete, and `.git` is the part that will hurt

**Verdict:** Compromise.

**Our response:** `parental_control_attempts.csv` is added to the prefetch list — you are right that there is both a global and a per-child one, and the spec conflated them.

On `.git`: we are taking your instruction to state a position, and the position is *named risk, not solved here*. Two reasons. First, the hazard is not introduced by this design — the current single-machine install already keeps `.git` inside iCloud; this spec makes the second machine real, which raises the probability without creating the mechanism. Second, the fix shape is a bare remote with per-machine working clones, which is a migration in its own right and is already earmarked as a follow-up spec. The spec now names the failure modes you listed — `index.lock` collisions, conflicted copies inside `.git/objects`, divergent refs with no merge — so the next reader inherits the analysis rather than rediscovering it.

We are not prefetching `.git`. Paging an entire object store to make the first write fast is the wrong trade; the spec instead states plainly that the first write to a cold child pays a git cost.

### Concern 5: registry lifecycle is not tied to child lifecycle, and the ownership model is unstated

**Verdict:** Accepted.

**Our response:** A "Registry and child lifecycle" subsection is added, specifying ordering for all three paths: create (mkdir → register → write `child.yaml`), local delete (deregister → remove folder), and remote-driven delete (**deregister only** — a sync event must never `remove_dir_all` a shared iCloud folder). That last one is the sharpest catch in this concern: `app_coordinator.rs:613` currently routes a remote `Child` delete straight into `delete_child` → `remove_dir_all`, which on a second machine is a normal UI path that destroys the other machine's data.

On ownership: adopted in the shape Greg proposed rather than `Arc<RwLock<_>>` — a copy-on-write `Mutex<Arc<ChildRegistry>>` with a `snapshot()` accessor. Readers clone one `Arc` and keep borrowing accessors; writers rebuild and swap; the lock is never held across I/O. This also gives the roster worker a stable snapshot for free, which is the mechanism the Testing section asserted without specifying. A generation counter on the worker's result messages is specified for the mutation-mid-load case.

Create-child ordering is resolved by the register-then-write sequence above.

## Questions Answered

### Q: Who calls `deregister`, on local delete and on a remote `Child` delete event?

A: Local delete: the Children UI, before removing the folder. Remote delete: the sync apply path, and it deregisters *only*. Added to the lifecycle subsection.

### Q: What watermark does a newly registered child start with, and is polling gated on `Available`?

A: Polling is gated on `Available`. Watermark policy is now stated explicitly in the spec. Note the answer is entangled with the desktop-to-desktop defect above and may be revisited when that is fixed.

### Q: What is the create-child ordering now that `store_child` resolves through the registry?

A: mkdir → register → write `child.yaml`. In the lifecycle subsection.

### Q: What is the position on two machines committing into one iCloud-hosted `.git`?

A: Named risk, deferred to a follow-up migration spec. See Concern 4.

### Q: After a sync-applied rename arrives, who refreshes the cached `label`?

A: The sync apply path marks the roster entry stale and the worker refreshes it. Added — you are right that a rename does not change the registry, so "rebuild whenever the registry changes" would have missed it.

### Q: Downgrade — a `version: 2` registry read by a `version: 1` binary?

A: Refuses to load and shows the banner, per the existing unknown-version rule. Re-adding children would produce duplicates on downgrade. Documented as a known limitation; single-operator deployment makes it acceptable.

## Closing

Concerns 1, 3, and 5 are in the spec as design changes. Concern 2's recommendation is in, its mechanism is corrected, and the larger defect it exposed is scheduled as immediate follow-up. Concern 4 is half accepted, half recorded as a named risk with a designated successor spec.

Nothing from your critique is left open without a written disposition.
