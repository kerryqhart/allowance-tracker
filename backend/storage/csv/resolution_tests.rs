//! Characterization tests for child-directory resolution.
//!
//! These pin *where bytes land* for every repository that resolves a
//! **per-child** path. They exist because the pre-registry suite was green
//! while resolution was broken: transactions resolved through the sanitized
//! display name, goals through the id, and the rest through a base-dir scan
//! of every `child.yaml`. Those three conventions agreed only because id,
//! folder name, and sanitized name happened to be the same string.
//!
//! All five now resolve through the one registry lookup,
//! `CsvConnection::child_dir`. These pins were written against the three
//! conventions they replaced, and they stay because they are what proves the
//! cutover landed — the *superseded* convention is named below so a failure
//! says which old behaviour crept back.
//!
//! Coverage — one pin per repository:
//!
//! | Repository                   | Convention it used to use   | Pinned by |
//! |------------------------------|-----------------------------|-----------|
//! | `TransactionRepository`      | sanitized display name      | `transactions_land_in_the_folder_the_id_names`, `renaming_a_child_does_not_move_or_lose_their_transactions`, `reading_a_child_with_a_missing_folder_creates_nothing` |
//! | `GoalRepository`             | `child_id` passed straight  | `goals_land_in_the_folder_the_id_names` |
//! | `ChildRepository`            | `child.id` as dir name      | `child_yaml_lands_in_the_folder_the_id_names` |
//! | `AllowanceRepository`        | base-dir scan               | `allowance_config_lands_in_the_folder_the_id_names` |
//! | `ParentalControlRepository`  | base-dir scan               | `parental_control_attempts_land_in_the_folder_the_id_names` |
//!
//! `GlobalConfigRepository` is **deliberately excluded**: it resolves a single
//! base-directory-level file and never derives a per-child path, so the child
//! registry cutover cannot change where its bytes land. Its file format is
//! covered separately by a dedicated table-driven test in a later task.
//!
//! Every test asserts the **full** subdirectory set of the base directory, not
//! just the expected path plus one named stray. A resolver that writes into a
//! third, unanticipated folder must fail these too — under the old layout
//! `create_dir_all` on the write *and read* paths meant a wrong folder was
//! silently manufactured rather than erroring, and the set assertion is what
//! catches a reintroduction.

#![cfg(test)]

use super::test_utils::TestHelper;
use crate::backend::storage::traits::{
    AllowanceStorage, ChildStorage, ParentalControlStorage, TransactionStorage,
};
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
        amount: allowance_core::money::Money::from_cents(500),
        balance: allowance_core::money::Money::from_cents(500),
        transaction_type: crate::backend::domain::models::transaction::TransactionType::OneOffIncome,
    };
    helper.transaction_repo.store_transaction(&tx).unwrap();

    let expected = helper.env.base_path.join(&child.id).join("transactions.csv");
    assert!(
        expected.exists(),
        "transactions must land under the id folder, not the sanitized name"
    );

    let stray = helper.env.base_path.join("keiko_hart");
    assert!(!stray.exists(), "no folder may be created from the display name");

    // Stronger than the named-stray check above: a write into ANY third,
    // unanticipated folder must fail this too.
    assert_eq!(
        subdirs(&helper),
        BTreeSet::from(["child_abc_123".to_string()]),
        "storing a transaction must not create any folder beyond the id folder"
    );
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
        amount: allowance_core::money::Money::from_cents(500),
        balance: allowance_core::money::Money::from_cents(500),
        transaction_type: crate::backend::domain::models::transaction::TransactionType::OneOffIncome,
    };
    helper.transaction_repo.store_transaction(&tx).unwrap();

    let before_count = helper
        .transaction_repo
        .list_transactions(&child.id, None, None)
        .unwrap()
        .len();
    let before_dirs = subdirs(&helper);
    assert_eq!(before_count, 1);

    child.name = "Keiko Smith".to_string();
    helper.child_repo.update_child(&child).unwrap();

    let after_count = helper
        .transaction_repo
        .list_transactions(&child.id, None, None)
        .unwrap()
        .len();
    assert_eq!(after_count, 1, "rename must not lose transactions");

    // The directory-set assertion is the load-bearing half. Without it, a
    // regression that resolves to a different-but-consistent wrong folder
    // still passes, because create_dir_all manufactures the folder silently.
    assert_eq!(
        before_dirs,
        subdirs(&helper),
        "rename must not create a directory"
    );
}

