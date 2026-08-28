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
#[cfg(test)]
use super::checksum::tree_checksum;

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
///
/// Writes the FULL four-key `global_config.yaml` shape
/// (`active_child_id` / `data_format_version` / `created_at` / `updated_at`),
/// not just the two keys touched by this migration. `GlobalConfig::load` (see
/// `global_config_repository.rs`) hard-errors on a missing field, and a later
/// task makes `GlobalConfigRepository` the sole owner of this file — a
/// two-key output would fail to parse on first launch after migration.
/// `data_format_version` and `created_at` are preserved from the existing
/// file when present; `updated_at` always reflects this migration.
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

    let data_format_version = config
        .get("data_format_version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "1.0".to_string());

    let created_at = config
        .get("created_at")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let updated_at = chrono::Utc::now().to_rfc3339();

    let mut out = serde_yaml::Mapping::new();
    out.insert(
        serde_yaml::Value::String("active_child_id".into()),
        serde_yaml::Value::String(active_id.as_str().to_string()),
    );
    out.insert(
        serde_yaml::Value::String("data_format_version".into()),
        serde_yaml::Value::String(data_format_version),
    );
    out.insert(
        serde_yaml::Value::String("created_at".into()),
        serde_yaml::Value::String(created_at),
    );
    out.insert(
        serde_yaml::Value::String("updated_at".into()),
        serde_yaml::Value::String(updated_at),
    );

    let rendered = serde_yaml::to_string(&serde_yaml::Value::Mapping(out))?;
    let temp = path.with_extension("yaml.tmp");
    fs::write(&temp, rendered)?;
    fs::rename(&temp, &path)?;
    Ok(())
}

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

    /// Mandatory override: the migrated global_config.yaml must carry all
    /// four fields GlobalConfig requires (active_child_id,
    /// data_format_version, created_at, updated_at), not just the two the
    /// migration itself changes. created_at is preserved verbatim from the
    /// pre-migration file; updated_at is refreshed to reflect the migration.
    #[test]
    fn run_migration_preserves_created_at_and_sets_updated_at() {
        let base = TempDir::new().unwrap();
        child_folder(base.path(), "keiko_hart", "keiko_hart", "Keiko Hart");
        std::fs::write(
            base.path().join("global_config.yaml"),
            "active_child_directory: keiko_hart\ndata_format_version: '1.0'\n\
             created_at: '2020-06-01T00:00:00Z'\nupdated_at: '2020-06-01T00:00:00Z'\n",
        )
        .unwrap();

        run_migration(base.path()).unwrap();

        let migrated = std::fs::read_to_string(base.path().join("global_config.yaml")).unwrap();
        assert!(
            migrated.contains("created_at: '2020-06-01T00:00:00Z'") || migrated.contains("created_at: 2020-06-01T00:00:00Z"),
            "created_at sentinel must be preserved verbatim: {migrated}"
        );
        assert!(migrated.contains("updated_at:"), "updated_at must be present: {migrated}");
        assert!(migrated.contains("active_child_id: keiko_hart"), "got: {migrated}");

        // Parse to confirm the file matches the shape GlobalConfig requires,
        // and that updated_at genuinely changed rather than being copied
        // through unchanged.
        let value: serde_yaml::Value = serde_yaml::from_str(&migrated).unwrap();
        assert_eq!(value.get("created_at").and_then(|v| v.as_str()), Some("2020-06-01T00:00:00Z"));
        assert_ne!(value.get("updated_at").and_then(|v| v.as_str()), Some("2020-06-01T00:00:00Z"));
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
