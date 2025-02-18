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
}

struct TabState {
    ports: HashMap<String, Rc<web_sys::MessagePort>>,
    tabs: VecDeque<String>,
}

impl TabState {
    fn new() -> Self {
        Self {
            ports: HashMap::new(),
            tabs: VecDeque::new(),
        }
    }

    fn get_leader(&self) -> Option<&String> {
        web_sys::console::log_1(&format!("Current tabs: {:?}", self.tabs).into());
        self.tabs.front()
    }

    fn register_tab(&mut self, tab_id: String, port: Rc<web_sys::MessagePort>) {
        web_sys::console::log_1(&format!("Registering tab: {}", tab_id).into());
        if !self.tabs.contains(&tab_id) {
            self.tabs.push_back(tab_id.clone());
            web_sys::console::log_1(
                &format!("Added new tab. Tabs are now: {:?}", self.tabs).into(),
            );
        } else {
            web_sys::console::log_1(&format!("Tab {} already registered", tab_id).into());
        }
        self.ports.insert(tab_id, port);
    }

    fn remove_tab(&mut self, tab_id: &str) {
        web_sys::console::log_1(&format!("Removing tab: {}", tab_id).into());
        self.tabs.retain(|id| id != tab_id);
        self.ports.remove(tab_id);
    }
}

thread_local! {
    static TAB_STATE: std::cell::RefCell<TabState> = std::cell::RefCell::new(TabState::new());
}

#[wasm_bindgen]
pub fn handle_connect(e: MessageEvent) {
    web_sys::console::log_1(&"Got connect event from JS".into());
    let ports = js_sys::Array::from(&e.ports());
    let port = Rc::new(ports.get(0).dyn_into::<web_sys::MessagePort>().unwrap());

    port.start();

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
            TAB_STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.register_tab(tab_id.clone(), port.clone());
                let is_leader = state.tabs.len() == 1;
                web_sys::console::log_1(
                    &format!("👑 Tab {} is_leader: {}", tab_id, is_leader).into(),
                );
                web_sys::console::log_1(&format!("📊 Current tabs: {:?}", state.tabs).into());
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
        TabMessage::QueryLeader { from_tab_id } => {
            web_sys::console::log_1(&"=== QUERY LEADER FLOW START ===".into());
            web_sys::console::log_1(&format!("1. Received query from tab: {}", from_tab_id).into());
            TAB_STATE.with(|state| {
                let state = state.borrow();
                web_sys::console::log_1(
                    &format!("2. Current tabs in state: {:?}", state.tabs).into(),
                );
                if let Some(leader_id) = state.get_leader() {
                    web_sys::console::log_1(&format!("3. Found leader tab: {}", leader_id).into());
                    if let Some(leader_port) = state.ports.get(leader_id) {
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
                if let Some(requester_port) = state.borrow().ports.get(&from_tab_id) {
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
            TAB_STATE.with(|state| {
                state.borrow_mut().remove_tab(&tab_id);
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
                    if let Some(leader_port) = state.ports.get(leader_id) {
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
                if let Some(requester_port) = state.ports.get(from_tab_id) {
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
