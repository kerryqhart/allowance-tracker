//! Balance management service for the allowance tracker.
//!
//! This service handles the complex logic of recalculating balances when backdated 
//! transactions are inserted. It ensures that all subsequent transactions have their
//! balances updated correctly to maintain data integrity.

use anyhow::Result;
use log::{info, warn};
use std::sync::Arc;
use crate::backend::storage::csv::{CsvConnection, TransactionRepository};
use crate::backend::storage::traits::TransactionStorage;

/// Service responsible for balance calculations and recalculations
#[derive(Clone)]
pub struct BalanceService {
    transaction_repository: TransactionRepository,
}

impl BalanceService {
    pub fn new(connection: Arc<CsvConnection>) -> Self {
        let transaction_repository = TransactionRepository::new((*connection).clone());
        Self { transaction_repository }
    }

    /// Recalculate all balances from a specific date forward
    /// This is called when a backdated transaction is inserted
    /// 
    /// The algorithm:
    /// 1. Get all transactions from the backdated date forward (chronological order)
    /// 2. Calculate the starting balance (balance before the first transaction in our list)
    /// 3. Recalculate each transaction's balance based on the running total
    /// 4. Update all affected transactions in the database atomically
    pub fn recalculate_balances_from_date(&self, child_id: &str, from_date: &str) -> Result<usize> {
        info!("Starting balance recalculation for child {} from date {}", child_id, from_date);

        // Get all transactions from the specified date forward (chronological order)
        let mut transactions = self.transaction_repository
            .get_transactions_since(child_id, from_date)?;

        if transactions.is_empty() {
            info!("No transactions found after {}, no balance recalculation needed", from_date);
            return Ok(0);
        }

        // CRITICAL: Sort transactions by date to ensure correct balance calculation
        transactions.sort_by(|a, b| a.date.cmp(&b.date));

        info!("Found {} transactions to recalculate", transactions.len());

        // Calculate the starting balance (balance just before our first transaction)
        let starting_balance = self.calculate_starting_balance(child_id, from_date)?;
        info!("Starting balance for recalculation: ${:.2}", starting_balance);

        // Recalculate balances for all transactions
        let mut running_balance = starting_balance;
        let mut balance_updates = Vec::new();

        for transaction in &transactions {
            running_balance += transaction.amount;
            balance_updates.push((transaction.id.clone(), running_balance));
            
            info!("Transaction {}: amount={:.2}, new_balance={:.2}", 
                  transaction.id, transaction.amount, running_balance);
        }

        // Update all balances atomically
        self.transaction_repository
            .update_transaction_balances(&balance_updates)?;

        info!("Successfully recalculated {} transaction balances", balance_updates.len());
        Ok(balance_updates.len())
    }

    /// Calculate the starting balance for a recalculation
    /// This is the balance just before the specified date
    fn calculate_starting_balance(&self, child_id: &str, from_date: &str) -> Result<f64> {
        // Find the most recent transaction before the specified date
        match self.transaction_repository
            .get_latest_transaction_before_date(child_id, from_date)? 
        {
            Some(transaction) => {
                info!("Found previous transaction {} with balance ${:.2}", 
                      transaction.id, transaction.balance);
                Ok(transaction.balance)
            }
            None => {
                info!("No transactions found before {}, starting balance is $0.00", from_date);
                Ok(0.0)
            }
        }
    }

