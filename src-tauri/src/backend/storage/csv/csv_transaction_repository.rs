use anyhow::Result;
use async_trait::async_trait;
use csv::{Reader, Writer};
use log::{info, warn};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use crate::backend::{
    domain::models::transaction::{
        Transaction as DomainTransaction, TransactionType as DomainTransactionType,
    },
    storage::traits::TransactionRepository,
};
use super::connection::CsvConnection;

/// CSV-based transaction repository
#[derive(Clone)]
pub struct CsvTransactionRepository {
    connection: CsvConnection,
}

impl CsvTransactionRepository {
    /// Create a new CSV transaction repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { connection }
    }
    
    /// Read all transactions for a child from their CSV file
    async fn read_transactions(&self, child_id: &str) -> Result<Vec<DomainTransaction>> {
        self.connection.ensure_transactions_file_exists(child_id)?;
        
        let file_path = self.connection.get_transactions_file_path(child_id);
        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);
        let mut csv_reader = Reader::from_reader(reader);
        
        let mut transactions = Vec::new();
        
        for result in csv_reader.records() {
            let record = result?;
            
            // Parse CSV record into Transaction
            let transaction = DomainTransaction {
                id: record.get(0).unwrap_or("").to_string(),
                child_id: record.get(1).unwrap_or("").to_string(),
                date: record.get(2).unwrap_or("").to_string(),
                description: record.get(3).unwrap_or("").to_string(),
                amount: record.get(4).unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                balance: record.get(5).unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                transaction_type: if record.get(4).unwrap_or("0").parse::<f64>().unwrap_or(0.0) >= 0.0 { 
                    DomainTransactionType::Income 
                } else { 
                    DomainTransactionType::Expense 
                },
            };
            
            transactions.push(transaction);
        }
        
        Ok(transactions)
    }
    
    /// Write all transactions for a child to their CSV file
    async fn write_transactions(&self, child_id: &str, transactions: &[DomainTransaction]) -> Result<()> {
        let file_path = self.connection.get_transactions_file_path(child_id);
        
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

    /// Normalize a transaction date to RFC 3339 format with conflict resolution
    /// 
    /// This ensures all dates are stored in full RFC 3339 format and handles same-day conflicts
    /// by incrementing the time component by 1-second intervals.
    fn normalize_transaction_date(&self, date: &str, existing_transactions: &[DomainTransaction]) -> Result<String> {
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
    fn resolve_timestamp_conflict(&self, base_timestamp: &str, existing_transactions: &[DomainTransaction]) -> Result<String> {
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

#[async_trait]
impl TransactionRepository for CsvTransactionRepository {
    async fn store_transaction(&self, transaction: &DomainTransaction) -> Result<()> {
        let mut transactions = self.read_transactions(&transaction.child_id).await?;
        transactions.push(transaction.clone());
        self.write_transactions(&transaction.child_id, &transactions).await
    }

    async fn get_transaction(
        &self,
        child_id: &str,
        transaction_id: &str,
    ) -> Result<Option<DomainTransaction>> {
        let transactions = self.read_transactions(child_id).await?;
        Ok(transactions.into_iter().find(|t| t.id == transaction_id))
    }

    async fn list_transactions(
        &self,
        child_id: &str,
        limit: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<DomainTransaction>> {
        let mut transactions = self.read_transactions(child_id).await?;
        transactions.sort_by(|a, b| b.date.cmp(&a.date));

        let start_index = after
            .and_then(|id| transactions.iter().position(|t| t.id == id).map(|p| p + 1))
            .unwrap_or(0);

        let end_index = limit
            .map(|l| start_index + l as usize)
            .unwrap_or(transactions.len());

        Ok(transactions.into_iter().skip(start_index).take(end_index - start_index).collect())
    }

    async fn get_transactions_after_date(&self, child_id: &str, date: &str) -> Result<Vec<DomainTransaction>> {
        let mut transactions = self.read_transactions(child_id).await?;
        transactions.retain(|t| self.compare_dates(&t.date, date) >= 0);
        Ok(transactions)
    }

    async fn get_transactions_before_date(
        &self,
        child_id: &str,
        date: &str,
    ) -> Result<Vec<DomainTransaction>> {
        let mut transactions = self.read_transactions(child_id).await?;
        transactions.retain(|t| self.compare_dates(&t.date, date) < 0);
        Ok(transactions)
    }

    async fn update_transaction(&self, transaction: &DomainTransaction) -> Result<()> {
        let mut transactions = self.read_transactions(&transaction.child_id).await?;
        if let Some(index) = transactions.iter().position(|t| t.id == transaction.id) {
            transactions[index] = transaction.clone();
            self.write_transactions(&transaction.child_id, &transactions).await?;
        }
        Ok(())
    }

    async fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool> {
        let mut transactions = self.read_transactions(child_id).await?;
        let initial_len = transactions.len();
        transactions.retain(|t| t.id != transaction_id);
        let was_deleted = transactions.len() < initial_len;
        if was_deleted {
            self.write_transactions(child_id, &transactions).await?;
        }
        Ok(was_deleted)
    }

    async fn delete_transactions(&self, child_id: &str, transaction_ids: &[String]) -> Result<u32> {
        let mut transactions = self.read_transactions(child_id).await?;
        let initial_len = transactions.len();
        transactions.retain(|t| !transaction_ids.contains(&t.id));
        let deleted_count = (initial_len - transactions.len()) as u32;
        if deleted_count > 0 {
            self.write_transactions(child_id, &transactions).await?;
        }
        Ok(deleted_count)
    }

    async fn get_latest_transaction(&self, child_id: &str) -> Result<Option<DomainTransaction>> {
        let mut transactions = self.read_transactions(child_id).await?;
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(transactions.into_iter().next())
    }

    async fn get_transactions_since(
        &self,
        child_id: &str,
        date: &str,
    ) -> Result<Vec<DomainTransaction>> {
        let mut transactions = self.read_transactions(child_id).await?;
        transactions.retain(|t| self.compare_dates(&t.date, date) >= 0);
        transactions.sort_by(|a, b| a.date.cmp(&b.date));
        Ok(transactions)
    }

    async fn get_latest_transaction_before_date(
        &self,
        child_id: &str,
        date: &str,
    ) -> Result<Option<DomainTransaction>> {
        let mut transactions = self.read_transactions(child_id).await?;
        transactions.retain(|t| self.compare_dates(&t.date, date) < 0);
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(transactions.into_iter().next())
    }

    async fn update_transaction_balance(
        &self,
        transaction_id: &str,
        new_balance: f64,
    ) -> Result<()> {
        // This is inefficient but necessary for CSV. Find the right child first.
        let child_id = self.find_child_for_transaction(transaction_id).await?;
        if let Some(child_id) = child_id {
            let mut transactions = self.read_transactions(&child_id).await?;
            if let Some(transaction) = transactions.iter_mut().find(|t| t.id == transaction_id) {
                transaction.balance = new_balance;
                self.write_transactions(&child_id, &transactions).await?;
            }
        }
        Ok(())
    }

    async fn update_transaction_balances(&self, updates: &[(String, f64)]) -> Result<()> {
        let mut child_transactions: std::collections::HashMap<String, Vec<DomainTransaction>> = std::collections::HashMap::new();

        for (transaction_id, _) in updates {
            if let Some(child_id) = self.find_child_for_transaction(transaction_id).await? {
                if !child_transactions.contains_key(&child_id) {
                    child_transactions.insert(child_id.clone(), self.read_transactions(&child_id).await?);
                }
            }
        }

        for (child_id, transactions) in child_transactions.iter_mut() {
            for (transaction_id, new_balance) in updates {
                if let Some(transaction) = transactions.iter_mut().find(|t| t.id == transaction_id) {
                    transaction.balance = *new_balance;
                }
            }
            self.write_transactions(child_id, transactions).await?;
        }

        Ok(())
    }

    async fn check_transactions_exist(
        &self,
        child_id: &str,
        transaction_ids: &[String],
    ) -> Result<Vec<String>> {
        let transactions = self.read_transactions(child_id).await?;
        let found_ids: Vec<String> = transactions
            .iter()
            .filter_map(|t| {
                if transaction_ids.contains(&t.id) {
                    Some(t.id.clone())
                } else {
                    None
                }
            })
            .collect();
        Ok(found_ids)
    }
}

impl CsvTransactionRepository {
    /// Helper to find which child a transaction belongs to. Inefficient.
    async fn find_child_for_transaction(&self, transaction_id: &str) -> Result<Option<String>> {
        // This is a placeholder for a more robust lookup.
        // For now, we assume transaction IDs might contain the child ID, or we need to search.
        // This is very inefficient and should be avoided in a real DB.
        warn!("Performing inefficient lookup for transaction's child: {}", transaction_id);
        // This is a simplified and incorrect assumption for the CSV implementation.
        // A proper implementation would require a different storage design.
        Ok(None)
    }
    
    /// Compare two date strings properly handling different timezones
    fn compare_dates(&self, date1: &str, date2: &str) -> i32 {
        use chrono::{DateTime, FixedOffset};
        
        // Try to parse both dates as RFC3339 datetime objects
        match (DateTime::parse_from_rfc3339(date1), DateTime::parse_from_rfc3339(date2)) {
            (Ok(dt1), Ok(dt2)) => {
                // Compare as datetime objects (automatically handles timezone conversion)
                if dt1 < dt2 { -1 } else if dt1 > dt2 { 1 } else { 0 }
            }
            _ => {
                // Fallback to string comparison if parsing fails
                date1.cmp(date2) as i32
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::csv::connection::CsvConnection;
    use tempfile::tempdir;

    async fn setup_test_repo() -> (CsvTransactionRepository, impl Fn() -> Result<()>) {
        let temp_dir = tempdir().unwrap();
        let connection = CsvConnection::new(temp_dir.path().to_path_buf()).unwrap();
        let repo = CsvTransactionRepository::new(connection);
        let cleanup = move || -> Result<()> {
            temp_dir.close()?;
            Ok(())
        };
        (repo, cleanup)
    }

    #[tokio::test]
    async fn test_store_and_retrieve_transaction() {
        let (repo, _cleanup) = setup_test_repo().await;
        let transaction = DomainTransaction {
            id: "tx1".to_string(),
            child_id: "child1".to_string(),
            date: "2024-01-01T12:00:00Z".to_string(),
            description: "Test".to_string(),
            amount: 10.0,
            balance: 10.0,
            transaction_type: DomainTransactionType::Income,
        };
        repo.store_transaction(&transaction).await.unwrap();
        let retrieved = repo.get_transaction("child1", "tx1").await.unwrap();
        assert_eq!(retrieved.as_ref().map(|t| &t.id), Some(&transaction.id));
    }

    #[tokio::test]
    async fn test_list_transactions_with_pagination() {
        let (repo, _cleanup) = setup_test_repo().await;
        for i in 0..10 {
            let transaction = DomainTransaction {
                id: format!("tx{}", i),
                child_id: "child1".to_string(),
                date: format!("2024-01-{}T12:00:00Z", 10-i),
                description: "Test".to_string(),
                amount: 10.0,
                balance: 10.0,
                transaction_type: DomainTransactionType::Income,
            };
            repo.store_transaction(&transaction).await.unwrap();
        }

        let page1 = repo.list_transactions("child1", Some(5), None).await.unwrap();
        assert_eq!(page1.len(), 5);
        assert_eq!(page1[0].id, "tx9");

        let last_id = page1.last().unwrap().id.clone();
        let page2 = repo.list_transactions("child1", Some(5), Some(last_id)).await.unwrap();
        assert_eq!(page2.len(), 5);
        assert_eq!(page2[0].id, "tx4");
    }

    #[tokio::test]
    async fn test_delete_transaction() {
        let (repo, _cleanup) = setup_test_repo().await;
        let transaction = DomainTransaction {
            id: "tx1".to_string(),
            child_id: "child1".to_string(),
            date: "2024-01-01T12:00:00Z".to_string(),
            description: "Test".to_string(),
            amount: 10.0,
            balance: 10.0,
            transaction_type: DomainTransactionType::Income,
        };
        repo.store_transaction(&transaction).await.unwrap();
        let was_deleted = repo.delete_transaction("child1", "tx1").await.unwrap();
        assert!(was_deleted);
        let retrieved = repo.get_transaction("child1", "tx1").await.unwrap();
        assert!(retrieved.is_none());
    }
} 