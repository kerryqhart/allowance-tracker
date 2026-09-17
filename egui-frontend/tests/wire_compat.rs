//! The domain Transaction is serialized directly onto the AWS wire
//! (app_coordinator.rs:512-526) and read by the MCP Lambda in the
//! zephytop-brain stack. Its JSON shape is a cross-repo contract.

use allowance_tracker_egui::backend::domain::models::transaction::{Transaction, TransactionType};
use allowance_core::money::Money;

/// A payload in exactly the shape already stored in DynamoDB, written before
/// Money existed. It must still deserialize.
const STORED_PAYLOAD: &str = r#"{
  "id": "ex-1702516125000-af3c",
  "child_id": "keiko_hart",
  "date": "2026-08-01T12:05:00-04:00",
  "description": "Slime kit",
  "amount": -5.5,
  "balance": 27.25,
  "transaction_type": "Expense"
}"#;

#[test]
fn legacy_stored_payload_still_deserializes() {
    let tx: Transaction = serde_json::from_str(STORED_PAYLOAD).unwrap();
    assert_eq!(tx.amount, Money::from_cents(-550));
    assert_eq!(tx.balance, Money::from_cents(2725));
}

#[test]
fn reserialization_keeps_numbers_as_numbers() {
    let tx: Transaction = serde_json::from_str(STORED_PAYLOAD).unwrap();
    let value: serde_json::Value = serde_json::from_str(&serde_json::to_string(&tx).unwrap()).unwrap();
    assert!(value["amount"].is_number(), "amount must stay a JSON number");
    assert!(value["balance"].is_number(), "balance must stay a JSON number");
    assert_eq!(value["amount"].as_f64().unwrap(), -5.5);
    assert_eq!(value["balance"].as_f64().unwrap(), 27.25);
}

#[test]
fn field_names_are_unchanged() {
    let tx: Transaction = serde_json::from_str(STORED_PAYLOAD).unwrap();
    let value: serde_json::Value = serde_json::from_str(&serde_json::to_string(&tx).unwrap()).unwrap();
    for key in ["id", "child_id", "date", "description", "amount", "balance", "transaction_type"] {
        assert!(value.get(key).is_some(), "missing wire field: {key}");
    }
}
