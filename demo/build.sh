#!/bin/bash
set -e

# Build the tab coordinator shared worker
wasm-pack build --target no-modules --out-dir ../../demo/pkg/worker crates/tab_coordinator_shared_worker

# Append the worker initialization code with embedded wasm base64 (shell-only)
base64 < ./demo/pkg/worker/tab_coordinator_shared_worker_bg.wasm | tr -d '\n' > ./demo/pkg/worker/tab_coordinator_shared_worker_bg.wasm.b64
awk 'BEGIN{getline b64<"./demo/pkg/worker/tab_coordinator_shared_worker_bg.wasm.b64"} {gsub(/__WASM_B64_TC__/, b64)}1' demo/tab-coordinator-append.js >> ./demo/pkg/worker/tab_coordinator_shared_worker.js

# Build the SQLite wrapper for the worker
wasm-pack build --target no-modules --out-dir ../../demo/pkg/sqlite_wrapper crates/sqlite_wrapper

# Append our worker init with embedded wasm base64 (shell-only)
base64 < ./demo/pkg/sqlite_wrapper/sqlite_wrapper_bg.wasm | tr -d '\n' > ./demo/pkg/sqlite_wrapper/sqlite_wrapper_bg.wasm.b64
awk 'BEGIN{getline b64<"./demo/pkg/sqlite_wrapper/sqlite_wrapper_bg.wasm.b64"} {gsub(/__WASM_B64_SQLITE__/, b64)}1' demo/sqlite-worker-append.js >> ./demo/pkg/sqlite_wrapper/sqlite_wrapper.js

# Produce base64 bundles of the final worker scripts (JS + embedded wasm shim)
base64 < ./demo/pkg/sqlite_wrapper/sqlite_wrapper.js | tr -d '\n' > ./demo/pkg/sqlite_wrapper/worker_bundle.js.b64
base64 < ./demo/pkg/worker/tab_coordinator_shared_worker.js | tr -d '\n' > ./demo/pkg/worker/shared_worker_bundle.js.b64

# Build the browser interface
wasm-pack build --target web --out-dir ../../demo/pkg crates/browser_sqlite

cd demo
python3 server.py 