use crate::coordinator::MockCoordinator;
use crate::SQLQuery;
use std::time::Duration;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
async fn test_basic_write_coordination() {
    let coordinator = MockCoordinator::new();

    // First write should be granted
    let response1 = coordinator.request_access("worker1", "write").await;
    assert!(response1.granted, "First write should be granted");

    // Second write should be queued
    let response2 = coordinator.request_access("worker2", "write").await;
    assert!(!response2.granted, "Second write should be queued");
    assert_eq!(response2.queue_position, Some(0));

    // Read during write should be denied
    let response3 = coordinator.request_access("worker3", "read").await;
    assert!(!response3.granted, "Read during write should be denied");

    // Complete first write
    coordinator.complete_operation("worker1").await;

    // Queued write should now be granted
    let next_response = coordinator.get_next_queued_response().await;
    assert!(next_response.granted);
    assert_eq!(next_response.worker_id, "worker2");
}

#[wasm_bindgen_test]
async fn test_concurrent_reads() {
    let coordinator = MockCoordinator::new();

    // Multiple reads should be allowed
    let response1 = coordinator.request_access("worker1", "read").await;
    let response2 = coordinator.request_access("worker2", "read").await;

    assert!(response1.granted, "First read should be granted");
    assert!(response2.granted, "Second read should be granted");

    // Write during read should be queued
    let response3 = coordinator.request_access("worker3", "write").await;
    assert!(!response3.granted, "Write during read should be queued");

    // Complete reads
    coordinator.complete_operation("worker1").await;
    coordinator.complete_operation("worker2").await;

    // Queued write should now be granted
    let next_response = coordinator.get_next_queued_response().await;
    assert!(next_response.granted);
    assert_eq!(next_response.worker_id, "worker3");
}

#[wasm_bindgen_test]
async fn test_tab_migration() {
    let coordinator = MockCoordinator::new();

    // Register tabs with health checks
    coordinator.register_tab("tab1", None).await;
    coordinator.register_tab("tab2", None).await;
    coordinator.set_active_tab("tab1").await;

    // Simulate tab1 closing
    coordinator.notify_tab_closed("tab1").await;

    // Verify tab2 becomes active
    let active_tab = coordinator.get_active_tab().await;
    assert_eq!(
        active_tab.as_deref(),
        Some("tab2"),
        "tab2 should become active after tab1 closes"
    );
}

#[wasm_bindgen_test]
async fn test_tab_health_monitoring() {
    let coordinator = MockCoordinator::new();

    // Register tabs with health checks
    coordinator
        .register_tab("tab1", Some(Duration::from_secs(5)))
        .await;
    coordinator
        .register_tab("tab2", Some(Duration::from_secs(5)))
        .await;

    // Set tab1 as active
    coordinator.set_active_tab("tab1").await;

    // Simulate tab becoming unresponsive
    coordinator.simulate_tab_timeout("tab1").await;

    // Verify automatic migration
    let active_tab = coordinator.get_active_tab().await;
    assert_eq!(
        active_tab,
        Some("tab2".to_string()),
        "Should migrate to healthy tab"
    );
}

#[wasm_bindgen_test]
async fn test_transaction_coordination() {
    let coordinator = MockCoordinator::new();

    // Start transaction
    coordinator.begin_transaction("tab1").await;

    // Verify other writes are blocked
    let response = coordinator.request_access("tab2", "write").await;
    assert!(
        !response.granted,
        "Writes should be blocked during transaction"
    );

    // Complete transaction
    coordinator.commit_transaction("tab1").await;

    // Verify writes are now allowed
    let response = coordinator.request_access("tab2", "write").await;
    assert!(
        response.granted,
        "Writes should be allowed after transaction"
    );
}

#[wasm_bindgen_test]
async fn test_query_routing() {
    let coordinator = MockCoordinator::new();

    // Register active tab
    coordinator.register_tab("tab1", None).await;
    coordinator.set_active_tab("tab1").await;

    // Test query routing
    let query = SQLQuery::new("SELECT * FROM users");
    let result = coordinator.route_query("tab2", query).await;

    assert!(result.is_ok(), "Query should be routed successfully");
    assert_eq!(
        result.unwrap().routed_through,
        "tab1",
        "Query should route through active tab"
    );
}

#[wasm_bindgen_test]
async fn test_complete_tab_migration() {
    let coordinator = MockCoordinator::new();

    // Set up initial state
    coordinator.register_tab("tab1", None).await;
    coordinator.register_tab("tab2", None).await;
    coordinator.set_active_tab("tab1").await;

    // Queue some operations
    coordinator
        .queue_operation("tab2", "SELECT * FROM users")
        .await;

    // Trigger migration
    coordinator.notify_tab_closed("tab1").await;

    // Verify migration
    assert_eq!(coordinator.get_active_tab().await.unwrap(), "tab2");

    // Verify queued operations were transferred
    let completed_ops = coordinator.get_completed_operations().await;
    assert_eq!(
        completed_ops.len(),
        1,
        "Queued operation should be completed"
    );
}

#[wasm_bindgen_test]
async fn test_error_recovery() {
    let coordinator = MockCoordinator::new();

    // Set up tabs
    coordinator.register_tab("tab1", None).await;
    coordinator.register_tab("tab2", None).await;

    // Simulate failed operation
    coordinator.simulate_operation_failure("tab1").await;

    // Verify automatic recovery
    let recovery_status = coordinator.get_recovery_status().await;
    assert!(
        recovery_status.recovered,
        "System should recover from failure"
    );
    assert!(
        recovery_status.data_consistent,
        "Data should remain consistent"
    );
}

#[wasm_bindgen_test]
async fn test_basic_usage() {
    // Initialize coordinator
    let coordinator = MockCoordinator::new();

    // Register tabs
    coordinator
        .register_tab("tab1", Some(std::time::Duration::from_secs(5)))
        .await;
    coordinator
        .register_tab("tab2", Some(std::time::Duration::from_secs(5)))
        .await;

    // Set active tab
    coordinator.set_active_tab("tab1").await;

    // Route a query
    let query = SQLQuery::new("SELECT * FROM users");
    let result = coordinator.route_query("tab2", query).await;
    assert!(result.is_ok());
}
