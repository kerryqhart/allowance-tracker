use anyhow::{Result, Context};
use chrono::{Utc, NaiveDate};
use log::{info, warn, debug};
use std::sync::Arc;

use crate::backend::domain::models::child::{ActiveChild, Child as DomainChild};
use crate::backend::domain::commands::child::{
    CreateChildCommand, UpdateChildCommand, GetChildCommand, SetActiveChildCommand, DeleteChildCommand,
    CreateChildResult, UpdateChildResult, GetChildResult, GetActiveChildResult, ListChildrenResult,
    SetActiveChildResult, DeleteChildResult,
};
use crate::backend::storage::csv::{CsvConnection, ChildRepository};
use crate::backend::storage::traits::ChildStorage;

/// Service for managing children in the allowance tracking system
#[derive(Clone)]
pub struct ChildService {
    child_repository: ChildRepository,
}

impl ChildService {
    /// Create a new ChildService
    pub fn new(csv_conn: Arc<CsvConnection>) -> Self {
        let child_repository = ChildRepository::new((*csv_conn).clone());
        Self { child_repository }
    }

    /// Create a new child
    pub async fn create_child(&self, command: CreateChildCommand) -> Result<CreateChildResult> {
        info!("Creating child: name={}, birthdate={}", command.name, command.birthdate);

        // Validate the command
        self.validate_create_command(&command)?;

        // Generate timestamps and parse birthdate
        let now = Utc::now();
        let birthdate = NaiveDate::parse_from_str(&command.birthdate, "%Y-%m-%d")
            .context("Invalid birthdate format in create_child command")?;

        // Create the domain child
        let child = DomainChild {
            id: DomainChild::generate_id(now.timestamp_millis() as u64),
            name: command.name.trim().to_string(),
            birthdate,
            created_at: now,
            updated_at: now,
        };

        // Store in database
        self.child_repository.store_child(&child).await?;

        info!("Created child: {} with ID: {}", child.name, child.id);

        Ok(CreateChildResult { child })
    }

    /// Get a child by ID
    pub async fn get_child(&self, command: GetChildCommand) -> Result<GetChildResult> {
        info!("Getting child: {}", command.child_id);

        let child = self.child_repository.get_child(&command.child_id).await?;

        if child.is_some() {
            info!("Found child: {}", command.child_id);
        } else {
            warn!("Child not found: {}", command.child_id);
        }

        Ok(GetChildResult { child })
    }

    /// List all children
    pub async fn list_children(&self) -> Result<ListChildrenResult> {
        info!("Listing all children");

        let children = self.child_repository.list_children().await?;
        
        info!("Found {} children", children.len());

        Ok(ListChildrenResult { children })
    }

    /// Update an existing child
    pub async fn update_child(&self, command: UpdateChildCommand) -> Result<UpdateChildResult> {
        info!("Updating child: {}", command.child_id);

        // Get the existing child
        let mut child = self.child_repository.get_child(&command.child_id).await?
            .ok_or_else(|| anyhow::anyhow!("Child not found: {}", command.child_id))?;

        // Validate the update command
        self.validate_update_command(&command)?;

        // Update fields if provided
        if let Some(name) = command.name {
            child.name = name.trim().to_string();
        }
        if let Some(birthdate_str) = command.birthdate {
            child.birthdate = NaiveDate::parse_from_str(&birthdate_str, "%Y-%m-%d")
                .context("Invalid birthdate format in update_child command")?;
        }

        // Update timestamp
        child.updated_at = Utc::now();

        // Store updated child
        self.child_repository.update_child(&child).await?;

        info!("Updated child: {} with ID: {}", child.name, child.id);

        Ok(UpdateChildResult { child })
    }

    /// Delete a child
    pub async fn delete_child(&self, command: DeleteChildCommand) -> Result<DeleteChildResult> {
        info!("Deleting child: {}", command.child_id);

        // Verify child exists
        let child = self.child_repository.get_child(&command.child_id).await?
            .ok_or_else(|| anyhow::anyhow!("Child not found: {}", command.child_id))?;

        // Delete from database
        self.child_repository.delete_child(&command.child_id).await?;

        info!("Deleted child: {} with ID: {}", child.name, child.id);

        Ok(DeleteChildResult {
            success_message: format!("Child '{}' deleted successfully", child.name),
        })
    }

