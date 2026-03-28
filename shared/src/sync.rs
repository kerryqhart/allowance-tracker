use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntityType {
    Transaction,
    Goal,
    Child, // Includes allowance config (1:1)
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Transaction => "transaction",
            EntityType::Goal => "goal",
            EntityType::Child => "child",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "transaction" => Ok(EntityType::Transaction),
            "goal" => Ok(EntityType::Goal),
            "child" => Ok(EntityType::Child),
            _ => Err(format!("Unknown entity type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SyncAction {
    Created,
    Updated,
    Deleted,
}

impl SyncAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncAction::Created => "created",
            SyncAction::Updated => "updated",
            SyncAction::Deleted => "deleted",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "created" => Ok(SyncAction::Created),
            "updated" => Ok(SyncAction::Updated),
            "deleted" => Ok(SyncAction::Deleted),
            _ => Err(format!("Unknown sync action: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SyncSource {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncEvent {
    pub event_id: String,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub child_id: String,
    pub action: SyncAction,
    pub source: SyncSource,
    pub source_timestamp: DateTime<Utc>,
    /// Sequence number assigned by the remote service. None until pushed.
    pub sequence: Option<u64>,
}

impl SyncEvent {
    pub fn new(
        entity_type: EntityType,
        entity_id: String,
        child_id: String,
        action: SyncAction,
        source: SyncSource,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            entity_type,
            entity_id,
            child_id,
            action,
            source,
            source_timestamp: Utc::now(),
            sequence: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    pub child_id: String,
    pub event_sequence: u64,
    pub local_watermark: u64,
    pub remote_watermark: u64,
}

impl SyncCheckpoint {
    pub fn new(child_id: String) -> Self {
        Self {
            child_id,
            event_sequence: 0,
            local_watermark: 0,
            remote_watermark: 0,
        }
    }

    /// Events with sequence <= this value are eligible for TTL cleanup.
    pub fn min_watermark(&self) -> u64 {
        self.local_watermark.min(self.remote_watermark)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub id: String,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub child_id: String,
    pub local_event: SyncEvent,
    pub remote_event: SyncEvent,
    pub status: ConflictStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConflictStatus {
    Pending,
    ResolvedKeepLocal,
    ResolvedKeepRemote,
    ResolvedMerged,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_type_round_trip() {
        for et in [EntityType::Transaction, EntityType::Goal, EntityType::Child] {
            let s = et.as_str();
            let parsed = EntityType::from_str(s).unwrap();
            assert_eq!(et, parsed);
        }
    }

    #[test]
    fn test_sync_action_round_trip() {
        for action in [SyncAction::Created, SyncAction::Updated, SyncAction::Deleted] {
            let s = action.as_str();
            let parsed = SyncAction::from_str(s).unwrap();
            assert_eq!(action, parsed);
        }
    }

    #[test]
    fn test_sync_event_new_generates_unique_ids() {
        let e1 = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        let e2 = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        assert_ne!(e1.event_id, e2.event_id);
        assert!(e1.sequence.is_none());
    }

    #[test]
    fn test_checkpoint_min_watermark() {
        let mut cp = SyncCheckpoint::new("child1".to_string());
        cp.local_watermark = 10;
        cp.remote_watermark = 5;
        assert_eq!(cp.min_watermark(), 5);

        cp.local_watermark = 3;
        assert_eq!(cp.min_watermark(), 3);
    }

    #[test]
    fn test_entity_type_from_str_invalid() {
        assert!(EntityType::from_str("invalid").is_err());
    }

    #[test]
    fn test_sync_action_from_str_invalid() {
        assert!(SyncAction::from_str("invalid").is_err());
    }
}
