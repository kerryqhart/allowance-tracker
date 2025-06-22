use anyhow::Result;
use async_trait::async_trait;
use csv::{Reader, Writer};
use log::{info, warn, error};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use shared::Transaction;
use super::connection::CsvConnection;
use super::child_repository::ChildRepository;
use crate::backend::storage::ChildStorage;

/// CSV-based transaction repository
#[derive(Clone)]
pub struct TransactionRepository {
    connection: CsvConnection,
    child_repository: ChildRepository,
}

impl TransactionRepository {
    /// Create a new CSV transaction repository
    pub fn new(connection: CsvConnection) -> Self {
        let child_repository = ChildRepository::new(connection.clone());
        Self { 
            connection,
            child_repository,
        }
    }
    
    /// Read all transactions for a child from their CSV file
    async fn read_transactions(&self, child_name: &str) -> Result<Vec<Transaction>> {
        self.connection.ensure_transactions_file_exists(child_name)?;
        
        let file_path = self.connection.get_transactions_file_path(child_name);
        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);
        let mut csv_reader = Reader::from_reader(reader);
        
        let mut transactions = Vec::new();
        
        for result in csv_reader.records() {
            let record = result?;
            
            // Parse CSV record into Transaction
            let transaction = Transaction {
                id: record.get(0).unwrap_or("").to_string(),
                child_id: record.get(1).unwrap_or("").to_string(),
                date: record.get(2).unwrap_or("").to_string(),
                description: record.get(3).unwrap_or("").to_string(),
                amount: record.get(4).unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                balance: record.get(5).unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                transaction_type: if record.get(4).unwrap_or("0").parse::<f64>().unwrap_or(0.0) >= 0.0 { 
                    shared::TransactionType::Income 
                } else { 
                    shared::TransactionType::Expense 
                },
            };
            
            transactions.push(transaction);
        }
        
