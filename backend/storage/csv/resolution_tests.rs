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
#[ignore = "fixed by the registry cutover in Task 9"]
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
}

#[test]
#[ignore = "fixed by the registry cutover in Task 9"]
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

#[test]
#[ignore = "fixed by the registry cutover in Task 9"]
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
