//! # Child Registry
//!
//! Owns `children.yaml`, the machine-local list of which children this
//! installation knows about and where their self-contained folders live.
//!
//! Paths here are absolute and machine-specific by nature, which is why this
//! file lives beside the other machine-local state in the base directory and
//! must never be placed in a synced folder.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use shared::ChildId;
use std::fs;
use std::path::{Path, PathBuf};

pub const REGISTRY_FILENAME: &str = "children.yaml";
const CURRENT_VERSION: u32 = 1;

/// One registered child: its identity, where its folder lives, and a cached
/// display name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub id: ChildId,
    pub path: PathBuf,
    /// Display cache only. Refreshed from `child.yaml` on a successful load so
    /// the picker can name a child whose folder is still downloading. Never
    /// authoritative — `child.yaml` always wins.
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryFile {
    version: u32,
    children: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChildRegistry {
    entries: Vec<RegistryEntry>,
}

impl ChildRegistry {
    /// Load `children.yaml` from the base directory.
    ///
    /// An absent file is not an error — it means this machine has no children
    /// registered yet, which is the fresh-install state. A file that exists but
    /// cannot be parsed *is* an error: this file is hand-editable by design, so
    /// a typo must be diagnosable rather than silently reset.
    pub fn load(base_dir: &Path) -> Result<Self> {
        let path = base_dir.join(REGISTRY_FILENAME);
        if !path.exists() {
            return Ok(Self::default());
        }

        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let parsed: RegistryFile = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing {}", path.display()))?;

        if parsed.version != CURRENT_VERSION {
            return Err(anyhow!(
                "unsupported {} version {} (this build understands version {})",
                REGISTRY_FILENAME,
                parsed.version,
                CURRENT_VERSION
            ));
        }

        Ok(Self { entries: parsed.children })
    }

    /// Persist atomically: write a temp file, then rename over the target.
    pub fn save(&self, base_dir: &Path) -> Result<()> {
        let path = base_dir.join(REGISTRY_FILENAME);
        let file = RegistryFile {
            version: CURRENT_VERSION,
            children: self.entries.clone(),
        };
        let text = serde_yaml::to_string(&file)?;

        if !base_dir.exists() {
            fs::create_dir_all(base_dir)?;
        }

        let temp = path.with_extension("yaml.tmp");
        fs::write(&temp, text)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }

    pub fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    pub fn path_for(&self, id: &ChildId) -> Option<&Path> {
        self.entries.iter().find(|e| &e.id == id).map(|e| e.path.as_path())
    }

    /// Register a child. Rejects a duplicate id, and rejects a path already
    /// claimed under a different id.
    ///
    /// Both are refused rather than silently deduped: two folders claiming one
    /// child, or one folder claimed by two ids, is a situation only the user
    /// can resolve correctly.
    pub fn register(&mut self, entry: RegistryEntry) -> Result<()> {
        if let Some(existing) = self.entries.iter().find(|e| e.id == entry.id) {
            return Err(anyhow!(
                "child '{}' is already registered at {}",
                existing.id,
                existing.path.display()
            ));
        }
        if let Some(existing) = self.entries.iter().find(|e| e.path == entry.path) {
            return Err(anyhow!(
                "{} is already registered to child '{}'",
                existing.path.display(),
                existing.id
            ));
        }
        self.entries.push(entry);
        Ok(())
    }

    pub fn deregister(&mut self, id: &ChildId) -> Result<()> {
        let before = self.entries.len();
        self.entries.retain(|e| &e.id != id);
        if self.entries.len() == before {
            return Err(anyhow!("child '{}' is not registered", id));
        }
        Ok(())
    }

    pub fn repoint(&mut self, id: &ChildId, new_path: PathBuf) -> Result<()> {
        if let Some(clash) = self.entries.iter().find(|e| e.path == new_path && &e.id != id) {
            return Err(anyhow!(
                "{} is already registered to child '{}'",
                new_path.display(),
                clash.id
            ));
        }
        let entry = self
            .entries
            .iter_mut()
            .find(|e| &e.id == id)
            .ok_or_else(|| anyhow!("child '{}' is not registered", id))?;
        entry.path = new_path;
        Ok(())
    }

    /// Update the cached display name. Returns whether it actually changed, so
    /// a roster walk can persist once at the end instead of once per child.
    pub fn set_label(&mut self, id: &ChildId, label: &str) -> bool {
        match self.entries.iter_mut().find(|e| &e.id == id) {
            Some(entry) if entry.label != label => {
                entry.label = label.to_string();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn entry(id: &str, path: &str, label: &str) -> RegistryEntry {
        RegistryEntry {
            id: ChildId::from(id),
            path: PathBuf::from(path),
            label: label.to_string(),
        }
    }

    #[test]
    fn absent_file_loads_as_empty_registry() {
        let dir = TempDir::new().unwrap();
        let reg = ChildRegistry::load(dir.path()).unwrap();
        assert!(reg.entries().is_empty());
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = TempDir::new().unwrap();
        let mut reg = ChildRegistry::load(dir.path()).unwrap();
        reg.register(entry("keiko_hart", "/data/keiko", "Keiko Hart")).unwrap();
        reg.save(dir.path()).unwrap();

        let reloaded = ChildRegistry::load(dir.path()).unwrap();
        assert_eq!(reloaded.entries().len(), 1);
        assert_eq!(reloaded.path_for(&ChildId::from("keiko_hart")),
                   Some(Path::new("/data/keiko")));
        assert_eq!(reloaded.entries()[0].label, "Keiko Hart");
    }

    #[test]
    fn rejects_duplicate_id_and_names_the_incumbent() {
        let mut reg = ChildRegistry::default();
        reg.register(entry("keiko_hart", "/data/a", "Keiko")).unwrap();
        let err = reg.register(entry("keiko_hart", "/data/b", "Keiko")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("keiko_hart"), "error must name the id: {msg}");
        assert!(msg.contains("/data/a"), "error must name the incumbent path: {msg}");
        assert_eq!(reg.entries().len(), 1, "rejected registration must not mutate");
    }

    #[test]
    fn rejects_duplicate_path_under_a_different_id() {
        let mut reg = ChildRegistry::default();
        reg.register(entry("keiko_hart", "/data/shared", "Keiko")).unwrap();
        let err = reg.register(entry("other_kid", "/data/shared", "Other")).unwrap_err();
        assert!(err.to_string().contains("keiko_hart"));
        assert_eq!(reg.entries().len(), 1);
    }

    #[test]
    fn repoint_preserves_id_and_label() {
        let mut reg = ChildRegistry::default();
        reg.register(entry("keiko_hart", "/old", "Keiko Hart")).unwrap();
        reg.repoint(&ChildId::from("keiko_hart"), PathBuf::from("/new")).unwrap();

        let e = &reg.entries()[0];
        assert_eq!(e.id, ChildId::from("keiko_hart"));
        assert_eq!(e.label, "Keiko Hart");
        assert_eq!(e.path, PathBuf::from("/new"));
    }

    #[test]
    fn deregister_removes_only_the_named_entry() {
        let mut reg = ChildRegistry::default();
        reg.register(entry("a", "/a", "A")).unwrap();
        reg.register(entry("b", "/b", "B")).unwrap();
        reg.deregister(&ChildId::from("a")).unwrap();
        assert_eq!(reg.entries().len(), 1);
        assert_eq!(reg.entries()[0].id, ChildId::from("b"));
    }

    #[test]
    fn set_label_reports_whether_it_changed() {
        let mut reg = ChildRegistry::default();
        reg.register(entry("a", "/a", "Old")).unwrap();
        assert!(reg.set_label(&ChildId::from("a"), "New"));
        assert!(!reg.set_label(&ChildId::from("a"), "New"));
    }

    #[test]
    fn malformed_yaml_errors_rather_than_resetting() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(REGISTRY_FILENAME), "children: [ this is not: valid").unwrap();
        assert!(ChildRegistry::load(dir.path()).is_err());
    }

    #[test]
    fn unknown_version_errors_rather_than_guessing() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(REGISTRY_FILENAME), "version: 2\nchildren: []\n").unwrap();
        let err = ChildRegistry::load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("version"));
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_file() {
        let dir = TempDir::new().unwrap();
        let mut reg = ChildRegistry::default();
        reg.register(entry("a", "/a", "A")).unwrap();
        reg.save(dir.path()).unwrap();

        assert!(dir.path().join(REGISTRY_FILENAME).exists());
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp file was left behind");
    }
}
