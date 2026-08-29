//! The child roster: what the UI reads instead of touching the filesystem.
//!
//! A worker walks a registry snapshot, prefetching each child's whole folder —
//! on iCloud that read *is* the download trigger — and reports status over an
//! `mpsc` channel, waking the UI the same way the sync thread does.
//!
//! Prefetching all five files rather than just `child.yaml` is deliberate. If
//! `child.yaml` is dataless then `transactions.csv` certainly is, and a
//! half-async load would simply move the freeze from the picker to the first
//! calendar render.

use shared::ChildId;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::backend::domain::child_availability::{
    classify, Availability, ChildFolderSource, ChildStatus, UnavailableReason,
};
use crate::backend::domain::WakeUi;
use crate::backend::storage::csv::{ChildRegistry, RegistryEntry};

/// Files materialized before a child is considered `Available`.
/// `.git` is excluded on purpose — see the module docs.
const PREFETCH: &[&str] = &[
    "child.yaml",
    "allowance_config.yaml",
    "transactions.csv",
    "goals.csv",
    "parental_control_attempts.csv",
];

#[derive(Debug, Clone)]
pub struct RosterEntry {
    pub entry: RegistryEntry,
    pub status: ChildStatus,
}

#[derive(Debug)]
pub enum RosterMessage {
    Status { generation: u64, id: ChildId, status: ChildStatus },
    Finished { generation: u64 },
}

pub struct ChildRoster {
    entries: Vec<RosterEntry>,
    generation: u64,
    /// Labels changed by an `Available` status this walk, awaiting a single
    /// persist at `Finished`. Not written to disk here — see `drain_changed_labels`.
    changed_labels: Vec<(ChildId, String)>,
}

