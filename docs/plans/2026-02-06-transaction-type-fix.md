# Transaction Type Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `Allowance` transaction type with backward compatibility to fix duplicate allowance creation bug

**Architecture:** Extend `TransactionType` enum with `Allowance` variant, store type in CSV, derive from description for legacy data, use type for duplicate detection

**Tech Stack:** Rust, CSV storage, serde

---

## Bug Root Cause

1. `use_age_based_amount: true` with `amount: 0.0` in config
2. `get_pending_allowance_dates()` doesn't calculate age-based amount
3. Transactions created with $0.00
4. `has_allowance_for_date()` checks `amount > 0.0` - fails for $0.00
5. Infinite loop creating duplicates every 2 minutes

## Solution Overview

| Change | Purpose |
|--------|---------|
| Add `Allowance` type | Distinguish auto-issued from manual income |
| Store type in CSV | Persist the distinction |
| Backward compat read | Derive type from description for old data |
| Extract amount calc | DRY - single function for age-based amounts |
| Fix duplicate check | Use `transaction_type == Allowance` |

---

## Task 1: Update TransactionType Enum (Domain)

**Files:**
- Modify: `backend/domain/models/transaction.rs:6-11`

**Step 1: Update the enum definition**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    Allowance,      // NEW: Automatically-issued allowances
    OneOffIncome,   // RENAMED: Manually-added positive amounts (was Income)
    Expense,
    FutureAllowance,
}
```

**Step 2: Run `cargo build` to find all breakages**

Run: `cargo build 2>&1 | grep -E "error|TransactionType"`
Expected: Multiple errors showing places that reference `TransactionType::Income`

**Step 3: Commit**

```bash
git add backend/domain/models/transaction.rs
git commit -m "refactor: rename Income to OneOffIncome, add Allowance type"
```

---

## Task 2: Update TransactionType Enum (Shared)

**Files:**
- Modify: `shared/src/lib.rs` (find TransactionType enum)

**Step 1: Find and update the shared enum**

Search for `TransactionType` in shared/src/lib.rs and update to match:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    Allowance,
    OneOffIncome,
    Expense,
    FutureAllowance,
}
```

**Step 2: Run `cargo build` again**

Run: `cargo build 2>&1`
Expected: Still errors, but fewer (shared crate compiles)

**Step 3: Commit**

```bash
git add shared/src/lib.rs
git commit -m "refactor: update shared TransactionType to match domain"
```

---

## Task 3: Update Mappers

**Files:**
- Modify: `backend/domain/mappers.rs:18-20`

**Step 1: Update the mapper**

```rust
impl From<DomainTransactionType> for TransactionType {
    fn from(domain: DomainTransactionType) -> Self {
        match domain {
            DomainTransactionType::Allowance => TransactionType::Allowance,
            DomainTransactionType::OneOffIncome => TransactionType::OneOffIncome,
            DomainTransactionType::Expense => TransactionType::Expense,
            DomainTransactionType::FutureAllowance => TransactionType::FutureAllowance,
        }
    }
}
```

**Step 2: Build to check progress**

Run: `cargo build 2>&1 | head -50`

**Step 3: Commit**

```bash
git add backend/domain/mappers.rs
git commit -m "refactor: update mappers for new transaction types"
```

---

## Task 4: Fix All Income → OneOffIncome References

**Files:**
- Multiple files referencing `TransactionType::Income` or `DomainTransactionType::Income`

**Step 1: Find all references**

Run: `grep -rn "TransactionType::Income" backend/ egui-frontend/`

**Step 2: Update each reference**

Replace `TransactionType::Income` → `TransactionType::OneOffIncome`
Replace `DomainTransactionType::Income` → `DomainTransactionType::OneOffIncome`

Key files likely affected:
- `backend/domain/transaction_service.rs:136-137`
- `backend/domain/calendar.rs` (multiple)
- `backend/domain/allowance_service.rs` (multiple)
- `backend/storage/csv/transaction_repository.rs:62-63`
- `backend/domain/balance_service.rs`
- `backend/domain/transaction_table.rs`

**Step 3: Build to verify**

Run: `cargo build 2>&1`
Expected: Compiles successfully (or close to it)

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: update all Income references to OneOffIncome"
```

---

## Task 5: Update CSV Schema - Write

**Files:**
- Modify: `backend/storage/csv/transaction_repository.rs:111-124`

**Step 1: Update CSV header and write**

```rust
// Write header - ADD "type" column
csv_writer.write_record(&["id", "child_id", "date", "description", "amount", "balance", "type"])?;

