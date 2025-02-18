#!/bin/bash
set -e

# Build the worker
cd worker
wasm-pack build --target no-modules --out-dir ../demo/pkg/worker --out-name worker

# Build the main wasm
cd ..
wasm-pack build --target web --out-dir demo/pkg

# Append the onconnect handler to the worker.js
cat demo/worker-append.js >> demo/pkg/worker/worker.js

cd demo
python3 -m http.server 8080 