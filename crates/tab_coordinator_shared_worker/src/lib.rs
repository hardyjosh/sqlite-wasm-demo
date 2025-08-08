use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum TabMessage {
    Register {
        tab_id: String,
    },
    CheckLeader {
        tab_id: String,
    },
    LeaderResponse {
        is_leader: bool,
    },
    QueryLeader {
        from_tab_id: String,
    },
    LeaderDataResponse {
        data: String,
        from_tab_id: String,
    },
    Disconnect {
        tab_id: String,
    },
    ExecuteQuery {
        sql: String,
        from_tab_id: String,
    },
    QueryResponse {
        results: Vec<Vec<String>>,
        from_tab_id: String,
        error: Option<String>,
    },
    Heartbeat {
        tab_id: String,
    },
    ClaimLeadership {
        tab_id: String,
    },
    FreshWorkerNotification {
        is_fresh: bool,
    },
}

struct TabInfo {
    port: Rc<web_sys::MessagePort>,
    last_heartbeat: f64,
}

struct TabState {
    tabs_info: HashMap<String, TabInfo>,
    tabs_order: VecDeque<String>,
}

impl TabState {
    fn new() -> Self {
        Self {
            tabs_info: HashMap::new(),
            tabs_order: VecDeque::new(),
        }
    }

    fn get_leader(&self) -> Option<&String> {
        web_sys::console::log_1(&format!("Current tabs: {:?}", self.tabs_order).into());
        self.tabs_order.front()
    }

    fn register_tab(&mut self, tab_id: String, port: Rc<web_sys::MessagePort>) {
        web_sys::console::log_1(&format!("Registering tab: {}", tab_id).into());

        let now = js_sys::Date::now();
        let tab_info = TabInfo {
            port: port.clone(),
            last_heartbeat: now,
        };

        if !self.tabs_order.contains(&tab_id) {
            self.tabs_order.push_back(tab_id.clone());
            web_sys::console::log_1(
                &format!("Added new tab. Tabs are now: {:?}", self.tabs_order).into(),
            );
        } else {
            web_sys::console::log_1(&format!("Tab {} already registered", tab_id).into());
        }
        self.tabs_info.insert(tab_id, tab_info);
    }

    fn remove_tab(&mut self, tab_id: &str) {
        web_sys::console::log_1(&format!("Removing tab: {}", tab_id).into());
        self.tabs_order.retain(|id| id != tab_id);
        self.tabs_info.remove(tab_id);
    }

    fn update_heartbeat(&mut self, tab_id: &str) {
        if let Some(tab_info) = self.tabs_info.get_mut(tab_id) {
            tab_info.last_heartbeat = js_sys::Date::now();
            web_sys::console::log_1(&format!("Updated heartbeat for tab: {}", tab_id).into());
        }
    }

    fn cleanup_stale_tabs(&mut self) {
        let now = js_sys::Date::now();
        let timeout = 6000.0; // 6 seconds timeout (very aggressive)

        let stale_tabs: Vec<String> = self
            .tabs_info
            .iter()
            .filter(|(_, info)| {
                let is_stale = now - info.last_heartbeat > timeout;
                if is_stale {
                    web_sys::console::log_1(
                        &format!(
                            "Tab {} is stale: last_heartbeat={}, now={}, diff={}",
                            info.port.as_ref() as *const _ as usize,
                            info.last_heartbeat,
                            now,
                            now - info.last_heartbeat
                        )
                        .into(),
                    );
                }
                is_stale
            })
            .map(|(id, _)| id.clone())
            .collect();

        if !stale_tabs.is_empty() {
            web_sys::console::log_1(
                &format!(
                    "🗑️ Removing {} stale tabs: {:?}",
                    stale_tabs.len(),
                    stale_tabs
                )
                .into(),
            );
        }

        for tab_id in stale_tabs {
            web_sys::console::log_1(&format!("Removing stale tab: {}", tab_id).into());
            self.remove_tab(&tab_id);
        }
    }

    fn get_port(&self, tab_id: &str) -> Option<&Rc<web_sys::MessagePort>> {
        self.tabs_info.get(tab_id).map(|info| &info.port)
    }
}

thread_local! {
    static TAB_STATE: std::cell::RefCell<TabState> = std::cell::RefCell::new(TabState::new());
    static IS_FRESH_WORKER: std::cell::RefCell<bool> = std::cell::RefCell::new(true);
}

