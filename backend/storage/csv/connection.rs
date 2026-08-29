use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use shared::ChildId;

use super::child_registry::ChildRegistry;
use crate::backend::storage::traits::Connection;

/// `CsvConnection` owns the machine-local base directory and the child
/// registry, and is the single place that maps a `ChildId` to a folder.
///
/// Before the registry there were three conventions in flight — the id, the
/// sanitized display name, and a scan of every `child.yaml` under the base
/// directory — and they agreed only because id, folder name, and sanitized
/// name happened to be the same string. Everything now resolves through
/// [`CsvConnection::child_dir`], keyed on the immutable id.
#[derive(Clone, Debug)]
pub struct CsvConnection {
    /// The machine-local base directory. A plain `PathBuf` now that relocate
    /// and revert are gone — those were the only mutators.
    base_directory: PathBuf,
    /// Copy-on-write registry. `CsvConnection` is `Clone` and shared as an
    /// `Arc` across the services, so readers take a snapshot (`Arc` clone) and
    /// hold borrows against it; writers rebuild and swap. The lock is held for
    /// a pointer copy and never across I/O.
    registry: Arc<Mutex<Arc<ChildRegistry>>>,
}

impl CsvConnection {
    /// Create a new CSV connection with a base directory
    pub fn new<P: AsRef<Path>>(base_directory: P) -> Result<Self> {
        let base_path = base_directory.as_ref().to_path_buf();

        if !base_path.exists() {
            fs::create_dir_all(&base_path)?;
        }

        let registry = ChildRegistry::load(&base_path)?;

        Ok(Self {
            base_directory: base_path,
            registry: Arc::new(Mutex::new(Arc::new(registry))),
        })
    }

    /// Get the base directory path
    pub fn base_directory(&self) -> &Path {
        &self.base_directory
    }

