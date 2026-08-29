use anyhow::Result;
use log::{debug, info, warn};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use crate::backend::domain::models::child::Child as DomainChild;
use serde::{Deserialize, Serialize};
use shared::ChildId;

/// Intermediate struct for YAML serialization with string date fields
#[derive(Debug, Clone, Serialize, Deserialize)]
struct YamlChild {
    id: String,
    name: String,
    birthdate: String, // String representation for YAML
    created_at: String, // String representation for YAML
    updated_at: String, // String representation for YAML
}
use super::child_registry::RegistryEntry;
use super::connection::CsvConnection;
use super::global_config_repository::{GlobalConfigRepository, GlobalConfigStorage};
use crate::backend::storage::GitManager;
use serde_yaml;

/// CSV-based child repository backed by the child registry
#[derive(Clone)]
pub struct ChildRepository {
    connection: Arc<CsvConnection>,
    git_manager: GitManager,
}

impl ChildRepository {
    /// Create a new CSV child repository
    pub fn new(connection: Arc<CsvConnection>) -> Self {
        Self {
            connection,
            git_manager: GitManager::new(),
        }
    }

    /// `global_config.yaml` has exactly one owner: `GlobalConfigRepository`.
    /// This repository reads and writes the active child through it rather
    /// than touching the file itself.
    fn global_config(&self) -> GlobalConfigRepository {
        GlobalConfigRepository::new((*self.connection).clone())
    }

    /// List children from the registry.
    ///
    /// No directory scan and no base-dir-wide `child.yaml` parse. An entry
    /// whose folder will not load is skipped here — this method answers "who
    /// is registered", not "who is loadable".
    fn discover_children(&self) -> Result<Vec<DomainChild>> {
        let registry = self.connection.registry();
        let mut children = Vec::new();

        for entry in registry.entries() {
            match self.load_child_at(&entry.path) {
                Ok(Some(child)) => children.push(child),
                Ok(None) => debug!("No child.yaml at {}", entry.path.display()),
                Err(e) => warn!("Could not load child at {}: {}", entry.path.display(), e),
            }
        }

        // Sort children by name for consistent ordering
        children.sort_by(|a, b| a.name.cmp(&b.name));

        debug!("Discovered {} children", children.len());
        Ok(children)
    }

    /// Load a child from an absolute folder path.
    ///
    /// The old id-equals-containing-directory-name check is gone. It is
    /// incompatible with registering a folder whose basename is not the id,
    /// which is the whole point of "Add existing child…". Identity now comes
    /// from `child.yaml` alone; the registry maps it to a location.
    fn load_child_at(&self, dir: &Path) -> Result<Option<DomainChild>> {
        let yaml_path = dir.join("child.yaml");

        if !yaml_path.exists() {
            return Ok(None);
        }

        let yaml_child: YamlChild = serde_yaml::from_str(&fs::read_to_string(&yaml_path)?)?;

        Ok(Some(DomainChild {
            id: yaml_child.id,
            name: yaml_child.name,
            birthdate: chrono::NaiveDate::parse_from_str(&yaml_child.birthdate, "%Y-%m-%d")
                .map_err(|e| anyhow::anyhow!("Failed to parse birthdate: {}", e))?,
            created_at: chrono::DateTime::parse_from_rfc3339(&yaml_child.created_at)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&yaml_child.updated_at)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&chrono::Utc),
        }))
    }

    /// Write `child.yaml` into an already-resolved child folder.
    fn write_child_yaml(&self, child: &DomainChild, child_dir: &Path) -> Result<()> {
        let yaml_child = YamlChild {
            id: child.id.clone(),
            name: child.name.clone(),
            birthdate: child.birthdate.format("%Y-%m-%d").to_string(),
            created_at: child.created_at.to_rfc3339(),
            updated_at: child.updated_at.to_rfc3339(),
        };

        let yaml_path = child_dir.join("child.yaml");
        let yaml_content = serde_yaml::to_string(&yaml_child)?;

        // Atomic write using temp file
        let temp_path = yaml_path.with_extension("tmp");
        fs::write(&temp_path, yaml_content)?;
        fs::rename(&temp_path, &yaml_path)?;

        info!("Saved child {} to directory: {}", child.name, child_dir.display());

        // Git commit the child.yaml change
        let action_description = format!("Updated child profile: {}", child.name);
        let _ = self.git_manager.commit_file_change(
            child_dir,
            "child.yaml",
            &action_description
        );

        // Keep the registry's display cache in step with `child.yaml`.
        let id = ChildId::from(child.id.as_str());
        let label = child.name.clone();
        if self
            .connection
            .registry()
            .entries()
            .iter()
            .any(|e| e.id == id && e.label != label)
        {
            // A failed write leaves the cached label stale, so the picker would
            // show the old name. Not worth failing the operation over — the
            // child itself saved fine and the label is only a display cache —
            // but it must not vanish silently: `children.yaml` can sit on a
            // read-only, full, or unmounted path.
            if let Err(e) = self.connection.update_registry(|reg| {
                reg.set_label(&id, &label);
                Ok(())
            }) {
                warn!(
                    "Could not refresh the cached display name for child '{}' to '{}' in the registry; \
                     the child picker may show a stale name until this is written again: {}",
                    id, label, e
                );
            }
        }

        Ok(())
    }
}

