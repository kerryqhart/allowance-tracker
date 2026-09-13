//! # Sync safety module
//!
//! Holds the guard that stands between the user and the original bug this
//! whole project fixes: a live `.git` directory being replicated by iCloud (or
//! another cloud-drive sync) as loose files. lgs's own documentation says
//! iCloud "writes conflict copies *inside* `.git`, which can corrupt the
//! index, packs, or refs." [`paths::is_cloud_synced`] is the check that keeps
//! a git working repo out of a cloud-synced folder in the first place.

pub mod bootstrap;
pub mod child_sync;
pub mod lgs_client;
pub mod migration_lgs;
pub mod paths;

pub use bootstrap::{copy_binary, ensure_lgs_binary, git_is_available, GIT_MISSING_MESSAGE};
pub use child_sync::{classify, ChildSyncEngine, Cycle, CycleOutcome};
pub use lgs_client::{DaemonInfo, DaemonState, DurabilityState, LgsClient, ProjectReport, StatusReport};
pub use migration_lgs::{plan_lgs_migration, run_lgs_migration, LgsMigrationPlan, LgsMigrationReport};
pub use paths::{is_cloud_synced, Reason, SyncPaths};
