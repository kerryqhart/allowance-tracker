use shared::sync::*;
use crate::backend::storage::remote::RemoteStorage;
use super::sync_manager::{SyncEngine, SyncMessage, SyncStatus, SyncCommand, UiMessenger, WakeUi};
use super::sync_persistence::{self, SyncState, RetryQueue};
use std::path::PathBuf;
use std::sync::{Arc, mpsc, atomic::{AtomicBool, Ordering}};

/// Handle to the background sync thread. Drop to shut down.
pub struct SyncThreadHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SyncThreadHandle {
    /// Spawn the background sync thread.
    ///
    /// - `event_rx`: receives SyncEvents from SyncNotifier (domain writes)
    /// - `command_rx`: receives control signals from the UI thread
    /// - `message_tx`: sends SyncMessages back to the UI thread
    /// - `initial_watermarks`: per-child watermarks loaded from persisted state
    /// - `initial_retry_queue`: events that failed previously and need retrying
    /// - `data_dir`: directory where sync_state.yaml and retry queue are written
    pub fn spawn(
        remote: Arc<dyn RemoteStorage>,
        event_rx: mpsc::Receiver<SyncEvent>,
        command_rx: mpsc::Receiver<SyncCommand>,
        message_tx: mpsc::Sender<SyncMessage>,
        wake_ui: WakeUi,
        initial_sync_state: SyncState,
        initial_retry_queue: Vec<SyncEvent>,
        data_dir: PathBuf,
    ) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();

        let mut engine = SyncEngine::new(remote.clone());
        for (child_id, watermark) in &initial_sync_state.watermarks {
            engine.set_watermark(child_id, *watermark);
        }
        let retry_queue = RetryQueue { events: initial_retry_queue };

        // Preserve the non-watermark fields of SyncState (enabled, remote_url) so
        // that the persistence writes below don't clobber user configuration.
        let sync_enabled = initial_sync_state.enabled;
        let sync_remote_url = initial_sync_state.remote_url.clone();

        let messenger = UiMessenger::new(message_tx, wake_ui);

