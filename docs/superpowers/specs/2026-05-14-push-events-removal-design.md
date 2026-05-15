# Push Events Removal Design

**Date:** 2026-05-14
**Author:** Kerry (with Claude)
**Status:** Draft, pending implementation
**Repos touched:** `allowance-tracker` (backend + sync-service)
**Predecessor:** [MCP Write Reliability](2026-05-08-mcp-write-reliability-design.md) — shipped 2026-05-08

## Problem

After the MCP write-reliability work, `PUT /entities/...` atomically writes the entity and emits a corresponding sync event server-side. The local app's `sync_thread::push_event` still issues a redundant `POST /sync/events` after each `upsert_entity`, producing duplicate events that survive only because of server-side deterministic-`event_id` dedup. The duplicate write path is harmless but carries real cost: dead code in the trait, the mock, the HTTP client, and a deployed server route nobody needs.

A symmetric gap exists on the DELETE side: the server's `DELETE /entities/...` removes the entity but does **not** emit a sync event. Today, deletes propagate to other clients only because the local app follows up with `push_events`. Removing `push_events` without filling this gap would silently break delete propagation — entities would disappear from DynamoDB but other clients polling for events would never learn of the deletion.

## Goals

- Remove the redundant `POST /sync/events` write path from local app and server.
- Bring the DELETE path to parity with PUT: server emits a `Deleted` sync event atomically with the row delete.
- Preserve the echo-skip semantics on the pull path (local app must not re-apply its own writes when they come back as remote events).

## Non-goals

- Balance recompute on sync (separate spec).
- The brand-new-child 500 edge case in `upsert_entity_with_event` (separate follow-up).
- Any change to the polling / watermark logic.
- Backwards compatibility with old local-app versions calling `POST /sync/events` (single-operator deployment — see Sequencing).

## Background: echo-skip and X-Sync-Source

`sync_manager.rs::poll_child` skips events with `source == SyncSource::Local`, so the local app does not re-apply its own writes that it sees come back via polling. The server's `PUT /entities` reads the `X-Sync-Source` header to set the source on the event it emits (default: `Remote`).

Today the local app's `HttpRemoteClient::upsert_entity` **does not** send the header. Its own writes therefore come back from the server with `source=Remote`. The echo-skip still works only because the local app *also* calls `push_events` with the original event (`source=Local`) — and the deterministic-event_id dedup on the server preserves the `Local`-tagged copy.

Removing `push_events` without adding `X-Sync-Source: local` to `upsert_entity` would break echo-skip: every local write would come back as `Remote` and be re-applied, redundantly hitting the UI thread's apply path. So the header add is a hard prerequisite, not optional polish.

## Design

### Server: `delete_entity_with_event`

Add `DynamoStore::delete_entity_with_event(child_id, entity_type, entity_id, source) -> Result<()>` modeled on the existing `upsert_entity_with_event`:

1. Compute deterministic `event_id`: `ev::deleted::{entity_id}`.
2. Issue `TransactWriteItems`:
   - `Delete` on the entities table (unconditional — idempotent if already gone).
   - `Put` on `sync_events` with `ConditionExpression: attribute_not_exists(event_id)`, action `Deleted`, given source, server-assigned timestamp.
3. On `ConditionalCheckFailed` for the event item: a prior call already deleted this entity and recorded the event. Swallow and return Ok — same reasoning as the upsert path.
4. Sequence allocation reuses the same helper as `upsert_entity_with_event`. Wasted sequences on dedup are tolerable.

Update `routes/entities.rs::delete_entity`:

- Read `X-Sync-Source` header. Default `Remote`, accept `local` → `Local` (same parsing as `upsert_entity`).
- Call `delete_entity_with_event` instead of `delete_entity`.

The unparameterized `DynamoStore::delete_entity` becomes unused after this change. Remove it.

### Server: remove `POST /sync/events`

Delete the `push_events` handler, the `POST /sync/events` route registration, and the `PushEventsRequest` / `PushEventsResponse` types in `sync-service/src/routes/sync.rs`. `GET /sync/events` (poll path) stays.

`DynamoStore::push_event` becomes unused after the route is removed. Remove it along with any internal helpers used only by it.

### Server: tests

- Remove `sync-service/tests/api_test.rs::test_push_events_endpoint`.
- Extend `sync-service/tests/atomic_upsert_test.rs` (or add a sibling test file) with cases covering:
  - DELETE produces a `Deleted` sync event with the source from the header.
  - DELETE is idempotent: deleting an already-deleted entity returns success and does not write a duplicate event.
  - DELETE missing `X-Sync-Source` defaults to `Remote` (parity with PUT).

### Local app: HTTP client

In `backend/storage/http_remote.rs`:

