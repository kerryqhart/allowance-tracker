//! Balance management service for the allowance tracker.
//!
//! This service handles the complex logic of recalculating balances when backdated 
//! transactions are inserted. It ensures that all subsequent transactions have their
//! balances updated correctly to maintain data integrity.

use anyhow::Result;
use log::info;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::backend::storage::csv::{CsvConnection, TransactionRepository};
use crate::backend::storage::traits::TransactionStorage;
use crate::backend::domain::models::transaction::{Transaction, TransactionType};
use crate::backend::domain::SyncNotifier;
use shared::sync::{EntityType, SyncAction, SyncEvent, SyncSource};
use allowance_core::balance::{self, BalanceMismatch};
use allowance_core::money::Money;
use allowance_core::row::{TxRow, TxType};

/// Outcome of checking a child's stored balances.
///
/// Kept distinct from a bare `Vec<BalanceMismatch>` (or an empty one) so a
/// verdict can never be misread as "no mismatches" when it actually means
/// something else — see `BalanceService::validate_all_balances`.
#[must_use]
#[derive(Debug, Clone, PartialEq)]
pub enum BalanceCheck {
    /// Every stored balance matches the recomputed running total.
    Ok,
    /// Balances were read successfully and disagree.
    Mismatches(Vec<BalanceMismatch>),
}

/// Map the domain `Transaction` onto the pure `allowance_core` row type so
/// `recompute_running_balances`/`validate` can run over it. `allowance-core`
/// does no I/O and knows nothing about the CSV storage layer, so this
/// boundary conversion lives here rather than in either crate.
fn domain_to_row(t: &Transaction) -> TxRow {
    TxRow {
        id: t.id.clone(),
        child_id: t.child_id.clone(),
        date: t.date,
        description: t.description.clone(),
        amount: t.amount,
        balance: t.balance,
        tx_type: match t.transaction_type {
            TransactionType::Allowance => TxType::Allowance,
            TransactionType::OneOffIncome => TxType::OneOffIncome,
            TransactionType::Expense => TxType::Expense,
            TransactionType::FutureAllowance => TxType::FutureAllowance,
        },
    }
}

/// Service responsible for balance calculations and recalculations
#[derive(Clone)]
pub struct BalanceService {
    transaction_repository: TransactionRepository,
    sync_notifier: Option<SyncNotifier>,
}

impl BalanceService {
    pub fn new(connection: Arc<CsvConnection>) -> Self {
        let transaction_repository = TransactionRepository::new((*connection).clone());
        Self { transaction_repository, sync_notifier: None }
    }

    /// Attach a sync notifier so recalculated rows emit Updated events.
    pub fn with_sync_notifier(mut self, notifier: Option<SyncNotifier>) -> Self {
        self.sync_notifier = notifier;
        self
    }

    /// Load every transaction for a child, mapped onto the pure `TxRow` type
    /// `allowance_core::balance` operates on.
    fn load_rows(&self, child_id: &str) -> Result<Vec<TxRow>> {
        let transactions = self.transaction_repository
            .list_transactions_chronological(child_id, None, None)?;
        Ok(transactions.iter().map(domain_to_row).collect())
    }

