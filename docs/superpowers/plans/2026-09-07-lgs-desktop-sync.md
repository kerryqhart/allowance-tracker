# Desktop-to-Desktop Sync over lgs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace iCloud-as-transport with lgs so two Macs converge on the same child data, without a terminal and without silent data loss.

**Architecture:** Each child is a git repo registered as one lgs project. The lgs daemon syncs *bare↔cloud* only, so the app does `git push`/`git fetch` itself. Concurrent edits are resolved by a **pure three-way semantic merge** (a textual merge is disqualified because `balance` is a stored running total), then a canonical re-render so both machines converge on *bytes*.

**Tech Stack:** Rust, git2 (libgit2), lgs CLI (bundled), egui, csv, serde, proptest.

**Spec:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md` — read it before Task 1. The plan argues from the spec; where they disagree, the spec wins.

## Global Constraints

- **Two machines only.** Every property here is 2-way. Do not add a third peer to any test.
- **Merge is pure.** `allowance-core` must not touch the filesystem, the network, or a repository. Enforced by Task 2 Step 6.
- **Canonical row order is `(date, id)`**, applied on *every* write, not only after a merge.
- **Money is `Money(i64)` cents.** No `f64` arithmetic on money anywhere in new code.
- **The AWS JSON wire format must not change.** The domain `Transaction` is serialized directly to the sync-service (`app_coordinator.rs:512-526`) and read by the MCP Lambda in the **zephytop-brain** stack, which this repo cannot deploy.
- **Fetch `refs/lgs-auth/heads/*`**, never `refs/heads/*`, as the merge input.
- **`git` is a runtime prerequisite.** lgs shells out to it. Never claim otherwise in code comments, UI copy, or docs.
- **All working-tree mutation happens on the UI thread.** Background threads touch `.git` only.
- **No compound shell commands** (`&&`, `;`, `|`, `$()`) in any script or docs this plan produces. One command per line. Use `git -C <dir>`.
- **Existing fixtures are extended, not duplicated:** `backend/storage/csv/test_utils.rs` (`TestHelper`, `TestEnvironment`). `tempfile` is already a dev-dependency.

---

## File Structure

**New crate `allowance-core/`** — pure, no I/O. This is where the correctness risk lives, so it is isolated by construction:

| File | Responsibility |
|---|---|
| `allowance-core/src/money.rs` | `Money(i64)` cents, wire-compatible serde, canonical 2-decimal rendering |
| `allowance-core/src/row.rs` | `TxRow`, `Sided<T>`, `Provenance` — the merge's data model |
| `allowance-core/src/codec.rs` | `parse_transactions` / `render_transactions`, canonical ordering. **Note:** `parse_transactions` returns `ParsedTransactions { rows, rows_rounded }`, not a bare `Vec<TxRow>` — Task 5 changed this so the legacy-precision rounding count cannot be silently dropped. Later tasks use `.rows`. |
| `allowance-core/src/merge.rs` | `merge(base, ours, theirs)` — the resolution table |
| `allowance-core/src/balance.rs` | pure `recompute_running_balances` / `validate` |

**Modified in the existing tree:**

| File | Change |
|---|---|
| `backend/domain/models/transaction.rs` | `amount`/`balance` become `Money`; real random id suffix |
| `backend/storage/csv/transaction_repository.rs` | delegate to `codec`; drop the `Utc::now()` fallback |
| `backend/domain/balance_service.rs` | thin wrapper over pure `balance` |
| `backend/storage/git/mod.rs` | remote ops + injectable clock |
| `backend/domain/sync_manager.rs` | `SyncMessage::ApplyMerge` |
| `backend/sync/lgs_client.rs` *(new)* | `parse_status` (pure) + `run` |
| `backend/sync/paths.rs` *(new)* | `SyncPaths`, `is_cloud_synced` |
| `backend/sync/child_sync.rs` *(new)* | `ChildSyncEngine` |
| `backend/sync/migration_lgs.rs` *(new)* | migrate a cloud-drive child into lgs |
| `shared/src/lib.rs` | delete dead `Transaction::generate_id` |

---

## Phase 0 — Unblock

### Task 1: Spike — can this build's libgit2 push to a local lgs daemon?

Everything in Phase 2 and 3 depends on the answer. `egui-frontend/Cargo.toml:48` declares `git2 = { version = "0.19", default-features = false }` — no `https`, no `ssh`. lgs serves `http://localhost:<port>/<name>.git`.

**Files:**
- Create: `egui-frontend/tests/spike_git2_http.rs`
- Modify: `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md` (the "Blocking spike" section)

**Interfaces:**
- Consumes: nothing.
- Produces: a recorded verdict. If push fails, Phase 2 changes shape before it starts.

- [ ] **Step 1: Confirm a daemon is up and get a clone URL**

Run: `lgs status --json`

Read `daemon.state` (must be `ok`) and copy any project's `clone_url`. If `daemon.state` is `down`, ask the user before starting anything — do not start a daemon unprompted.

- [ ] **Step 2: Write the spike test**

```rust
// egui-frontend/tests/spike_git2_http.rs
//! Spike: does `git2` with default-features = false speak smart-HTTP to lgs?
//! Ignored by default — it needs a live daemon. Run explicitly:
//!   cargo test -p allowance_tracker_egui --test spike_git2_http -- --ignored --nocapture

#[test]
#[ignore]
fn git2_can_clone_fetch_and_push_over_local_http() {
    let url = std::env::var("LGS_CLONE_URL")
        .expect("set LGS_CLONE_URL to a clone_url from `lgs status --json`");
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");

    // 1. Clone.
    let repo = git2::Repository::clone(&url, &work)
        .expect("CLONE FAILED — libgit2 has no usable http transport");
    println!("clone: OK");

    // 2. Commit something.
    std::fs::write(work.join("spike.txt"), "spike").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("spike.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("spike", "spike@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "spike", &tree, &[&parent]).unwrap();

    // 3. Push — the operation most likely to be missing.
    let mut remote = repo.find_remote("origin").unwrap();
    let head = repo.head().unwrap();
    let branch = head.shorthand().unwrap();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    remote.push(&[refspec.as_str()], None)
        .expect("PUSH FAILED — receive-pack unsupported under these feature flags");
    println!("push: OK");

    // 4. Fetch the lgs-auth mirror namespace the real design depends on.
    remote.fetch(&["+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*"], None, None)
        .expect("FETCH of refs/lgs-auth/* FAILED");
    println!("fetch refs/lgs-auth/*: OK");
}
```

- [ ] **Step 3: Run it**

Run: `LGS_CLONE_URL=<url from Step 1> cargo test -p allowance_tracker_egui --test spike_git2_http -- --ignored --nocapture`

Expected: three `OK` lines. Any panic is the answer, not a bug to fix.

- [ ] **Step 4: Record the verdict in the spec**

Replace the spec's "Blocking spike before planning" body with what happened — the date, the git2 version, and which of the three operations worked.

If push failed, pick an exit **and write down which**, because it changes later tasks:
- Enable `https` on git2 (`default-features` back on) and accept the build weight; Phase 2/3 are unchanged.
- Shell out to `git` for clone/fetch/push. `GitManager`'s new methods in Task 13 become `Command::new("git")` wrappers, and `git` moves from "prerequisite lgs needs" to "prerequisite we need directly."

- [ ] **Step 5: Commit**

```bash
git add egui-frontend/tests/spike_git2_http.rs docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md
git commit -m "spike: record whether git2 default-features=false can push to lgs"
```

---

## Phase 1 — Data correctness (no lgs dependency; can run in parallel with Task 1)

### Task 2: `allowance-core` crate with `Money`

**Files:**
- Create: `allowance-core/Cargo.toml`, `allowance-core/src/lib.rs`, `allowance-core/src/money.rs`
- Create: `allowance-core/tests/no_io.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: nothing.
- Produces: `Money` with `Money::from_cents(i64) -> Money`, `Money::cents(&self) -> i64`, `Money::render(&self) -> String`, `impl Add/Sub/Neg/Sum`, `Serialize`/`Deserialize` that read **and write JSON numbers** (wire-compatible), and `FromStr`.

- [ ] **Step 1: Write the failing tests**

```rust
// allowance-core/src/money.rs  (bottom of file)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_two_decimals_always() {
        assert_eq!(Money::from_cents(500).render(), "5.00");
        assert_eq!(Money::from_cents(-250).render(), "-2.50");
        assert_eq!(Money::from_cents(0).render(), "0.00");
        assert_eq!(Money::from_cents(5).render(), "0.05");
        assert_eq!(Money::from_cents(-5).render(), "-0.05");
    }

    #[test]
    fn addition_is_exact_where_f64_is_not() {
        // 0.1 + 0.2 != 0.3 in f64. In cents it is exact.
        let sum = Money::from_cents(10) + Money::from_cents(20);
        assert_eq!(sum, Money::from_cents(30));
    }

    #[test]
    fn parses_the_strings_the_existing_csv_contains() {
        // f64::to_string() output that is already on disk today.
        assert_eq!("5".parse::<Money>().unwrap(), Money::from_cents(500));
        assert_eq!("5.0".parse::<Money>().unwrap(), Money::from_cents(500));
        assert_eq!("5.5".parse::<Money>().unwrap(), Money::from_cents(550));
        assert_eq!("-2.25".parse::<Money>().unwrap(), Money::from_cents(-225));
        assert_eq!("0".parse::<Money>().unwrap(), Money::from_cents(0));
    }

    #[test]
    fn rejects_more_precision_than_cents() {
        assert!("5.005".parse::<Money>().is_err());
    }

    #[test]
    fn json_round_trip_is_a_number_not_a_string() {
        let m = Money::from_cents(1234);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, "12.34", "the AWS wire format must stay a JSON number");
        assert_eq!(serde_json::from_str::<Money>(&json).unwrap(), m);
    }

    #[test]
    fn json_accepts_values_already_stored_in_dynamodb() {
        assert_eq!(serde_json::from_str::<Money>("5").unwrap(), Money::from_cents(500));
        assert_eq!(serde_json::from_str::<Money>("5.0").unwrap(), Money::from_cents(500));
        assert_eq!(serde_json::from_str::<Money>("-2.5").unwrap(), Money::from_cents(-250));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance-core money`
Expected: FAIL — the crate does not exist yet.

- [ ] **Step 3: Create the crate**

```toml
# allowance-core/Cargo.toml
[package]
name = "allowance-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
csv = "1"
thiserror = "1"

[dev-dependencies]
proptest = "1"
```

Add `"allowance-core"` to the `members` list in the workspace `Cargo.toml`.

```rust
// allowance-core/src/lib.rs
pub mod balance;
pub mod codec;
pub mod merge;
pub mod money;
pub mod row;
```

- [ ] **Step 4: Implement `Money`**

```rust
// allowance-core/src/money.rs
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::iter::Sum;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

/// Money in whole cents. Never a float: f64 addition is not associative, so two
/// machines applying the same rows in different orders would produce different
/// bytes and never converge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Money(i64);

impl Money {
    pub const fn from_cents(cents: i64) -> Self { Money(cents) }
    pub const fn cents(&self) -> i64 { self.0 }

    /// Canonical rendering: always exactly two decimals. Replaces
    /// `f64::to_string()`, whose output varies with the value and breaks
    /// byte-convergence.
    pub fn render(&self) -> String {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        format!("{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("not a valid money value: {0}")]
pub struct MoneyParseError(String);

impl FromStr for Money {
    type Err = MoneyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let (neg, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (whole, frac) = match digits.split_once('.') {
            Some((w, f)) => (w, f),
            None => (digits, ""),
        };
        if whole.is_empty() && frac.is_empty() {
            return Err(MoneyParseError(s.to_string()));
        }
        if frac.len() > 2 {
            return Err(MoneyParseError(s.to_string()));
        }
        if !whole.chars().all(|c| c.is_ascii_digit())
            || !frac.chars().all(|c| c.is_ascii_digit())
        {
            return Err(MoneyParseError(s.to_string()));
        }
        let whole: i64 = if whole.is_empty() { 0 } else {
            whole.parse().map_err(|_| MoneyParseError(s.to_string()))?
        };
        // "5.5" is 50 cents of fraction, not 5.
        let frac_cents: i64 = match frac.len() {
            0 => 0,
            1 => frac.parse::<i64>().map_err(|_| MoneyParseError(s.to_string()))? * 10,
            _ => frac.parse().map_err(|_| MoneyParseError(s.to_string()))?,
        };
        let total = whole * 100 + frac_cents;
        Ok(Money(if neg { -total } else { total }))
    }
}

impl Add for Money { type Output = Money; fn add(self, o: Money) -> Money { Money(self.0 + o.0) } }
impl Sub for Money { type Output = Money; fn sub(self, o: Money) -> Money { Money(self.0 - o.0) } }
impl Neg for Money { type Output = Money; fn neg(self) -> Money { Money(-self.0) } }
impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money { Money(iter.map(|m| m.0).sum()) }
}

/// Serializes as a plain number and deserializes from one. This is
/// load-bearing: the domain `Transaction` is serialized straight onto the AWS
/// wire and read by the MCP Lambda in another stack, so the shape cannot
/// change.
///
/// Deliberately `serialize_f64` rather than routing through
/// `serde_json::Number` — the latter is JSON-specific, and money also has to
/// survive the YAML serializers this codebase uses for `child.yaml` and
/// `allowance_config.yaml`. A format-specific impl would work in tests and
/// fail the first time a `Money` field reached YAML.
impl Serialize for Money {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.0 as f64 / 100.0)
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = f64::deserialize(d)?;
        // Round rather than truncate: 5.0 stored as 4.999999 must not become 4.99.
        Ok(Money((n * 100.0).round() as i64))
    }
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p allowance-core money`
Expected: PASS (6 tests).

- [ ] **Step 6: Add the no-I/O guard**

The spec claims this crate cannot reach the filesystem. Make that checkable rather than aspirational — Rust has no stable lint that bans `std::fs`, so assert it directly:

```rust
// allowance-core/tests/no_io.rs
//! The merge is only trustworthy if it is a total function over data.
//! This test is the enforcement the spec promises.

use std::path::Path;

#[test]
fn crate_source_performs_no_io() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    visit(&src, &mut offenders);
    assert!(offenders.is_empty(), "allowance-core must not do I/O: {offenders:?}");
}

fn visit(dir: &Path, offenders: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, offenders);
            continue;
        }
        if path.extension().map(|e| e != "rs").unwrap_or(true) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for needle in ["std::fs", "std::net", "File::open", "File::create", "Command::new"] {
                if line.contains(needle) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, needle));
                }
            }
        }
    }
}
```

- [ ] **Step 7: Run the guard**

Run: `cargo test -p allowance-core --test no_io`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add allowance-core Cargo.toml
git commit -m "feat(core): add allowance-core crate with exact Money(i64)"
```

---

### Task 3: Domain `Transaction` adopts `Money`

**Files:**
- Modify: `backend/domain/models/transaction.rs`
- Create: `egui-frontend/tests/wire_compat.rs`
- Modify: every call site the compiler names

**Interfaces:**
- Consumes: `allowance_core::money::Money` (Task 2).
- Produces: `Transaction { amount: Money, balance: Money, .. }` with an unchanged JSON shape.

- [ ] **Step 1: Write the failing wire-compatibility test**

This is the test that stops us breaking the MCP Lambda in a repo we cannot deploy.

```rust
// egui-frontend/tests/wire_compat.rs
//! The domain Transaction is serialized directly onto the AWS wire
//! (app_coordinator.rs:512-526) and read by the MCP Lambda in the
//! zephytop-brain stack. Its JSON shape is a cross-repo contract.

use allowance_tracker_egui::backend::domain::models::transaction::{Transaction, TransactionType};
use allowance_core::money::Money;

/// A payload in exactly the shape already stored in DynamoDB, written before
/// Money existed. It must still deserialize.
const STORED_PAYLOAD: &str = r#"{
  "id": "ex-1702516125000-af3c",
  "child_id": "keiko_hart",
  "date": "2026-08-01T12:05:00-04:00",
  "description": "Slime kit",
  "amount": -5.5,
  "balance": 27.25,
  "transaction_type": "Expense"
}"#;

#[test]
fn legacy_stored_payload_still_deserializes() {
    let tx: Transaction = serde_json::from_str(STORED_PAYLOAD).unwrap();
    assert_eq!(tx.amount, Money::from_cents(-550));
    assert_eq!(tx.balance, Money::from_cents(2725));
}

#[test]
fn reserialization_keeps_numbers_as_numbers() {
    let tx: Transaction = serde_json::from_str(STORED_PAYLOAD).unwrap();
    let value: serde_json::Value = serde_json::from_str(&serde_json::to_string(&tx).unwrap()).unwrap();
    assert!(value["amount"].is_number(), "amount must stay a JSON number");
    assert!(value["balance"].is_number(), "balance must stay a JSON number");
    assert_eq!(value["amount"].as_f64().unwrap(), -5.5);
    assert_eq!(value["balance"].as_f64().unwrap(), 27.25);
}

#[test]
fn field_names_are_unchanged() {
    let tx: Transaction = serde_json::from_str(STORED_PAYLOAD).unwrap();
    let value: serde_json::Value = serde_json::from_str(&serde_json::to_string(&tx).unwrap()).unwrap();
    for key in ["id", "child_id", "date", "description", "amount", "balance", "transaction_type"] {
        assert!(value.get(key).is_some(), "missing wire field: {key}");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui --test wire_compat`
Expected: FAIL — `amount` is still `f64`, so the `Money` comparison does not compile.

- [ ] **Step 3: Change the fields**

```rust
// backend/domain/models/transaction.rs
use allowance_core::money::Money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub child_id: String,
    pub date: DateTime<FixedOffset>,
    pub description: String,
    pub amount: Money,
    pub balance: Money,
    pub transaction_type: TransactionType,
}
```

Add `allowance-core = { path = "../allowance-core" }` to `egui-frontend/Cargo.toml` dependencies.

- [ ] **Step 4: Fix every call site the compiler names**

Run: `cargo check -p allowance_tracker_egui`

Work the error list top to bottom. The mechanical translations:
- `tx.amount > 0.0` → `tx.amount > Money::from_cents(0)`
- `a + b` where both are money → unchanged (operators are implemented)
- `format!("{:.2}", tx.amount)` → `tx.amount.render()`
- a literal `5.0` meant as money → `Money::from_cents(500)`
- `generate_id(amount, ..)` takes `f64` — change its parameter to `Money` and use `amount.cents() >= 0` for the in/ex prefix.

Do **not** introduce `as f64` to make an error disappear. If a site genuinely needs a float (chart plotting), convert at that boundary with `m.cents() as f64 / 100.0` and leave a comment saying it is display-only.

- [ ] **Step 5: Run the wire tests**

Run: `cargo test -p allowance_tracker_egui --test wire_compat`
Expected: PASS (3 tests).

- [ ] **Step 6: Run the whole suite**

Run: `cargo test --workspace`
Expected: PASS. Existing tests that construct transactions need their literals updated; that is expected churn, not a failure to work around.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(domain): Transaction money becomes Money(i64), wire format unchanged"
```

---

### Task 4: Pure balance arithmetic

**Files:**
- Create: `allowance-core/src/balance.rs`
- Modify: `backend/domain/balance_service.rs`

**Interfaces:**
- Consumes: `Money` (Task 2).
- Produces: **`allowance-core/src/row.rs`** — `TxRow`, `TxType`, `Provenance`, `Sided` — plus `recompute_running_balances(&mut [TxRow])` and `validate(&[TxRow]) -> Vec<BalanceMismatch>`.

> **This task owns `row.rs`.** `balance`, `codec` (Task 5), `merge` (Task 7) and `ChildSyncEngine` (Task 15) all consume those types and none of them redefine any part of it.

- [ ] **Step 1: Write the failing tests**

```rust
// allowance-core/src/balance.rs (bottom)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::row::{TxRow, TxType};
    use chrono::DateTime;

    fn row(id: &str, date: &str, cents: i64) -> TxRow {
        TxRow {
            id: id.to_string(),
            child_id: "c".to_string(),
            date: DateTime::parse_from_rfc3339(date).unwrap(),
            description: "d".to_string(),
            amount: Money::from_cents(cents),
            balance: Money::from_cents(0),
            tx_type: TxType::Expense,
        }
    }

    #[test]
    fn running_balance_accumulates_in_canonical_order() {
        let mut rows = vec![
            row("b", "2026-01-02T00:00:00Z", -200),
            row("a", "2026-01-01T00:00:00Z", 1000),
        ];
        recompute_running_balances(&mut rows);
        assert_eq!(rows[0].id, "a", "must be sorted before accumulating");
        assert_eq!(rows[0].balance, Money::from_cents(1000));
        assert_eq!(rows[1].balance, Money::from_cents(800));
    }

    #[test]
    fn same_timestamp_rows_are_ordered_by_id_not_input_order() {
        let mut one = vec![
            row("z", "2026-01-01T00:00:00Z", 100),
            row("a", "2026-01-01T00:00:00Z", 200),
        ];
        let mut two = vec![
            row("a", "2026-01-01T00:00:00Z", 200),
            row("z", "2026-01-01T00:00:00Z", 100),
        ];
        recompute_running_balances(&mut one);
        recompute_running_balances(&mut two);
        assert_eq!(one, two, "input order must not affect the result");
    }

    #[test]
    fn validate_is_empty_after_recompute_and_names_the_row_otherwise() {
        let mut rows = vec![row("a", "2026-01-01T00:00:00Z", 500)];
        recompute_running_balances(&mut rows);
        assert!(validate(&rows).is_empty());

        rows[0].balance = Money::from_cents(1);
        let errs = validate(&rows);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].id, "a");
        assert_eq!(errs[0].expected, Money::from_cents(500));
        assert_eq!(errs[0].found, Money::from_cents(1));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance-core balance`
Expected: FAIL — `row` module and functions do not exist.

- [ ] **Step 3: Define `TxRow` and implement**

```rust
// allowance-core/src/row.rs
use crate::money::Money;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxType { Allowance, OneOffIncome, Expense, FutureAllowance }

impl TxType {
    pub fn as_csv(&self) -> &'static str {
        match self {
            TxType::Allowance => "allowance",
            TxType::OneOffIncome => "income",
            TxType::Expense => "expense",
            TxType::FutureAllowance => "future_allowance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxRow {
    pub id: String,
    pub child_id: String,
    pub date: DateTime<FixedOffset>,
    pub description: String,
    pub amount: Money,
    /// Derived output. Never compared during a merge — see `intrinsic_eq`.
    pub balance: Money,
    pub tx_type: TxType,
}

impl TxRow {
    /// The canonical total order. `date` alone is not enough: the sort would be
    /// stable over *input* order, which differs per machine.
    pub fn sort_key(&self) -> (DateTime<FixedOffset>, &str) { (self.date, &self.id) }

    /// Equality over intrinsic fields only. `balance` is regenerated output, so
    /// including it would make one non-tail insert mark most of the file as
    /// "changed" and fire the conflict rule across rows that never really
    /// conflicted.
    pub fn intrinsic_eq(&self, other: &TxRow) -> bool {
        self.id == other.id
            && self.child_id == other.child_id
            && self.date == other.date
            && self.description == other.description
            && self.amount == other.amount
            && self.tx_type == other.tx_type
    }
}

/// A row paired with the provenance of the side it came from.
///
/// Provenance is resolved by the caller and passed in, so `merge` stays a total
/// function over data and never walks git history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    pub committer_epoch: i64,
    /// Full commit oid as hex. Ties break on this, so it must be stable and
    /// identical on both machines.
    pub commit_oid: [u8; 20],
}

impl Provenance {
    pub fn short_hex(&self) -> String {
        self.commit_oid.iter().take(4).map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Clone)]
pub struct Sided {
    pub rows: Vec<TxRow>,
    pub provenance: Provenance,
}
```

```rust
// allowance-core/src/balance.rs
use crate::money::Money;
use crate::row::TxRow;

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceMismatch {
    pub id: String,
    pub expected: Money,
    pub found: Money,
}

/// Sort into canonical order and rewrite every running balance.
///
/// Pure: no repository, no filesystem. The repository-bound
/// `recalculate_balances_from_date` calls this rather than reimplementing it.
pub fn recompute_running_balances(rows: &mut [TxRow]) {
    rows.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    let mut running = Money::from_cents(0);
    for row in rows.iter_mut() {
        running = running + row.amount;
        row.balance = running;
    }
}

/// Exact check — no epsilon. With `Money` there is nothing to tolerate.
pub fn validate(rows: &[TxRow]) -> Vec<BalanceMismatch> {
    let mut sorted: Vec<&TxRow> = rows.iter().collect();
    sorted.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    let mut running = Money::from_cents(0);
    let mut out = Vec::new();
    for row in sorted {
        running = running + row.amount;
        if row.balance != running {
            out.push(BalanceMismatch {
                id: row.id.clone(),
                expected: running,
                found: row.balance,
            });
        }
    }
    out
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance-core balance`
Expected: PASS (3 tests).

- [ ] **Step 5: Make `BalanceService` a wrapper**

In `backend/domain/balance_service.rs`, change `validate_all_balances` to return a real error type instead of `Ok` when balances are wrong:

```rust
/// Was `Result<Vec<String>>`, which returned Ok while reporting broken money —
/// a `?` at the call site swallowed it entirely.
pub fn validate_all_balances(&self, child_id: &str) -> Result<(), Vec<BalanceMismatch>> {
    let rows = self.load_rows(child_id).map_err(|_| Vec::new())?;
    let errors = allowance_core::balance::validate(&rows);
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

Rewrite `recalculate_balances_from_date` to load rows, call `allowance_core::balance::recompute_running_balances`, and write them back in one pass. Delete the per-row `find_child_id_for_transaction` loop — the child id is already on every row, and that loop parsed every child's entire CSV once per row.

- [ ] **Step 6: Run the workspace suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(balance): extract pure recompute/validate, drop quadratic rewrite path"
```

---

### Task 5: CSV codec with canonical order and no `Utc::now()` fallback

**Files:**
- Create: `allowance-core/src/codec.rs`
- Modify: `backend/storage/csv/transaction_repository.rs`

**Interfaces:**
- Consumes: `TxRow`, `Money`.
- Produces: `parse_transactions(&str) -> Result<Vec<TxRow>, CodecError>` and `render_transactions(&[TxRow]) -> String`.

- [ ] **Step 1: Write the failing tests**

```rust
// allowance-core/src/codec.rs (bottom)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;

    const CSV: &str = "id,child_id,date,description,amount,balance,type\n\
ex-2-b,keiko,2026-01-02T00:00:00+00:00,Slime,-5.50,4.50,expense\n\
in-1-a,keiko,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n";

    #[test]
    fn parse_then_render_is_canonically_ordered() {
        let rows = parse_transactions(CSV).unwrap();
        let out = render_transactions(&rows);
        let ids: Vec<&str> = out.lines().skip(1).map(|l| l.split(',').next().unwrap()).collect();
        assert_eq!(ids, vec!["in-1-a", "ex-2-b"], "output must be sorted by (date, id)");
    }

    #[test]
    fn render_is_byte_stable_across_a_round_trip() {
        let once = render_transactions(&parse_transactions(CSV).unwrap());
        let twice = render_transactions(&parse_transactions(&once).unwrap());
        assert_eq!(once, twice, "render(parse(x)) must be a fixed point");
    }

    #[test]
    fn money_renders_with_exactly_two_decimals() {
        let out = render_transactions(&parse_transactions(CSV).unwrap());
        assert!(out.contains(",10.00,10.00,"), "got: {out}");
        assert!(out.contains(",-5.50,4.50,"), "got: {out}");
    }

    #[test]
    fn an_unparseable_date_is_an_error_not_the_current_time() {
        let bad = "id,child_id,date,description,amount,balance,type\n\
x,keiko,not-a-date,d,1.00,1.00,expense\n";
        let err = parse_transactions(bad).unwrap_err();
        assert!(matches!(err, CodecError::Date { .. }), "got {err:?}");
    }

    #[test]
    fn a_date_only_value_is_an_error_not_a_local_midnight() {
        // Resolving through chrono::Local would parse differently in two
        // timezones, so the two machines would never converge.
        let bad = "id,child_id,date,description,amount,balance,type\n\
x,keiko,2026-01-01,d,1.00,1.00,expense\n";
        assert!(parse_transactions(bad).is_err());
    }

    #[test]
    fn an_unknown_type_is_refused_not_derived() {
        let future = "id,child_id,date,description,amount,balance,type\n\
x,keiko,2026-01-01T00:00:00+00:00,d,1.00,1.00,rebate\n";
        // Derivation would silently downgrade a row an older app does not know,
        // and then push it. Refuse instead.
        assert!(parse_transactions(future).is_err());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance-core codec`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement the codec**

```rust
// allowance-core/src/codec.rs
use crate::money::Money;
use crate::row::{TxRow, TxType};
use chrono::DateTime;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("csv error: {0}")]
    Csv(String),
    #[error("row {id}: unparseable date {value:?}")]
    Date { id: String, value: String },
    #[error("row {id}: unparseable money {value:?}")]
    Money { id: String, value: String },
    #[error("row {id}: unknown transaction type {value:?} — refusing rather than guessing")]
    Type { id: String, value: String },
}

pub const HEADER: [&str; 7] =
    ["id", "child_id", "date", "description", "amount", "balance", "type"];

pub fn parse_transactions(text: &str) -> Result<Vec<TxRow>, CodecError> {
    let mut reader = csv::Reader::from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let r = record.map_err(|e| CodecError::Csv(e.to_string()))?;
        let id = r.get(0).unwrap_or_default().to_string();
        let date_raw = r.get(2).unwrap_or_default();

        // RFC3339 only. No current-time fallback, and no chrono::Local path:
        // both make read-modify-write non-idempotent, so one bad row would make
        // the file change on every cycle and both machines re-merge forever.
        let date = DateTime::parse_from_rfc3339(date_raw)
            .map_err(|_| CodecError::Date { id: id.clone(), value: date_raw.to_string() })?;

        let parse_money = |idx: usize| -> Result<Money, CodecError> {
            let raw = r.get(idx).unwrap_or_default();
            raw.parse::<Money>()
                .map_err(|_| CodecError::Money { id: id.clone(), value: raw.to_string() })
        };

        let type_raw = r.get(6).unwrap_or_default();
        let tx_type = match type_raw.to_lowercase().as_str() {
            "allowance" => TxType::Allowance,
            "income" | "oneoffincome" => TxType::OneOffIncome,
            "expense" => TxType::Expense,
            "future_allowance" | "futureallowance" => TxType::FutureAllowance,
            _ => return Err(CodecError::Type { id, value: type_raw.to_string() }),
        };

        rows.push(TxRow {
            id: id.clone(),
            child_id: r.get(1).unwrap_or_default().to_string(),
            date,
            description: r.get(3).unwrap_or_default().to_string(),
            amount: parse_money(4)?,
            balance: parse_money(5)?,
            tx_type,
        });
    }
    rows.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    Ok(rows)
}

/// Always emits canonical order and canonical money. This is the only writer;
/// the repository calls it rather than keeping a second serializer that drifts.
pub fn render_transactions(rows: &[TxRow]) -> String {
    let mut sorted: Vec<&TxRow> = rows.iter().collect();
    sorted.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(HEADER).expect("in-memory write");
    for row in sorted {
        writer
            .write_record([
                row.id.as_str(),
                row.child_id.as_str(),
                &row.date.to_rfc3339(),
                row.description.as_str(),
                &row.amount.render(),
                &row.balance.render(),
                row.tx_type.as_csv(),
            ])
            .expect("in-memory write");
    }
    String::from_utf8(writer.into_inner().expect("in-memory flush")).expect("utf-8")
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance-core codec`
Expected: PASS (6 tests).

- [ ] **Step 5: Delegate from the repository**

In `transaction_repository.rs`, replace the hand-rolled reader and `write_transactions_internal` body with calls to `parse_transactions` / `render_transactions`, mapping `TxRow` to and from the domain `Transaction`. Delete `parse_date_string` and `parse_transaction_type` entirely — both are now failure modes rather than helpers.

An unparseable row now surfaces as an error instead of being silently rewritten. Report it through the existing `StartupNotice` mechanism so it is visible rather than fatal.

- [ ] **Step 6: Add the production-corpus round-trip test**

```rust
// backend/storage/csv/transaction_repository.rs (in the existing #[cfg(test)] mod)
#[test]
fn round_trips_a_legacy_shaped_csv_byte_for_byte() {
    // Guards against a codec change that quietly rewrites every row and makes
    // the first sync look like a thousand-row conflict.
    let text = std::fs::read_to_string("tests/fixtures/transactions_legacy_shapes.csv").unwrap();
    let once = allowance_core::codec::render_transactions(
        &allowance_core::codec::parse_transactions(&text).unwrap());
    let twice = allowance_core::codec::render_transactions(
        &allowance_core::codec::parse_transactions(&once).unwrap());
    assert_eq!(once, twice);
}
```

**The fixture is synthesized, not copied from real data.** This repo has a
GitHub remote, so committing a child's real `transactions.csv` would put their
financial history — dates, descriptions, amounts — into git history permanently
and push it to GitHub. The round-trip property holds over any input, so the
fixture only needs to reproduce the *shapes* real data contains, not the data.

Build `egui-frontend/tests/fixtures/transactions_legacy_shapes.csv` by hand to
cover every quirk the live file actually contains — these are what a naive codec
change silently rewrites:

- money that `f64::to_string()` rendered without decimals (`5`), with one (`5.5`),
  and with two (`27.25`), plus a negative of each
- a zero amount and a zero balance
- RFC3339 dates with a non-UTC offset (`-04:00`, `-05:00` — the file spans a DST
  boundary) and at least one with `+00:00`
- every legacy `type` value the parser accepts: `allowance`, `income`,
  `expense`, `future_allowance`
- a description containing a comma, so CSV quoting is exercised
- rows deliberately out of `(date, id)` order, so the canonical re-sort is proven

Additionally, as a **local, uncommitted** check, run the same round-trip against
the real file once and report the result:

```bash
cargo test -p allowance_tracker_egui --test codec_real_data -- --ignored
```

Write that as an `#[ignore]`d test reading a path from the `REAL_CSV` env var, so
the real data is exercised on the developer's machine and never enters the repo.

- [ ] **Step 7: Run the suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(codec): one CSV codec, canonical order, no current-time date fallback"
```

---

### Task 6: Real entropy in the transaction id suffix

The live generator is `DomainTransaction::generate_id` (`backend/domain/models/transaction.rs:29`) and it *already* appends four hex chars — Greg's critique cited `shared/src/lib.rs:579`, which is dead code. But the suffix is `SystemTime::now().as_nanos() % 16^4`: a second clock reading, not entropy, and two machines are correlated in exactly the way the suffix is supposed to break.

**Files:**
- Modify: `backend/domain/models/transaction.rs`
- Modify: `shared/src/lib.rs` (delete the dead generator)
- Modify: `egui-frontend/Cargo.toml`

**Interfaces:**
- Consumes: nothing.
- Produces: `Transaction::generate_id(amount: Money, timestamp_ms: u64) -> String`, format unchanged (`in-<ms>-<4 hex>`), so `parse_id` and every stored id keep working.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn suffixes_differ_within_one_clock_tick() {
    // The old implementation derived the suffix from the clock, so ids minted
    // inside one tick collided — the exact case two machines hit at once.
    let ids: std::collections::HashSet<String> = (0..1000)
        .map(|_| Transaction::generate_id(Money::from_cents(-500), 1_702_516_125_000))
        .collect();
    assert!(ids.len() > 990, "only {} distinct ids from 1000 draws", ids.len());
}

#[test]
fn format_is_unchanged_so_existing_ids_still_parse() {
    let id = Transaction::generate_id(Money::from_cents(-500), 1_702_516_125_000);
    let (kind, ts) = Transaction::parse_id(&id).unwrap();
    assert_eq!(kind, "ex");
    assert_eq!(ts, 1_702_516_125_000);
    assert_eq!(Transaction::parse_id("in-1625846400123-af3c").unwrap().0, "in");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui suffixes_differ`
Expected: FAIL — nanosecond-derived suffixes repeat heavily in a tight loop.

- [ ] **Step 3: Use a real RNG**

Add `rand = "0.8"` to `egui-frontend/Cargo.toml`.

```rust
// backend/domain/models/transaction.rs
pub fn generate_id(amount: Money, timestamp_ms: u64) -> String {
    let tx_type = if amount.cents() >= 0 { "in" } else { "ex" };
    format!("{}-{}-{:04x}", tx_type, timestamp_ms, rand::random::<u16>())
}
```

Delete `generate_random_suffix`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui suffixes_differ format_is_unchanged`
Expected: PASS.

- [ ] **Step 5: Delete the dead generator**

Remove `Transaction::generate_id` and its tests from `shared/src/lib.rs` (around `:579` and `:744-753`). It is referenced only by its own tests; leaving it invites a future reader to mistake it for the live path. Check `Goal::generate_id` (`backend/domain/models/goal.rs:43`) for the same clock-derived suffix and fix it identically if present.

- [ ] **Step 6: Run the suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "fix(ids): real entropy in transaction id suffix; delete dead generator"
```

---

### Task 7: The merge

**Files:**
- Create: `allowance-core/src/merge.rs`

**Interfaces:**
- Consumes: `TxRow`, `Sided`, `Provenance`, `recompute_running_balances`.
- Produces: `merge(base: Option<&[TxRow]>, ours: &Sided, theirs: &Sided) -> MergeOutcome`, where `MergeOutcome { rows: Vec<TxRow>, decisions: Vec<Decision> }`.

- [ ] **Step 1: Write the failing tests — one per row of the resolution table**

```rust
// allowance-core/src/merge.rs (bottom)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::row::{Provenance, Sided, TxRow, TxType};
    use chrono::DateTime;

    fn row(id: &str, desc: &str, cents: i64) -> TxRow {
        TxRow {
            id: id.to_string(),
            child_id: "c".to_string(),
            date: DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00").unwrap(),
            description: desc.to_string(),
            amount: Money::from_cents(cents),
            balance: Money::from_cents(0),
            tx_type: TxType::Expense,
        }
    }

    fn sided(rows: Vec<TxRow>, epoch: i64, oid_byte: u8) -> Sided {
        Sided { rows, provenance: Provenance { committer_epoch: epoch, commit_oid: [oid_byte; 20] } }
    }

    fn ids(o: &MergeOutcome) -> Vec<&str> { o.rows.iter().map(|r| r.id.as_str()).collect() }

    #[test]
    fn keeps_adds_from_both_sides() {
        let base = vec![];
        let ours = sided(vec![row("a", "x", -100)], 10, 1);
        let theirs = sided(vec![row("b", "y", -200)], 20, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(ids(&out), vec!["a", "b"]);
    }

    #[test]
    fn drops_a_row_deleted_on_one_side_and_untouched_on_the_other() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![], 10, 1);
        let theirs = sided(vec![row("a", "x", -100)], 20, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert!(ids(&out).is_empty(), "a genuine delete must not resurrect");
    }

    #[test]
    fn an_edit_beats_a_delete() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![], 10, 1);
        let theirs = sided(vec![row("a", "edited", -100)], 20, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(ids(&out), vec!["a"]);
        assert_eq!(out.rows[0].description, "edited");
    }

    #[test]
    fn edit_edit_resolves_to_the_later_committer_timestamp() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "ours", -100)], 10, 1);
        let theirs = sided(vec![row("a", "theirs", -100)], 99, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(out.rows[0].description, "theirs");
    }

    #[test]
    fn edit_edit_ties_break_on_commit_oid_not_on_side() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "ours", -100)], 50, 9);
        let theirs = sided(vec![row("a", "theirs", -100)], 50, 2);
        // Same epoch: the higher oid wins, so both machines agree regardless of
        // which side each is standing on.
        assert_eq!(merge(Some(&base), &ours, &theirs).rows[0].description, "ours");
        assert_eq!(merge(Some(&base), &theirs, &ours).rows[0].description, "ours");
    }

    #[test]
    fn add_add_with_identical_content_keeps_one_row() {
        let ours = sided(vec![row("a", "same", -100)], 10, 1);
        let theirs = sided(vec![row("a", "same", -100)], 20, 2);
        let out = merge(Some(&[]), &ours, &theirs);
        assert_eq!(out.rows.len(), 1);
    }

    #[test]
    fn add_add_with_differing_content_keeps_both_rows() {
        // THE data-loss case. Two Macs mint the same id for two different
        // transactions; picking one destroys real money.
        let ours = sided(vec![row("a", "slime kit", -100)], 10, 1);
        let theirs = sided(vec![row("a", "book fair", -250)], 20, 2);
        let out = merge(Some(&[]), &ours, &theirs);
        assert_eq!(out.rows.len(), 2, "both transactions must survive");
        let descs: Vec<&str> = out.rows.iter().map(|r| r.description.as_str()).collect();
        assert!(descs.contains(&"slime kit"));
        assert!(descs.contains(&"book fair"));
        let re_keyed: Vec<&str> = out.rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(re_keyed.iter().collect::<std::collections::HashSet<_>>().len(), 2);
    }

    #[test]
    fn add_add_re_keys_the_same_side_on_both_machines() {
        let ours = sided(vec![row("a", "slime kit", -100)], 10, 1);
        let theirs = sided(vec![row("a", "book fair", -250)], 20, 2);
        let one = merge(Some(&[]), &ours, &theirs);
        let two = merge(Some(&[]), &theirs, &ours);
        assert_eq!(ids(&one), ids(&two), "both machines must pick the same loser");
    }

    #[test]
    fn no_merge_base_unions_both_sides() {
        // Independent `git init` on each machine. With no common ancestor,
        // nothing can be shown to have been deleted, so nothing is dropped.
        let ours = sided(vec![row("a", "x", -100)], 10, 1);
        let theirs = sided(vec![row("b", "y", -200)], 20, 2);
        let out = merge(None, &ours, &theirs);
        assert_eq!(ids(&out), vec!["a", "b"]);
    }

    #[test]
    fn output_balances_are_recomputed_and_valid() {
        let ours = sided(vec![row("a", "x", 1000)], 10, 1);
        let theirs = sided(vec![row("b", "y", -400)], 20, 2);
        let out = merge(Some(&[]), &ours, &theirs);
        assert!(crate::balance::validate(&out.rows).is_empty());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance-core merge`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement**

```rust
// allowance-core/src/merge.rs
use crate::balance::recompute_running_balances;
use crate::row::{Provenance, Sided, TxRow};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    TookOurs { id: String },
    TookTheirs { id: String },
    KeptBothReKeyed { original: String, re_keyed: String },
    Deleted { id: String },
}

#[derive(Debug, Clone)]
pub struct MergeOutcome {
    pub rows: Vec<TxRow>,
    /// Every non-trivial choice, so a surprising result is auditable after the
    /// fact. The caller logs these.
    pub decisions: Vec<Decision>,
}

/// Three-way semantic merge.
///
/// Pure and total: provenance is supplied by the caller, never looked up, so
/// this never touches git. `base` is `None` when the histories are unrelated.
pub fn merge(base: Option<&[TxRow]>, ours: &Sided, theirs: &Sided) -> MergeOutcome {
    let index = |rows: &[TxRow]| -> BTreeMap<String, TxRow> {
        rows.iter().map(|r| (r.id.clone(), r.clone())).collect()
    };
    let base_map = base.map(index).unwrap_or_default();
    let base_known = base.is_some();
    let ours_map = index(&ours.rows);
    let theirs_map = index(&theirs.rows);

    let all: BTreeSet<&String> = ours_map.keys().chain(theirs_map.keys()).collect();
    let mut rows = Vec::new();
    let mut decisions = Vec::new();

    for id in all {
        let b = base_map.get(id);
        let o = ours_map.get(id);
        let t = theirs_map.get(id);

        match (o, t) {
            (Some(o), None) => {
                // Deleted on theirs. Only a real delete if the base had it AND
                // we did not change it — otherwise an edit beats the delete.
                let unchanged_by_us = b.map(|b| b.intrinsic_eq(o)).unwrap_or(false);
                if base_known && b.is_some() && unchanged_by_us {
                    decisions.push(Decision::Deleted { id: id.clone() });
                } else {
                    rows.push(o.clone());
                }
            }
            (None, Some(t)) => {
                let unchanged_by_them = b.map(|b| b.intrinsic_eq(t)).unwrap_or(false);
                if base_known && b.is_some() && unchanged_by_them {
                    decisions.push(Decision::Deleted { id: id.clone() });
                } else {
                    rows.push(t.clone());
                }
            }
            (Some(o), Some(t)) => {
                if o.intrinsic_eq(t) {
                    rows.push(o.clone());
                } else if b.is_none() {
                    // add/add: two DIFFERENT rows that collided on a key.
                    // Keeping one destroys a real transaction, so keep both and
                    // re-key deterministically.
                    //
                    // The suffix is derived from the ROW'S OWN CONTENT, never
                    // from provenance. A provenance-derived suffix is not a
                    // fixed point: re-merging the result against the same side
                    // sees the same collision on the original id and re-keys
                    // again with a fresh suffix, forever. Content-derived plus
                    // the dedupe below means the second merge produces exactly
                    // the first merge's rows.
                    let (keep, rekey) = if wins(&ours.provenance, &theirs.provenance) {
                        (o, t)
                    } else {
                        (t, o)
                    };
                    let mut moved = rekey.clone();
                    moved.id = format!("{}-{}", rekey.id, content_suffix(rekey));
                    decisions.push(Decision::KeptBothReKeyed {
                        original: rekey.id.clone(),
                        re_keyed: moved.id.clone(),
                    });
                    rows.push(keep.clone());
                    rows.push(moved);
                } else {
                    // edit/edit: one row, two edits. Pick one.
                    if wins(&ours.provenance, &theirs.provenance) {
                        decisions.push(Decision::TookOurs { id: id.clone() });
                        rows.push(o.clone());
                    } else {
                        decisions.push(Decision::TookTheirs { id: id.clone() });
                        rows.push(t.clone());
                    }
                }
            }
            (None, None) => unreachable!("id came from one of the two maps"),
        }
    }

    // A re-keyed row can equal a row the other side already carries (exactly
    // what makes the second merge a fixed point). Collapse those.
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows.dedup_by(|a, b| a.id == b.id);

    recompute_running_balances(&mut rows);
    MergeOutcome { rows, decisions }
}

/// A stable fingerprint of a row's intrinsic fields.
///
/// FNV-1a, written out explicitly. `DefaultHasher` would be wrong here: Rust
/// does not guarantee its output is stable across compiler versions, and this
/// value becomes part of a transaction id that both machines must agree on
/// while building from separate toolchains.
fn content_suffix(row: &TxRow) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(row.child_id.as_bytes());
    eat(b"\x1f");
    eat(row.date.to_rfc3339().as_bytes());
    eat(b"\x1f");
    eat(row.description.as_bytes());
    eat(b"\x1f");
    eat(row.amount.cents().to_string().as_bytes());
    eat(b"\x1f");
    eat(row.tx_type.as_csv().as_bytes());
    format!("{:08x}", hash as u32)
}

/// Later committer timestamp wins; ties break on commit oid.
///
/// Must be symmetric — a "prefer ours" rule would have each machine choose its
/// own side and the two would never converge.
fn wins(ours: &Provenance, theirs: &Provenance) -> bool {
    match ours.committer_epoch.cmp(&theirs.committer_epoch) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => ours.commit_oid > theirs.commit_oid,
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance-core merge`
Expected: PASS (10 tests).

- [ ] **Step 5: Commit**

```bash
git add allowance-core/src/merge.rs
git commit -m "feat(merge): pure three-way semantic merge with symmetric resolution"
```

---

### Task 8: The four properties

Symmetry alone is necessary and **not sufficient** — two machines can agree on values and still disagree on bytes forever. The fixed-point property is the one that proves the re-merge loop terminates.

**Files:**
- Create: `allowance-core/tests/properties.rs`

**Interfaces:**
- Consumes: `merge`, `codec`, `balance`.
- Produces: nothing — this task is the safety net for every task after it.

- [ ] **Step 1: Write the properties**

```rust
// allowance-core/tests/properties.rs
use allowance_core::balance::validate;
use allowance_core::codec::{parse_transactions, render_transactions};
use allowance_core::merge::merge;
use allowance_core::money::Money;
use allowance_core::row::{Provenance, Sided, TxRow, TxType};
use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;

fn row_strategy() -> impl Strategy<Value = TxRow> {
    (
        "[a-z]{1,6}",
        0i64..5,
        -100_000i64..100_000,
        "[a-z ]{0,12}",
    )
        .prop_map(|(id, day, cents, desc)| TxRow {
            id,
            child_id: "c".to_string(),
            date: DateTime::from(Utc.timestamp_opt(1_760_000_000 + day * 86_400, 0).unwrap()),
            description: desc,
            // Integer cents only: with f64 the property would fail for reasons
            // unrelated to the merge, and the repair would be to widen an epsilon.
            amount: Money::from_cents(cents),
            balance: Money::from_cents(0),
            tx_type: TxType::Expense,
        })
}

fn sided_strategy() -> impl Strategy<Value = Sided> {
    (prop::collection::vec(row_strategy(), 0..8), 0i64..1000, 0u8..255).prop_map(
        |(mut rows, epoch, oid)| {
            // Sort BEFORE dedup: `dedup_by` only removes *consecutive*
            // duplicates, so an unsorted vec keeps duplicate ids. The merge
            // indexes rows by id, so a duplicate would be silently dropped and
            // symmetry would appear to fail for a reason that is purely an
            // artifact of the generator.
            rows.sort_by(|a, b| a.id.cmp(&b.id));
            rows.dedup_by(|a, b| a.id == b.id);
            Sided { rows, provenance: Provenance { committer_epoch: epoch, commit_oid: [oid; 20] } }
        },
    )
}

proptest! {
    /// Symmetry — "prefer ours" would make each machine choose its own side and
    /// the two would diverge permanently.
    #[test]
    fn symmetric(base in prop::collection::vec(row_strategy(), 0..5),
                 a in sided_strategy(), b in sided_strategy()) {
        let ab = merge(Some(&base), &a, &b);
        let ba = merge(Some(&base), &b, &a);
        prop_assert_eq!(render_transactions(&ab.rows), render_transactions(&ba.rows));
    }

    /// Idempotence — merging a side with itself is just canonicalisation.
    #[test]
    fn idempotent(a in sided_strategy()) {
        let out = merge(Some(&a.rows), &a, &a);
        prop_assert_eq!(render_transactions(&out.rows), render_transactions(&{
            let mut rows = a.rows.clone();
            allowance_core::balance::recompute_running_balances(&mut rows);
            rows
        }));
    }

    /// FIXED POINT — re-merging a merged result against one of its inputs
    /// changes nothing. This is what proves the machines stop re-merging.
    #[test]
    fn fixed_point(base in prop::collection::vec(row_strategy(), 0..5),
                   a in sided_strategy(), b in sided_strategy()) {
        let first = merge(Some(&base), &a, &b);
        let merged_side = Sided { rows: first.rows.clone(), provenance: a.provenance };
        let second = merge(Some(&base), &merged_side, &b);
        prop_assert_eq!(render_transactions(&first.rows), render_transactions(&second.rows));
    }

    /// Byte round-trip — render(parse(x)) == x for canonical x.
    #[test]
    fn round_trip_is_byte_stable(rows in prop::collection::vec(row_strategy(), 0..10)) {
        let once = render_transactions(&rows);
        let twice = render_transactions(&parse_transactions(&once).unwrap().rows);
        prop_assert_eq!(once, twice);
    }

    /// Money is always self-consistent after a merge.
    #[test]
    fn balances_validate(base in prop::collection::vec(row_strategy(), 0..5),
                         a in sided_strategy(), b in sided_strategy()) {
        let out = merge(Some(&base), &a, &b);
        prop_assert!(validate(&out.rows).is_empty());
    }
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -p allowance-core --test properties`
Expected: PASS, 256 cases each. A failure prints a shrunk counterexample — fix `merge`, never the property.

- [ ] **Step 3: Add the timezone determinism test**

```rust
// allowance-core/tests/properties.rs (append)
#[test]
fn merge_output_does_not_depend_on_the_machine_timezone() {
    // Two Macs in different timezones must produce identical bytes. The codec
    // parses RFC3339 only and never resolves through chrono::Local, so this
    // holds — the test is here to keep it holding.
    let csv = "id,child_id,date,description,amount,balance,type\n\
a,c,2026-01-01T00:00:00+00:00,x,1.00,1.00,expense\n";
    std::env::set_var("TZ", "UTC");
    let utc = render_transactions(&parse_transactions(csv).unwrap().rows);
    std::env::set_var("TZ", "America/Los_Angeles");
    let la = render_transactions(&parse_transactions(csv).unwrap().rows);
    assert_eq!(utc, la);
}
```

- [ ] **Step 4: Run it**

Run: `cargo test -p allowance-core --test properties timezone`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add allowance-core/tests/properties.rs
git commit -m "test(core): symmetry, idempotence, fixed point, byte round-trip, balances"
```

---

## Phase 2 — lgs integration (requires Task 1's verdict)

### Task 9: `SyncPaths` and the cloud-path guard

The single most important safety rule in the spec. As a pure predicate over injected paths it is unit-testable; reading `dirs::home_dir()` internally would mean it gets verified once by hand and then silently regresses.

**Files:**
- Create: `backend/sync/mod.rs`, `backend/sync/paths.rs`
- Modify: `backend/mod.rs` (add `pub mod sync;`)

**Interfaces:**
- Consumes: nothing.
- Produces: `SyncPaths { data_dir, children_root, lgs_binary, cloud_root: Option<PathBuf>, home: PathBuf }` and `is_cloud_synced(candidate: &Path, env: &SyncPaths, documents_is_symlink: bool) -> Option<Reason>`. The `home` field is required — the guard compares against `home/Library/Mobile Documents` and `home/Documents`, and reading `dirs::home_dir()` internally is what would make it untestable.

- [ ] **Step 1: Write the failing table test**

```rust
// backend/sync/paths.rs (bottom)
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn env() -> SyncPaths {
        SyncPaths {
            data_dir: PathBuf::from("/Users/k/Documents/Allowance Tracker"),
            children_root: PathBuf::from("/Users/k/Library/Application Support/Allowance Tracker/children"),
            lgs_binary: PathBuf::from("/Users/k/Library/Application Support/Allowance Tracker/bin/lgs"),
            cloud_root: Some(PathBuf::from("/Users/k/Library/CloudStorage/ProtonDrive-x/Code")),
            home: PathBuf::from("/Users/k"),
        }
    }

    #[test]
    fn rejection_table() {
        let cases: Vec<(&str, bool, Option<Reason>)> = vec![
            ("/Users/k/Library/Application Support/Allowance Tracker/children/keiko", false, None),
            ("/Users/k/Library/CloudStorage/ProtonDrive-x/Code/inside", false, Some(Reason::InsideCloudRoot)),
            ("/Users/k/Library/Mobile Documents/com~apple~CloudDocs/x", false, Some(Reason::MobileDocuments)),
            // Documents is only a hazard when Desktop & Documents sync is really on.
            ("/Users/k/Documents/Allowance Tracker/keiko", true, Some(Reason::DocumentsSyncOn)),
            ("/Users/k/Documents/Allowance Tracker/keiko", false, None),
        ];
        for (path, docs_symlink, expected) in cases {
            let got = is_cloud_synced(std::path::Path::new(path), &env(), docs_symlink);
            assert_eq!(got, expected, "for {path} (symlink={docs_symlink})");
        }
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui rejection_table`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement**

```rust
// backend/sync/paths.rs
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SyncPaths {
    pub data_dir: PathBuf,
    pub children_root: PathBuf,
    pub lgs_binary: PathBuf,
    pub cloud_root: Option<PathBuf>,
    pub home: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    InsideCloudRoot,
    MobileDocuments,
    DocumentsSyncOn,
}

impl Reason {
    pub fn message(&self) -> &'static str {
        match self {
            Reason::InsideCloudRoot =>
                "that folder is inside the lgs cloud root — two systems replicating the same bytes is the failure this design exists to avoid",
            Reason::MobileDocuments =>
                "that folder is in iCloud Drive, which writes conflict copies inside .git and can corrupt the repository",
            Reason::DocumentsSyncOn =>
                "Desktop & Documents Folders syncing is on, so iCloud would replicate this repository's .git",
        }
    }
}

/// `documents_is_symlink` is the caller's observation of whether `~/Documents`
/// is a symlink — the reliable signal that Desktop & Documents sync is on. The
/// `FXICloudDriveDocuments` Finder pref is stale on real machines and must not
/// be used.
pub fn is_cloud_synced(
    candidate: &Path,
    env: &SyncPaths,
    documents_is_symlink: bool,
) -> Option<Reason> {
    if let Some(root) = &env.cloud_root {
        if candidate.starts_with(root) {
            return Some(Reason::InsideCloudRoot);
        }
    }
    if candidate.starts_with(env.home.join("Library/Mobile Documents")) {
        return Some(Reason::MobileDocuments);
    }
    if documents_is_symlink && candidate.starts_with(env.home.join("Documents")) {
        return Some(Reason::DocumentsSyncOn);
    }
    None
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui rejection_table`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/sync backend/mod.rs
git commit -m "feat(sync): SyncPaths and pure cloud-synced-path guard"
```

---

### Task 10: `LgsClient`

No trait — one implementation, one caller. The testable seam is the pure parser.

**Files:**
- Create: `backend/sync/lgs_client.rs`
- Create: `egui-frontend/tests/fixtures/lgs_status.json`

**Interfaces:**
- Consumes: `SyncPaths`.
- Produces: `parse_status(&str) -> Result<StatusReport>`, `LgsClient::run(&[&str]) -> Result<String>`, `LgsClient::status()`, `add()`, `restore()`, `projects()`.

- [ ] **Step 1: Capture a real fixture**

Run: `lgs status --json`

Save the output to `egui-frontend/tests/fixtures/lgs_status.json`. Record the lgs commit it came from in a comment at the top of `lgs_client.rs` — a hand-retyped fixture is a second representation of a contract and keeps passing after the real one changes.

- [ ] **Step 2: Write the failing tests**

```rust
// backend/sync/lgs_client.rs (bottom)
#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../egui-frontend/tests/fixtures/lgs_status.json");

    #[test]
    fn parses_the_real_fixture() {
        let report = parse_status(FIXTURE).unwrap();
        assert!(report.cloud_root.is_some());
        assert!(!report.projects.is_empty());
    }

    #[test]
    fn an_unknown_durability_state_does_not_fail_the_parse() {
        // lgs's own tests feed "a_variant_from_the_future". A String field would
        // model this loosely; a closed enum would reject the whole document.
        let json = r#"{"daemon_running":true,"cloud_root":"/tmp","cloud_root_exists":true,
          "port":8418,"projects":[{"name":"p","working_repo_path":"/tmp/p",
          "clone_url":"http://localhost:8418/p.git","archived":false,
          "durability":{"state":"a_variant_from_the_future","generation":1}}]}"#;
        let report = parse_status(json).unwrap();
        assert_eq!(report.projects[0].durability_state, DurabilityState::Unknown);
    }

    #[test]
    fn outdated_daemon_is_reported_not_swallowed() {
        let json = r#"{"daemon_running":true,"cloud_root":"/tmp","cloud_root_exists":true,
          "port":8418,"projects":[],
          "daemon":{"state":"outdated","message":"restart the service to pick up the new binary"}}"#;
        let report = parse_status(json).unwrap();
        assert_eq!(report.daemon.state, DaemonState::Outdated);
        assert!(report.daemon.message.contains("restart the service"));
        assert!(!report.durability_data_is_fresh(),
            "a skewed daemon reads durability from disk; we must not claim backed-up");
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui lgs_client`
Expected: FAIL — module does not exist.

- [ ] **Step 4: Implement**

```rust
// backend/sync/lgs_client.rs
//! Fixture captured from lgs at commit <fill in from Step 1>.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurabilityState {
    BackedUp,
    Pending,
    NotBackedUp,
    Diverged,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DaemonState {
    #[default]
    Ok,
    Down,
    Outdated,
    Error,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DaemonInfo {
    #[serde(default)]
    pub state: DaemonState,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ProjectReport {
    pub name: String,
    pub clone_url: String,
    pub working_repo_path: PathBuf,
    pub durability_state: DurabilityState,
    pub durability_label: Option<String>,
    pub failed_sync_attempts: usize,
    pub archived: bool,
}

#[derive(Debug, Clone)]
pub struct StatusReport {
    pub daemon: DaemonInfo,
    pub cloud_root: Option<PathBuf>,
    pub cloud_root_exists: bool,
    pub projects: Vec<ProjectReport>,
    pub adoptable: Vec<String>,
}

impl StatusReport {
    /// Durability is only trustworthy from a daemon we can actually talk to.
    /// Whether the durability numbers in this report are FRESH — i.e. the
    /// daemon answered rather than the values being read stale from disk.
    /// This is report-wide and is NOT a per-project safety answer: use
    /// `ProjectReport::is_confirmed_backed_up()` for that. Task 10's review
    /// found this returning true for a report containing a genuinely
    /// stranded project, which is exactly the misreading the old name
    /// (`can_claim_durability`) invited.
    pub fn durability_data_is_fresh(&self) -> bool {
        matches!(self.daemon.state, DaemonState::Ok)
    }
    pub fn project(&self, name: &str) -> Option<&ProjectReport> {
        self.projects.iter().find(|p| p.name == name)
    }
}

pub fn parse_status(json: &str) -> Result<StatusReport> {
    #[derive(Deserialize)]
    struct RawDurability { state: DurabilityState }
    #[derive(Deserialize)]
    struct RawProject {
        name: String,
        clone_url: String,
        working_repo_path: PathBuf,
        #[serde(default)] durability: Option<RawDurability>,
        #[serde(default)] durability_label: Option<String>,
        #[serde(default)] failed_sync_attempts: Option<usize>,
        #[serde(default)] archived: bool,
    }
    #[derive(Deserialize)]
    struct RawAdoptable { name: String }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)] daemon: DaemonInfo,
        #[serde(default)] cloud_root: Option<PathBuf>,
        #[serde(default)] cloud_root_exists: bool,
        #[serde(default)] projects: Vec<RawProject>,
        #[serde(default)] adoptable: Vec<RawAdoptable>,
    }

    let raw: Raw = serde_json::from_str(json).context("parsing `lgs status --json`")?;
    Ok(StatusReport {
        daemon: raw.daemon,
        cloud_root: raw.cloud_root,
        cloud_root_exists: raw.cloud_root_exists,
        projects: raw.projects.into_iter().map(|p| ProjectReport {
            name: p.name,
            clone_url: p.clone_url,
            working_repo_path: p.working_repo_path,
            durability_state: p.durability.map(|d| d.state).unwrap_or(DurabilityState::Unknown),
            durability_label: p.durability_label,
            failed_sync_attempts: p.failed_sync_attempts.unwrap_or(0),
            archived: p.archived,
        }).collect(),
        adoptable: raw.adoptable.into_iter().map(|a| a.name).collect(),
    })
}

pub struct LgsClient { binary: PathBuf }

impl LgsClient {
    pub fn new(binary: PathBuf) -> Self { Self { binary } }

    pub fn run(&self, args: &[&str]) -> Result<String> {
        let out = Command::new(&self.binary).args(args).output()
            .with_context(|| format!("running {:?} {:?}", self.binary, args))?;
        if !out.status.success() {
            bail!("lgs {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    pub fn status(&self) -> Result<StatusReport> {
        parse_status(&self.run(&["status", "--json"])?)
    }
    pub fn add(&self, path: &str, name: &str) -> Result<()> {
        self.run(&["add", path, "--name", name]).map(|_| ())
    }
    pub fn restore(&self, name: &str, path: &str) -> Result<String> {
        self.run(&["restore", name, path])
    }
    pub fn init(&self, cloud_root: &str) -> Result<()> {
        self.run(&["init", "--cloud-root", cloud_root]).map(|_| ())
    }
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui lgs_client`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add backend/sync/lgs_client.rs egui-frontend/tests/fixtures/lgs_status.json
git commit -m "feat(sync): LgsClient with pure status parser and future-proof enums"
```

---

### Task 11: Bundle the binary, copy it out, detect `git`

**Files:**
- Create: `egui-frontend/build.rs`
- Modify: `egui-frontend/Cargo.toml` (bundle resources)
- Create: `backend/sync/bootstrap.rs`

**Interfaces:**
- Consumes: `SyncPaths`, `LgsClient`.
- Produces: `ensure_lgs_binary(&SyncPaths) -> Result<PathBuf>`, `git_is_available() -> bool`.

- [ ] **Step 1: Write the failing test**

```rust
// backend/sync/bootstrap.rs (bottom)
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn copies_the_binary_out_of_the_bundle_and_is_idempotent() {
        // The plist points at whatever path we install from
        // (lgs service.rs uses current_exe()), so it must be a stable location
        // outside the .app — otherwise moving the app to /Applications leaves
        // launchd retrying a missing path forever under KeepAlive.
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"#!/bin/sh\nexit 0\n").unwrap();

        let dst = dst_dir.path().join("bin").join("lgs");
        copy_binary(&src, &dst).unwrap();
        assert!(dst.exists());

        copy_binary(&src, &dst).unwrap();
        assert!(dst.exists(), "re-running must not fail or corrupt the target");
    }

    #[test]
    fn copied_binary_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let src = src_dir.path().join("lgs");
        std::fs::write(&src, b"x").unwrap();
        let dst = dst_dir.path().join("lgs");
        copy_binary(&src, &dst).unwrap();
        let mode = std::fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "must be executable by owner/group/other");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui bootstrap`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
// backend/sync/bootstrap.rs
use crate::backend::sync::paths::SyncPaths;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// lgs shells out to `git` for every repository operation, and spawns
/// `git http-backend` to serve the remote. Bundling lgs does NOT remove this
/// dependency — on a Mac without Xcode Command Line Tools, /usr/bin/git is a
/// stub that opens a dialog and exits non-zero.
pub fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub const GIT_MISSING_MESSAGE: &str = "Sync needs Apple's Command Line Tools, which include git. \
Open Terminal and run: xcode-select --install";

pub fn copy_binary(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {parent:?}"))?;
    }
    // Write to a temp name then rename, so a crash mid-copy cannot leave a
    // truncated binary that launchd would happily keep executing.
    let tmp = dst.with_extension("tmp");
    std::fs::copy(src, &tmp).with_context(|| format!("copying {src:?} -> {tmp:?}"))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, dst).with_context(|| format!("renaming into {dst:?}"))?;
    Ok(())
}

/// Resolve the bundled binary and copy it to the stable path.
pub fn ensure_lgs_binary(env: &SyncPaths) -> Result<PathBuf> {
    let bundled = bundled_lgs_path()?;
    copy_binary(&bundled, &env.lgs_binary)?;
    Ok(env.lgs_binary.clone())
}

fn bundled_lgs_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolving current exe")?;
    // .app/Contents/MacOS/<exe> -> .app/Contents/Resources/lgs
    if let Some(macos_dir) = exe.parent() {
        let candidate = macos_dir.join("../Resources/lgs");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    // Dev builds: target/debug/lgs, produced by build.rs.
    let dev = exe.parent().map(|p| p.join("lgs")).unwrap_or_default();
    if dev.exists() {
        return Ok(dev);
    }
    anyhow::bail!("bundled lgs binary not found next to {exe:?}")
}
```

- [ ] **Step 4: Add the build wiring**

```rust
// egui-frontend/build.rs
//! Builds the pinned lgs and places the binary where bundling picks it up.
fn main() {
    println!("cargo:rerun-if-env-changed=LGS_BINARY");
    if let Ok(path) = std::env::var("LGS_BINARY") {
        let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap())
            .ancestors().nth(3).unwrap().join("lgs");
        let _ = std::fs::copy(&path, &out);
    }
}
```

Add to `egui-frontend/Cargo.toml` under `[package.metadata.bundle]`:

```toml
resources = ["assets/background.jpg", "../target/release/lgs"]
```

Document in the plan's README section that release builds set `LGS_BINARY` to a `lgs` built from the pinned commit.

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui bootstrap`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(sync): bundle lgs, copy to a stable path, detect git"
```

---

### Task 12: Daemon adopt-or-install with ownership

**Files:**
- Modify: `backend/sync/bootstrap.rs`
- Modify: `backend/domain/sync_persistence.rs` (record ownership)

**Interfaces:**
- Consumes: `LgsClient`, `SyncPaths`.
- Produces: `ensure_daemon(&LgsClient, &SyncPaths, &mut DaemonOwnership) -> Result<DaemonOutcome>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_adopted_daemon_is_never_upgraded() {
    // "Adopt, don't reinstall" alone strands the daemon: the bundled CLI
    // advances every release, `outdated` becomes permanent, and because we
    // refuse to claim backed-up while outdated holds, the app would never
    // report a project as backed up again. So we only upgrade what we own.
    let mut ownership = DaemonOwnership { installed_by_app: false };
    assert!(!should_upgrade(&ownership));
    ownership.installed_by_app = true;
    assert!(should_upgrade(&ownership));
}

#[test]
fn an_adopted_daemon_below_the_floor_is_reported_not_replaced() {
    let action = plan_daemon_action(DaemonState::Outdated, &DaemonOwnership { installed_by_app: false });
    assert_eq!(action, DaemonAction::ReportSkew);
}

#[test]
fn our_own_outdated_daemon_gets_restarted() {
    let action = plan_daemon_action(DaemonState::Outdated, &DaemonOwnership { installed_by_app: true });
    assert_eq!(action, DaemonAction::Restart);
}

#[test]
fn no_daemon_means_install_and_take_ownership() {
    let action = plan_daemon_action(DaemonState::Down, &DaemonOwnership { installed_by_app: false });
    assert_eq!(action, DaemonAction::InstallAndOwn);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui daemon`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
// backend/sync/bootstrap.rs (append)
use crate::backend::sync::lgs_client::{DaemonState, LgsClient};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DaemonOwnership { pub installed_by_app: bool }

#[derive(Debug, PartialEq, Eq)]
pub enum DaemonAction { None, Restart, ReportSkew, InstallAndOwn }

pub fn should_upgrade(o: &DaemonOwnership) -> bool { o.installed_by_app }

pub fn plan_daemon_action(state: DaemonState, owner: &DaemonOwnership) -> DaemonAction {
    match state {
        DaemonState::Ok => DaemonAction::None,
        DaemonState::Down => DaemonAction::InstallAndOwn,
        // Someone else's daemon is not ours to restart or overwrite.
        DaemonState::Outdated if owner.installed_by_app => DaemonAction::Restart,
        DaemonState::Outdated => DaemonAction::ReportSkew,
        DaemonState::Error | DaemonState::Unknown => DaemonAction::ReportSkew,
    }
}

/// `lgs install-service` writes the plist and then PRINTS the launchctl
/// commands for a human to run — nothing loads and nothing starts until the
/// next login. We run them ourselves, or first run hands a non-technical user a
/// plist, no daemon, no clone URL, and a terminal command as the remedy.
pub fn install_and_start(lgs: &LgsClient) -> Result<()> {
    lgs.run(&["install-service"])?;
    let uid = Command::new("id").arg("-u").output().context("running id -u")?;
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let plist = dirs::home_dir().unwrap_or_default()
        .join("Library/LaunchAgents/com.local-git-sync.daemon.plist");
    let _ = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()])
        .output();
    let st = Command::new("launchctl")
        .args(["kickstart", &format!("gui/{uid}/com.local-git-sync.daemon")])
        .output()
        .context("running launchctl kickstart")?;
    anyhow::ensure!(st.status.success(), "launchctl kickstart failed: {}",
        String::from_utf8_lossy(&st.stderr));
    Ok(())
}
```

Persist `DaemonOwnership` in `SyncState` (`sync_persistence.rs`) with `#[serde(default)]` so existing files keep loading.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui daemon`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(sync): adopt-or-install daemon, upgrade only what the app owns"
```

---

### Task 13: `GitManager` remote operations

**Files:**
- Modify: `backend/storage/git/mod.rs`

**Interfaces:**
- Consumes: Task 1's verdict (git2 vs shelling out).
- Produces: `clone_repo`, `fetch_lgs`, `push_lgs`, `merge_base`, `commit_merge`, `ensure_lgs_remote`, and `GitManager::with_clock`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn ensure_lgs_remote_is_idempotent_and_renames_origin() {
    // `lgs restore` clones and names the remote `origin`; migration names it
    // `lgs`. One name must exist in the system or the two paths disagree.
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "http://localhost:8418/p.git").unwrap();

    ensure_lgs_remote(&repo, "http://localhost:8418/p.git").unwrap();
    assert!(repo.find_remote("lgs").is_ok());

    ensure_lgs_remote(&repo, "http://localhost:8418/p.git").unwrap();
    assert!(repo.find_remote("lgs").is_ok(), "second call must not fail");
}

#[test]
fn ensure_lgs_remote_updates_a_stale_port() {
    // clone_url carries the daemon port, which is configurable. A URL frozen
    // into .git/config at migration time breaks push forever after a port change.
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("lgs", "http://localhost:8418/p.git").unwrap();
    ensure_lgs_remote(&repo, "http://localhost:9999/p.git").unwrap();
    assert_eq!(repo.find_remote("lgs").unwrap().url().unwrap(),
               "http://localhost:9999/p.git");
}

#[test]
fn commit_uses_the_injected_clock_so_ties_are_constructible() {
    // With Signature::now() hardcoded, no test can build a committer-timestamp
    // tie, so the tiebreak-by-oid branch of the resolution rule ships uncovered.
    let dir = tempfile::tempdir().unwrap();
    let gm = GitManager::with_clock(|| 1_700_000_000);
    gm.init_repo(dir.path()).unwrap();
    std::fs::write(dir.path().join("f.txt"), "x").unwrap();
    gm.add_all(dir.path()).unwrap();
    let oid = gm.commit(dir.path(), "m").unwrap();
    let repo = git2::Repository::open(dir.path()).unwrap();
    let commit = repo.find_commit(git2::Oid::from_str(&oid).unwrap()).unwrap();
    assert_eq!(commit.committer().when().seconds(), 1_700_000_000);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui git::`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
// backend/storage/git/mod.rs (additions)

/// Injectable so tests can construct committer-timestamp ties.
type Clock = fn() -> i64;

impl GitManager {
    pub fn with_clock(clock: Clock) -> Self {
        Self { author_name: "Allowance Tracker".into(),
               author_email: "noreply@localhost".into(), clock }
    }
    fn signature(&self) -> Result<Signature<'static>> {
        let when = git2::Time::new((self.clock)(), 0);
        Ok(Signature::new(&self.author_name, &self.author_email, &when)?)
    }
}

/// One remote name in the system, and never a stale URL.
pub fn ensure_lgs_remote(repo: &git2::Repository, url: &str) -> Result<()> {
    match repo.find_remote("lgs") {
        Ok(r) if r.url() == Some(url) => Ok(()),
        Ok(_) => Ok(repo.remote_set_url("lgs", url)?),
        Err(_) => {
            // `lgs restore` clones with the remote named `origin`.
            if let Ok(origin) = repo.find_remote("origin") {
                if origin.url() == Some(url) {
                    repo.remote_rename("origin", "lgs")?;
                    return Ok(());
                }
            }
            repo.remote("lgs", url)?;
            Ok(())
        }
    }
}

/// THE refspec. `reconcile` never moves a head over a divergence
/// (local-git-sync engine.rs:399), so a fetch of refs/heads/* returns this
/// machine's own tip and the merge would never fire. The authoritative peer tip
/// lives in refs/lgs-auth/heads/*.
pub const LGS_AUTH_REFSPEC: &str = "+refs/lgs-auth/heads/*:refs/remotes/lgs-auth/*";
pub const LGS_HEADS_REFSPEC: &str = "+refs/heads/*:refs/remotes/lgs/*";

pub fn fetch_lgs(repo: &git2::Repository) -> Result<()> {
    let mut remote = repo.find_remote("lgs")?;
    remote.fetch(&[LGS_AUTH_REFSPEC, LGS_HEADS_REFSPEC], None, None)?;
    Ok(())
}

pub fn push_lgs(repo: &git2::Repository, branch: &str) -> Result<()> {
    let mut remote = repo.find_remote("lgs")?;
    remote.push(&[format!("refs/heads/{branch}:refs/heads/{branch}").as_str()], None)?;
    Ok(())
}
```

If Task 1 concluded that git2 cannot push, implement `fetch_lgs`/`push_lgs`/`clone_repo` as `Command::new("git")` wrappers instead, keeping the same signatures so no later task changes.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui git::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/storage/git/mod.rs
git commit -m "feat(git): remote ops, lgs-auth refspec, one remote name, injectable clock"
```

---

## Phase 3 — Lifecycle

### Task 14: `TwoMachineHarness`

Two machines do not need two Macs. `lgs::paths` resolves everything from `$HOME`, and lgs's own `spawn_test_daemon` already supports two daemons over one `TempDir` cloud root.

**Files:**
- Create: `egui-frontend/tests/common/two_machine.rs`
- Create: `egui-frontend/tests/two_machine_sync.rs`

**Interfaces:**
- Consumes: `LgsClient`, `GitManager`.
- Produces: `TwoMachineHarness` with `machine_a()`, `machine_b()`, `cloud()`, `sync_both_ways()`.

- [ ] **Step 1: Write the harness**

```rust
// egui-frontend/tests/common/two_machine.rs
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Two lgs daemons under two HOMEs sharing one cloud root — the full
/// A -> bare -> cloud -> bare -> B loop, offline, in CI.
pub struct TwoMachineHarness {
    _cloud: TempDir,
    cloud_root: PathBuf,
    pub a: Machine,
    pub b: Machine,
    lgs_binary: PathBuf,
}

pub struct Machine { _home: TempDir, pub home: PathBuf, pub work: PathBuf }

impl TwoMachineHarness {
    pub fn new(lgs_binary: PathBuf) -> Self {
        let cloud = TempDir::new().unwrap();
        let cloud_root = cloud.path().to_path_buf();
        let a = Machine::new("a");
        let b = Machine::new("b");
        let h = Self { _cloud: cloud, cloud_root, a, b, lgs_binary };
        h.init(&h.a);
        h.init(&h.b);
        h
    }

    fn init(&self, m: &Machine) {
        self.lgs(m, &["init", "--cloud-root", &self.cloud_root.to_string_lossy()]);
    }

    pub fn lgs(&self, m: &Machine, args: &[&str]) -> String {
        let out = Command::new(&self.lgs_binary)
            .args(args)
            .env("HOME", &m.home)
            .output()
            .expect("running lgs");
        assert!(out.status.success(), "lgs {:?}: {}", args,
                String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// Publish A, ingest into B, publish B, ingest into A.
    pub fn sync_both_ways(&self, project: &str) {
        self.lgs(&self.a, &["sync", project]);
        self.lgs(&self.b, &["sync", project]);
        self.lgs(&self.a, &["sync", project]);
    }

    pub fn cloud(&self) -> &Path { &self.cloud_root }
}

impl Machine {
    fn new(tag: &str) -> Self {
        let home = TempDir::new().unwrap();
        let path = home.path().to_path_buf();
        let work = path.join(format!("work-{tag}"));
        std::fs::create_dir_all(&work).unwrap();
        Self { _home: home, home: path, work }
    }
}
```

- [ ] **Step 2: Write the divergence test the harness exists for**

```rust
// egui-frontend/tests/two_machine_sync.rs
mod common;
use common::two_machine::TwoMachineHarness;

fn harness() -> Option<TwoMachineHarness> {
    let bin = std::env::var("LGS_BINARY").ok()?;
    Some(TwoMachineHarness::new(bin.into()))
}

#[test]
fn concurrent_edits_are_visible_through_the_lgs_auth_mirror() {
    // The test that would have caught the refspec bug. A plain bare repo
    // neither accepts nor rejects the way lgs does, so this needs real daemons.
    let Some(h) = harness() else {
        eprintln!("set LGS_BINARY to run; skipping");
        return;
    };
    // ... register the project on both machines, commit divergently on each,
    // sync_both_ways, then assert refs/lgs-auth/heads/main on B resolves to
    // A's commit while refs/heads/main still points at B's own tip.
    let repo_b = git2::Repository::open(&h.b.work).unwrap();
    let auth = repo_b.find_reference("refs/remotes/lgs-auth/main");
    assert!(auth.is_ok(),
        "the peer tip must be reachable under refs/lgs-auth/*; \
         fetching refs/heads/* alone would return B's own commit");
}
```

- [ ] **Step 3: Run it**

Run: `LGS_BINARY=$(which lgs) cargo test -p allowance_tracker_egui --test two_machine_sync -- --nocapture`
Expected: PASS, or a clear skip when `LGS_BINARY` is unset.

- [ ] **Step 4: Commit**

```bash
git add egui-frontend/tests/common egui-frontend/tests/two_machine_sync.rs
git commit -m "test(sync): two-machine harness over one temp cloud root"
```

---

### Task 15: `ChildSyncEngine`

**Files:**
- Create: `backend/sync/child_sync.rs`
- Modify: `backend/domain/sync_manager.rs` (add `ApplyMerge`)
- Modify: `egui-frontend/src/ui/app_coordinator.rs` (handle it)

**Interfaces:**
- Consumes: `GitManager` remote ops, `allowance_core::merge`.
- Produces: `SyncMessage::ApplyMerge { child_id, rows: Vec<TxRow>, parents: (String, String), decisions: Vec<Decision> }`; `ChildSyncEngine::cycle(&ChildId) -> Result<CycleOutcome>`; and the pure helper the test in Step 2 targets — `classify(ours: Option<&str>, auth: Option<&str>, base: Option<&str>) -> Cycle` with `enum Cycle { UpToDate, FastForward, Diverged }`. `cycle()` calls `classify` after the fetch; keeping the decision separable is what makes it testable without a repository.

- [ ] **Step 1: Add the message**

```rust
// backend/domain/sync_manager.rs — in enum SyncMessage
/// A merge computed off-thread. The UI thread owns every byte in the working
/// tree (sync_manager.rs:37-40), so the background thread does fetch/push only
/// and hands the result over here.
ApplyMerge {
    child_id: String,
    rows: Vec<allowance_core::row::TxRow>,
    parents: (String, String),
    decisions: Vec<allowance_core::merge::Decision>,
},
```

Add a matching `Debug` arm — the enum's manual `Debug` impl will not compile without one.

- [ ] **Step 2: Write the failing test**

```rust
#[test]
fn classifies_up_to_date_fast_forward_and_diverged() {
    assert_eq!(classify(Some("x"), Some("x"), Some("x")), Cycle::UpToDate);
    // ours is an ancestor of the auth tip -> fast-forward.
    assert_eq!(classify(Some("a"), Some("b"), Some("a")), Cycle::FastForward);
    // neither is an ancestor of the other -> merge.
    assert_eq!(classify(Some("a"), Some("b"), Some("base")), Cycle::Diverged);
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui child_sync`
Expected: FAIL.

- [ ] **Step 4: Implement the cycle**

```rust
// backend/sync/child_sync.rs
/// One sync cycle for one child. Runs on the background thread; touches .git
/// only. Never writes the working tree — that is ApplyMerge's job on the UI
/// thread.
pub fn cycle(&self, child_id: &ChildId) -> Result<CycleOutcome> {
    let repo = git2::Repository::open(self.work_dir(child_id)?)?;
    ensure_lgs_remote(&repo, &self.clone_url(child_id)?)?;
    fetch_lgs(&repo)?;

    let ours_oid = repo.head()?.peel_to_commit()?.id();
    let auth = match repo.find_reference("refs/remotes/lgs-auth/main") {
        Ok(r) => r.peel_to_commit()?.id(),
        Err(_) => return Ok(CycleOutcome::UpToDate),
    };
    if ours_oid == auth {
        return Ok(CycleOutcome::UpToDate);
    }
    if repo.graph_descendant_of(auth, ours_oid)? {
        return Ok(CycleOutcome::FastForward { to: auth.to_string() });
    }

    // Diverged. Read all three sides as blobs and resolve provenance here, so
    // the merge itself never walks history.
    let base_oid = repo.merge_base(ours_oid, auth).ok();
    let base = base_oid.map(|o| self.read_rows(&repo, o)).transpose()?;
    let ours = Sided {
        rows: self.read_rows(&repo, ours_oid)?,
        provenance: self.provenance(&repo, ours_oid)?,
    };
    let theirs = Sided {
        rows: self.read_rows(&repo, auth)?,
        provenance: self.provenance(&repo, auth)?,
    };

    let outcome = merge(base.as_deref(), &ours, &theirs);
    Ok(CycleOutcome::Merged {
        rows: outcome.rows,
        parents: (ours_oid.to_string(), auth.to_string()),
        decisions: outcome.decisions,
    })
}

fn provenance(&self, repo: &git2::Repository, oid: git2::Oid) -> Result<Provenance> {
    let commit = repo.find_commit(oid)?;
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(oid.as_bytes());
    Ok(Provenance { committer_epoch: commit.committer().when().seconds(), commit_oid: bytes })
}
```

- [ ] **Step 5: Handle `ApplyMerge` on the UI thread**

In `app_coordinator.rs`'s message drain, add an arm that renders the rows through `render_transactions`, writes the file, creates a **single two-parent merge commit** (a separate recompute commit would produce a tree that never satisfies the fixed-point property), pushes, and logs every `Decision` with the child id and both oids.

- [ ] **Step 6: Run the suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(sync): ChildSyncEngine fetches lgs-auth, merges off-thread, applies on UI thread"
```

---

### Task 16: AWS coexistence

**Files:**
- Modify: `egui-frontend/src/ui/app_coordinator.rs` (`ApplyRemoteEntity`)
- Modify: `backend/domain/balance_service.rs`

**Interfaces:**
- Consumes: `BalanceService::with_sync_notifier`.
- Produces: no new API — behavioural change only.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn applying_a_remote_entity_does_not_create_a_git_commit() {
    // One MCP write otherwise produces an independent commit on EACH machine
    // for the same logical change, making divergence the steady state whenever
    // the MCP server is active.
    let helper = TestHelper::new().unwrap();
    let child = helper.create_test_child().unwrap();
    let before = head_oid(&helper.child_dir(&child));
    helper.apply_remote_entity_for_test(&child, sample_transaction_json());
    assert_eq!(before, head_oid(&helper.child_dir(&child)),
        "the merge produces the commit, not the apply path");
}

#[test]
fn merge_driven_recalculation_emits_no_sync_events() {
    // recalculate emitted an Updated event per changed row: a 40-row merge
    // became 40 channel round-trips plus 40 HTTP PUTs, on both machines.
    let (service, rx) = balance_service_with_notifier_disabled();
    service.recalculate_for_merge(&child_id, &rows).unwrap();
    assert!(rx.try_recv().is_err(), "merge recalculation must not notify AWS");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui coexist`
Expected: FAIL.

- [ ] **Step 3: Implement**

Route `ApplyRemoteEntity` through the repositories' `write_transactions_internal` (no git commit) instead of `write_transactions`. Build the merge path's `BalanceService` with `.with_sync_notifier(None)`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui coexist`
Expected: PASS.

- [ ] **Step 5: Document the resurrection hole**

Add a comment above the `ApplyRemoteEntity` arm recording the known gap: a row deleted on A and correctly dropped by the merge on B can be re-created by an AWS replay and then read as a fresh add on the next merge. The real fix belongs to the deferred AWS spec; this comment exists so the next reader finds it deliberately documented rather than accidentally missing.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "fix(sync): suppress commits on the AWS apply path and events during merge recompute"
```

---

### Task 17: Crash recovery and bounded push retry

**Files:**
- Modify: `backend/sync/child_sync.rs`

**Interfaces:**
- Consumes: `GitManager`.
- Produces: `recover_if_dirty(&Repository) -> Result<Recovered>`; `push_with_retry(&Repository, &str, max: u8)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_dirty_tree_on_a_diverged_branch_is_discarded_and_re_merged() {
    // Crash between writing merged CSVs and the merge commit. Re-running is
    // safe precisely because the merge is deterministic.
    let (repo, _dir) = repo_with_commit();
    std::fs::write(repo.workdir().unwrap().join("transactions.csv"), "garbage").unwrap();
    let recovered = recover_if_dirty(&repo).unwrap();
    assert_eq!(recovered, Recovered::DiscardedAndReMerged);
    let text = std::fs::read_to_string(repo.workdir().unwrap().join("transactions.csv")).unwrap();
    assert_ne!(text, "garbage");
}

#[test]
fn push_retry_is_bounded() {
    let attempts = std::cell::Cell::new(0);
    let result = push_with_retry_inner(3, || { attempts.set(attempts.get() + 1); Err(anyhow!("moved")) });
    assert!(result.is_err());
    assert_eq!(attempts.get(), 3, "must not spin forever when the remote keeps moving");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui recover push_retry`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
pub fn recover_if_dirty(repo: &git2::Repository) -> Result<Recovered> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false);
    if repo.statuses(Some(&mut opts))?.is_empty() {
        return Ok(Recovered::Clean);
    }
    let head = repo.head()?.peel_to_commit()?;
    repo.reset(head.as_object(), git2::ResetType::Hard, None)?;
    Ok(Recovered::DiscardedAndReMerged)
}

pub fn push_with_retry_inner<F>(max: u8, mut attempt: F) -> Result<()>
where F: FnMut() -> Result<()> {
    let mut last = None;
    for _ in 0..max {
        match attempt() {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    // Leaving it for the next scheduled pull is correct: the commit is durable,
    // so nothing is lost by stopping here.
    Err(last.unwrap_or_else(|| anyhow!("push failed")))
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui recover push_retry`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/sync/child_sync.rs
git commit -m "feat(sync): recover a dirty tree after a mid-merge crash; bound push retries"
```

---

## Phase 4 — User-facing

### Task 18: Migration

**Files:**
- Create: `backend/sync/migration_lgs.rs`

**Interfaces:**
- Consumes: `LgsClient`, `GitManager`, `is_cloud_synced`.
- Produces: `plan_lgs_migration(&[ChildEntry], &SyncPaths, &StatusReport) -> LgsMigrationPlan` and `run_lgs_migration(plan) -> LgsMigrationReport`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn adopts_an_existing_project_instead_of_creating_an_unrelated_root() {
    // A migrates Monday, B on Friday. Without this check B does its own
    // `git init`, the histories are unrelated, and any row A deleted in that
    // window resurrects through the empty-base union.
    let status = status_with_adoptable(&["allowance-keiko_hart"]);
    let plan = plan_lgs_migration(&[child("keiko_hart")], &paths(), &status);
    assert_eq!(plan.steps[0], Step::RestoreExisting { name: "allowance-keiko_hart".into() });
}

#[test]
fn creates_a_fresh_repo_when_nothing_is_adoptable() {
    let plan = plan_lgs_migration(&[child("keiko_hart")], &paths(), &status_with_adoptable(&[]));
    assert_eq!(plan.steps[0], Step::InitAndPush { name: "allowance-keiko_hart".into() });
}

#[test]
fn registry_is_repointed_only_after_every_other_step_succeeds() {
    let plan = plan_lgs_migration(&[child("keiko_hart")], &paths(), &status_with_adoptable(&[]));
    assert_eq!(*plan.steps.last().unwrap(), Step::RepointRegistry);
}

#[test]
fn a_failure_leaves_the_registry_and_the_old_folder_untouched() {
    let env = TestEnvironment::new().unwrap();
    let before = std::fs::read_to_string(env.registry_path()).unwrap();
    let report = run_lgs_migration(plan_that_fails_at_push(&env));
    assert!(report.failed());
    assert_eq!(std::fs::read_to_string(env.registry_path()).unwrap(), before);
    assert!(env.old_child_dir().exists(), "the old folder is never deleted");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui migration_lgs`
Expected: FAIL.

- [ ] **Step 3: Implement**

Follow the existing `plan_migration` / `MigrationReport` / `SkippedFolder` shape in `backend/storage/csv/migration.rs` — same plan-report-run discipline, not a second one. Steps, in order: adopt-or-init, copy data files only (never `.git`), canonicalize through `render_transactions`, commit, `lgs add`, `ensure_lgs_remote`, push, **repoint registry last**. Reject any target where `is_cloud_synced` returns `Some`. Raise a `StartupNotice` naming the old folder's path.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui migration_lgs`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/sync/migration_lgs.rs
git commit -m "feat(migration): adopt-or-init, copy-then-repoint, registry written last"
```

---

### Task 19: Onboarding a second machine

**Files:**
- Modify: `egui-frontend/src/ui/components/` (a new settings panel section)
- Modify: `backend/sync/migration_lgs.rs`

**Interfaces:**
- Consumes: `LgsClient::status`, `restore`, `ensure_lgs_remote`.
- Produces: `adoptable_children(&StatusReport) -> Vec<AdoptableChild>` and `adopt_child(&LgsClient, &str, &SyncPaths) -> Result<()>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn lists_only_allowance_projects() {
    let status = status_with_adoptable(&["allowance-keiko_hart", "weathertop-data"]);
    let found = adoptable_children(&status);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].child_id, "keiko_hart");
}

#[test]
fn an_archived_project_is_labelled_rather_than_hidden() {
    let status = status_with_archived_adoptable("allowance-keiko_hart", "finished with this child");
    let found = adoptable_children(&status);
    assert!(found[0].archived);
    assert_eq!(found[0].note.as_deref(), Some("finished with this child"));
}

#[test]
fn a_refusal_because_it_is_already_registered_falls_through_to_pull() {
    // `lgs restore` refuses a project already present here — by design, not an
    // error. This is the "both machines set up independently" case.
    let outcome = interpret_restore_result(Err(anyhow!("project 'allowance-keiko_hart' already exists")));
    assert_eq!(outcome, RestoreOutcome::AlreadyPresentUsePull);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui onboard`
Expected: FAIL.

- [ ] **Step 3: Implement**

`lgs restore` **already clones** (`ensure_working_copy`, `cli.rs:700-733`) and refuses a non-empty non-repo directory, so do **not** clone again. After `restore`, open with git2, call `ensure_lgs_remote` to rename `origin` to `lgs`, then write the `children.yaml` entry.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui onboard`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(onboarding): adoptable-children checklist, no double clone"
```

---

### Task 20: "Check sync"

Health *reporting* without a way to *exercise* the loop is half an answer, and the no-terminal goal means the user needs a button rather than a command.

**Files:**
- Modify: `backend/sync/child_sync.rs`
- Modify: the settings UI

**Interfaces:**
- Consumes: `GitManager`, `LgsClient`.
- Produces: `check_sync(&ChildId) -> Vec<StageResult>` where `StageResult { stage: Stage, ok: bool, detail: String }`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn every_stage_is_named_so_a_failure_says_which_one_broke() {
    let stages = check_sync_stages();
    assert_eq!(stages, vec![
        Stage::DaemonReachable, Stage::RemoteResolved, Stage::WriteSentinel,
        Stage::Commit, Stage::Push, Stage::Fetch, Stage::ReadBack, Stage::Cleanup,
    ]);
}

#[test]
fn a_failure_stops_at_the_failing_stage_and_reports_it() {
    let results = run_check_with(|stage| stage != Stage::Push);
    let failed: Vec<&StageResult> = results.iter().filter(|r| !r.ok).collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].stage, Stage::Push);
    assert!(results.iter().all(|r| r.stage <= Stage::Push), "must not continue past a failure");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p allowance_tracker_egui check_sync`
Expected: FAIL.

- [ ] **Step 3: Implement**

Write a sentinel to `.sync-check` in the child repo, commit, push, fetch, read back, then remove it and commit the removal. Report each stage by name. Stop at the first failure.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p allowance_tracker_egui check_sync`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ui): Check sync exercises the whole loop and names the failing stage"
```

---

### Task 21: Manual acceptance checklist

Only what genuinely needs hardware. Everything else moved into CI in Task 14.

**Files:**
- Create: `docs/lgs-sync-acceptance-checklist.md`

**Interfaces:**
- Consumes: nothing.
- Produces: the document.

- [ ] **Step 1: Write it**

Mirror the structure of `local-git-sync/docs/bootstrap-install-acceptance-checklist.md`, including its append-only run history. Cover only:

1. **Proton File Provider materialization** — reads against genuinely un-materialized bundles: do they block, return partial data, or need an explicit call?
2. **launchd at login** — reboot, confirm the daemon runs without anyone touching it; kill it and confirm respawn (allow ~10s for throttling).
3. **Real clock skew** — set the two Macs a minute apart, make conflicting edits, confirm the later committer timestamp wins on both and the two converge.
4. **The app-moved case** — move the `.app` to `/Applications` after first run and confirm the daemon still starts, since the plist points at the copied-out binary rather than into the bundle.

State plainly that our cold-start path inherits an unproven assumption: lgs's own checklist still reads "Run 1 — not yet performed", and its item 1 is described there as the weakest assumption its whole design leans on.

- [ ] **Step 2: Commit**

```bash
git add docs/lgs-sync-acceptance-checklist.md
git commit -m "docs: manual acceptance checklist for what CI cannot cover"
```

---

## Self-Review

**Spec coverage.** Every spec section maps to a task: goals/two-machines → Global Constraints + Task 8; blocking spike → Task 1; architecture and component boundaries → Tasks 2–7, 9–10, 15; repo layout and the guard → Task 9; lgs integration (bundle, git prerequisite, daemon ownership, health, remote URL, naming) → Tasks 10–13; Money → Tasks 2–3; canonical form → Tasks 4–5; sync lifecycle and thread ownership → Tasks 15, 17; the merge → Tasks 6–7; two transports → Task 16; migration → Task 18; onboarding → Task 19; Check sync → Task 20; testing → Tasks 8, 14, 21.

**Known gap, stated rather than hidden:** the spec's `Availability`/`Downloading` narrowing has no task of its own. It is a deletion that only becomes safe once no child resolves to a cloud-drive path, so it belongs after Task 18 lands on both machines — fold it into Task 18's follow-up rather than doing it blind.

**Type consistency.** `TxRow`, `Sided`, `Provenance`, `Money`, `Decision`, `MergeOutcome`, `StatusReport`, `DaemonState`, `SyncPaths`, `Reason` are each defined once and used with the same names and signatures throughout. `Sided` is defined in Task 4's `row.rs` and consumed unchanged in Tasks 7, 8, 15. `merge` keeps the signature `(Option<&[TxRow]>, &Sided, &Sided) -> MergeOutcome` in every task that mentions it.

**Ordering risk.** Task 3 (`Money` on `Transaction`) touches many call sites and will produce the largest diff. It is deliberately placed before the codec and merge work so that everything downstream is written against the final types rather than migrated twice.

---

**Plan complete and saved to `docs/superpowers/plans/2026-09-07-lgs-desktop-sync.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
