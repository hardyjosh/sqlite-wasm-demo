# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a browser-based SQLite implementation using WebAssembly and Rust. The project implements a multi-tab coordination pattern to manage SQLite database access across browser tabs using OPFS (Origin Private File System) with synchronous access handles for optimal performance.

## Architecture

The codebase is organized as a Rust workspace with four main crates:

- **browser_sqlite**: Main browser interface that applications interact with
- **sqlite_wrapper**: Core SQLite database operations running in dedicated workers  
- **tab_coordinator**: Cross-tab coordination logic for managing database access
- **tab_coordinator_shared_worker**: SharedWorker implementation for tab coordination

### Key Architecture Patterns

The system follows the architecture documented in `sqlite-browser-arch.md`:

1. **Single Active Tab**: Only one tab at a time has direct SQLite access via OPFS sync handles
2. **Tab Coordination**: SharedWorker coordinates which tab is the active database owner
3. **Query Routing**: All database operations are routed through the active tab
4. **Tab Migration**: When the active tab closes, coordination transfers to another tab

## Development Commands

### Building the Project

Build all components and start development server:
```bash
./demo/build.sh
```

This script:
1. Builds the SharedWorker coordinator (`tab_coordinator_shared_worker`)
2. Builds the SQLite wrapper for workers (`sqlite_wrapper`) 
3. Builds the browser interface (`browser_sqlite`)
4. Starts a development server at http://localhost:8080

### Individual Builds

Build specific components:
```bash
# SharedWorker coordinator
wasm-pack build --target no-modules --out-dir ../../demo/pkg/worker crates/tab_coordinator_shared_worker

# SQLite wrapper (for workers)  
wasm-pack build --target web --out-dir ../../demo/pkg/sqlite_wrapper crates/sqlite_wrapper

# Browser interface
wasm-pack build --target web --out-dir ../../demo/pkg crates/browser_sqlite
```

### Development Server

The development server requires CORS headers for SharedArrayBuffer support:
```bash
cd demo
python3 server.py
```

Server runs on http://localhost:8080 with required COOP/COEP headers.

## Key Components

### BrowserSQLite (browser_sqlite/src/lib.rs)
Main API that applications use. Manages worker creation and coordinates with TabManager for database operations.

### Database (sqlite_wrapper/src/lib.rs)  
SQLite operations running in dedicated workers. Uses OPFS synchronous access handles for performance. Only the active tab's worker actually connects to the database.

### TabManager (tab_coordinator/src/lib.rs)
Handles cross-tab coordination via SharedWorker. Manages leader election, query routing, and tab lifecycle.

## Message Flow

1. Application calls BrowserSQLite methods
2. TabManager checks if current tab is the leader
3. If leader: Execute directly via worker
4. If not leader: Route query through SharedWorker to leader tab
5. Leader tab executes and returns results

## Testing Multi-Tab Functionality

1. Run `./demo/build.sh` to build and start server
2. Open multiple browser tabs to http://localhost:8080  
3. Only one tab will be the active database leader
4. Close the leader tab to see coordination transfer to another tab

## Important Dependencies

- `sqlite-wasm-rs`: Rust bindings for SQLite compiled to WebAssembly
- `wasm-bindgen`: Rust/WebAssembly/JS interop
- `web-sys`: Web APIs bindings for Rust
- `uuid`: Tab identification
- `serde`: Message serialization between tabs/workers

## Browser Requirements

- SharedWorker support (for tab coordination)
- OPFS with synchronous access handles (for optimal performance)
- WebAssembly support
- Requires COOP/COEP headers for SharedArrayBuffer