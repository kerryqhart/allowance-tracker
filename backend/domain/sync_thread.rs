use shared::sync::*;
use crate::backend::storage::remote::RemoteStorage;
use crate::backend::sync::bootstrap::DaemonOwnership;
use crate::backend::sync::{ChildSyncEngine, CycleOutcome};
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
    /// - `child_sync`: the lgs (desktop-to-desktop) transport, when configured.
    ///   `None` disables it entirely — this thread then behaves exactly as it
    ///   did before Task 17, running only the AWS-transport poll below. When
    ///   `Some`, [`run_child_sync_cycles`] runs a cycle per registered child
    ///   on the same triggers `poll_remote` already runs on (see the call
    ///   sites in [`sync_loop`]): the first iteration after spawn, the 30s
    ///   timer, and `PollNow` (window focus).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        remote: Arc<dyn RemoteStorage>,
        event_rx: mpsc::Receiver<SyncEvent>,
        command_rx: mpsc::Receiver<SyncCommand>,
        message_tx: mpsc::Sender<SyncMessage>,
        wake_ui: WakeUi,
        initial_sync_state: SyncState,
        initial_retry_queue: Vec<SyncEvent>,
        data_dir: PathBuf,
        child_sync: Option<ChildSyncEngine>,
    ) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();

        let mut engine = SyncEngine::new(remote.clone());
        for (child_id, watermark) in &initial_sync_state.watermarks {
            engine.set_watermark(child_id, *watermark);
        }
        let retry_queue = RetryQueue { events: initial_retry_queue };

        // Preserve the non-watermark fields of SyncState (enabled, remote_url,
        // daemon_ownership) so that the persistence writes below don't clobber
        // user configuration or forget which daemon this app installed.
        let sync_enabled = initial_sync_state.enabled;
        let sync_remote_url = initial_sync_state.remote_url.clone();
        let daemon_ownership = initial_sync_state.daemon_ownership.clone();

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
                    daemon_ownership,
                    shutdown_flag,
                    child_sync,
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
    daemon_ownership: DaemonOwnership,
    shutdown: Arc<AtomicBool>,
    child_sync: Option<ChildSyncEngine>,
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
            run_child_sync_cycles(&child_sync, &messenger, &shutdown);
            let _ = messenger.send(SyncMessage::StatusChanged(SyncStatus::Idle));
        }

        // 4. Persist state every loop iteration. Preserve enabled/remote_url from
        // the config we were spawned with so that a save doesn't clobber user config.
        let sync_state = SyncState {
            watermarks: engine.watermarks_snapshot(),
            enabled: sync_enabled,
            remote_url: sync_remote_url.clone(),
            daemon_ownership: daemon_ownership.clone(),
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
                    run_child_sync_cycles(&child_sync, &messenger, &shutdown);
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
/// requests the entity JSON from the UI thread via message_tx. The server's
/// PUT/DELETE on /entities emits the sync event atomically; this function no
/// longer calls a separate /sync/events endpoint.
fn push_event(
    remote: &Arc<dyn RemoteStorage>,
    event: &SyncEvent,
    messenger: &UiMessenger,
) -> Result<(), String> {
    if event.action == SyncAction::Deleted {
        remote.delete_entity(&event.child_id, event.entity_type.clone(), &event.entity_id)
            .map_err(|e| format!("delete_entity failed: {e}"))?;
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

    remote.upsert_entity(&event.child_id, event.entity_type.clone(), &event.entity_id, &entity_json)
        .map_err(|e| format!("upsert_entity failed: {e}"))?;

    Ok(())
}

/// Ask the UI thread for the current list of registered child ids. The UI
/// thread owns the registry (`sync_manager.rs:36-38`, "UI owns all repo
/// I/O"), so both transports that need "which children exist right now" —
/// the AWS-style `poll_remote` below and the lgs-style
/// `run_child_sync_cycles` — go through this same request rather than each
/// guessing independently.
///
/// `None` means the request timed out (UI thread busy or gone); callers
/// decide their own fallback, since "no answer" means something different
/// to each of them.
fn get_child_ids(messenger: &UiMessenger) -> Option<Vec<String>> {
    let (response_tx, response_rx) = mpsc::channel();
    messenger.send(SyncMessage::GetChildIdsRequest { response_tx }).ok()?;
    response_rx.recv_timeout(std::time::Duration::from_secs(5)).ok()
}

/// Per-tick time budget for [`run_child_sync_cycles`]. Review Critical-2:
/// without a bound, iterating every registered child with no time budget and
/// no shutdown check let one hung child (a stalled daemon, a fetch that
/// never returns) cost the WHOLE tick — and everything downstream of it in
/// `sync_loop` (the AWS poll's own next call, the 30s timer, shutdown)
/// blocks right along with it, since this all runs synchronously on the one
/// background sync thread. `LgsClient::run` now bounds any single `lgs`
/// process spawn to 10s (see `LGS_RUN_TIMEOUT`), but `fetch_lgs`/`push_lgs`
/// are raw libgit2 network calls with no such bound of their own — this
/// budget is what keeps a slow child to costing at most one tick rather than
/// the thread, by refusing to START another child's cycle once it is spent
/// (a child already in flight when the budget expires still finishes; nb
/// this does not preempt it).
const CHILD_SYNC_TICK_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// Run one lgs sync cycle ([`ChildSyncEngine::cycle_with_status`]) for every
/// registered child. Called from [`sync_loop`] on the exact same triggers as
/// the AWS-transport `poll_remote` (see the call sites there): the first
/// iteration after spawn, the 30s timer, and `PollNow` (sent on window
/// focus) — Task 17's brief for folding this in.
///
/// # Thread ownership
///
/// This function runs on the background sync thread and must never touch a
/// child's working tree. [`ChildSyncEngine::cycle_with_status`] already
/// enforces that on its own side (see its module doc) — it only fetches,
/// reads git objects, and (for `Cycle::Ahead`) pushes. Every outcome that
/// WOULD require writing a file is handed to the UI thread as a message
/// instead: `CycleOutcome::Merged` becomes `SyncMessage::ApplyMerge` and
/// `CycleOutcome::FastForward` becomes `SyncMessage::ApplyFastForward`,
/// both applied on `app_coordinator.rs` — the only place in this whole
/// feature that writes to a child's working tree.
///
/// # Failure isolation
///
/// Each child has its own repository and its own remote, so one child's
/// failure (its lgs project not registered yet, a transient fetch error,
/// the daemon being briefly unreachable) has no bearing on any other
/// child's. A cycle that returns `Err` is logged and the loop moves on —
/// never `?`, never `return`, never a panic that would take the rest of the
/// tick's children down with it.
///
/// # One `lgs status --json` per tick, not per child
///
/// `lgs status --json` answers "what are my projects," which does not
/// depend on which child is asking — fetched once here via
/// [`ChildSyncEngine::fetch_status`] and passed to every child's
/// `cycle_with_status`, rather than this engine spawning `lgs` once per
/// child per tick (each spawn being its own process and its own
/// [`LGS_RUN_TIMEOUT`]-bounded wait).
///
/// # Per-tick budget and shutdown
///
/// `shutdown` is checked before each child, and the tick stops taking on
/// NEW children once [`CHILD_SYNC_TICK_BUDGET`] has elapsed — see that
/// constant's doc comment. It is ALSO checked before either of this
/// function's own two blocking calls (`get_child_ids`, up to 5s; and
/// `ChildSyncEngine::fetch_status`, up to `LGS_RUN_TIMEOUT` — 10s) — Review
/// Important-2 — so a shutdown request already pending is never delayed by
/// work this tick has not started yet. `fetch_lgs`/`push_lgs` inside each
/// child's cycle are themselves bounded (`storage::git::NETWORK_TIMEOUT`,
/// 30s, via libgit2 transfer/negotiation callbacks) rather than left to
/// block this thread indefinitely on a stalled remote.
fn run_child_sync_cycles(
    child_sync: &Option<ChildSyncEngine>,
    messenger: &UiMessenger,
    shutdown: &Arc<AtomicBool>,
) {
    run_child_sync_cycles_with_budget(child_sync, messenger, shutdown, CHILD_SYNC_TICK_BUDGET)
}

/// The actual implementation behind [`run_child_sync_cycles`], with the
/// per-tick budget exposed so tests can drive the "budget already spent"
/// path deterministically (a zero/near-zero budget) instead of waiting out
/// the real 20s bound.
fn run_child_sync_cycles_with_budget(
    child_sync: &Option<ChildSyncEngine>,
    messenger: &UiMessenger,
    shutdown: &Arc<AtomicBool>,
    budget: std::time::Duration,
) {
    let Some(engine) = child_sync else { return };

    // Review Important-2: this is "the first shutdown check" — checked
    // before EITHER of the two blocking calls below (`get_child_ids` can
    // wait up to 5s for the UI thread; `fetch_status` up to
    // `LGS_RUN_TIMEOUT`, 10s), so a shutdown request is never delayed by
    // work this tick has not even started yet.
    if shutdown.load(Ordering::Relaxed) {
        return;
    }

    let child_ids = match get_child_ids(messenger) {
        Some(ids) => ids,
        None => {
            log::warn!("SYNC(lgs): GetChildIdsRequest timed out — skipping this child-sync tick");
            return;
        }
    };

    // Checked again before `fetch_status` specifically (up to 10s) for the
    // same reason — a shutdown requested while `get_child_ids` was waiting
    // must not then be delayed by a second, unrelated blocking call.
    if shutdown.load(Ordering::Relaxed) {
        log::info!("SYNC(lgs): shutdown requested before fetching lgs status — skipping this tick");
        return;
    }

    let status = match engine.fetch_status() {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "SYNC(lgs): could not fetch `lgs status --json` for this tick — skipping all \
                 {} registered child(ren): {e:#}",
                child_ids.len()
            );
            return;
        }
    };

    let tick_deadline = std::time::Instant::now() + budget;
    for (i, child_id) in child_ids.iter().enumerate() {
        if shutdown.load(Ordering::Relaxed) {
            log::info!(
                "SYNC(lgs): shutdown requested mid-tick — stopping after {i} of {} children",
                child_ids.len()
            );
            break;
        }
        if std::time::Instant::now() >= tick_deadline {
            log::warn!(
                "SYNC(lgs): per-tick time budget ({budget:?}) exceeded after {i} of {} children \
                 — deferring the rest to the next tick",
                child_ids.len()
            );
            break;
        }

        let id = shared::ChildId::from(child_id.as_str());
        match engine.cycle_with_status(&id, &status) {
            Ok(CycleOutcome::UpToDate) | Ok(CycleOutcome::Ahead) => {
                // `Ahead` already pushed inside `cycle_with_status`
                // (fetch/push are both background-thread operations — see
                // `Cycle::Ahead`'s doc comment in `child_sync.rs`). Nothing
                // further to do on either thread.
            }
            Ok(CycleOutcome::FastForward { to }) => {
                if let Err(e) = messenger.send(SyncMessage::ApplyFastForward {
                    child_id: child_id.clone(),
                    to,
                }) {
                    log::warn!(
                        "SYNC(lgs): failed to hand a fast-forward for child {child_id} to the UI \
                         thread (channel closed?): {e}"
                    );
                }
            }
            Ok(CycleOutcome::Merged { rows, parents, decisions, .. }) => {
                if let Err(e) = messenger.send(SyncMessage::ApplyMerge {
                    child_id: child_id.clone(),
                    rows,
                    parents,
                    decisions,
                }) {
                    log::warn!(
                        "SYNC(lgs): failed to hand a computed merge for child {child_id} to the \
                         UI thread (channel closed?): {e}"
                    );
                }
            }
            Err(e) => {
                // Never abort the tick: the next child's repo and remote are
                // entirely independent of this one's failure.
                log::warn!("SYNC(lgs): cycle failed for child {child_id}: {e:#}");
            }
        }
    }
}

