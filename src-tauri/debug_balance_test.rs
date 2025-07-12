#[tokio::test]
async fn debug_balance_update() {
    // Test just the update_transaction_balances method in isolation
    let service = create_test_service().await;
    
    // Create a child first
    let temp_dir = tempfile::tempdir().unwrap();
    let db = Arc::new(CsvConnection::new(temp_dir.path().to_path_buf()).unwrap());
    let child_service = crate::backend::domain::child_service::ChildService::new(db.clone());
    let child_result = child_service.create_child(CreateChildCommand {
        name: "Test Child".to_string(),
        birthdate: "2015-01-01".to_string(),
    }).await.unwrap();
    let child_id = &child_result.child.id;
    
    // Create a transaction
    let transaction = create_test_transaction(&service, child_id, "2025-01-10T10:00:00-05:00", "Test", 100.0, 100.0).await;
    
    // Test the get_all_child_ids method
    let child_ids = service.transaction_repository.get_all_child_ids().await.unwrap();
    println!("Child IDs found: {:?}", child_ids);
    println!("Test child ID: {}", child_id);
    
    // Test update_transaction_balances directly
    let updates = vec![(transaction.id.clone(), 999.0)];
    let result = service.transaction_repository.update_transaction_balances(&updates).await;
    println!("Update result: {:?}", result);
    
    // Check if the balance was actually updated
    let updated_transaction = service.transaction_repository.get_transaction(child_id, &transaction.id).await.unwrap();
    if let Some(tx) = updated_transaction {
        println!("Transaction balance after update: {}", tx.balance);
        assert_eq!(tx.balance, 999.0);
    } else {
        panic!("Transaction not found after update");
    }
}
