#!/bin/bash
set -e

# Build the worker
wasm-pack build --target no-modules --out-dir ../../demo/pkg/worker crates/tab_coordinator_shared_worker

# Append the worker initialization code
cat demo/worker-append.js >> ./demo/pkg/worker/tab_coordinator_shared_worker.js

# Build the main library
wasm-pack build --target web --out-dir ../../demo/pkg crates/tab_coordinator

cd demo
python3 -m http.server 8080 