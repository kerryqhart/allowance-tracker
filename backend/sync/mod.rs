//! # Sync safety module
//!
//! Holds the guard that stands between the user and the original bug this
//! whole project fixes: a live `.git` directory being replicated by iCloud (or
//! another cloud-drive sync) as loose files. lgs's own documentation says
//! iCloud "writes conflict copies *inside* `.git`, which can corrupt the
//! index, packs, or refs." [`paths::is_cloud_synced`] is the check that keeps
//! a git working repo out of a cloud-synced folder in the first place.

pub mod paths;

pub use paths::{is_cloud_synced, Reason, SyncPaths};
