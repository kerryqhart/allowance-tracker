use shared::sync::*;
use std::sync::mpsc;

/// Non-blocking sender for sync events. Injected into repositories.
/// Sending is best-effort: if the channel is full or disconnected,
/// the event is dropped with a warning log. Local writes must never
/// fail because of sync.
#[derive(Clone)]
pub struct SyncNotifier {
    tx: mpsc::Sender<SyncEvent>,
}

impl SyncNotifier {
    pub fn new(tx: mpsc::Sender<SyncEvent>) -> Self {
        Self { tx }
    }

    pub fn notify(&self, event: SyncEvent) {
        if let Err(e) = self.tx.send(event) {
            log::warn!("Failed to send sync event (channel disconnected): {}", e);
        }
    }
}

/// Create a (SyncNotifier, Receiver) pair.
pub fn sync_channel() -> (SyncNotifier, mpsc::Receiver<SyncEvent>) {
    let (tx, rx) = mpsc::channel();
    (SyncNotifier::new(tx), rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notify_sends_event() {
        let (notifier, rx) = sync_channel();

        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        notifier.notify(event.clone());

        let received = rx.recv().unwrap();
        assert_eq!(received.event_id, event.event_id);
        assert_eq!(received.entity_id, "tx1");
    }

    #[test]
    fn test_notify_on_disconnected_channel_does_not_panic() {
        let (tx, rx) = mpsc::channel::<SyncEvent>();
        let notifier = SyncNotifier::new(tx);
        drop(rx); // disconnect the receiver

        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        // Should not panic
        notifier.notify(event);
    }

    #[test]
    fn test_notifier_is_clone() {
        let (notifier, rx) = sync_channel();
        let notifier2 = notifier.clone();

        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        notifier2.notify(event);
        let _ = rx.recv().unwrap();
    }
}
