use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use shared::sync::*;
use super::table_config::TableConfig;
use std::collections::HashMap;

pub struct DynamoStore {
    client: Client,
    config: TableConfig,
}

impl DynamoStore {
    pub fn new(client: Client, config: TableConfig) -> Self {
        Self { client, config }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn config(&self) -> &TableConfig {
        &self.config
    }

    /// Initialize child metadata with event_sequence=0, local_watermark=0, remote_watermark=0.
    /// Idempotent: uses condition attribute_not_exists(child_id) to prevent overwrites.
    pub async fn initialize_child_metadata(&self, child_id: &str) -> anyhow::Result<()> {
        let table = self.config.sync_metadata.clone();

        let item = HashMap::from([
            ("child_id".to_string(), AttributeValue::S(child_id.to_string())),
            ("event_sequence".to_string(), AttributeValue::N("0".to_string())),
            ("local_watermark".to_string(), AttributeValue::N("0".to_string())),
            ("remote_watermark".to_string(), AttributeValue::N("0".to_string())),
        ]);

        let result = self.client
            .put_item()
            .table_name(&table)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(child_id)")
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                if let Some(service_err) = e.as_service_error() {
                    if service_err.is_conditional_check_failed_exception() {
                        return Ok(());
                    }
                }
                Err(anyhow::anyhow!("Failed to initialize child metadata: {}", aws_sdk_dynamodb::error::DisplayErrorContext(&e)))
            }
        }
    }

    /// Push a sync event: check for duplicates, increment sequence counter atomically,
    /// and write event. Returns the assigned sequence number.
    pub async fn push_event(&self, event: &SyncEvent) -> anyhow::Result<u64> {
        // Check for duplicate
        if let Some(existing_seq) = self.find_event_by_id(&event.child_id, &event.event_id).await? {
            return Ok(existing_seq);
        }

        // Atomically increment sequence counter
        let metadata_table = self.config.sync_metadata.clone();
        let update_response = self.client
            .update_item()
            .table_name(&metadata_table)
            .key("child_id", AttributeValue::S(event.child_id.clone()))
            .update_expression("SET event_sequence = event_sequence + :inc")
            .expression_attribute_values(":inc", AttributeValue::N("1".to_string()))
            .condition_expression("attribute_exists(event_sequence)")
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllNew)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to increment sequence: {}", e))?;

        // Extract the new sequence number
        let new_sequence = update_response
            .attributes()
            .and_then(|attrs| attrs.get("event_sequence"))
            .and_then(|attr| attr.as_n().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| anyhow::anyhow!("Failed to parse new sequence number"))?;

        // Write event to sync_events table with the new sequence
        let events_table = self.config.sync_events.clone();
        let event_item = self.event_to_item(event, new_sequence);

        self.client
            .put_item()
            .table_name(&events_table)
            .set_item(Some(event_item))
            .condition_expression("attribute_not_exists(#seq)")
            .expression_attribute_names("#seq", "sequence")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write event: {}", aws_sdk_dynamodb::error::DisplayErrorContext(&e)))?;

        Ok(new_sequence)
    }

    /// Find an event by its event_id, returning its sequence number if found.
    pub async fn find_event_by_id(&self, child_id: &str, event_id: &str) -> anyhow::Result<Option<u64>> {
        let table = self.config.sync_events.clone();

        let response = self.client
            .query()
            .table_name(&table)
            .key_condition_expression("child_id = :cid")
            .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
            .filter_expression("event_id = :eid")
            .expression_attribute_values(":eid", AttributeValue::S(event_id.to_string()))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query for event: {}", e))?;

        if let Some(mut items) = response.items {
            if let Some(item) = items.pop() {
                if let Some(seq_attr) = item.get("sequence") {
                    if let Ok(seq_str) = seq_attr.as_n() {
                        if let Ok(seq) = seq_str.parse::<u64>() {
                            return Ok(Some(seq));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Get all events for a child since a given sequence number (exclusive).
    pub async fn get_events_since(&self, child_id: &str, since_sequence: u64) -> anyhow::Result<Vec<SyncEvent>> {
        let table = self.config.sync_events.clone();

        let response = self.client
            .query()
            .table_name(&table)
            .key_condition_expression("child_id = :cid AND #seq > :since")
            .expression_attribute_names("#seq", "sequence")
            .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
            .expression_attribute_values(":since", AttributeValue::N(since_sequence.to_string()))
            .scan_index_forward(true)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query events: {}", e))?;

        let mut events = Vec::new();
        if let Some(items) = response.items {
            for item in items {
                events.push(self.parse_sync_event(&item)?);
            }
        }

        Ok(events)
    }

    /// Parse a DynamoDB item into a SyncEvent.
    pub fn parse_sync_event(&self, item: &HashMap<String, AttributeValue>) -> anyhow::Result<SyncEvent> {
        let event_id = item
            .get("event_id")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Missing event_id"))?;

        let entity_type_str = item
            .get("entity_type")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| anyhow::anyhow!("Missing entity_type"))?;
        let entity_type = EntityType::from_str(entity_type_str)
            .map_err(|e| anyhow::anyhow!("Invalid entity_type: {}", e))?;

        let entity_id = item
            .get("entity_id")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Missing entity_id"))?;

        let child_id = item
            .get("child_id")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Missing child_id"))?;

        let action_str = item
            .get("action")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| anyhow::anyhow!("Missing action"))?;
        let action = SyncAction::from_str(action_str)
            .map_err(|e| anyhow::anyhow!("Invalid action: {}", e))?;

        let source_str = item
            .get("source")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| anyhow::anyhow!("Missing source"))?;
        let source = match source_str.as_ref() {
            "local" => SyncSource::Local,
            "remote" => SyncSource::Remote,
            _ => return Err(anyhow::anyhow!("Invalid source: {}", source_str)),
        };

        let source_timestamp_str = item
            .get("source_timestamp")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| anyhow::anyhow!("Missing source_timestamp"))?;
        let source_timestamp = chrono::DateTime::parse_from_rfc3339(source_timestamp_str)
            .map_err(|e| anyhow::anyhow!("Invalid source_timestamp: {}", e))?
            .with_timezone(&chrono::Utc);

        let sequence = item
            .get("sequence")
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<u64>().ok());

        Ok(SyncEvent {
            event_id,
            entity_type,
            entity_id,
            child_id,
            action,
            source,
            source_timestamp,
            sequence,
        })
    }

    /// Convert a SyncEvent to a DynamoDB item with a sequence number.
    fn event_to_item(&self, event: &SyncEvent, sequence: u64) -> HashMap<String, AttributeValue> {
        HashMap::from([
            ("child_id".to_string(), AttributeValue::S(event.child_id.clone())),
            ("sequence".to_string(), AttributeValue::N(sequence.to_string())),
            ("event_id".to_string(), AttributeValue::S(event.event_id.clone())),
            ("entity_type".to_string(), AttributeValue::S(event.entity_type.as_str().to_string())),
            ("entity_id".to_string(), AttributeValue::S(event.entity_id.clone())),
            ("action".to_string(), AttributeValue::S(event.action.as_str().to_string())),
            ("source".to_string(), AttributeValue::S(match event.source {
                SyncSource::Local => "local",
                SyncSource::Remote => "remote",
            }.to_string())),
            ("source_timestamp".to_string(), AttributeValue::S(event.source_timestamp.to_rfc3339())),
        ])
    }

    // ============================================================================
    // Entity CRUD Operations
    // ============================================================================

    /// Upsert an entity (create or update).
    /// Uses entity_table_and_sort_key to get table name and sort key, stores entity_json in "data" attribute.
    pub async fn upsert_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str, entity_json: &str) -> anyhow::Result<()> {
        let (table, sort_key) = self.entity_table_and_sort_key(&entity_type);

        let mut item = HashMap::from([
            ("child_id".to_string(), AttributeValue::S(child_id.to_string())),
            ("data".to_string(), AttributeValue::S(entity_json.to_string())),
        ]);

        if let Some(sk_name) = sort_key {
            item.insert(sk_name.to_string(), AttributeValue::S(entity_id.to_string()));
        }

        // For transactions, extract date from JSON and store as sort_date for GSI
        if matches!(entity_type, EntityType::Transaction) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(entity_json) {
                if let Some(date_str) = parsed.get("date").and_then(|v| v.as_str()) {
                    item.insert("sort_date".to_string(), AttributeValue::S(date_str.to_string()));
                }
            }
        }

        self.client
            .put_item()
            .table_name(&table)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to upsert entity: {}", e))?;

        Ok(())
    }

    /// Get an entity's JSON data by child_id, entity_type, and entity_id.
    /// Returns None if the entity doesn't exist.
    pub async fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> anyhow::Result<Option<String>> {
        let (table, sort_key) = self.entity_table_and_sort_key(&entity_type);

        let mut key_map = HashMap::from([
            ("child_id".to_string(), AttributeValue::S(child_id.to_string())),
        ]);

        if let Some(sk_name) = sort_key {
            key_map.insert(sk_name.to_string(), AttributeValue::S(entity_id.to_string()));
        }

        let response = self.client
            .get_item()
            .table_name(&table)
            .set_key(Some(key_map))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get entity: {}", e))?;

        match response.item {
            Some(item) => {
                let data = item
                    .get("data")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string());
                Ok(data)
            }
            None => Ok(None),
        }
    }

    /// Delete an entity by child_id, entity_type, and entity_id.
    pub async fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> anyhow::Result<()> {
        let (table, sort_key) = self.entity_table_and_sort_key(&entity_type);

        let mut key_map = HashMap::from([
            ("child_id".to_string(), AttributeValue::S(child_id.to_string())),
        ]);

        if let Some(sk_name) = sort_key {
            key_map.insert(sk_name.to_string(), AttributeValue::S(entity_id.to_string()));
        }

        self.client
            .delete_item()
            .table_name(&table)
            .set_key(Some(key_map))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete entity: {}", e))?;

        Ok(())
    }

    /// List all entities in a table (used for children which have no sort key).
    /// Returns a vec of (entity_id, entity_json) pairs.
    pub async fn list_all_entities_in_table(&self, entity_type: EntityType) -> anyhow::Result<Vec<(String, String)>> {
        let (table, _sort_key) = self.entity_table_and_sort_key(&entity_type);

        let response = self.client
            .scan()
            .table_name(&table)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scan table: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = response.items {
            for item in items {
                let child_id = item
                    .get("child_id")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let data = item
                    .get("data")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                results.push((child_id, data));
            }
        }

        Ok(results)
    }

    /// List all entities for a child_id in a table with a sort key.
    /// Returns a vec of (entity_id, entity_json) pairs.
    pub async fn list_entities_for_child(&self, child_id: &str, entity_type: EntityType) -> anyhow::Result<Vec<(String, String)>> {
        self.list_entities_for_child_with_options(child_id, entity_type, None, None).await
    }

    /// List entities for a child with optional sort direction and limit.
    /// sort_direction: "asc" (default) or "desc". Maps to DynamoDB ScanIndexForward.
    /// limit: max items to return.
    pub async fn list_entities_for_child_with_options(
        &self,
        child_id: &str,
        entity_type: EntityType,
        sort_direction: Option<&str>,
        limit: Option<i32>,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let (table, sort_key) = self.entity_table_and_sort_key(&entity_type);

        let scan_forward = match sort_direction {
            Some("desc") => false,
            _ => true,
        };

        let mut query = self.client
            .query()
            .table_name(&table)
            .key_condition_expression("child_id = :cid")
            .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
            .scan_index_forward(scan_forward);

        if let Some(l) = limit {
            query = query.limit(l);
        }

        let response = query
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query table: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = response.items {
            for item in items {
                let entity_id = if let Some(sk_name) = sort_key {
                    item.get(sk_name)
                        .and_then(|v| v.as_s().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                } else {
                    item.get("child_id")
                        .and_then(|v| v.as_s().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                };
                let data = item
                    .get("data")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                results.push((entity_id, data));
            }
        }

        Ok(results)
    }

    /// Get the balance from the most recent transaction for a child.
    /// Queries the DateSortIndex GSI (child_id + sort_date) in descending order, limit 1.
    /// Returns None if no transactions exist.
    pub async fn get_latest_balance(&self, child_id: &str) -> anyhow::Result<Option<f64>> {
        let table = self.config.transactions.clone();

        let response = self.client
            .query()
            .table_name(&table)
            .index_name("DateSortIndex")
            .key_condition_expression("child_id = :cid")
            .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
            .scan_index_forward(false)
            .limit(1)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query DateSortIndex: {}", e))?;

        let balance = response.items
            .and_then(|items| items.into_iter().next())
            .and_then(|item| item.get("data").and_then(|v| v.as_s().ok()).map(|s| s.to_string()))
            .and_then(|json_str| serde_json::from_str::<serde_json::Value>(&json_str).ok())
            .and_then(|val| val.get("balance").and_then(|b| b.as_f64()));

        Ok(balance)
    }

    /// Map EntityType to table name and optional sort key name.
    fn entity_table_and_sort_key(&self, entity_type: &EntityType) -> (String, Option<&'static str>) {
        match entity_type {
            EntityType::Transaction => (self.config.transactions.clone(), Some("transaction_id")),
            EntityType::Goal => (self.config.goals.clone(), Some("goal_id")),
            EntityType::Child => (self.config.children.clone(), None),
        }
    }

    // ============================================================================
    // Checkpoint and Watermark Management
    // ============================================================================

    /// Get the checkpoint for a child.
    /// Returns error if child metadata doesn't exist.
    pub async fn get_checkpoint(&self, child_id: &str) -> anyhow::Result<SyncCheckpoint> {
        let table = self.config.sync_metadata.clone();

        let response = self.client
            .get_item()
            .table_name(&table)
            .key("child_id", AttributeValue::S(child_id.to_string()))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get checkpoint: {}", e))?;

        let item = response.item
            .ok_or_else(|| anyhow::anyhow!("Child metadata not found for child_id: {}", child_id))?;

        let event_sequence = item
            .get("event_sequence")
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let local_watermark = item
            .get("local_watermark")
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let remote_watermark = item
            .get("remote_watermark")
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        Ok(SyncCheckpoint {
            child_id: child_id.to_string(),
            event_sequence,
            local_watermark,
            remote_watermark,
        })
    }

    /// Update a watermark (local or remote) for a child.
    /// Uses conditional write: only updates if the new value is greater than the current value.
    /// Returns error if child metadata doesn't exist.
    /// ConditionalCheckFailedException is converted to Ok (watermark already >= value).
    pub async fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> anyhow::Result<()> {
        let table = self.config.sync_metadata.clone();
        let watermark_attr = match which {
            "local" => "local_watermark",
            "remote" => "remote_watermark",
            _ => return Err(anyhow::anyhow!("Invalid watermark type: {}", which)),
        };

        let result = self.client
            .update_item()
            .table_name(&table)
            .key("child_id", AttributeValue::S(child_id.to_string()))
            .update_expression(format!("SET #{} = :val", watermark_attr))
            .expression_attribute_names(format!("#{}", watermark_attr), watermark_attr)
            .expression_attribute_values(":val", AttributeValue::N(value.to_string()))
            .condition_expression(format!("attribute_exists(child_id) AND #{} < :val", watermark_attr))
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                // ConditionalCheckFailedException is expected when watermark already >= value
                if let Some(service_err) = e.as_service_error() {
                    if service_err.is_conditional_check_failed_exception() {
                        // Watermark already >= value, which is fine
                        return Ok(());
                    }
                }
                Err(anyhow::anyhow!("Failed to update watermark: {}", e))
            }
        }
    }
}