    /// Calculate the correct balance for a new transaction at a specific date
    /// This is used when inserting a backdated transaction to determine its balance
    pub fn calculate_balance_for_new_transaction(&self, child_id: &str, transaction_date: &str, transaction_amount: f64) -> Result<f64> {
        // First, get the most recent transaction before this date (excluding same day)
        let base_balance = match self.transaction_repository
            .get_latest_transaction_before_date(child_id, transaction_date)? 
        {
            Some(transaction) => transaction.balance,
            None => 0.0,
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
            running_balance += tx.amount;
        }

        let final_balance = running_balance + transaction_amount;
        
        info!("Calculated balance for new transaction: base_balance={:.2} + same_day_adjustments={:.2} + amount={:.2} = {:.2}", 
              base_balance, running_balance - base_balance, transaction_amount, final_balance);
        if !same_day_earlier_transactions.is_empty() {
            info!("  Found {} same-day earlier transactions", same_day_earlier_transactions.len());
            for (i, tx) in same_day_earlier_transactions.iter().enumerate() {
                info!("    {}: {} amount={:.2} at {}", i + 1, tx.id, tx.amount, tx.date);
            }
        }
        
        Ok(final_balance)
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

    /// Validate that all balances are correct for a child
    /// This is a diagnostic method to ensure balance integrity
    pub fn validate_all_balances(&self, child_id: &str) -> Result<Vec<String>> {
        info!("Validating all balances for child {}", child_id);
        
        let transactions = self.transaction_repository
            .list_transactions_chronological(child_id, None, None)?;

        let mut errors = Vec::new();
        let mut expected_balance = 0.0;

        for transaction in transactions {
            expected_balance += transaction.amount;
            
            if (transaction.balance - expected_balance).abs() > 0.001 { // Small epsilon for float comparison
                let error = format!(
                    "Transaction {} has incorrect balance: expected {:.2}, actual {:.2}", 
                    transaction.id, expected_balance, transaction.balance
                );
                errors.push(error);
                warn!("Balance validation error: {}", errors.last().unwrap());
            }
        }

        if errors.is_empty() {
            info!("All balances are correct for child {}", child_id);
        } else {
            warn!("Found {} balance errors for child {}", errors.len(), child_id);
        }

        Ok(errors)
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
                    info!("Found transaction {} at {} with balance ${:.2}", 
                          transaction.id, tx_date_str, transaction.balance);
                    Ok(transaction.balance)
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
                info!("Current balance for child {}: ${:.2}", child_id, transaction.balance);
                Ok(transaction.balance)
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

    fn create_test_service() -> BalanceService {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        BalanceService::new(db)
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
        
        let transaction = Transaction {
            id: Transaction::generate_id(amount, now_millis),
            child_id: child_id.to_string(),
            date: parsed_date,
            description: description.to_string(),
            amount,
            balance,
            transaction_type: if amount >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
        };

        service.transaction_repository.store_transaction(&transaction).unwrap();
        transaction
    }

    #[test]
    fn test_calculate_starting_balance_with_previous_transaction() {
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        // Create a transaction before our target date
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "Previous transaction", 50.0, 50.0);

        let starting_balance = service.calculate_starting_balance(child_id, "2025-01-15T10:00:00-05:00").unwrap();
        assert_eq!(starting_balance, 50.0);
    }

    #[test]
    fn test_calculate_starting_balance_no_previous_transaction() {
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        let starting_balance = service.calculate_starting_balance(child_id, "2025-01-15T10:00:00-05:00").unwrap();
        assert_eq!(starting_balance, 0.0);
    }

    #[test]
    fn test_calculate_balance_for_new_transaction() {
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

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
        let child_service = crate::backend::domain::child_service::ChildService::new(connection.clone());
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        println!("🧪 TEST: Setting up initial transactions with correct balances");
        
        // Step 1: Create sequential transactions with correct balances
        let tx1 = create_test_transaction(&balance_service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0);
        
        let tx2 = create_test_transaction(&balance_service, child_id, "2025-01-15T10:00:00-05:00", "Second", -20.0, 80.0);
        
        let tx3 = create_test_transaction(&balance_service, child_id, "2025-01-20T10:00:00-05:00", "Third", 50.0, 130.0);

        println!("🧪 TEST: Initial balances - tx1: {}, tx2: {}, tx3: {}", tx1.balance, tx2.balance, tx3.balance);
        
        // Step 2: Verify initial balances are correct
        let initial_errors = balance_service.validate_all_balances(child_id).unwrap();
        assert!(initial_errors.is_empty(), "Initial balances should be correct: {:?}", initial_errors);

        // Step 3: Insert a backdated transaction between tx1 and tx2
        let backdated_tx = create_test_transaction(&balance_service, child_id, "2025-01-12T10:00:00-05:00", "Backdated", 25.0, 125.0);
        println!("🧪 TEST: Inserted backdated transaction: {}", backdated_tx.balance);
        
        // Step 4: At this point, tx2 and tx3 have wrong balances because of the backdated insertion
        // tx2 should be 105.0 (125.0 - 20.0) but is still 80.0
        // tx3 should be 155.0 (105.0 + 50.0) but is still 130.0
        
        // Step 5: Recalculate balances from the backdated transaction date
        println!("🧪 TEST: Recalculating balances from backdated transaction date");
        let updated_count = balance_service.recalculate_balances_from_date(child_id, "2025-01-12T10:00:00-05:00").unwrap();
        
        // Should update 3 transactions: backdated + 2 subsequent
        assert_eq!(updated_count, 3, "Should have updated 3 transactions (backdated + 2 subsequent)");

        // Step 6: Validate that all balances are now correct
        let final_errors = balance_service.validate_all_balances(child_id).unwrap();
        assert!(final_errors.is_empty(), "Final balance validation should pass: {:?}", final_errors);
        
        println!("🧪 TEST: Balance recalculation test passed!");
    }

    #[test]
    fn test_requires_balance_recalculation() {
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

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
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        // Create transactions with correct balances
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0);
        
        create_test_transaction(&service, child_id, "2025-01-15T10:00:00-05:00", "Second", -30.0, 70.0);
        
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Third", 20.0, 90.0);

        let errors = service.validate_all_balances(child_id).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_all_balances_incorrect() {
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        // Create transactions with intentionally incorrect balances
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0);
        
        create_test_transaction(&service, child_id, "2025-01-15T10:00:00-05:00", "Second", -30.0, 75.0); // Should be 70.0
        
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Third", 20.0, 85.0); // Should be 90.0

        let errors = service.validate_all_balances(child_id).unwrap();
        assert_eq!(errors.len(), 2); // Two incorrect balances
    }

    #[test]
    fn test_calculate_projected_balance_for_transaction_no_history() {
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        // No previous transactions - first allowance should be the amount itself
        let projected_balance = service
            .calculate_projected_balance_for_transaction(child_id, "2025-07-25T12:00:00+00:00", 10.0)
            .expect("Failed to calculate projected balance");
        
        assert_eq!(projected_balance, 10.0, "First transaction should result in balance equal to the amount");
    }

    #[test]
    fn test_calculate_projected_balance_for_transaction_with_history() {
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

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
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

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
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

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
        let service = create_test_service();
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

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