    /// Recalculate all balances from a specific date forward.
    /// This is called when a backdated transaction is inserted.
    ///
    /// The algorithm:
    /// 1. Load every transaction for the child.
    /// 2. Recompute every running balance in one pure pass, always
    ///    accumulating from `Money::from_cents(0)` — there is no seed balance
    ///    to derive, so a stale or `BALANCE_PENDING` prior balance can never
    ///    reach an unchecked `Add` (the hazard the old
    ///    `calculate_starting_balance` seed carried).
    /// 3. Write the recomputed balances for the rows on/after `from_date`
    ///    back in one pass — the child id is already known, so there is no
    ///    need to rediscover it by scanning every child's CSV per row.
    pub fn recalculate_balances_from_date(&self, child_id: &str, from_date: &str) -> Result<usize> {
        info!("Starting balance recalculation for child {} from date {}", child_id, from_date);

        // Which rows are in scope for this call. Kept only to preserve the
        // early return and the returned count — the arithmetic below always
        // runs over the whole ledger.
        let affected_ids: HashSet<String> = self.transaction_repository
            .get_transactions_since(child_id, from_date)?
            .into_iter()
            .map(|t| t.id)
            .collect();

        if affected_ids.is_empty() {
            info!("No transactions found after {}, no balance recalculation needed", from_date);
            return Ok(0);
        }

        let mut rows = self.load_rows(child_id)?;
        let original_balances: HashMap<String, Money> =
            rows.iter().map(|r| (r.id.clone(), r.balance)).collect();

        balance::recompute_running_balances(&mut rows);

        // Track which in-scope rows actually changed balance, so we only
        // emit Updated sync events for real diffs — the common path (a
        // current-date insert) recomputes the just-inserted row to the same
        // balance it was stored with, and we don't want to emit a redundant
        // Updated on top of the Created event.
        let mut balance_updates: Vec<(String, Money)> = Vec::new();
        let mut changed_ids: Vec<String> = Vec::new();

        for row in &rows {
            if !affected_ids.contains(&row.id) {
                continue;
            }
            balance_updates.push((row.id.clone(), row.balance));
            if original_balances.get(&row.id) != Some(&row.balance) {
                changed_ids.push(row.id.clone());
            }
            info!("Transaction {}: new_balance={}", row.id, row.balance.render());
        }

        // Write the recalculated balances back in one pass.
        self.transaction_repository
            .update_transaction_balances(child_id, &balance_updates)?;

        // Emit Updated sync events only for rows whose balance actually changed.
        if let Some(ref notifier) = self.sync_notifier {
            for tx_id in &changed_ids {
                notifier.notify(SyncEvent::new(
                    EntityType::Transaction,
                    tx_id.clone(),
                    child_id.to_string(),
                    SyncAction::Updated,
                    SyncSource::Local,
                ));
            }
        }

        info!("Successfully recalculated {} transaction balances", balance_updates.len());
        Ok(balance_updates.len())
    }

    /// Calculate the correct balance for a new transaction at a specific date
    /// This is used when inserting a backdated transaction to determine its balance
    pub fn calculate_balance_for_new_transaction(&self, child_id: &str, transaction_date: &str, transaction_amount: f64) -> Result<f64> {
        // Boundary conversion: this method's public signature is still f64
        // dollars (it is called from call sites this task does not own), but
        // every internal accumulation below is done in exact Money cents.
        let transaction_amount = Money::from_cents((transaction_amount * 100.0).round() as i64);

        // First, get the most recent transaction before this date (excluding same day)
        let base_balance = match self.transaction_repository
            .get_latest_transaction_before_date(child_id, transaction_date)?
        {
            Some(transaction) => transaction.balance,
            None => Money::from_cents(0),
        };

        // Then, get all transactions from the same day that occurred before this one
        // by getting all transactions from that day and filtering by timestamp
        let same_day_transactions = self.transaction_repository
            .get_transactions_since(child_id, transaction_date)?;

        // Filter to only transactions from the exact same day that have a lower timestamp
        let mut same_day_earlier_transactions = Vec::new();
        
        // Extract the date part from the transaction date (YYYY-MM-DD)
        let target_date_part = if let Some(date_part) = transaction_date.split('T').next() {
            date_part
        } else {
            transaction_date // Fallback if not RFC3339 format
        };

        for tx in same_day_transactions {
            // Check if this transaction is from the same day
            let tx_date_str = tx.date.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
            if let Some(tx_date_part) = tx_date_str.split('T').next() {
                if tx_date_part == target_date_part {
                    // Check if this transaction occurred before our new transaction
                    // We'll use string comparison of the full timestamp since RFC3339 sorts lexicographically
                    if tx_date_str.as_str() < transaction_date {
                        same_day_earlier_transactions.push(tx);
                    }
                }
            }
        }

        // Sort same-day transactions by date to ensure proper order
        same_day_earlier_transactions.sort_by(|a, b| a.date.cmp(&b.date));

        // Calculate the running balance including same-day transactions
        let mut running_balance = base_balance;
        for tx in &same_day_earlier_transactions {
            running_balance = running_balance + tx.amount;
        }

        let final_balance = running_balance + transaction_amount;

        info!("Calculated balance for new transaction: base_balance={} + same_day_adjustments={} + amount={} = {}",
              base_balance.render(), (running_balance - base_balance).render(), transaction_amount.render(), final_balance.render());
        if !same_day_earlier_transactions.is_empty() {
            info!("  Found {} same-day earlier transactions", same_day_earlier_transactions.len());
            for (i, tx) in same_day_earlier_transactions.iter().enumerate() {
                info!("    {}: {} amount={} at {}", i + 1, tx.id, tx.amount.render(), tx.date);
            }
        }

        // Boundary conversion back to the untouched f64 public signature.
        Ok(final_balance.cents() as f64 / 100.0)
    }

