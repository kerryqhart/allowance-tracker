# MCP Write Reliability Design

**Date:** 2026-05-08
**Author:** Kerry (with Claude)
**Status:** Draft, pending implementation
**Repos touched:** `allowance-tracker` (sync-service), `zephytop-brain` (MCP Lambda)
**Follow-up:** local-app `push_events` removal (separate plan)

## Problem

On 2026-05-07, an MCP `add_expense` call from claude.ai produced two duplicate `Yoshi stuffie -$28` entries in the cloud, 5 min 4 sec apart. Symptom: claude.ai's request timed out (~5 min) and was retried; the MCP minted a fresh `transaction_id` from `now()` on each attempt, so retries hit different DynamoDB rows and both succeeded.

Two underlying issues:

1. **No idempotency on retry.** `tool_add_expense` generates `transaction_id = format!("transaction::expense::{timestamp_ms}")` on each invocation. Identical inputs produce different IDs.
2. **Two-phase write with no atomicity.** The MCP issues `PUT /internal/entities/...` followed by `POST /internal/sync/events`. Either can fail independently. A successful entity write with a failed event push leaves an orphan: the row exists in cloud but no sync event ever logs the change, so the local app can never pull it down.

The orphan class is structurally identical to a separate Pig stuffie symptom we observed: an MCP-written cloud entry that never appeared in the local app, with no obvious cause. Whether or not that specific row was a victim of partial failure, the architecture allows it.

## Goals

- Eliminate duplicate cloud entries from claude.ai timeout-retry of `add_expense`.
- Eliminate the orphan class (entity-yes / event-no, or vice versa) by construction.
- Keep the change minimal in surface area while preserving room for the local-app cleanup as a follow-up.

## Non-goals

- Balance recompute on sync (separate spec, topic "(b)").
- Removing `POST /internal/sync/events` from the wire (follow-up after local-app cleanup).
- Updating `tool_add_expense` semantics beyond idempotency (no new fields, no behavior change for first-call success).

## Design

### Server-side: atomic entity + event write

The sync-service `PUT /internal/entities/{entity_type}/{child_id}/{entity_id}` becomes the single point of truth for entity changes. It atomically writes the entity and a corresponding sync event in one DynamoDB `TransactWriteItems` call.

**Endpoint contract (revised):**

- Request: same as today (path identifies entity, body is JSON entity).
- Optional header: `X-Sync-Source: local | remote | mcp`. Default: `remote`.
- Server logic:
  1. Determine action by checking whether the entity already exists. `Created` if absent, `Updated` if present.
  2. Compute deterministic `event_id`:
     - `Created`: `ev::created::{entity_id}`
     - `Updated`: `ev::updated::{entity_id}::{content_sha8}` where `content_sha8` is the first 8 hex chars of SHA-256 over the new entity body.
  3. Build the `SyncEvent` with that `event_id`, the determined action, the source from header, and a server-assigned timestamp.
  4. Issue `TransactWriteItems`:
     - Put on entities table (unconditional upsert).
     - Put on `sync_events` table with `ConditionExpression: attribute_not_exists(event_id)`.
  5. If the transaction succeeds, return 200.
  6. If the transaction fails because of the event condition (`TransactionCanceledException` with reason `ConditionalCheckFailed` on the event item): the event already exists from a prior call with the same id (`Created`) or same content (`Updated`). The prior call's writes are already in DynamoDB. Swallow the failure and return 200 without re-issuing any writes.
  7. Other failures: return 5xx, client retries.

Step 6 is correct because `TransactWriteItems` is all-or-nothing: if our event condition fails, our entity write also rolled back. The state currently in DynamoDB is from the prior call (or, if a concurrent writer raced us, theirs). Either way the entity reflects an already-accepted version of "this change", which is exactly what a retry expects to find. Re-issuing the entity put would risk clobbering an intervening manual edit and is unnecessary.

**Sequence allocation for events.** The current `push_event` server flow allocates a sequence via DynamoDB atomic counter. With `TransactWriteItems`, sequence allocation must happen before the transaction; if the conditional fails (event_id collision), the allocated sequence is wasted. Implementation note: the existing `push_event` code in `sync-service/src/storage/dynamo.rs` already encapsulates sequence allocation; the new code should reuse that helper but invoke the put inside the transaction rather than as a standalone call. Wasted sequence numbers are tolerable — sequences are monotonic, not contiguous.

**`POST /internal/sync/events` is preserved.** The local app still uses it. We do not deprecate it in this spec.

### MCP-side: deterministic transaction_id with bucket lookback

`tool_add_expense` in `zephytop-brain/services/allowance-tracker/src/mcp.rs`:

```
1. Validate amount > 0.
2. now = Utc::now().
3. current_bucket = floor(now to 1-hour boundary).
4. prev_bucket    = current_bucket - 1h.
5. current_id = deterministic_id("transaction::expense", child_id, amount, description, current_bucket).
6. prev_id    = deterministic_id("transaction::expense", child_id, amount, description, prev_bucket).
7. For id in [current_id, prev_id]:
       if Some(existing) = client.get_entity("transaction", child_id, &id)?:
           return response_from(existing).
8. // First-time path
   current_balance = client.get_balance(child_id)?.unwrap_or(0.0).
   new_balance     = current_balance - amount.
   tx = json!{
       id:               current_id,
       child_id:         child_id,
       date:             now.to_rfc3339(),
       description:      description,
       amount:           -amount,
       balance:          new_balance,
       transaction_type: "Expense",
   }.
   client.put_entity("transaction", child_id, &current_id, &tx_json)?.
   // server now emits the event atomically; no separate push call.
9. return success(description, -amount, new_balance).
```

