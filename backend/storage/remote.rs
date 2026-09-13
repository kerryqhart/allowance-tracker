use anyhow::Result;
use shared::sync::*;

pub trait RemoteStorage: Send + Sync {
    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>>;
    fn upsert_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str, entity_json: &str) -> Result<()>;
    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>>;
    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()>;
    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint>;
    fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> Result<()>;
    fn initialize_child(&self, child_id: &str) -> Result<()>;
    fn health_check(&self) -> Result<bool>;
}

/// A `RemoteStorage` that does nothing, for the installation that has no AWS
/// transport configured at all.
///
/// `SyncThreadHandle::spawn` requires a `remote: Arc<dyn RemoteStorage>`
/// unconditionally, but the lgs (desktop-to-desktop) transport — wired via
/// `child_sync`, not `remote` — needs that same background thread to run
/// even when this household never set up the AWS side. Wiring this in for
/// that case, rather than leaving the whole thread unspawned, is what makes
/// `child_sync` reachable on an installation that only ever configures lgs.
///
/// Every method is a safe no-op: reads report "nothing new" and writes
/// succeed trivially. This is not a hidden data sink — `push_event`/
/// `poll_remote` only reach it when a `SyncNotifier` feeds local write
/// events into the thread's `event_rx`, and the caller that chooses
/// `NullRemoteStorage` also passes `None` for that notifier (see
/// `AllowanceTrackerApp::new`), so the write paths here are never actually
/// exercised in that configuration — this exists to satisfy the type the
/// thread requires, not to pretend to transport anything.
pub struct NullRemoteStorage;

impl RemoteStorage for NullRemoteStorage {
    fn get_events_since(&self, _child_id: &str, _since_sequence: u64) -> Result<Vec<SyncEvent>> {
        Ok(Vec::new())
    }

    fn upsert_entity(
        &self,
        _child_id: &str,
        _entity_type: EntityType,
        _entity_id: &str,
        _entity_json: &str,
    ) -> Result<()> {
        Ok(())
    }

    fn get_entity(&self, _child_id: &str, _entity_type: EntityType, _entity_id: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn delete_entity(&self, _child_id: &str, _entity_type: EntityType, _entity_id: &str) -> Result<()> {
        Ok(())
    }

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint> {
        Ok(SyncCheckpoint::new(child_id.to_string()))
    }

    fn update_watermark(&self, _child_id: &str, _which: &str, _value: u64) -> Result<()> {
        Ok(())
    }

    fn initialize_child(&self, _child_id: &str) -> Result<()> {
        Ok(())
    }

    fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_remote_storage_reports_nothing_new_and_never_errors() {
        let remote = NullRemoteStorage;
        assert_eq!(remote.get_events_since("child1", 0).unwrap().len(), 0);
        assert_eq!(remote.get_entity("child1", EntityType::Goal, "g1").unwrap(), None);
        assert!(remote.upsert_entity("child1", EntityType::Goal, "g1", "{}").is_ok());
        assert!(remote.delete_entity("child1", EntityType::Goal, "g1").is_ok());
        assert!(remote.update_watermark("child1", "local", 5).is_ok());
        assert!(remote.initialize_child("child1").is_ok());
        assert!(remote.health_check().unwrap());
        assert_eq!(remote.get_checkpoint("child1").unwrap().child_id, "child1");
    }
}
