//! Child folder availability.
//!
//! Split deliberately into a pure decision and an I/O shell. `classify` takes
//! the *results* of a `stat` and a `read` and returns a status, so five of the
//! six outcomes are testable with synthesized inputs and no filesystem at all.
//! The trait exists only for the one thing a pure function cannot pin: that a
//! dataless folder reports `Downloading` *before* the blocking read completes.

use serde::Deserialize;
use shared::ChildId;
use std::fs::Metadata;
use std::io;
use std::path::Path;

use crate::backend::domain::models::child::Child;

/// macOS: file is a dataless object — present, sized, not yet materialized.
const SF_DATALESS: u32 = 0x4000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Materialized,
    Dataless,
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnavailableReason {
    PathMissing,
    NotAChildFolder,
    IdMismatch { found: String },
    /// Stringified at this boundary on purpose: `ChildStatus` crosses an
    /// `mpsc` channel into UI state and must be `Clone`, and `io::Error` is
    /// not. Do not "fix" this back to a typed error.
    ReadFailed(String),
    ParseFailed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChildStatus {
    Available(Child),
    Downloading,
    Unavailable(UnavailableReason),
}

#[derive(Deserialize)]
struct YamlChild {
    id: String,
    name: String,
    birthdate: String,
    created_at: String,
    updated_at: String,
}

/// Decide a child's status from the results of a `stat` and a `read`.
pub fn classify(
    id: &ChildId,
    meta: io::Result<Metadata>,
    yaml: io::Result<String>,
) -> ChildStatus {
    if meta.is_err() {
        return ChildStatus::Unavailable(UnavailableReason::PathMissing);
    }

    let text = match yaml {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return ChildStatus::Unavailable(UnavailableReason::NotAChildFolder)
        }
        Err(e) => return ChildStatus::Unavailable(UnavailableReason::ReadFailed(e.to_string())),
    };

    let parsed: YamlChild = match serde_yaml::from_str(&text) {
        Ok(p) => p,
        Err(e) => return ChildStatus::Unavailable(UnavailableReason::ParseFailed(e.to_string())),
    };

    if parsed.id != id.as_str() {
        return ChildStatus::Unavailable(UnavailableReason::IdMismatch { found: parsed.id });
    }

    let birthdate = match chrono::NaiveDate::parse_from_str(&parsed.birthdate, "%Y-%m-%d") {
        Ok(d) => d,
        Err(e) => return ChildStatus::Unavailable(UnavailableReason::ParseFailed(e.to_string())),
    };
    let created_at = match chrono::DateTime::parse_from_rfc3339(&parsed.created_at) {
        Ok(d) => d.with_timezone(&chrono::Utc),
        Err(e) => return ChildStatus::Unavailable(UnavailableReason::ParseFailed(e.to_string())),
    };
    let updated_at = match chrono::DateTime::parse_from_rfc3339(&parsed.updated_at) {
        Ok(d) => d.with_timezone(&chrono::Utc),
        Err(e) => return ChildStatus::Unavailable(UnavailableReason::ParseFailed(e.to_string())),
    };

    ChildStatus::Available(Child {
        id: parsed.id,
        name: parsed.name,
        birthdate,
        created_at,
        updated_at,
    })
}

/// The I/O shell. Faked in tests so the read can be made to block.
pub trait ChildFolderSource: Send + Sync {
    fn probe(&self, path: &Path) -> Availability;
    fn read(&self, path: &Path) -> io::Result<String>;
}

pub struct RealFolderSource;

impl ChildFolderSource for RealFolderSource {
    /// `stat` only — reading metadata does not materialize a dataless file,
    /// which is what makes this a free pre-check before a blocking read.
    fn probe(&self, path: &Path) -> Availability {
        match std::fs::metadata(path) {
            Err(_) => Availability::Missing,
            Ok(meta) => {
                if is_dataless(&meta) {
                    Availability::Dataless
                } else {
                    Availability::Materialized
                }
            }
        }
    }

    /// This read is the iCloud download trigger. On a dataless file it traps to
    /// `fileproviderd` and blocks until the bytes arrive — which is why every
    /// caller must be off the UI thread.
    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
}

#[cfg(target_os = "macos")]
fn is_dataless(meta: &Metadata) -> bool {
    use std::os::darwin::fs::MetadataExt;
    meta.st_flags() & SF_DATALESS != 0
}

#[cfg(not(target_os = "macos"))]
fn is_dataless(_meta: &Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    const VALID_YAML: &str = "id: keiko_hart\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
                              created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n";

    fn missing() -> Error {
        Error::new(ErrorKind::NotFound, "no such file")
    }

    fn fake_meta() -> std::fs::Metadata {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::metadata(f.path()).unwrap()
    }

    #[test]
    fn a_readable_matching_yaml_is_available() {
        let status = classify(&ChildId::from("keiko_hart"), Ok(fake_meta()), Ok(VALID_YAML.into()));
        match status {
            ChildStatus::Available(child) => assert_eq!(child.name, "Keiko Hart"),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_folder_is_path_missing() {
        let status = classify(&ChildId::from("keiko_hart"), Err(missing()), Err(missing()));
        assert_eq!(status, ChildStatus::Unavailable(UnavailableReason::PathMissing));
    }

    #[test]
    fn a_present_folder_without_child_yaml_is_not_a_child_folder() {
        let status = classify(&ChildId::from("keiko_hart"), Ok(fake_meta()), Err(missing()));
        assert_eq!(status, ChildStatus::Unavailable(UnavailableReason::NotAChildFolder));
    }

    #[test]
    fn an_id_that_disagrees_with_the_yaml_is_reported_with_both() {
        let status = classify(&ChildId::from("someone_else"), Ok(fake_meta()), Ok(VALID_YAML.into()));
        assert_eq!(
            status,
            ChildStatus::Unavailable(UnavailableReason::IdMismatch { found: "keiko_hart".into() })
        );
    }

    #[test]
    fn malformed_yaml_is_parse_failed() {
        let status = classify(&ChildId::from("keiko_hart"), Ok(fake_meta()), Ok("{{{ not yaml".into()));
        assert!(matches!(status, ChildStatus::Unavailable(UnavailableReason::ParseFailed(_))));
    }

    #[test]
    fn an_io_error_other_than_not_found_is_read_failed() {
        let err = Error::new(ErrorKind::Other, "device not configured");
        let status = classify(&ChildId::from("keiko_hart"), Ok(fake_meta()), Err(err));
        assert!(matches!(status, ChildStatus::Unavailable(UnavailableReason::ReadFailed(_))));
    }
}
