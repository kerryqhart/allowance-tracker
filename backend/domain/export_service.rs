//! Export service domain logic for the allowance tracker.
//!
//! This module contains all business logic related to exporting transaction data
//! as CSV files, including orchestration of child lookup, transaction retrieval,
//! and file operations. The UI should only handle presentation concerns.

use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{info, error};
use std::fs;


use shared::{ExportDataRequest, ExportDataResponse, ExportToPathRequest, ExportToPathResponse, Transaction, TransactionType};
use crate::backend::domain::child_service::ChildService;
use crate::backend::domain::transaction_service::TransactionService;
use crate::backend::domain::commands::transactions::TransactionListQuery;
use crate::backend::domain::commands::child::GetChildCommand;

use crate::backend::storage::Connection;

// Create TransactionMapper placeholder
struct TransactionMapper;

impl TransactionMapper {
    pub fn to_dto(transaction: crate::backend::domain::models::transaction::Transaction) -> Transaction {
        Transaction {
            id: transaction.id,
            date: transaction.date,
            amount: transaction.amount,
            description: transaction.description,
            transaction_type: match transaction.transaction_type {
                crate::backend::domain::models::transaction::TransactionType::Income => TransactionType::Income,
                crate::backend::domain::models::transaction::TransactionType::Expense => TransactionType::Expense,
                crate::backend::domain::models::transaction::TransactionType::FutureAllowance => TransactionType::FutureAllowance,
            },
            balance: transaction.balance,
            child_id: transaction.child_id,
        }
    }
}

/// Export service that handles all export-related business logic
#[derive(Clone)]
pub struct ExportService {
    // No internal state needed for now
}

impl ExportService {
    /// Create a new ExportService instance
    pub fn new() -> Self {
        Self {}
    }

    /// Export transactions as CSV data with complete orchestration
    /// This method moves the orchestration logic from the REST API layer into the domain layer
    pub fn export_transactions_csv<C: Connection>(
        &self,
        request: ExportDataRequest,
        child_service: &ChildService,
        transaction_service: &TransactionService<C>,
    ) -> Result<ExportDataResponse> {
        info!("📄 EXPORT: Exporting transactions as CSV for child_id: {:?}", request.child_id);

        // Step 1: Determine which child to export for
        let child_id_to_use = if let Some(id) = request.child_id {
            id
        } else {
            let active_child_response = child_service.get_active_child()?;
            match active_child_response.active_child.child {
                Some(child) => {
                    info!("✅ EXPORT: Using active child: {}", child.id);
                    child.id
                }
                None => {
                    error!("❌ EXPORT: No active child found and no child_id provided");
                    return Err(anyhow::anyhow!("No active child set and no child_id provided"));
                }
            }
        };

        // Step 2: Get the child details for the filename
        let get_child_command = GetChildCommand {
            child_id: child_id_to_use.clone(),
        };
        let child = match child_service.get_child(get_child_command)? {
            result if result.child.is_some() => result.child.unwrap(),
            _ => {
                error!("❌ EXPORT: Child not found: {}", child_id_to_use);
                return Err(anyhow::anyhow!("Child not found: {}", child_id_to_use));
            }
        };

        info!("✅ EXPORT: Exporting transactions for child: {}", child.name);

        // Step 3: Get all transactions for the child (no pagination for export)
        let domain_query = TransactionListQuery {
            after: None,
            limit: Some(10000),
            start_date: None,
            end_date: None,
        };

        let domain_res = transaction_service.list_transactions_domain(domain_query)?;

        let mut transactions: Vec<Transaction> = domain_res
            .transactions
            .into_iter()
            .map(TransactionMapper::to_dto)
            .collect();

        // Sort chronologically (oldest first)
        transactions.sort_by(|a, b| a.date.cmp(&b.date));

        info!("✅ EXPORT: Retrieved {} transactions for export", transactions.len());

        // Step 4: Generate CSV content
        let mut csv_content = String::new();
        csv_content.push_str("transaction_id,transaction_date,description,amount\n");

        for (index, transaction) in transactions.iter().enumerate() {
            // Format the date as yyyy/mm/dd
            let formatted_date = transaction.date.format("%Y/%m/%d").to_string();

            // Format the CSV row
            let row = format!(
                "{},{},\"{}\",{:.2}\n",
                index + 1, // Simple incrementing integer as requested
                formatted_date,
                transaction.description.replace("\"", "\"\""), // Escape quotes in description
                transaction.amount
            );
            csv_content.push_str(&row);
        }

        // Step 5: Generate filename with current date
        let now = Utc::now();
        let filename = format!(
            "{}_transactions_{}.csv",
            child.name.replace(" ", "_").to_lowercase(),
            now.format("%Y%m%d")
        );

        let response = ExportDataResponse {
            csv_content,
            filename,
            transaction_count: transactions.len(),
            child_name: child.name,
        };

        info!("✅ EXPORT: Successfully exported {} transactions for child: {} - generated CSV content ({} bytes) with filename: {}", 
              response.transaction_count, response.child_name, response.csv_content.len(), response.filename);

        Ok(response)
    }