- `upsert_entity`: add request header `X-Sync-Source: local`.
- `delete_entity`: add request header `X-Sync-Source: local`.
- Remove `push_events` method entirely.

### Local app: sync thread

In `backend/domain/sync_thread.rs::push_event`:

- Delete branch (currently lines 236–242): replace the `delete_entity` + `push_events` pair with a single `remote.delete_entity(...)` call. The crash-ordering comment ("delete the entity body first so that a crash between the two calls leaves a dangling event") becomes moot — the server now performs both as an atomic transaction.
- Upsert branch (currently lines 268–272): drop the `push_events` line after `upsert_entity`.

### Local app: sync manager

In `backend/domain/sync_manager.rs`:

- Remove `enqueue_event` method, `push_pending` method, and the `pending_push: Vec<SyncEvent>` field. These are no longer reachable from production code; only tests use them.
- Remove the corresponding unit tests that exercise the enqueue/push_pending path.
- Rewrite `backfill()` to call only `upsert_entity` per entity. Drop the event accumulator, the batch size, and the `push_events` flush calls. The server emits sync events atomically from each PUT. Update `test_backfill_pushes_all_entities` to assert on remote events the server emits, not on events the client pushed.
- Keep `poll_child`'s `if event.source == SyncSource::Local { continue; }` echo-skip. The X-Sync-Source: local header preserves its semantics.

### Local app: RemoteStorage trait

In `backend/storage/remote.rs`:

- Remove `push_events` from the `RemoteStorage` trait.
- Remove the implementation from `backend/storage/mock_remote.rs` and any tests that exercise it directly. Tests that called `mock.push_events(...)` to seed remote state should instead seed via `upsert_entity` (which, in the mock, should now also emit an internal event to mirror server behavior — see Mock parity below).

### Mock parity

The mock currently models `upsert_entity` and `push_events` as independent operations, matching the old wire protocol. With `push_events` gone, `MockRemoteClient::upsert_entity` and `delete_entity` must emit a sync event internally so that polling tests see the entity change. This mirrors what the deployed server now does.

Implementation: in `MockRemoteClient`, `upsert_entity` appends a `SyncEvent` to the internal event log with a monotonic sequence, action determined by whether the row existed (`Created` vs `Updated`), source `Remote`. `delete_entity` appends a `Deleted` event, source `Remote`. Tests that previously called `push_events(...)` to seed `Local`-sourced events for echo-skip coverage migrate to a new test-only helper `MockRemoteClient::seed_event(event)` that bypasses the upsert/delete entry points and writes directly to the event log.

## Deploy sequencing

This is a single PR but two deployable artifacts (local app, sync-service). The deploy order matters:

- **Local app first, then server:** safe. New local app stops calling `POST /sync/events`. Old server still has the route; it sits unused.
- **Server first, then local app:** unsafe for two reasons. (1) New server lacks `POST /sync/events`; an old local app calls it and 404s, queuing failed events for retry indefinitely. (2) New server's DELETE emits events tagged `Remote` (no header from old client); old local app polls them back and re-applies the delete to an already-deleted entity. Even if `DeleteLocalEntity` tolerates that idempotently, reason (1) alone disqualifies this ordering.

Recommended: deploy local app build first, verify, then deploy sync-service.

## Risk

- **Delete idempotency on the local app:** the new server-emitted Delete event will be observed by the source client when it polls (unless `X-Sync-Source: local` is sent on DELETE, which it will be after this PR). During the deploy window where local app has the header but server hasn't shipped yet, deletes still go via `push_events`, so this is fine. After both ship, the header tags server-emitted events as Local, echo-skip drops them on poll. Net: no regression.
- **Mock test reshape:** the test suite changes substantially. Risk of accidentally weakening coverage. Mitigation: review the diff with the question "for each removed test, is the behavior it covered tested elsewhere?"
- **Backfill rewrite:** disaster-recovery affordance is preserved but its event-emission semantics change (now server-side per entity, instead of batched client-side). Verify that re-running backfill on an already-populated remote still completes without errors (entity puts are unconditional upserts; event puts dedup on `event_id` per the upsert_entity_with_event design).

## Testing strategy

- All existing sync integration tests must pass.
- New tests in `sync-service/tests/atomic_upsert_test.rs` for the delete-with-event path.
- Manually verify on a dev DynamoDB Local: PUT entity → poll → see Local-sourced event → echo-skipped. DELETE entity → poll → see Local-sourced delete event → echo-skipped. Repeat from a second simulated client to confirm cross-client propagation.

## Out of scope, tracked for follow-up

- Balance recompute on sync.
- The brand-new-child 500 in `upsert_entity_with_event` (child has no metadata row → `attribute_exists(event_sequence)` fails).
- Renaming or unifying `entity_type` string parsing across routes.