impl crate::backend::storage::ChildStorage for ChildRepository {
    /// Store a new child
    fn store_child(&self, child: &DomainChild) -> Result<()> {
        let id = ChildId::from(child.id.as_str());

        // Lifecycle ordering: mkdir -> register -> write child.yaml. `child_dir`
        // would fail for an unregistered child, so registration precedes the
        // first write rather than following it.
        if self.connection.registry().path_for(&id).is_none() {
            let folder = self.connection.base_directory().join(child.id.as_str());
            fs::create_dir_all(&folder)?;
            let entry = RegistryEntry {
                id: id.clone(),
                path: folder,
                label: child.name.clone(),
            };
            self.connection.update_registry(|reg| reg.register(entry))?;
        }

        let dir = self.connection.child_dir_for_create(&id)?;
        self.write_child_yaml(child, &dir)
    }

    /// Retrieve a specific child by ID
    fn get_child(&self, child_id: &str) -> Result<Option<DomainChild>> {
        let id = ChildId::from(child_id);
        match self.connection.child_dir(&id) {
            Ok(dir) => self.load_child_at(&dir),
            Err(e) => {
                debug!("Could not resolve child '{}': {}", child_id, e);
                Ok(None)
            }
        }
    }

    /// List all children ordered by name
    fn list_children(&self) -> Result<Vec<DomainChild>> {
        self.discover_children()
    }

    /// Update an existing child
    fn update_child(&self, child: &DomainChild) -> Result<()> {
        let id = ChildId::from(child.id.as_str());
        match self.connection.child_dir(&id) {
            Ok(dir) => self.write_child_yaml(child, &dir),
            Err(e) => {
                warn!("Attempted to update a non-existent child {}: {}", child.id, e);
                Err(anyhow::anyhow!("Child not found for update"))
            }
        }
    }

