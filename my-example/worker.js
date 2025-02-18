importScripts('sqlite_wasm_rs.js');

self.onmessage = async function(e) {
    if (e.data === 'try_access') {
        try {
            // Try to initialize OPFS in the worker
            await wasm_bindgen.install_opfs_sahpool(null, true);
            
            // Try to open the same database
            const result = await wasm_bindgen.open_database('cross_worker_test.db');
            self.postMessage({ result: 'Successfully opened database in worker' });
        } catch (error) {
            self.postMessage({ result: `Error: ${error.message}` });
        }
    }
}; 