use anyhow::Result;
use chrono::Utc;
use log::{info, warn};
use std::sync::Arc;

use crate::backend::storage::{DbConnection, AllowanceRepository};
use crate::backend::domain::child_service::ChildService;
use shared::{
    AllowanceConfig, GetAllowanceConfigRequest, GetAllowanceConfigResponse,
    UpdateAllowanceConfigRequest, UpdateAllowanceConfigResponse,
};

/// Service for managing allowance configurations
#[derive(Clone)]
pub struct AllowanceService {
    allowance_repository: AllowanceRepository,
    child_service: ChildService,
}

impl AllowanceService {
    /// Create a new AllowanceService
    pub fn new(db: Arc<DbConnection>) -> Self {
        let allowance_repository = AllowanceRepository::new((*db).clone());
        let child_service = ChildService::new(db);
        Self {
            allowance_repository,
            child_service,
        }
    }

    /// Get allowance configuration for a child
    pub async fn get_allowance_config(
        &self,
        request: GetAllowanceConfigRequest,
    ) -> Result<GetAllowanceConfigResponse> {
        info!("Getting allowance config: {:?}", request);

        let child_id = match request.child_id {
            Some(id) => id,
            None => {
                // Use active child if no child_id provided
                let active_child_response = self.child_service.get_active_child().await?;
                match active_child_response.active_child {
                    Some(child) => child.id,
                    None => {
                        warn!("No active child found for allowance config request");
                        return Ok(GetAllowanceConfigResponse {
                            allowance_config: None,
                        });
                    }
                }
            }
        };

        let allowance_config = self
            .allowance_repository
            .get_allowance_config(&child_id)
            .await?;

        if allowance_config.is_some() {
            info!("Found allowance config for child: {}", child_id);
        } else {
            info!("No allowance config found for child: {}", child_id);
        }

        Ok(GetAllowanceConfigResponse { allowance_config })
    }

    /// Update allowance configuration for a child
    pub async fn update_allowance_config(
        &self,
        request: UpdateAllowanceConfigRequest,
    ) -> Result<UpdateAllowanceConfigResponse> {
        info!("Updating allowance config: {:?}", request);

        // Validate day of week
        if !AllowanceConfig::is_valid_day_of_week(request.day_of_week) {
            return Err(anyhow::anyhow!(
                "Invalid day of week: {}. Must be 0-6 (Sunday-Saturday)",
                request.day_of_week
            ));
        }

        // Validate amount
        if request.amount < 0.0 {
            return Err(anyhow::anyhow!("Allowance amount cannot be negative"));
        }

        if request.amount > 1_000_000.0 {
            return Err(anyhow::anyhow!("Allowance amount is too large"));
        }

        let child_id = match request.child_id {
            Some(id) => {
                // Verify the child exists
                if self.child_service.get_child(&id).await?.is_none() {
                    return Err(anyhow::anyhow!("Child not found: {}", id));
                }
                id
            }
            None => {
                // Use active child if no child_id provided
                let active_child_response = self.child_service.get_active_child().await?;
                match active_child_response.active_child {
                    Some(child) => child.id,
                    None => {
                        return Err(anyhow::anyhow!(
                            "No active child found. Please select a child first."
                        ));
                    }
                }
            }
        };

        // Check if allowance config already exists
        let existing_config = self
            .allowance_repository
            .get_allowance_config(&child_id)
            .await?;

        let now = Utc::now();
        let timestamp_rfc3339 = now.to_rfc3339();

        let allowance_config = match existing_config {
            Some(mut config) => {
                // Update existing config
                config.amount = request.amount;
                config.day_of_week = request.day_of_week;
                config.is_active = request.is_active;
                config.updated_at = timestamp_rfc3339;
                config
            }
            None => {
                // Create new config
                let timestamp_millis = now.timestamp_millis() as u64;
                AllowanceConfig {
                    id: AllowanceConfig::generate_id(&child_id, timestamp_millis),
                    child_id: child_id.clone(),
                    amount: request.amount,
                    day_of_week: request.day_of_week,
                    is_active: request.is_active,
                    created_at: timestamp_rfc3339.clone(),
                    updated_at: timestamp_rfc3339,
                }
            }
        };

        // Store the configuration
        self.allowance_repository
            .store_allowance_config(&allowance_config)
            .await?;

        info!(
            "Updated allowance config for child {}: ${:.2} on {}s",
            child_id,
            allowance_config.amount,
            allowance_config.day_name()
        );

        Ok(UpdateAllowanceConfigResponse {
            allowance_config,
            success_message: "Allowance configuration updated successfully".to_string(),
        })
    }

