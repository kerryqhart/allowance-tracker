use anyhow::Result;
use chrono::{Utc, NaiveDate, Datelike, Local, FixedOffset};
use log::{info, warn};
use std::sync::Arc;

use crate::backend::storage::csv::{CsvConnection, AllowanceRepository, TransactionRepository};
use crate::backend::storage::traits::{AllowanceStorage, TransactionStorage};
use crate::backend::domain::child_service::ChildService;
use crate::backend::domain::models::allowance::AllowanceConfig;
use crate::backend::domain::models::transaction::{Transaction as DomainTransaction, TransactionType as DomainTransactionType};
use crate::backend::domain::commands::allowance::{
    GetAllowanceConfigCommand, UpdateAllowanceConfigCommand
};
use crate::backend::domain::commands::allowance::{
    GetAllowanceConfigResult, UpdateAllowanceConfigResult
};
use crate::backend::domain::commands::child::GetChildCommand;


/// Service for managing allowance configurations
#[derive(Clone)]
pub struct AllowanceService {
    allowance_repository: AllowanceRepository,
    transaction_repository: TransactionRepository,
    child_service: ChildService,
}

impl AllowanceService {
    /// Create a new AllowanceService
    pub fn new(csv_conn: Arc<CsvConnection>) -> Self {
        let allowance_repository = AllowanceRepository::new((*csv_conn).clone());
        let transaction_repository = TransactionRepository::new((*csv_conn).clone());
        let child_service = ChildService::new(csv_conn);
        Self {
            allowance_repository,
            transaction_repository,
            child_service,
        }
    }

    /// Get allowance configuration for a child
    pub fn get_allowance_config(
        &self,
        command: GetAllowanceConfigCommand,
    ) -> Result<GetAllowanceConfigResult> {
        info!("Getting allowance config: {:?}", command);

        let child_id = match command.child_id {
            Some(id) => id,
            None => {
                // Use active child if no child_id provided
                let active_child_result = self.child_service.get_active_child()?;
                let child = match active_child_result.active_child.child {
                    Some(c) => c.id,
                    None => {
                        warn!("No active child found for allowance config request");
                        return Ok(GetAllowanceConfigResult {
                            allowance_config: None,
                        });
                    }
                };
                child
            }
        };

        let domain_allowance_config = self
            .allowance_repository
            .get_allowance_config(&child_id)?;

        if let Some(ref config) = domain_allowance_config {
            info!("Found allowance config for child: {}", child_id);
            info!("🔍 DEBUG: Allowance config details - day_of_week: {}, day_name: {}, amount: {}, is_active: {}", 
                config.day_of_week, config.day_name(), config.amount, config.is_active);
        } else {
            info!("No allowance config found for child: {}", child_id);
        }

        Ok(GetAllowanceConfigResult { allowance_config: domain_allowance_config })
    }