    /// Delete a child by ID
    /// Delete a child: remove its folder, then deregister it.
    ///
    /// Resolution goes through `child_dir_for_create`, **not** `child_dir`.
    /// `child_dir` additionally requires `child.yaml` to be present, and a
    /// folder that has lost its `child.yaml` is precisely the damaged state a
    /// user reaches for delete to clean up. Resolving through `child_dir` here
    /// meant that case skipped the removal, deregistered anyway, and left the
    /// folder behind with nothing in the registry still naming it — an orphan
    /// no UI could see or reach.
    ///
    /// The registered path is only ever bound by a flow that first validated a
    /// `child.yaml` there (create, Add existing child…, Locate…), so this is a
    /// child's folder even when its `child.yaml` has since gone missing.
    ///
    /// Order is remove-then-deregister: a failed removal aborts with the entry
    /// still in place, so the operation is retryable and the registry never
    /// describes a machine state that isn't true.
    fn delete_child(&self, child_id: &str) -> Result<()> {
        let id = ChildId::from(child_id);

        match self.connection.child_dir_for_create(&id) {
            Ok(child_dir) => {
                if child_dir.exists() {
                    fs::remove_dir_all(&child_dir)?;
                    info!("Deleted child directory: {:?}", child_dir);
                } else {
                    warn!(
                        "Child {} was registered at {} but nothing is there; deregistering only",
                        child_id,
                        child_dir.display()
                    );
                }
            }
            Err(e) => {
                warn!("Attempted to delete a non-existent child {}: {}", child_id, e);
                return Ok(());
            }
        }

        self.connection.update_registry(|reg| reg.deregister(&id))?;

        Ok(())
    }

    /// Get the currently active child
    fn get_active_child(&self) -> Result<Option<String>> {
        // Resolve through the repository's own accessor so this and the UI's
        // `active_child_id()` read the same key — a migrated config's legacy
        // `active_child_directory` holds a folder name, not an id.
        let Some(active_id) = self.global_config().active_child_id()? else {
            return Ok(None);
        };

        // Confirm the child is still resolvable before reporting it active.
        Ok(self.get_child(active_id.as_str())?.map(|c| c.id))
    }

    /// Set the currently active child
    fn set_active_child(&self, child_id: &str) -> Result<()> {
        self.global_config()
            .set_active_child_directory(Some(child_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::backend::storage::ChildStorage;

    fn setup_test_repo() -> (ChildRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let connection = CsvConnection::new(temp_dir.path()).unwrap();
        let repo = ChildRepository::new(Arc::new(connection));
        (repo, temp_dir)
    }

    #[test]
    fn test_store_and_discover_child() {
        let (repo, _temp_dir) = setup_test_repo();

        // Create a child
        let now = chrono::Utc::now();
        let child = DomainChild {
            id: "test_child".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2015, 5, 15).unwrap(),
            created_at: now,
            updated_at: now,
        };

        // Store the child
        repo.store_child(&child).expect("Failed to store child");

        // Discover children
        let children = repo.list_children().expect("Failed to list children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "Test Child");
        assert_eq!(children[0].id, "test_child");

        // Get the specific child
        let retrieved_child = repo.get_child("test_child").expect("Failed to get child");
        assert!(retrieved_child.is_some());
        assert_eq!(retrieved_child.unwrap().name, "Test Child");
    }

    /// The registry, not the folder's basename, decides where a child lives.
    /// A folder whose name is not the id must still load — this is what makes
    /// "Add existing child…" possible.
    #[test]
    fn a_folder_whose_basename_is_not_the_id_still_loads() {
        let (repo, temp_dir) = setup_test_repo();

        let folder = temp_dir.path().join("some other folder");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("child.yaml"),
            "id: child_abc_123\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
             created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n",
        )
        .unwrap();

        repo.connection
            .update_registry(|reg| {
                reg.register(RegistryEntry {
                    id: ChildId::from("child_abc_123"),
                    path: folder.clone(),
                    label: "Keiko Hart".to_string(),
                })
            })
            .unwrap();

        let child = repo.get_child("child_abc_123").unwrap().expect("child must load");
        assert_eq!(child.name, "Keiko Hart");
    }

    /// Delete must not leave the folder behind when `child.yaml` is missing.
    ///
    /// That folder is unreachable afterwards — nothing in `children.yaml`
    /// names it and no screen lists it — so a "delete" that leaves it is a
    /// silent inconsistency, not a conservative choice.
    #[test]
    fn delete_removes_the_folder_even_without_a_child_yaml() {
        let (repo, temp_dir) = setup_test_repo();

        let now = chrono::Utc::now();
        let child = DomainChild {
            id: "test_child".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2015, 5, 15).unwrap(),
            created_at: now,
            updated_at: now,
        };
        repo.store_child(&child).unwrap();

        let folder = temp_dir.path().join("test_child");
        std::fs::remove_file(folder.join("child.yaml")).unwrap();
        std::fs::write(folder.join("transactions.csv"), "id,child_id\n").unwrap();

        repo.delete_child("test_child").unwrap();

        assert!(!folder.exists(), "the folder must not be orphaned");
        assert!(repo
            .connection
            .registry()
            .path_for(&ChildId::from("test_child"))
            .is_none());
    }

    /// An unregistered child is not an error, and deleting one must not
    /// invent a path to remove.
    #[test]
    fn delete_of_an_unregistered_child_is_a_no_op() {
        let (repo, _temp_dir) = setup_test_repo();
        repo.delete_child("never_existed").unwrap();
    }

    #[test]
    fn test_active_child_management() {
        let (repo, _temp_dir) = setup_test_repo();

        // Initially, no active child
        let active_child_id = repo.get_active_child().expect("Failed to get active child");
        assert!(active_child_id.is_none());

        // Create and store a child
        let now = chrono::Utc::now();
        let child = DomainChild {
            id: "test_child".to_string(),
            name: "Active Child".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2018, 8, 8).unwrap(),
            created_at: now,
            updated_at: now,
        };
        repo.store_child(&child).expect("Failed to store child");

        // Set active child
        repo.set_active_child("test_child").expect("Failed to set active child");

        // Get active child
        let active_child_id = repo.get_active_child().expect("Failed to get active child");
        assert_eq!(active_child_id, Some("test_child".to_string()));
    }

