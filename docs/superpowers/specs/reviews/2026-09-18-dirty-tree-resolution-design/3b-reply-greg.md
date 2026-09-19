# [3B] Reply to Greg Grubberstone's Critique

**Spec:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer addressed:** Greg Grubberstone
**Reply date:** 2026-09-19

---

## Overall Response

Accepted almost entirely. Every shape change you proposed makes the diff smaller
than the spec did, which is the correct direction and is why they were taken
without argument. Your Concern 1 was one of three independent findings of the
same inert fix, and Concern 3 was one of three independent findings of the
missing validity gate.

## Point-by-Point

### Concern 1: `commit_file_change` returning its error fixes nothing

**Verdict:** Accepted, and taken further than proposed.

**Our response:** Confirmed at all five sites. Under Deiko's Concern 1 the design
widened the guard from the owned allowlist to all tracked paths, which closes
this route directly: a `parental_control_attempts.csv` left dirty by a swallowed
commit is staged and committed by the guard on the next cycle. With the hard
reset also gone, an uncommitted row is no longer at risk from anything.

So rather than propagate, the change is dropped from scope with the reason
stated. Your `#[must_use]` recommendation is kept, along with replacing each
`let _ =` with an explicit logged disposition, so a future `let _` has to be
written deliberately — which was the durable half of your point.

Your instinct on `parental_control_repository.rs:163` being the one worth arguing
about was right for the original design; it is simply moot now that the guard
reaches the file as a tracked path.

### Concern 2: `DirtyTreeResolution` is a `Result` wearing a costume

**Verdict:** Accepted.

**Our response:** Correct on all three counts — the variants no consumer
distinguishes, the stringly-typed error flattening the `git2::Error` cause chain
at the point of failure, and `Committed(String)` when `git2::Oid` is `Copy` and
already right. `thiserror` is already an `egui-frontend` dependency. The spec now
specifies `Result<git2::Oid, DirtyTreeError>` with your three-variant enum whose
`Display` is the user-facing text, one definition rather than a `String` passed
hand to hand.

`fn fail_sync(&mut self, child_id: &str, ...)` is adopted and the eight copies of
"format a message, set `sync.status`, call `record_sync_failure`, return `Failed`"
collapse into it.

One amendment from Pierre's Concern 4, which cuts against the `Display`-is-the-
message part: the variants carry *structure* (which file, which condition) and the
modal composes the final sentence, so wording changes do not become edits to
merge code. `Display` remains for logs and for the error chain. Your one-definition
principle is preserved; the definition just lives at the UI boundary rather than
inside the sync engine.

### Concern 3: Removing the hard reset leaves no validity gate

**Verdict:** Accepted.

**Our response:** Agreed, and independently reached by Deiko and Ted. The reset
was a bad answer to a real question and the spec removed the answer while the
question still applies — precisely during the upgrade window, and permanently for
corruption this app did not cause.

The guard now parse-validates `transactions.csv` with
`allowance_core::codec::parse_transactions` before staging, exactly as you scoped
it: the function `read_rows` already calls, no new dependency, no reset. Test
added: dirty *and* unparseable ⇒ failure notice, no commit, file untouched.

### Concern 4: The staging loop hand-rolls what libgit2 already does

**Verdict:** Accepted, and superseded in a way that proves your point.

**Our response:** You proposed `add_all` + `update_all` over the owned pathspecs
in place of the three-branch loop. Deiko's Concern 1 then established that the
guard should operate on tracked paths generally, and `index.update_all` is
precisely that primitive — so the guard is now two libgit2 calls with no loop, no
`exists()` stat, no HEAD peel and no tree lookup. Your observation that "the
danger was always the `*`, never `add_all`" is the load-bearing one; it is quoted
in the spec.

`stage_owned_files` keeps the narrow list for `commit_merge` and migration, and
loses its `repo_path` parameter per your second point — `repo.workdir()` supplies
it, and two parameters that must agree is an unenforced invariant.

