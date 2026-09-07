# [3b] Reply to Greg Grubberstone's Critique

**Spec:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md`
**Reviewer addressed:** Greg Grubberstone
**Reply date:** 2026-09-06

---

## Overall Response

Six of seven concerns accepted, one taken further than proposed. Concern 4 is a
silent-data-loss bug against the spec's own stated goal and was verified before
acceptance. Concern 1 replaced the spec's threading story with something with
strictly fewer moving parts, which is the rare review outcome where the fix is
smaller than the thing it replaces.

## Point-by-Point

### Concern 1: The merge cannot run where the spec says it runs

**Verdict:** Accepted, adopting your alternative exactly.

**Our response:** You are right that the existing sync thread touches zero
repository files by design (`sync_manager.rs:37-40`), and that the spec's
"serialize through the same path" was an assertion with no mechanism behind it.

The split is now specified as you proposed: background thread does `fetch` and
`push` only (`.git` objects and refs, never the working tree); the merge is pure
and runs anywhere; **all** working-tree mutation — writing merged files,
recompute, commit — lands on the UI thread in the existing
`handle_sync_messages` drain via one new
`SyncMessage::ApplyMerge { child_id, merged }`.

No mutex, no in-flight flag, no new invariant. Deiko independently asked for a
per-child lock; this achieves the same exclusion by preserving the existing "UI
owns the bytes" rule instead of adding a second one, and the reply to Deiko says
so.

### Concern 2: The transport is an unverified assumption, and `git2` is built without it

**Verdict:** Accepted. Promoted to a blocking spike.

**Our response:** Verified: `git2 = { version = "0.19", default-features = false }`
(`egui-frontend/Cargo.toml:48`). The spec now carries a **spike that must complete
before planning**, with its result written back into the spec: git2
clone/fetch/push against a live lgs daemon under the current feature flags. If it
fails, the spec records the two exits you named — enable `https` and accept the
build weight, or shell out to `git` and retract the remaining zero-dependency
language.

Second part also accepted: the port-bearing URL is no longer frozen into
`.git/config` at migration. It is re-resolved from `lgs status --json` on startup
and reconciled. Deiko raised this independently.

### Concern 3: Onboarding step 4 double-clones, and names the remote differently

**Verdict:** Accepted.

**Our response:** Verified — `ensure_working_copy` (`cli.rs:700-733`) shells out
to `git clone` and refuses a non-empty non-repo directory, so the git2 clone that
followed was dead code or a collision.

lgs's clone is now authoritative. After `restore`, the app opens the repo with
git2 and normalizes the remote through a single `ensure_lgs_remote(&Repository,
&str)` used by *both* migration and onboarding, so one remote name exists in the
system. The transitive `git` dependency is stated in the spec rather than
claimed away — it is the same retraction Deiko's Concern 2 forced.

### Concern 4: Transaction IDs collide across machines by construction

**Verdict:** Accepted, both halves.

**Our response:** Verified: `shared/src/lib.rs:581` is
`format!("transaction::{}::{}", transaction_type, epoch_millis)` with no device
component. Your distinction is the important part and the spec had it wrong:
base-present edit/edit is a genuine conflict, base-absent add/add is two distinct
rows that collided on a key, and the merge table treated them identically.

Both changes specified: (1) the table now splits base-absent add/add — identical
content keeps one, differing content keeps **both**, re-keying one
deterministically so each machine picks the same loser; (2) the generator gains a
short random suffix and `parse_id` relaxes to `parts.len() >= 3`.

Your framing that the merge rule should be a backstop rather than the only
defense is now the spec's stated rationale for doing both rather than either.

`Goal` id generation is checked for the same defect as part of the work — you
raised it as a question and it belongs in scope.

### Concern 5: `recalculate_balances_from_date` is the wrong thing to reuse

**Verdict:** Accepted.

**Our response:** "Right instinct, wrong target" is a fair description of what
the spec did. `update_transaction_balances` calling
`find_child_id_for_transaction` per row — each parsing every child's CSV — makes
a 500-row merge quadratic, on a 30-second timer.

The spec now specifies extracting the arithmetic rather than the I/O:
`recompute_running_balances(&mut [Transaction])`, pure, called by the merge on
in-memory rows, with `recalculate_balances_from_date` refactored to call the same
function. One implementation of the arithmetic, which was the original goal.
Ted reached the same conclusion from the testability side.

The merge path's `BalanceService` is built with `.with_sync_notifier(None)`.

### Concern 6: f64 money will make the property test lie

**Verdict:** Accepted, and taken further than you proposed.

**Our response:** You recommended integer cents in the proptest strategy as a
minimum and a `Money(i64)` newtype as the better option. The author chose the
newtype — his words: "I spent too much time around financial systems to accept a
float."

The spec now specifies `Money(i64)` at the domain boundary with `From`/`TryFrom`
conversions, and — a consequence worth noting — a canonical 2-decimal renderer
replacing `f64::to_string()`. That incidentally closes part of your Concern 7:
deterministic money rendering removes a whole class of byte-divergence.

Blast radius is called out in the spec because it is not small: `Transaction` is
serialized to JSON for the AWS sync-service and read by the MCP Lambda in a
different stack. The spec therefore requires a wire-compatible serde
representation so no cross-repo change is forced.

Also accepted: `validate_all_balances` returning `Ok` when balances are wrong is
a genuine footgun. It becomes `Result<(), Vec<BalanceMismatch>>` before anything
relies on it as an assertion.

### Concern 7: CSV codec will fork

**Verdict:** Accepted.

**Our response:** Correct that the merge reads git blobs with no path while all
CSV parsing currently lives inside path-bound repositories, and that the
predictable outcome is a second parser that drifts.

Specified: free functions `parse_transactions(&str)` and
`render_transactions(&[Transaction])`, with the repository calling them, plus the
byte-stability round-trip test `render(parse(s)) == s`. Ted asked for the same
round-trip over a corpus including the real production `transactions.csv`, which
the spec adopts.

## Smaller Notes

- **No trait behind `LgsClient`** — accepted. The seam is a pure
  `parse_status(&str) -> Result<StatusReport>` plus a thin
  `run(&[&str]) -> Result<String>`. Ted's demand for injection seams is satisfied
  by the same shape, so there was no conflict to resolve.
- **Health as an enum with `#[serde(other)] Unknown`** — accepted, not optional.
  lgs's own tests feed `"a_variant_from_the_future"` (`cli.rs:1788`).