    /// Delete allowance configuration for a child
    pub async fn delete_allowance_config(&self, child_id: &str) -> Result<bool> {
        info!("Deleting allowance config for child: {}", child_id);

        let deleted = self
            .allowance_repository
            .delete_allowance_config(child_id)
            .await?;

        if deleted {
            info!("Deleted allowance config for child: {}", child_id);
        } else {
            warn!("No allowance config found to delete for child: {}", child_id);
        }

        Ok(deleted)
    }

    /// List all allowance configurations (for admin purposes)
    pub async fn list_allowance_configs(&self) -> Result<Vec<AllowanceConfig>> {
        info!("Listing all allowance configurations");

        let configs = self.allowance_repository.list_allowance_configs().await?;

        info!("Found {} allowance configurations", configs.len());

        Ok(configs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{Child, CreateChildRequest};

    async fn setup_test() -> AllowanceService {
        let db = Arc::new(DbConnection::init_test().await.expect("Failed to init test DB"));
        AllowanceService::new(db)
    }

    async fn create_test_child(service: &AllowanceService) -> Child {
        let request = CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        let response = service
            .child_service
            .create_child(request)
            .await
            .expect("Failed to create test child");
        response.child
    }

    #[tokio::test]
    async fn test_get_allowance_config_not_found() {
        let service = setup_test().await;
        let child = create_test_child(&service).await;

        let request = GetAllowanceConfigRequest {
            child_id: Some(child.id),
        };

        let response = service
            .get_allowance_config(request)
            .await
            .expect("Failed to get allowance config");

        assert!(response.allowance_config.is_none());
    }

    #[tokio::test]
    async fn test_update_and_get_allowance_config() {
        let service = setup_test().await;
        let child = create_test_child(&service).await;

        // Create allowance config
        let update_request = UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
        };

        let update_response = service
            .update_allowance_config(update_request)
            .await
            .expect("Failed to update allowance config");

        assert_eq!(update_response.allowance_config.amount, 10.0);
        assert_eq!(update_response.allowance_config.day_of_week, 1);
        assert_eq!(update_response.allowance_config.is_active, true);
        assert_eq!(update_response.allowance_config.child_id, child.id);
        assert_eq!(update_response.allowance_config.day_name(), "Monday");

        // Get the config back
        let get_request = GetAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
        };

        let get_response = service
            .get_allowance_config(get_request)
            .await
            .expect("Failed to get allowance config");

        assert!(get_response.allowance_config.is_some());
        let config = get_response.allowance_config.unwrap();
        assert_eq!(config.amount, 10.0);
        assert_eq!(config.day_of_week, 1);
        assert_eq!(config.is_active, true);
        assert_eq!(config.child_id, child.id);
    }

    #[tokio::test]
    async fn test_update_existing_allowance_config() {
        let service = setup_test().await;
        let child = create_test_child(&service).await;

        // Create initial config
        let initial_request = UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 5.0,
            day_of_week: 0, // Sunday
            is_active: true,
        };

        let initial_response = service
            .update_allowance_config(initial_request)
            .await
            .expect("Failed to create initial allowance config");

        let initial_id = initial_response.allowance_config.id.clone();