        Ok(transactions)
    }
    
    /// Write all transactions for a child to their CSV file
    async fn write_transactions(&self, child_name: &str, transactions: &[Transaction]) -> Result<()> {
        let file_path = self.connection.get_transactions_file_path(child_name);
        
        // Create a temporary file for atomic write
        let temp_path = file_path.with_extension("tmp");
        
        {
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temp_path)?;
            
            let writer = BufWriter::new(file);
            let mut csv_writer = Writer::from_writer(writer);
            
            // Write header
            csv_writer.write_record(&["id", "child_id", "date", "description", "amount", "balance"])?;
            
            // Write transactions
            for transaction in transactions {
                csv_writer.write_record(&[
                    &transaction.id,
                    &transaction.child_id,
                    &transaction.date,
                    &transaction.description,
                    &transaction.amount.to_string(),
                    &transaction.balance.to_string(),
                ])?;
            }
            
            csv_writer.flush()?;
        }
        
        // Atomic move from temp to final file
        std::fs::rename(&temp_path, &file_path)?;
        
        Ok(())
    }
    
    /// Helper method to get child directory name from child ID
    /// This looks up the actual child and generates a safe directory name
    async fn get_child_directory_name(&self, child_id: &str) -> Result<String> {
        // Look up the child by ID to get their actual name
        match self.child_repository.get_child(child_id).await? {
            Some(child) => {
                // Use the same safe directory name generation as the child repository
                Ok(ChildRepository::generate_safe_directory_name(&child.name))
            }
            None => {
                // Child not found - this shouldn't happen in normal operation
                // Return a fallback, but log a warning
                warn!("Child not found for ID: {}. Using fallback directory name.", child_id);
                Ok(format!("unknown_child_{}", 
                    child_id.chars()
                        .filter(|c| c.is_alphanumeric())
                        .take(10)
                        .collect::<String>()
                        .to_lowercase()
                ))
            }
        }
    }

    /// Normalize a transaction date to RFC 3339 format with conflict resolution
    /// 
    /// This ensures all dates are stored in full RFC 3339 format and handles same-day conflicts
    /// by incrementing the time component by 1-second intervals.
    fn normalize_transaction_date(&self, date: &str, existing_transactions: &[Transaction]) -> Result<String> {
        use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
        
        info!("🕐 Normalizing transaction date: '{}'", date);
        
        // If already in RFC 3339 format, check for conflicts and increment if needed
        if let Ok(_dt) = DateTime::parse_from_rfc3339(date) {
            info!("📅 Date is already RFC 3339, checking for conflicts...");
            return self.resolve_timestamp_conflict(date, existing_transactions);
        }
        
        // If date-only format (YYYY-MM-DD), convert to RFC 3339
        if let Ok(naive_date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            info!("📅 Converting date-only format to RFC 3339...");
            // Start at beginning of day in Eastern Time
            let naive_datetime = naive_date.and_hms_opt(0, 0, 0).unwrap();
            let eastern_offset = FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)
            
            if let Some(dt) = eastern_offset.from_local_datetime(&naive_datetime).single() {
                // Use the same format as successful transactions: -0500 instead of -05:00
                let original_timestamp = dt.format("%Y-%m-%dT%H:%M:%S%z").to_string();
                info!("🕐 Original chrono timestamp: '{}'", original_timestamp);
                
                let mut base_timestamp = original_timestamp;
                // Only remove colon from timezone offset (find colon after + or -)
                if let Some(tz_start) = base_timestamp.rfind('+').or_else(|| base_timestamp.rfind('-')) {
                    info!("🌍 Found timezone start at position: {}", tz_start);
                    if let Some(colon_pos) = base_timestamp[tz_start..].find(':') {
                        info!("🌍 Found colon in timezone at relative position: {}", colon_pos);
                        base_timestamp.remove(tz_start + colon_pos);
                        info!("🌍 Removed colon, result: '{}'", base_timestamp);
                    }
                }
                info!("Generated base timestamp for date-only '{}': '{}'", date, base_timestamp);
                return self.resolve_timestamp_conflict(&base_timestamp, existing_transactions);
            }
        }
        
        // If we can't parse the date, return it as-is (fallback)
        warn!("Could not parse date '{}', storing as-is", date);
        Ok(date.to_string())
    }
    
    /// Resolve timestamp conflicts by incrementing seconds until we find a unique timestamp
    fn resolve_timestamp_conflict(&self, base_timestamp: &str, existing_transactions: &[Transaction]) -> Result<String> {
        use chrono::{DateTime, Duration};
        
        info!("🔄 Resolving timestamp conflict for: '{}'", base_timestamp);
        
        // Ensure the base timestamp has the colon for parsing
        let parseable_timestamp = if base_timestamp.contains("T") && base_timestamp.len() > 5 {
            let mut temp = base_timestamp.to_string();
            // Add colon back to timezone if missing (e.g., -0500 -> -05:00)
            if let Some(tz_start) = temp.rfind('+').or_else(|| temp.rfind('-')) {
                let tz_part = &temp[tz_start..];
                // Check if timezone part lacks colon (e.g., "-0500" instead of "-05:00")
                if tz_part.len() == 5 && !tz_part.contains(':') {
                    temp.insert(tz_start + 3, ':'); // Insert colon at position 3 in timezone
                }
            }
            temp
        } else {
            base_timestamp.to_string()
        };
        
        info!("🔧 Parseable timestamp: '{}'", parseable_timestamp);
        let mut current_dt = DateTime::parse_from_rfc3339(&parseable_timestamp)?;
        let mut attempts = 0;
        const MAX_ATTEMPTS: i32 = 86400; // Max 1 day worth of seconds
        
        // Keep incrementing by 1 second until we find a unique timestamp
        while attempts < MAX_ATTEMPTS {
            let mut current_timestamp = current_dt.format("%Y-%m-%dT%H:%M:%S%z").to_string();
            // Only remove colon from timezone offset (find colon after + or -)
            if let Some(tz_start) = current_timestamp.rfind('+').or_else(|| current_timestamp.rfind('-')) {
                if let Some(colon_pos) = current_timestamp[tz_start..].find(':') {
                    current_timestamp.remove(tz_start + colon_pos);
                }
            }
            
            info!("Checking timestamp conflict for: '{}'", current_timestamp);
            
            // Check if this timestamp already exists
            let conflict_exists = existing_transactions.iter().any(|tx| tx.date == current_timestamp);
            
            if !conflict_exists {
                if attempts > 0 {
                    info!("Resolved timestamp conflict: '{}' -> '{}' (incremented {} seconds)", 
                          base_timestamp, current_timestamp, attempts);
                }
                return Ok(current_timestamp);
            }
            
            // Increment by 1 second and try again
            current_dt = current_dt + Duration::seconds(1);
            attempts += 1;
        }
        
        // If we somehow exhausted all attempts, return the original with a warning
        warn!("Could not resolve timestamp conflict for '{}' after {} attempts", base_timestamp, MAX_ATTEMPTS);
        Ok(base_timestamp.to_string())
    }
}

