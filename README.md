# SQLite WASM Example

This example demonstrates how to use SQLite in a WebAssembly environment with Origin Private File System (OPFS) support. The tests cover various aspects of database operations and coordination between workers.

## Test Structure

The test suite is organized into several modules:

### Basic OPFS Tests (`opfs_tests.rs`)

1. `test_opfs_example`
   - Demonstrates basic database operations using OPFS
   - Creates a database, writes data, closes connection
   - Reopens database and verifies data persistence
   - Shows how data survives across different connections

2. `test_simultaneous_connections`
   - Shows how multiple connections can access the same database
   - One connection writes while another reads
   - Demonstrates basic concurrent access patterns

3. `test_concurrent_writes`
   - Tests multiple connections writing to the same database
   - Verifies that all writes are properly serialized
   - Shows how SQLite handles concurrent write operations

### Worker Tests (`worker_tests.rs`)

1. `test_cross_worker_access`
   - Demonstrates limitations of cross-worker OPFS access
   - Shows why coordination is needed for multi-worker scenarios
   - Tests error handling when a worker tries to access a database created in another worker

### Coordinator Tests (`coordinator_tests.rs`)

1. `test_basic_write_coordination`
   - Tests the write coordination protocol
   - Verifies that only one write operation is allowed at a time
   - Shows how write requests are queued
   - Demonstrates that reads are blocked during writes

2. `test_concurrent_reads`
   - Shows that multiple simultaneous reads are allowed
   - Verifies that writes are queued when reads are active
   - Tests the transition from read to write access

## Architecture

The example demonstrates a multi-layered approach to database access:

1. **Base Layer**: SQLite operations through OPFS
   - Direct database operations
   - File system persistence
   - Connection management

2. **Coordination Layer**: Worker access management
   - Write queue management
   - Read/write access control
   - Request coordination

3. **Worker Layer**: Isolated database operations
   - Per-worker database connections
   - Error handling for cross-worker access
   - Message passing for coordination

## Usage

Run the tests with:
```bash
wasm-pack test --firefox --headless
```

This will run all tests in a headless Firefox browser. For debugging, you can run without the `--headless` flag to see the browser console output.

## Key Concepts Demonstrated

1. **Data Persistence**: How to persist SQLite databases in the browser
2. **Concurrent Access**: How to handle multiple connections to the same database
3. **Worker Coordination**: How to manage database access across multiple workers
4. **Error Handling**: How to handle various error conditions and edge cases
5. **Test Organization**: How to structure tests for different aspects of functionality

## Limitations and Considerations

- OPFS access is limited to the worker where it was initialized
- Coordination between workers requires a separate mechanism (e.g., SharedWorker)
- Write operations need to be carefully managed to prevent conflicts
- Error handling is crucial for robust operation

## Technical Details

### OPFS-sahpool VFS Characteristics
1. Thread Safety:
   - SQLite is compiled with -DSQLITE_THREADSAFE=0
   - Not thread-safe by design
   - JsValue cannot be shared across threads

2. Connection Handling:
   - Multiple connections are supported within the same worker
   - Connections are not sharable across workers
   - Each worker needs its own connection

3. OPFS Integration:
   - Provides persistent storage through the Origin Private File System
   - Does not require COOP/COEP HTTP headers
   - Storage is isolated to the origin

## Best Practices

1. Connection Management:
   - Keep connections within the same worker
   - Close connections when done
   - Use multiple connections for concurrent operations

2. Worker Usage:
   - Don't try to share connections across workers
   - Each worker should manage its own database connections
   - Use message passing for cross-worker communication

3. Error Handling:
   - Always check SQLite return codes
   - Properly clean up resources
   - Handle worker communication errors

## Conclusions

OPFS-sahpool provides a robust solution for persistent SQLite storage in WebAssembly, but with important limitations:
1. Great for single-worker scenarios
2. Supports multiple concurrent connections within a worker
3. Not suitable for cross-worker database sharing
4. Provides good performance for typical web application needs

These examples demonstrate both the capabilities and limitations of SQLite WASM with OPFS-sahpool, helping developers make informed decisions about their database architecture in WebAssembly applications.

# SQLite WASM Coordinator Architecture

This module implements a tab coordination system for SQLite WASM that manages database access across multiple browser tabs and workers.

## Architecture Overview

### Core Components

1. **Tab Coordinator (`MockCoordinator`)**
   - Manages active tabs and their health status
   - Coordinates read/write access to the database
   - Handles tab migration when failures occur
   - Manages transactions and query routing

2. **State Management**
   ```rust
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
   ```