#[wasm_bindgen]
pub fn handle_connect(e: MessageEvent) {
    web_sys::console::log_1(&"Got connect event from JS".into());

    // Check if this is a fresh worker instance
    let is_fresh = IS_FRESH_WORKER.with(|fresh| {
        let is_fresh = *fresh.borrow();
        if is_fresh {
            web_sys::console::log_1(&"🆕 This is a FRESH SharedWorker instance!".into());
            *fresh.borrow_mut() = false; // Mark as no longer fresh
        } else {
            web_sys::console::log_1(&"♻️ This is an EXISTING SharedWorker instance".into());
        }
        is_fresh
    });

    let ports = js_sys::Array::from(&e.ports());
    let port = Rc::new(ports.get(0).dyn_into::<web_sys::MessagePort>().unwrap());

    port.start();

    // Immediately notify the connecting tab if this is a fresh worker
    if is_fresh {
        web_sys::console::log_1(&"📢 Notifying tab that this is a fresh worker".into());
        let fresh_notification = TabMessage::FreshWorkerNotification { is_fresh: true };
        if let Ok(msg) = serde_wasm_bindgen::to_value(&fresh_notification) {
            let _ = port.post_message(&msg);
        }

        // Since this is a fresh worker, any tab that connects should immediately become the leader
        // We don't need to wait for registration - grant leadership preemptively
        web_sys::console::log_1(
            &"👑 Fresh worker detected - first connecting tab will become immediate leader".into(),
        );
    }

    let port_clone = port.clone();
    let port_message_handler = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(msg) = serde_wasm_bindgen::from_value::<TabMessage>(e.data()) {
            handle_message(msg, port_clone.clone());
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    port.set_onmessage(Some(port_message_handler.as_ref().unchecked_ref()));
    port_message_handler.forget();
}

fn handle_message(msg: TabMessage, port: Rc<web_sys::MessagePort>) {
    web_sys::console::log_1(&"========================================".into());
    web_sys::console::log_1(&format!("📨 Received message in shared worker: {:?}", msg).into());
    web_sys::console::log_1(&"========================================".into());

    match msg {
        TabMessage::Register { tab_id } => {
            web_sys::console::log_1(&format!("📝 Registering tab: {}", tab_id).into());

            // Check if worker was recently fresh (for logging purposes)
            let was_fresh_recently = IS_FRESH_WORKER.with(|fresh| !*fresh.borrow());
            if was_fresh_recently {
                web_sys::console::log_1(
                    &"ℹ️ Note: This worker was fresh when first connection was made".into(),
                );
            }

            TAB_STATE.with(|state| {
                let mut state = state.borrow_mut();

                // IMMEDIATE stale cleanup before registration to ensure accurate tab count
                web_sys::console::log_1(
                    &"🧹 Performing immediate stale cleanup before registration".into(),
                );
                state.cleanup_stale_tabs();

                // Remove this tab if it was already registered (handles refresh)
                state.remove_tab(&tab_id);

                // Register the tab
                state.register_tab(tab_id.clone(), port.clone());

                // ENHANCED RULE: Grant leadership if this is the only tab OR if no other active tabs exist
                let total_tabs = state.tabs_order.len();
                let is_leader = total_tabs == 1;

                // Special case: If this was a fresh worker, the first tab should always be leader
                if was_fresh_recently && total_tabs == 1 {
                    web_sys::console::log_1(
                        &"🆕 First tab connecting to fresh worker - immediate leadership granted"
                            .into(),
                    );
                }

                web_sys::console::log_1(
                    &format!(
                        "👑 Tab {} is_leader: {} (total_tabs: {}, fresh_worker: {})",
                        tab_id, is_leader, total_tabs, was_fresh_recently
                    )
                    .into(),
                );
                web_sys::console::log_1(&format!("📊 All tabs: {:?}", state.tabs_order).into());

                // Send leadership status immediately back to registering tab
                let response = TabMessage::LeaderResponse { is_leader };
                port.post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                    .unwrap();
            });
        }
        TabMessage::CheckLeader { tab_id } => {
            // Get current leader status from TAB_STATE
            TAB_STATE.with(|state| {
                let state = state.borrow();
                let is_leader = state
                    .get_leader()
                    .map(|leader_id| leader_id == &tab_id)
                    .unwrap_or(false);

                web_sys::console::log_1(&format!("Tab {} is_leader: {}", tab_id, is_leader).into());

                let response = TabMessage::LeaderResponse { is_leader };
                port.post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                    .unwrap();
            });
        }
        TabMessage::Heartbeat { tab_id } => {
            TAB_STATE.with(|state| {
                let mut state = state.borrow_mut();
                // Clean up stale tabs first
                state.cleanup_stale_tabs();
                // Update heartbeat for this tab
                state.update_heartbeat(&tab_id);
            });
        }
        TabMessage::ClaimLeadership { tab_id } => {
            web_sys::console::log_1(&format!("🚩 Tab {} claiming leadership", tab_id).into());
            TAB_STATE.with(|state| {
                let mut state = state.borrow_mut();
                
                // Clean up stale tabs first to get accurate count
                state.cleanup_stale_tabs();
                
                let total_tabs = state.tabs_order.len();
                let tab_exists = state.tabs_order.contains(&tab_id);
                let current_leader = state.get_leader().cloned();
                
                // AGGRESSIVE leadership granting logic for refresh scenarios:
                // 1. If this is the only tab, always grant leadership
                // 2. If there's no current leader, grant leadership to the claimant
                // 3. If the current leader doesn't exist in tabs (stale), grant leadership
                let should_grant_leadership = if total_tabs == 1 && tab_exists {
                    web_sys::console::log_1(&"✓ Only tab remaining - granting leadership".into());
                    true
                } else if current_leader.is_none() {
                    web_sys::console::log_1(&"✓ No current leader - granting leadership".into());
                    true
                } else if let Some(leader_id) = &current_leader {
                    if !state.tabs_order.contains(leader_id) {
                        web_sys::console::log_1(&format!("✓ Current leader {} is stale - granting leadership to {}", leader_id, tab_id).into());
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                
                web_sys::console::log_1(&format!("🔍 Leadership claim analysis: total_tabs={}, tab_exists={}, current_leader={:?}, should_grant={}", 
                    total_tabs, tab_exists, current_leader, should_grant_leadership).into());
                
                if should_grant_leadership {
                    web_sys::console::log_1(&format!("✅ Granting leadership to tab {}", tab_id).into());
                    
                    // Make sure this tab is at the front of the queue (leader position)
                    if let Some(pos) = state.tabs_order.iter().position(|x| x == &tab_id) {
                        let tab_id_moved = state.tabs_order.remove(pos).unwrap();
                        state.tabs_order.push_front(tab_id_moved);
                    }
                    
                    let response = TabMessage::LeaderResponse { is_leader: true };
                    if let Some(port) = state.get_port(&tab_id) {
                        port.post_message(&serde_wasm_bindgen::to_value(&response).unwrap()).unwrap();
                    }
                } else {
                    web_sys::console::log_1(&format!("❌ Denying leadership claim from tab {} - active leader exists", tab_id).into());
                    let response = TabMessage::LeaderResponse { is_leader: false };
                    if let Some(port) = state.get_port(&tab_id) {
                        port.post_message(&serde_wasm_bindgen::to_value(&response).unwrap()).unwrap();
                    }
                }
            });
        }
        TabMessage::QueryLeader { from_tab_id } => {
            web_sys::console::log_1(&"=== QUERY LEADER FLOW START ===".into());
            web_sys::console::log_1(&format!("1. Received query from tab: {}", from_tab_id).into());
            TAB_STATE.with(|state| {
                let state = state.borrow();
                web_sys::console::log_1(
                    &format!("2. Current tabs in state: {:?}", state.tabs_order).into(),
                );
                if let Some(leader_id) = state.get_leader() {
                    web_sys::console::log_1(&format!("3. Found leader tab: {}", leader_id).into());
                    if let Some(leader_port) = state.get_port(leader_id) {
                        web_sys::console::log_1(
                            &format!(
                                "4. Forwarding query to leader {} from tab {}",
                                leader_id, from_tab_id
                            )
                            .into(),
                        );
                        let query = TabMessage::QueryLeader {
                            from_tab_id: from_tab_id.clone(),
                        };
                        match leader_port
                            .post_message(&serde_wasm_bindgen::to_value(&query).unwrap())
                        {
                            Ok(_) => web_sys::console::log_1(
                                &"5. ✅ Successfully forwarded query to leader".into(),
                            ),
                            Err(e) => web_sys::console::log_1(
                                &format!("5. ❌ Failed to forward query: {:?}", e).into(),
                            ),
                        }
                    } else {
                        web_sys::console::log_1(
                            &format!("❌ ERROR: Found leader {} but no port for it!", leader_id)
                                .into(),
                        );
                    }
                } else {
                    web_sys::console::log_1(&"❌ ERROR: No leader found in tab state!".into());
                }
            });
            web_sys::console::log_1(&"=== QUERY LEADER FLOW END ===".into());
        }
        TabMessage::LeaderDataResponse { data, from_tab_id } => {
            web_sys::console::log_1(&"=== LEADER RESPONSE FLOW START ===".into());
            web_sys::console::log_1(
                &format!("1. Got response from leader {}: {:?}", from_tab_id, data).into(),
            );
            let data = data.clone();
            let from_tab_id = from_tab_id.clone();
            TAB_STATE.with(|state| {
                web_sys::console::log_1(
                    &format!("2. Looking for port for tab: {}", from_tab_id).into(),
                );
                if let Some(requester_port) = state.borrow().get_port(&from_tab_id) {
                    web_sys::console::log_1(&"3. Found requester's port, sending response".into());
                    let response = TabMessage::LeaderDataResponse { data, from_tab_id };
                    match requester_port
                        .post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                    {
                        Ok(_) => web_sys::console::log_1(
                            &"4. ✅ Successfully sent response to requester".into(),
                        ),
                        Err(e) => web_sys::console::log_1(
                            &format!("4. ❌ Failed to send response: {:?}", e).into(),
                        ),
                    }
                } else {
                    web_sys::console::log_1(
                        &format!("❌ ERROR: No port found for tab {}", from_tab_id).into(),
                    );
                }
            });
            web_sys::console::log_1(&"=== LEADER RESPONSE FLOW END ===".into());
        }
        TabMessage::Disconnect { tab_id } => {
            web_sys::console::log_1(&format!("🔌 Disconnecting tab: {}", tab_id).into());
            TAB_STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.remove_tab(&tab_id);
                web_sys::console::log_1(
                    &format!("📊 Tabs after disconnect: {:?}", state.tabs_order).into(),
                );
            });
        }
        TabMessage::ExecuteQuery { sql, from_tab_id } => {
            web_sys::console::log_1(&"=== EXECUTE QUERY FLOW START ===".into());
            web_sys::console::log_1(
                &format!(
                    "1. Received query request: {} from tab: {}",
                    sql, from_tab_id
                )
                .into(),
            );
            let requester_id = from_tab_id.clone();
            TAB_STATE.with(|state| {
                let state = state.borrow();
                if let Some(leader_id) = state.get_leader() {
                    if let Some(leader_port) = state.get_port(leader_id) {
                        web_sys::console::log_1(
                            &format!("2. Forwarding query to leader {}", leader_id).into(),
                        );
                        let query = TabMessage::ExecuteQuery {
                            sql: sql.clone(),
                            from_tab_id: requester_id,
                        };
                        match leader_port
                            .post_message(&serde_wasm_bindgen::to_value(&query).unwrap())
                        {
                            Ok(_) => web_sys::console::log_1(
                                &"3. ✅ Successfully forwarded query to leader".into(),
                            ),
                            Err(e) => web_sys::console::log_1(
                                &format!("3. ❌ Failed to forward query: {:?}", e).into(),
                            ),
                        }
                    }
                }
            });
        }
        TabMessage::QueryResponse {
            ref results,
            ref from_tab_id,
            ref error,
        } => {
            web_sys::console::log_1(&"=== QUERY RESPONSE FLOW START ===".into());
            web_sys::console::log_1(
                &format!(
                    "1. Got query response for tab {}: {:?} (error: {:?})",
                    from_tab_id, results, error
                )
                .into(),
            );
            TAB_STATE.with(|state| {
                let state = state.borrow();
                if let Some(requester_port) = state.get_port(from_tab_id) {
                    web_sys::console::log_1(&"2. Found requester's port, sending response".into());
                    match requester_port.post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
                    {
                        Ok(_) => web_sys::console::log_1(
                            &"3. ✅ Successfully sent query response to requester".into(),
                        ),
                        Err(e) => web_sys::console::log_1(
                            &format!("3. ❌ Failed to send query response: {:?}", e).into(),
                        ),
                    }
                }
            });
            web_sys::console::log_1(&"=== QUERY RESPONSE FLOW END ===".into());
        }
        _ => {}
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    web_sys::console::log_1(&"SharedWorker WASM initialized".into());
}