impl TransactionRepository {
    /// Read transactions using child_id, extracting child name
    pub async fn read_transactions_by_id(&self, child_id: &str) -> Result<Vec<Transaction>> {
        let child_dir_name = self.get_child_directory_name(child_id).await?;
        self.read_transactions(&child_dir_name).await
    }
    
    /// Write transactions using child_id, extracting child name
    pub async fn write_transactions_by_id(&self, child_id: &str, transactions: &[Transaction]) -> Result<()> {
        let child_dir_name = self.get_child_directory_name(child_id).await?;
        self.write_transactions(&child_dir_name, transactions).await
    }
    
    /// Store transaction with explicit child name (preferred method)
    pub async fn store_transaction_with_child_name(&self, transaction: &Transaction, child_name: &str) -> Result<()> {
        info!("Storing transaction in CSV for child '{}': {}", child_name, transaction.id);
        
        // Read existing transactions using child name
        let mut transactions = self.read_transactions(child_name).await?;
        
        // Add new transaction
        transactions.push(transaction.clone());
        
        // Sort by date to maintain chronological order
        transactions.sort_by(|a, b| a.date.cmp(&b.date));
        
        // Write back to file using child name
        self.write_transactions(child_name, &transactions).await?;
        
        info!("Successfully stored transaction for child '{}': {}", child_name, transaction.id);
        Ok(())
    }
    
    /// Helper method to get all child IDs
    async fn get_all_child_ids(&self) -> Result<Vec<String>> {
        // Get all children from the child repository
        let children = self.child_repository.list_children().await?;
        let child_ids: Vec<String> = children.into_iter().map(|child| child.id).collect();
        Ok(child_ids)
    }
}

#[async_trait]
impl crate::backend::storage::TransactionStorage for TransactionRepository {
    async fn store_transaction(&self, transaction: &Transaction) -> Result<()> {
        info!("Storing transaction in CSV: {}", transaction.id);
        
        // Read existing transactions
        let mut transactions = self.read_transactions_by_id(&transaction.child_id).await?;
        
        // Normalize the transaction date to RFC 3339 format with conflict resolution
        let mut normalized_transaction = transaction.clone();
        normalized_transaction.date = self.normalize_transaction_date(&transaction.date, &transactions)?;
        
        info!("Normalized transaction date from '{}' to '{}'", transaction.date, normalized_transaction.date);
        
        // Add new transaction with normalized date
        transactions.push(normalized_transaction);
        
        // Sort by date to maintain chronological order (now all RFC 3339, so string sorting works)
        transactions.sort_by(|a, b| a.date.cmp(&b.date));
        
        // Write back to file
        self.write_transactions_by_id(&transaction.child_id, &transactions).await?;
        
        info!("Successfully stored transaction: {}", transaction.id);
        Ok(())
    }
    
