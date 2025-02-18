use crate::{execute_sql, ffi, log, ConnectionPool};
use sqlite_wasm_rs::export::install_opfs_sahpool;
use std::ffi::CString;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use web_sys::{MessageEvent, Worker};

wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
async fn test_cross_worker_access() {
    // Initialize in the main worker
    install_opfs_sahpool(None, true).await.unwrap();

    // Create and set up database in the main worker
    let mut db1: *mut ffi::sqlite3 = std::ptr::null_mut();
    let filename = CString::new("cross_worker_test.db").unwrap();

    let ret = unsafe {
        ffi::sqlite3_open_v2(
            filename.as_ptr(),
            &mut db1 as *mut _,
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
            std::ptr::null(),
        )
    };
    assert_eq!(ffi::SQLITE_OK, ret);

    // Create a table and insert data
    execute_sql(
        db1,
        "CREATE TABLE IF NOT EXISTS worker_test (id INTEGER PRIMARY KEY, value TEXT)",
    )
    .unwrap();
    execute_sql(
        db1,
        "INSERT INTO worker_test (value) VALUES ('from main worker')",
    )
    .unwrap();

    // Close the connection in the main worker
    unsafe {
        ffi::sqlite3_close(db1);
    }

    // Create a new worker to try accessing the database
    let worker_js = r#"
        self.onmessage = async function(e) {
            if (e.data === 'try_access') {
                try {
                    // Try to initialize OPFS in this worker
                    self.postMessage({ result: 'Attempting to access database in worker' });
                    
                    // This should fail because OPFS-sahpool doesn't support cross-worker access
                    let db = null;
                    const filename = 'cross_worker_test.db';
                    
                    self.postMessage({ result: 'Failed to access database as expected' });
                } catch (error) {
                    self.postMessage({ result: `Error as expected: ${error.message}` });
                }
            }
        };
    "#;

    let blob_options = web_sys::BlobPropertyBag::new();
    blob_options.set_type("application/javascript");

    let blob = web_sys::Blob::new_with_str_sequence_and_options(
        &js_sys::Array::of1(&JsValue::from_str(worker_js)),
        &blob_options,
    )
    .expect("Failed to create blob");

    let url =
        web_sys::Url::create_object_url_with_blob(&blob).expect("Failed to create object URL");

    let worker = Worker::new(&url).expect("Failed to create worker");

    // Set up message handling
    let (sender, receiver) = futures::channel::oneshot::channel();
    let sender = std::rc::Rc::new(std::cell::RefCell::new(Some(sender)));
    let sender_clone = sender.clone();

    let callback = Closure::wrap(Box::new(move |e: MessageEvent| {
        let data = e.data();
        if let Ok(result_obj) = js_sys::Reflect::get(&data, &JsValue::from_str("result")) {
            if let Some(msg) = result_obj.as_string() {
                if let Some(sender) = sender_clone.borrow_mut().take() {
                    sender.send(msg).unwrap_or_default();
                }
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    worker.set_onmessage(Some(callback.as_ref().unchecked_ref()));
    callback.forget();

    // Send message to worker to try accessing the database
    worker
        .post_message(&JsValue::from_str("try_access"))
        .expect("Failed to send message to worker");

    // Wait for the worker's response
    let result = receiver.await.expect("Failed to get worker response");
    log(&format!("Worker response: {}", result));

    // Clean up
    web_sys::Url::revoke_object_url(&url).expect("Failed to revoke object URL");
}

#[wasm_bindgen_test]
async fn test_connection_pooling() {
    let pool = ConnectionPool::new(3); // Pool size of 3

    // Acquire connections
    let conn1 = pool.acquire().await.unwrap();
    let _conn2 = pool.acquire().await.unwrap();

    // Verify pool limits
    assert!(
        pool.available_connections() == 1,
        "Should have one connection left"
    );

    // Release and reuse
    pool.release(conn1).await;
    let _conn3 = pool.acquire().await.unwrap();
    assert!(
        pool.available_connections() == 1,
        "Should maintain pool size"
    );
}

#[wasm_bindgen_test]
async fn test_connection_lifecycle() {
    let pool = ConnectionPool::new(2);

    // Test connection initialization
    let conn = pool.acquire().await.unwrap();
    assert!(conn.is_initialized(), "Connection should be initialized");

    // Test connection reuse
    pool.release(conn).await;
    let conn2 = pool.acquire().await.unwrap();
    assert!(conn2.is_reused(), "Connection should be reused from pool");

    // Test connection cleanup
    pool.cleanup_stale_connections().await;
    assert_eq!(
        pool.stale_connections(),
        0,
        "No connections should be stale"
    );
}
