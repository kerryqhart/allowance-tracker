use shared::sync::*;
use crate::backend::storage::remote::RemoteStorage;
use super::sync_manager::{SyncEngine, SyncMessage, SyncStatus};
use std::sync::{Arc, mpsc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

const BASE_POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(300);

/// Handle to the background sync thread. Drop to shut down.
pub struct SyncThreadHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SyncThreadHandle {
    /// Spawn the background sync thread.
    /// `event_rx`: receives SyncEvents from SyncNotifier (domain writes).
    /// `message_tx`: sends SyncMessages to the UI thread.
    /// `child_ids`: list of children to poll for.
    pub fn spawn(
        remote: Arc<dyn RemoteStorage>,
        event_rx: mpsc::Receiver<SyncEvent>,
        message_tx: mpsc::Sender<SyncMessage>,
        child_ids: Vec<String>,
    ) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();

        let thread = std::thread::Builder::new()
            .name("sync-thread".to_string())
            .spawn(move || {
                sync_loop(remote, event_rx, message_tx, child_ids, shutdown_flag);
            })
            .expect("Failed to spawn sync thread");

        Self {
            shutdown,
            thread: Some(thread),
        }
    }

    /// Signal the thread to shut down and wait for it.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SyncThreadHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn sync_loop(
    remote: Arc<dyn RemoteStorage>,
    event_rx: mpsc::Receiver<SyncEvent>,
    message_tx: mpsc::Sender<SyncMessage>,
    child_ids: Vec<String>,
    shutdown: Arc<AtomicBool>,
) {
    let mut engine = SyncEngine::new(remote);
    let mut poll_interval = BASE_POLL_INTERVAL;
    let mut had_activity;

    while !shutdown.load(Ordering::Relaxed) {
        had_activity = false;

        // 1. Drain any new local events from the notifier channel
        while let Ok(event) = event_rx.try_recv() {
            engine.enqueue_event(event);
            had_activity = true;
        }

        // 2. Push pending events
        let _ = message_tx.send(SyncMessage::StatusChanged(SyncStatus::Syncing));

        let failures = engine.push_pending();
        for (event, error) in &failures {
            let _ = message_tx.send(SyncMessage::PushFailed {
                event_id: event.event_id.clone(),
                error: error.clone(),
            });
            // Re-enqueue for retry
            engine.enqueue_event(event.clone());
        }

        // 3. Poll each child for remote events
        for child_id in &child_ids {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            match engine.poll_child(child_id) {
                Ok(result) => {
                    if !result.events_to_apply.is_empty() {
                        had_activity = true;
                        let _ = message_tx.send(SyncMessage::EntitiesUpdated {
                            child_id: child_id.clone(),
                            entity_type: result.events_to_apply[0].entity_type.clone(),
                            count: result.events_to_apply.len(),
                        });
                    }
                    for conflict in result.new_conflicts {
                        let _ = message_tx.send(SyncMessage::ConflictDetected(conflict));
                    }
                }
                Err(e) => {
                    let _ = message_tx.send(SyncMessage::Error(format!(
                        "Poll failed for {}: {}", child_id, e
                    )));
                }
            }
        }

        // 4. Update status
        let conflict_count = engine.pending_conflicts().iter()
            .filter(|c| c.status == ConflictStatus::Pending)
            .count();

        let status = if conflict_count > 0 {
            SyncStatus::HasConflicts(conflict_count)
        } else {
            SyncStatus::Idle
        };
        let _ = message_tx.send(SyncMessage::StatusChanged(status));

        // 5. Backoff
        if had_activity {
            poll_interval = BASE_POLL_INTERVAL;
        } else {
            poll_interval = (poll_interval * 2).min(MAX_POLL_INTERVAL);
        }

        // Sleep in small increments so we can check shutdown flag
        let sleep_end = std::time::Instant::now() + poll_interval;
        while std::time::Instant::now() < sleep_end && !shutdown.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::mock_remote::MockRemoteClient;
    use std::time::Duration;

    #[test]
    fn test_thread_starts_and_shuts_down() {
        let mock = Arc::new(MockRemoteClient::new());
        let (_, event_rx) = mpsc::channel();
        let (message_tx, _) = mpsc::channel();

        let mut handle = SyncThreadHandle::spawn(
            mock,
            event_rx,
            message_tx,
            vec![],
        );

        // Give the thread a moment to start
        std::thread::sleep(Duration::from_millis(100));

        handle.shutdown();
        // If we get here without hanging, the test passes
    }

    #[test]
    fn test_thread_pushes_local_events() {
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();

        let (event_tx, event_rx) = mpsc::channel();
        let (message_tx, _message_rx) = mpsc::channel();

        let mut handle = SyncThreadHandle::spawn(
            mock.clone(),
            event_rx,
            message_tx,
            vec!["child1".to_string()],
        );

        // Send a local event
        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        event_tx.send(event).unwrap();

        // Wait for multiple sync cycles to process it (BASE_POLL_INTERVAL is 30s, so wait long enough)
        std::thread::sleep(Duration::from_secs(2));

        // Verify the event was pushed to remote
        let events = mock.get_events_since("child1", 0).unwrap();
        assert_eq!(events.len(), 1);

        handle.shutdown();
    }

    #[test]
    fn test_thread_detects_remote_events() {
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();

        let (_, event_rx) = mpsc::channel();
        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();

        // Push a remote event before starting the thread
        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx_remote".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let mut handle = SyncThreadHandle::spawn(
            mock,
            event_rx,
            message_tx,
            vec!["child1".to_string()],
        );

        // Wait for sync cycle
        std::thread::sleep(Duration::from_millis(500));

        // Drain messages looking for EntitiesUpdated
        let mut found_update = false;
        while let Ok(msg) = message_rx.try_recv() {
            if let SyncMessage::EntitiesUpdated { child_id, count, .. } = msg {
                if child_id == "child1" && count == 1 {
                    found_update = true;
                }
            }
        }
        assert!(found_update, "Expected EntitiesUpdated message from sync thread");

        handle.shutdown();
    }
}