    async fn get_transaction(&self, child_id: &str, transaction_id: &str) -> Result<Option<Transaction>> {
        let transactions = self.read_transactions_by_id(child_id).await?;
        
        Ok(transactions.into_iter().find(|t| t.id == transaction_id))
    }
    
    async fn list_transactions(&self, child_id: &str, limit: Option<u32>, after: Option<String>) -> Result<Vec<Transaction>> {
        let mut transactions = self.read_transactions_by_id(child_id).await?;
        
        // Sort by date descending (most recent first)
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        
        // Apply pagination
        if let Some(after_id) = after {
            if let Some(after_index) = transactions.iter().position(|t| t.id == after_id) {
                transactions = transactions.into_iter().skip(after_index + 1).collect();
            }
        }
        
        // Apply limit
        if let Some(limit) = limit {
            transactions.truncate(limit as usize);
        }
        
        Ok(transactions)
    }
    
    async fn list_transactions_chronological(&self, child_id: &str, start_date: Option<String>, end_date: Option<String>) -> Result<Vec<Transaction>> {
        let mut transactions = self.read_transactions_by_id(child_id).await?;
        
        // Filter by date range if provided
        if let Some(start) = start_date {
            transactions.retain(|t| t.date >= start);
        }
        
        if let Some(end) = end_date {
            transactions.retain(|t| t.date <= end);
        }
        
        // Sort chronologically (ascending)
        transactions.sort_by(|a, b| a.date.cmp(&b.date));
        
        Ok(transactions)
    }
    
    async fn update_transaction(&self, transaction: &Transaction) -> Result<()> {
        info!("Updating transaction in CSV: {}", transaction.id);
        
        let mut transactions = self.read_transactions_by_id(&transaction.child_id).await?;
        
        // Find and update the transaction
        if let Some(existing) = transactions.iter_mut().find(|t| t.id == transaction.id) {
            *existing = transaction.clone();
            
            // Sort by date to maintain chronological order
            transactions.sort_by(|a, b| a.date.cmp(&b.date));
            
            // Write back to file
            self.write_transactions_by_id(&transaction.child_id, &transactions).await?;
            
            info!("Successfully updated transaction: {}", transaction.id);
        } else {
            warn!("Transaction not found for update: {}", transaction.id);
        }
        
        Ok(())
    }
    
    async fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool> {
        info!("Deleting transaction from CSV: {}", transaction_id);
        
        let mut transactions = self.read_transactions_by_id(child_id).await?;
        let initial_len = transactions.len();
        
        // Remove the transaction
        transactions.retain(|t| t.id != transaction_id);
        
        if transactions.len() < initial_len {
            // Write back to file
            self.write_transactions_by_id(child_id, &transactions).await?;
            info!("Successfully deleted transaction: {}", transaction_id);
            Ok(true)
        } else {
            warn!("Transaction not found for deletion: {}", transaction_id);
            Ok(false)
        }
    }
    
    async fn delete_transactions(&self, child_id: &str, transaction_ids: &[String]) -> Result<u32> {
        info!("Deleting {} transactions from CSV", transaction_ids.len());
        
        let mut transactions = self.read_transactions_by_id(child_id).await?;
        let initial_len = transactions.len();
        
        // Remove the transactions
        transactions.retain(|t| !transaction_ids.contains(&t.id));
        
        let deleted_count = (initial_len - transactions.len()) as u32;
        
        if deleted_count > 0 {
            // Write back to file
            self.write_transactions_by_id(child_id, &transactions).await?;
            info!("Successfully deleted {} transactions", deleted_count);
        }
        
        Ok(deleted_count)
    }
    
    /// Get the most recent transaction for a specific child (trait implementation)
    async fn get_latest_transaction(&self, child_id: &str) -> Result<Option<Transaction>> {
        let mut transactions = self.read_transactions_by_id(child_id).await?;
        
        // Sort by date descending (most recent first)
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        
        Ok(transactions.into_iter().next())
    }
    