    /// Update allowance configuration for a child
    pub fn update_allowance_config(
        &self,
        command: UpdateAllowanceConfigCommand,
    ) -> Result<UpdateAllowanceConfigResult> {
        info!("Updating allowance config: {:?}", command);

        // Validate day of week
        if !AllowanceConfig::is_valid_day_of_week(command.day_of_week) {
            return Err(anyhow::anyhow!(
                "Invalid day of week: {}. Must be 0-6 (Sunday-Saturday)",
                command.day_of_week
            ));
        }

        // Validate amount
        if command.amount < 0.0 {
            return Err(anyhow::anyhow!("Allowance amount cannot be negative"));
        }

        if command.amount > 1_000_000.0 {
            return Err(anyhow::anyhow!("Allowance amount is too large"));
        }

        let child_id = match command.child_id {
            Some(id) => {
                // Verify the child exists
                let get_child_command = GetChildCommand { child_id: id.clone() };
                if self.child_service.get_child(get_child_command)?.child.is_none() {
                    return Err(anyhow::anyhow!("Child not found: {}", id));
                }
                id
            }
            None => {
                // Use active child if no child_id provided
                let active_child_result = self.child_service.get_active_child()?;
                let child = match active_child_result.active_child.child {
                    Some(c) => c.id,
                    None => return Err(anyhow::anyhow!("No active child found to update allowance config")),
                };
                child
            }
        };

        // Check if allowance config already exists
        let existing_domain_config = self
            .allowance_repository
            .get_allowance_config(&child_id)?;

        let now = Utc::now();
        let timestamp_rfc3339 = now.to_rfc3339();

        let domain_allowance_config = match existing_domain_config {
            Some(mut config) => {
                // Update existing config
                config.amount = command.amount;
                config.day_of_week = command.day_of_week;
                config.is_active = command.is_active;
                config.updated_at = timestamp_rfc3339;
                config
            }
            None => {
                // Create new config
                let timestamp_millis = now.timestamp_millis() as u64;
                AllowanceConfig {
                    id: AllowanceConfig::generate_id(&child_id, timestamp_millis),
                    child_id: child_id.clone(),
                    amount: command.amount,
                    day_of_week: command.day_of_week,
                    is_active: command.is_active,
                    created_at: timestamp_rfc3339.clone(),
                    updated_at: timestamp_rfc3339,
                }
            }
        };

        // Store the configuration directly as domain model
        self.allowance_repository
            .store_allowance_config(&domain_allowance_config)?;

        info!(
            "Updated allowance config for child {}: ${:.2} on {}s",
            child_id,
            domain_allowance_config.amount,
            domain_allowance_config.day_name()
        );

        Ok(UpdateAllowanceConfigResult {
            allowance_config: domain_allowance_config,
            success_message: "Allowance configuration updated successfully".to_string(),
        })
    }

    /// Delete allowance configuration for a child
    pub fn delete_allowance_config(&self, child_id: &str) -> Result<bool> {
        info!("Deleting allowance config for child: {}", child_id);

        let deleted = self
            .allowance_repository
            .delete_allowance_config(child_id)?;

        if deleted {
            info!("Deleted allowance config for child: {}", child_id);
        } else {
            warn!("No allowance config found to delete for child: {}", child_id);
        }

        Ok(deleted)
    }

    /// List all allowance configurations (for admin purposes)
    pub fn list_allowance_configs(&self) -> Result<Vec<AllowanceConfig>> {
        info!("Listing all allowance configurations");

        let domain_configs = self.allowance_repository.list_allowance_configs()?;

        info!("Found {} allowance configurations", domain_configs.len());

        Ok(domain_configs)
    }

