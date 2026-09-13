use crate::backend::sync::bootstrap::DaemonOwnership;
use anyhow::Result;
use shared::sync::SyncEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    /// Per-child local watermark (last sequence we've processed).
    pub watermarks: HashMap<String, u64>,
    /// Whether sync is enabled.
    pub enabled: bool,
    /// Remote service URL.
    pub remote_url: Option<String>,
    /// Whether this Mac's lgs daemon is one the app installed, or one it
    /// found already running and adopted. `#[serde(default)]` so a
    /// `sync_state.yaml` written before this field existed keeps loading —
    /// it comes back as `installed_by_app: false`, the safe assumption for a
    /// daemon this app has no record of installing.
    #[serde(default)]
    pub daemon_ownership: DaemonOwnership,
    /// The lgs cloud root chosen at first run (`lgs init --cloud-root
    /// <path>`), persisted so later launches know the lgs (desktop-to-desktop)
    /// transport is already set up and can build a real `ChildSyncEngine`
    /// without re-running first run. `None` before first run has ever
    /// completed — the safe default; `#[serde(default)]` so a
    /// `sync_state.yaml` written before this field existed keeps loading.
    #[serde(default)]
    pub cloud_root: Option<PathBuf>,
}

impl SyncState {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let state: SyncState = serde_yaml::from_str(&contents)?;
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RetryQueue {
    pub events: Vec<SyncEvent>,
}

impl RetryQueue {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        if contents.trim().is_empty() {
            return Ok(Self::default());
        }
        let queue: RetryQueue = serde_yaml::from_str(&contents)?;
        Ok(queue)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

/// Standard file names for sync persistence.
pub fn sync_state_path(base_dir: &Path) -> PathBuf {
    base_dir.join("sync_state.yaml")
}

pub fn retry_queue_path(base_dir: &Path) -> PathBuf {
    base_dir.join("sync_retry_queue.yaml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::sync::*;
    use tempfile::TempDir;

    #[test]
    fn test_sync_state_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = sync_state_path(dir.path());

        let mut state = SyncState::default();
        state.enabled = true;
        state.remote_url = Some("http://localhost:3030".to_string());
        state.watermarks.insert("child1".to_string(), 42);

        state.save(&path).unwrap();

        let loaded = SyncState::load(&path).unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.remote_url.unwrap(), "http://localhost:3030");
        assert_eq!(*loaded.watermarks.get("child1").unwrap(), 42);
    }

    #[test]
    fn test_sync_state_load_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = sync_state_path(dir.path());

        let state = SyncState::load(&path).unwrap();
        assert!(!state.enabled);
        assert!(state.remote_url.is_none());
    }

    #[test]
    fn test_retry_queue_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = retry_queue_path(dir.path());

        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );

        let queue = RetryQueue { events: vec![event.clone()] };
        queue.save(&path).unwrap();

        let loaded = RetryQueue::load(&path).unwrap();
        assert_eq!(loaded.events.len(), 1);
        assert_eq!(loaded.events[0].event_id, event.event_id);
    }

    #[test]
    fn test_retry_queue_load_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = retry_queue_path(dir.path());

        let queue = RetryQueue::load(&path).unwrap();
        assert!(queue.events.is_empty());
    }

    #[test]
    fn daemon_ownership_round_trips_through_sync_state_yaml() {
        let dir = TempDir::new().unwrap();
        let path = sync_state_path(dir.path());

        let mut state = SyncState::default();
        state.daemon_ownership = DaemonOwnership { installed_by_app: true };
        state.save(&path).unwrap();

        let loaded = SyncState::load(&path).unwrap();
        assert!(loaded.daemon_ownership.installed_by_app);
    }

    /// A `sync_state.yaml` written before `daemon_ownership` existed must
    /// still load — this is exactly what `#[serde(default)]` on the field is
    /// for, proved here with a literal YAML string that omits the key
    /// entirely rather than by round-tripping a value we just wrote.
    #[test]
    fn an_existing_sync_state_yaml_without_daemon_ownership_still_loads() {
        let dir = TempDir::new().unwrap();
        let path = sync_state_path(dir.path());

        let legacy_yaml = "watermarks: {child1: 42}\nenabled: true\nremote_url: http://localhost:3030\n";
        std::fs::write(&path, legacy_yaml).unwrap();

        let loaded = SyncState::load(&path).unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.remote_url.as_deref(), Some("http://localhost:3030"));
        assert_eq!(*loaded.watermarks.get("child1").unwrap(), 42);
        assert!(
            !loaded.daemon_ownership.installed_by_app,
            "a pre-existing file with no daemon_ownership key must default to installed_by_app: false"
        );
    }
}