#[test]
fn goals_land_in_the_folder_the_id_names() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};
    helper
        .goal_repo
        .store_goal(&DomainGoal {
            id: "goal::1".to_string(),
            child_id: child.id.clone(),
            description: "Bike".to_string(),
            target_amount: 100.0,
            state: DomainGoalState::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        })
        .unwrap();

    assert!(helper
        .env
        .base_path
        .join(&child.id)
        .join("goals.csv")
        .exists());
    assert!(!helper.env.base_path.join("keiko_hart").exists());

    // Stronger than the named-stray check above: a write into ANY third,
    // unanticipated folder must fail this too.
    assert_eq!(
        subdirs(&helper),
        BTreeSet::from(["child_abc_123".to_string()]),
        "storing a goal must not create any folder beyond the id folder"
    );
}

#[test]
fn child_yaml_lands_in_the_folder_the_id_names() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    assert!(helper
        .env
        .base_path
        .join(&child.id)
        .join("child.yaml")
        .exists());
    assert_eq!(
        subdirs(&helper),
        BTreeSet::from(["child_abc_123".to_string()]),
        "exactly one folder, named by the id"
    );
}

/// `AllowanceRepository` used to resolve via a scan of every `child.yaml`
/// under the base directory — a third convention again. It now goes through
/// `CsvConnection::child_dir` like everything else; this pins that the bytes
/// did not move when the scan was deleted.
#[test]
fn allowance_config_lands_in_the_folder_the_id_names() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    helper
        .allowance_repo
        .store_allowance_config(
            &crate::backend::domain::models::allowance::AllowanceConfig {
                child_id: child.id.clone(),
                amount: 5.0,
                day_of_week: 5,
                is_active: true,
                use_age_based_amount: false,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                updated_at: "2024-01-01T00:00:00Z".to_string(),
            },
        )
        .unwrap();

    assert!(
        helper
            .env
            .base_path
            .join(&child.id)
            .join("allowance_config.yaml")
            .exists(),
        "allowance config must land under the id folder, not the sanitized name"
    );
    assert!(
        !helper.env.base_path.join("keiko_hart").exists(),
        "no folder may be created from the display name"
    );
    assert_eq!(
        subdirs(&helper),
        BTreeSet::from(["child_abc_123".to_string()]),
        "storing an allowance config must not create any folder beyond the id folder"
    );
}

/// `ParentalControlRepository` also used to resolve via the base-dir scan. It writes
/// BOTH a per-child `parental_control_attempts.csv` and a global one at the
/// base directory; this pins the per-child path.
///
/// The global file sits directly in the base directory as a *file*, and
/// `subdirs()` filters to directories only, so the full-set assertion below
/// cannot trip on it. The explicit check that it was not created keeps that
/// reasoning honest rather than implicit.
#[test]
fn parental_control_attempts_land_in_the_folder_the_id_names() {
    let helper = TestHelper::new().unwrap();
    let child = helper
        .create_test_child_with_distinct_id("Keiko Hart", "child_abc_123")
        .unwrap();

    helper
        .parental_control_repo
        .record_parental_control_attempt(&child.id, "1234", false)
        .unwrap();

    assert!(
        helper
            .env
            .base_path
            .join(&child.id)
            .join("parental_control_attempts.csv")
            .exists(),
        "per-child attempts must land under the id folder, not the sanitized name"
    );
    assert!(
        !helper.env.base_path.join("keiko_hart").exists(),
        "no folder may be created from the display name"
    );

    // A per-child attempt must not leak into the base-level global file.
    assert!(
        !helper
            .env
            .base_path
            .join("parental_control_attempts.csv")
            .exists(),
        "a per-child attempt must not be written to the global base-level file"
    );

    assert_eq!(
        subdirs(&helper),
        BTreeSet::from(["child_abc_123".to_string()]),
        "recording an attempt must not create any folder beyond the id folder"
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
    let _ = helper
        .transaction_repo
        .list_transactions(&child.id, None, None);

    assert_eq!(
        before,
        subdirs(&helper),
        "a read must never create a child folder"
    );
}