    /// Get the currently active child
    pub async fn get_active_child(&self) -> Result<GetActiveChildResult> {
        debug!("Getting active child");

        let active_child_id = self.child_repository.get_active_child().await?;

        let active_child_model = if let Some(child_id) = active_child_id {
            match self.child_repository.get_child(&child_id).await? {
                Some(child) => {
                    debug!("Found active child: {}", child_id);
                    Some(child)
                },
                None => {
                    warn!("Active child ID exists but child not found: {}", child_id);
                    None
                }
            }
        } else {
            info!("No active child set");
            None
        };

        Ok(GetActiveChildResult {
            active_child: ActiveChild { child: active_child_model },
        })
    }

    /// Set the active child
    pub async fn set_active_child(&self, command: SetActiveChildCommand) -> Result<SetActiveChildResult> {
        info!("Setting active child: {}", command.child_id);

        // Validate that the child exists
        let domain_child = self.child_repository.get_child(&command.child_id).await?
            .ok_or_else(|| anyhow::anyhow!("Child not found: {}", command.child_id))?;

        // Set as active child in database
        self.child_repository.set_active_child(&command.child_id).await?;
        
        info!("Successfully set active child: {} ({})", domain_child.name, domain_child.id);

        Ok(SetActiveChildResult { child: domain_child })
    }

    /// Validate create child command
    fn validate_create_command(&self, command: &CreateChildCommand) -> Result<()> {
        // Validate name
        if command.name.trim().is_empty() {
            return Err(anyhow::anyhow!("Child name cannot be empty"));
        }

        if command.name.len() > 100 {
            return Err(anyhow::anyhow!("Child name cannot exceed 100 characters"));
        }

        // Validate birthdate format (ISO 8601: YYYY-MM-DD)
        self.validate_birthdate(&command.birthdate)?;

        Ok(())
    }

    /// Validate update child command
    fn validate_update_command(&self, command: &UpdateChildCommand) -> Result<()> {
        // Validate name if provided
        if let Some(ref name) = command.name {
            if name.trim().is_empty() {
                return Err(anyhow::anyhow!("Child name cannot be empty"));
            }

            if name.len() > 100 {
                return Err(anyhow::anyhow!("Child name cannot exceed 100 characters"));
            }
        }

        // Validate birthdate if provided
        if let Some(ref birthdate) = command.birthdate {
            self.validate_birthdate(birthdate)?;
        }

        Ok(())
    }

    /// Validate birthdate format
    fn validate_birthdate(&self, birthdate: &str) -> Result<()> {
        let date_parts: Vec<&str> = birthdate.split('-').collect();
        if date_parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid birthdate format. Use YYYY-MM-DD."));
        }

        let year: u32 = date_parts[0].parse()
            .map_err(|_| anyhow::anyhow!("Invalid year in birthdate"))?;
        let month: u32 = date_parts[1].parse()
            .map_err(|_| anyhow::anyhow!("Invalid month in birthdate"))?;
        let day: u32 = date_parts[2].parse()
            .map_err(|_| anyhow::anyhow!("Invalid day in birthdate"))?;

