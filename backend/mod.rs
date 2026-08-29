//! # Backend Module for egui Frontend
//!
//! This backend module provides direct access to domain services and storage
//! for the egui frontend. Unlike the Tauri version, this backend:
//! - Uses synchronous operations (no async/await)
//! - Provides direct access to domain services
//! - Excludes the IO/REST layer entirely
//! - Is optimized for desktop-only operation

use anyhow::Result;
use std::sync::Arc;
use crate::backend::domain::SyncNotifier;

// Domain modules
pub mod domain;
pub mod storage;

// Re-export commonly used types
pub use storage::csv::CsvConnection;

/// Main backend struct that orchestrates all services
pub struct Backend {
    pub child_service: domain::child_service::ChildService,
    pub transaction_service: Arc<domain::TransactionService>,
    pub calendar_service: domain::CalendarService,
    pub allowance_service: domain::AllowanceService,
    pub goal_service: domain::GoalService,
    pub parental_control_service: domain::ParentalControlService,
    pub balance_service: domain::BalanceService,
    pub export_service: domain::ExportService,
    /// The shared CSV connection, which owns the child registry.
    ///
    /// Exposed so UI-driven registry mutations (registering an existing
    /// child's folder, repointing one, deregistering) can reach
    /// `update_registry` without going through a domain service. Every
    /// service above holds a clone of this same `Arc`, so a mutation here is
    /// visible to all of them.
    pub csv_connection: Arc<CsvConnection>,
    /// Base data directory (e.g. ~/Documents/Allowance Tracker)
    pub data_dir: std::path::PathBuf,
}

impl Backend {
    /// Resolve the default data directory (`~/Documents/Allowance Tracker`).
    /// Exposed so startup code can load sync persistence before constructing
    /// Backend (which lets us decide whether to pass `Some(SyncNotifier)` at all).
    pub fn default_data_dir() -> Result<std::path::PathBuf> {
        let home_dir = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        Ok(home_dir.join("Documents").join("Allowance Tracker"))
    }

    /// Create a new backend instance with all services
    pub fn new(sync_notifier: Option<SyncNotifier>) -> Result<Self> {
        // Use the real data directory in ~/Documents/Allowance Tracker
        let data_path = Self::default_data_dir()?;
        Self::with_data_dir(data_path, sync_notifier)
    }

    /// Create a backend rooted at a specific data directory.
    ///
    /// [`Backend::new`] delegates here with the default
    /// `~/Documents/Allowance Tracker` path. Tests use it to run against a
    /// temporary directory instead of touching real user data.
    pub fn with_data_dir(
        data_path: std::path::PathBuf,
        sync_notifier: Option<SyncNotifier>,
    ) -> Result<Self> {
        // Load email config path before moving data_path
        let email_config_path = data_path.join("email_config.toml");

        // Convert the legacy scan-plus-redirect layout to a children.yaml
        // registry. Runs at most once — a no-op when children.yaml already
        // exists. Nothing reads the registry yet; this phase only produces
        // it. Failure is logged, not fatal: the legacy directory scan is
        // still authoritative in this phase, so a failed migration must
        // never prevent the app from starting.
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
            Err(e) => log::error!(
                "Child registry migration failed and wrote nothing to children.yaml: {e} — \
                 this may mean candidate folders under {:?} could not be recognized as \
                 children (inspect them before assuming data loss), or it may be an unrelated \
                 disk or permissions failure; startup continues regardless since the legacy \
                 directory scan is still authoritative in this phase",
                data_path
            ),
        }

        // Create the CSV connection with the real data directory
        log::info!("Backend::new() using real data path: {:?}", data_path);
        let csv_connection = Arc::new(CsvConnection::new(data_path.clone())?);

        // Create services using the Arc<CsvConnection> pattern
        let child_service = domain::child_service::ChildService::new(csv_connection.clone(), sync_notifier.clone());
        let allowance_service = domain::AllowanceService::new(csv_connection.clone());
        let balance_service = domain::BalanceService::new(csv_connection.clone())
            .with_sync_notifier(sync_notifier.clone());

        // Load email config and create TransactionService with email support
        let email_config = domain::EmailConfigService::load_config_or_default(&email_config_path);
        log::info!("Email config loaded: SMTP server = {}", email_config.smtp_server);

        let transaction_service = Arc::new(domain::TransactionService::with_email_service(
            csv_connection.clone(),
            child_service.clone(),
            allowance_service.clone(),
            balance_service.clone(),
            email_config,
            sync_notifier.clone(),
        )?);

        let calendar_service = domain::CalendarService::new();

        let goal_service = domain::GoalService::new(
            csv_connection.clone(),
            child_service.clone(),
            allowance_service.clone(),
            transaction_service.clone(), // Pass Arc
            balance_service.clone(),
            sync_notifier.clone(),
        );
        
        let parental_control_service = domain::ParentalControlService::new(csv_connection.clone());
        
        let export_service = domain::ExportService::new();
        
        Ok(Backend {
            child_service,
            transaction_service,
            calendar_service,
            allowance_service,
            goal_service,
            parental_control_service,
            balance_service,
            export_service,
            csv_connection,
            data_dir: data_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The central constraint of this task: a failed migration must never
    /// block startup. The legacy directory scan is still authoritative in
    /// this phase, so `Backend::with_data_dir` must come up regardless of
    /// what `run_migration` does. Build a folder that `run_migration`
    /// cannot register (no `child.yaml`, no recognizable child data), which
    /// makes it return `Err` and write nothing, then assert startup still
    /// succeeds and `children.yaml` was never created.
    #[test]
    fn with_data_dir_starts_up_even_when_migration_fails() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let mystery = dir.path().join("mystery_folder");
        std::fs::create_dir_all(&mystery).unwrap();
        std::fs::write(mystery.join("notes.txt"), "hi").unwrap();

        let backend = Backend::with_data_dir(dir.path().to_path_buf(), None);
        assert!(backend.is_ok(), "startup must succeed even when migration fails: {:?}", backend.err());

        let registry_path = dir.path().join("children.yaml");
        assert!(!registry_path.exists(), "a failed migration must not write children.yaml");
    }

    /// Read-only verification gate against the REAL data directory. Never run
    /// automatically — `plan_migration` writes nothing, so this is safe, but
    /// it is still excluded from the default test run because it depends on
    /// the state of this machine's real install.
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
}