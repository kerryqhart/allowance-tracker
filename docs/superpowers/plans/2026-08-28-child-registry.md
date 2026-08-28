# Child Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace base-directory scanning and `.allowance_redirect` stubs with an explicit `children.yaml` registry mapping `ChildId → absolute path`, so a self-contained child folder can be registered wherever it lives.

**Architecture:** A new `ChildRegistry` owns `children.yaml` and is held copy-on-write inside `CsvConnection` as `Mutex<Arc<ChildRegistry>>`. All five CSV repositories stop deriving directories from names or scanning the base dir and resolve through one fallible method, `CsvConnection::child_dir(&ChildId)`, which verifies `child.yaml` exists before returning. Child loading moves onto a worker thread that prefetches the whole folder — which is what triggers iCloud materialization — and reports status to the UI over `mpsc`.

**Tech Stack:** Rust 1.93, egui/eframe 0.31, `serde_yaml` 0.9, `anyhow`, `tempfile` (dev), `rfd` 0.15 for folder picking. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-08-28-child-registry-design.md`

## Global Constraints

- **Test command:** `cargo test -p allowance-tracker-egui` — `backend/` is a `#[path]` module inside the `allowance-tracker-egui` crate, not its own crate. Do not try `-p backend`.
- **No new dependencies.** `SF_DATALESS` is reached through `std::os::darwin::fs::MetadataExt::st_flags()`; the checksum helper uses `std::collections::hash_map::DefaultHasher`. Do not add `sha2`, `walkdir`, `objc2`, or `libc`.
- **Registry file version is `1`.** An unknown version is an error, never a silent reset.
- **Every file write to `children.yaml` and `global_config.yaml` is atomic:** write `<path>.tmp`, then `fs::rename`.
- **`SF_DATALESS` is `0x40000000`** (from `sys/stat.h:359`).
- **Never call `create_dir_all` on a read path.** Folder creation happens at registration only.
- **Never `remove_dir_all` a child folder in response to a sync event.** Remote deletes deregister only.
- **Shell rules for this repo:** no `&&`, `;`, `||`, pipes, or `$(...)` in commands. One command per invocation.
- **Commit after every task.** Do not batch tasks into one commit.

---

## File Structure

**New files:**

| File | Responsibility |
|---|---|
| `shared/src/child_id.rs` | `ChildId` newtype — the immutable identity used as map key, sync partition key, and directory-resolution key. |
| `backend/storage/csv/child_registry.rs` | `RegistryEntry`, `ChildRegistry`, `children.yaml` load/save/mutate. Pure filesystem logic, no UI, no egui. |
| `backend/storage/csv/migration.rs` | One-shot legacy-layout → registry migration, plus the dry-run report. |
| `backend/storage/csv/checksum.rs` | Recursive tree checksum used by migration tests and by `Move data…` verification. |
| `backend/domain/child_availability.rs` | `ChildStatus`, `UnavailableReason`, `Availability`, the pure `classify` function, and the `ChildFolderSource` trait plus its real implementation. |
| `egui-frontend/src/ui/state/roster.rs` | `RosterEntry`, `ChildRoster`, and the worker thread that walks a registry snapshot and reports over `mpsc`. |
| `egui-frontend/src/ui/components/settings/children_modal.rs` | Settings → Children UI. Replaces `data_directory_modal.rs`. |

**Modified files:**

| File | Change |
|---|---|
| `backend/storage/csv/connection.rs` | Gains the registry and `child_dir`; loses redirect handling, `find_child_directory_by_id`, `relocate`/`revert`, `new_default`, and `create_dir_all` on the read path. |
| `backend/storage/csv/child_repository.rs` | Registry-backed `list_children`; id/dirname check and `set_active_child_directory` deleted. |
| `backend/storage/csv/transaction_repository.rs` | `get_child_directory_name` and the `unknown_child_*` fallback deleted. |
| `backend/storage/csv/goal_repository.rs`, `allowance_repository.rs`, `parental_control_repository.rs` | Resolve through `child_dir`. |
| `backend/storage/csv/test_utils.rs` | Fixtures inverted so `id ≠ safe_name` is the default. |
| `backend/mod.rs` | Runs migration inside `with_data_dir`. |
| `egui-frontend/src/ui/app_state.rs` | Registry banner policy; allowance issuance moved behind the roster. |
| `egui-frontend/src/ui/app_coordinator.rs` | `GetChildIdsRequest` filtered on `Available`; remote child delete deregisters only. |
| `egui-frontend/src/ui/components/header.rs`, `modals/child_selector.rs`, `settings/backfill_modal.rs` | Read the roster instead of calling `list_children`. |

**Deleted files:** `egui-frontend/src/ui/components/settings/data_directory_modal.rs`, `backend/domain/data_directory_service.rs`.

---

## Task 1: `ChildId` newtype

**Files:**
- Create: `shared/src/child_id.rs`
- Modify: `shared/src/lib.rs:1` (add `pub mod child_id;` and re-export)
- Test: inline `#[cfg(test)] mod tests` in `shared/src/child_id.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `shared::ChildId`, with `ChildId::new(impl Into<String>) -> ChildId`, `as_str(&self) -> &str`, `impl AsRef<str>`, `impl From<&str>`, `impl fmt::Display`, and derives `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`. Serializes as a bare string (transparent), so existing YAML/JSON stays byte-compatible.

- [ ] **Step 1: Write the failing test**

```rust
// shared/src/child_id.rs
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
```

- [ ] **Step 2: Add dev-dependencies needed by the test**

`shared/Cargo.toml` has no `serde_json` or `serde_yaml`. Add them as dev-dependencies only — this does not add runtime dependencies to the workspace.

```toml
[dev-dependencies]
serde_json = "1.0"
serde_yaml = "0.9"
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p shared child_id`
Expected: FAIL — `cannot find type ChildId in this scope`.

- [ ] **Step 4: Write the implementation**

```rust
// shared/src/child_id.rs
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
```

- [ ] **Step 5: Register the module**

In `shared/src/lib.rs`, directly below the existing `pub mod sync;` on line 1:

```rust
pub mod sync;
pub mod child_id;

pub use child_id::ChildId;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p shared child_id`
Expected: PASS, 3 tests.

- [ ] **Step 7: Commit**

```bash
git add shared/src/child_id.rs shared/src/lib.rs shared/Cargo.toml
git commit -m "feat(shared): add ChildId newtype"
```

---

## Task 2: `ChildRegistry` and `children.yaml`

**Files:**
- Create: `backend/storage/csv/child_registry.rs`
- Modify: `backend/storage/csv/mod.rs:23-40` (register module + re-export)
- Test: inline `#[cfg(test)] mod tests` in `child_registry.rs`

**Interfaces:**
- Consumes: `shared::ChildId` (Task 1).
- Produces:
  - `RegistryEntry { pub id: ChildId, pub path: PathBuf, pub label: String }`
  - `ChildRegistry::load(base_dir: &Path) -> Result<ChildRegistry>` — returns an empty registry when `children.yaml` is absent; `Err` when it exists but is malformed or carries an unknown `version`.
  - `ChildRegistry::save(&self, base_dir: &Path) -> Result<()>` — atomic.
  - `ChildRegistry::entries(&self) -> &[RegistryEntry]`
  - `ChildRegistry::path_for(&self, id: &ChildId) -> Option<&Path>`
  - `ChildRegistry::register(&mut self, entry: RegistryEntry) -> Result<()>` — rejects duplicate id **and** duplicate path.
  - `ChildRegistry::deregister(&mut self, id: &ChildId) -> Result<()>`
  - `ChildRegistry::repoint(&mut self, id: &ChildId, new_path: PathBuf) -> Result<()>`
  - `ChildRegistry::set_label(&mut self, id: &ChildId, label: &str) -> bool` — returns `true` if the label actually changed, so callers can persist once per walk rather than once per child.
  - `pub const REGISTRY_FILENAME: &str = "children.yaml";`

- [ ] **Step 1: Write the failing tests**

```rust
// backend/storage/csv/child_registry.rs
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p allowance-tracker-egui child_registry`
Expected: FAIL — `failed to resolve: use of undeclared crate or module child_registry`.

- [ ] **Step 3: Write the implementation**

```rust
// backend/storage/csv/child_registry.rs
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
```

- [ ] **Step 4: Register the module**

In `backend/storage/csv/mod.rs`, add to the `pub mod` block and the re-export block:

```rust
pub mod child_registry;
```

```rust
pub use child_registry::{ChildRegistry, RegistryEntry, REGISTRY_FILENAME};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p allowance-tracker-egui child_registry`
Expected: PASS, 9 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/storage/csv/child_registry.rs backend/storage/csv/mod.rs
git commit -m "feat(storage): add ChildRegistry backed by children.yaml"
```

---

## Task 3: CI job

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing consumed by later tasks. This exists because the panel found there is no CI at all, and every subsequent task's safety depends on the suite actually being run.

- [ ] **Step 1: Write the workflow**

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    runs-on: macos-latest   # SF_DATALESS and the darwin MetadataExt path are macOS-only
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo registry and target
        uses: Swatinem/rust-cache@v2

      - name: Check workspace
        run: cargo check --workspace

      - name: Run tests
        run: cargo test --workspace
```