        if year < 1900 || year > 2100 {
            return Err(anyhow::anyhow!("Year must be between 1900 and 2100"));
        }
        if !(1..=12).contains(&month) {
            return Err(anyhow::anyhow!("Month must be between 1 and 12"));
        }
        if !(1..=31).contains(&day) {
            return Err(anyhow::anyhow!("Day must be between 1 and 31"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::path::Path;
    use crate::backend::domain::commands::child::{
        CreateChildCommand, UpdateChildCommand, GetChildCommand,
        DeleteChildCommand, SetActiveChildCommand
    };

    async fn setup_test() -> ChildService {
        let temp_dir = tempdir().unwrap();
        let conn = CsvConnection::new(temp_dir.path().to_path_buf()).await.unwrap();
        ChildService::new(Arc::new(conn))
    }

    #[tokio::test]
    async fn test_create_child() {
        let service = setup_test().await;
        let command = CreateChildCommand {
            name: "  Test Child ".to_string(),
            birthdate: "2015-05-20".to_string(),
        };

        let result = service.create_child(command).await.unwrap();
        assert_eq!(result.child.name, "Test Child");
        assert_eq!(result.child.birthdate.to_string(), "2015-05-20");
    }

    #[tokio::test]
    async fn test_create_child_validation() {
        let service = setup_test().await;
        
        let cmd_empty_name = CreateChildCommand { name: " ".to_string(), birthdate: "2010-01-01".to_string() };
        assert!(service.create_child(cmd_empty_name).await.is_err());

        let cmd_long_name = CreateChildCommand { name: "a".repeat(101), birthdate: "2010-01-01".to_string() };
        assert!(service.create_child(cmd_long_name).await.is_err());
        
        let cmd_bad_date = CreateChildCommand { name: "Bad Date".to_string(), birthdate: "2010/01/01".to_string() };
        assert!(service.create_child(cmd_bad_date).await.is_err());
    }

    #[tokio::test]
    async fn test_get_child() {
        let service = setup_test().await;
        let create_cmd = CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2010-01-01".to_string(),
        };
        let created_child_result = service.create_child(create_cmd).await.unwrap();
        
        let get_cmd = GetChildCommand { child_id: created_child_result.child.id.clone() };
        let retrieved_child_result = service.get_child(get_cmd).await.unwrap();
        let retrieved_child = retrieved_child_result.child.unwrap();
        assert_eq!(retrieved_child.id, created_child_result.child.id);
        assert_eq!(retrieved_child.name, "Test Child");

        let children_result = service.list_children().await.unwrap();
        assert_eq!(children_result.children.len(), 1);
        assert_eq!(children_result.children[0].name, "Test Child");
    }

    #[tokio::test]
    async fn test_get_nonexistent_child() {
        let service = setup_test().await;
        let get_cmd = GetChildCommand { child_id: "non-existent-id".to_string() };
        let result = service.get_child(get_cmd).await.unwrap();
        assert!(result.child.is_none());
    }

    #[tokio::test]
    async fn test_list_children() {
        let service = setup_test().await;

        // Create a few children
        let cmd1 = CreateChildCommand { name: "Alice".to_string(), birthdate: "2010-01-01".to_string() };
        let cmd2 = CreateChildCommand { name: "Bob".to_string(), birthdate: "2012-02-02".to_string() };
        service.create_child(cmd1).await.unwrap();
        service.create_child(cmd2).await.unwrap();

        let response = service.list_children().await.unwrap();

        assert_eq!(response.children.len(), 2);
        assert!(response.children.iter().any(|c| c.name == "Alice"));
        assert!(response.children.iter().any(|c| c.name == "Bob"));
    }

    #[tokio::test]
    async fn test_update_child() {
        let service = setup_test().await;
        let create_cmd = CreateChildCommand {
            name: "Original Name".to_string(),
            birthdate: "2010-01-01".to_string(),
        };
        let created_child_result = service.create_child(create_cmd).await.unwrap();

        let update_cmd = UpdateChildCommand {
            child_id: created_child_result.child.id.clone(),
            name: Some("  Updated Name  ".to_string()),
            birthdate: Some("2011-02-02".to_string()),
        };

        let updated_child_result = service.update_child(update_cmd).await.unwrap();
        assert_eq!(updated_child_result.child.name, "Updated Name");
        assert_eq!(updated_child_result.child.birthdate.to_string(), "2011-02-02");
        assert!(updated_child_result.child.updated_at > created_child_result.child.created_at);
    }

    #[tokio::test]
    async fn test_update_nonexistent_child() {
        let service = setup_test().await;
        let update_cmd = UpdateChildCommand {
            child_id: "non-existent-id".to_string(),
            name: Some("New Name".to_string()),
            birthdate: None,
        };
        let result = service.update_child(update_cmd).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_child() {
        let service = setup_test().await;
        let create_cmd = CreateChildCommand {
            name: "To Be Deleted".to_string(),
            birthdate: "2010-01-01".to_string(),
        };
        let created_child_result = service.create_child(create_cmd).await.unwrap();

        let delete_cmd = DeleteChildCommand { child_id: created_child_result.child.id.clone() };
        service.delete_child(delete_cmd).await.unwrap();
        
        let get_cmd = GetChildCommand { child_id: created_child_result.child.id };
        let retrieved_child_result = service.get_child(get_cmd).await.unwrap();
        assert!(retrieved_child_result.child.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_child() {
        let service = setup_test().await;
        let delete_cmd = DeleteChildCommand { child_id: "non-existent-id".to_string() };
        let result = service.delete_child(delete_cmd).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_birthdate() {
        let service = setup_test().await;
        service.validate_birthdate("2020-01-15").unwrap();
        service.validate_birthdate("not-a-date").unwrap_err();
        service.validate_birthdate("2020-13-01").unwrap_err(); // Invalid month
        service.validate_birthdate("2020-02-30").unwrap_err(); // Invalid day
    }

    /// Get active child when none is set
    #[tokio::test]
    async fn test_get_active_child_when_none_set() {
        let service = setup_test().await;
        let response = service.get_active_child().await.unwrap();
        assert!(response.active_child.child.is_none());
    }

    /// Set and get the active child
    #[tokio::test]
    async fn test_set_and_get_active_child() {
        let service = setup_test().await;

        // Create a child first
        let create_cmd = CreateChildCommand { name: "Charlie".to_string(), birthdate: "2015-03-03".to_string() };
        let created_child_resp = service.create_child(create_cmd).await.unwrap();
        let child_id = created_child_resp.child.id;

        // Set the active child
        let set_active_cmd = SetActiveChildCommand { child_id: child_id.clone() };
        let _ = service.set_active_child(set_active_cmd).await.unwrap();

        // Get the active child
        let active_child_resp = service.get_active_child().await.unwrap();
        
        assert!(active_child_resp.active_child.child.is_some());
        let active_child = active_child_resp.active_child.child.unwrap();
        assert_eq!(active_child.id, child_id);
        assert_eq!(active_child.name, "Charlie");
    }

    /// Try to set a non-existent child as active
    #[tokio::test]
    async fn test_set_active_child_with_nonexistent_child() {
        let service = setup_test().await;
        let set_active_cmd = SetActiveChildCommand { child_id: "non-existent-id".to_string() };
        let result = service.set_active_child(set_active_cmd).await;
        assert!(result.is_err());
    }

    /// Test that the active child is correctly updated when a new child is set as active
    #[tokio::test]
    async fn test_update_active_child() {
        let service = setup_test().await;

        // Create two children
        let child1_cmd = CreateChildCommand { name: "Dave".to_string(), birthdate: "2018-04-04".to_string() };
        let child1_resp = service.create_child(child1_cmd).await.unwrap();
        let child1_id = child1_resp.child.id;

        let child2_cmd = CreateChildCommand { name: "Eve".to_string(), birthdate: "2020-05-05".to_string() };
        let child2_resp = service.create_child(child2_cmd).await.unwrap();
        let child2_id = child2_resp.child.id;

        // Set child1 as active
        let set_active_cmd1 = SetActiveChildCommand { child_id: child1_id.clone() };
        service.set_active_child(set_active_cmd1).await.unwrap();
        let active_child_resp1 = service.get_active_child().await.unwrap();
        assert_eq!(active_child_resp1.active_child.child.as_ref().unwrap().id, child1_id);

        // Set child2 as active
        let set_active_cmd2 = SetActiveChildCommand { child_id: child2_id.clone() };
        service.set_active_child(set_active_cmd2).await.unwrap();
        let active_child_resp2 = service.get_active_child().await.unwrap();
        assert_eq!(active_child_resp2.active_child.child.as_ref().unwrap().id, child2_id);
        assert_eq!(active_child_resp2.active_child.child.as_ref().unwrap().name, "Eve");
    }

    /// Ensure get_active_child returns None if the active child has been deleted
    #[tokio::test]
    async fn test_active_child_after_child_deletion() {
        let service = setup_test().await;

        // Create a child and set it as active
        let create_cmd = CreateChildCommand { name: "Frank".to_string(), birthdate: "2021-06-06".to_string() };
        let created_child_resp = service.create_child(create_cmd).await.unwrap();
        let child_id = created_child_resp.child.id;
        let set_active_cmd = SetActiveChildCommand { child_id: child_id.clone() };
        service.set_active_child(set_active_cmd).await.unwrap();

        // Ensure it's the active child
        let active_child_resp = service.get_active_child().await.unwrap();
        assert_eq!(active_child_resp.active_child.child.as_ref().unwrap().id, child_id);

        // Delete the child
        let delete_cmd = DeleteChildCommand { child_id: child_id.clone() };
        service.delete_child(delete_cmd).await.unwrap();

        // Now, getting active child should return None as the underlying child is gone
        let active_child_resp_after_delete = service.get_active_child().await.unwrap();
        assert!(active_child_resp_after_delete.active_child.child.is_none());
    }
}