#!/bin/bash
set -e

# Build the tab coordinator shared worker
wasm-pack build --target no-modules --out-dir ../../demo/pkg/worker crates/tab_coordinator_shared_worker

# Append the worker initialization code
cat demo/tab-coordinator-append.js >> ./demo/pkg/worker/tab_coordinator_shared_worker.js

# Build the SQLite wrapper for the worker
wasm-pack build --target no-modules --out-dir ../../demo/pkg/sqlite_wrapper crates/sqlite_wrapper

# Remove the auto-initialization and append our worker init
cat demo/sqlite-worker-append.js >> demo/pkg/sqlite_wrapper/sqlite_wrapper.js


# Build the browser interface
wasm-pack build --target web --out-dir ../../demo/pkg crates/browser_sqlite

cd demo
python3 server.py 