impl ChildRoster {
    /// Build a roster from a registry snapshot. Every entry starts as
    /// `Downloading` so a cold child reads as "Downloading from iCloud…"
    /// rather than vanishing — a silently empty picker is indistinguishable
    /// from the bug this design exists to fix.
    pub fn new(registry: Arc<ChildRegistry>, generation: u64) -> Self {
        Self {
            entries: registry
                .entries()
                .iter()
                .map(|entry| RosterEntry { entry: entry.clone(), status: ChildStatus::Downloading })
                .collect(),
            generation,
            changed_labels: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[RosterEntry] {
        &self.entries
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn status_of(&self, id: &ChildId) -> Option<&ChildStatus> {
        self.entries.iter().find(|e| &e.entry.id == id).map(|e| &e.status)
    }

    /// Ids safe to act on: sync polls only these, and only these can be active.
    pub fn available_ids(&self) -> Vec<ChildId> {
        self.entries
            .iter()
            .filter(|e| matches!(e.status, ChildStatus::Available(_)))
            .map(|e| e.entry.id.clone())
            .collect()
    }

    /// Apply a worker message, discarding results from a superseded walk.
    ///
    /// An `Available` status whose child name differs from the cached
    /// `RegistryEntry::label` updates the in-memory label immediately (so the
    /// UI paints the fresh name right away) and records the change so a
    /// caller can persist all of this walk's renames in one registry write
    /// via `drain_changed_labels`. This method itself never touches disk —
    /// the worker holds only a snapshot, and writing from here would race the
    /// UI thread's own registry mutations.
    pub fn apply(&mut self, msg: RosterMessage) {
        match msg {
            RosterMessage::Status { generation, id, status } => {
                if generation != self.generation {
                    return;
                }
                if let Some(e) = self.entries.iter_mut().find(|e| e.entry.id == id) {
                    if let ChildStatus::Available(ref child) = status {
                        if e.entry.label != child.name {
                            self.changed_labels.push((id.clone(), child.name.clone()));
                            e.entry.label = child.name.clone();
                        }
                    }
                    e.status = status;
                }
            }
            RosterMessage::Finished { .. } => {}
        }
    }

    /// Take the labels changed since the last drain, clearing the internal
    /// list. Callers persist these with a single `ChildRegistry` write —
    /// typically on `RosterMessage::Finished`, so one walk produces at most
    /// one disk write regardless of how many children were renamed.
    pub fn drain_changed_labels(&mut self) -> Vec<(ChildId, String)> {
        std::mem::take(&mut self.changed_labels)
    }

    /// Mark one entry for reload — used when a sync-applied rename arrives,
    /// which changes `child.yaml` without changing the registry.
    pub fn mark_stale(&mut self, id: &ChildId) {
        if let Some(e) = self.entries.iter_mut().find(|e| &e.entry.id == id) {
            e.status = ChildStatus::Downloading;
        }
    }
}

/// Walk the registry on a worker thread, reporting each child's status.
pub fn spawn_loader(
    registry: Arc<ChildRegistry>,
    source: Arc<dyn ChildFolderSource>,
    generation: u64,
    tx: Sender<RosterMessage>,
    wake: WakeUi,
) {
    std::thread::spawn(move || {
        for entry in registry.entries() {
            let status = load_one(&entry.id, &entry.path, source.as_ref(), generation, &tx, &wake);
            let _ = tx.send(RosterMessage::Status {
                generation,
                id: entry.id.clone(),
                status,
            });
            wake();
        }
        let _ = tx.send(RosterMessage::Finished { generation });
        wake();
    });
}

fn load_one(
    id: &ChildId,
    dir: &Path,
    source: &dyn ChildFolderSource,
    generation: u64,
    tx: &Sender<RosterMessage>,
    wake: &WakeUi,
) -> ChildStatus {
    let yaml_path = dir.join("child.yaml");

    match source.probe(&yaml_path) {
        Availability::Missing => {
            // Distinguish "folder gone" from "folder present, not a child folder".
            let reason = if dir.exists() {
                UnavailableReason::NotAChildFolder
            } else {
                UnavailableReason::PathMissing
            };
            return ChildStatus::Unavailable(reason);
        }
        Availability::Dataless => {
            // Report before the blocking read so the UI can paint a label
            // instead of freezing with nothing to show.
            let _ = tx.send(RosterMessage::Status {
                generation,
                id: id.clone(),
                status: ChildStatus::Downloading,
            });
            wake();
        }
        Availability::Materialized => {}
    }

    let yaml = source.read(&yaml_path);
    let meta = std::fs::metadata(dir);
    let status = classify(id, meta, yaml);

    // Materialize the rest of the folder so downstream synchronous reads hit
    // warm files. Failures here are not fatal: the child is loadable, and a
    // later read will block once rather than never resolving.
    if matches!(status, ChildStatus::Available(_)) {
        for name in PREFETCH.iter().skip(1) {
            let path = dir.join(name);
            if path.exists() {
                let _ = source.read(&path);
            }
        }
    }

    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::domain::models::child::Child;
    use chrono::{NaiveDate, TimeZone, Utc};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::sync::{Arc, Barrier};

    const VALID_YAML: &str = "id: kid\nname: Kid\nbirthdate: '2010-01-01'\n\
                              created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n";

    /// A fake whose `read` blocks on a barrier the test releases. This is the one
    /// thing the pure `classify` cannot pin: that `Downloading` is observable
    /// before the blocking read completes.
    struct BlockingSource {
        availability: Availability,
        release: Arc<Barrier>,
    }

    impl ChildFolderSource for BlockingSource {
        fn probe(&self, _p: &Path) -> Availability { self.availability }
        fn read(&self, _p: &Path) -> std::io::Result<String> {
            self.release.wait();
            Ok(VALID_YAML.to_string())
        }
    }

    fn registry_with_one() -> Arc<ChildRegistry> {
        let mut reg = ChildRegistry::default();
        reg.register(RegistryEntry {
            id: ChildId::from("kid"),
            path: PathBuf::from("/data/kid"),
            label: "Kid".into(),
        })
        .unwrap();
        Arc::new(reg)
    }

    fn child_named(name: &str) -> Child {
        Child {
            id: "kid".into(),
            name: name.into(),
            birthdate: NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            created_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn a_dataless_folder_reports_downloading_before_available() {
        // A *real* dataless file is still present and stat-able at its real
        // path — only its content is unmaterialized (see the module docs).
        // So this uses a genuine, existing temp dir (sans `child.yaml`, which
        // `BlockingSource::read` fakes) rather than the fictional path the
        // other tests use, so `load_one`'s own `std::fs::metadata(dir)` call
        // — deliberately real, not routed through `ChildFolderSource` — sees
        // what a truly-present-but-cold folder looks like.
        let tmp = tempfile::TempDir::new().unwrap();
        let mut reg = ChildRegistry::default();
        reg.register(RegistryEntry {
            id: ChildId::from("kid"),
            path: tmp.path().to_path_buf(),
            label: "Kid".into(),
        })
        .unwrap();
        let registry = Arc::new(reg);

        let barrier = Arc::new(Barrier::new(2));
        let source = Arc::new(BlockingSource {
            availability: Availability::Dataless,
            release: barrier.clone(),
        });
        let (tx, rx) = mpsc::channel();

        spawn_loader(registry, source, 1, tx, Arc::new(|| {}));

        // The read is still blocked, so the only message so far must be Downloading.
        let first = rx.recv().unwrap();
        assert!(matches!(
            first,
            RosterMessage::Status { ref status, .. } if *status == ChildStatus::Downloading
        ));

        barrier.wait(); // let the read complete

        let second = rx.recv().unwrap();
        assert!(matches!(
            second,
            RosterMessage::Status { ref status, .. } if matches!(status, ChildStatus::Available(_))
        ));
    }

    #[test]
    fn a_missing_folder_reports_path_missing_without_blocking() {
        struct MissingSource;
        impl ChildFolderSource for MissingSource {
            fn probe(&self, _p: &Path) -> Availability { Availability::Missing }
            fn read(&self, _p: &Path) -> std::io::Result<String> {
                panic!("must not read a missing folder");
            }
        }
        let (tx, rx) = mpsc::channel();
        spawn_loader(registry_with_one(), Arc::new(MissingSource), 1, tx, Arc::new(|| {}));

        let msg = rx.recv().unwrap();
        assert!(matches!(
            msg,
            RosterMessage::Status { ref status, .. }
                if *status == ChildStatus::Unavailable(UnavailableReason::PathMissing)
        ));
    }

    #[test]
    fn a_stale_generation_is_discarded() {
        let mut roster = ChildRoster::new(registry_with_one(), 2);
        roster.apply(RosterMessage::Status {
            generation: 1, // older than the roster's current generation
            id: ChildId::from("kid"),
            status: ChildStatus::Unavailable(UnavailableReason::PathMissing),
        });
        assert_eq!(roster.status_of(&ChildId::from("kid")), Some(&ChildStatus::Downloading));
    }

    #[test]
    fn available_ids_excludes_downloading_and_unavailable() {
        let mut roster = ChildRoster::new(registry_with_one(), 1);
        assert!(roster.available_ids().is_empty(), "nothing is available before loading");

        roster.apply(RosterMessage::Status {
            generation: 1,
            id: ChildId::from("kid"),
            status: ChildStatus::Unavailable(UnavailableReason::PathMissing),
        });
        assert!(roster.available_ids().is_empty());
    }

    /// Cached labels are persisted once at the end of a roster walk, not per
    /// child — see `drain_changed_labels`. This pins: a rename surfaces, an
    /// unchanged name produces nothing, and draining is destructive.
    #[test]
    fn drain_changed_labels_tracks_renames_only_and_is_destructive() {
        let mut roster = ChildRoster::new(registry_with_one(), 1);

        // Same name as the cached label ("Kid") — nothing changed.
        roster.apply(RosterMessage::Status {
            generation: 1,
            id: ChildId::from("kid"),
            status: ChildStatus::Available(child_named("Kid")),
        });
        assert!(
            roster.drain_changed_labels().is_empty(),
            "an unchanged name must not be reported as a change"
        );

        // A different name — this is a rename.
        roster.apply(RosterMessage::Status {
            generation: 1,
            id: ChildId::from("kid"),
            status: ChildStatus::Available(child_named("New Name")),
        });
        assert_eq!(
            roster.drain_changed_labels(),
            vec![(ChildId::from("kid"), "New Name".to_string())]
        );

        // Draining again returns nothing — the list was cleared by the drain above.
        assert!(roster.drain_changed_labels().is_empty(), "drain must clear the list");
    }
}
