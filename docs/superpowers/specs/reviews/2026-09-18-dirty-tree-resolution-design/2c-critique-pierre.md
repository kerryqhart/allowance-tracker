# [2C] Critique — Pierre Portier

**Spec reviewed:** `/Users/kerryhart/Code/allowance-tracker/docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer:** Pierre Portier, Principal Technical Product Manager
**Date:** 2026-09-18

---

## Overall Verdict

**Approve with changes.**

Most of this document is exactly the kind of spec where my lens does not apply:
atomic writes, one staging implementation, removing a hard reset. No user-facing
surface moves. Sections 1, 2 and 4 I have nothing to add to — they are internal
correctness work, and correctly scoped.

Section 3 is different. It introduces a *new user-visible terminal state* — "sync
for this child stops, and says so" — and the spec spends one paragraph on it. The
thing one must understand is that this is the first place in the product where the
app says "I have stopped, you must go fix something." That deserves more than a
row in an error table. My concerns are all downstream of that one paragraph.

## Top Concerns

### Concern 1: "and says so" — but says so to whom, exactly?

**What I see:** §3 ends with "Sync for that child stops, and says so." The
mechanism is `SyncFailureNotice` + `SyncStatus::Error`.

But tell me — where does the user actually read this? I looked. `SyncStatus` is
written in roughly thirty places in `app_coordinator.rs` and **read by no UI
component at all** — `self.sync.status` appears only in `app_coordinator.rs` and
`sync_state.rs`. It is a write-only field today. And `SyncFailureNotice` has
exactly one reader: `render_sync_notices` in `lgs_sync_modal.rs:614`, inside a
120px scroll area, inside Settings → "Sync with another Mac…" — a modal a parent
opens perhaps twice in the lifetime of the app, during setup.

**Why it matters:** The defect this spec is fixing is *a silent stall*. The fix
replaces a stall that is invisible-in-the-logs with a stall that is
invisible-unless-you-open-a-settings-modal. From where the user sits — a parent
recording Tuesday's allowance on the kitchen Mac while the study Mac quietly
stopped syncing eleven days ago — the observable behavior is unchanged. Two Macs
disagree about a child's balance, and nothing on either screen suggests why. The
spec's own test is named "no-silent-stall invariant," but the invariant it
actually asserts is "a notice object exists," which is not the same thing.

**Recommendation:** State in the spec where this becomes visible on a surface the
user is already looking at. Something small is enough — a badge or a line near
the child picker when `sync_failures` is non-empty, that opens the sync modal.
And say plainly in the spec that `SyncStatus::Error` currently has no reader, so
the author does not believe the status write is doing work it is not doing.

### Concern 2: "informational" and "sync is dead" render identically

**What I see:** `render_sync_notices` draws all three notice kinds —
`sync_failures`, `goals_diverged`, `fast_forward_blocked` — through the same
`render_sync_notice_line`: child id in red, detail in grey. Three parallel `Vec`s,
three ad-hoc format strings at the call site.

**Why it matters:** `FastForwardBlockedNotice` means *"we handled it, nothing for
you to do."* The new backstop means *"this child is not syncing until you act."*
They look the same. A user seeing two red lines against the same child has no way
to tell which one is the emergency. And because each `record_*` is
replace-not-accumulate *per kind*, one child can legitimately hold three notices
at once, in a fixed non-priority order, with the blocking one possibly scrolled
out of a 120px box.

This is the engineering model on the product surface: three fields exist because
three code paths needed somewhere to put something, not because the user
distinguishes three concepts. The user has one concept — *something about this
child's sync needs your attention* — with a severity and sometimes an action.

**Recommendation:** Before adding a fourth condition to this model, collapse it.
One entity:

```rust
struct SyncNotice { child: ChildId, severity: Informational | Blocking, … }
```

One list, sorted blocking-first. The three `record_*` helpers become one. This is
a small change now and a migration later, and it is the one entity decision in
this spec I would push on.

### Concern 3: The notice names a `child_id`, not a child

**What I see:** `render_sync_notice_line(ui, &notice.child_id, …)` prints the raw
id. The existing goals notice reads, verbatim: *"goals.csv diverged between
<oid> and <oid> and was not merged — reconcile by hand."* The new backstop
would follow suit with something like "could not stage its local
transactions.csv."

**Why it matters:** This is a family allowance app. The user knows *Amélie*; they
do not know `child_7f3a…`, and they certainly do not know what a git oid is or
what "stage" means. The information is available — `RegistryEntry`
(`child_registry.rs:22`) carries both `label` (display name) and `path` (the
folder). The product is choosing not to use them.

**Recommendation:** Specify the user-facing sentence in the spec, and make it name
the child by label and the file by its human path. Roughly: *"Amélie's sync is
paused. A file in her folder was changed outside the app and the app will not
overwrite it: `notes.txt`."* Plus a **"Show the folder"** button — `path` is right
there, and it converts a dead end into something a parent can act on. Without it,
the remediation instruction is "open Terminal and run git," which is not an
instruction this product can give.

### Concern 4: `Failed(String)` bakes the wording into the sync engine

**What I see:** `DirtyTreeResolution::Failed(String) // user-facing message`.