/// Poll remote for new events for all known children and send them to the UI thread.
fn poll_remote(
    remote: &Arc<dyn RemoteStorage>,
    engine: &mut SyncEngine,
    messenger: &UiMessenger,
) {
    // Deriving the child list from watermarks would be a bootstrap trap: a
    // fresh install (or a blown-away sync_state) has no watermarks yet, so
    // nothing would ever get polled. Fall back to watermark-derived IDs only
    // if the UI thread fails to respond within the timeout.
    let child_ids: Vec<String> = match get_child_ids(messenger) {
        Some(ids) => ids,
        None => {
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
    use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
    use crate::backend::domain::commands::transactions::CreateTransactionCommand;
    use crate::backend::storage::mock_remote::MockRemoteClient;
    use crate::backend::sync::lgs_client::LgsClient;
    use crate::backend::Backend;
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
            None,
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
            None,
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
        mock.seed_event(remote_event);

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
            daemon_ownership: DaemonOwnership::default(),
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
            None,
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

    // --- Task 17, Part B: the lgs scheduling loop --------------------------
    //
    // No `lgs` binary and no daemon anywhere below — `LgsClient` just shells
    // out to whatever binary path it is given, so a tiny shell script that
    // prints canned `lgs status --json` output is a fake, not the real CLI.
    // The "remote" is a local bare repo in a tempdir, exactly like
    // `child_sync.rs`'s own tests. This matches the safety constraint: no
    // launchctl, no real lgs command, no live daemon required.

    /// A real, git-backed child (created the same way `Backend`'s ordinary
    /// write path does it) whose lgs "auth" tip has diverged from an
    /// unrelated peer bare repo. `ChildSyncEngine::cycle` for this child
    /// must therefore return `CycleOutcome::Merged` — `classify`'s
    /// catch-all arm treats "no common ancestor at all" the same as any
    /// other divergence (see `child_sync::classify`'s doc comment), so the
    /// peer repo does not need to share any history with the child's repo.
    struct DivergedChildFixture {
        engine: ChildSyncEngine,
        child_id: String,
        ours_oid: git2::Oid,
        theirs_oid: git2::Oid,
        // Held only for their Drop (tempdir cleanup) — never read directly.
        _data_dir: TempDir,
        _bare_dir: TempDir,
        _script_dir: TempDir,
    }

    fn diverged_child_fixture() -> DivergedChildFixture {
        let data_dir = TempDir::new().unwrap();
        let backend = Backend::with_data_dir(data_dir.path().to_path_buf(), None).unwrap();
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .unwrap()
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .unwrap();
        backend
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Allowance".to_string(),
                amount: 10.0,
                date: None,
            })
            .unwrap();

        let child_dir = backend.csv_connection.child_dir(&shared::ChildId::from(child.id.as_str())).unwrap();
        let repo = git2::Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();

        // An unrelated "peer" bare repo standing in for lgs's cloud copy,
        // with one commit made directly against its object database (no
        // working tree needed) advertised as the authoritative tip.
        let bare_dir = TempDir::new().unwrap();
        let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
        let sig =
            git2::Signature::new("Peer", "peer@example.com", &git2::Time::new(1_700_000_500, 0)).unwrap();
        let mut builder = bare.treebuilder(None).unwrap();
        let blob = bare
            .blob(
                b"id,child_id,date,description,amount,balance,type\n\
                  in-peer-a,x,2026-01-02T00:00:00+00:00,Peer Allowance,5.00,5.00,allowance\n",
            )
            .unwrap();
        builder.insert("transactions.csv", blob, 0o100644).unwrap();
        let tree_id = builder.write().unwrap();
        let tree = bare.find_tree(tree_id).unwrap();
        let theirs_oid = bare.commit(None, &sig, &sig, "peer edit", &tree, &[]).unwrap();
        bare.reference("refs/lgs-auth/heads/main", theirs_oid, true, "auth tip").unwrap();

        // A fake `lgs status --json`: a shell script (never the real `lgs`
        // binary) that always prints one project pointing at the bare repo
        // above.
        let script_dir = TempDir::new().unwrap();
        let script_path = script_dir.path().join("lgs");
        let json = format!(
            r#"{{"projects":[{{"name":"allowance-{}","clone_url":"{}","working_repo_path":"{}"}}]}}"#,
            child.id,
            bare_dir.path().to_str().unwrap(),
            child_dir.to_str().unwrap(),
        );
        std::fs::write(&script_path, format!("#!/bin/sh\ncat <<'JSON'\n{json}\nJSON\n")).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let lgs = LgsClient::new(script_path);
        let engine = ChildSyncEngine::new(lgs, backend.csv_connection.clone());

        DivergedChildFixture {
            engine,
            child_id: child.id,
            ours_oid,
            theirs_oid,
            _data_dir: data_dir,
            _bare_dir: bare_dir,
            _script_dir: script_dir,
        }
    }

    /// Drain `message_rx` until either the expected `ApplyMerge` for
    /// `expect_child_id` arrives (answering every `GetChildIdsRequest` along
    /// the way with `respond_ids`) or the deadline passes.
    fn wait_for_apply_merge(
        message_rx: &mpsc::Receiver<SyncMessage>,
        respond_ids: Vec<String>,
        expect_child_id: &str,
        timeout: Duration,
    ) -> Option<(Vec<allowance_core::row::TxRow>, (String, String))> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match message_rx.recv_timeout(remaining) {
                Ok(SyncMessage::GetChildIdsRequest { response_tx }) => {
                    let _ = response_tx.send(respond_ids.clone());
                }
                Ok(SyncMessage::ApplyMerge { child_id, rows, parents, .. }) if child_id == expect_child_id => {
                    return Some((rows, parents));
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        None
    }

    /// Task 17, Part B: nothing called `ChildSyncEngine::cycle()` or sent
    /// `ApplyMerge` at runtime before this task. This proves the scheduling
    /// loop actually runs a cycle for a registered child on a tick (here,
    /// the unconditional first-iteration poll right after spawn — "startup"
    /// in the brief's list of triggers) and that a genuine divergence
    /// produces a real `SyncMessage::ApplyMerge` carrying the merged rows.
    #[test]
    fn child_sync_cycle_runs_on_startup_and_sends_apply_merge() {
        let fixture = diverged_child_fixture();
        let child_id = fixture.child_id.clone();
        let ours_oid = fixture.ours_oid;
        let theirs_oid = fixture.theirs_oid;

        let (_event_tx, event_rx) = mpsc::channel::<SyncEvent>();
        let (_command_tx, command_rx) = mpsc::channel::<SyncCommand>();
        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();
        let dir = TempDir::new().unwrap();

        let mut handle = SyncThreadHandle::spawn(
            Arc::new(MockRemoteClient::new()),
            event_rx,
            command_rx,
            message_tx,
            noop_wake(),
            SyncState::default(),
            vec![],
            dir.path().to_path_buf(),
            Some(fixture.engine),
        );

        let result = wait_for_apply_merge(
            &message_rx,
            vec![child_id.clone()],
            &child_id,
            Duration::from_secs(10),
        );
        handle.shutdown();

        let (rows, parents) = result.expect("expected ApplyMerge for the registered child on startup");
        assert_eq!(parents, (ours_oid.to_string(), theirs_oid.to_string()));
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"in-peer-a"), "the merge must include the peer's row: {ids:?}");
    }

    /// Task 17, Part B's explicit requirement: "a failure syncing ONE child
    /// must not abort the others." A bogus, unregistered child id ahead of
    /// the real one in the roster response makes `ChildSyncEngine::cycle`
    /// fail immediately (its `work_dir` lookup errors — no such child is
    /// registered in `CsvConnection`). That failure must be logged and
    /// skipped, not stop the loop: the real, diverged child after it must
    /// still get its cycle and its `ApplyMerge`.
    #[test]
    fn one_childs_failed_cycle_does_not_block_another_childs_cycle() {
        let fixture = diverged_child_fixture();
        let child_id = fixture.child_id.clone();

        let (_event_tx, event_rx) = mpsc::channel::<SyncEvent>();
        let (_command_tx, command_rx) = mpsc::channel::<SyncCommand>();
        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();
        let dir = TempDir::new().unwrap();

        let mut handle = SyncThreadHandle::spawn(
            Arc::new(MockRemoteClient::new()),
            event_rx,
            command_rx,
            message_tx,
            noop_wake(),
            SyncState::default(),
            vec![],
            dir.path().to_path_buf(),
            Some(fixture.engine),
        );

        // The bogus id is listed FIRST, so its failure happens before the
        // real child's cycle is even attempted — proving a failure earlier
        // in the loop does not short-circuit the rest of it.
        let result = wait_for_apply_merge(
            &message_rx,
            vec!["nonexistent-child".to_string(), child_id.clone()],
            &child_id,
            Duration::from_secs(10),
        );
        handle.shutdown();

        assert!(
            result.is_some(),
            "the registered child's cycle must still run and produce an ApplyMerge despite the \
             other (nonexistent) child's cycle failing"
        );
    }

    /// A real, git-backed child that is ORDINARILY behind — its own tip is a
    /// genuine ancestor of the peer's advertised tip, with no local commits
    /// of its own since then. Review Critical-1: `classify` returns
    /// `Cycle::FastForward` in exactly this situation, and it is the
    /// COMMON case (any machine that is not the one currently editing), not
    /// an edge case — so this fixture exists specifically to exercise it
    /// end to end through the scheduling loop.
    struct AheadChildFixture {
        engine: ChildSyncEngine,
        child_id: String,
        ahead_oid: git2::Oid,
        _data_dir: TempDir,
        _bare_dir: TempDir,
        _script_dir: TempDir,
    }

    fn ahead_child_fixture() -> AheadChildFixture {
        use crate::backend::storage::git::{ensure_lgs_remote, push_lgs};
        use crate::backend::sync::child_sync::current_branch;

        let data_dir = TempDir::new().unwrap();
        let backend = Backend::with_data_dir(data_dir.path().to_path_buf(), None).unwrap();
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .unwrap()
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .unwrap();
        backend
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Allowance".to_string(),
                amount: 10.0,
                date: None,
            })
            .unwrap();

        let child_dir = backend.csv_connection.child_dir(&shared::ChildId::from(child.id.as_str())).unwrap();
        let child_repo = git2::Repository::open(&child_dir).unwrap();
        let base_oid = child_repo.head().unwrap().peel_to_commit().unwrap().id();

        // Push the child's OWN current tip into a bare repo first, then add
        // one more commit on top directly against the bare's object
        // database — this makes `base_oid` a genuine ancestor of the new
        // tip (a real shared history), which is what distinguishes a
        // fast-forward from `diverged_child_fixture`'s deliberately
        // unrelated history.
        let bare_dir = TempDir::new().unwrap();
        git2::Repository::init_bare(bare_dir.path()).unwrap();
        ensure_lgs_remote(&child_repo, bare_dir.path().to_str().unwrap()).unwrap();
        // Push the child's OWN actual branch name — its default branch may
        // be "main" or "master" depending on the local git install's
        // `init.defaultBranch`, and `push_lgs`'s refspec needs the real one
        // to exist. The "auth" ref below stays hardcoded to
        // `refs/lgs-auth/heads/main` regardless — that literal path is what
        // `child_sync::LGS_AUTH_MAIN` always looks for.
        let child_branch = current_branch(&child_repo).unwrap();
        push_lgs(&child_repo, &child_branch).unwrap();

        let bare = git2::Repository::open_bare(bare_dir.path()).unwrap();
        let base_commit = bare.find_commit(base_oid).unwrap();
        let sig =
            git2::Signature::new("Peer", "peer@example.com", &git2::Time::new(1_700_000_500, 0)).unwrap();
        let mut builder = bare.treebuilder(Some(&base_commit.tree().unwrap())).unwrap();
        let blob = bare
            .blob(
                b"id,child_id,date,description,amount,balance,type\n\
                  in-1-a,x,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n\
                  in-peer-a,x,2026-01-02T00:00:00+00:00,Peer Allowance,5.00,15.00,allowance\n",
            )
            .unwrap();
        builder.insert("transactions.csv", blob, 0o100644).unwrap();
        let tree_id = builder.write().unwrap();
        let tree = bare.find_tree(tree_id).unwrap();
        let ahead_oid = bare.commit(None, &sig, &sig, "peer advanced", &tree, &[&base_commit]).unwrap();
        bare.reference("refs/lgs-auth/heads/main", ahead_oid, true, "auth tip").unwrap();

        let script_dir = TempDir::new().unwrap();
        let script_path = script_dir.path().join("lgs");
        let json = format!(
            r#"{{"projects":[{{"name":"allowance-{}","clone_url":"{}","working_repo_path":"{}"}}]}}"#,
            child.id,
            bare_dir.path().to_str().unwrap(),
            child_dir.to_str().unwrap(),
        );
        std::fs::write(&script_path, format!("#!/bin/sh\ncat <<'JSON'\n{json}\nJSON\n")).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let lgs = LgsClient::new(script_path);
        let engine = ChildSyncEngine::new(lgs, backend.csv_connection.clone());

        AheadChildFixture {
            engine,
            child_id: child.id,
            ahead_oid,
            _data_dir: data_dir,
            _bare_dir: bare_dir,
            _script_dir: script_dir,
        }
    }

    /// Drain `message_rx` until either the expected `ApplyFastForward` for
    /// `expect_child_id` arrives (answering every `GetChildIdsRequest`
    /// along the way with `respond_ids`) or the deadline passes.
    fn wait_for_apply_fast_forward(
        message_rx: &mpsc::Receiver<SyncMessage>,
        respond_ids: Vec<String>,
        expect_child_id: &str,
        timeout: Duration,
    ) -> Option<String> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match message_rx.recv_timeout(remaining) {
                Ok(SyncMessage::GetChildIdsRequest { response_tx }) => {
                    let _ = response_tx.send(respond_ids.clone());
                }
                Ok(SyncMessage::ApplyFastForward { child_id, to }) if child_id == expect_child_id => {
                    return Some(to);
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        None
    }

    /// Review Critical-1: `CycleOutcome::FastForward` used to have no
    /// consumer at all — this proves the scheduling loop now sends
    /// `SyncMessage::ApplyFastForward` for the ORDINARY case (a child that
    /// is simply behind, no divergence), end to end through
    /// `SyncThreadHandle::spawn`. (`apply_fast_forward_tests` in
    /// `app_coordinator.rs` separately proves the UI-thread handler for
    /// this message actually updates the working tree on disk.)
    #[test]
    fn child_sync_cycle_sends_apply_fast_forward_for_an_ordinary_behind_child() {
        let fixture = ahead_child_fixture();
        let child_id = fixture.child_id.clone();
        let ahead_oid = fixture.ahead_oid;

        let (_event_tx, event_rx) = mpsc::channel::<SyncEvent>();
        let (_command_tx, command_rx) = mpsc::channel::<SyncCommand>();
        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();
        let dir = TempDir::new().unwrap();

        let mut handle = SyncThreadHandle::spawn(
            Arc::new(MockRemoteClient::new()),
            event_rx,
            command_rx,
            message_tx,
            noop_wake(),
            SyncState::default(),
            vec![],
            dir.path().to_path_buf(),
            Some(fixture.engine),
        );

        let result = wait_for_apply_fast_forward(
            &message_rx,
            vec![child_id.clone()],
            &child_id,
            Duration::from_secs(10),
        );
        handle.shutdown();

        let to = result.expect("expected ApplyFastForward for the registered, ordinarily-behind child");
        assert_eq!(to, ahead_oid.to_string());
    }

    // --- Review Critical-2: bounded, budgeted, single-status-fetch tick ---

    /// If `shutdown` is already set before a tick starts, `run_child_sync_cycles`
    /// must stop before running ANY child's cycle rather than pushing
    /// through the whole registered list first — in fact, per Review
    /// Important-2's "first shutdown check" fix, it must not even send a
    /// `GetChildIdsRequest` (a call the UI thread could otherwise take up
    /// to 5s to answer). Calls the function directly (not through a live
    /// `SyncThreadHandle`) with a hand-rolled responder thread, so the
    /// shutdown flag can be controlled precisely.
    #[test]
    fn shutdown_already_set_prevents_any_child_cycle_from_running() {
        let fixture = diverged_child_fixture();
        let child_id = fixture.child_id.clone();

        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();
        let messenger = UiMessenger::new(message_tx, noop_wake());
        let shutdown = Arc::new(AtomicBool::new(true));

        let responder = std::thread::spawn(move || {
            let mut saw_apply = false;
            loop {
                // Short timeout: with shutdown already set, nothing should
                // ever be sent at all (not even a GetChildIdsRequest), so
                // this only needs to wait long enough to be confident
                // nothing is coming, not to accommodate real work.
                match message_rx.recv_timeout(Duration::from_millis(300)) {
                    Ok(SyncMessage::GetChildIdsRequest { response_tx }) => {
                        let _ = response_tx.send(vec![child_id.clone()]);
                    }
                    Ok(SyncMessage::ApplyMerge { .. }) | Ok(SyncMessage::ApplyFastForward { .. }) => {
                        saw_apply = true;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            saw_apply
        });

        run_child_sync_cycles(&Some(fixture.engine), &messenger, &shutdown);
        drop(messenger); // closes the channel so the responder's recv times out and returns

        let saw_apply = responder.join().unwrap();
        assert!(
            !saw_apply,
            "no child cycle should have run at all once shutdown was already set"
        );
    }

    /// An effectively-zero per-tick budget must stop the loop before it
    /// takes on any child's cycle — proving the budget check is a real gate
    /// (not a no-op), without waiting out the real 20s production bound.
    /// Contrast with `child_sync_cycle_runs_on_startup_and_sends_apply_merge`,
    /// which proves the SAME fixture's child DOES get a cycle under the
    /// ordinary (non-tiny) budget.
    #[test]
    fn a_near_zero_budget_prevents_any_child_cycle_from_running() {
        let fixture = diverged_child_fixture();
        let child_id = fixture.child_id.clone();

        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();
        let messenger = UiMessenger::new(message_tx, noop_wake());
        let shutdown = Arc::new(AtomicBool::new(false));

        let responder = std::thread::spawn(move || {
            let mut saw_apply = false;
            loop {
                match message_rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(SyncMessage::GetChildIdsRequest { response_tx }) => {
                        let _ = response_tx.send(vec![child_id.clone()]);
                    }
                    Ok(SyncMessage::ApplyMerge { .. }) | Ok(SyncMessage::ApplyFastForward { .. }) => {
                        saw_apply = true;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            saw_apply
        });

        run_child_sync_cycles_with_budget(&Some(fixture.engine), &messenger, &shutdown, Duration::ZERO);
        drop(messenger);

        let saw_apply = responder.join().unwrap();
        assert!(
            !saw_apply,
            "a zero-duration tick budget should have expired before any child's cycle ran"
        );
    }

    /// Review Critical-2(c): `lgs status --json` must be fetched ONCE per
    /// tick and reused across every child, not once per child. Two real,
    /// diverged children share ONE fake `lgs` script that appends a line to
    /// a counter file every time it is invoked; after a tick that
    /// successfully syncs both, the counter must show exactly one
    /// invocation.
    #[test]
    fn lgs_status_is_fetched_once_per_tick_not_once_per_child() {
        let data_dir = TempDir::new().unwrap();
        let backend = Backend::with_data_dir(data_dir.path().to_path_buf(), None).unwrap();

        let mut child_ids = Vec::new();
        let mut bare_dirs = Vec::new();
        let mut projects_json = Vec::new();
        for n in 0..2 {
            let child = backend
                .child_service
                .create_child(CreateChildCommand {
                    name: format!("Test Kid {n}"),
                    birthdate: "2015-01-01".to_string(),
                })
                .unwrap()
                .child;
            backend
                .child_service
                .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
                .unwrap();
            backend
                .transaction_service
                .create_transaction(CreateTransactionCommand {
                    description: "Allowance".to_string(),
                    amount: 10.0,
                    date: None,
                })
                .unwrap();

            let child_dir = backend.csv_connection.child_dir(&shared::ChildId::from(child.id.as_str())).unwrap();
            let repo = git2::Repository::open(&child_dir).unwrap();
            let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();

            let bare_dir = TempDir::new().unwrap();
            let bare = git2::Repository::init_bare(bare_dir.path()).unwrap();
            let sig = git2::Signature::new("Peer", "peer@example.com", &git2::Time::new(1_700_000_500, 0))
                .unwrap();
            let mut builder = bare.treebuilder(None).unwrap();
            let blob = bare
                .blob(
                    format!(
                        "id,child_id,date,description,amount,balance,type\n\
                         in-peer-{n},x,2026-01-02T00:00:00+00:00,Peer Allowance,5.00,5.00,allowance\n"
                    )
                    .as_bytes(),
                )
                .unwrap();
            builder.insert("transactions.csv", blob, 0o100644).unwrap();
            let tree_id = builder.write().unwrap();
            let tree = bare.find_tree(tree_id).unwrap();
            let theirs_oid = bare.commit(None, &sig, &sig, "peer edit", &tree, &[]).unwrap();
            bare.reference("refs/lgs-auth/heads/main", theirs_oid, true, "auth tip").unwrap();

            projects_json.push(format!(
                r#"{{"name":"allowance-{}","clone_url":"{}","working_repo_path":"{}"}}"#,
                child.id,
                bare_dir.path().to_str().unwrap(),
                child_dir.to_str().unwrap(),
            ));
            child_ids.push(child.id.clone());
            let _ = ours_oid; // only needed to prove the repo has a HEAD; not asserted on directly
            bare_dirs.push(bare_dir);
        }

        let script_dir = TempDir::new().unwrap();
        let script_path = script_dir.path().join("lgs");
        let counter_path = script_dir.path().join("invocations");
        let json = format!(r#"{{"projects":[{}]}}"#, projects_json.join(","));
        // Appends one line to the counter file on every invocation, then
        // prints the canned status JSON — a shell-script fake, never the
        // real `lgs` binary.
        std::fs::write(
            &script_path,
            format!(
                "#!/bin/sh\necho invoked >> {}\ncat <<'JSON'\n{json}\nJSON\n",
                counter_path.to_str().unwrap()
            ),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        let lgs = LgsClient::new(script_path);
        let engine = ChildSyncEngine::new(lgs, backend.csv_connection.clone());

        let (message_tx, message_rx) = mpsc::channel::<SyncMessage>();
        let messenger = UiMessenger::new(message_tx, noop_wake());
        let shutdown = Arc::new(AtomicBool::new(false));

        let responder_child_ids = child_ids.clone();
        let responder = std::thread::spawn(move || {
            let mut applied: Vec<String> = Vec::new();
            loop {
                match message_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(SyncMessage::GetChildIdsRequest { response_tx }) => {
                        let _ = response_tx.send(responder_child_ids.clone());
                    }
                    Ok(SyncMessage::ApplyMerge { child_id, .. }) => applied.push(child_id),
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            applied
        });

        run_child_sync_cycles(&Some(engine), &messenger, &shutdown);
        drop(messenger);
        let applied = responder.join().unwrap();

        assert_eq!(
            applied.len(),
            2,
            "precondition: both children must have actually been synced this tick: {applied:?}"
        );

        let invocations = std::fs::read_to_string(&counter_path).unwrap_or_default();
        let count = invocations.lines().count();
        assert_eq!(
            count, 1,
            "lgs status --json must be fetched exactly once per tick regardless of child count, \
             got {count} invocation(s): {invocations:?}"
        );
    }
}