### Key Features

1. **Concurrent Access Control**
   - Multiple simultaneous readers allowed
   - Single writer at a time
   - Write requests are queued when a writer is active
   - Reads are blocked during writes

2. **Tab Health Monitoring**
   - Tracks tab health using timestamps
   - Automatically migrates to healthy tabs when failures occur
   - Configurable health check intervals
   - Graceful handling of tab closures

3. **Transaction Management**
   - Atomic transaction support
   - Blocks other writes during transactions
   - Maintains operation ordering
   - Ensures data consistency

4. **Query Routing**
   - Routes queries through active tabs
   - Handles query failures and retries
   - Maintains operation order across tabs

5. **OPFS Access Coordination**
   - All OPFS operations are routed through the active tab's worker
   - Ensures OPFS access remains within a single worker context
   - Uses message passing to coordinate between tabs
   - Maintains OPFS access restrictions while enabling multi-tab functionality

   ```rust
   // Example of how queries are routed to active tab
   pub async fn route_query(&self, from_tab: &str, query: SQLQuery) -> Result<QueryResult, String> {
       let state = self.state.lock().unwrap();
       
       if let Some(active_tab) = &state.active_tab {
           // Route the query through the active tab's worker
           Ok(QueryResult {
               routed_through: active_tab.clone(),
               data: vec![], // In real impl, would contain results from active tab's worker
           })
       } else {
           Err("No active tab available".to_string())
       }
   }
   ```

   This design pattern ensures we respect OPFS's single-worker limitation while still
   providing multi-tab database access through careful coordination and routing.

## Testing Strategy

The system is thoroughly tested using WASM-bindgen test suite:

1. **Basic Coordination Tests**
   ```rust
   #[wasm_bindgen_test]
   async fn test_basic_write_coordination()
   #[wasm_bindgen_test]
   async fn test_concurrent_reads()
   ```
   - Verifies basic read/write coordination
   - Tests concurrent access patterns
   - Ensures proper queuing behavior

2. **Tab Management Tests**
   ```rust
   #[wasm_bindgen_test]
   async fn test_tab_migration()
   #[wasm_bindgen_test]
   async fn test_tab_health_monitoring()
   ```
   - Tests tab registration and health tracking
   - Verifies automatic migration on failures
   - Ensures proper tab state management

3. **Transaction Tests**
   ```rust
   #[wasm_bindgen_test]
   async fn test_transaction_coordination()
   ```
   - Verifies transaction isolation
   - Tests write blocking during transactions
   - Ensures proper transaction cleanup

4. **Error Recovery Tests**
   ```rust
   #[wasm_bindgen_test]
   async fn test_error_recovery()
   ```
   - Tests system recovery from failures
   - Verifies data consistency after errors
   - Ensures proper state restoration

5. **Integration Tests**
   ```rust
   #[wasm_bindgen_test]
   async fn test_complete_tab_migration()
   ```
   - Tests end-to-end scenarios
   - Verifies system behavior under load
   - Tests multiple feature interactions

## Usage Example

```rust
// Initialize coordinator
let coordinator = MockCoordinator::new();

// Register tabs
coordinator.register_tab("tab1", Some(Duration::from_secs(5))).await;
coordinator.register_tab("tab2", Some(Duration::from_secs(5))).await;

// Request write access
let response = coordinator.request_access("worker1", "write").await;
if response.granted {
    // Perform write operation
    coordinator.complete_operation("worker1").await;
}

// Start transaction
coordinator.begin_transaction("tab1").await;
// Perform transactional operations
coordinator.commit_transaction("tab1").await;
```

## Performance Considerations

1. **Lock Management**
   - Uses fine-grained locking for state access
   - Minimizes lock contention
   - Releases locks during async operations

2. **Resource Usage**
   - Tracks active connections
   - Monitors memory and storage usage
   - Implements cleanup for stale resources

3. **Scalability**
   - Handles multiple concurrent tabs
   - Efficient queue management
   - Optimized state transitions

## Error Handling

1. **Failure Detection**
   - Monitors tab health
   - Detects timeouts and crashes
   - Identifies network issues

2. **Recovery Mechanisms**
   - Automatic tab migration
   - Transaction rollback
   - State restoration
   - Operation replay if needed

## Future Improvements

1. **Enhanced Monitoring**
   - More detailed health metrics
   - Performance tracking
   - Resource usage analytics

2. **Advanced Features**
   - Distributed transactions
   - Priority queuing
   - Custom routing strategies

3. **Testing Enhancements**
   - Load testing scenarios
   - Chaos testing
   - Performance benchmarks 