#!/bin/bash
set -e

# Build the worker
cd worker
wasm-pack build --target no-modules --out-dir ../demo/pkg/worker --out-name worker
cd ..

# Add WASM initialization to worker.js
cat >> demo/pkg/worker/worker.js << 'EOL'

// Initialize WASM
wasm_bindgen("./worker_bg.wasm")
    .then(() => {
        console.log("Worker WASM loaded");
        wasm_bindgen.main();
    })
    .catch(console.error);
EOL

# Build the main package
wasm-pack build --target web --out-dir demo/pkg

cd demo
python3 -m http.server 8080 