        let thread = std::thread::Builder::new()
            .name("sync-thread".to_string())
            .spawn(move || {
                sync_loop(
                    remote,
                    event_rx,
                    command_rx,
                    messenger,
                    engine,
                    retry_queue,
                    data_dir,
                    sync_enabled,
                    sync_remote_url,
                    shutdown_flag,
                );
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

#[allow(clippy::too_many_arguments)]
fn sync_loop(
    remote: Arc<dyn RemoteStorage>,
    event_rx: mpsc::Receiver<SyncEvent>,
    command_rx: mpsc::Receiver<SyncCommand>,
    messenger: UiMessenger,
    mut engine: SyncEngine,
    mut retry_queue: RetryQueue,
    data_dir: PathBuf,
    sync_enabled: bool,
    sync_remote_url: Option<String>,
    shutdown: Arc<AtomicBool>,
) {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // 1. Drain retry queue first — these are prior failures, push them again
        let retry_count_before = retry_queue.events.len();
        if retry_count_before > 0 {
            log::info!("SYNC: retrying {} queued event(s)", retry_count_before);
        }
        let mut remaining_retries = Vec::new();
        for event in retry_queue.events.drain(..) {
            match push_event(&remote, &event, &messenger) {
                Ok(()) => {
                    log::info!("SYNC: retry drained event {}", event.event_id);
                }
                Err(err) => {
                    log::warn!("SYNC: retry still failing for event {}: {}", event.event_id, err);
                    let _ = messenger.send(SyncMessage::PushFailed {
                        event_id: event.event_id.clone(),
                        error: err,
                    });
                    remaining_retries.push(event);
                }
            }
        }
        retry_queue.events = remaining_retries;

        // 2. Drain new local events
        while let Ok(event) = event_rx.try_recv() {
            match push_event(&remote, &event, &messenger) {
                Ok(()) => {}
                Err(err) => {
                    log::warn!("SYNC: push failed, queuing for retry: event {} ({})", event.event_id, err);
                    let _ = messenger.send(SyncMessage::PushFailed {
                        event_id: event.event_id.clone(),
                        error: err,
                    });
                    retry_queue.events.push(event);
                }
            }
        }

        // 3. Process commands — PollNow triggers a poll, Shutdown sets flag.
        // Poll unconditionally every outer iteration (~every 30s) as a safety
        // net so sync works even if focus detection is flaky or disabled.
        // PollNow still lets us poll sooner by breaking out of the sleep.
        let mut should_poll = true;
        while let Ok(cmd) = command_rx.try_recv() {
            match cmd {
                SyncCommand::PollNow => {
                    log::info!("SYNC: PollNow received at outer loop");
                    should_poll = true;
                }
                SyncCommand::Shutdown => {
                    shutdown.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }

        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        if should_poll {
            let _ = messenger.send(SyncMessage::StatusChanged(SyncStatus::Syncing));
            poll_remote(&remote, &mut engine, &messenger);
            let _ = messenger.send(SyncMessage::StatusChanged(SyncStatus::Idle));
        }

        // 4. Persist state every loop iteration. Preserve enabled/remote_url from
        // the config we were spawned with so that a save doesn't clobber user config.
        let sync_state = SyncState {
            watermarks: engine.watermarks_snapshot(),
            enabled: sync_enabled,
            remote_url: sync_remote_url.clone(),
        };
        let _ = sync_state.save(&sync_persistence::sync_state_path(&data_dir));
        let _ = retry_queue.save(&sync_persistence::retry_queue_path(&data_dir));

        // 5. Sleep responsively — check every 500ms for new events, commands,
        //    or shutdown. Process work immediately rather than waiting for the
        //    next full outer loop iteration.
        let sleep_end = std::time::Instant::now() + std::time::Duration::from_secs(30);
        'sleep: while std::time::Instant::now() < sleep_end {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Drain any new local events that arrived during the sleep
            while let Ok(event) = event_rx.try_recv() {
                match push_event(&remote, &event, &messenger) {
                    Ok(()) => {}
                    Err(err) => {
                        log::warn!("SYNC: push failed, queuing for retry: event {} ({})", event.event_id, err);
                        let _ = messenger.send(SyncMessage::PushFailed {
                            event_id: event.event_id.clone(),
                            error: err,
                        });
                        retry_queue.events.push(event);
                    }
                }
            }

            // Check for a command
            match command_rx.try_recv() {
                Ok(SyncCommand::PollNow) => {
                    log::info!("SYNC: PollNow received in sleep loop — polling remote now");
                    let _ = messenger.send(SyncMessage::StatusChanged(SyncStatus::Syncing));
                    poll_remote(&remote, &mut engine, &messenger);
                    let _ = messenger.send(SyncMessage::StatusChanged(SyncStatus::Idle));
                    break 'sleep;
                }
                Ok(SyncCommand::Shutdown) => {
                    shutdown.store(true, Ordering::Relaxed);
                    break 'sleep;
                }
                Err(_) => {}
            }
        }
    }
}

/// Push a single local event to the remote. For non-delete events, first
/// requests the entity JSON from the UI thread via message_tx.
fn push_event(
    remote: &Arc<dyn RemoteStorage>,
    event: &SyncEvent,
    messenger: &UiMessenger,
) -> Result<(), String> {
    // Deletes don't need entity data. Delete the entity body first so that a
    // crash between the two calls leaves a dangling event to retry rather than
    // letting other clients see a delete-event referencing a still-present body.
    if event.action == SyncAction::Deleted {
        remote.delete_entity(&event.child_id, event.entity_type.clone(), &event.entity_id)
            .map_err(|e| format!("delete_entity failed: {e}"))?;
        remote.push_events(std::slice::from_ref(event))
            .map_err(|e| format!("push_events failed: {e}"))?;
        return Ok(());
    }

    // Request entity data from UI thread
    let (response_tx, response_rx) = mpsc::channel();
    messenger.send(SyncMessage::ReadEntityRequest {
        child_id: event.child_id.clone(),
        entity_type: event.entity_type.clone(),
        entity_id: event.entity_id.clone(),
        response_tx,
    }).map_err(|e| format!("failed to request entity from UI: {e}"))?;

    let entity_json = match response_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Some(json)) => json,
        Ok(None) => {
            // Entity is gone locally (likely deleted after this event was queued).
            // The subsequent Deleted event will handle remote cleanup; discard
            // this event rather than retrying it forever.
            log::warn!(
                "Discarding sync event {} for missing entity {:?}/{}",
                event.event_id, event.entity_type, event.entity_id
            );
            return Ok(());
        }
        Err(e) => return Err(format!("timeout waiting for entity read: {e}")),
    };

    // Upsert entity first, then the event record
    remote.upsert_entity(&event.child_id, event.entity_type.clone(), &event.entity_id, &entity_json)
        .map_err(|e| format!("upsert_entity failed: {e}"))?;
    remote.push_events(std::slice::from_ref(event))
        .map_err(|e| format!("push_events failed: {e}"))?;

    Ok(())
}

/// Poll remote for new events for all known children and send them to the UI thread.
fn poll_remote(
    remote: &Arc<dyn RemoteStorage>,
    engine: &mut SyncEngine,
    messenger: &UiMessenger,
) {
    // Ask the UI thread for the current list of local child IDs. Deriving it
    // from watermarks would be a bootstrap trap: a fresh install (or a blown-
    // away sync_state) has no watermarks yet, so nothing would ever get
    // polled. Fall back to watermark-derived IDs only if the UI thread fails
    // to respond within the timeout.
    let (response_tx, response_rx) = mpsc::channel();
    let _ = messenger.send(SyncMessage::GetChildIdsRequest { response_tx });
    let child_ids: Vec<String> = match response_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(ids) => ids,
        Err(_) => {
            log::warn!("SYNC: GetChildIdsRequest timed out — falling back to watermark keys");
            engine.watermarks_snapshot().keys().cloned().collect()
        }
    };

