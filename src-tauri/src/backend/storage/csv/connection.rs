use anyhow::Result;
use std::path::{Path, PathBuf};
use std::fs;
use chrono::Utc;
use crate::backend::storage::traits::Connection;

/// CsvConnection manages file paths and ensures CSV files exist for each child
#[derive(Clone)]
pub struct CsvConnection {
    base_directory: PathBuf,
}

impl CsvConnection {
    /// Create a new CSV connection with a base directory
    pub fn new<P: AsRef<Path>>(base_directory: P) -> Result<Self> {
        let base_path = base_directory.as_ref().to_path_buf();
        
        // Create the base directory if it doesn't exist
        if !base_path.exists() {
            fs::create_dir_all(&base_path)?;
        }
        
        Ok(Self {
            base_directory: base_path,
        })
    }
    
    /// Create a new CSV connection in the default data directory
    pub fn new_default() -> Result<Self> {
        // Use a data directory in the app's working directory
        let data_dir = PathBuf::from("data");
        Self::new(data_dir)
    }
    
    /// Create a new CSV connection for testing with a unique directory
    pub async fn new_for_testing() -> Result<Self> {
        // Create a unique test directory using timestamp
        let timestamp = Utc::now().timestamp_millis();
        let test_dir = PathBuf::from(format!("test_data_{}", timestamp));
        Self::new(test_dir)
    }
    
    /// Get the file path for a child's transactions
    pub fn get_transactions_file_path(&self, child_id: &str) -> PathBuf {
        self.base_directory.join(format!("{}_transactions.csv", child_id))
    }
    
    /// Ensure a CSV file exists with proper header for the child
    pub fn ensure_transactions_file_exists(&self, child_id: &str) -> Result<()> {
        let file_path = self.get_transactions_file_path(child_id);
        
        if !file_path.exists() {
            // Create the file with CSV header
            let header = "id,child_id,date,description,amount,balance\n";
            fs::write(&file_path, header)?;
        }
        
        Ok(())
    }
    
    /// Get the base directory path
    pub fn base_directory(&self) -> &Path {
        &self.base_directory
    }
    
    /// Clean up test data (useful for tests)
    #[cfg(test)]
    pub fn cleanup(&self) -> Result<()> {
        if self.base_directory.exists() {
            fs::remove_dir_all(&self.base_directory)?;
        }
        Ok(())
    }
}

impl Connection for CsvConnection {
    type TransactionRepository = super::transaction_repository::TransactionRepository;
    
    fn create_transaction_repository(&self) -> Self::TransactionRepository {
        super::transaction_repository::TransactionRepository::new(self.clone())
    }
} 