    /// Get all transactions after a specific date (trait implementation)
    async fn get_transactions_after_date(&self, child_id: &str, date: &str) -> Result<Vec<Transaction>> {
        let mut transactions = self.read_transactions_by_id(child_id).await?;
        
        // Filter by date
        transactions.retain(|t| t.date.as_str() >= date);
        
        // Sort chronologically (ascending)
        transactions.sort_by(|a, b| a.date.cmp(&b.date));
        
        Ok(transactions)
    }
    
    /// Get the most recent transaction before a specific date (trait implementation)
    async fn get_latest_transaction_before_date(&self, child_id: &str, date: &str) -> Result<Option<Transaction>> {
        let mut transactions = self.read_transactions_by_id(child_id).await?;
        
        // Filter transactions before the specified date
        transactions.retain(|t| t.date.as_str() < date);
        
        // Sort by date descending (most recent first)
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        
        Ok(transactions.into_iter().next())
    }
    
    /// Update the balance of a specific transaction (trait implementation)
    async fn update_transaction_balance(&self, _transaction_id: &str, _new_balance: f64) -> Result<()> {
        // CSV storage doesn't efficiently support updating individual transaction balances
        // This would require reading the entire file, updating one record, and rewriting the file
        // For now, return an error suggesting bulk updates or individual file operations
        Err(anyhow::anyhow!("CSV storage requires child_id for transaction updates. Consider using update_transaction for each item individually."))
    }
    