    /// Generate forward-looking allowance transactions for a given date range
    pub fn generate_future_allowance_transactions(
        &self,
        child_id: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<DomainTransaction>> {
        info!("🔮 ALLOWANCE DEBUG: Generating future allowances for child {} from {} to {}", 
             child_id, start_date, end_date);

        // Get allowance config for the child
        let allowance_config = self.allowance_repository.get_allowance_config(child_id)?;
        
        info!("🔮 ALLOWANCE DEBUG: Retrieved allowance config: {:?}", allowance_config);
        
        let config = match allowance_config {
            Some(config) if config.is_active => {
                info!("🔮 ALLOWANCE DEBUG: Found active config - amount: ${:.2}, day_of_week: {} ({})", 
                     config.amount, config.day_of_week, config.day_name());
                config
            },
            Some(_config) => {
                info!("🔮 ALLOWANCE DEBUG: Found inactive allowance config for child: {}", child_id);
                return Ok(Vec::new());
            },
            None => {
                info!("🔮 ALLOWANCE DEBUG: No allowance config found for child: {}", child_id);
                return Ok(Vec::new());
            }
        };

        let mut future_allowances = Vec::new();
        let current_date = Local::now().date_naive();
        
        info!("🔮 ALLOWANCE DEBUG: Current date: {}", current_date);

        // Iterate through each date in the range
        let mut current = start_date;
        let mut checked_days = 0;
        while current <= end_date {
            checked_days += 1;
            let day_of_week = current.weekday().num_days_from_sunday() as u8;
            let is_future = current > current_date;
            let matches_allowance_day = day_of_week == config.day_of_week;
            
            info!("🔮 ALLOWANCE DEBUG: Checking date {} - day_of_week: {} ({}), is_future: {}, matches_allowance_day: {}", 
                 current, day_of_week, current.weekday(), is_future, matches_allowance_day);
            
            // Check if this date is in the future and matches the allowance day of week
            if current > current_date {
                if day_of_week == config.day_of_week {
                    info!("🔮 ALLOWANCE DEBUG: ✅ CREATING future allowance for {} on {}", child_id, current);
                    
                    // This is a future allowance day!
                    // Create DateTime at 12:00 UTC for the date
                    let naive_datetime = current.and_hms_opt(12, 0, 0).unwrap();
                    let utc_offset = FixedOffset::east_opt(0).unwrap(); // UTC offset
                    let transaction_datetime = naive_datetime.and_local_timezone(utc_offset)
                        .single()
                        .unwrap();
                    
                    let allowance_transaction = DomainTransaction {
                        id: format!("future-allowance::{}::{}", child_id, current.format("%Y-%m-%d")),
                        child_id: child_id.to_string(),
                        date: transaction_datetime,
                        description: "Weekly allowance".to_string(),
                        amount: config.amount,
                        balance: 0.0, // Balance is not meaningful for future transactions
                        transaction_type: DomainTransactionType::FutureAllowance,
                    };
                    
                    future_allowances.push(allowance_transaction);
                    info!("🔮 ALLOWANCE DEBUG: Generated future allowance for {} on {} (day_of_week: {}, expected: {}, datetime: {})", 
                          child_id, current, day_of_week, config.day_of_week, transaction_datetime);
                } else {
                    info!("🔮 ALLOWANCE DEBUG: ❌ Future date {} doesn't match allowance day (got {}, need {})", 
                         current, day_of_week, config.day_of_week);
                }
            } else {
                info!("🔮 ALLOWANCE DEBUG: ❌ Date {} is not in the future (current: {})", current, current_date);
            }
            
            // Move to next day
            current = current.succ_opt().unwrap_or(current);
            if current == current.succ_opt().unwrap_or(current) {
                // Prevent infinite loop if succ_opt fails
                break;
            }
        }

        info!("🔮 ALLOWANCE DEBUG: Checked {} days total, generated {} future allowance transactions for child: {}", 
              checked_days, future_allowances.len(), child_id);

        if future_allowances.is_empty() {
            info!("🔮 ALLOWANCE DEBUG: ❌ NO FUTURE ALLOWANCES GENERATED! Check the configuration and date range.");
        } else {
            info!("🔮 ALLOWANCE DEBUG: ✅ Generated future allowances:");
            for allowance in &future_allowances {
                info!("  - {} on {}: ${:.2}", allowance.id, allowance.date.format("%Y-%m-%d"), allowance.amount);
            }
        }

        Ok(future_allowances)
    }

    /// Check for pending allowances that need to be issued
    /// Returns a list of dates for which allowances should be created
    pub fn get_pending_allowance_dates(
        &self,
        child_id: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> Result<Vec<(NaiveDate, f64)>> {
        info!("Checking for pending allowances for child: {} from {} to {}", 
              child_id, from_date, to_date);

        // Get allowance config for the child
        let allowance_config = self.allowance_repository.get_allowance_config(child_id)?;
        
        let config = match allowance_config {
            Some(config) if config.is_active => config,
            _ => {
                info!("No active allowance config found for child: {}", child_id);
                return Ok(Vec::new());
            }
        };

        let mut pending_dates = Vec::new();
        let current_date = Local::now().date_naive();

        // Iterate through each date in the range
        let mut current = from_date;
        while current <= to_date && current <= current_date {
            let day_of_week = current.weekday().num_days_from_sunday() as u8;
            
            if day_of_week == config.day_of_week {
                // This is an allowance day - check if allowance already exists
                if !self.has_allowance_for_date(&config.child_id, current)? {
                    pending_dates.push((current, config.amount));
                    info!("🎯 Found pending allowance for {} on {} (${:.2})", 
                          child_id, current, config.amount);
                }
            }
            
            // Move to next day
            current = current.succ_opt().unwrap_or(current);
            if current == current.succ_opt().unwrap_or(current) {
                // Prevent infinite loop if succ_opt fails
                break;
            }
        }

        info!("Found {} pending allowances for child: {}", 
              pending_dates.len(), child_id);

        Ok(pending_dates)
    }

    /// Check if an allowance already exists for a specific date
    /// This is used to prevent duplicate allowances
    fn has_allowance_for_date(&self, child_id: &str, date: NaiveDate) -> Result<bool> {
        info!("🎯 ALLOWANCE DEBUG: has_allowance_for_date() called for child {} on date {}", child_id, date);
        
        // Get all transactions for the child
        let transactions = self.transaction_repository.list_transactions(child_id, None, None)?;
        info!("🎯 ALLOWANCE DEBUG: Retrieved {} total transactions for allowance check", transactions.len());
        
        // Format the date as string prefix (YYYY-MM-DD) to match
        let date_prefix = date.format("%Y-%m-%d").to_string();
        info!("🎯 ALLOWANCE DEBUG: Looking for allowances on date prefix: {}", date_prefix);
        
        // Check if any transaction for this date looks like an allowance
        // We'll be more conservative: look for any positive income on allowance day
        let mut allowance_count = 0;
        for transaction in transactions {
            // Format the transaction date to string for comparison
            let transaction_date_str = transaction.date.format("%Y-%m-%d").to_string();
            
            // Check if transaction is on this date and has positive amount (indicating income/allowance)
            if transaction_date_str.starts_with(&date_prefix) && transaction.amount > 0.0 {
                info!("🎯 ALLOWANCE DEBUG: Found positive transaction on target date: {} (${:.2}) - {}", 
                      transaction.id, transaction.amount, transaction.description);
                // Check if the description suggests it's an allowance
                let desc_lower = transaction.description.to_lowercase();
                if desc_lower.contains("allowance") || desc_lower.contains("weekly") {
                    allowance_count += 1;
                    info!("🎯 ALLOWANCE DEBUG: ✅ Found existing allowance {} for {} on {}: {}", 
                          allowance_count, child_id, date, transaction.description);
                } else {
                    info!("🎯 ALLOWANCE DEBUG: ❌ Positive transaction but not an allowance: {}", transaction.description);
                }
            } else if transaction_date_str.starts_with(&date_prefix) {
                info!("🎯 ALLOWANCE DEBUG: Found transaction on target date but not positive: {} (${:.2}) - {}", 
                      transaction.id, transaction.amount, transaction.description);
            }
        }
        
        let has_allowance = allowance_count > 0;
        info!("🎯 ALLOWANCE DEBUG: has_allowance_for_date() result: {} (found {} allowances)", has_allowance, allowance_count);
        
        // Return true if we found at least one allowance for this date
        Ok(has_allowance)
    }

    /// Utility method to check if a given date is an allowance day for a specific day of week
    pub fn is_allowance_day(date: NaiveDate, allowance_day_of_week: u8) -> bool {
        let day_of_week = date.weekday().num_days_from_sunday() as u8;
        day_of_week == allowance_day_of_week
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::domain::models::child::Child as DomainChild;
    use crate::backend::domain::commands::child::CreateChildCommand;
    use tempfile::tempdir;

    fn setup_test() -> AllowanceService {
        let temp_dir = tempdir().unwrap();
        let conn = CsvConnection::new(temp_dir.path().to_path_buf()).unwrap();
        AllowanceService::new(Arc::new(conn))
    }

    fn create_test_child(service: &AllowanceService) -> DomainChild {
        let command = crate::backend::domain::commands::child::CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        let result = service
            .child_service
            .create_child(command)
            
            .expect("Failed to create test child");
        
        result.child
    }

    #[test]
    fn test_get_allowance_config_not_found() {
        let service = setup_test();
        let child = create_test_child(&service);

        let command = GetAllowanceConfigCommand {
            child_id: Some(child.id),
        };

        let response = service
            .get_allowance_config(command)
            
            .expect("Failed to get allowance config");

        assert!(response.allowance_config.is_none());
    }

    #[test]
    fn test_update_and_get_allowance_config() {
        let service = setup_test();
        let child = create_test_child(&service);

        // Create allowance config
        let update_command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
        };

        let update_response = service
            .update_allowance_config(update_command)
            
            .expect("Failed to update allowance config");

        assert_eq!(update_response.allowance_config.amount, 10.0);
        assert_eq!(update_response.allowance_config.day_of_week, 1);
        assert_eq!(update_response.allowance_config.is_active, true);
        assert_eq!(update_response.allowance_config.child_id, child.id);
        assert_eq!(update_response.allowance_config.day_name(), "Monday");

        // Get the config back
        let get_command = GetAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
        };

        let get_result = service
            .get_allowance_config(get_command)
            
            .expect("Failed to get allowance config");

        assert!(get_result.allowance_config.is_some());
        let config = get_result.allowance_config.unwrap();
        assert_eq!(config.amount, 10.0);
        assert_eq!(config.day_of_week, 1);
        assert_eq!(config.is_active, true);
        assert_eq!(config.child_id, child.id);
    }

    #[test]
    fn test_update_existing_allowance_config() {
        let service = setup_test();
        let child = create_test_child(&service);

        // Create initial config
        let initial_command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 5.0,
            day_of_week: 0, // Sunday
            is_active: true,
        };

        let initial_response = service
            .update_allowance_config(initial_command)
            
            .expect("Failed to create initial allowance config");

        let initial_id = initial_response.allowance_config.id.clone();

        // Update the config
        let update_command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 15.0,
            day_of_week: 6, // Saturday
            is_active: false,
        };