- [ ] **Step 2: Verify the commands succeed locally first**

Run: `cargo check --workspace`
Expected: finishes with no errors.

- [ ] **Step 3: Run the full suite locally**

Run: `cargo test --workspace`
Expected: PASS. Record the test count in the commit message — it is the baseline the cutover is measured against.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: run cargo check and cargo test on push and PR"
```

---

## Task 4: Recursive tree checksum helper

**Files:**
- Create: `backend/storage/csv/checksum.rs`
- Modify: `backend/storage/csv/mod.rs` (register module + re-export)
- Test: inline `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub fn tree_checksum(root: &Path) -> Result<u64>` — a content-and-layout hash over every file under `root`, order-independent. Used by Task 6's migration tests to assert nothing moved, and by Task 13's `Move data…` to verify a copy before deleting the source.

- [ ] **Step 1: Write the failing tests**

```rust
// backend/storage/csv/checksum.rs
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p allowance-tracker-egui checksum`
Expected: FAIL — `cannot find function tree_checksum`.

- [ ] **Step 3: Write the implementation**

```rust
// backend/storage/csv/checksum.rs
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
```

- [ ] **Step 4: Register the module**

In `backend/storage/csv/mod.rs`:

```rust
pub mod checksum;
```

```rust
pub use checksum::tree_checksum;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p allowance-tracker-egui checksum`
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/storage/csv/checksum.rs backend/storage/csv/mod.rs
git commit -m "feat(storage): add recursive tree checksum helper"
```

---

## Task 5: Migration from the legacy layout