**Why it matters:** The moment the prose lives inside a `String` produced deep in
merge code, every improvement to the wording — naming the child, naming the
folder, offering an action, softening the tone — becomes an edit to
`app_coordinator.rs`. And the UI can never enrich it, because by then it is an
opaque sentence. This is precisely the mechanism by which "stage", "dirty tree"
and "oid" reached the screen in the first place.

**Recommendation:** Make the variant carry *structure*, not prose — which file,
which condition — and let the modal compose the sentence. Same work, and the
product surface stays where the product can change it.

### Concern 5: The spec does not say what "sync stops" costs the user

**What I see:** §3: "Sync for that child stops." The cycle diagram shows the guard
raising the notice and discarding the merge — so push never runs either.

**Why it matters:** That means this machine's *own* new transactions stop reaching
the other Mac. From the user's side: the app works perfectly, transactions save,
balances update — and silently do not travel. The other Mac shows no error at all,
because nothing is wrong there. A parent could record three weeks of allowance
believing both machines agree.

Also unstated: does this resolve itself? `clear_sync_failure` is called on
`Applied` (`app_coordinator.rs:1249`, `:1617`), so once the user tidies the folder
the next timer tick heals and the notice disappears. That is good behavior — but
the spec says "a human must resolve it," which reads as a dead stop. Those are
very different promises.

**Recommendation:** Two sentences in the spec: (a) the blast radius — the app
keeps working, no data is lost, this Mac and the other stop agreeing until it is
fixed; (b) that recovery is automatic on the next cycle once the file is
resolved, with no button to press. Then make the notice text say (b) too, so the
user is not hunting for a "retry" that does not exist.

## Questions the spec does not answer

- After `parental_control_attempts.csv` joins `FILES_THIS_APP_OWNS`, what is
  actually left that is *tracked but unowned*? If the honest answer is "nothing,
  unless something has gone wrong elsewhere," then the backstop is a
  should-never-happen assertion and should be worded to the user as *"something
  unexpected happened"* — not as a normal operating condition with an implied
  routine. If the answer is "the user may drop files in the child folder," that
  is a supported product behavior and halting sync forever is a harsh response.
- Are child folders exposed to the user in Finder? The spec mentions a
  "child-folder UI" as the reason for the dotted temp prefix. If a parent can see
  and touch that folder, someone eventually will, and this backstop is their
  first encounter with the consequences.
- Can a child be in the backstop state *and* be the currently-selected child, with
  the main screen showing a balance the other Mac does not share? That is the
  moment the user is most likely to be misled, and the surface where a hint would
  cost least.

## What I thought was well-handled

The root-cause framing — *inference versus evidence* — is genuinely good, and it
is what makes the four changes read as one design rather than four patches. The
`parental_control_attempts.csv` append exception is exactly the right call and,
better, is *recorded as a conscious trade* so a future reader does not helpfully
undo it. Declaring the marker's retirement an explicit non-goal, with the reason,
is the kind of restraint specs usually lack.

And §2's observation that the "change it in both places" comment was itself
evidence of a liability — that is the correct instinct, applied well.

## Closing

The technical core is ready. The one paragraph that creates a user-visible
terminal state is not — it names a behavior without naming what the user sees,
what it costs them, or how it ends. Tighten §3 along the lines above (especially
Concerns 1 and 2, which are where the entity model is deciding something) and this
is good to proceed.