    /// Calculate the projected balance for a future transaction
    /// This is specifically for calculating what the balance would be for future allowances
    /// without actually inserting the transaction. Used by CalendarService for display purposes.
    pub fn calculate_projected_balance_for_transaction(&self, child_id: &str, transaction_date: &str, transaction_amount: f64) -> Result<f64> {
        // Reuse the existing calculate_balance_for_new_transaction logic
        // This method is identical in implementation but semantically different:
        // - calculate_balance_for_new_transaction: for actual insertion
        // - calculate_projected_balance_for_transaction: for projection/display only
        self.calculate_balance_for_new_transaction(child_id, transaction_date, transaction_amount)
    }

    /// Check if inserting a transaction at a specific date would require balance recalculation
    /// Returns true if there are any transactions after the specified date
    pub fn requires_balance_recalculation(&self, child_id: &str, transaction_date: &str) -> Result<bool> {
        let transactions_after = self.transaction_repository
            .get_transactions_since(child_id, transaction_date)?;

        // If there are transactions after this date (excluding exact matches), we need recalculation
        let needs_recalc = transactions_after.iter().any(|tx| {
            let tx_date_str = tx.date.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
            tx_date_str.as_str() > transaction_date
        });
        
        info!("Balance recalculation needed for date {}: {}", transaction_date, needs_recalc);
        Ok(needs_recalc)
    }

    /// Validate that all balances are correct for a child.
    ///
    /// Was `Result<Vec<String>>`, which returned Ok while reporting broken
    /// money — a `?` at the call site swallowed it entirely.
    ///
    /// This deviates from the spec's stated `Result<(), Vec<BalanceMismatch>>`
    /// signature. Two channels cannot distinguish "could not read the ledger"
    /// from "read it and it's clean" from "read it and it's wrong" — an
    /// earlier version of this method mapped a read failure to
    /// `Err(Vec::new())`, which a caller inspecting the mismatch list cannot
    /// tell apart from a genuine clean validation. `BalanceCheck` gives the
    /// verdict its own channel so the outer `Result` can carry the I/O error
    /// (with its real cause) without ever being confused for "no mismatches".
    pub fn validate_all_balances(&self, child_id: &str) -> Result<BalanceCheck> {
        let rows = self.load_rows(child_id)?;
        let errors = balance::validate(&rows);
        Ok(if errors.is_empty() { BalanceCheck::Ok } else { BalanceCheck::Mismatches(errors) })
    }

    /// Get balance at or before a specific date
    /// Returns the most recent transaction balance at/before the specified date
    /// Used for goal progression tracking to find balance at goal creation date
    pub fn get_balance_at_date(&self, child_id: &str, target_date: &str) -> Result<f64> {
        info!("Getting balance at date {} for child {}", target_date, child_id);
        
        // Find the most recent transaction at or before the target date
        match self.transaction_repository.get_latest_transaction_before_date(child_id, target_date)? {
            Some(transaction) => {
                // Check if this transaction is exactly on the target date or before
                let tx_date_str = transaction.date.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
                if tx_date_str.as_str() <= target_date {
                    info!("Found transaction {} at {} with balance ${}",
                          transaction.id, tx_date_str, transaction.balance.render());
                    // Boundary conversion back to the untouched f64 public signature.
                    Ok(transaction.balance.cents() as f64 / 100.0)
                } else {
                    info!("No transactions found at or before {}, balance is $0.00", target_date);
                    Ok(0.0)
                }
            }
            None => {
                info!("No transactions found before {}, balance is $0.00", target_date);
                Ok(0.0)
            }
        }
    }