**Files:**
- Create: `backend/storage/csv/migration.rs`
- Modify: `backend/storage/csv/mod.rs` (register module + re-export)
- Test: inline `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `ChildRegistry`, `RegistryEntry`, `REGISTRY_FILENAME` (Task 2); `tree_checksum` (Task 4); `ChildId` (Task 1).
- Produces:
  - `pub struct MigrationReport { pub registered: Vec<ChildId>, pub orphans: Vec<PathBuf>, pub skipped: Vec<(PathBuf, String)> }`
  - `pub fn plan_migration(base_dir: &Path) -> Result<(ChildRegistry, MigrationReport)>` — pure: reads, decides, writes nothing.
  - `pub fn run_migration(base_dir: &Path) -> Result<Option<MigrationReport>>` — returns `Ok(None)` when `children.yaml` already exists (idempotent no-op); otherwise persists the registry, migrates `global_config.yaml`, and returns the report.
  - `pub fn write_dry_run(base_dir: &Path) -> Result<PathBuf>` — writes `children.yaml.proposed` and returns its path. Used in this phase so the output can be eyeballed against the real install before anything depends on it.

**Why `plan_migration` is separate from `run_migration`:** every interesting assertion is about the *decision*, not the writing. Splitting them means the fixtures test a pure function, and `run_migration` is a thin shell whose only extra responsibility is persistence.

- [ ] **Step 1: Write the failing tests**

```rust
// backend/storage/csv/migration.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Write a self-contained child folder at `rel` under `root`.
    fn child_folder(root: &Path, rel: &str, id: &str, name: &str) {
        let dir = root.join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("child.yaml"),
            format!(
                "id: {id}\nname: {name}\nbirthdate: '2010-01-01'\n\
                 created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n"
            ),
        )
        .unwrap();
    }

    /// Write a redirect stub at `base/<stub>` pointing to `target`.
    fn redirect_stub(base: &Path, stub: &str, target: &Path) {
        let dir = base.join(stub);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".allowance_redirect"), target.to_string_lossy().as_bytes()).unwrap();
    }

    #[test]
    fn registers_an_in_tree_child() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "keiko_hart", "keiko_hart", "Keiko Hart");

        let (reg, report) = plan_migration(base.path()).unwrap();
        assert_eq!(reg.entries().len(), 1);
        assert_eq!(reg.entries()[0].id, ChildId::from("keiko_hart"));
        assert_eq!(reg.entries()[0].path, base.path().join("keiko_hart"));
        assert_eq!(reg.entries()[0].label, "Keiko Hart");
        assert!(report.orphans.is_empty());
    }

    #[test]
    fn follows_a_redirect_stub_to_the_real_folder() {
        let base = TempDir::new().unwrap();
        let real = TempDir::new().unwrap();
        child_folder(real.path(), "keiko_hart", "keiko_hart", "Keiko Hart");
        redirect_stub(base.path(), "keiko_hart", &real.path().join("keiko_hart"));

        let (reg, _) = plan_migration(base.path()).unwrap();
        assert_eq!(reg.entries().len(), 1);
        assert_eq!(reg.entries()[0].path, real.path().join("keiko_hart"));
    }

    /// The single highest-value assertion in this task: the id is read from
    /// child.yaml, never inferred from the folder name. The fixture makes the
    /// two deliberately differ so an inference bug cannot pass.
    #[test]
    fn takes_the_id_from_the_yaml_not_the_folder_name() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "some_other_folder_name", "keiko_hart", "Keiko Hart");

        let (reg, _) = plan_migration(base.path()).unwrap();
        assert_eq!(reg.entries()[0].id, ChildId::from("keiko_hart"));
        assert_eq!(reg.entries()[0].path, base.path().join("some_other_folder_name"));
    }

    #[test]
    fn reports_orphan_folders_rather_than_skipping_them_silently() {
        let base = TempDir::new().unwrap();
        let orphan = base.path().join("keiko_smith");
        std::fs::create_dir_all(&orphan).unwrap();
        std::fs::write(orphan.join("transactions.csv"), "id,child_id,date,description,amount,balance\n").unwrap();

        let (reg, report) = plan_migration(base.path()).unwrap();
        assert!(reg.entries().is_empty());
        assert_eq!(report.orphans, vec![orphan]);
    }

    #[test]
    fn ignores_machine_local_files_and_known_non_child_dirs() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "keiko_hart", "keiko_hart", "Keiko Hart");
        std::fs::write(base.path().join("sync_state.yaml"), "enabled: false\n").unwrap();
        std::fs::write(base.path().join(".DS_Store"), "").unwrap();
        std::fs::create_dir_all(base.path().join("archive/old_thing")).unwrap();
        std::fs::create_dir_all(base.path().join("global")).unwrap();

        let (reg, report) = plan_migration(base.path()).unwrap();
        assert_eq!(reg.entries().len(), 1);
        assert!(report.orphans.is_empty(), "archive/ and global/ must not be reported as orphans");
    }

    #[test]
    fn redirect_to_a_missing_path_is_skipped_with_a_reason() {
        let base = TempDir::new().unwrap();
        redirect_stub(base.path(), "keiko_hart", Path::new("/nonexistent/keiko_hart"));

        let (reg, report) = plan_migration(base.path()).unwrap();
        assert!(reg.entries().is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].1.contains("does not exist"));
    }

    #[test]
    fn two_folders_claiming_one_id_registers_the_first_and_reports_the_second() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "aaa_first", "keiko_hart", "Keiko Hart");
        child_folder(base.path(), "zzz_second", "keiko_hart", "Keiko Hart");

        let (reg, report) = plan_migration(base.path()).unwrap();
        assert_eq!(reg.entries().len(), 1, "duplicate id must not be registered twice");
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn migration_moves_nothing() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "keiko_hart", "keiko_hart", "Keiko Hart");
        std::fs::write(base.path().join("global_config.yaml"),
                       "active_child_directory: keiko_hart\ndata_format_version: '1.0'\n").unwrap();

        let before = tree_checksum(base.path()).unwrap();
        let (_, _) = plan_migration(base.path()).unwrap();
        assert_eq!(before, tree_checksum(base.path()).unwrap(),
                   "plan_migration must not touch the disk");
    }

    #[test]
    fn run_migration_is_a_no_op_when_the_registry_already_exists() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "keiko_hart", "keiko_hart", "Keiko Hart");
        assert!(run_migration(base.path()).unwrap().is_some());

        let after_first = tree_checksum(base.path()).unwrap();
        assert!(run_migration(base.path()).unwrap().is_none(), "second run must be a no-op");
        assert_eq!(after_first, tree_checksum(base.path()).unwrap());
    }

    #[test]
    fn run_migration_converts_active_child_and_preserves_the_original() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "keiko_hart", "keiko_hart", "Keiko Hart");
        std::fs::write(base.path().join("global_config.yaml"),
                       "active_child_directory: keiko_hart\ndata_format_version: '1.0'\n").unwrap();

        run_migration(base.path()).unwrap();

        let migrated = std::fs::read_to_string(base.path().join("global_config.yaml")).unwrap();
        assert!(migrated.contains("active_child_id: keiko_hart"), "got: {migrated}");
        assert!(base.path().join("global_config.yaml.pre-registry").exists(),
                "the pre-migration file must be preserved for rollback");
    }

    /// Golden fixture replicating the real install's shape: a redirect stub
    /// carrying a .git, machine-local files, archive/ and global/ dirs.
    #[test]
    fn golden_fixture_matching_the_real_install() {
        let base = TempDir::new().unwrap();
        let icloud = TempDir::new().unwrap();

        child_folder(icloud.path(), "keiko_hart", "keiko_hart", "Keiko Hart");
        std::fs::write(icloud.path().join("keiko_hart/allowance_config.yaml"), "amount: 5.0\n").unwrap();
        std::fs::write(icloud.path().join("keiko_hart/transactions.csv"),
                       "id,child_id,date,description,amount,balance\n").unwrap();
        std::fs::write(icloud.path().join("keiko_hart/goals.csv"), "id,child_id,description\n").unwrap();

        redirect_stub(base.path(), "keiko_hart", &icloud.path().join("keiko_hart"));
        std::fs::create_dir_all(base.path().join("keiko_hart/.git")).unwrap();
        std::fs::write(base.path().join("global_config.yaml"),
                       "active_child_directory: keiko_hart\ndata_format_version: '1.0'\n").unwrap();
        std::fs::write(base.path().join("sync_state.yaml"), "enabled: true\n").unwrap();
        std::fs::write(base.path().join("sync_retry_queue.yaml"), "events: []\n").unwrap();
        std::fs::write(base.path().join("parental_control_attempts.csv"), "id,attempted_value\n").unwrap();
        std::fs::write(base.path().join(".DS_Store"), "").unwrap();
        std::fs::create_dir_all(base.path().join("archive/Keiko Hart_20250722_173144")).unwrap();
        std::fs::create_dir_all(base.path().join("global")).unwrap();

        let (reg, report) = plan_migration(base.path()).unwrap();

        assert_eq!(reg.entries().len(), 1);
        assert_eq!(reg.entries()[0].id, ChildId::from("keiko_hart"));
        assert_eq!(reg.entries()[0].path, icloud.path().join("keiko_hart"));
        assert_eq!(reg.entries()[0].label, "Keiko Hart");
        assert!(report.orphans.is_empty());
        assert!(report.skipped.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p allowance-tracker-egui migration`
Expected: FAIL — `cannot find function plan_migration`.

- [ ] **Step 3: Write the implementation**

```rust
// backend/storage/csv/migration.rs
//! # Legacy layout migration
//!
//! Converts the scan-plus-`.allowance_redirect` layout into a `children.yaml`
//! registry, exactly once, at startup.
//!
//! Two hard rules. It moves, copies, and deletes nothing — redirect stubs are
//! left inert on disk because each carries a `.git` history worth keeping. And
//! it reports what it could not understand rather than skipping in silence: a
//! folder holding transactions but no `child.yaml` is very likely an orphan
//! manufactured by the pre-registry rename bug, and it may hold real data.

use anyhow::{Context, Result};
use log::{info, warn};
use shared::ChildId;
use std::fs;
use std::path::{Path, PathBuf};

use super::child_registry::{ChildRegistry, RegistryEntry, REGISTRY_FILENAME};

/// Directory names in the base dir that are never children.
const NON_CHILD_DIRS: &[&str] = &["archive", "global"];

/// Files that suggest a folder held child data even though `child.yaml` is gone.
const ORPHAN_MARKERS: &[&str] = &["transactions.csv", "goals.csv", "allowance_config.yaml"];

#[derive(Debug, Default, PartialEq)]
pub struct MigrationReport {
    pub registered: Vec<ChildId>,
    /// Folders with child data but no `child.yaml`. Surfaced to the user.
    pub orphans: Vec<PathBuf>,
    /// Folders we declined to register, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
}

/// Minimal view of `child.yaml` — only what migration needs.
#[derive(serde::Deserialize)]
struct ChildYaml {
    id: String,
    name: String,
}

/// Decide what the registry should contain. Reads only; writes nothing.
pub fn plan_migration(base_dir: &Path) -> Result<(ChildRegistry, MigrationReport)> {
    let mut registry = ChildRegistry::default();
    let mut report = MigrationReport::default();

    if !base_dir.exists() {
        return Ok((registry, report));
    }

    // Sort for deterministic ordering: with two folders claiming one id, the
    // first by name wins and the second is reported.
    let mut dirs: Vec<PathBuf> = fs::read_dir(base_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    for dir in dirs {
        let name = match dir.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if NON_CHILD_DIRS.contains(&name) || name.starts_with('.') {
            continue;
        }

        let resolved = match resolve_legacy_dir(&dir) {
            Ok(p) => p,
            Err(e) => {
                report.skipped.push((dir.clone(), e.to_string()));
                continue;
            }
        };

        let yaml_path = resolved.join("child.yaml");
        if !yaml_path.exists() {
            if ORPHAN_MARKERS.iter().any(|f| resolved.join(f).exists()) {
                warn!("Migration found an orphan folder with child data: {}", resolved.display());
                report.orphans.push(resolved);
            }
            continue;
        }

        let text = match fs::read_to_string(&yaml_path) {
            Ok(t) => t,
            Err(e) => {
                report.skipped.push((resolved, format!("could not read child.yaml: {e}")));
                continue;
            }
        };
        let parsed: ChildYaml = match serde_yaml::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                report.skipped.push((resolved, format!("could not parse child.yaml: {e}")));
                continue;
            }
        };

        let entry = RegistryEntry {
            id: ChildId::new(parsed.id),
            path: resolved.clone(),
            label: parsed.name,
        };
        let id = entry.id.clone();
        match registry.register(entry) {
            Ok(()) => report.registered.push(id),
            Err(e) => report.skipped.push((resolved, e.to_string())),
        }
    }

    Ok((registry, report))
}

/// Follow `.allowance_redirect` if present, else return the directory itself.
fn resolve_legacy_dir(dir: &Path) -> Result<PathBuf> {
    let redirect = dir.join(".allowance_redirect");
    if !redirect.exists() {
        return Ok(dir.to_path_buf());
    }
    let target = fs::read_to_string(&redirect)
        .with_context(|| format!("reading {}", redirect.display()))?;
    let target = PathBuf::from(target.trim());
    if !target.exists() {
        anyhow::bail!("redirect target does not exist: {}", target.display());
    }
    Ok(target)
}

/// Run migration once. Returns `Ok(None)` when the registry already exists.
pub fn run_migration(base_dir: &Path) -> Result<Option<MigrationReport>> {
    if base_dir.join(REGISTRY_FILENAME).exists() {
        return Ok(None);
    }

    let (registry, report) = plan_migration(base_dir)?;
    registry.save(base_dir)?;
    migrate_global_config(base_dir, &registry)?;

    info!(
        "Migrated to child registry: {} registered, {} orphans, {} skipped",
        report.registered.len(),
        report.orphans.len(),
        report.skipped.len()
    );
    Ok(Some(report))
}

/// Write `children.yaml.proposed` so the result can be inspected before it is
/// authoritative. Inert is not the same as verifiable.
pub fn write_dry_run(base_dir: &Path) -> Result<PathBuf> {
    let (registry, _) = plan_migration(base_dir)?;
    let path = base_dir.join("children.yaml.proposed");

    // `save` owns the filename, so serialize through a scratch dir and move.
    let scratch = base_dir.join(".registry_dry_run");
    fs::create_dir_all(&scratch)?;
    registry.save(&scratch)?;
    fs::rename(scratch.join(REGISTRY_FILENAME), &path)?;
    fs::remove_dir_all(&scratch)?;
    Ok(path)
}

/// Convert `active_child_directory` to `active_child_id`, preserving the
/// original file so a pre-migration build can be restored by hand.
fn migrate_global_config(base_dir: &Path, registry: &ChildRegistry) -> Result<()> {
    let path = base_dir.join("global_config.yaml");
    if !path.exists() {
        return Ok(());
    }

    let text = fs::read_to_string(&path)?;
    let config: serde_yaml::Value = serde_yaml::from_str(&text)?;

    let legacy_dir = config
        .get("active_child_directory")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let Some(legacy_dir) = legacy_dir else {
        return Ok(());
    };

    // The legacy value is a directory name under the base dir. Find the entry
    // whose resolved path ends with it, or whose id matches it outright.
    let active_id = registry
        .entries()
        .iter()
        .find(|e| {
            e.id.as_str() == legacy_dir
                || e.path.file_name().and_then(|n| n.to_str()) == Some(legacy_dir.as_str())
        })
        .map(|e| e.id.clone());

    let Some(active_id) = active_id else {
        warn!("Could not resolve active_child_directory '{legacy_dir}' to a registered child");
        return Ok(());
    };

    fs::copy(&path, base_dir.join("global_config.yaml.pre-registry"))?;

    let mut out = serde_yaml::Mapping::new();
    out.insert(
        serde_yaml::Value::String("active_child_id".into()),
        serde_yaml::Value::String(active_id.as_str().to_string()),
    );
    out.insert(
        serde_yaml::Value::String("data_format_version".into()),
        serde_yaml::Value::String("1.0".into()),
    );

    let rendered = serde_yaml::to_string(&serde_yaml::Value::Mapping(out))?;
    let temp = path.with_extension("yaml.tmp");
    fs::write(&temp, rendered)?;
    fs::rename(&temp, &path)?;
    Ok(())
}
```

- [ ] **Step 4: Register the module**

In `backend/storage/csv/mod.rs`:

```rust
pub mod migration;
```

```rust
pub use migration::{plan_migration, run_migration, write_dry_run, MigrationReport};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p allowance-tracker-egui migration`
Expected: PASS, 11 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/storage/csv/migration.rs backend/storage/csv/mod.rs
git commit -m "feat(storage): migrate legacy redirect layout to children.yaml"
```

---

## Task 6: Wire migration into startup and verify against the real install

**Files:**
- Modify: `backend/mod.rs:57-66` (`with_data_dir`)
- Test: inline `#[cfg(test)] mod tests` in `backend/mod.rs`

**Interfaces:**
- Consumes: `run_migration`, `write_dry_run` (Task 5).
- Produces: `Backend::with_data_dir` runs migration before any repository is constructed. This is also the end-to-end seam the later tests use.

**This task is deliberately inert.** Migration writes `children.yaml`, but nothing reads it until Task 9. That is what makes it safe to run against the real install.

- [ ] **Step 1: Write the failing test**

```rust
// backend/mod.rs — inside #[cfg(test)] mod tests
#[test]
fn with_data_dir_migrates_a_legacy_layout_once() {
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap();
    let child = dir.path().join("keiko_hart");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(
        child.join("child.yaml"),
        "id: keiko_hart\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
         created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n",
    )
    .unwrap();

    let _backend = Backend::with_data_dir(dir.path().to_path_buf(), None).unwrap();

    let registry_path = dir.path().join("children.yaml");
    assert!(registry_path.exists(), "startup must produce children.yaml");
    let text = std::fs::read_to_string(&registry_path).unwrap();
    assert!(text.contains("keiko_hart"), "got: {text}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p allowance-tracker-egui with_data_dir_migrates`
Expected: FAIL — `children.yaml` does not exist.

- [ ] **Step 3: Call migration from `with_data_dir`**

In `backend/mod.rs`, inside `with_data_dir`, immediately before `let csv_connection = Arc::new(CsvConnection::new(data_path.clone())?);`:

```rust
// Convert the legacy scan-plus-redirect layout to a children.yaml registry.
// Runs at most once — a no-op when children.yaml already exists. Nothing
// reads the registry yet; this phase only produces it.
match crate::backend::storage::csv::run_migration(&data_path) {
    Ok(Some(report)) => {
        log::info!(
            "Child registry migration: {} registered, {} orphan(s), {} skipped",
            report.registered.len(),
            report.orphans.len(),
            report.skipped.len()
        );
        for orphan in &report.orphans {
            log::warn!(
                "Folder holds child data but no child.yaml — not registered: {}",
                orphan.display()
            );
        }
        for (path, reason) in &report.skipped {
            log::warn!("Skipped {} during migration: {}", path.display(), reason);
        }
    }
    Ok(None) => log::debug!("Child registry already present; migration skipped"),
    Err(e) => log::error!("Child registry migration failed: {e}"),
}
```

Migration failure is logged, not fatal: the legacy scan is still authoritative in this phase, so a failed migration must not prevent the app from starting.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p allowance-tracker-egui with_data_dir_migrates`
Expected: PASS.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test --workspace`
Expected: PASS, no regressions against the Task 3 baseline.

- [ ] **Step 6: Dry-run against the real install and eyeball the output**

This is the verification gate the spec calls for. Add a temporary binary target or run via a scratch test — simplest is a one-off test that is deleted afterwards:

```rust
#[test]
#[ignore] // run explicitly: cargo test -p allowance-tracker-egui dry_run_real_install -- --ignored --nocapture
fn dry_run_real_install() {
    let base = dirs::home_dir().unwrap().join("Documents").join("Allowance Tracker");
    let (registry, report) = crate::backend::storage::csv::plan_migration(&base).unwrap();
    println!("--- proposed registry ---");
    for e in registry.entries() {
        println!("  {} -> {} ({})", e.id, e.path.display(), e.label);
    }
    println!("orphans: {:?}", report.orphans);
    println!("skipped: {:?}", report.skipped);
}
```

Run: `cargo test -p allowance-tracker-egui dry_run_real_install -- --ignored --nocapture`

Expected output for the known install — confirm all three lines before continuing:
```
  keiko_hart -> /Users/<you>/Library/Mobile Documents/com~apple~CloudDocs/HartRoot/Parent Portal/Allowance Tracker/keiko_hart (Keiko Hart)
orphans: []
skipped: []
```

**STOP if `orphans` is non-empty.** That means the rename bug has fired on the real install and a folder holds transactions with no `child.yaml`. Resolve it by hand before proceeding — the registry will not adopt it.

- [ ] **Step 7: Commit**

```bash
git add backend/mod.rs
git commit -m "feat(backend): run child registry migration at startup (inert)"
```

---

## Task 7: Characterization tests at the resolution seam

**Files:**
- Modify: `backend/storage/csv/test_utils.rs:96-123` (invert fixture defaults)
- Create: `backend/storage/csv/resolution_tests.rs`
- Modify: `backend/storage/csv/mod.rs` (register test module)

**Interfaces:**
- Consumes: existing `TestHelper`.
- Produces: `TestHelper::create_test_child_with_distinct_id(name, id)`. The existing `create_test_child` and `create_test_child_with_name` change behaviour — they now mint an id that deliberately **differs** from the sanitized name.

**This is a hard gate.** Task 8 does not begin until these pass. The panel demonstrated the existing suite is green while the rename bug is live: nine of ten `transaction_repository` tests create no child and route through the `unknown_child_*` fallback, and the tenth passes *while exercising the defect*.

- [ ] **Step 1: Invert the fixture defaults**

Replace the two constructors in `test_utils.rs` so `id ≠ generate_safe_directory_name(name)` is the default. The old fixtures set `id = safe_name`, which is precisely the accident that hides mis-resolution.

```rust
    /// Create a test child whose id deliberately differs from its sanitized
    /// display name.
    ///
    /// This is the default on purpose. When id, folder name, and sanitized
    /// name are all the same string, a resolver that uses the wrong one still
    /// passes — which is how the rename bug survived in a green suite.
    pub fn create_test_child(&self) -> Result<DomainChild> {
        self.create_test_child_with_distinct_id("Test Child", "child_fixture_001")
    }

    /// Create a test child with a specific display name and a distinct id.
    pub fn create_test_child_with_name(&self, name: &str) -> Result<DomainChild> {
        let safe = CsvConnection::generate_safe_directory_name(name);
        self.create_test_child_with_distinct_id(name, &format!("id_{safe}"))
    }

    /// Create a test child with an explicitly chosen id.
    pub fn create_test_child_with_distinct_id(&self, name: &str, id: &str) -> Result<DomainChild> {
        debug_assert_ne!(
            id,
            CsvConnection::generate_safe_directory_name(name),
            "fixtures must keep id and sanitized name distinct"
        );
        let child = DomainChild {
            id: id.to_string(),
            name: name.to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.child_repo.store_child(&child)?;
        Ok(child)
    }
```

- [ ] **Step 2: Run the suite and expect failures**

Run: `cargo test -p allowance-tracker-egui`
Expected: FAIL. Tests that silently depended on `id == safe_name` now break. **This is the point of the task** — each failure is a place where resolution was never actually asserted. Record the failing list; do not "fix" them by restoring the old fixture.

- [ ] **Step 3: Write the characterization tests**

```rust
// backend/storage/csv/resolution_tests.rs
//! Characterization tests for child-directory resolution.
//!
//! These pin *where bytes land* for all five repositories. They exist because
//! the pre-registry suite was green while resolution was broken: transactions
//! resolved through the display name, goals through the id, and three other
//! repositories through a base-dir scan. Those three conventions agreed only
//! because id, folder name, and sanitized name were the same string.

#![cfg(test)]

use super::test_utils::TestHelper;
use crate::backend::storage::traits::{ChildStorage, TransactionStorage};
use std::collections::BTreeSet;

/// Every immediate subdirectory of the base dir, sorted.
fn subdirs(helper: &TestHelper) -> BTreeSet<String> {
    std::fs::read_dir(&helper.env.base_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

#[test]
fn transactions_land_in_the_folder_the_id_names() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    let tx = crate::backend::domain::models::transaction::Transaction {
        id: "transaction::income::1".to_string(),
        child_id: child.id.clone(),
        date: chrono::Utc::now().fixed_offset(),
        description: "Allowance".to_string(),
        amount: 5.0,
        balance: 5.0,
        transaction_type: crate::backend::domain::models::transaction::TransactionType::Income,
    };
    helper.transaction_repo.store_transaction(&tx).unwrap();

    let expected = helper.env.base_path.join(&child.id).join("transactions.csv");
    assert!(
        expected.exists(),
        "transactions must land under the id folder, not the sanitized name"
    );

    let stray = helper.env.base_path.join("keiko_hart");
    assert!(!stray.exists(), "no folder may be created from the display name");
}

#[test]
fn renaming_a_child_does_not_move_or_lose_their_transactions() {
    let helper = TestHelper::new().unwrap();
    let mut child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    let tx = crate::backend::domain::models::transaction::Transaction {
        id: "transaction::income::1".to_string(),
        child_id: child.id.clone(),
        date: chrono::Utc::now().fixed_offset(),
        description: "Allowance".to_string(),
        amount: 5.0,
        balance: 5.0,
        transaction_type: crate::backend::domain::models::transaction::TransactionType::Income,
    };
    helper.transaction_repo.store_transaction(&tx).unwrap();

    let before_count = helper.transaction_repo.list_transactions(&child.id, None, None).unwrap().len();
    let before_dirs = subdirs(&helper);
    assert_eq!(before_count, 1);

    child.name = "Keiko Smith".to_string();
    helper.child_repo.update_child(&child).unwrap();

    let after_count = helper.transaction_repo.list_transactions(&child.id, None, None).unwrap().len();
    assert_eq!(after_count, 1, "rename must not lose transactions");

    // The directory-set assertion is the load-bearing half. Without it, a
    // regression that resolves to a different-but-consistent wrong folder
    // still passes, because create_dir_all manufactures the folder silently.
    assert_eq!(before_dirs, subdirs(&helper), "rename must not create a directory");
}

#[test]
fn goals_land_in_the_folder_the_id_names() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};
    helper.goal_repo.store_goal(&DomainGoal {
        id: "goal::1".to_string(),
        child_id: child.id.clone(),
        description: "Bike".to_string(),
        target_amount: 100.0,
        state: DomainGoalState::Active,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    assert!(helper.env.base_path.join(&child.id).join("goals.csv").exists());
    assert!(!helper.env.base_path.join("keiko_hart").exists());
}

#[test]
fn child_yaml_lands_in_the_folder_the_id_names() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    assert!(helper.env.base_path.join(&child.id).join("child.yaml").exists());
    assert_eq!(
        subdirs(&helper),
        BTreeSet::from(["child_abc_123".to_string()]),
        "exactly one folder, named by the id"
    );
}

#[test]
fn reading_a_child_with_a_missing_folder_creates_nothing() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    std::fs::remove_dir_all(helper.env.base_path.join(&child.id)).unwrap();
    let before = subdirs(&helper);

    // Whatever this returns, it must not fabricate a directory.
    let _ = helper.transaction_repo.list_transactions(&child.id, None, None);

    assert_eq!(before, subdirs(&helper), "a read must never create a child folder");
}
```

**Model shapes are verified against the codebase:** `Transaction` carries a `transaction_type: TransactionType` (`backend/domain/models/transaction.rs:15-23`) and the goal type is `DomainGoal` with `String` timestamps and `DomainGoalState` (`backend/domain/models/goal.rs:32-39`). If `store_goal`'s signature differs, adjust the call but not the assertions.

- [ ] **Step 4: Register the test module**

In `backend/storage/csv/mod.rs`:

```rust
#[cfg(test)]
mod resolution_tests;
```

- [ ] **Step 5: Run the characterization tests and record which fail**

Run: `cargo test -p allowance-tracker-egui resolution_tests`

Expected on `main`: `renaming_a_child_does_not_move_or_lose_their_transactions` FAILS, `transactions_land_in_the_folder_the_id_names` FAILS, and `reading_a_child_with_a_missing_folder_creates_nothing` FAILS. These three are the pins for the defects Task 8 and Task 9 fix.

Mark the three known-failing tests `#[ignore = "fixed by the registry cutover in Task 9"]` so the suite stays green through the intermediate phases, and remove the attribute in Task 9.

- [ ] **Step 6: Fix the fixture-inversion fallout from Step 2**

For each test that broke in Step 2, make it assert the correct behaviour rather than restoring the old fixture. Most will need `child.id` where they previously used a hardcoded folder name.

- [ ] **Step 7: Run the whole suite**

Run: `cargo test --workspace`
Expected: PASS, with the three pins ignored.

- [ ] **Step 8: Commit**

```bash
git add backend/storage/csv/test_utils.rs backend/storage/csv/resolution_tests.rs backend/storage/csv/mod.rs
git commit -m "test(storage): pin child-directory resolution before the registry cutover"
```

---

## Task 8: `CsvConnection` holds the registry and resolves through `child_dir`

**Files:**
- Modify: `backend/storage/csv/connection.rs:11-14` (struct), `:16-30` (`new`), `:33-70` (delete `new_default`), `:72-101` (replace `get_child_directory`), `:113-135` (`ensure_transactions_file_exists`), `:658-698` (delete `find_child_directory_by_id`)
- Test: inline `#[cfg(test)] mod tests` in `connection.rs`

**Interfaces:**
- Consumes: `ChildRegistry`, `RegistryEntry` (Task 2); `ChildId` (Task 1).
- Produces:
  - `CsvConnection::child_dir(&self, id: &ChildId) -> Result<PathBuf>` — registry lookup **plus one `stat` of `child.yaml`**; `Err` when the entry is absent or the folder is not there.
  - `CsvConnection::child_dir_for_create(&self, id: &ChildId) -> Result<PathBuf>` — registry lookup without the existence check, for the one caller that legitimately writes `child.yaml` for the first time.
  - `CsvConnection::registry(&self) -> Arc<ChildRegistry>` — snapshot.
  - `CsvConnection::update_registry(&self, f: impl FnOnce(&mut ChildRegistry) -> Result<()>) -> Result<()>` — clone, mutate, persist, swap.
  - `CsvConnection::transactions_path(&self, id: &ChildId) -> Result<PathBuf>`
  - `CsvConnection::goals_path(&self, id: &ChildId) -> Result<PathBuf>`

- [ ] **Step 1: Write the failing tests**

```rust
// backend/storage/csv/connection.rs — inside #[cfg(test)] mod tests
use tempfile::TempDir;
use shared::ChildId;

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
    assert_eq!(conn.child_dir(&ChildId::from("child_abc")).unwrap(), dir.path().join("child_abc"));
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
    assert!(!dir.path().join("child_abc").exists(), "resolution must not create the folder");
}

#[test]
fn ensure_transactions_file_does_not_create_the_child_folder() {
    let dir = TempDir::new().unwrap();
    let conn = conn_with_child(dir.path(), "child_abc");
    std::fs::remove_dir_all(dir.path().join("child_abc")).unwrap();

    assert!(conn.ensure_transactions_file_exists(&ChildId::from("child_abc")).is_err());
    assert!(!dir.path().join("child_abc").exists());
}

#[test]
fn update_registry_persists_and_is_visible_to_a_fresh_snapshot() {
    let dir = TempDir::new().unwrap();
    let conn = conn_with_child(dir.path(), "child_abc");
    assert_eq!(conn.registry().entries().len(), 1);

    let reloaded = CsvConnection::new(dir.path()).unwrap();
    assert_eq!(reloaded.registry().entries().len(), 1, "mutation must have been persisted");
}

#[test]
fn a_snapshot_is_unaffected_by_a_later_mutation() {
    let dir = TempDir::new().unwrap();
    let conn = conn_with_child(dir.path(), "child_abc");
    let snapshot = conn.registry();

    conn.update_registry(|reg| reg.deregister(&ChildId::from("child_abc"))).unwrap();

    assert_eq!(snapshot.entries().len(), 1, "held snapshot must not change under the reader");
    assert_eq!(conn.registry().entries().len(), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p allowance-tracker-egui connection::tests`
Expected: FAIL — `no method named child_dir`.

- [ ] **Step 3: Replace the struct and constructor**

```rust
#[derive(Clone, Debug)]
pub struct CsvConnection {
    /// The machine-local base directory. Plain `PathBuf` now that relocate and
    /// revert are gone — those were the only mutators, and they are deleted.
    base_directory: PathBuf,
    /// Copy-on-write registry. `CsvConnection` is `Clone` and shared as `Arc`
    /// across seven services, so readers take a snapshot (`Arc` clone) and hold
    /// borrows against it; writers rebuild and swap. The lock is held for a
    /// pointer copy and never across I/O.
    registry: Arc<Mutex<Arc<ChildRegistry>>>,
}

impl CsvConnection {
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

    pub fn base_directory(&self) -> &Path {
        &self.base_directory
    }

    /// A stable snapshot of the registry. Cheap: one `Arc` clone.
    pub fn registry(&self) -> Arc<ChildRegistry> {
        self.registry.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Mutate the registry: clone, apply, persist, then swap the new value in.
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
}
```

Note `get_current_data_directory` returned an owned `PathBuf`; keep it as `self.base_directory.clone()` for existing callers.

- [ ] **Step 4: Replace `get_child_directory` with `child_dir`**

```rust
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
    /// write (see the lifecycle ordering in the spec), so the entry is present.
    pub fn child_dir_for_create(&self, id: &ChildId) -> Result<PathBuf> {
        self.registry()
            .path_for(id)
            .map(|p| p.to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("child '{}' is not registered on this machine", id))
    }

    pub fn transactions_path(&self, id: &ChildId) -> Result<PathBuf> {
        Ok(self.child_dir(id)?.join("transactions.csv"))
    }

    pub fn goals_path(&self, id: &ChildId) -> Result<PathBuf> {
        Ok(self.child_dir(id)?.join("goals.csv"))
    }
```

- [ ] **Step 5: Strip `create_dir_all` from `ensure_transactions_file_exists`**

```rust
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
```

- [ ] **Step 6: Delete the dead and superseded code**

Delete outright from `connection.rs`:
- `new_default` (lines ~33-70) — reads a base-level `.allowance_redirect` and is called from nowhere.
- `relocate_child_data_directory`, `revert_child_data_directory`, `commit_redirect_file`, `copy_directory_contents` and any other helper used only by those.
- `find_child_directory_by_id` (lines ~658-698).
- Every `.lock().unwrap_or_else(|e| e.into_inner())` on `base_directory`.

Keep `generate_safe_directory_name` — it still mints ids and folder names for newly created children.

- [ ] **Step 7: Run the connection tests**

Run: `cargo test -p allowance-tracker-egui connection::tests`
Expected: PASS, 6 tests. The rest of the crate will not compile yet — that is Task 9.

- [ ] **Step 8: Commit**

```bash
git add backend/storage/csv/connection.rs
git commit -m "feat(storage): resolve child folders through the registry with an existence check"
```

---

## Task 9: Cut the five repositories over

**Files:**
- Modify: `backend/storage/csv/child_repository.rs`, `transaction_repository.rs`, `goal_repository.rs`, `allowance_repository.rs`, `parental_control_repository.rs`
- Modify: `backend/storage/csv/resolution_tests.rs` (remove the three `#[ignore]` attributes from Task 7)

**Interfaces:**
- Consumes: `child_dir`, `child_dir_for_create`, `transactions_path`, `goals_path` (Task 8).
- Produces: all five repositories resolve through `child_dir`. `list_children` is registry-backed.

**This is the highest-risk task.** The guards are the Task 7 characterization tests and the `ChildId` newtype. Do not begin until Task 7 is green.

- [ ] **Step 1: Un-ignore the three pins**

Remove `#[ignore = "fixed by the registry cutover in Task 9"]` from the three tests in `resolution_tests.rs`.

Run: `cargo test -p allowance-tracker-egui resolution_tests`
Expected: FAIL (3 tests). These are the target.

- [ ] **Step 2: `ChildRepository` — registry-backed listing, id/dirname check deleted**

Replace `discover_children` with a registry walk, and delete the id-equals-directory-name check.

```rust
    /// List children from the registry.
    ///
    /// No directory scan and no `child.yaml` parse per call. An entry whose
    /// folder will not load is skipped here and surfaced by the roster instead
    /// — this method answers "who is registered", not "who is loadable".
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
        children.sort_by(|a, b| a.name.cmp(&b.name));
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
```

`store_child` resolves through `child_dir_for_create`, and registers first if the child is not yet known:

```rust
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
```

Delete `get_child_yaml_path`, `get_active_child_directory`, and `set_active_child_directory` — `GlobalConfigRepository` becomes the sole owner of `global_config.yaml`. `get_active_child` and `set_active_child` delegate to it.

- [ ] **Step 3: `TransactionRepository` — delete the name-derived resolver**

Delete `get_child_directory_name` (lines ~185-205) entirely, including the `unknown_child_*` fallback. Every `read_transactions(child_name)` / `write_transactions(child_name)` takes `&ChildId` and calls `self.connection.transactions_path(id)?`. The `*_by_id` wrappers collapse into the primary methods.

- [ ] **Step 4: The other three repositories**

- `GoalRepository`: `self.connection.goals_path(id)?` in place of `get_goals_file_path(child_id)`.
- `AllowanceRepository`: replace `find_child_directory_by_id` + `get_child_directory` with `self.connection.child_dir(id)?.join("allowance_config.yaml")`.
- `ParentalControlRepository`: per-child attempts use `self.connection.child_dir(id)?.join("parental_control_attempts.csv")`; the global file stays at `base_directory().join("parental_control_attempts.csv")`.

- [ ] **Step 5: Propagate fallibility**

`child_dir` returns `Result` where `get_child_directory` returned `PathBuf`, so previously-infallible helpers become fallible. Follow the compiler: add `?` and change return types to `Result<...>` up each chain. Expect this to reach `backend/domain/*_service.rs`.

- [ ] **Step 6: Run the characterization tests**

Run: `cargo test -p allowance-tracker-egui resolution_tests`
Expected: PASS, 5 tests — including the three that failed in Step 1.

- [ ] **Step 7: Run the whole suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add backend/storage/csv/ backend/domain/
git commit -m "refactor(storage): resolve all five repositories through the child registry"
```

---

## Task 10: Availability classification

**Files:**
- Create: `backend/domain/child_availability.rs`
- Modify: `backend/domain/mod.rs` (register module + re-export)
- Test: inline `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `ChildId` (Task 1).
- Produces:
  - `pub enum Availability { Materialized, Dataless, Missing }`
  - `pub enum ChildStatus { Available(Child), Downloading, Unavailable(UnavailableReason) }` — derives `Clone, PartialEq`
  - `pub enum UnavailableReason { PathMissing, NotAChildFolder, IdMismatch { found: String }, ReadFailed(String), ParseFailed(String) }` — derives `Clone, PartialEq`
  - `pub fn classify(id: &ChildId, meta: io::Result<Metadata>, yaml: io::Result<String>) -> ChildStatus`
  - `pub trait ChildFolderSource: Send + Sync { fn probe(&self, path: &Path) -> Availability; fn read(&self, path: &Path) -> io::Result<String>; }`
  - `pub struct RealFolderSource;` implementing it.

- [ ] **Step 1: Write the failing tests**

```rust
// backend/domain/child_availability.rs — inside #[cfg(test)] mod tests
use super::*;
use std::io::{Error, ErrorKind};

const VALID_YAML: &str = "id: keiko_hart\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
                          created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n";

fn missing() -> Error { Error::new(ErrorKind::NotFound, "no such file") }

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
```

`fake_meta()` needs a real `Metadata`; obtain one from a temp file:

```rust
fn fake_meta() -> std::fs::Metadata {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::metadata(f.path()).unwrap()
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p allowance-tracker-egui child_availability`
Expected: FAIL — `cannot find function classify`.

- [ ] **Step 3: Write the implementation**

```rust
// backend/domain/child_availability.rs
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
```

- [ ] **Step 4: Register the module**

In `backend/domain/mod.rs`:

```rust
pub mod child_availability;
```

```rust
pub use child_availability::{
    classify, Availability, ChildFolderSource, ChildStatus, RealFolderSource, UnavailableReason,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p allowance-tracker-egui child_availability`
Expected: PASS, 6 tests.

- [ ] **Step 6: Commit**

```bash
git add backend/domain/child_availability.rs backend/domain/mod.rs
git commit -m "feat(domain): add child folder availability classification"
```

---

## Task 11: Roster and the loader worker

**Files:**
- Create: `egui-frontend/src/ui/state/roster.rs`
- Modify: `egui-frontend/src/ui/state/mod.rs` (register module)
- Test: inline `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `ChildRegistry`, `RegistryEntry` (Task 2); `classify`, `ChildStatus`, `Availability`, `ChildFolderSource` (Task 10).
- Produces:
  - `pub struct RosterEntry { pub entry: RegistryEntry, pub status: ChildStatus }`
  - `pub struct ChildRoster` with `entries(&self) -> &[RosterEntry]`, `available_ids(&self) -> Vec<ChildId>`, `status_of(&self, id: &ChildId) -> Option<&ChildStatus>`, `apply(&mut self, msg: RosterMessage)`, `mark_stale(&mut self, id: &ChildId)`
  - `pub enum RosterMessage { Status { generation: u64, id: ChildId, status: ChildStatus }, Finished { generation: u64 } }`
  - `pub fn spawn_loader(registry: Arc<ChildRegistry>, source: Arc<dyn ChildFolderSource>, generation: u64, tx: Sender<RosterMessage>, wake: WakeUi)`

**Prefetch list — all five files, in this order:** `child.yaml`, `allowance_config.yaml`, `transactions.csv`, `goals.csv`, `parental_control_attempts.csv`. `.git` is deliberately excluded; paging an entire object store to make one write fast is the wrong trade.

- [ ] **Step 1: Write the failing tests**

```rust
// egui-frontend/src/ui/state/roster.rs — inside #[cfg(test)] mod tests
use super::*;
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

#[test]
fn a_dataless_folder_reports_downloading_before_available() {
    let barrier = Arc::new(Barrier::new(2));
    let source = Arc::new(BlockingSource {
        availability: Availability::Dataless,
        release: barrier.clone(),
    });
    let (tx, rx) = mpsc::channel();

    spawn_loader(registry_with_one(), source, 1, tx, Arc::new(|| {}));

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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p allowance-tracker-egui roster`
Expected: FAIL — `cannot find function spawn_loader`.

- [ ] **Step 3: Write the implementation**

```rust
// egui-frontend/src/ui/state/roster.rs
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
use std::path::{Path, PathBuf};
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
    pub fn apply(&mut self, msg: RosterMessage) {
        match msg {
            RosterMessage::Status { generation, id, status } => {
                if generation != self.generation {
                    return;
                }
                if let Some(e) = self.entries.iter_mut().find(|e| e.entry.id == id) {
                    if let ChildStatus::Available(ref child) = status {
                        e.entry.label = child.name.clone();
                    }
                    e.status = status;
                }
            }
            RosterMessage::Finished { .. } => {}
        }
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
```

- [ ] **Step 4: Register the module**

In `egui-frontend/src/ui/state/mod.rs`:

```rust
pub mod roster;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p allowance-tracker-egui roster`
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit**

```bash
git add egui-frontend/src/ui/state/roster.rs egui-frontend/src/ui/state/mod.rs
git commit -m "feat(ui): load children off the UI thread with an iCloud-aware roster"
```

---

## Task 12: Point the UI, sync, and allowances at the roster

**Files:**
- Modify: `egui-frontend/src/ui/app_state.rs:96-135` (registry banner, roster construction, allowance re-sequencing)
- Modify: `egui-frontend/src/ui/app_coordinator.rs:417` (`GetChildIdsRequest`), `:613-617` (remote child delete), `:642` (periodic allowance check)
- Modify: `egui-frontend/src/ui/components/header.rs:121`, `modals/child_selector.rs:38`, `settings/backfill_modal.rs:16,62`

**Interfaces:**
- Consumes: `ChildRoster`, `RosterMessage`, `spawn_loader` (Task 11).
- Produces: no render path touches the filesystem; sync polls only `Available` children.

- [ ] **Step 1: Write the failing test for the sync gate**

```rust
// egui-frontend/src/ui/state/roster.rs — add to the tests module
#[test]
fn sync_is_offered_only_available_children() {
    let mut reg = ChildRegistry::default();
    reg.register(RegistryEntry { id: ChildId::from("ready"), path: PathBuf::from("/a"), label: "A".into() }).unwrap();
    reg.register(RegistryEntry { id: ChildId::from("cold"), path: PathBuf::from("/b"), label: "B".into() }).unwrap();

    let mut roster = ChildRoster::new(Arc::new(reg), 1);
    roster.apply(RosterMessage::Status {
        generation: 1,
        id: ChildId::from("ready"),
        status: ChildStatus::Available(crate::backend::domain::models::child::Child {
            id: "ready".into(),
            name: "A".into(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }),
    });
    roster.apply(RosterMessage::Status {
        generation: 1,
        id: ChildId::from("cold"),
        status: ChildStatus::Unavailable(UnavailableReason::PathMissing),
    });

    assert_eq!(roster.available_ids(), vec![ChildId::from("ready")]);
}
```

- [ ] **Step 2: Run it and confirm it passes**

Run: `cargo test -p allowance-tracker-egui sync_is_offered_only_available`
Expected: PASS — `available_ids` already implements this. The test exists to pin the contract before the call site depends on it.

- [ ] **Step 3: Add the roster to app state and surface the registry banner**

In `app_state.rs`, replace the eager `check_and_issue_pending_allowances` block (currently at `:120-133`) and add roster construction after the backend is built:

```rust
// Registry load policy lives here, mirroring the sync_state.yaml precedent
// above: a malformed hand-editable file must be diagnosable, never silently
// reset to empty.
let registry = backend.csv_connection.registry();
if registry.entries().is_empty() {
    info!("No children registered — Settings → Children → Add existing child…");
}

let (roster_tx, roster_rx) = std::sync::mpsc::channel();
let roster = ChildRoster::new(registry.clone(), 1);
let ctx_for_roster = cc.egui_ctx.clone();
let roster_wake: crate::backend::domain::WakeUi =
    std::sync::Arc::new(move || ctx_for_roster.request_repaint());
spawn_loader(
    registry,
    std::sync::Arc::new(crate::backend::domain::RealFolderSource),
    1,
    roster_tx,
    roster_wake,
);
```

**Delete the eager allowance call entirely.** It currently runs at `app_state.rs:122` on the main thread before the first frame; on a cold iCloud folder that is a bouncing Dock icon with no window at all — strictly worse than the mid-frame freeze this design removes, because there is not even a label to read.

- [ ] **Step 4: Trigger allowance issuance from the roster instead**

Where `RosterMessage` is drained each frame:

```rust
while let Ok(msg) = self.roster_rx.try_recv() {
    let newly_available = matches!(
        msg,
        RosterMessage::Status { ref status, .. } if matches!(status, ChildStatus::Available(_))
    );
    let msg_id = match &msg {
        RosterMessage::Status { id, .. } => Some(id.clone()),
        RosterMessage::Finished { .. } => None,
    };
    self.roster.apply(msg);

    // Allowance issuance is a consumer of roster completion, not of app start.
    if newly_available {
        if let Some(id) = msg_id {
            if self.active_child_id().as_ref() == Some(&id) {
                match self.backend().transaction_service.check_and_issue_pending_allowances() {
                    Ok(n) if n > 0 => info!("Issued {n} pending allowances for {id}"),
                    Ok(_) => {}
                    Err(e) => warn!("Failed to check pending allowances: {e}"),
                }
            }
        }
    }
}
```

Gate the periodic check at `app_coordinator.rs:642` on the active child being `Available` too.

- [ ] **Step 5: Filter `GetChildIdsRequest`**

At `app_coordinator.rs:417`, replace the `list_children()` scan:

```rust
SyncMessage::GetChildIdsRequest { response_tx } => {
    // Only Available children are polled. Reporting a Downloading or missing
    // child would let the apply path write into a folder iCloud is still
    // pulling down — a conflict generator on exactly the first-run scenario
    // this design exists to fix.
    let ids: Vec<String> = self
        .roster
        .available_ids()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect();
    let _ = response_tx.send(ids);
}
```

- [ ] **Step 6: Make a remote child delete deregister only**

At `app_coordinator.rs:613-617`:

```rust
EntityType::Child => {
    // Deregister only. A sync event must never remove a folder it does not
    // own: on a second machine this path would remove_dir_all the shared
    // iCloud folder out from under the first.
    let id = ChildId::from(child_id);
    if let Err(e) = self
        .core
        .backend
        .csv_connection
        .update_registry(|reg| reg.deregister(&id))
    {
        log::error!("Failed to deregister child {child_id} after remote delete: {e}");
    }
    self.roster = ChildRoster::new(self.core.backend.csv_connection.registry(), self.next_generation());
}
```

- [ ] **Step 7: Repoint the four render call sites**

Replace `self.backend().child_service.list_children()` with roster reads:

- `header.rs:121` — build the dropdown from `self.roster.entries()`, rendering non-`Available` entries greyed out and unclickable with their status as a suffix ("Downloading from iCloud…", "Folder not found").
- `child_selector.rs:38` — same, and replace the `"Debug: Check if test_data directory exists"` placeholder with "No children registered yet. Settings → Children → Add existing child…".
- `backfill_modal.rs:16` and `:62` — count and push only `available_ids()`.

- [ ] **Step 8: Run the whole suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 9: Verify no render path calls `list_children`**

Run: `grep -rn "list_children()" egui-frontend/src`
Expected: no hits in `header.rs`, `child_selector.rs`, `backfill_modal.rs`, or `app_coordinator.rs`.

- [ ] **Step 10: Commit**

```bash
git add egui-frontend/src/ui/
git commit -m "feat(ui): read children from the roster; gate sync and allowances on availability"
```

---

## Task 13: Settings → Children, and delete the Data directory machinery

**Files:**
- Create: `egui-frontend/src/ui/components/settings/children_modal.rs`
- Delete: `egui-frontend/src/ui/components/settings/data_directory_modal.rs`, `backend/domain/data_directory_service.rs`
- Modify: `egui-frontend/src/ui/components/header.rs:250-292` (menu label), `app_state.rs` (`SettingsAction::DataDirectory` → `Children`), `backend/domain/mod.rs`, `backend/mod.rs`, `shared/src/lib.rs:400-460`
- Test: inline `#[cfg(test)] mod tests` in `children_modal.rs` for the non-UI helpers

**Interfaces:**
- Consumes: `ChildRegistry`, `update_registry` (Tasks 2, 8); `tree_checksum` (Task 4); `ChildRoster` (Task 11).
- Produces:
  - `pub fn validate_child_folder(path: &Path) -> Result<RegistryEntry>` — reads `child.yaml`, returns the entry to register.
  - `pub fn move_child_data(conn: &CsvConnection, id: &ChildId, target: &Path) -> Result<()>` — refuses a non-empty target; verifies by checksum before deleting the source.

- [ ] **Step 1: Write the failing tests for the helpers**

```rust
// egui-frontend/src/ui/components/settings/children_modal.rs — tests module
use tempfile::TempDir;

fn child_folder(root: &Path, name: &str, id: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("child.yaml"),
        format!("id: {id}\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
                 created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n"),
    )
    .unwrap();
    dir
}

#[test]
fn validate_accepts_a_real_child_folder_and_reads_the_id_from_yaml() {
    let root = TempDir::new().unwrap();
    let dir = child_folder(root.path(), "any_folder_name", "keiko_hart");

    let entry = validate_child_folder(&dir).unwrap();
    assert_eq!(entry.id, ChildId::from("keiko_hart"));
    assert_eq!(entry.label, "Keiko Hart");
    assert_eq!(entry.path, dir);
}

#[test]
fn validate_rejects_a_folder_without_child_yaml() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("empty");
    std::fs::create_dir_all(&dir).unwrap();

    let err = validate_child_folder(&dir).unwrap_err();
    assert!(err.to_string().contains("child.yaml"));
}

#[test]
fn move_refuses_a_non_empty_target_and_changes_nothing() {
    let base = TempDir::new().unwrap();
    let source = child_folder(base.path(), "kid", "kid");
    let target = base.path().join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("something.txt"), "occupied").unwrap();

    let conn = CsvConnection::new(base.path()).unwrap();
    conn.update_registry(|reg| {
        reg.register(RegistryEntry { id: ChildId::from("kid"), path: source.clone(), label: "Kid".into() })
    })
    .unwrap();

    let before = tree_checksum(base.path()).unwrap();
    assert!(move_child_data(&conn, &ChildId::from("kid"), &target).is_err());
    assert_eq!(before, tree_checksum(base.path()).unwrap(), "a refused move must change nothing");
}

#[test]
fn move_relocates_and_repoints_the_registry() {
    let base = TempDir::new().unwrap();
    let source = child_folder(base.path(), "kid", "kid");
    let target = base.path().join("moved");

    let conn = CsvConnection::new(base.path()).unwrap();
    conn.update_registry(|reg| {
        reg.register(RegistryEntry { id: ChildId::from("kid"), path: source.clone(), label: "Kid".into() })
    })
    .unwrap();

    move_child_data(&conn, &ChildId::from("kid"), &target).unwrap();

    assert!(target.join("child.yaml").exists());
    assert!(!source.exists(), "source must be removed after a verified copy");
    assert_eq!(conn.registry().path_for(&ChildId::from("kid")), Some(target.as_path()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p allowance-tracker-egui children_modal`
Expected: FAIL — `cannot find function validate_child_folder`.

- [ ] **Step 3: Write the helpers**

```rust
/// Validate a folder the user picked and produce the entry to register.
/// Identity comes from `child.yaml`; the folder's own name is irrelevant.
pub fn validate_child_folder(path: &Path) -> Result<RegistryEntry> {
    let yaml_path = path.join("child.yaml");
    if !yaml_path.exists() {
        anyhow::bail!("{} does not contain a child.yaml", path.display());
    }
    #[derive(serde::Deserialize)]
    struct Y { id: String, name: String }
    let parsed: Y = serde_yaml::from_str(&std::fs::read_to_string(&yaml_path)?)
        .with_context(|| format!("parsing {}", yaml_path.display()))?;

    Ok(RegistryEntry {
        id: ChildId::new(parsed.id),
        path: path.to_path_buf(),
        label: parsed.name,
    })
}

/// Move a child's folder, then repoint the registry.
///
/// Refuses a non-empty target — adopting a folder that already holds data is
/// Add-existing, which is non-destructive. Verifies the copy by checksum before
/// deleting the source, so a partial copy never costs data.
pub fn move_child_data(conn: &CsvConnection, id: &ChildId, target: &Path) -> Result<()> {
    let source = conn.child_dir(id)?;

    if target.exists() {
        let occupied = std::fs::read_dir(target)?.next().is_some();
        if occupied {
            anyhow::bail!(
                "{} is not empty — use Add existing child… to adopt a folder that already holds data",
                target.display()
            );
        }
    }

    copy_dir_recursive(&source, target)?;

    if tree_checksum(&source)? != tree_checksum(target)? {
        std::fs::remove_dir_all(target).ok();
        anyhow::bail!("copy verification failed; nothing was moved");
    }

    conn.update_registry(|reg| reg.repoint(id, target.to_path_buf()))?;
    std::fs::remove_dir_all(&source)?;
    Ok(())
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run the helper tests**

Run: `cargo test -p allowance-tracker-egui children_modal`
Expected: PASS, 4 tests.

- [ ] **Step 5: Build the modal UI**

Render one row per `self.roster.entries()`:
- Status chip: "Ready", "Downloading from iCloud…", or the reason ("Folder not found", "Not a child folder", "ID mismatch: found `x`").
- The path, in a muted, wrapping label.
- Row buttons for non-`Available` entries: **Retry** (re-run the loader for that entry), **Locate…** (`rfd` picker → `repoint`), **Remove from this machine**.
- Footer buttons: **Add existing child…**, **Create new child…**, **Move data…** (enabled only for the selected `Available` child).

**Remove from this machine** must show a confirmation naming the path being left behind, and must call only `deregister` — never a filesystem delete.

Follow `SettingsModalStyle` and the `Area`/`Order::Foreground` structure used by `create_child_modal.rs:26-100` so it matches the other modals.

- [ ] **Step 6: Rename the menu entry and its action**

In `header.rs:250-292`, change the `"Data directory"` item to `"Children"` with icon `"👶"`. In `modal_state.rs`, rename `SettingsAction::DataDirectory` to `SettingsAction::Children`; in `app_state.rs`, have it set `show_children_modal`.

- [ ] **Step 7: Delete the superseded machinery**

- `egui-frontend/src/ui/components/settings/data_directory_modal.rs` — whole file.
- `backend/domain/data_directory_service.rs` — whole file; drop it from `backend/domain/mod.rs` and from `Backend`'s fields and constructor.
- From `shared/src/lib.rs`: `ConflictResolution`, `RelocateWithConflictResolutionRequest`/`Response`, `CheckDataDirectoryConflictRequest`/`Response`, `RevertDataDirectoryRequest`/`Response`, `ReturnToDefaultLocationRequest`/`Response`, `RelocateDataDirectoryRequest`/`Response`, `GetDataDirectoryResponse`.
- The `data_directory_form` field and its state from `settings/state.rs`.

- [ ] **Step 8: Run the whole suite and check for dead code**

Run: `cargo test --workspace`
Expected: PASS.

Run: `cargo check --workspace`
Expected: no `unused` warnings for the deleted types.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(ui): replace Data directory with Settings -> Children"
```

---

## Task 14: Manual verification on the real machine

**Files:** none — this is the checklist the spec calls for, covering what cannot be exercised in CI.

Two facts about iCloud cannot be tested automatically: that `SF_DATALESS` is read off the correct metadata call, and that a `read()` actually triggers materialization. Both are verified once, by hand.

- [ ] **Step 1: Build and install**

Run: `scripts/install.sh`
Expected: builds and installs the `.app`.

- [ ] **Step 2: Confirm the migration produced the right registry**

Run: `cat ~/Documents/"Allowance Tracker"/children.yaml`

Expected: one entry, `id: keiko_hart`, `path` pointing into iCloud, `label: Keiko Hart`.

- [ ] **Step 3: Confirm the app starts and shows the child**

Launch the app. Expected: Keiko is listed and selectable, the balance matches what it was before migration, and the calendar renders transactions.

- [ ] **Step 4: Verify the dataless path**

In Finder, right-click the iCloud child folder and choose **Remove Download**. Confirm with `ls -lO` that files now show the `dataless` flag. Relaunch the app.

Expected observation: the child appears as **"Downloading from iCloud…"** and the window paints immediately — no beachball, no missing window. Within a few seconds it flips to Ready with correct data.

**If instead the child shows "Folder not found",** `is_dataless` is reading the wrong metadata call — the `probe` is treating a dataless file as missing. Fix before shipping.

- [ ] **Step 5: Verify the offline path**

Turn off networking, Remove Download again, relaunch.

Expected: the child shows an Unavailable reason with a Retry button, the app remains usable, and nothing is written into the child folder. Confirm with `ls` that no `transactions.csv` was fabricated.

- [ ] **Step 6: Verify Add-existing on a second machine**

On the second Mac: launch, confirm the picker is empty with the "Add existing child…" prompt, add the iCloud folder, and confirm the full history appears.

- [ ] **Step 7: Record the results**

```bash
git commit --allow-empty -m "test: manual iCloud verification passed on macOS 26"
```

---

## Self-Review

**Spec coverage.** Every design section maps to a task: `children.yaml` → 2; `ChildId` → 1; `ChildRegistry` sharing → 2, 8; `child_dir` with existence check → 8; lifecycle ordering → 9, 12; config ownership → 5, 9; availability model → 10; `classify` plus seam → 10; off-thread loading and prefetch → 11; `list_children` callers → 12; sync contract → 12; migration with dry-run and orphan reporting → 5, 6; UI → 13; deletions → 8, 9, 13; CI → 3; manual checklist → 14. Deferred work (sync propagation, shared `.git`, crate extraction) is correctly absent.

**Known gaps, stated rather than hidden.**

- **Task 9 is the one task whose full diff is not written out.** Five repositories, three prior conventions, and a fallibility change that propagates into the domain services — enumerating every call site here would be guesswork about compiler output. It is bounded instead by the Task 7 characterization tests, which must pass before and after, and by the `ChildId` newtype, which makes wrong-parameter swaps a compile error. Execute Task 9 by following the compiler, not by pattern-matching this plan.
- **Task 13's modal rendering is described rather than coded.** egui layout is iterative and the existing modals are the better reference; the two helpers with real logic (`validate_child_folder`, `move_child_data`) are fully specified and tested.
- **Task 7 Step 6 cannot enumerate the fixture-inversion fallout** without running it. The instruction is explicit that each break is fixed by asserting correct behaviour, never by restoring the old fixture.

**Type consistency.** `ChildId` is used uniformly from Task 1. `RegistryEntry { id, path, label }` is identical in Tasks 2, 5, 8, 11, 13. `ChildStatus` and `UnavailableReason` are defined once in Task 10 and consumed unchanged in 11, 12, 13. `tree_checksum` (Task 4) is used in 5 and 13. `WakeUi` is the existing alias from `sync_manager.rs:112`. `child_dir` returns `Result<PathBuf>` everywhere; `child_dir_for_create` is used only by `store_child`.

**Ordering hazard.** Tasks must run in order. Task 9 before Task 7 would remove the only guard on the riskiest change; Task 12 before Task 11 has no roster to read.
