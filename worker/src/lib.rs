use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum TabMessage {
    Register { tab_id: String },
    CheckLeader { tab_id: String },
    LeaderResponse { is_leader: bool },
    QueryLeader { from_tab_id: String },
    LeaderDataResponse { data: String },
    Disconnect { tab_id: String },
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
        let was_leader = self.get_leader().map(|id| id == tab_id).unwrap_or(false);
        self.tabs.retain(|id| id != tab_id);
        self.ports.remove(tab_id);

        if was_leader {
            web_sys::console::log_1(&"Leader disconnected, notifying new leader".into());
            if let Some(new_leader) = self.get_leader() {
                if let Some(leader_port) = self.ports.get(new_leader) {
                    let response = TabMessage::LeaderResponse { is_leader: true };
                    leader_port
                        .post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                        .unwrap();
                }
            }
        }
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
    match msg {
        TabMessage::Register { tab_id } => {
            web_sys::console::log_1(&format!("Handling register for tab: {}", tab_id).into());
            TAB_STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.register_tab(tab_id.clone(), port.clone());

                let is_leader = state.get_leader().map(|id| id == &tab_id).unwrap_or(false);

                web_sys::console::log_1(&format!("Tab {} is_leader: {}", tab_id, is_leader).into());

                let response = TabMessage::LeaderResponse { is_leader };
                port.post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                    .unwrap();
            });
        }
        TabMessage::CheckLeader { tab_id } => {
            TAB_STATE.with(|state| {
                let is_leader = state
                    .borrow()
                    .get_leader()
                    .map(|id| id == &tab_id)
                    .unwrap_or(false);
                let response = TabMessage::LeaderResponse { is_leader };
                port.post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                    .unwrap();
            });
        }
        TabMessage::QueryLeader { ref from_tab_id } => {
            TAB_STATE.with(|state| {
                if let Some(leader_id) = state.borrow().get_leader() {
                    if let Some(leader_port) = state.borrow().ports.get(leader_id) {
                        leader_port
                            .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
                            .unwrap();
                    }
                }
            });
        }
        TabMessage::LeaderDataResponse { ref data } => {
            TAB_STATE.with(|state| {
                // Just send to all tabs for now - the leader will know who to respond to
                for port in state.borrow().ports.values() {
                    port.post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
                        .unwrap();
                }
            });
        }
        TabMessage::Disconnect { tab_id } => {
            TAB_STATE.with(|state| {
                state.borrow_mut().remove_tab(&tab_id);
            });
        }
        _ => {}
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    web_sys::console::log_1(&"SharedWorker WASM initialized".into());
}
