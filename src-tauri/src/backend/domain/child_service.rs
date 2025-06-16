use anyhow::Result;
use chrono::Utc;
use log::{info, warn};
use std::sync::Arc;

use crate::backend::storage::db::DbConnection;
use shared::{
    Child, CreateChildRequest, UpdateChildRequest, ChildResponse, ChildListResponse
};

/// Service for managing children in the allowance tracking system
#[derive(Clone)]
pub struct ChildService {
    db: Arc<DbConnection>,
}

impl ChildService {
    /// Create a new ChildService
    pub fn new(db: Arc<DbConnection>) -> Self {
        Self { db }
    }

    /// Create a new child
    pub async fn create_child(&self, request: CreateChildRequest) -> Result<ChildResponse> {
        info!("Creating child: name={}, birthdate={}", request.name, request.birthdate);

        // Validate the request
        self.validate_create_request(&request)?;

        // Generate timestamps
        let now = Utc::now();
        let timestamp_millis = now.timestamp_millis() as u64;
        let timestamp_rfc3339 = now.to_rfc3339();

        // Create the child
        let child = Child {
            id: Child::generate_id(timestamp_millis),
            name: request.name.trim().to_string(),
            birthdate: request.birthdate,
            created_at: timestamp_rfc3339.clone(),
            updated_at: timestamp_rfc3339,
        };

        // Store in database
        self.db.store_child(&child).await?;

        info!("Created child: {} with ID: {}", child.name, child.id);

        Ok(ChildResponse {
            child,
            success_message: "Child created successfully".to_string(),
        })
    }

    /// Get a child by ID
    pub async fn get_child(&self, child_id: &str) -> Result<Option<Child>> {
        info!("Getting child: {}", child_id);

        let child = self.db.get_child(child_id).await?;

        if child.is_some() {
            info!("Found child: {}", child_id);
        } else {
            warn!("Child not found: {}", child_id);
        }

        Ok(child)
    }

    /// List all children
    pub async fn list_children(&self) -> Result<ChildListResponse> {
        info!("Listing all children");

        let children = self.db.list_children().await?;

        info!("Found {} children", children.len());

        Ok(ChildListResponse { children })
    }

    /// Update an existing child
    pub async fn update_child(&self, child_id: &str, request: UpdateChildRequest) -> Result<ChildResponse> {
        info!("Updating child: {}", child_id);

        // Get the existing child
        let mut child = self.db.get_child(child_id).await?
            .ok_or_else(|| anyhow::anyhow!("Child not found: {}", child_id))?;

        // Validate the update request
        self.validate_update_request(&request)?;

        // Update fields if provided
        if let Some(name) = request.name {
            child.name = name.trim().to_string();
        }
        if let Some(birthdate) = request.birthdate {
            child.birthdate = birthdate;
        }

        // Update timestamp
        child.updated_at = Utc::now().to_rfc3339();

        // Store updated child
        self.db.update_child(&child).await?;

        info!("Updated child: {} with ID: {}", child.name, child.id);

        Ok(ChildResponse {
            child,
            success_message: "Child updated successfully".to_string(),
        })
    }

    /// Delete a child
    pub async fn delete_child(&self, child_id: &str) -> Result<()> {
        info!("Deleting child: {}", child_id);

        // Verify child exists
        let child = self.db.get_child(child_id).await?
            .ok_or_else(|| anyhow::anyhow!("Child not found: {}", child_id))?;

        // Delete from database
        self.db.delete_child(child_id).await?;

        info!("Deleted child: {} with ID: {}", child.name, child.id);

        Ok(())
    }

    /// Validate create child request
    fn validate_create_request(&self, request: &CreateChildRequest) -> Result<()> {
        // Validate name
        if request.name.trim().is_empty() {
            return Err(anyhow::anyhow!("Child name cannot be empty"));
        }

        if request.name.len() > 100 {
            return Err(anyhow::anyhow!("Child name cannot exceed 100 characters"));
        }

        // Validate birthdate format (ISO 8601: YYYY-MM-DD)
        self.validate_birthdate(&request.birthdate)?;

        Ok(())
    }