        let update_response = service
            .update_allowance_config(update_command)
            
            .expect("Failed to update allowance config");

        // Should have same ID but updated values
        assert_eq!(update_response.allowance_config.id, initial_id);
        assert_eq!(update_response.allowance_config.amount, 15.0);
        assert_eq!(update_response.allowance_config.day_of_week, 6);
        assert_eq!(update_response.allowance_config.is_active, false);
        assert_eq!(update_response.allowance_config.day_name(), "Saturday");
    }

    #[test]
    fn test_invalid_day_of_week() {
        let service = setup_test();
        let child = create_test_child(&service);

        let command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id),
            amount: 10.0,
            day_of_week: 7, // Invalid - should be 0-6
            is_active: true,
        };

        let result = service.update_allowance_config(command);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid day of week"));
    }

    #[test]
    fn test_negative_amount() {
        let service = setup_test();
        let child = create_test_child(&service);

        let command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id),
            amount: -5.0,
            day_of_week: 1,
            is_active: true,
        };

        let result = service.update_allowance_config(command);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be negative"));
    }

    #[test]
    fn test_delete_allowance_config() {
        let service = setup_test();
        let child = create_test_child(&service);

        // Create config first
        let update_command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
        };

        service
            .update_allowance_config(update_command)
            
            .expect("Failed to create allowance config");

        // Delete it
        let deleted = service
            .delete_allowance_config(&child.id)
            
            .expect("Failed to delete allowance config");

        assert!(deleted);

        // Verify it's gone
        let get_command = GetAllowanceConfigCommand {
            child_id: Some(child.id),
        };

        let get_result = service
            .get_allowance_config(get_command)
            
            .expect("Failed to get allowance config");

        assert!(get_result.allowance_config.is_none());
    }

    #[test]
    fn test_list_allowance_configs() {
        let service = setup_test();
        let child1 = create_test_child(&service);

        // Create child2 with different name to get different ID
        let command2 = crate::backend::domain::commands::child::CreateChildCommand {
            name: "Test Child 2".to_string(),
            birthdate: "2016-01-01".to_string(),
        };
        let result2 = service
            .child_service
            .create_child(command2)
            
            .expect("Failed to create test child 2");
        let child2 = result2.child;

        // Create configs for both children
        let command1 = UpdateAllowanceConfigCommand {
            child_id: Some(child1.id),
            amount: 5.0,
            day_of_week: 1,
            is_active: true,
        };

        let command2 = UpdateAllowanceConfigCommand {
            child_id: Some(child2.id),
            amount: 10.0,
            day_of_week: 5,
            is_active: false,
        };

        service
            .update_allowance_config(command1)
            
            .expect("Failed to create config 1");

        service
            .update_allowance_config(command2)
            
            .expect("Failed to create config 2");

        // List all configs
        let configs = service
            .list_allowance_configs()
            
            .expect("Failed to list configs");

        assert_eq!(configs.len(), 2);

        // Check that both configs are present
        let config1 = configs.iter().find(|c| c.amount == 5.0).unwrap();
        let config2 = configs.iter().find(|c| c.amount == 10.0).unwrap();

        assert_eq!(config1.day_of_week, 1);
        assert_eq!(config1.is_active, true);
        assert_eq!(config2.day_of_week, 5);
        assert_eq!(config2.is_active, false);
    }

    #[test]
    fn test_allowance_config_day_names() {
        let config = AllowanceConfig {
            id: "test".to_string(),
            child_id: "test".to_string(),
            amount: 10.0,
            day_of_week: 0,
            is_active: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        let days = [
            (0, "Sunday"),
            (1, "Monday"),
            (2, "Tuesday"),
            (3, "Wednesday"),
            (4, "Thursday"),
            (5, "Friday"),
            (6, "Saturday"),
            (7, "Invalid"),
        ];

        for (day_num, expected_name) in days {
            let mut test_config = config.clone();
            test_config.day_of_week = day_num;
            assert_eq!(test_config.day_name(), expected_name);
        }
    }

    #[test]
    fn test_is_valid_day_of_week() {
        assert!(AllowanceConfig::is_valid_day_of_week(0));
        assert!(AllowanceConfig::is_valid_day_of_week(1));
        assert!(AllowanceConfig::is_valid_day_of_week(6));
        assert!(!AllowanceConfig::is_valid_day_of_week(7));
        assert!(!AllowanceConfig::is_valid_day_of_week(255));
    }

    #[test]
    fn test_generate_id() {
        let child_id = "child123";
        let timestamp = 1234567890u64;
        let id = AllowanceConfig::generate_id(child_id, timestamp);
        assert_eq!(id, "allowance::child123::1234567890");
    }

    #[test]
    fn test_get_pending_allowance_dates_no_config() {
        let service = setup_test();
        let child = create_test_child(&service);

        let from_date = Local::now().date_naive() - chrono::Duration::days(7);
        let to_date = Local::now().date_naive();

        let pending = service
            .get_pending_allowance_dates(&child.id, from_date, to_date)
            
            .expect("Failed to get pending allowances");

        assert!(pending.is_empty(), "Should have no pending allowances without config");
    }

    #[test]
    fn test_get_pending_allowance_dates_inactive_config() {
        let service = setup_test();
        let child = create_test_child(&service);

        // Create inactive allowance config
        let command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: false, // Inactive
        };

        service
            .update_allowance_config(command)
            
            .expect("Failed to create allowance config");

        let from_date = Local::now().date_naive() - chrono::Duration::days(7);
        let to_date = Local::now().date_naive();

        let pending = service
            .get_pending_allowance_dates(&child.id, from_date, to_date)
            
            .expect("Failed to get pending allowances");

        assert!(pending.is_empty(), "Should have no pending allowances with inactive config");
    }

    #[test]
    fn test_get_pending_allowance_dates_with_config() {
        let service = setup_test();
        let child = create_test_child(&service);

        // Create active allowance config for every day (day_of_week: 0-6)
        // We'll use Sunday (0) for testing
        let command = UpdateAllowanceConfigCommand {
            child_id: Some(child.id.clone()),
            amount: 5.0,
            day_of_week: 0, // Sunday
            is_active: true,
        };

        service
            .update_allowance_config(command)
            
            .expect("Failed to create allowance config");

        // Test a 7-day range that includes at least one Sunday
        let current_date = Local::now().date_naive();
        let from_date = current_date - chrono::Duration::days(7);
        let to_date = current_date;

        let pending = service
            .get_pending_allowance_dates(&child.id, from_date, to_date)
            
            .expect("Failed to get pending allowances");

        // Count how many Sundays are in the range (should be at least 1)
        let mut expected_sundays = 0;
        let mut date = from_date;
        while date <= to_date {
            if date.weekday().num_days_from_sunday() as u8 == 0 {
                expected_sundays += 1;
            }
            date = date.succ_opt().unwrap_or(date);
            if date == date.succ_opt().unwrap_or(date) {
                break;
            }
        }

        assert!(expected_sundays > 0, "Test range should include at least one Sunday");
        assert_eq!(pending.len(), expected_sundays, "Should find all Sundays in range as pending allowances");

        // Verify the amount is correct
        for (_, amount) in &pending {
            assert_eq!(*amount, 5.0, "Pending allowance amount should match config");
        }
    }

    #[test]
    fn test_is_allowance_day() {
        // Test different days of week
        let monday = NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(); // Known Monday
        let tuesday = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap(); // Known Tuesday
        let sunday = NaiveDate::from_ymd_opt(2025, 7, 6).unwrap(); // Known Sunday

        assert_eq!(monday.weekday().num_days_from_sunday() as u8, 1);
        assert_eq!(tuesday.weekday().num_days_from_sunday() as u8, 2);
        assert_eq!(sunday.weekday().num_days_from_sunday() as u8, 0);

        // Test is_allowance_day function
        assert!(AllowanceService::is_allowance_day(monday, 1)); // Monday = 1
        assert!(AllowanceService::is_allowance_day(tuesday, 2)); // Tuesday = 2
        assert!(AllowanceService::is_allowance_day(sunday, 0)); // Sunday = 0

        assert!(!AllowanceService::is_allowance_day(monday, 0)); // Monday is not Sunday
        assert!(!AllowanceService::is_allowance_day(tuesday, 1)); // Tuesday is not Monday
        assert!(!AllowanceService::is_allowance_day(sunday, 6)); // Sunday is not Saturday
    }

    #[test]
    fn test_has_allowance_for_date_no_transactions() {
        let service = setup_test();
        let child = create_test_child(&service);

        let test_date = Local::now().date_naive();
        let has_allowance = service
            .has_allowance_for_date(&child.id, test_date)
            
            .expect("Failed to check allowance for date");

        assert!(!has_allowance, "Should not have allowance when no transactions exist");
    }

    #[test]
    fn test_has_allowance_for_date_with_allowance() {
        let service = setup_test();
        let child = create_test_child(&service);

        let test_date = Local::now().date_naive();
        
        // Create a mock allowance transaction for today
        let transaction = DomainTransaction {
            id: "test_allowance_123".to_string(),
            child_id: child.id.clone(),
            date: chrono::DateTime::parse_from_str(&format!("{}T12:00:00-05:00", test_date.format("%Y-%m-%d")), "%Y-%m-%dT%H:%M:%S%z").expect("Failed to parse date"),
            description: "Weekly allowance".to_string(),
            amount: 5.0,
            balance: 5.0,
            transaction_type: DomainTransactionType::Income,
        };

        // Store the transaction
        service
            .transaction_repository
            .store_transaction(&transaction)
            
            .expect("Failed to store test transaction");

        let has_allowance = service
            .has_allowance_for_date(&child.id, test_date)
            
            .expect("Failed to check allowance for date");

        assert!(has_allowance, "Should detect existing allowance transaction");
    }

    #[test]
    fn test_has_allowance_for_date_with_non_allowance_transaction() {
        let service = setup_test();
        let child = create_test_child(&service);

        let test_date = Local::now().date_naive();
        
        // Create a non-allowance transaction for today
        let transaction = DomainTransaction {
            id: "test_expense_123".to_string(),
            child_id: child.id.clone(),
            date: chrono::DateTime::parse_from_str(&format!("{}T12:00:00-05:00", test_date.format("%Y-%m-%d")), "%Y-%m-%dT%H:%M:%S%z").expect("Failed to parse date"),
            description: "Bought candy".to_string(),
            amount: -2.0, // Negative amount (expense)
            balance: 3.0,
            transaction_type: DomainTransactionType::Expense,
        };

        // Store the transaction
        service
            .transaction_repository
            .store_transaction(&transaction)
            
            .expect("Failed to store test transaction");

        let has_allowance = service
            .has_allowance_for_date(&child.id, test_date)
            
            .expect("Failed to check allowance for date");

        assert!(!has_allowance, "Should not detect allowance from non-allowance transaction");
    }
} 