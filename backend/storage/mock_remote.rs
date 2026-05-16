use super::remote::RemoteStorage;
use anyhow::{anyhow, Result};
use shared::sync::*;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MockRemoteClient {
    events: Mutex<Vec<SyncEvent>>,
    entities: Mutex<HashMap<(String, String, String), String>>, // (child_id, entity_type_str, entity_id) -> json
    checkpoints: Mutex<HashMap<String, SyncCheckpoint>>,
    event_dedup: Mutex<HashMap<String, u64>>, // event_id -> sequence for deduplication
    next_sequence: Mutex<u64>,
    force_error: Mutex<Option<String>>,
}

impl MockRemoteClient {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            entities: Mutex::new(HashMap::new()),
            checkpoints: Mutex::new(HashMap::new()),
            event_dedup: Mutex::new(HashMap::new()),
            next_sequence: Mutex::new(1),
            force_error: Mutex::new(None),
        }
    }

    pub fn force_error(&self, msg: &str) {
        *self.force_error.lock().unwrap() = Some(msg.to_string());
    }

    pub fn clear_error(&self) {
        *self.force_error.lock().unwrap() = None;
    }

    fn check_error(&self) -> Result<()> {
        if let Some(msg) = self.force_error.lock().unwrap().as_ref() {
            return Err(anyhow!(msg.clone()));
        }
        Ok(())
    }

    fn append_event_internal(&self, event: SyncEvent) -> u64 {
        let mut event_dedup = self.event_dedup.lock().unwrap();
        let mut stored_events = self.events.lock().unwrap();
        let mut next_seq = self.next_sequence.lock().unwrap();

        if let Some(&existing_seq) = event_dedup.get(&event.event_id) {
            return existing_seq;
        }
        let seq = *next_seq;
        *next_seq += 1;
        let mut new_event = event;
        new_event.sequence = Some(seq);
        let id = new_event.event_id.clone();
        stored_events.push(new_event);
        event_dedup.insert(id, seq);
        seq
    }

    /// Test-only helper: insert a `SyncEvent` directly into the mock's event log,
    /// bypassing the trait surface. Used by tests that need to seed remote-origin
    /// or local-origin events without going through upsert/delete.
    #[cfg(test)]
    pub fn seed_event(&self, event: SyncEvent) -> u64 {
        self.append_event_internal(event)
    }
}

impl Default for MockRemoteClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteStorage for MockRemoteClient {
    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>> {
        self.check_error()?;

        let events = self.events.lock().unwrap();
        let filtered = events
            .iter()
            .filter(|e| e.child_id == child_id && e.sequence.map_or(false, |seq| seq > since_sequence))
            .cloned()
            .collect();
        Ok(filtered)
    }

    fn upsert_entity(
        &self,
        child_id: &str,
        entity_type: EntityType,
        entity_id: &str,
        entity_json: &str,
    ) -> Result<()> {
        self.check_error()?;

        let key = (
            child_id.to_string(),
            entity_type.as_str().to_string(),
            entity_id.to_string(),
        );

        let mut entities = self.entities.lock().unwrap();
        let prior = entities.get(&key).cloned();
        if prior.as_deref() == Some(entity_json) {
            // Identical content — no event, no-op. Mirrors server short-circuit.
            return Ok(());
        }
        let action = if prior.is_none() {
            SyncAction::Created
        } else {
            SyncAction::Updated
        };
        entities.insert(key, entity_json.to_string());
        drop(entities);

        // Emit event with deterministic id, source=Local (mirrors what the deployed
        // server will produce given an X-Sync-Source: local request — which is what
        // the http client now sends).
        let event_id = match action {
            SyncAction::Created => format!("ev::created::{entity_id}"),
            SyncAction::Updated => format!("ev::updated::{entity_id}::mock"),
            SyncAction::Deleted => unreachable!(),
        };
        let event = SyncEvent {
            event_id,
            entity_type,
            entity_id: entity_id.to_string(),
            child_id: child_id.to_string(),
            action,
            source: SyncSource::Local,
            source_timestamp: chrono::Utc::now(),
            sequence: None,
        };
        // Append via the infallible internal helper (dedup, sequence assignment).
        self.append_event_internal(event);
        Ok(())
    }

    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>> {
        self.check_error()?;

        let key = (
            child_id.to_string(),
            entity_type.as_str().to_string(),
            entity_id.to_string(),
        );
        Ok(self.entities.lock().unwrap().get(&key).cloned())
    }

    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()> {
        self.check_error()?;

        let key = (
            child_id.to_string(),
            entity_type.as_str().to_string(),
            entity_id.to_string(),
        );
        self.entities.lock().unwrap().remove(&key);

        let event = SyncEvent {
            event_id: format!("ev::deleted::{entity_id}"),
            entity_type,
            entity_id: entity_id.to_string(),
            child_id: child_id.to_string(),
            action: SyncAction::Deleted,
            source: SyncSource::Local,
            source_timestamp: chrono::Utc::now(),
            sequence: None,
        };
        self.append_event_internal(event);
        Ok(())
    }

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint> {
        self.check_error()?;

        let checkpoints = self.checkpoints.lock().unwrap();
        Ok(checkpoints
            .get(child_id)
            .cloned()
            .unwrap_or_else(|| SyncCheckpoint::new(child_id.to_string())))
    }

    fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> Result<()> {
        self.check_error()?;

        let mut checkpoints = self.checkpoints.lock().unwrap();
        let cp = checkpoints
            .entry(child_id.to_string())
            .or_insert_with(|| SyncCheckpoint::new(child_id.to_string()));

        match which {
            "local" => {
                if value > cp.local_watermark {
                    cp.local_watermark = value;
                }
            }
            "remote" => {
                if value > cp.remote_watermark {
                    cp.remote_watermark = value;
                }
            }
            _ => return Err(anyhow!("Unknown watermark type: {}", which)),
        }

        Ok(())
    }

    fn initialize_child(&self, child_id: &str) -> Result<()> {
        self.check_error()?;

        let mut checkpoints = self.checkpoints.lock().unwrap();
        checkpoints.insert(child_id.to_string(), SyncCheckpoint::new(child_id.to_string()));
        Ok(())
    }

    fn health_check(&self) -> Result<bool> {
        self.check_error()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seed_and_get_events() {
        let client = MockRemoteClient::new();
        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        let seq = client.seed_event(event);
        assert_eq!(seq, 1);

        let retrieved = client.get_events_since("child1", 0).unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].sequence, Some(1));
    }

    #[test]
    fn test_seed_event_dedup() {
        let client = MockRemoteClient::new();
        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        let seq1 = client.seed_event(event.clone());
        let seq2 = client.seed_event(event);
        assert_eq!(seq1, seq2);

        let events = client.get_events_since("child1", 0).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_entity_crud() {
        let client = MockRemoteClient::new();

        // Upsert
        client
            .upsert_entity("child1", EntityType::Goal, "goal1", r#"{"name":"Save"}"#)
            .unwrap();

        // Get
        let entity = client.get_entity("child1", EntityType::Goal, "goal1").unwrap();
        assert_eq!(entity, Some(r#"{"name":"Save"}"#.to_string()));

        // Update
        client
            .upsert_entity("child1", EntityType::Goal, "goal1", r#"{"name":"Save2"}"#)
            .unwrap();
        let entity = client.get_entity("child1", EntityType::Goal, "goal1").unwrap();
        assert_eq!(entity, Some(r#"{"name":"Save2"}"#.to_string()));

        // Delete
        client.delete_entity("child1", EntityType::Goal, "goal1").unwrap();
        let entity = client.get_entity("child1", EntityType::Goal, "goal1").unwrap();
        assert_eq!(entity, None);
    }

    #[test]
    fn test_watermark_only_moves_forward() {
        let client = MockRemoteClient::new();

        client.initialize_child("child1").unwrap();
        client.update_watermark("child1", "local", 5).unwrap();

        let cp = client.get_checkpoint("child1").unwrap();
        assert_eq!(cp.local_watermark, 5);

        // Attempt to move backward (should be ignored)
        client.update_watermark("child1", "local", 3).unwrap();
        let cp = client.get_checkpoint("child1").unwrap();
        assert_eq!(cp.local_watermark, 5); // Still 5

        // Move forward
        client.update_watermark("child1", "local", 10).unwrap();
        let cp = client.get_checkpoint("child1").unwrap();
        assert_eq!(cp.local_watermark, 10);
    }

    #[test]
    fn test_force_error() {
        let client = MockRemoteClient::new();
        client.force_error("Test error");

        assert!(client.upsert_entity("child1", EntityType::Goal, "goal1", "{}").is_err());
        assert!(client.get_entity("child1", EntityType::Goal, "goal1").is_err());
        assert!(client.health_check().is_err());

        client.clear_error();
        assert!(client.health_check().is_ok());
    }
}
