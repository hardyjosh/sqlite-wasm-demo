use sqlite_wasm_rs::export::{self as ffi, install_opfs_sahpool};
use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{self, JsFuture};
use web_sys::DedicatedWorkerGlobalScope;

#[wasm_bindgen]
pub struct Database {
    filename: String,
}

#[wasm_bindgen]
impl Database {
    #[wasm_bindgen(constructor)]
    pub async fn new(filename: &str) -> Result<Database, JsValue> {
        // Initialize OPFS once
        install_opfs_sahpool(None, true)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(Database {
            filename: filename.to_string(),
        })
    }

    pub fn execute(&self, sql: &str) -> Result<(), JsValue> {
        // Open DB
        let mut db = std::ptr::null_mut();
        let filename = CString::new(self.filename.as_str()).unwrap();
        let ret = unsafe {
            ffi::sqlite3_open_v2(
                filename.as_ptr(),
                &mut db,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                std::ptr::null(),
            )
        };

        if ret != ffi::SQLITE_OK {
            return Err(JsValue::from_str("Failed to open database"));
        }

        // Execute SQL
        let sql = CString::new(sql).unwrap();
        let mut err_msg = std::ptr::null_mut();
        let ret = unsafe {
            ffi::sqlite3_exec(db, sql.as_ptr(), None, std::ptr::null_mut(), &mut err_msg)
        };

        // Close DB
        unsafe { ffi::sqlite3_close(db) };

        if ret != ffi::SQLITE_OK {
            let error = unsafe { CString::from_raw(err_msg).into_string().unwrap() };
            unsafe { ffi::sqlite3_free(err_msg as *mut _) };
            return Err(JsValue::from_str(&error));
        }

        Ok(())
    }

    pub fn query(&self, sql: &str) -> Result<JsValue, JsValue> {
        // Open DB
        let mut db = std::ptr::null_mut();
        let filename = CString::new(self.filename.as_str()).unwrap();
        let ret = unsafe {
            ffi::sqlite3_open_v2(
                filename.as_ptr(),
                &mut db,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                std::ptr::null(),
            )
        };

        if ret != ffi::SQLITE_OK {
            return Err(JsValue::from_str("Failed to open database"));
        }

        // Query logic
        let sql = CString::new(sql).unwrap();
        let mut stmt = std::ptr::null_mut();
        let mut results = Vec::new();

        let ret = unsafe {
            ffi::sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut())
        };

        if ret != ffi::SQLITE_OK {
            unsafe { ffi::sqlite3_close(db) };
            return Err(JsValue::from_str("Failed to prepare statement"));
        }

        while unsafe { ffi::sqlite3_step(stmt) } == ffi::SQLITE_ROW {
            let mut row = Vec::new();
            let cols = unsafe { ffi::sqlite3_column_count(stmt) };

            for i in 0..cols {
                let value = unsafe {
                    let text = ffi::sqlite3_column_text(stmt, i);
                    if text.is_null() {
                        JsValue::NULL
                    } else {
                        let str_val = std::ffi::CStr::from_ptr(text as *mut i8)
                            .to_str()
                            .unwrap_or("invalid utf8");
                        JsValue::from_str(str_val)
                    }
                };
                row.push(value);
            }
            results.push(js_sys::Array::from_iter(row));
        }

        unsafe {
            ffi::sqlite3_finalize(stmt);
            ffi::sqlite3_close(db);
        }

        Ok(js_sys::Array::from_iter(results).into())
    }
}

#[wasm_bindgen]
pub async fn main() -> Result<(), JsValue> {
    web_sys::console::log_1(&JsValue::from_str("Setting up worker..."));
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let scope_clone = scope.clone();

    // Specify the type for Option<Database>
    let db: Rc<RefCell<Option<Database>>> = Rc::new(RefCell::new(None));
    let db_clone = db.clone();

    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Some(msg) = e.data().as_string() {
            let msg = msg.to_string();
            let scope_clone = scope_clone.clone();
            wasm_bindgen_futures::spawn_local(async move {
                web_sys::console::log_1(&format!("Worker received: {}", msg).into());

                let result = if msg.starts_with("QUERY:") {
                    match Database::new("app.db").await {
                        Ok(db) => db.query(&msg[6..]),
                        Err(e) => Err(e),
                    }
                } else {
                    match Database::new("app.db").await {
                        Ok(db) => db.execute(&msg).map(|_| JsValue::NULL),
                        Err(e) => Err(e),
                    }
                };
                match result {
                    Ok(val) => scope_clone.post_message(&val),
                    Err(e) => scope_clone.post_message(&e),
                }
                .unwrap();
            });
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    scope.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    Ok(())
}