// Write transactions - ADD type field
for transaction in transactions {
    let type_str = match transaction.transaction_type {
        DomainTransactionType::Allowance => "allowance",
        DomainTransactionType::OneOffIncome => "income",
        DomainTransactionType::Expense => "expense",
        DomainTransactionType::FutureAllowance => "future_allowance",
    };
    csv_writer.write_record(&[
        &transaction.id,
        &transaction.child_id,
        &transaction.date.to_rfc3339(),
        &transaction.description,
        &transaction.amount.to_string(),
        &transaction.balance.to_string(),
        type_str,
    ])?;
}
```

**Step 2: Build**

Run: `cargo build 2>&1`

**Step 3: Commit**

```bash
git add backend/storage/csv/transaction_repository.rs
git commit -m "feat: write transaction type to CSV"
```

---

## Task 6: Update CSV Schema - Read with Backward Compatibility

**Files:**
- Modify: `backend/storage/csv/transaction_repository.rs:55-67`

**Step 1: Update transaction reading with backward compat**

```rust
let transaction = DomainTransaction {
    id: record.get(0).unwrap_or("").to_string(),
    child_id: record.get(1).unwrap_or("").to_string(),
    date: parsed_date,
    description: record.get(3).unwrap_or("").to_string(),
    amount: record.get(4).unwrap_or("0").parse::<f64>().unwrap_or(0.0),
    balance: record.get(5).unwrap_or("0").parse::<f64>().unwrap_or(0.0),
    transaction_type: Self::parse_transaction_type(
        record.get(6),  // type column (may be None for old data)
        record.get(3).unwrap_or(""),  // description for fallback
        record.get(4).unwrap_or("0").parse::<f64>().unwrap_or(0.0),  // amount for fallback
    ),
};
```

**Step 2: Add helper method for type parsing**

```rust
/// Parse transaction type from CSV, with backward compatibility
fn parse_transaction_type(
    type_field: Option<&str>,
    description: &str,
    amount: f64,
) -> DomainTransactionType {
    // If type column exists, use it
    if let Some(type_str) = type_field {
        match type_str.to_lowercase().as_str() {
            "allowance" => return DomainTransactionType::Allowance,
            "income" | "oneoffincome" => return DomainTransactionType::OneOffIncome,
            "expense" => return DomainTransactionType::Expense,
            "future_allowance" | "futureallowance" => return DomainTransactionType::FutureAllowance,
            _ => {} // Fall through to derivation
        }
    }

    // Backward compatibility: derive from description/amount
    let desc_lower = description.to_lowercase();
    if desc_lower.contains("allowance") || desc_lower.contains("weekly") {
        DomainTransactionType::Allowance
    } else if amount >= 0.0 {
        DomainTransactionType::OneOffIncome
    } else {
        DomainTransactionType::Expense
    }
}
```

**Step 3: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -20`

**Step 4: Commit**

```bash
git add backend/storage/csv/transaction_repository.rs
git commit -m "feat: read transaction type from CSV with backward compat"
```

---

## Task 7: Update Transaction Creation to Set Allowance Type

**Files:**
- Modify: `backend/domain/transaction_service.rs:420-450`

**Step 1: Update create_allowance_transaction to set Allowance type**

Find `create_allowance_transaction` and ensure it creates with `Allowance` type:

```rust
fn create_allowance_transaction(
    &self,
    child_id: &str,
    date: NaiveDate,
    amount: f64,
) -> Result<DomainTransaction> {
    // ... existing date conversion code ...

    let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let transaction_id = DomainTransaction::generate_id(amount, now_millis);

    let transaction_balance = self
        .balance_service
        .calculate_balance_for_new_transaction(
            child_id,
            &eastern_datetime.to_rfc3339(),
            amount,
        )?;

    let domain_transaction = DomainTransaction {
        id: transaction_id,
        child_id: child_id.to_string(),
        date: eastern_datetime,
        description: "Weekly allowance".to_string(),
        amount,
        balance: transaction_balance,
        transaction_type: DomainTransactionType::Allowance,  // EXPLICIT Allowance type
    };

    self.transaction_repository
        .store_transaction(&domain_transaction)?;

    // ... balance recalculation ...

    Ok(domain_transaction)
}
```

**Step 2: Build**

Run: `cargo build 2>&1`

**Step 3: Commit**

```bash
git add backend/domain/transaction_service.rs
git commit -m "feat: set Allowance type when creating allowance transactions"
```

---

## Task 8: Extract Shared Amount Calculation

**Files:**
- Modify: `backend/domain/allowance_service.rs`

**Step 1: Add helper function**

Add near top of impl block:

```rust
/// Calculate allowance amount for a given date
/// Handles both fixed amount and age-based amount modes
fn calculate_allowance_amount(
    config: &DomainAllowanceConfig,
    child_birthdate: Option<NaiveDate>,
    target_date: NaiveDate,
) -> f64 {
    if config.use_age_based_amount {
        if let Some(birthdate) = child_birthdate {
            age_on_date(birthdate, target_date) as f64
        } else {
            config.amount
        }
    } else {
        config.amount
    }
}
```

