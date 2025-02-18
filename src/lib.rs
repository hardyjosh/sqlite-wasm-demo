use futures::channel::oneshot;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use web_sys::{console, MessagePort, SharedWorker};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum TabMessage {
    Register { tab_id: String },
    CheckLeader { tab_id: String },
    LeaderResponse { is_leader: bool },
    QueryLeader { from_tab_id: String },
    LeaderDataResponse { data: String, from_tab_id: String },
    Disconnect { tab_id: String },
}

#[wasm_bindgen]
pub struct TabManager {
    port: MessagePort,
    tab_id: String,
    leader_data: Rc<RefCell<String>>,
    response_sender: Rc<RefCell<Option<oneshot::Sender<String>>>>,
    leader_callback: Rc<RefCell<Option<js_sys::Function>>>,
}

#[wasm_bindgen]
impl TabManager {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<TabManager, JsValue> {
        let tab_id = Uuid::new_v4().to_string();
        let leader_data = Rc::new(RefCell::new(String::new()));
        let leader_callback = Rc::new(RefCell::new(None::<js_sys::Function>));

        let worker = match SharedWorker::new("/pkg/worker/worker.js") {
            Ok(w) => w,
            Err(e) => return Err(e),
        };
        let port = worker.port();
        port.start();

        // Set up message handler
        let port_clone = port.clone();
        let leader_data_clone = leader_data.clone();
        let tab_id_clone = tab_id.clone();
        let response_sender: Rc<RefCell<Option<oneshot::Sender<String>>>> =
            Rc::new(RefCell::new(None));
        let response_sender_clone = response_sender.clone();
        let leader_callback_clone = leader_callback.clone();

        let onmessage =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                console::log_1(&JsValue::from_str("Tab received message"));
                if let Ok(msg) = serde_wasm_bindgen::from_value::<TabMessage>(e.data()) {
                    console::log_1(&JsValue::from_str(&format!("Tab message: {:?}", msg)));
                    match msg {
                        // Handle leader queries
                        TabMessage::QueryLeader { from_tab_id } => {
                            console::log_1(&JsValue::from_str("QueryLeader received by tab"));
                            let response = TabMessage::LeaderDataResponse {
                                data: leader_data_clone.borrow().clone(),
                                from_tab_id,
                            };
                            console::log_1(&JsValue::from_str(&format!(
                                "Leader sending response: {:?}",
                                response
                            )));
                            port_clone
                                .post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                                .unwrap();
                        }
                        // Handle leader responses
                        TabMessage::LeaderDataResponse { data, from_tab_id } => {
                            if from_tab_id == tab_id_clone {
                                if let Some(sender) = response_sender_clone.borrow_mut().take() {
                                    sender.send(data).unwrap();
                                }
                            }
                        }
                        // Handle leader status responses
                        TabMessage::LeaderResponse { is_leader } => {
                            if let Some(callback) = leader_callback_clone.borrow().as_ref() {
                                callback
                                    .call1(&JsValue::NULL, &JsValue::from_bool(is_leader))
                                    .unwrap();
                            }
                        }
                        _ => {}
                    }
                }
            })
                as Box<dyn FnMut(web_sys::MessageEvent)>);

        port.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        // Set up disconnect handler
        let port_clone = port.clone();
        let tab_id_clone = tab_id.clone();
        let onbeforeunload =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
                let msg = TabMessage::Disconnect {
                    tab_id: tab_id_clone.clone(),
                };
                port_clone
                    .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
                    .unwrap();
            }) as Box<dyn FnMut(web_sys::Event)>);

        web_sys::window()
            .unwrap()
            .set_onbeforeunload(Some(onbeforeunload.as_ref().unchecked_ref()));
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
            leader_data,
            response_sender,
            leader_callback,
        })
    }

    #[wasm_bindgen]
    pub fn check_leader(&self, callback: js_sys::Function) {
        *self.leader_callback.borrow_mut() = Some(callback);

        let msg = TabMessage::CheckLeader {
            tab_id: self.tab_id.clone(),
        };

        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
            .unwrap();
    }

    #[wasm_bindgen]
    pub fn query_leader(&self) -> js_sys::Promise {
        let (sender, receiver) = oneshot::channel();
        *self.response_sender.borrow_mut() = Some(sender);

        let msg = TabMessage::QueryLeader {
            from_tab_id: self.tab_id.clone(),
        };

        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
            .unwrap();

        future_to_promise(async move {
            let data = receiver.await.unwrap();
            Ok(JsValue::from_str(&data))
        })
    }

    #[wasm_bindgen]
    pub fn get_tab_id(&self) -> String {
        self.tab_id.clone()
    }

    #[wasm_bindgen]
    pub fn save_data(&mut self, data: String) {
        *self.leader_data.borrow_mut() = data;
    }

    #[wasm_bindgen]
    pub fn get_data(&self) -> String {
        self.leader_data.borrow().clone()
    }

    #[wasm_bindgen]
    pub fn send_leader_response(&self, from_tab_id: String) {
        let msg = TabMessage::LeaderDataResponse {
            data: self.leader_data.borrow().clone(),
            from_tab_id,
        };
        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
            .unwrap();
    }

    #[wasm_bindgen]
    pub fn port(&self) -> MessagePort {
        self.port.clone()
    }
}
