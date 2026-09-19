# [3C] Reply to Pierre Portier's Critique

**Spec:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer addressed:** Pierre Portier
**Reply date:** 2026-09-19

---

## Overall Response

Accepted, with one compromise. Your central observation — that the fix replaces a
stall invisible in the logs with a stall invisible unless the user opens a
settings modal — was correct and is fixed. Separately, your closing question
predicted the outcome of a decision taken elsewhere in this panel: the backstop
*is* a should-never-happen assertion, and it is now worded as one.

## Point-by-Point

### Concern 1: "and says so" — but says so to whom?

**Verdict:** Accepted.

**Our response:** Verified in the main session before ruling: `sync.status` does
not appear in any file under `egui-frontend/src/ui/components/`. It is a
write-only field, written in roughly thirty places and read by no UI component at
all. Your characterisation stands without qualification.

Two changes. The spec now states plainly that `SyncStatus::Error` currently has
no reader, so no future author believes the status write is doing work it is not.
And the failure surfaces where the user already looks: a badge near the child
picker when `sync_failures` is non-empty, which opens the sync modal. The user
chose this level ("medium") over both the minimal and the full restructure.

Your kitchen-Mac framing — a parent recording Tuesday's allowance while the study
Mac stopped syncing eleven days ago — is quoted in the spec's rationale, because
it is the clearest statement of why a notice object existing is not the same as
the user being told.

### Concern 2: "Informational" and "sync is dead" render identically

**Verdict:** Compromise.

**Our response:** You are right that three parallel `Vec`s are the engineering
model on the product surface, and right that adding a fourth condition to it
entrenches the wrong entity. But collapsing `sync_failures`, `goals_diverged` and
`fast_forward_blocked` into one `SyncNotice` reaches well beyond two data-loss
fixes, and this spec is not the place to restructure notice state.

Taken now: severity on the existing notices, sorted blocking-first, so a fatal
stall never renders identically to `FastForwardBlockedNotice`'s "handled, nothing
for you to do" — which is the half that actually misleads a user.

Left on the table: the single-entity collapse, recorded as a named follow-up in
the spec's scope boundaries rather than as an implicit someday. What that costs
is that three `record_*` helpers survive and the next condition still has to pick
a `Vec`.

Note that the pressure is lower than when you wrote: under the widening described
below, the blocking notice now fires only on genuine staging or commit failure,
not as a routine terminal state.

### Concern 3: The notice names a `child_id`, not a child

**Verdict:** Accepted.

**Our response:** "This is a family allowance app. The user knows *Amélie*; they
do not know `child_7f3a…`" is the right test and the app currently fails it.
`RegistryEntry` (`child_registry.rs:22`) carries both `label` and `path`, and the
product was choosing not to use them.

The spec now specifies the user-facing sentence rather than leaving it to be
improvised at the call site: the child named by label, the file named by its
human path, and a **"Show the folder"** button built on `path`. Your point that
without it the remediation instruction is "open Terminal and run git" — an
instruction this product cannot give — is the reason the button is in scope
rather than deferred.

### Concern 4: `Failed(String)` bakes the wording into the sync engine

**Verdict:** Accepted.

**Our response:** Agreed, and it amends what Greg's Concern 2 proposed. Greg
asked for a `thiserror` enum whose `Display` *is* the user-facing message; you
point out that putting prose inside a `String` produced deep in merge code is
exactly the mechanism by which "stage", "dirty tree" and "oid" reached the screen.

Both are satisfiable: the variants carry structure — which file, which condition
— and the modal composes the sentence. `Display` survives for logs and the error
chain, where a developer is the reader. Greg's one-definition principle holds; the
definition of the *user-facing* sentence just lives at the UI boundary.

### Concern 5: The spec does not say what "sync stops" costs the user

**Verdict:** Accepted.

**Our response:** Both sentences added. Blast radius: the app keeps working,
transactions save, no data is lost — this Mac and the other stop agreeing until
it is resolved, and the other Mac shows no error because nothing is wrong there.
Recovery: automatic on the next cycle once the file is resolved, since
`clear_sync_failure` already runs on `Applied` (`app_coordinator.rs:1249`,
`:1617`).

You were right that "a human must resolve it" read as a dead stop and promised
something different from what the code does. The notice text now says the
recovery is automatic, so the user is not hunting for a retry button that does
not exist.

## Questions Answered

### Q: After `parental_control_attempts.csv` joins `FILES_THIS_APP_OWNS`, what is left that is tracked but unowned? If nothing, the backstop is a should-never-happen assertion and should be worded as such.

A: You called this correctly, and the answer arrived from Deiko's Concern 1
rather than from the list membership. The guard no longer uses the allowlist at
all: it stages every *tracked* path via `index.update_all`, on the reasoning that
the allowlist exists to block *untracked* strays and a tracked file is already in
pushed history. Since `working_tree_dirty` already ignores untracked files, any
tree that enters the guard has a tracked change to stage — so the
"nothing to commit" branch is unreachable.

It survives only as a should-never-happen, worded to the user as "something
unexpected happened", exactly as you specified. (A side effect:
`parental_control_attempts.csv` no longer needs to join the owned list, so that
fold-in is reverted.)

### Q: Are child folders exposed to the user in Finder?

A: Yes — and the spec now says so where it matters. The "Show the folder" button
in Concern 3 makes that exposure deliberate rather than incidental. Your inference
holds: if a parent can see and touch the folder, someone eventually will.

### Q: Can a child be in the backstop state *and* be the currently-selected child, showing a balance the other Mac does not share?

A: Yes. That is the case the child-picker badge in Concern 1 is aimed at — it is
visible on the main surface precisely when the selected child is the affected one.
Added to the spec as the rationale for putting the badge there rather than only in
the modal.

## Closing

One item open by design: the single-`SyncNotice` collapse (Concern 2), recorded as
a named follow-up rather than absorbed into a data-loss fix. Everything else is
accepted and reflected in the spec.
