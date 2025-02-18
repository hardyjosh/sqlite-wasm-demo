// Save the real onconnect handler
let realOnconnect = null;

// Set up onconnect immediately
self.onconnect = (e) => {
    console.log("JS onconnect fired");
    if (realOnconnect) {
        realOnconnect(e);
    } else {
        console.log("WASM not ready yet, connection will be handled after init");
        // Store the event to handle after WASM init
        self._pendingConnection = e;
    }
};

// Initialize the worker
wasm_bindgen("./sqlite_wrapper_bg.wasm").then(async () => {
    try {
        await wasm_bindgen.main();
        console.log("SQLite worker initialized");
        
        // Store the real handler
        realOnconnect = wasm_bindgen.handle_connect;
        
        // Handle any pending connection
        if (self._pendingConnection) {
            console.log("Handling pending connection");
            realOnconnect(self._pendingConnection);
            self._pendingConnection = null;
        }
    } catch (err) {
        console.error("Failed to initialize SQLite worker:", err);
    }
}).catch(err => {
    console.error("Failed to load WASM:", err);
}); 