**Step 2: Update generate_future_allowance_transactions to use helper**

Replace lines 273-284:

```rust
let amount = Self::calculate_allowance_amount(&config, child_birthdate, current);
```

**Step 3: Build**

Run: `cargo build 2>&1`

**Step 4: Commit**

```bash
git add backend/domain/allowance_service.rs
git commit -m "refactor: extract calculate_allowance_amount helper"
```

---

## Task 9: Fix get_pending_allowance_dates to Use Shared Amount Calc

**Files:**
- Modify: `backend/domain/allowance_service.rs:342-391`

**Step 1: Add birthdate lookup (like generate_future_allowance_transactions)**

After getting config, add:

```rust
// If using age-based amount, fetch child's birthdate
let child_birthdate = if config.use_age_based_amount {
    let get_child_command = GetChildCommand { child_id: child_id.to_string() };
    match self.child_service.get_child(get_child_command)? {
        result if result.child.is_some() => {
            let child = result.child.unwrap();
            Some(child.birthdate)
        }
        _ => {
            warn!("Age-based amount enabled but child not found: {}", child_id);
            return Ok(Vec::new());
        }
    }
} else {
    None
};
```

**Step 2: Update the loop to use shared amount calculation**

Replace line 373:
```rust
// OLD: pending_dates.push((current, config.amount));
// NEW:
let amount = Self::calculate_allowance_amount(&config, child_birthdate, current);
pending_dates.push((current, amount));
```

**Step 3: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -20`

**Step 4: Commit**

```bash
git add backend/domain/allowance_service.rs
git commit -m "fix: use age-based amount in get_pending_allowance_dates"
```

---

## Task 10: Fix Duplicate Detection to Use Transaction Type

**Files:**
- Modify: `backend/domain/allowance_service.rs:395-437`

**Step 1: Simplify has_allowance_for_date**

Replace the entire method with type-based check:

```rust
/// Check if an allowance already exists for a specific date
/// This is used to prevent duplicate allowances
fn has_allowance_for_date(&self, child_id: &str, date: NaiveDate) -> Result<bool> {
    info!("ALLOWANCE DEBUG: has_allowance_for_date() called for child {} on date {}", child_id, date);

    let transactions = self.transaction_repository.list_transactions(child_id, None, None)?;
    let date_str = date.format("%Y-%m-%d").to_string();

    let has_allowance = transactions.iter().any(|t| {
        let tx_date_str = t.date.format("%Y-%m-%d").to_string();
        let is_allowance_type = t.transaction_type == DomainTransactionType::Allowance;
        let is_same_date = tx_date_str == date_str;

        if is_same_date {
            info!("ALLOWANCE DEBUG: Found transaction on {}: type={:?}, desc={}",
                  date, t.transaction_type, t.description);
        }

        is_allowance_type && is_same_date
    });

    info!("ALLOWANCE DEBUG: has_allowance_for_date() result: {}", has_allowance);
    Ok(has_allowance)
}
```

**Step 2: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -20`

**Step 3: Commit**

```bash
git add backend/domain/allowance_service.rs
git commit -m "fix: use transaction type for duplicate allowance detection"
```

---

## Task 11: Update Tests

**Files:**
- Multiple test files

**Step 1: Find and update test references**

Run: `grep -rn "TransactionType::Income\|DomainTransactionType::Income" backend/ --include="*.rs" | grep -v "^Binary"`

Update all test assertions to use `OneOffIncome` where appropriate, and `Allowance` for allowance-related tests.

**Step 2: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass

**Step 3: Commit**

```bash
git add -A
git commit -m "test: update tests for new transaction types"
```

---

## Task 12: Manual Verification

**Step 1: Build release**

Run: `cargo build --release 2>&1`

**Step 2: Test the app manually**

1. Start the app
2. Verify existing transactions load correctly (backward compat)
3. Verify today's allowance is created with correct amount
4. Restart app - verify NO duplicate allowances created
5. Check CSV file has "type" column for new transactions

**Step 3: Final commit if any fixes needed**

---

## Files Changed Summary

| File | Changes |
|------|---------|
| `backend/domain/models/transaction.rs` | Add Allowance, rename Income→OneOffIncome |
| `shared/src/lib.rs` | Same enum changes |
| `backend/domain/mappers.rs` | Update type mappings |
| `backend/storage/csv/transaction_repository.rs` | CSV read/write with type column |
| `backend/domain/transaction_service.rs` | Set Allowance type on creation |
| `backend/domain/allowance_service.rs` | Extract amount calc, fix duplicate detection |
| Multiple | Update Income→OneOffIncome references |

---

## Rollback Plan

If issues arise:
1. Existing CSV files without "type" column will still work (backward compat)
2. Can revert commits individually
3. No database migration needed - CSV is self-describing
