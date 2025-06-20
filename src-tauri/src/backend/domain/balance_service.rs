//! Balance management service for the allowance tracker.
//!
//! This service handles the complex logic of recalculating balances when backdated 
//! transactions are inserted. It ensures that all subsequent transactions have their
//! balances updated correctly to maintain data integrity.

use anyhow::Result;
use log::{info, warn};
use std::sync::Arc;
use crate::backend::storage::{connection::DbConnection, repositories::transaction_repository::TransactionRepository};

/// Service responsible for balance calculations and recalculations
#[derive(Clone)]
pub struct BalanceService {
    transaction_repository: TransactionRepository,
}

impl BalanceService {
    pub fn new(db: Arc<DbConnection>) -> Self {
        let transaction_repository = TransactionRepository::new((*db).clone());
        Self { transaction_repository }
    }

    /// Get the underlying database connection for testing purposes
    pub fn get_db_connection(&self) -> &DbConnection {
        self.transaction_repository.get_db_connection()
    }

    /// Recalculate all balances from a specific date forward
    /// This is called when a backdated transaction is inserted
    /// 
    /// The algorithm:
    /// 1. Get all transactions from the backdated date forward (chronological order)
    /// 2. Calculate the starting balance (balance before the first transaction in our list)
    /// 3. Recalculate each transaction's balance based on the running total
    /// 4. Update all affected transactions in the database atomically
    pub async fn recalculate_balances_from_date(&self, child_id: &str, from_date: &str) -> Result<usize> {
        info!("Starting balance recalculation for child {} from date {}", child_id, from_date);

        // Get all transactions from the specified date forward (chronological order)
        let transactions = self.transaction_repository
            .get_transactions_after_date(child_id, from_date)
            .await?;

        if transactions.is_empty() {
            info!("No transactions found after {}, no balance recalculation needed", from_date);
            return Ok(0);
        }

        info!("Found {} transactions to recalculate", transactions.len());

        // Calculate the starting balance (balance just before our first transaction)
        let starting_balance = self.calculate_starting_balance(child_id, from_date).await?;
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
            .update_transaction_balances(&balance_updates)
            .await?;

        info!("Successfully recalculated {} transaction balances", balance_updates.len());
        Ok(balance_updates.len())
    }

