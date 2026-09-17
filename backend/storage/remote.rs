use anyhow::{anyhow, Result};
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
/// Reads report "nothing new" — safe, since there is nothing this transport
/// could have missed. Writes (`upsert_entity`/`delete_entity`) are NOT a
/// safe no-op and must never silently report success:
///
/// Review Important-5: `sync_loop`'s step 1 unconditionally drains
/// `retry_queue.yaml` through `push_event` on every iteration, regardless of
/// which `remote` it was spawned with — that queue can hold events left
/// over from a PRIOR run where AWS genuinely was configured (a user who
/// disables AWS but keeps lgs configured does not get `retry_queue.yaml`
/// wiped). If these write paths returned `Ok(())`, that drain would read as
/// "pushed successfully" and delete each event from the queue — discarding
/// real, previously-queued AWS events with no trace, into a transport that
/// was never actually asked to do anything. Returning `Err` here instead
/// makes `push_event` fail, which is `sync_loop`'s existing, already-tested
/// path for "keep this event queued and tell the user" (see
/// `SyncMessage::PushFailed`) — the safe, honest outcome for a write this
/// installation has no way to actually deliver.
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
        Err(anyhow!(
            "no AWS transport is configured on this installation — this event stays queued for \
             retry rather than being silently discarded"
        ))
    }

    fn get_entity(&self, _child_id: &str, _entity_type: EntityType, _entity_id: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn delete_entity(&self, _child_id: &str, _entity_type: EntityType, _entity_id: &str) -> Result<()> {
        Err(anyhow!(
            "no AWS transport is configured on this installation — this event stays queued for \
             retry rather than being silently discarded"
        ))
    }

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint> {
        Ok(SyncCheckpoint::new(child_id.to_string()))
    }

    fn update_watermark(&self, _child_id: &str, _which: &str, _value: u64) -> Result<()> {
        // Bookkeeping only (advances a "don't re-fetch what we've already
        // seen" marker), never a delivery guarantee — unlike
        // `upsert_entity`/`delete_entity`, a no-op here loses nothing:
        // there is nothing to re-fetch from a transport that does not exist.
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
    fn null_remote_storage_reads_report_nothing_new_and_never_error() {
        let remote = NullRemoteStorage;
        assert_eq!(remote.get_events_since("child1", 0).unwrap().len(), 0);
        assert_eq!(remote.get_entity("child1", EntityType::Goal, "g1").unwrap(), None);
        assert!(remote.update_watermark("child1", "local", 5).is_ok());
        assert!(remote.initialize_child("child1").is_ok());
        assert!(remote.health_check().unwrap());
        assert_eq!(remote.get_checkpoint("child1").unwrap().child_id, "child1");
    }

    /// Review Important-5: writes must error, not silently succeed — an
    /// `Ok(())` here is exactly what would make `sync_loop`'s retry-queue
    /// drain read a leftover, undelivered AWS event as "pushed" and delete
    /// it from the queue forever.
    #[test]
    fn null_remote_storage_writes_error_instead_of_silently_succeeding() {
        let remote = NullRemoteStorage;
        assert!(remote.upsert_entity("child1", EntityType::Goal, "g1", "{}").is_err());
        assert!(remote.delete_entity("child1", EntityType::Goal, "g1").is_err());
    }
}
