use crate::get_time_ms;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct Connection {
    id: String,
    initialized: bool,
    last_used: f64,
    reused: bool,
}

pub struct ConnectionPool {
    connections: Arc<Mutex<HashMap<String, Connection>>>,
    max_size: usize,
}

impl ConnectionPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            max_size,
        }
    }

    pub async fn acquire(&self) -> Result<Connection, String> {
        let mut connections = self.connections.lock().unwrap();

        // Try to find an available connection
        if let Some(conn) = connections.values_mut().find(|c| !c.initialized) {
            conn.initialized = true;
            conn.last_used = get_time_ms();
            conn.reused = true;
            return Ok(conn.clone());
        }

        // Create new if under max size
        if connections.len() < self.max_size {
            let conn = Connection {
                id: format!("conn-{}", connections.len()),
                initialized: true,
                last_used: get_time_ms(),
                reused: false,
            };
            connections.insert(conn.id.clone(), conn.clone());
            Ok(conn)
        } else {
            Err("Pool exhausted".to_string())
        }
    }

    pub async fn release(&self, conn: Connection) {
        let mut connections = self.connections.lock().unwrap();
        if let Some(existing) = connections.get_mut(&conn.id) {
            existing.initialized = false;
            existing.last_used = get_time_ms();
        }
    }

    pub fn available_connections(&self) -> usize {
        self.max_size - self.connections.lock().unwrap().len()
    }

    pub async fn cleanup_stale_connections(&self) {
        let mut connections = self.connections.lock().unwrap();
        let now = get_time_ms();
        let stale_threshold = 300_000.0; // 5 minutes in milliseconds

        connections.retain(|_, conn| (now - conn.last_used) < stale_threshold);
    }

    pub fn stale_connections(&self) -> usize {
        let connections = self.connections.lock().unwrap();
        let now = get_time_ms();
        let stale_threshold = 300_000.0;

        connections
            .values()
            .filter(|conn| (now - conn.last_used) > stale_threshold)
            .count()
    }
}

impl Clone for Connection {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            initialized: self.initialized,
            last_used: self.last_used,
            reused: self.reused,
        }
    }
}

impl Connection {
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn is_reused(&self) -> bool {
        self.reused
    }
}