- **One open `Repository` per cycle, `&Path` over `P: AsRef<Path>`** — accepted.
- **Reuse `plan_migration` / `MigrationReport` / `SkippedFolder`, and
  `tree_checksum` as the convergence oracle** — accepted. `tree_checksum`
  (`csv/checksum.rs:17`) being exactly the oracle the convergence test needs is
  the kind of thing only someone who read the codebase would find.
- **Cloud-path guard as one typed function** — accepted:
  `reject_if_cloud_synced(&Path) -> Result<(), CloudPathRejected>` with the error
  naming which rule fired, table-driven test. Ted independently required this be
  a pure predicate over injected paths; the spec takes that version.

## Questions Answered

### Q: Which thread owns the working tree during a merge?

A: The UI thread, exclusively. See Concern 1.

### Q: Does the merge commit's tree include the recomputed balances, or is recompute a second commit?

A: Included. One two-parent merge commit whose tree is already canonical and
rebalanced — a second commit would create a tree that never satisfies the
convergence property. Stated in the spec.

### Q: What happens when push is rejected because the remote moved between fetch and push? Is the cycle bounded?

A: Bounded. Specified as a retry cap per cycle, after which the child is left for
the next scheduled pull rather than spinning.

### Q: Do merge-driven row changes re-enter the AWS notifier?

A: No — suppressed via `with_sync_notifier(None)`. See Concern 5.

### Q: Is `Goal`'s id generated the same timestamp way?

A: Checked as part of the id work. See Concern 4.

## Closing

Concerns 1, 2, 4 and 5 were named as pre-planning blockers and all four are
resolved in the spec, with 2 carried as a spike whose result is written back
before planning begins. Nothing is left open.