    /// Validate update child request
    fn validate_update_request(&self, request: &UpdateChildRequest) -> Result<()> {
        // Validate name if provided
        if let Some(ref name) = request.name {
            if name.trim().is_empty() {
                return Err(anyhow::anyhow!("Child name cannot be empty"));
            }

            if name.len() > 100 {
                return Err(anyhow::anyhow!("Child name cannot exceed 100 characters"));
            }
        }

        // Validate birthdate if provided
        if let Some(ref birthdate) = request.birthdate {
            self.validate_birthdate(birthdate)?;
        }

        Ok(())
    }

    /// Validate birthdate format
    fn validate_birthdate(&self, birthdate: &str) -> Result<()> {
        // Check basic format (YYYY-MM-DD)
        if birthdate.len() != 10 {
            return Err(anyhow::anyhow!("Birthdate must be in YYYY-MM-DD format"));
        }

        let parts: Vec<&str> = birthdate.split('-').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Birthdate must be in YYYY-MM-DD format"));
        }

        // Validate year
        let year: u32 = parts[0].parse()
            .map_err(|_| anyhow::anyhow!("Invalid year in birthdate"))?;
        
        if year < 1900 || year > 2100 {
            return Err(anyhow::anyhow!("Year must be between 1900 and 2100"));
        }

        // Validate month
        let month: u32 = parts[1].parse()
            .map_err(|_| anyhow::anyhow!("Invalid month in birthdate"))?;
        
        if month < 1 || month > 12 {
            return Err(anyhow::anyhow!("Month must be between 1 and 12"));
        }

        // Validate day
        let day: u32 = parts[2].parse()
            .map_err(|_| anyhow::anyhow!("Invalid day in birthdate"))?;
        
        if day < 1 || day > 31 {
            return Err(anyhow::anyhow!("Day must be between 1 and 31"));
        }