### Concern 5: `write_atomic` re-implements `tempfile`, badly

**Verdict:** Accepted.

**Our response:** "Removed on every failure path is a promise a human keeps by
hand at every `?`" is the right frame. `tempfile` moves from `[dev-dependencies]`
to a real dependency; `NamedTempFile::new_in` gives collision-safe naming and
cleanup-on-drop. Signature mirrors `std::fs::write` (`impl AsRef<Path>`,
`impl AsRef<[u8]>`) so the conversions are a mechanical identifier swap, and the
name is `atomic::write` — `write_atomic` stuttered.

`goal_repository.rs:128` renders to a `Vec<u8>` via
`csv::Writer::from_writer(Vec::new())` then `into_inner()` and calls the same
function. No closure-taking variant for one caller.

One thing your version needs that Deiko's question surfaced: `NamedTempFile`
creates at `0600`, so `persist` would *tighten* an existing `0644` file. The spec
requires the mode to be set explicitly before persist, preserving the existing
file's mode where there is one.

### Concern 6: The directory fsync promises durability macOS won't give

**Verdict:** Accepted.

**Our response:** Correct, and the user ruled on it directly. The spec now
separates atomicity (from `rename(2)` alone — the property both defects actually
need) from durability (`fsync`, which on macOS does not flush the drive's own
cache). The directory fsync is dropped. `F_FULLFSYNC` is rejected: tens of
milliseconds on every transaction write, to buy survival of the most recent
write, when rename already guarantees the file is never corrupt.

`sync_all()` before rename is kept, but on narrower grounds than the spec first
gave — not APFS, where copy-on-write checkpoints already order data before
metadata, but the odd volume: an external disk, a network mount, HFS+.

You also caught, indirectly, an error in the spec's own reasoning: its
"no fsync ⇒ zero-length file on power loss" justification for consolidating the
six hand-rolled writers imported an ext4 delayed-allocation failure mode into an
APFS context. Corrected in place. The consolidation stands on cleanup-on-drop,
collision-safe naming, and one definition instead of six.

### Concern 7: Dead code in the file you're already editing

**Verdict:** Accepted.

**Our response:** The six `*_sync` forwarders at `git/mod.rs:410-446` have zero
callers and sit in the file whose staging logic this work rewrites. Deleted.
Recorded in the appendix so the scope addition is visible rather than smuggled.

## Questions Answered

### Q: How is the "failed write leaves the prior file byte-identical" failure induced deterministically?

A: Named in the spec so the test does not get written as `#[ignore]` and then
never run: `chmod 0555` on the parent directory, which makes `NamedTempFile::
new_in` fail for a non-root user on macOS while leaving the existing file
readable and intact. The test asserts the prior file is byte-identical and that
no temp residue remains.

### Q: Can `commit_file_change`'s `has_uncommitted_changes` gate produce a hard error where it previously produced a benign empty commit?

A: Moot — propagation is dropped (Concern 1), so the gate's pairing with a narrow
allowlist stays exactly as it is today. Worth noting the asymmetry you spotted is
real: `has_uncommitted_changes` uses `statuses(None)`, which counts untracked
files, while staging is an allowlist. It is now recorded in the spec as a known
inconsistency rather than left for someone to rediscover.

### Q: `parental_control_attempts.csv` joins the owned list but no merge rule covers it.

A: It no longer joins. Under Deiko's widening the guard reaches it as a tracked
path, so the fold-in is unnecessary and `FILES_THIS_APP_OWNS` keeps its original
membership. Your underlying observation stands and is recorded as a known gap: a
peer's appends to that file are dropped on merge the same way `goals.csv`'s are,
except `goals.csv` at least raises `GoalsDivergedNotice`. Not fixed here — it is
a merge-semantics gap, not a dirty-tree one.

## Closing

Nothing open. Concerns 1 and 3 — your must-fixes — are both accepted, and 2, 4
and 5 are taken as specified. The net effect is what you predicted: a smaller diff
than the spec originally proposed.