    /// Get the current balance for a child
    /// This returns the balance from the most recent transaction
    pub fn get_current_balance(&self, child_id: &str) -> Result<f64> {
        match self.transaction_repository.get_latest_transaction(child_id)? {
            Some(transaction) => {
                info!("Current balance for child {}: ${}", child_id, transaction.balance.render());
                // Boundary conversion back to the untouched f64 public signature.
                Ok(transaction.balance.cents() as f64 / 100.0)
            }
            None => {
                info!("No transactions found for child {}, balance is $0.00", child_id);
                Ok(0.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::csv::CsvConnection;
    use crate::backend::domain::commands::child::CreateChildCommand;
    use crate::backend::domain::models::transaction::{Transaction, TransactionType};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A balance service over a temp dir, with one child already created
    /// through the *same* connection.
    ///
    /// The child must exist before any transaction call: resolution goes
    /// through the child registry now, so an unregistered child is an error
    /// rather than a fabricated folder. The `TempDir` is returned so the base
    /// directory stays alive for the duration of the test — it used to be
    /// dropped immediately, and only `create_dir_all` on the read path hid
    /// that the base directory had already been deleted.
    fn create_test_service() -> (BalanceService, tempfile::TempDir, String) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service =
            crate::backend::domain::child_service::ChildService::new(db.clone(), None);
        let child_result = child_service
            .create_child(CreateChildCommand {
                name: "Test Child".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .unwrap();
        (BalanceService::new(db), temp_dir, child_result.child.id)
    }

    /// Convert a dollar-amount literal (as tests already write them) into
    /// exact `Money` cents, mirroring `Money`'s own `Deserialize` boundary
    /// conversion. Not money arithmetic — a one-shot literal conversion.
    fn dollars(amount: f64) -> Money {
        Money::from_cents((amount * 100.0).round() as i64)
    }

    fn create_test_transaction(service: &BalanceService, child_id: &str, date: &str, description: &str, amount: f64, balance: f64) -> Transaction {
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let parsed_date = chrono::DateTime::parse_from_rfc3339(date)
            .unwrap_or_else(|_| {
                chrono::DateTime::parse_from_str(&format!("{}T12:00:00-05:00", date), "%Y-%m-%dT%H:%M:%S%z")
                    .expect("Failed to parse date")
            });

        let amount = dollars(amount);
        let balance = dollars(balance);

        let transaction = Transaction {
            id: Transaction::generate_id(amount, now_millis),
            child_id: child_id.to_string(),
            date: parsed_date,
            description: description.to_string(),
            amount,
            balance,
            transaction_type: if amount.cents() >= 0 { TransactionType::OneOffIncome } else { TransactionType::Expense },
        };

        service.transaction_repository.store_transaction(&transaction).unwrap();
        transaction
    }

    #[test]
    fn test_calculate_balance_for_new_transaction() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create a previous transaction
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "Previous", 30.0, 30.0);

        let new_balance = service.calculate_balance_for_new_transaction(child_id, "2025-01-15T10:00:00-05:00", 20.0).unwrap();
        assert_eq!(new_balance, 50.0); // 30 + 20
    }

    #[test]
    fn test_recalculate_balances_from_date() {
        // Fresh test: Test balance recalculation after inserting a backdated transaction
        
        // Set up test environment with shared connection
        let temp_dir = tempfile::tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        
        // Create a child for testing
        let child_service = crate::backend::domain::child_service::ChildService::new(connection.clone(), None);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        println!("TEST: Setting up initial transactions with correct balances");
        
        // Step 1: Create sequential transactions with correct balances
        let tx1 = create_test_transaction(&balance_service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0);
        
        let tx2 = create_test_transaction(&balance_service, child_id, "2025-01-15T10:00:00-05:00", "Second", -20.0, 80.0);
        
        let tx3 = create_test_transaction(&balance_service, child_id, "2025-01-20T10:00:00-05:00", "Third", 50.0, 130.0);

        println!("TEST: Initial balances - tx1: {}, tx2: {}, tx3: {}", tx1.balance.render(), tx2.balance.render(), tx3.balance.render());
        
        // Step 2: Verify initial balances are correct
        let initial_result = balance_service.validate_all_balances(child_id).unwrap();
        assert_eq!(initial_result, BalanceCheck::Ok, "Initial balances should be correct: {:?}", initial_result);

        // Step 3: Insert a backdated transaction between tx1 and tx2
        let backdated_tx = create_test_transaction(&balance_service, child_id, "2025-01-12T10:00:00-05:00", "Backdated", 25.0, 125.0);
        println!("TEST: Inserted backdated transaction: {}", backdated_tx.balance.render());
        
        // Step 4: At this point, tx2 and tx3 have wrong balances because of the backdated insertion
        // tx2 should be 105.0 (125.0 - 20.0) but is still 80.0
        // tx3 should be 155.0 (105.0 + 50.0) but is still 130.0
        
        // Step 5: Recalculate balances from the backdated transaction date
        println!("TEST: Recalculating balances from backdated transaction date");
        let updated_count = balance_service.recalculate_balances_from_date(child_id, "2025-01-12T10:00:00-05:00").unwrap();
        
        // Should update 3 transactions: backdated + 2 subsequent
        assert_eq!(updated_count, 3, "Should have updated 3 transactions (backdated + 2 subsequent)");

        // Step 6: Validate that all balances are now correct
        let final_result = balance_service.validate_all_balances(child_id).unwrap();
        assert_eq!(final_result, BalanceCheck::Ok, "Final balance validation should pass: {:?}", final_result);
        
        println!("TEST: Balance recalculation test passed!");
    }

    #[test]
    fn test_requires_balance_recalculation() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create a transaction after our test date
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Future transaction", 100.0, 100.0);

        // Check if inserting at an earlier date requires recalculation
        let requires_recalc = service.requires_balance_recalculation(child_id, "2025-01-15T10:00:00-05:00").unwrap();
        assert!(requires_recalc);

        // Check if inserting after the last transaction doesn't require recalculation
        let no_recalc_needed = service.requires_balance_recalculation(child_id, "2025-01-25T10:00:00-05:00").unwrap();
        assert!(!no_recalc_needed);
    }

    #[test]
    fn test_validate_all_balances_correct() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create transactions with correct balances
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0);
        
        create_test_transaction(&service, child_id, "2025-01-15T10:00:00-05:00", "Second", -30.0, 70.0);
        
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Third", 20.0, 90.0);

        let result = service.validate_all_balances(child_id).unwrap();
        assert_eq!(result, BalanceCheck::Ok);
    }