    /// Export data directly to a specified path (or default location) with complete orchestration
    /// This method moves the orchestration logic from the REST API layer into the domain layer
    pub fn export_to_path<C: Connection>(
        &self,
        request: ExportToPathRequest,
        child_service: &ChildService,
        transaction_service: &TransactionService<C>,
    ) -> Result<ExportToPathResponse> {
        info!("📁 EXPORT: Exporting to path - custom_path: {:?}", request.custom_path);

        // Step 1: First, get the export data using existing logic
        let export_request = ExportDataRequest {
            child_id: request.child_id.clone(),
        };

        let export_response = self.export_transactions_csv(export_request, child_service, transaction_service)?;

        // Step 2: Determine the export directory
        let export_dir = match request.custom_path.clone() {
            Some(custom_path) if !custom_path.trim().is_empty() => {
                // Basic path sanitization: remove quotes, trim whitespace, handle common issues
                let cleaned_path = self.sanitize_path(&custom_path);
                std::path::PathBuf::from(cleaned_path)
            }
            _ => {
                // Use default location: Documents folder
                match dirs::document_dir() {
                    Some(docs_dir) => docs_dir,
                    None => {
                        // Fallback to home directory if Documents not available
                        match dirs::home_dir() {
                            Some(home_dir) => home_dir,
                            None => {
                                error!("❌ EXPORT: Could not determine default export directory");
                                return Ok(ExportToPathResponse {
                                    success: false,
                                    message: "Failed to determine export directory".to_string(),
                                    file_path: String::new(),
                                    transaction_count: 0,
                                    child_name: String::new(),
                                });
                            }
                        }
                    }
                }
            }
        };

        // Step 3: Create the full file path
        let file_path = export_dir.join(&export_response.filename);
        
        // Step 4: Ensure the directory exists
        if let Some(parent_dir) = file_path.parent() {
            if let Err(e) = fs::create_dir_all(parent_dir) {
                error!("❌ EXPORT: Failed to create export directory {:?}: {}", parent_dir, e);
                return Ok(ExportToPathResponse {
                    success: false,
                    message: format!("Failed to create export directory: {}", e),
                    file_path: parent_dir.to_string_lossy().to_string(),
                    transaction_count: 0,
                    child_name: String::new(),
                });
            }
        }

        // Step 5: Write the file
        match fs::write(&file_path, &export_response.csv_content) {
            Ok(_) => {
                let file_path_str = file_path.to_string_lossy().to_string();
                info!("✅ EXPORT: Successfully exported {} transactions for {} to: {}", 
                      export_response.transaction_count, export_response.child_name, file_path_str);
                
                Ok(ExportToPathResponse {
                    success: true,
                    message: format!("File exported successfully to: {}", file_path_str),
                    file_path: file_path_str,
                    transaction_count: export_response.transaction_count,
                    child_name: export_response.child_name,
                })
            }
            Err(e) => {
                error!("❌ EXPORT: Failed to write export file to {:?}: {}", file_path, e);
                Ok(ExportToPathResponse {
                    success: false,
                    message: format!("Failed to write export file: {}", e),
                    file_path: file_path.to_string_lossy().to_string(),
                    transaction_count: 0,
                    child_name: String::new(),
                })
            }
        }
    }

    /// Basic path sanitization to handle common user input issues
    fn sanitize_path(&self, path: &str) -> String {
        let mut cleaned = path.trim().to_string();
        
        // Remove surrounding quotes (single or double)
        if (cleaned.starts_with('"') && cleaned.ends_with('"')) ||
           (cleaned.starts_with('\'') && cleaned.ends_with('\'')) {
            cleaned = cleaned[1..cleaned.len()-1].to_string();
        }
        
        // Trim again after quote removal
        cleaned = cleaned.trim().to_string();
        
        // Handle escaped spaces (common on some systems)
        cleaned = cleaned.replace("\\ ", " ");
        
        // Remove any trailing slashes/backslashes
        while cleaned.ends_with('/') || cleaned.ends_with('\\') {
            cleaned.pop();
        }
        
        // Handle tilde expansion for home directory
        if cleaned.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                if cleaned == "~" {
                    cleaned = home.to_string_lossy().to_string();
                } else if cleaned.starts_with("~/") || cleaned.starts_with("~\\") {
                    cleaned = home.join(&cleaned[2..]).to_string_lossy().to_string();
                }
            }
        }
        
        cleaned
    }
}

impl Default for ExportService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sanitize_path() {
        let service = ExportService::new();
        
        // Test quote removal and tilde expansion
        let home_dir = dirs::home_dir().unwrap().to_string_lossy().to_string();
        let expected_documents = std::path::PathBuf::from(&home_dir).join("Documents").to_string_lossy().to_string();
        
        assert_eq!(service.sanitize_path("\"~/Documents\""), expected_documents);
        assert_eq!(service.sanitize_path("'~/Documents'"), expected_documents);
        
        // Test space handling
        assert_eq!(service.sanitize_path("  /path/to/dir  "), "/path/to/dir");
        assert_eq!(service.sanitize_path("/path\\ to\\ dir"), "/path to dir");
        
        // Test trailing slash removal
        assert_eq!(service.sanitize_path("/path/to/dir/"), "/path/to/dir");
        assert_eq!(service.sanitize_path("/path/to/dir\\"), "/path/to/dir");
    }
    
    #[test]
    fn test_export_service_creation() {
        let service = ExportService::new();
        assert!(true); // Service created successfully
        
        let service_default = ExportService::default();
        assert!(true); // Default service created successfully
    }
} 