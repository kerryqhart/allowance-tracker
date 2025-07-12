//! Balance management service for the allowance tracker.
//!
//! This service handles the complex logic of recalculating balances when backdated 
//! transactions are inserted. It ensures that all subsequent transactions have their
//! balances updated correctly to maintain data integrity.

use anyhow::Result;
use log::{info, warn};
use std::sync::Arc;
use crate::backend::storage::{Connection, TransactionStorage};

/// Service responsible for balance calculations and recalculations
#[derive(Clone)]
pub struct BalanceService<C: Connection> {
    transaction_repository: C::TransactionRepository,
}

impl<C: Connection> BalanceService<C> {
    pub fn new(connection: Arc<C>) -> Self {
        let transaction_repository = connection.create_transaction_repository();
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
            if let Some(tx_date_part) = tx.date.split('T').next() {
                if tx_date_part == target_date_part {
                    // Check if this transaction occurred before our new transaction
                    // We'll use string comparison of the full timestamp since RFC3339 sorts lexicographically
                    if tx.date.as_str() < transaction_date {
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

    /// Check if inserting a transaction at a specific date would require balance recalculation
    /// Returns true if there are any transactions after the specified date
    pub fn requires_balance_recalculation(&self, child_id: &str, transaction_date: &str) -> Result<bool> {
        let transactions_after = self.transaction_repository
            .get_transactions_since(child_id, transaction_date)?;

        // If there are transactions after this date (excluding exact matches), we need recalculation
        let needs_recalc = transactions_after.iter().any(|tx| tx.date.as_str() > transaction_date);
        
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::csv::CsvConnection;
    use crate::backend::domain::commands::child::CreateChildCommand;
    use crate::backend::domain::models::transaction::{Transaction, TransactionType};
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn create_test_service() -> BalanceService<CsvConnection> {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        BalanceService::new(db)
    }

    async fn create_test_transaction(service: &BalanceService<CsvConnection>, child_id: &str, date: &str, description: &str, amount: f64, balance: f64) -> Transaction {
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let transaction = Transaction {
            id: Transaction::generate_id(amount, now_millis),
            child_id: child_id.to_string(),
            date: date.to_string(),
            description: description.to_string(),
            amount,
            balance,
            transaction_type: if amount >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
        };

        service.transaction_repository.store_transaction(&transaction).await.unwrap();
        transaction
    }

    #[tokio::test]
    async fn test_calculate_starting_balance_with_previous_transaction() {
        let service = create_test_service().await;
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_result.child.id;

        // Create a transaction before our target date
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "Previous transaction", 50.0, 50.0).await;

        let starting_balance = service.calculate_starting_balance(child_id, "2025-01-15T10:00:00-05:00").await.unwrap();
        assert_eq!(starting_balance, 50.0);
    }

    #[tokio::test]
    async fn test_calculate_starting_balance_no_previous_transaction() {
        let service = create_test_service().await;
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_result.child.id;

        let starting_balance = service.calculate_starting_balance(child_id, "2025-01-15T10:00:00-05:00").await.unwrap();
        assert_eq!(starting_balance, 0.0);
    }

    #[tokio::test]
    async fn test_calculate_balance_for_new_transaction() {
        let service = create_test_service().await;
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_result.child.id;

        // Create a previous transaction
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "Previous", 30.0, 30.0).await;

        let new_balance = service.calculate_balance_for_new_transaction(child_id, "2025-01-15T10:00:00-05:00", 20.0).await.unwrap();
        assert_eq!(new_balance, 50.0); // 30 + 20
    }

    #[tokio::test]
    async fn test_recalculate_balances_from_date() {
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
        }).await.unwrap();
        let child_id = &child_result.child.id;

        println!("🧪 TEST: Setting up initial transactions with correct balances");
        
        // Step 1: Create sequential transactions with correct balances
        let tx1 = create_test_transaction(&balance_service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let tx2 = create_test_transaction(&balance_service, child_id, "2025-01-15T10:00:00-05:00", "Second", -20.0, 80.0).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let tx3 = create_test_transaction(&balance_service, child_id, "2025-01-20T10:00:00-05:00", "Third", 50.0, 130.0).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        println!("🧪 TEST: Initial balances - tx1: {}, tx2: {}, tx3: {}", tx1.balance, tx2.balance, tx3.balance);
        
        // Step 2: Verify initial balances are correct
        let initial_errors = balance_service.validate_all_balances(child_id).await.unwrap();
        assert!(initial_errors.is_empty(), "Initial balances should be correct: {:?}", initial_errors);

        // Step 3: Insert a backdated transaction between tx1 and tx2
        let backdated_tx = create_test_transaction(&balance_service, child_id, "2025-01-12T10:00:00-05:00", "Backdated", 25.0, 125.0).await;
        println!("🧪 TEST: Inserted backdated transaction: {}", backdated_tx.balance);
        
        // Step 4: At this point, tx2 and tx3 have wrong balances because of the backdated insertion
        // tx2 should be 105.0 (125.0 - 20.0) but is still 80.0
        // tx3 should be 155.0 (105.0 + 50.0) but is still 130.0
        
        // Step 5: Recalculate balances from the backdated transaction date
        println!("🧪 TEST: Recalculating balances from backdated transaction date");
        let updated_count = balance_service.recalculate_balances_from_date(child_id, "2025-01-12T10:00:00-05:00").await.unwrap();
        
        // Should update 3 transactions: backdated + 2 subsequent
        assert_eq!(updated_count, 3, "Should have updated 3 transactions (backdated + 2 subsequent)");

        // Step 6: Validate that all balances are now correct
        let final_errors = balance_service.validate_all_balances(child_id).await.unwrap();
        assert!(final_errors.is_empty(), "Final balance validation should pass: {:?}", final_errors);
        
        println!("🧪 TEST: Balance recalculation test passed!");
    }

    #[tokio::test]
    async fn test_requires_balance_recalculation() {
        let service = create_test_service().await;
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_result.child.id;

        // Create a transaction after our test date
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Future transaction", 100.0, 100.0).await;

        // Check if inserting at an earlier date requires recalculation
        let requires_recalc = service.requires_balance_recalculation(child_id, "2025-01-15T10:00:00-05:00").await.unwrap();
        assert!(requires_recalc);

        // Check if inserting after the last transaction doesn't require recalculation
        let no_recalc_needed = service.requires_balance_recalculation(child_id, "2025-01-25T10:00:00-05:00").await.unwrap();
        assert!(!no_recalc_needed);
    }

    #[tokio::test]
    async fn test_validate_all_balances_correct() {
        let service = create_test_service().await;
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_result.child.id;

        // Create transactions with correct balances
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0).await;
        
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        create_test_transaction(&service, child_id, "2025-01-15T10:00:00-05:00", "Second", -30.0, 70.0).await;
        
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Third", 20.0, 90.0).await;

        let errors = service.validate_all_balances(child_id).await.unwrap();
        assert!(errors.is_empty());
    }

    #[tokio::test]
    async fn test_validate_all_balances_incorrect() {
        let service = create_test_service().await;
        
        // Create a child first
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = crate::backend::domain::child_service::ChildService::new(db);
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_result.child.id;

        // Create transactions with intentionally incorrect balances
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0).await;
        
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        create_test_transaction(&service, child_id, "2025-01-15T10:00:00-05:00", "Second", -30.0, 75.0).await; // Should be 70.0
        
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Third", 20.0, 85.0).await; // Should be 90.0

        let errors = service.validate_all_balances(child_id).await.unwrap();
        assert_eq!(errors.len(), 2); // Two incorrect balances
    }
} 