    for child_id in &child_ids {
        match engine.poll_child(child_id) {
            Ok(poll_result) => {
                for event in poll_result.events_to_apply {
                    match event.action {
                        SyncAction::Deleted => {
                            let _ = messenger.send(SyncMessage::DeleteLocalEntity {
                                child_id: event.child_id.clone(),
                                entity_type: event.entity_type.clone(),
                                entity_id: event.entity_id.clone(),
                                event_id: event.event_id.clone(),
                            });
                        }
                        SyncAction::Created | SyncAction::Updated => {
                            match remote.get_entity(
                                &event.child_id,
                                event.entity_type.clone(),
                                &event.entity_id,
                            ) {
                                Ok(Some(json)) => {
                                    let _ = messenger.send(SyncMessage::ApplyRemoteEntity {
                                        child_id: event.child_id.clone(),
                                        entity_type: event.entity_type.clone(),
                                        entity_id: event.entity_id.clone(),
                                        entity_json: json,
                                        event_id: event.event_id.clone(),
                                    });
                                }
                                Ok(None) => {
                                    // Entity deleted between event and fetch — skip
                                }
                                Err(e) => {
                                    let _ = messenger.send(SyncMessage::Error(
                                        format!("Failed to fetch entity: {e}"),
                                    ));
                                }
                            }
                        }
                    }
                }
                // Update the server-side watermark
                let new_watermark = engine.get_watermark(child_id);
                let _ = remote.update_watermark(child_id, "remote", new_watermark);
            }
            Err(e) => {
                let _ = messenger.send(SyncMessage::Error(
                    format!("Poll failed for {child_id}: {e}"),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::mock_remote::MockRemoteClient;
    use std::collections::HashMap;
    use std::time::Duration;
    use tempfile::TempDir;

    fn noop_wake() -> WakeUi {
        Arc::new(|| {})
    }

    fn empty_spawn(
        mock: Arc<MockRemoteClient>,
        event_rx: mpsc::Receiver<SyncEvent>,
        command_rx: mpsc::Receiver<SyncCommand>,
        message_tx: mpsc::Sender<SyncMessage>,
        data_dir: &std::path::Path,
    ) -> SyncThreadHandle {
        SyncThreadHandle::spawn(
            mock,
            event_rx,
            command_rx,
            message_tx,
            noop_wake(),
            SyncState::default(),
            vec![],
            data_dir.to_path_buf(),
        )
    }

    #[test]
    fn test_thread_starts_and_shuts_down() {
        let dir = TempDir::new().unwrap();
        let mock = Arc::new(MockRemoteClient::new());
        let (_event_tx, event_rx) = mpsc::channel();
        let (_command_tx, command_rx) = mpsc::channel();
        let (message_tx, _message_rx) = mpsc::channel();

        let mut handle = empty_spawn(mock, event_rx, command_rx, message_tx, dir.path());

        // Give the thread a moment to start
        std::thread::sleep(Duration::from_millis(100));

        handle.shutdown();
        // If we reach here without hanging, the test passes
    }

    #[test]
    fn test_thread_pushes_local_event_with_ui_handler() {
        let dir = TempDir::new().unwrap();
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();

        let (event_tx, event_rx) = mpsc::channel();
        let (_command_tx, command_rx) = mpsc::channel::<SyncCommand>();
        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();

        let mut handle = SyncThreadHandle::spawn(
            mock.clone(),
            event_rx,
            command_rx,
            message_tx,
            noop_wake(),
            SyncState::default(),
            vec![],
            dir.path().to_path_buf(),
        );

        // Start a helper thread to simulate the UI: respond to ReadEntityRequest
        std::thread::spawn(move || {
            // Drain messages until we see a ReadEntityRequest, then respond
            loop {
                match message_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(SyncMessage::ReadEntityRequest { response_tx, .. }) => {
                        let _ = response_tx.send(Some(r#"{"fake":"json"}"#.to_string()));
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });

        // Send a local event
        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        event_tx.send(event).unwrap();

        // Poll the mock until the event arrives or we hit a generous deadline.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while std::time::Instant::now() < deadline {
            events = mock.get_events_since("child1", 0).unwrap();
            if !events.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(events.len(), 1, "Expected event pushed to remote");

        handle.shutdown();
    }

    #[test]
    fn test_thread_responds_to_poll_now() {
        let dir = TempDir::new().unwrap();
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();

        // Pre-set a remote entity and event so poll will find something
        mock.upsert_entity("child1", EntityType::Transaction, "tx_remote", r#"{"id":"tx_remote"}"#).unwrap();
        let remote_event = SyncEvent::new(
            EntityType::Transaction,
            "tx_remote".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let (_event_tx, event_rx) = mpsc::channel::<SyncEvent>();
        let (command_tx, command_rx) = mpsc::channel::<SyncCommand>();
        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();

        // Initialize with child1 watermark = 0 so it gets polled
        let mut initial_watermarks = HashMap::new();
        initial_watermarks.insert("child1".to_string(), 0u64);
        let initial_state = SyncState {
            watermarks: initial_watermarks,
            enabled: true,
            remote_url: None,
        };

        let mut handle = SyncThreadHandle::spawn(
            mock.clone(),
            event_rx,
            command_rx,
            message_tx,
            noop_wake(),
            initial_state,
            vec![],
            dir.path().to_path_buf(),
        );

        command_tx.send(SyncCommand::PollNow).unwrap();

        // Block on the message channel until the expected ApplyRemoteEntity
        // arrives. Uses recv_timeout so a slow machine doesn't flake.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut found = false;
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match message_rx.recv_timeout(remaining) {
                Ok(SyncMessage::ApplyRemoteEntity { entity_id, .. }) if entity_id == "tx_remote" => {
                    found = true;
                    break;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert!(found, "Expected ApplyRemoteEntity for tx_remote");

        handle.shutdown();
    }
}
