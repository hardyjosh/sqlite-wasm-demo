use crate::{execute_sql, ffi, query_users};
use sqlite_wasm_rs::export::install_opfs_sahpool;
use std::ffi::CString;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
async fn test_opfs_example() {
    // Install OPFS VFS and set it as the default
    install_opfs_sahpool(None, true).await.unwrap();

    // First "tab" - create and populate the database
    {
        let mut db1 = std::ptr::null_mut();
        let filename = CString::new("my_database.db").unwrap();
        let ret = unsafe {
            ffi::sqlite3_open_v2(
                filename.as_ptr(),
                &mut db1 as *mut _,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                std::ptr::null(),
            )
        };
        assert_eq!(ffi::SQLITE_OK, ret);

        // Create table and insert data
        execute_sql(
            db1,
            "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)",
        )
        .unwrap();
        execute_sql(
            db1,
            "INSERT INTO users (name, age) VALUES ('Alice', 30), ('Bob', 25)",
        )
        .unwrap();

        unsafe {
            ffi::sqlite3_close(db1);
        }
    }

    // Second "tab" - verify data persists
    {
        let mut db2 = std::ptr::null_mut();
        let filename = CString::new("my_database.db").unwrap();
        let ret = unsafe {
            ffi::sqlite3_open_v2(
                filename.as_ptr(),
                &mut db2 as *mut _,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
                std::ptr::null(),
            )
        };
        assert_eq!(ffi::SQLITE_OK, ret);

        let users = query_users(db2).unwrap();
        assert_eq!(users.len(), 2);

        unsafe {
            ffi::sqlite3_close(db2);
        }
    }
}

#[wasm_bindgen_test]
async fn test_simultaneous_connections() {
    // Install OPFS VFS and set it as the default
    install_opfs_sahpool(None, true).await.unwrap();

    // Create first connection and set up database
    let mut db1 = std::ptr::null_mut();
    let filename = CString::new("simultaneous_test.db").unwrap();
    let ret = unsafe {
        ffi::sqlite3_open_v2(
            filename.as_ptr(),
            &mut db1 as *mut _,
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
            std::ptr::null(),
        )
    };
    assert_eq!(ffi::SQLITE_OK, ret);

    // Create table
    execute_sql(
        db1,
        "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY, value TEXT)",
    )
    .unwrap();

    // Create second connection to same database
    let mut db2 = std::ptr::null_mut();
    let ret = unsafe {
        ffi::sqlite3_open_v2(
            filename.as_ptr(),
            &mut db2 as *mut _,
            ffi::SQLITE_OPEN_READWRITE,
            std::ptr::null(),
        )
    };
    assert_eq!(ffi::SQLITE_OK, ret);

    // Insert data through first connection
    execute_sql(db1, "INSERT INTO test (value) VALUES ('from db1')").unwrap();

    // Read data through second connection
    let mut stmt = std::ptr::null_mut();
    let sql = CString::new("SELECT value FROM test").unwrap();
    unsafe {
        ffi::sqlite3_prepare_v2(db2, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut());
    }

    let mut found = false;
    while unsafe { ffi::sqlite3_step(stmt) } == ffi::SQLITE_ROW {
        let value = unsafe {
            std::ffi::CStr::from_ptr(ffi::sqlite3_column_text(stmt, 0).cast())
                .to_str()
                .unwrap()
        };
        assert_eq!(value, "from db1");
        found = true;
    }
    assert!(found, "Should have found the inserted data");

    unsafe {
        ffi::sqlite3_finalize(stmt);
        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

#[wasm_bindgen_test]
async fn test_concurrent_writes() {
    // Install OPFS VFS and set it as the default
    install_opfs_sahpool(None, true).await.unwrap();

    // Create first connection
    let mut db1 = std::ptr::null_mut();
    let filename = CString::new("concurrent_test.db").unwrap();
    let ret = unsafe {
        ffi::sqlite3_open_v2(
            filename.as_ptr(),
            &mut db1 as *mut _,
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
            std::ptr::null(),
        )
    };
    assert_eq!(ffi::SQLITE_OK, ret);

    // Create table
    execute_sql(
        db1,
        "CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY, value TEXT)",
    )
    .unwrap();

    // Create second connection
    let mut db2 = std::ptr::null_mut();
    let ret = unsafe {
        ffi::sqlite3_open_v2(
            filename.as_ptr(),
            &mut db2 as *mut _,
            ffi::SQLITE_OPEN_READWRITE,
            std::ptr::null(),
        )
    };
    assert_eq!(ffi::SQLITE_OK, ret);

    // Write from both connections
    execute_sql(db1, "INSERT INTO test (value) VALUES ('from db1')").unwrap();
    execute_sql(db2, "INSERT INTO test (value) VALUES ('from db2')").unwrap();

    // Verify both writes succeeded
    let mut stmt = std::ptr::null_mut();
    let sql = CString::new("SELECT value FROM test ORDER BY id").unwrap();
    unsafe {
        ffi::sqlite3_prepare_v2(db1, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut());
    }

    let mut values = Vec::new();
    while unsafe { ffi::sqlite3_step(stmt) } == ffi::SQLITE_ROW {
        let value = unsafe {
            std::ffi::CStr::from_ptr(ffi::sqlite3_column_text(stmt, 0).cast())
                .to_str()
                .unwrap()
                .to_string()
        };
        values.push(value);
    }

    assert_eq!(values.len(), 2);
    assert_eq!(values[0], "from db1");
    assert_eq!(values[1], "from db2");

    unsafe {
        ffi::sqlite3_finalize(stmt);
        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}