    /// The registry's `label` is only a display cache, so a failure to refresh
    /// it must not fail the save — but it must not vanish silently either.
    /// `children.yaml` sits on a path that can be read-only, full, or on an
    /// unmounted volume, so this is a failure that actually happens.
    ///
    /// Making the base directory unwritable blocks `children.yaml.tmp` while
    /// still allowing the write of `child.yaml` inside the child's own folder:
    /// on Unix, creating an entry needs write permission on the *containing*
    /// directory, and the child directory keeps its own.
    #[cfg(unix)]
    #[test]
    fn a_failed_label_refresh_does_not_fail_the_save() {
        use std::os::unix::fs::PermissionsExt;

        let (repo, temp_dir) = setup_test_repo();

        let now = chrono::Utc::now();
        let mut child = DomainChild {
            id: "test_child".to_string(),
            name: "Keiko Hart".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2015, 5, 15).unwrap(),
            created_at: now,
            updated_at: now,
        };
        repo.store_child(&child).unwrap();

        let base = temp_dir.path();
        let original = std::fs::metadata(base).unwrap().permissions();

        // Read + execute only: entries can be resolved, but none created.
        std::fs::set_permissions(base, std::fs::Permissions::from_mode(0o500)).unwrap();

        child.name = "Keiko Smith".to_string();
        let result = repo.update_child(&child);

        // Restore before asserting, so a failed assertion still leaves the
        // TempDir removable.
        std::fs::set_permissions(base, original).unwrap();

        assert!(
            result.is_ok(),
            "a stale display cache must not fail the save: {:?}",
            result.err()
        );

        // child.yaml is the source of truth and did get the new name...
        let reloaded = repo.get_child("test_child").unwrap().unwrap();
        assert_eq!(reloaded.name, "Keiko Smith");

        // ...while the cached label stayed behind, which is exactly the
        // condition the warning exists to report.
        let registry = repo.connection.registry();
        let entry = registry
            .entries()
            .iter()
            .find(|e| e.id == ChildId::from("test_child"))
            .unwrap();
        assert_eq!(
            entry.label, "Keiko Hart",
            "the label refresh was expected to fail and leave the cache stale"
        );
    }
}