**`deterministic_id` helper:**

```
fn deterministic_id(
    prefix:      &str,
    child_id:    &str,
    amount:      f64,
    description: &str,
    bucket:      DateTime<Utc>,
) -> String {
    let bucket_str   = bucket.format("%Y%m%dT%H").to_string();
    let amount_cents = (amount * 100.0).round() as i64;   // stable f64 rep, dodges NaN
    let mut h = Sha256::new();
    h.update(child_id.as_bytes());
    h.update(amount_cents.to_be_bytes());
    h.update(description.trim().as_bytes());
    h.update(bucket_str.as_bytes());
    let short = &hex::encode(h.finalize())[..16];          // 64 bits is plenty for this scale
    format!("{prefix}::{bucket_str}::{short}")
}
```

Example output: `transaction::expense::20260508T03::a1b2c3d4e5f6a7b8`

**`hour_bucket` helper:**

```
fn hour_bucket(now: DateTime<Utc>) -> DateTime<Utc> {
    let secs = (now.timestamp() / 3600) * 3600;
    DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
}
```

**Crate adds:** `sha2 = "0.10"`, `hex = "0.4"`.

**Removed:** all logic in `tool_add_expense` after the entity put, related to building and pushing a sync event. The server now does that.

### Bucket sizing and lookback rationale

The bucket boundary is the only window in which a retry can produce a different deterministic ID. With a 1-hour bucket:

- Single-retry safety (5 min separation): same bucket 92% of the time. Lookback to the previous bucket catches the remaining 8%.
- Two-retry safety (10 min span): same bucket 83%. Lookback covers the rest.
- For any retry within 60 minutes total: 100% safe.

If retries somehow extend beyond 60 minutes (claude.ai does not currently behave this way), a third-bucket lookback could be added. We do not add it preemptively.

### Source field

The `source` field on `SyncEvent` records who originated the change. Currently set client-side. After this change, the server populates it from the `X-Sync-Source` header on PUT (default `remote`). The MCP sets `X-Sync-Source: remote` (the local app, when it eventually moves to relying on auto-emit, would set `local`).

A TODO accompanies this: investigate whether the engine in `backend/domain/sync_manager.rs` and `backend/domain/sync_thread.rs` actually consumes `source`. The `poll_remote` path appears to ignore it. If `source` is purely cosmetic, drop it as part of the local-app cleanup follow-up.

## Test plan

**sync-service:**

- Unit: `event_id` derivation for `Created` action returns `ev::created::{entity_id}`.
- Unit: `event_id` derivation for `Updated` action returns `ev::updated::{entity_id}::{content_sha8}` where the hash matches a known fixture.
- Integration (DynamoDB Local in `sync-service/tests/`):
  - Two identical PUTs to `/internal/entities/transaction/keiko_hart/tx-1` produce exactly one row in entities table and exactly one row in `sync_events`.
  - PUT to existing entity with new content produces an `Updated` event; events table has two rows.
  - `TransactWriteItems` failure simulated (force a transient error) leaves both tables unchanged.

**zephytop-brain MCP:**

- Unit: `deterministic_id` returns the same string for identical inputs across calls.
- Unit: `deterministic_id` returns distinct strings when any one input varies (each of: `child_id`, `amount`, `description`, `bucket`).
- Unit: `hour_bucket` rounds down to the hour for several timestamps including DST boundaries (UTC has none, but verify behavior).
- Unit: `amount_cents` rounding is stable for representative dollar values (`1.23`, `0.07`, `100.0`, `28.0`).
- Integration (mock `SyncClient` or local sync-service):
  - Two consecutive `tool_add_expense` calls with identical inputs: first writes, second hits lookback on `current_id` and returns the same response without issuing a second PUT.
  - Cross-bucket retry: first call at minute 55 of bucket N, second call at minute 5 of bucket N+1 with mock clock. Second call hits lookback on `prev_id`, returns same response.
  - Two `tool_add_expense` calls with different `description` produce two distinct entries (no false dedup).

## Rollout

1. **Server first.** Deploy sync-service with auto-emit on PUT. The existing `POST /internal/sync/events` continues to work — local app keeps calling it and produces duplicate events that are harmlessly idempotent on the local-app pull side. Verify in production that auto-emit produces correctly-formed events.
2. **MCP second.** Deploy zephytop-brain MCP with deterministic IDs and the explicit `push_sync_events` call removed. From this moment forward, MCP retries are silent no-ops.
3. **Local app cleanup (follow-up plan).** Remove `remote.push_events(...)` from `backend/domain/sync_thread.rs:240,271`. Adjust tests. Ship as a separate PR.
4. **Optional cleanup (follow-up plan).** Remove `POST /internal/sync/events` from sync-service entirely once no client uses it.

## Open questions

- **Does anything consume the `source` field on events?** Behavior of the engine appears to ignore it on the pull path. To be confirmed during implementation; if confirmed cosmetic, drop in the local-app follow-up.
- **Sequence wastage on event_id condition failure.** Acceptable in current design (sequences are monotonic, not contiguous). If wastage becomes a problem, alternative is to allocate sequence inside the transaction via a separate `UpdateItem` on a counter table — but that complicates the transaction. Defer.

## Out of scope

- Balance recompute on sync (`BalanceRecomputeOnSync` will be its own design). The denormalized `balance` field on each transaction row remains incorrect across divergent merges. Addressed separately.
- Goals / children parallel changes. The auto-emit applies to all entity types via the generic `/entities/` path, but their MCP tooling does not yet exist beyond `add_expense`. When new write tools are added, they get idempotency for free if they use deterministic IDs.
