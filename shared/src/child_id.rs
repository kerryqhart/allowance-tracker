use serde::{Deserialize, Serialize};
use std::fmt;

/// The immutable identity of a child.
///
/// This is the key for directory resolution, the sync-service partition key,
/// and the registry map key. It is minted once at creation from the sanitized
/// display name and never changes afterwards — renaming a child does not
/// change their `ChildId`.
///
/// `#[serde(transparent)]` keeps the wire format a bare string, so existing
/// `child.yaml`, `transactions.csv`, and sync payloads stay byte-compatible.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChildId(String);

impl ChildId {
    pub fn new(id: impl Into<String>) -> Self {
        ChildId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ChildId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ChildId {
    fn from(s: &str) -> Self {
        ChildId(s.to_string())
    }
}

impl From<String> for ChildId {
    fn from(s: String) -> Self {
        ChildId(s)
    }
}

impl fmt::Display for ChildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_transparently_as_a_bare_string() {
        let id = ChildId::new("keiko_hart");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"keiko_hart\"");
    }

    #[test]
    fn round_trips_through_yaml() {
        let id = ChildId::new("keiko_hart");
        let yaml = serde_yaml::to_string(&id).unwrap();
        let back: ChildId = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn borrows_as_str_and_displays() {
        let id = ChildId::from("keiko_hart");
        assert_eq!(id.as_str(), "keiko_hart");
        assert_eq!(id.as_ref() as &str, "keiko_hart");
        assert_eq!(format!("{}", id), "keiko_hart");
    }
}