    /// A stable snapshot of the registry. Cheap: one `Arc` clone.
    pub fn registry(&self) -> Arc<ChildRegistry> {
        self.registry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Mutate the registry: clone, apply, persist, then swap the new value in.
    ///
    /// Persisting before the swap means a write failure leaves the in-memory
    /// registry untouched rather than diverging from disk.
    pub fn update_registry(
        &self,
        f: impl FnOnce(&mut ChildRegistry) -> Result<()>,
    ) -> Result<()> {
        let current = self.registry();
        let mut next = (*current).clone();
        f(&mut next)?;
        next.save(&self.base_directory)?;
        *self.registry.lock().unwrap_or_else(|e| e.into_inner()) = Arc::new(next);
        Ok(())
    }

    /// Resolve a child's folder.
    ///
    /// This is a registry lookup **plus one `stat` of `child.yaml`**, and the
    /// `stat` is not optional. The registry converts "child not found" — a
    /// safe, self-limiting failure under the old directory scan — into "child
    /// found at a path we will happily create." Combined with a read path that
    /// once called `create_dir_all`, an unavailable child would get a
    /// fabricated folder, a $0.00 balance, and writes pushed to sync as truth.
    ///
    /// `stat` does not materialize a dataless iCloud file, so this costs
    /// nothing on the cold-folder path.
    pub fn child_dir(&self, id: &ChildId) -> Result<PathBuf> {
        let dir = self.child_dir_for_create(id)?;
        if !dir.join("child.yaml").exists() {
            anyhow::bail!(
                "child '{}' is registered at {} but no child.yaml is there",
                id,
                dir.display()
            );
        }
        Ok(dir)
    }

    /// Resolve without the existence check.
    ///
    /// Exactly one caller is legitimate: writing `child.yaml` for the first
    /// time, where the file cannot exist yet. Registration happens before the
    /// write (mkdir -> register -> write `child.yaml`), so the entry is
    /// present by the time this is called.
    pub fn child_dir_for_create(&self, id: &ChildId) -> Result<PathBuf> {
        self.registry()
            .path_for(id)
            .map(|p| p.to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("child '{}' is not registered on this machine", id))
    }

    /// Get the file path for a child's transactions
    pub fn transactions_path(&self, id: &ChildId) -> Result<PathBuf> {
        Ok(self.child_dir(id)?.join("transactions.csv"))
    }

    /// Get the file path for a child's goals
    pub fn goals_path(&self, id: &ChildId) -> Result<PathBuf> {
        Ok(self.child_dir(id)?.join("goals.csv"))
    }

    /// Ensure the transactions file exists, with its header.
    ///
    /// Deliberately does **not** create the child directory. Folder creation is
    /// a registration-time act; a read must never manufacture a child folder.
    pub fn ensure_transactions_file_exists(&self, id: &ChildId) -> Result<()> {
        let child_dir = self.child_dir(id)?;
        let file_path = child_dir.join("transactions.csv");

        if !file_path.exists() {
            fs::write(&file_path, "id,child_id,date,description,amount,balance\n")?;
        }

        Ok(())
    }

    /// Clean up test data (useful for tests)
    #[cfg(test)]
    pub fn cleanup(&self) -> Result<()> {
        if self.base_directory.exists() {
            fs::remove_dir_all(&self.base_directory)?;
        }
        Ok(())
    }

    // ========================================================================
    // DIRECTORY NAME MINTING
    // ========================================================================

    /// Generate a safe directory name from a child's name.
    ///
    /// This mints an id and folder name for a **newly created** child. It is
    /// no longer a resolver: once a child exists, its folder is found through
    /// the registry, keyed on the immutable id.
    pub fn generate_safe_directory_name(child_name: &str) -> String {
        child_name
            .to_lowercase()
            .trim()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c
                } else if c.is_whitespace() || c == '-' || c == '_' {
                    '_'
                } else {
                    // Convert accented characters to their base equivalents
                    match c {
                        'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
                        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
                        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
                        'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
                        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
                        'ñ' | 'Ñ' => 'n',
                        'ç' | 'Ç' => 'c',
                        'ß' => 's',
                        _ => ' ', // Skip other special characters
                    }
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join("_")
            .trim_matches('_')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::csv::child_registry::RegistryEntry;
    use tempfile::TempDir;

    fn conn_with_child(dir: &Path, id: &str) -> CsvConnection {
        let folder = dir.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("child.yaml"), format!("id: {id}\n")).unwrap();

        let conn = CsvConnection::new(dir).unwrap();
        conn.update_registry(|reg| {
            reg.register(RegistryEntry {
                id: ChildId::from(id),
                path: folder.clone(),
                label: "Test".to_string(),
            })
        })
        .unwrap();
        conn
    }

    #[test]
    fn child_dir_returns_the_registered_path() {
        let dir = TempDir::new().unwrap();
        let conn = conn_with_child(dir.path(), "child_abc");
        assert_eq!(
            conn.child_dir(&ChildId::from("child_abc")).unwrap(),
            dir.path().join("child_abc")
        );
    }

    #[test]
    fn child_dir_errors_for_an_unregistered_child() {
        let dir = TempDir::new().unwrap();
        let conn = CsvConnection::new(dir.path()).unwrap();
        assert!(conn.child_dir(&ChildId::from("nope")).is_err());
    }

    /// The design's central invariant. A registered path whose folder has gone
    /// must fail loudly, not resolve to a path something downstream will create.
    #[test]
    fn child_dir_errors_when_the_registered_folder_is_missing() {
        let dir = TempDir::new().unwrap();
        let conn = conn_with_child(dir.path(), "child_abc");
        std::fs::remove_dir_all(dir.path().join("child_abc")).unwrap();

        let err = conn.child_dir(&ChildId::from("child_abc")).unwrap_err();
        assert!(err.to_string().contains("child_abc"));
        assert!(
            !dir.path().join("child_abc").exists(),
            "resolution must not create the folder"
        );
    }

    #[test]
    fn ensure_transactions_file_does_not_create_the_child_folder() {
        let dir = TempDir::new().unwrap();
        let conn = conn_with_child(dir.path(), "child_abc");
        std::fs::remove_dir_all(dir.path().join("child_abc")).unwrap();

        assert!(conn
            .ensure_transactions_file_exists(&ChildId::from("child_abc"))
            .is_err());
        assert!(!dir.path().join("child_abc").exists());
    }

    #[test]
    fn update_registry_persists_and_is_visible_to_a_fresh_snapshot() {
        let dir = TempDir::new().unwrap();
        let conn = conn_with_child(dir.path(), "child_abc");
        assert_eq!(conn.registry().entries().len(), 1);

        let reloaded = CsvConnection::new(dir.path()).unwrap();
        assert_eq!(
            reloaded.registry().entries().len(),
            1,
            "mutation must have been persisted"
        );
    }

    #[test]
    fn a_snapshot_is_unaffected_by_a_later_mutation() {
        let dir = TempDir::new().unwrap();
        let conn = conn_with_child(dir.path(), "child_abc");
        let snapshot = conn.registry();

        conn.update_registry(|reg| reg.deregister(&ChildId::from("child_abc")))
            .unwrap();

        assert_eq!(
            snapshot.entries().len(),
            1,
            "held snapshot must not change under the reader"
        );
        assert_eq!(conn.registry().entries().len(), 0);
    }
}

impl Connection for CsvConnection {
    type TransactionRepository = super::transaction_repository::TransactionRepository;

    fn create_transaction_repository(&self) -> Self::TransactionRepository {
        super::transaction_repository::TransactionRepository::new(self.clone())
    }
}
