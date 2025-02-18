use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{MessagePort, SharedWorker};

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

#[wasm_bindgen]
pub struct TabManager {
    port: MessagePort,
    tab_id: String,
    _onmessage: Closure<dyn FnMut(web_sys::MessageEvent)>,
}

#[wasm_bindgen]
impl TabManager {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<TabManager, JsValue> {
        let tab_id = Uuid::new_v4().to_string();
        web_sys::console::log_1(&format!("Creating new tab with ID: {}", tab_id).into());

        let worker = match SharedWorker::new("/pkg/worker/worker.js") {
            Ok(w) => w,
            Err(e) => {
                web_sys::console::log_2(&"Failed to load worker:".into(), &e);
                return Err(e);
            }
        };
        let port = worker.port();
        port.start();

        // Set up message handler first
        let port_clone = port.clone();
        let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
            if let Ok(msg) = serde_wasm_bindgen::from_value::<TabMessage>(e.data()) {
                match msg {
                    TabMessage::LeaderResponse { is_leader } => {
                        let window = web_sys::window().unwrap();
                        let document = window.document().unwrap();
                        if let Some(elem) = document.get_element_by_id("leader-status") {
                            elem.set_text_content(Some(if is_leader {
                                "You are the leader"
                            } else {
                                "You are not the leader"
                            }));
                        }
                        if is_leader {
                            if let Some(controls) = document.get_element_by_id("leader-controls") {
                                controls.set_attribute("style", "display: block").unwrap();
                            }
                        }
                    }
                    TabMessage::QueryLeader { from_tab_id } => {
                        // If we receive this message, we are the leader
                        let window = web_sys::window().unwrap();
                        let data = js_sys::Reflect::get(&window, &"leaderData".into())
                            .unwrap()
                            .as_string()
                            .unwrap_or_default();

                        let response = TabMessage::LeaderDataResponse { data };
                        port_clone
                            .post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                            .unwrap();
                    }
                    TabMessage::LeaderDataResponse { data } => {
                        let window = web_sys::window().unwrap();
                        let document = window.document().unwrap();
                        if let Some(elem) = document.get_element_by_id("leader-data") {
                            elem.set_text_content(Some(&format!("Leader data: {}", data)));
                        }
                    }
                    _ => {}
                }
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);

        port.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        // Set up disconnect handler
        let port_clone = port.clone();
        let tab_id_clone = tab_id.clone();
        let onbeforeunload = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let msg = TabMessage::Disconnect {
                tab_id: tab_id_clone.clone(),
            };
            port_clone
                .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
                .unwrap();
        }) as Box<dyn FnMut(web_sys::Event)>);

        // Add the beforeunload event listener to window
        web_sys::window()
            .unwrap()
            .set_onbeforeunload(Some(onbeforeunload.as_ref().unchecked_ref()));

        // Keep the closure alive
        onbeforeunload.forget();

        // Register this tab
        let register_msg = TabMessage::Register {
            tab_id: tab_id.clone(),
        };
        port.post_message(&serde_wasm_bindgen::to_value(&register_msg).unwrap())
            .unwrap();

        Ok(TabManager {
            port,
            tab_id,
            _onmessage: onmessage,
        })
    }

    #[wasm_bindgen]
    pub fn check_leader(&self) {
        let msg = TabMessage::CheckLeader {
            tab_id: self.tab_id.clone(),
        };
        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
            .unwrap();
    }

    #[wasm_bindgen]
    pub fn query_leader(&self) {
        let msg = TabMessage::QueryLeader {
            from_tab_id: self.tab_id.clone(),
        };
        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
            .unwrap();
    }

    #[wasm_bindgen]
    pub fn get_tab_id(&self) -> String {
        self.tab_id.clone()
    }

    #[wasm_bindgen]
    pub fn send_leader_response(&self, data: String, to_tab_id: String) {
        let msg = TabMessage::LeaderDataResponse { data };
        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
            .unwrap();
    }
}
