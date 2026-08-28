//! Recursive tree checksum.
//!
//! Used to assert that a migration moved nothing, and to verify a folder copy
//! before the source is deleted. Deliberately uses `DefaultHasher` rather than
//! pulling in a crypto hash: this guards against accident, not tampering.

use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Hash every file under `root` by relative path and byte content.
///
/// Order-independent: entries are collected, sorted by relative path, then
/// hashed, so filesystem iteration order cannot change the result.
pub fn tree_checksum(root: &Path) -> Result<u64> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    collect(root, root, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = DefaultHasher::new();
    for (rel, bytes) in entries {
        rel.hash(&mut hasher);
        bytes.hash(&mut hasher);
    }
    Ok(hasher.finish())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            out.push((rel, fs::read(&path)?));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn identical_trees_hash_equal() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        write(a.path(), "kid/child.yaml", "id: kid");
        write(b.path(), "kid/child.yaml", "id: kid");
        assert_eq!(tree_checksum(a.path()).unwrap(), tree_checksum(b.path()).unwrap());
    }

    #[test]
    fn changed_content_changes_the_hash() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "kid/child.yaml", "id: kid");
        let before = tree_checksum(dir.path()).unwrap();
        write(dir.path(), "kid/child.yaml", "id: other");
        assert_ne!(before, tree_checksum(dir.path()).unwrap());
    }

    #[test]
    fn a_new_file_changes_the_hash() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "kid/child.yaml", "id: kid");
        let before = tree_checksum(dir.path()).unwrap();
        write(dir.path(), "kid/transactions.csv", "id,amount");
        assert_ne!(before, tree_checksum(dir.path()).unwrap());
    }

    #[test]
    fn a_moved_file_changes_the_hash() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "kid/child.yaml", "id: kid");
        let before = tree_checksum(dir.path()).unwrap();
        std::fs::rename(dir.path().join("kid/child.yaml"), dir.path().join("kid/moved.yaml")).unwrap();
        assert_ne!(before, tree_checksum(dir.path()).unwrap());
    }
}