        // Additional validation for days in month (simplified)
        match month {
            2 => {
                // February - check for leap year
                let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
                let max_day = if is_leap { 29 } else { 28 };
                if day > max_day {
                    return Err(anyhow::anyhow!("Invalid day for February"));
                }
            },
            4 | 6 | 9 | 11 => {
                // April, June, September, November have 30 days
                if day > 30 {
                    return Err(anyhow::anyhow!("Invalid day for month {}", month));
                }
            },
            _ => {
                // Other months have 31 days (already validated above)
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::db::DbConnection;

    async fn setup_test() -> ChildService {
        let db = Arc::new(DbConnection::init_test().await.expect("Failed to create test database"));
        ChildService::new(db)
    }

    #[tokio::test]
    async fn test_create_child() {
        let service = setup_test().await;

        let request = CreateChildRequest {
            name: "Alice Smith".to_string(),
            birthdate: "2015-06-15".to_string(),
        };

        let response = service.create_child(request).await.expect("Failed to create child");
        
        assert_eq!(response.child.name, "Alice Smith");
        assert_eq!(response.child.birthdate, "2015-06-15");
        assert!(!response.child.id.is_empty());
        assert!(!response.child.created_at.is_empty());
        assert!(!response.child.updated_at.is_empty());
        assert_eq!(response.success_message, "Child created successfully");
    }

    #[tokio::test]
    async fn test_create_child_validation() {
        let service = setup_test().await;

        // Test empty name
        let request = CreateChildRequest {
            name: "".to_string(),
            birthdate: "2015-06-15".to_string(),
        };
        assert!(service.create_child(request).await.is_err());

        // Test invalid birthdate
        let request = CreateChildRequest {
            name: "Alice".to_string(),
            birthdate: "invalid-date".to_string(),
        };
        assert!(service.create_child(request).await.is_err());

        // Test invalid month
        let request = CreateChildRequest {
            name: "Alice".to_string(),
            birthdate: "2015-13-15".to_string(),
        };
        assert!(service.create_child(request).await.is_err());

        // Test invalid day for February
        let request = CreateChildRequest {
            name: "Alice".to_string(),
            birthdate: "2015-02-30".to_string(),
        };
        assert!(service.create_child(request).await.is_err());
    }

    #[tokio::test]
    async fn test_get_child() {
        let service = setup_test().await;

        // Create a child first
        let request = CreateChildRequest {
            name: "Bob Johnson".to_string(),
            birthdate: "2012-03-20".to_string(),
        };
        let response = service.create_child(request).await.expect("Failed to create child");
        let child_id = response.child.id.clone();

        // Get the child
        let child = service.get_child(&child_id).await.expect("Failed to get child");
        assert!(child.is_some());
        
        let child = child.unwrap();
        assert_eq!(child.name, "Bob Johnson");
        assert_eq!(child.birthdate, "2012-03-20");
    }

    #[tokio::test]
    async fn test_get_nonexistent_child() {
        let service = setup_test().await;

        let child = service.get_child("child::nonexistent").await.expect("Failed to query child");
        assert!(child.is_none());
    }

    #[tokio::test]
    async fn test_list_children() {
        let service = setup_test().await;

        // Initially should have no children
        let response = service.list_children().await.expect("Failed to list children");
        assert_eq!(response.children.len(), 0);

        // Create two children with a small delay to ensure different timestamps
        let request1 = CreateChildRequest {
            name: "Bob Johnson".to_string(),
            birthdate: "2012-03-20".to_string(),
        };
        service.create_child(request1).await.expect("Failed to create child1");

        // Small delay to ensure different timestamp
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

        let request2 = CreateChildRequest {
            name: "Alice Smith".to_string(),
            birthdate: "2015-06-15".to_string(),
        };
        service.create_child(request2).await.expect("Failed to create child2");

        // List children
        let response = service.list_children().await.expect("Failed to list children");
        assert_eq!(response.children.len(), 2);
        
        // Should be ordered by name: Alice, Bob
        assert_eq!(response.children[0].name, "Alice Smith");
        assert_eq!(response.children[1].name, "Bob Johnson");
    }

    #[tokio::test]
    async fn test_update_child() {
        let service = setup_test().await;

        // Create a child first
        let request = CreateChildRequest {
            name: "Original Name".to_string(),
            birthdate: "2015-06-15".to_string(),
        };
        let response = service.create_child(request).await.expect("Failed to create child");
        let child_id = response.child.id.clone();
        let original_created_at = response.child.created_at.clone();

        // Update the child
        let update_request = UpdateChildRequest {
            name: Some("Updated Name".to_string()),
            birthdate: Some("2015-07-20".to_string()),
        };
        let update_response = service.update_child(&child_id, update_request).await.expect("Failed to update child");

        assert_eq!(update_response.child.name, "Updated Name");
        assert_eq!(update_response.child.birthdate, "2015-07-20");
        assert_eq!(update_response.child.created_at, original_created_at); // Should remain unchanged
        assert_ne!(update_response.child.updated_at, original_created_at); // Should be updated
        assert_eq!(update_response.success_message, "Child updated successfully");
    }

    #[tokio::test]
    async fn test_update_nonexistent_child() {
        let service = setup_test().await;

        let update_request = UpdateChildRequest {
            name: Some("Updated Name".to_string()),
            birthdate: None,
        };
        
        let result = service.update_child("child::nonexistent", update_request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_child() {
        let service = setup_test().await;

        // Create a child first
        let request = CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-06-15".to_string(),
        };
        let response = service.create_child(request).await.expect("Failed to create child");
        let child_id = response.child.id.clone();

        // Verify child exists
        let child = service.get_child(&child_id).await.expect("Failed to get child");
        assert!(child.is_some());

        // Delete the child
        service.delete_child(&child_id).await.expect("Failed to delete child");

        // Verify child no longer exists
        let child = service.get_child(&child_id).await.expect("Failed to query child");
        assert!(child.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_child() {
        let service = setup_test().await;

        let result = service.delete_child("child::nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_birthdate() {
        let service = setup_test().await;

        // Valid dates
        assert!(service.validate_birthdate("2015-06-15").is_ok());
        assert!(service.validate_birthdate("2000-02-29").is_ok()); // Leap year
        assert!(service.validate_birthdate("1999-02-28").is_ok());

        // Invalid formats
        assert!(service.validate_birthdate("2015/06/15").is_err());
        assert!(service.validate_birthdate("15-06-2015").is_err());
        assert!(service.validate_birthdate("2015-6-15").is_err());

        // Invalid dates
        assert!(service.validate_birthdate("2015-13-15").is_err()); // Invalid month
        assert!(service.validate_birthdate("2015-06-32").is_err()); // Invalid day
        assert!(service.validate_birthdate("2015-02-30").is_err()); // Invalid day for February
        assert!(service.validate_birthdate("2015-04-31").is_err()); // Invalid day for April
        assert!(service.validate_birthdate("1899-06-15").is_err()); // Year too early
        assert!(service.validate_birthdate("2101-06-15").is_err()); // Year too late
    }
} 