    /// Update multiple transaction balances atomically (trait implementation)
    async fn update_transaction_balances(&self, updates: &[(String, f64)]) -> Result<()> {
        info!("🔄 CSV update_transaction_balances called with {} updates", updates.len());
        
        // Log each update for debugging
        for (i, (transaction_id, new_balance)) in updates.iter().enumerate() {
            info!("  Update {}: transaction_id='{}', new_balance={:.2}", i + 1, transaction_id, new_balance);
        }
        
        // The challenge with CSV storage is that we need to know which child each transaction belongs to
        // But the trait method only gives us transaction IDs, not child IDs
        
        // We need to implement this properly by iterating through each update
        for (transaction_id, new_balance) in updates {
            info!("🔍 Processing balance update for transaction: {}", transaction_id);
            
            // We need to find which child this transaction belongs to
            // This is inefficient but necessary for CSV storage
            let child_ids = self.get_all_child_ids().await?;
            info!("🔍 Found {} child IDs to search through", child_ids.len());
            
            let mut transaction_found = false;
            
            for child_id in &child_ids {
                info!("🔍 Searching for transaction {} in child {}", transaction_id, child_id);
                
                match self.get_transaction(child_id, transaction_id).await {
                    Ok(Some(mut transaction)) => {
                        info!("✅ Found transaction {} in child {}, updating balance from {:.2} to {:.2}", 
                              transaction_id, child_id, transaction.balance, new_balance);
                        
                        transaction.balance = *new_balance;
                        
                        match self.update_transaction(&transaction).await {
                            Ok(()) => {
                                info!("✅ Successfully updated balance for transaction {}", transaction_id);
                                transaction_found = true;
                                break;
                            }
                            Err(e) => {
                                error!("❌ Failed to update transaction {}: {}", transaction_id, e);
                                return Err(e);
                            }
                        }
                    }
                    Ok(None) => {
                        // Transaction not found in this child, continue searching
                        continue;
                    }
                    Err(e) => {
                        error!("❌ Error searching for transaction {} in child {}: {}", transaction_id, child_id, e);
                        return Err(e);
                    }
                }
            }
            
            if !transaction_found {
                let error_msg = format!("Transaction {} not found in any child", transaction_id);
                error!("❌ {}", error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        }
        
        info!("✅ Successfully updated {} transaction balances", updates.len());
        Ok(())
    }
    
    /// Check if transactions exist by their IDs for a specific child (trait implementation)
    async fn check_transactions_exist(&self, child_id: &str, transaction_ids: &[String]) -> Result<Vec<String>> {
        let transactions = self.read_transactions_by_id(child_id).await?;
        
        let existing_ids: Vec<String> = transactions
            .iter()
            .filter(|t| transaction_ids.contains(&t.id))
            .map(|t| t.id.clone())
            .collect();
        
        Ok(existing_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::TransactionStorage;
    
    async fn setup_test_repo() -> (TransactionRepository, impl Fn() -> Result<()>) {
        let connection = CsvConnection::new_for_testing().await.unwrap();
        let cleanup_dir = connection.base_directory().to_path_buf();
        let repo = TransactionRepository::new(connection);
        
        let cleanup = move || {
            if cleanup_dir.exists() {
                std::fs::remove_dir_all(&cleanup_dir)?;
            }
            Ok(())
        };
        
        (repo, cleanup)
    }
    
    #[tokio::test]
    async fn test_store_and_retrieve_transaction() {
        let (repo, cleanup) = setup_test_repo().await;
        
        let transaction = Transaction {
            id: "test_tx_001".to_string(),
            child_id: "test_child".to_string(),
            date: "2024-01-15T10:30:00Z".to_string(),
            description: "Test transaction".to_string(),
            amount: 25.50,
            balance: 25.50,
            transaction_type: shared::TransactionType::Income,
        };
        
        // Store transaction
        repo.store_transaction(&transaction).await.unwrap();
        
        // Retrieve transaction
        let retrieved = repo.get_transaction("test_child", "test_tx_001").await.unwrap();
        assert!(retrieved.is_some());
        
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "test_tx_001");
        assert_eq!(retrieved.description, "Test transaction");
        assert_eq!(retrieved.amount, 25.50);
        
        cleanup().unwrap();
    }
    
    #[tokio::test]
    async fn test_list_transactions_with_pagination() {
        let (repo, cleanup) = setup_test_repo().await;
        
        // Store multiple transactions
        for i in 1..=5 {
            let transaction = Transaction {
                id: format!("tx_{:03}", i),
                child_id: "test_child".to_string(),
                date: format!("2024-01-{:02}T10:30:00Z", i + 10),
                description: format!("Transaction {}", i),
                amount: i as f64 * 10.0,
                balance: (i * (i + 1) / 2) as f64 * 10.0, // Cumulative sum
                transaction_type: shared::TransactionType::Income,
            };
            
            repo.store_transaction(&transaction).await.unwrap();
        }
        
        // Test listing with limit
        let transactions = repo.list_transactions("test_child", Some(3), None).await.unwrap();
        assert_eq!(transactions.len(), 3);
        
        // Should be ordered by date descending (most recent first)
        assert_eq!(transactions[0].id, "tx_005");
        assert_eq!(transactions[1].id, "tx_004");
        assert_eq!(transactions[2].id, "tx_003");
        
        cleanup().unwrap();
    }
    
    #[tokio::test]
    async fn test_delete_transaction() {
        let (repo, cleanup) = setup_test_repo().await;
        
        let transaction = Transaction {
            id: "to_delete".to_string(),
            child_id: "test_child".to_string(),
            date: "2024-01-15T10:30:00Z".to_string(),
            description: "Will be deleted".to_string(),
            amount: 100.0,
            balance: 100.0,
            transaction_type: shared::TransactionType::Income,
        };
        
        // Store transaction
        repo.store_transaction(&transaction).await.unwrap();
        
        // Verify it exists
        let retrieved = repo.get_transaction("test_child", "to_delete").await.unwrap();
        assert!(retrieved.is_some());
        
        // Delete transaction
        let deleted = repo.delete_transaction("test_child", "to_delete").await.unwrap();
        assert!(deleted);
        
        // Verify it's gone
        let retrieved = repo.get_transaction("test_child", "to_delete").await.unwrap();
        assert!(retrieved.is_none());
        
        cleanup().unwrap();
    }
} 