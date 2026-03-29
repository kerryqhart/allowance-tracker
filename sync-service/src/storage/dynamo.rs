use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use shared::sync::*;
use super::table_definitions;
use std::collections::HashMap;

pub struct DynamoStore {
    client: Client,
    table_prefix: String,
}

impl DynamoStore {
    pub fn new(client: Client, table_prefix: String) -> Self {
        Self { client, table_prefix }
    }

    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn table_prefix(&self) -> &str {
        &self.table_prefix
    }

    pub async fn create_tables(&self) -> anyhow::Result<()> {
        table_definitions::create_all_tables(&self.client, &self.table_prefix).await
    }

    pub async fn delete_tables(&self) -> anyhow::Result<()> {
        table_definitions::delete_all_tables(&self.client, &self.table_prefix).await
    }

    /// Initialize child metadata with event_sequence=0, local_watermark=0, remote_watermark=0.
    /// Idempotent: uses condition attribute_not_exists(child_id) to prevent overwrites.
    pub async fn initialize_child_metadata(&self, child_id: &str) -> anyhow::Result<()> {
        let table = self.table_name("sync_metadata");

        let item = HashMap::from([
            ("child_id".to_string(), AttributeValue::S(child_id.to_string())),
            ("event_sequence".to_string(), AttributeValue::N("0".to_string())),
            ("local_watermark".to_string(), AttributeValue::N("0".to_string())),
            ("remote_watermark".to_string(), AttributeValue::N("0".to_string())),
        ]);

        self.client
            .put_item()
            .table_name(&table)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(child_id)")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize child metadata: {}", e))?;

        Ok(())
    }

    /// Push a sync event: check for duplicates, increment sequence counter atomically,
    /// and write event. Returns the assigned sequence number.
    pub async fn push_event(&self, event: &SyncEvent) -> anyhow::Result<u64> {
        // Check for duplicate
        if let Some(existing_seq) = self.find_event_by_id(&event.child_id, &event.event_id).await? {
            return Ok(existing_seq);
        }

        // Atomically increment sequence counter
        let metadata_table = self.table_name("sync_metadata");
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
        let events_table = self.table_name("sync_events");
        let event_item = self.event_to_item(event, new_sequence);

        self.client
            .put_item()
            .table_name(&events_table)
            .set_item(Some(event_item))
            .condition_expression("attribute_not_exists(sequence)")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write event: {}", e))?;

        Ok(new_sequence)
    }

    /// Find an event by its event_id, returning its sequence number if found.
    pub async fn find_event_by_id(&self, child_id: &str, event_id: &str) -> anyhow::Result<Option<u64>> {
        let table = self.table_name("sync_events");

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
        let table = self.table_name("sync_events");

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
}