    /// Calculate the starting balance for a recalculation
    /// This is the balance just before the specified date
    async fn calculate_starting_balance(&self, child_id: &str, from_date: &str) -> Result<f64> {
        // Find the most recent transaction before the specified date
        match self.transaction_repository
            .get_latest_transaction_before_date(child_id, from_date)
            .await? 
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
    pub async fn calculate_balance_for_new_transaction(&self, child_id: &str, transaction_date: &str, transaction_amount: f64) -> Result<f64> {
        let starting_balance = self.calculate_starting_balance(child_id, transaction_date).await?;
        let new_balance = starting_balance + transaction_amount;
        
        info!("Calculated balance for new transaction: starting_balance={:.2} + amount={:.2} = {:.2}", 
              starting_balance, transaction_amount, new_balance);
        
        Ok(new_balance)
    }

    /// Check if inserting a transaction at a specific date would require balance recalculation
    /// Returns true if there are any transactions after the specified date
    pub async fn requires_balance_recalculation(&self, child_id: &str, transaction_date: &str) -> Result<bool> {
        let transactions_after = self.transaction_repository
            .get_transactions_after_date(child_id, transaction_date)
            .await?;

        // If there are transactions after this date (excluding exact matches), we need recalculation
        let needs_recalc = transactions_after.iter().any(|tx| tx.date.as_str() > transaction_date);
        
        info!("Balance recalculation needed for date {}: {}", transaction_date, needs_recalc);
        Ok(needs_recalc)
    }

    /// Validate that all balances are correct for a child
    /// This is a diagnostic method to ensure balance integrity
    pub async fn validate_all_balances(&self, child_id: &str) -> Result<Vec<String>> {
        info!("Validating all balances for child {}", child_id);
        
        let transactions = self.transaction_repository
            .get_all_transactions_chronological(child_id)
            .await?;

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
    use crate::backend::storage::DbConnection;
    use shared::{Transaction, TransactionType};
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn create_test_service() -> BalanceService {
        let db = Arc::new(DbConnection::init_test().await.unwrap());
        BalanceService::new(db)
    }

    async fn create_test_transaction(service: &BalanceService, child_id: &str, date: &str, description: &str, amount: f64, balance: f64) -> Transaction {
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
        let child_service = crate::backend::domain::child_service::ChildService::new(
            Arc::new(service.get_db_connection().clone())
        );
        let child_response = child_service.create_child(shared::CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_response.child.id;

        // Create a transaction before our target date
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "Previous transaction", 50.0, 50.0).await;

        let starting_balance = service.calculate_starting_balance(child_id, "2025-01-15T10:00:00-05:00").await.unwrap();
        assert_eq!(starting_balance, 50.0);
    }

    #[tokio::test]
    async fn test_calculate_starting_balance_no_previous_transaction() {
        let service = create_test_service().await;
        
        // Create a child first
        let child_service = crate::backend::domain::child_service::ChildService::new(
            Arc::new(service.get_db_connection().clone())
        );
        let child_response = child_service.create_child(shared::CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_response.child.id;

        let starting_balance = service.calculate_starting_balance(child_id, "2025-01-15T10:00:00-05:00").await.unwrap();
        assert_eq!(starting_balance, 0.0);
    }

    #[tokio::test]
    async fn test_calculate_balance_for_new_transaction() {
        let service = create_test_service().await;
        
        // Create a child first
        let child_service = crate::backend::domain::child_service::ChildService::new(
            Arc::new(service.get_db_connection().clone())
        );
        let child_response = child_service.create_child(shared::CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_response.child.id;

        // Create a previous transaction
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "Previous", 30.0, 30.0).await;

        let new_balance = service.calculate_balance_for_new_transaction(child_id, "2025-01-15T10:00:00-05:00", 20.0).await.unwrap();
        assert_eq!(new_balance, 50.0); // 30 + 20
    }

    #[tokio::test]
    async fn test_recalculate_balances_from_date() {
        let service = create_test_service().await;
        
        // Create a child first
        let child_service = crate::backend::domain::child_service::ChildService::new(
            Arc::new(service.get_db_connection().clone())
        );
        let child_response = child_service.create_child(shared::CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_response.child.id;

        // Create transactions with initially correct balances
        create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "First", 100.0, 100.0).await;
        
        // Small delay to ensure different timestamp for unique transaction ID
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        create_test_transaction(&service, child_id, "2025-01-15T10:00:00-05:00", "Second", -20.0, 80.0).await;
        
        // Small delay to ensure different timestamp for unique transaction ID
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        create_test_transaction(&service, child_id, "2025-01-20T10:00:00-05:00", "Third", 50.0, 130.0).await;

        // Now imagine we inserted a backdated transaction on 2025-01-12 for +25.0
        // This would change the balances of the Second and Third transactions
        
        // Small delay to ensure different timestamp for unique transaction ID
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        // Simulate inserting the backdated transaction (this would normally be done by TransactionService)
        create_test_transaction(&service, child_id, "2025-01-12T10:00:00-05:00", "Backdated", 25.0, 125.0).await;

        // Recalculate balances from the backdated transaction date
        let updated_count = service.recalculate_balances_from_date(child_id, "2025-01-12T10:00:00-05:00").await.unwrap();
        
        // Should have updated 3 transactions (backdated + 2 subsequent)
        assert_eq!(updated_count, 3);

        // Validate that all balances are now correct
        let errors = service.validate_all_balances(child_id).await.unwrap();
        assert!(errors.is_empty(), "Balance validation errors: {:?}", errors);
    }

    #[tokio::test]
    async fn test_requires_balance_recalculation() {
        let service = create_test_service().await;
        
        // Create a child first
        let child_service = crate::backend::domain::child_service::ChildService::new(
            Arc::new(service.get_db_connection().clone())
        );
        let child_response = child_service.create_child(shared::CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_response.child.id;

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
        let child_service = crate::backend::domain::child_service::ChildService::new(
            Arc::new(service.get_db_connection().clone())
        );
        let child_response = child_service.create_child(shared::CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_response.child.id;

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
        let child_service = crate::backend::domain::child_service::ChildService::new(
            Arc::new(service.get_db_connection().clone())
        );
        let child_response = child_service.create_child(shared::CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).await.unwrap();
        let child_id = &child_response.child.id;

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