    #[test]
    fn test_validate_all_balances_incorrect() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create transactions with intentionally incorrect balances
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0);

        create_test_transaction(&service, child_id, "2025-01-15T10:00:00-05:00", "Second", -30.0, 75.0); // Should be 70.0

        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Third", 20.0, 85.0); // Should be 90.0

        let result = service.validate_all_balances(child_id).unwrap();
        match result {
            BalanceCheck::Mismatches(errors) => assert_eq!(errors.len(), 2), // Two incorrect balances
            BalanceCheck::Ok => panic!("expected mismatches, balances are intentionally wrong"),
        }
    }

    /// A read failure (unregistered child — `list_transactions_chronological`
    /// errors rather than returning an empty list) must surface as `Err` with
    /// its real cause, never as `Ok(BalanceCheck::Ok)` or an empty mismatch
    /// list standing in for "could not read".
    #[test]
    fn validate_all_balances_propagates_a_read_failure_rather_than_reporting_clean() {
        let (service, _temp_dir, _child_id) = create_test_service();

        let result = service.validate_all_balances("no-such-child");

        let err = result.expect_err("an unregistered child must fail to read, not validate as clean");
        let message = format!("{err:#}");
        assert!(
            !message.is_empty(),
            "the underlying cause must survive on the error, not be discarded"
        );
    }

    #[test]
    fn test_calculate_projected_balance_for_transaction_no_history() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // No previous transactions - first allowance should be the amount itself
        let projected_balance = service
            .calculate_projected_balance_for_transaction(child_id, "2025-07-25T12:00:00+00:00", 10.0)
            .expect("Failed to calculate projected balance");
        
        assert_eq!(projected_balance, 10.0, "First transaction should result in balance equal to the amount");
    }

    #[test]
    fn test_calculate_projected_balance_for_transaction_with_history() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create some historical transactions
        create_test_transaction(&service, child_id, "2025-07-01T12:00:00+00:00", "Previous allowance", 10.0, 10.0);
        create_test_transaction(&service, child_id, "2025-07-15T12:00:00+00:00", "Spending", -3.0, 7.0);

        // Project balance for a future allowance
        let projected_balance = service
            .calculate_projected_balance_for_transaction(child_id, "2025-07-25T12:00:00+00:00", 10.0)
            .expect("Failed to calculate projected balance");
        
        assert_eq!(projected_balance, 17.0, "Projected balance should be previous balance (7.0) + new amount (10.0)");
    }

    #[test]
    fn test_calculate_projected_balance_for_transaction_mid_month() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create transactions across multiple weeks
        create_test_transaction(&service, child_id, "2025-07-04T12:00:00+00:00", "Week 1 allowance", 10.0, 10.0);
        create_test_transaction(&service, child_id, "2025-07-11T12:00:00+00:00", "Week 2 allowance", 10.0, 20.0);
        create_test_transaction(&service, child_id, "2025-07-16T14:30:00+00:00", "Spending", -5.0, 15.0);

        // Project balance for next allowance
        let projected_balance = service
            .calculate_projected_balance_for_transaction(child_id, "2025-07-18T12:00:00+00:00", 10.0)
            .expect("Failed to calculate projected balance");
        
        assert_eq!(projected_balance, 25.0, "Mid-month projection should account for all previous transactions");
    }

    #[test]
    fn test_calculate_projected_balance_for_transaction_same_day_earlier() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create early morning spending transaction
        create_test_transaction(&service, child_id, "2025-07-18T08:00:00+00:00", "Early spending", -2.0, -2.0);
        
        // Create morning allowance transaction (after spending)
        create_test_transaction(&service, child_id, "2025-07-18T10:00:00+00:00", "Morning allowance", 10.0, 8.0);

        // Project balance for afternoon transaction on same day
        let projected_balance = service
            .calculate_projected_balance_for_transaction(child_id, "2025-07-18T15:00:00+00:00", 5.0)
            .expect("Failed to calculate projected balance");
        
        assert_eq!(projected_balance, 13.0, "Same-day projection should account for earlier same-day transactions (8.0 + 5.0)");
    }

    #[test]
    fn test_calculate_projected_balance_for_transaction_complex_scenario() {
        let (service, _temp_dir, child_id) = create_test_service();
        let child_id = &child_id;

        // Create a complex history that mimics real allowance scenario
        create_test_transaction(&service, child_id, "2025-06-27T12:00:00+00:00", "June Week 4", 10.0, 10.0);
        create_test_transaction(&service, child_id, "2025-07-04T12:00:00+00:00", "July Week 1", 10.0, 20.0);
        create_test_transaction(&service, child_id, "2025-07-07T16:00:00+00:00", "Toy purchase", -8.0, 12.0);
        create_test_transaction(&service, child_id, "2025-07-11T12:00:00+00:00", "July Week 2", 10.0, 22.0);
        create_test_transaction(&service, child_id, "2025-07-18T12:00:00+00:00", "July Week 3", 10.0, 32.0);

        // Project balance for the next allowance (July Week 4)
        let projected_balance = service
            .calculate_projected_balance_for_transaction(child_id, "2025-07-25T12:00:00+00:00", 10.0)
            .expect("Failed to calculate projected balance");
        
        assert_eq!(projected_balance, 42.0, "Complex scenario: should project correct balance for future allowance");
    }
} 