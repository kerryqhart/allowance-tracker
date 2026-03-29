use anyhow::Result;
use shared::sync::*;

pub trait RemoteStorage: Send + Sync {
    fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>>;
    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>>;
    fn upsert_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str, entity_json: &str) -> Result<()>;
    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>>;
    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()>;
    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint>;
    fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> Result<()>;
    fn initialize_child(&self, child_id: &str) -> Result<()>;
    fn health_check(&self) -> Result<bool>;
}
