use sqlite_wasm_rs::export::{self as ffi};
use std::ffi::CString;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::WorkerGlobalScope;

mod connection_pool;
pub mod coordinator;
mod coordinator_tests;
mod opfs_tests;
mod worker_tests;

// Helper struct to represent a user
#[derive(Debug)]
#[allow(dead_code)] // Used in tests
pub(crate) struct User {
    id: i32,
    name: String,
    age: i32,
}

// Common utilities for database operations
pub(crate) fn execute_sql(db: *mut ffi::sqlite3, sql: &str) -> Result<(), i32> {
    let sql = CString::new(sql).unwrap();
    let mut err_msg = std::ptr::null_mut();

    let ret =
        unsafe { ffi::sqlite3_exec(db, sql.as_ptr(), None, std::ptr::null_mut(), &mut err_msg) };

    if ret == ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(ret)
    }
}

pub(crate) fn query_users(db: *mut ffi::sqlite3) -> Result<Vec<User>, i32> {
    let sql = CString::new("SELECT * FROM users").unwrap();
    let mut stmt = std::ptr::null_mut();
    let mut users = Vec::new();

    let ret =
        unsafe { ffi::sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) };

    if ret != ffi::SQLITE_OK {
        return Err(ret);
    }

    while unsafe { ffi::sqlite3_step(stmt) } == ffi::SQLITE_ROW {
        let user = unsafe {
            User {
                id: ffi::sqlite3_column_int(stmt, 0),
                name: std::ffi::CStr::from_ptr(ffi::sqlite3_column_text(stmt, 1).cast())
                    .to_str()
                    .unwrap()
                    .to_string(),
                age: ffi::sqlite3_column_int(stmt, 2),
            }
        };
        users.push(user);
    }

    unsafe {
        ffi::sqlite3_finalize(stmt);
    }

    Ok(users)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub(crate) fn log(s: &str);
}

// Add these new types
#[derive(Debug)]
pub struct SQLQuery {
    sql: String,
}

impl SQLQuery {
    pub fn new(sql: &str) -> Self {
        Self {
            sql: sql.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct QueryResult {
    pub routed_through: String,
    pub data: Vec<Vec<String>>,
}

#[derive(Debug)]
pub struct RecoveryStatus {
    pub recovered: bool,
    pub data_consistent: bool,
    pub error_details: Option<String>,
}

#[derive(Debug)]
pub struct ResourceMetrics {
    pub active_connections: usize,
    pub pending_operations: usize,
    pub memory_usage: usize,
    pub storage_usage: usize,
}

pub fn get_time_ms() -> f64 {
    // Try window context first
    if let Some(window) = web_sys::window() {
        if let Some(perf) = window.performance() {
            return perf.now();
        }
    }

    // Try worker context
    if let Ok(scope) = js_sys::global().dyn_into::<WorkerGlobalScope>() {
        if let Some(perf) = scope.performance() {
            return perf.now();
        }
    }

    // Fallback to Date.now()
    js_sys::Date::now()
}

#[derive(Debug, Clone)]
pub struct PendingOperation {
    pub id: String,
    pub sql: String,
    pub tab_id: String,
    pub timestamp: f64,
}

#[derive(Debug, Clone)]
pub struct TransactionState {
    pub tab_id: String,
    pub operations: Vec<String>,
    pub start_time: f64,
}

pub use connection_pool::ConnectionPool;
pub use coordinator::MockCoordinator;