        // Update the config
        let update_request = UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 15.0,
            day_of_week: 6, // Saturday
            is_active: false,
        };

        let update_response = service
            .update_allowance_config(update_request)
            .await
            .expect("Failed to update allowance config");

        // Should have same ID but updated values
        assert_eq!(update_response.allowance_config.id, initial_id);
        assert_eq!(update_response.allowance_config.amount, 15.0);
        assert_eq!(update_response.allowance_config.day_of_week, 6);
        assert_eq!(update_response.allowance_config.is_active, false);
        assert_eq!(update_response.allowance_config.day_name(), "Saturday");
    }

    #[tokio::test]
    async fn test_invalid_day_of_week() {
        let service = setup_test().await;
        let child = create_test_child(&service).await;

        let request = UpdateAllowanceConfigRequest {
            child_id: Some(child.id),
            amount: 10.0,
            day_of_week: 7, // Invalid - should be 0-6
            is_active: true,
        };

        let result = service.update_allowance_config(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid day of week"));
    }

    #[tokio::test]
    async fn test_negative_amount() {
        let service = setup_test().await;
        let child = create_test_child(&service).await;

        let request = UpdateAllowanceConfigRequest {
            child_id: Some(child.id),
            amount: -5.0,
            day_of_week: 1,
            is_active: true,
        };

        let result = service.update_allowance_config(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be negative"));
    }

    #[tokio::test]
    async fn test_delete_allowance_config() {
        let service = setup_test().await;
        let child = create_test_child(&service).await;

        // Create config first
        let update_request = UpdateAllowanceConfigRequest {
            child_id: Some(child.id.clone()),
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
        };

        service
            .update_allowance_config(update_request)
            .await
            .expect("Failed to create allowance config");

        // Delete it
        let deleted = service
            .delete_allowance_config(&child.id)
            .await
            .expect("Failed to delete allowance config");

        assert!(deleted);

        // Verify it's gone
        let get_request = GetAllowanceConfigRequest {
            child_id: Some(child.id),
        };

        let get_response = service
            .get_allowance_config(get_request)
            .await
            .expect("Failed to get allowance config");

        assert!(get_response.allowance_config.is_none());
    }

    #[tokio::test]
    async fn test_list_allowance_configs() {
        let service = setup_test().await;
        let child1 = create_test_child(&service).await;

        // Create child2 with different name to get different ID
        let request2 = CreateChildRequest {
            name: "Test Child 2".to_string(),
            birthdate: "2016-01-01".to_string(),
        };
        let response2 = service
            .child_service
            .create_child(request2)
            .await
            .expect("Failed to create test child 2");
        let child2 = response2.child;

        // Create configs for both children
        let request1 = UpdateAllowanceConfigRequest {
            child_id: Some(child1.id),
            amount: 5.0,
            day_of_week: 1,
            is_active: true,
        };

        let request2 = UpdateAllowanceConfigRequest {
            child_id: Some(child2.id),
            amount: 10.0,
            day_of_week: 5,
            is_active: false,
        };

        service
            .update_allowance_config(request1)
            .await
            .expect("Failed to create config 1");

        service
            .update_allowance_config(request2)
            .await
            .expect("Failed to create config 2");

        // List all configs
        let configs = service
            .list_allowance_configs()
            .await
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

    #[tokio::test]
    async fn test_allowance_config_day_names() {
        let config = AllowanceConfig {
            id: "test".to_string(),
            child_id: "test".to_string(),
            amount: 10.0,
            day_of_week: 0,
            is_active: true,
            created_at: "test".to_string(),
            updated_at: "test".to_string(),
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

    #[tokio::test]
    async fn test_is_valid_day_of_week() {
        assert!(AllowanceConfig::is_valid_day_of_week(0));
        assert!(AllowanceConfig::is_valid_day_of_week(1));
        assert!(AllowanceConfig::is_valid_day_of_week(6));
        assert!(!AllowanceConfig::is_valid_day_of_week(7));
        assert!(!AllowanceConfig::is_valid_day_of_week(255));
    }

    #[tokio::test]
    async fn test_generate_id() {
        let id1 = AllowanceConfig::generate_id("child::123", 1000);
        let id2 = AllowanceConfig::generate_id("child::456", 2000);

        assert_eq!(id1, "allowance::child::123::1000");
        assert_eq!(id2, "allowance::child::456::2000");
        assert_ne!(id1, id2);
    }
} 