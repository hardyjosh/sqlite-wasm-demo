use crate::get_time_ms;
use crate::{
    PendingOperation, QueryResult, RecoveryStatus, ResourceMetrics, SQLQuery, TransactionState,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AccessResponse {
    pub granted: bool,
    pub worker_id: String,
    pub operation: String,
    pub queue_position: Option<usize>,
}

#[derive(Debug)]
pub struct QueuedRequest {
    worker_id: String,
    operation: String,
}

pub struct MockCoordinator {
    state: Arc<Mutex<CoordinatorState>>,
}

#[derive(Default)]
struct CoordinatorState {
    active_writer: Option<String>,
    write_queue: VecDeque<QueuedRequest>,
    active_readers: HashMap<String, ()>,
    active_tab: Option<String>,
    tab_health: HashMap<String, f64>,
    pending_operations: Vec<PendingOperation>,
    current_transaction: Option<TransactionState>,
    active_connections: HashMap<String, ()>,
}

impl MockCoordinator {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CoordinatorState::default())),
        }
    }

    pub async fn request_access(&self, worker_id: &str, operation: &str) -> AccessResponse {
        let mut state = self.state.lock().unwrap();
        let worker_id = worker_id.to_string();
        let operation = operation.to_string();

        // Block all writes if there's an active transaction
        if operation == "write" && state.current_transaction.is_some() {
            return AccessResponse {
                granted: false,
                worker_id,
                operation,
                queue_position: Some(state.write_queue.len()),
            };
        }

        match operation.as_str() {
            "write" => {
                if state.active_writer.is_none() && state.active_readers.is_empty() {
                    state.active_writer = Some(worker_id.clone());
                    AccessResponse {
                        granted: true,
                        worker_id,
                        operation,
                        queue_position: None,
                    }
                } else {
                    state.write_queue.push_back(QueuedRequest {
                        worker_id: worker_id.clone(),
                        operation: operation.clone(),
                    });
                    AccessResponse {
                        granted: false,
                        worker_id,
                        operation,
                        queue_position: Some(state.write_queue.len() - 1),
                    }
                }
            }
            "read" => {
                if state.active_writer.is_none() {
                    state.active_readers.insert(worker_id.clone(), ());
                    AccessResponse {
                        granted: true,
                        worker_id,
                        operation,
                        queue_position: None,
                    }
                } else {
                    AccessResponse {
                        granted: false,
                        worker_id,
                        operation,
                        queue_position: None,
                    }
                }
            }
            _ => panic!("Unknown operation type"),
        }
    }

    pub async fn complete_operation(&self, worker_id: &str) {
        let mut state = self.state.lock().unwrap();

        // Remove from active writers if present
        if state
            .active_writer
            .as_ref()
            .map(|w| w == worker_id)
            .unwrap_or(false)
        {
            state.active_writer = None;
        }

        // Remove from active readers if present
        state.active_readers.remove(worker_id);
    }

    pub async fn get_next_queued_response(&self) -> AccessResponse {
        let mut state = self.state.lock().unwrap();

        if let Some(next_request) = state.write_queue.pop_front() {
            if state.active_writer.is_none() && state.active_readers.is_empty() {
                state.active_writer = Some(next_request.worker_id.clone());
                AccessResponse {
                    granted: true,
                    worker_id: next_request.worker_id,
                    operation: next_request.operation,
                    queue_position: None,
                }
            } else {
                panic!("Unexpected state: Cannot grant access to queued request");
            }
        } else {
            panic!("No queued requests");
        }
    }

    pub async fn register_tab(&self, tab_id: &str, _health_check_interval: Option<Duration>) {
        let mut state = self.state.lock().unwrap();
        state.tab_health.insert(tab_id.to_string(), get_time_ms());
    }

    pub async fn notify_tab_closed(&self, tab_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.tab_health.remove(tab_id);

        if state.active_tab.as_deref() == Some(tab_id) {
            drop(state); // Release lock before async call
            self.migrate_active_tab().await;
        }
    }

    async fn migrate_active_tab(&self) {
        let mut state = self.state.lock().unwrap();

        // Find next healthy tab
        let current_time = get_time_ms();
        let timeout = 5000.0; // 5 seconds timeout

        let next_tab = state
            .tab_health
            .iter()
            .find(|(tab_id, &last_seen)| {
                // Skip current active tab and check health
                Some(tab_id.as_str()) != state.active_tab.as_deref()
                    && (current_time - last_seen) < timeout
            })
            .map(|(tab_id, _)| tab_id.clone());

        // Update active tab
        if next_tab.is_some() {
            state.active_tab = next_tab;
        }
    }

    pub async fn get_resource_metrics(&self) -> ResourceMetrics {
        let state = self.state.lock().unwrap();
        ResourceMetrics {
            active_connections: state.active_connections.len(),
            pending_operations: state.pending_operations.len(),
            memory_usage: self.calculate_memory_usage(),
            storage_usage: self.calculate_storage_usage(),
        }
    }

    pub async fn set_active_tab(&self, tab_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.active_tab = Some(tab_id.to_string());
    }

    pub async fn route_query(
        &self,
        _from_tab: &str,
        _query: SQLQuery,
    ) -> Result<QueryResult, String> {
        let state = self.state.lock().unwrap();

        if let Some(active_tab) = &state.active_tab {
            Ok(QueryResult {
                routed_through: active_tab.clone(),
                data: vec![], // In real implementation, would contain query results
            })
        } else {
            Err("No active tab available".to_string())
        }
    }

    pub async fn queue_operation(&self, tab_id: &str, sql: &str) {
        let mut state = self.state.lock().unwrap();
        let op_id = format!("op-{}", state.pending_operations.len());
        state.pending_operations.push(PendingOperation {
            id: op_id,
            sql: sql.to_string(),
            tab_id: tab_id.to_string(),
            timestamp: get_time_ms(),
        });
    }

    pub async fn get_completed_operations(&self) -> Vec<PendingOperation> {
        self.state.lock().unwrap().pending_operations.clone()
    }

    pub async fn simulate_operation_failure(&self, _tab_id: &str) {
        // Simulate failure and recovery
    }

    pub async fn get_recovery_status(&self) -> RecoveryStatus {
        RecoveryStatus {
            recovered: true,
            data_consistent: true,
            error_details: None,
        }
    }

    pub async fn begin_transaction(&self, tab_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.current_transaction = Some(TransactionState {
            tab_id: tab_id.to_string(),
            operations: Vec::new(),
            start_time: get_time_ms(),
        });
    }

    pub async fn commit_transaction(&self, _tab_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.current_transaction = None;
    }

    fn calculate_memory_usage(&self) -> usize {
        // In real implementation, would track actual memory usage
        0
    }

    fn calculate_storage_usage(&self) -> usize {
        // In real implementation, would track actual storage usage
        0
    }

    pub async fn get_pending_operations(&self) -> Vec<PendingOperation> {
        self.state.lock().unwrap().pending_operations.clone()
    }

    pub async fn simulate_tab_timeout(&self, tab_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.tab_health.remove(tab_id);

        // If the timed out tab was active, migrate to tab2
        if state.active_tab.as_deref() == Some(tab_id) {
            // Find a healthy tab to migrate to
            let current_time = get_time_ms();
            let timeout = 5000.0; // 5 seconds timeout

            let next_tab = state
                .tab_health
                .iter()
                .find(|(_, &last_seen)| (current_time - last_seen) < timeout)
                .map(|(tab_id, _)| tab_id.clone());

            state.active_tab = next_tab;
        }
    }

    pub async fn get_active_tab(&self) -> Option<String> {
        self.state.lock().unwrap().active_tab.clone()
    }
}
