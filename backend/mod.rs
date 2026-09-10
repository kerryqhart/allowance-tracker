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

/// How loudly a [`StartupNotice`] should be painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeSeverity {
    /// Something the user should look at, but the app works.
    Warning,
    /// Something is broken and children are missing because of it.
    Error,
}

/// Something the user must be told about what happened during startup.
///
/// Migration orphans, migration skips, and a `children.yaml` that would not
/// parse were all log-only. In a GUI app whose stdout nobody reads, that is
/// indistinguishable from silence — and the failure mode it hides is an empty
/// child picker, which is the exact bug this branch exists to remove. These
/// are collected while the backend is built and drained by the UI into the
/// startup banner.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupNotice {
    pub severity: NoticeSeverity,
    /// One line, the headline. Rendered bold.
    pub title: String,
    /// Supporting lines — paths, reasons, what to do next.
    pub details: Vec<String>,
}

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
    /// What startup found that the user needs to know. Drained by the UI into
    /// the startup banner; see [`StartupNotice`].
    pub startup_notices: Vec<StartupNotice>,
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
        // exists.
        //
        // The registry is now the ONLY source of children: the legacy
        // directory scan was deleted with the resolver cutover, so nothing
        // backstops a migration that writes nothing. Failure therefore stays
        // non-fatal (an app that will not launch is strictly worse than one
        // with an empty roster and a message) but it must never be silent —
        // every outcome below that the user could care about becomes a
        // StartupNotice and is painted in the startup banner.
        let mut startup_notices: Vec<StartupNotice> = Vec::new();
        match crate::backend::storage::csv::run_migration(&data_path) {
            Ok(Some(report)) => {
                log::info!(
                    "Child registry migration: {} registered, {} orphan(s), {} skipped",
                    report.registered.len(),
                    report.orphans.len(),
                    report.skipped.len()
                );
                let mut details = Vec::new();
                for orphan in &report.orphans {
                    log::warn!(
                        "Folder holds child data but no child.yaml — not registered: {}",
                        orphan.display()
                    );
                    // Named in full: an orphan is a folder that may hold real
                    // transactions, and the path is the only way to find it.
                    details.push(format!(
                        "{} holds child data but no child.yaml, so it was not registered. \
                         Nothing was deleted.",
                        orphan.display()
                    ));
                }
                for skip in &report.skipped {
                    log::warn!(
                        "Skipped {} during migration: {}",
                        skip.path.display(),
                        skip.reason
                    );
                    details.push(format!("{} — {}", skip.path.display(), skip.reason));
                }
                if report.needs_attention() {
                    details.push(
                        "Add any missing child with Settings → Children → Add existing child…"
                            .to_string(),
                    );
                    startup_notices.push(StartupNotice {
                        severity: NoticeSeverity::Warning,
                        title: "Some folders were not added to the child registry".to_string(),
                        details,
                    });
                }
            }
            Ok(None) => log::debug!("Child registry already present; migration skipped"),
            Err(e) => {
                log::error!(
                    "Child registry migration failed and wrote nothing to children.yaml: {e} — \
                     candidate folders under {:?} could not be recognized as children; inspect \
                     them before assuming data loss. Startup continues with no children \
                     registered.",
                    data_path
                );
                startup_notices.push(StartupNotice {
                    severity: NoticeSeverity::Error,
                    title: "The child registry could not be created".to_string(),
                    details: vec![
                        format!("{e}"),
                        format!("Nothing was written to {}.", data_path.join("children.yaml").display()),
                        "No data was changed. Add a child with Settings → Children → Add \
                         existing child…"
                            .to_string(),
                    ],
                });
            }
        }

        // Create the CSV connection with the real data directory
        log::info!("Backend::new() using real data path: {:?}", data_path);
        let csv_connection = Arc::new(CsvConnection::new(data_path.clone())?);

        // A children.yaml that would not parse no longer aborts the launch —
        // it starts an empty roster instead — so this is the only thing that
        // tells the user why their children are gone.
        if let Some(reason) = csv_connection.registry_load_error() {
            startup_notices.push(StartupNotice {
                severity: NoticeSeverity::Error,
                title: format!("{} could not be read", csv_connection.registry_path().display()),
                details: vec![
                    reason.to_string(),
                    "No children are registered until this is fixed. The file was left exactly \
                     as it is — nothing was reset."
                        .to_string(),
                    "Fix the file (or move it aside to start over) and restart the app."
                        .to_string(),
                ],
            });
        }

        // Every registered child's transactions.csv must parse under the one
        // canonical codec (allowance_core::codec): no current-time fallback on
        // an unparseable date, no chrono::Local resolution of a date-only
        // value, and no deriving an unrecognised transaction type from the
        // description/amount. Before this cutover those cases were silently
        // rewritten; now they are hard errors. Checked once here, up front,
        // so a malformed row surfaces in the startup banner instead of only
        // failing the moment someone opens that child's page.
        {
            let transaction_repository =
                storage::csv::TransactionRepository::new((*csv_connection).clone());
            for (child_id, reason) in transaction_repository.validate_all_transaction_files() {
                startup_notices.push(StartupNotice {
                    severity: NoticeSeverity::Error,
                    title: format!("{child_id}'s transactions could not be read"),
                    details: vec![
                        reason,
                        "Nothing was changed. Fix the row named above (or restore a backup) \
                         and restart the app."
                            .to_string(),
                    ],
                });
            }
        }

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
            startup_notices,
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

    /// A failed migration must never block startup — an app that will not
    /// launch is strictly worse than one that comes up with an empty roster
    /// and says why. Nothing backstops it any more (the legacy directory scan
    /// was deleted with the resolver cutover), so the compensating requirement
    /// is that the failure is *visible*: it must produce a StartupNotice for
    /// the banner rather than only a log line.
    ///
    /// Build a folder `run_migration` cannot register (no `child.yaml`, no
    /// recognizable child data), which makes it return `Err` and write
    /// nothing.
    #[test]
    fn with_data_dir_starts_up_even_when_migration_fails() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let mystery = dir.path().join("mystery_folder");
        std::fs::create_dir_all(&mystery).unwrap();
        std::fs::write(mystery.join("notes.txt"), "hi").unwrap();

        let backend = Backend::with_data_dir(dir.path().to_path_buf(), None);
        assert!(backend.is_ok(), "startup must succeed even when migration fails: {:?}", backend.err());
        let backend = backend.unwrap();

        let registry_path = dir.path().join("children.yaml");
        assert!(!registry_path.exists(), "a failed migration must not write children.yaml");

        assert_eq!(backend.startup_notices.len(), 1, "the user must be told");
        assert_eq!(backend.startup_notices[0].severity, NoticeSeverity::Error);
    }

    /// The Critical case, end to end through the real startup path: the only
    /// candidate is a redirect stub whose iCloud target has not arrived. The
    /// app must come up, `children.yaml` must exist so a later run is a no-op,
    /// and the skip must reach the banner instead of dying in the log.
    #[test]
    fn with_data_dir_persists_and_reports_when_a_redirect_target_is_not_here_yet() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let stub = dir.path().join("keiko_hart");
        std::fs::create_dir_all(&stub).unwrap();
        std::fs::write(
            stub.join(".allowance_redirect"),
            "/nowhere/yet/Allowance Tracker/keiko_hart",
        )
        .unwrap();

        let backend = Backend::with_data_dir(dir.path().to_path_buf(), None).unwrap();

        assert!(
            dir.path().join("children.yaml").exists(),
            "a not-yet-present redirect target must not stop the registry being written"
        );
        assert_eq!(backend.startup_notices.len(), 1, "the skip must be surfaced");
        assert_eq!(backend.startup_notices[0].severity, NoticeSeverity::Warning);
        assert!(
            backend.startup_notices[0]
                .details
                .iter()
                .any(|d| d.contains("keiko_hart")),
            "the notice must name the folder: {:?}",
            backend.startup_notices[0].details
        );
    }

    /// A malformed `children.yaml` must launch (see the connection.rs pins for
    /// the roster half) *and* say so.
    #[test]
    fn with_data_dir_reports_an_unreadable_registry_instead_of_failing_to_launch() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("children.yaml"), "children: [ not: valid").unwrap();

        let backend = Backend::with_data_dir(dir.path().to_path_buf(), None)
            .expect("a malformed children.yaml must not stop the app launching");

        assert!(backend.csv_connection.registry().entries().is_empty());
        assert_eq!(backend.startup_notices.len(), 1);
        assert_eq!(backend.startup_notices[0].severity, NoticeSeverity::Error);
        assert!(
            backend.startup_notices[0].title.contains("children.yaml"),
            "the banner must name the file: {}",
            backend.startup_notices[0].title
        );
    }

    /// A malformed row in a registered child's `transactions.csv` (here, a
    /// date the codec refuses) must launch — an app that will not start over
    /// one bad row is worse than a startup notice — and must say so, rather
    /// than surfacing only the moment the child's page is opened.
    #[test]
    fn with_data_dir_reports_an_unreadable_transactions_file_instead_of_failing_to_launch() {
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
        std::fs::write(
            child.join("transactions.csv"),
            "id,child_id,date,description,amount,balance,type\n\
             x,keiko_hart,not-a-date,d,1.00,1.00,expense\n",
        )
        .unwrap();

        let backend = Backend::with_data_dir(dir.path().to_path_buf(), None)
            .expect("a malformed transactions.csv must not stop the app launching");

        assert!(
            !backend.csv_connection.registry().entries().is_empty(),
            "the migration should still register the child"
        );
        let notice = backend
            .startup_notices
            .iter()
            .find(|n| n.title.contains("keiko_hart"))
            .expect("the unreadable transactions file must be reported");
        assert_eq!(notice.severity, NoticeSeverity::Error);
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