use sha2::{Digest, Sha256};
use shared::sync::SyncAction;

/// Compute the first 8 hex characters of SHA-256 over the given bytes.
/// Used as a stable content fingerprint inside event ids and content checks.
pub fn content_sha8(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    hex::encode(&digest[..4])  // 4 bytes = 8 hex chars
}

/// Derive a deterministic event_id for an entity write.
///
/// `Created` → `ev::created::{entity_id}`
/// `Updated` → `ev::updated::{entity_id}::{content_sha8}`
///
/// `Deleted` is currently produced by the dedicated delete path with a uuid
/// and is not changed by this module.
pub fn event_id_for(action: &SyncAction, entity_id: &str, entity_json: &str) -> String {
    match action {
        SyncAction::Created => format!("ev::created::{entity_id}"),
        SyncAction::Updated => format!("ev::updated::{entity_id}::{}", content_sha8(entity_json)),
        SyncAction::Deleted => format!("ev::deleted::{entity_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_sha8_is_stable() {
        assert_eq!(content_sha8(r#"{"a":1}"#), content_sha8(r#"{"a":1}"#));
    }

    #[test]
    fn content_sha8_distinct_for_different_input() {
        assert_ne!(content_sha8(r#"{"a":1}"#), content_sha8(r#"{"a":2}"#));
    }

    #[test]
    fn content_sha8_is_8_hex_chars() {
        let h = content_sha8("anything");
        assert_eq!(h.len(), 8);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn event_id_created_format() {
        let id = event_id_for(&SyncAction::Created, "tx1", r#"{"foo":1}"#);
        assert_eq!(id, "ev::created::tx1");
    }

    #[test]
    fn event_id_updated_includes_content_hash() {
        let id = event_id_for(&SyncAction::Updated, "tx1", r#"{"foo":1}"#);
        assert!(id.starts_with("ev::updated::tx1::"));
        assert_eq!(id.len(), "ev::updated::tx1::".len() + 8);
    }

    #[test]
    fn event_id_updated_changes_with_content() {
        let a = event_id_for(&SyncAction::Updated, "tx1", r#"{"foo":1}"#);
        let b = event_id_for(&SyncAction::Updated, "tx1", r#"{"foo":2}"#);
        assert_ne!(a, b);
    }
}
