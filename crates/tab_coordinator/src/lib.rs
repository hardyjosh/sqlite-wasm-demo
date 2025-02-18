use futures::channel::oneshot;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, JsFuture};
use web_sys::{console, MessagePort, SharedWorker};

#[derive(Serialize, Deserialize, Debug)]
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
    ExecuteQuery {
        sql: String,
        from_tab_id: String,
    },
    QueryResponse {
        results: Vec<Vec<String>>,
        from_tab_id: String,
        error: Option<String>,
    },
    Disconnect {
        tab_id: String,
    },
}

#[wasm_bindgen]
pub struct TabManager {
    port: MessagePort,
    tab_id: String,
    leader_data: Rc<RefCell<String>>,
    response_sender: Rc<RefCell<Option<oneshot::Sender<String>>>>,
    leader_callback: Rc<RefCell<Option<js_sys::Function>>>,
    query_response_sender: Rc<RefCell<Option<oneshot::Sender<Result<Vec<Vec<String>>, String>>>>>,
    worker: Rc<web_sys::Worker>,
}

#[wasm_bindgen]
impl TabManager {
    #[wasm_bindgen(constructor)]
    pub fn new(worker: web_sys::Worker) -> Result<TabManager, JsValue> {
        let tab_id = Uuid::new_v4().to_string();
        let leader_data = Rc::new(RefCell::new(String::new()));
        let leader_callback = Rc::new(RefCell::new(None::<js_sys::Function>));

        // Create the shared worker
        let shared_worker = SharedWorker::new("/pkg/worker/tab_coordinator_shared_worker.js")?;
        let port = shared_worker.port();
        port.start();

        // Use the provided SQLite worker
        let worker = Rc::new(worker);

        // Set up message handler
        let port_clone = port.clone();
        let leader_data_clone = leader_data.clone();
        let tab_id_clone = tab_id.clone();
        let response_sender: Rc<RefCell<Option<oneshot::Sender<String>>>> =
            Rc::new(RefCell::new(None));
        let response_sender_clone = response_sender.clone();
        let leader_callback_clone = leader_callback.clone();

        let port_message_handler = {
            // Create a struct to hold our shared state
            struct SharedState {
                response_sender: Rc<RefCell<Option<oneshot::Sender<String>>>>,
                leader_data: Rc<RefCell<String>>,
                port: MessagePort,
                tab_id: String,
                worker: Rc<web_sys::Worker>,
                query_response_sender:
                    Rc<RefCell<Option<oneshot::Sender<Result<Vec<Vec<String>>, String>>>>>,
            }

            let state = Rc::new(RefCell::new(SharedState {
                response_sender: response_sender.clone(),
                leader_data: leader_data.clone(),
                port: port.clone(),
                tab_id: tab_id.clone(),
                worker: worker.clone(),
                query_response_sender: Rc::new(RefCell::new(None)),
            }));

            Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                if let Ok(msg) = serde_wasm_bindgen::from_value::<TabMessage>(e.data()) {
                    web_sys::console::log_1(&JsValue::from_str(&format!("Tab received message")));
                    web_sys::console::log_1(&JsValue::from_str(&format!("Tab message: {:?}", msg)));

                    match msg {
                        TabMessage::LeaderResponse { is_leader } => {
                            let sender = {
                                let state = state.borrow();
                                let sender = state.response_sender.borrow_mut().take();
                                drop(state);
                                sender
                            };
                            if let Some(sender) = sender {
                                let _ = sender.send(is_leader.to_string());
                            }
                        }
                        TabMessage::QueryLeader { from_tab_id } => {
                            console::log_1(&JsValue::from_str("QueryLeader received by tab"));
                            let data = {
                                let state = state.borrow();
                                let data = state.leader_data.borrow().clone();
                                drop(state);
                                data
                            };
                            let response = TabMessage::LeaderDataResponse { data, from_tab_id };
                            let port = {
                                let state = state.borrow();
                                let port = state.port.clone();
                                drop(state);
                                port
                            };
                            port.post_message(&serde_wasm_bindgen::to_value(&response).unwrap())
                                .unwrap();
                        }
                        TabMessage::ExecuteQuery { sql, from_tab_id } => {
                            let state = state.borrow();
                            console::log_1(&JsValue::from_str("ExecuteQuery received by tab"));
                            let port = state.port.clone();
                            let tab_id = state.tab_id.clone();
                            let worker = state.worker.clone();
                            let original_requester = from_tab_id.clone();
                            let response_sender = state.response_sender.clone();

                            drop(state); // Explicitly drop the borrow before the async block

                            wasm_bindgen_futures::spawn_local(async move {
                                // Check if we're the leader
                                let msg = TabMessage::CheckLeader {
                                    tab_id: tab_id.clone(),
                                };
                                let (sender, receiver) = oneshot::channel();
                                *response_sender.borrow_mut() = Some(sender);

                                port.post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
                                    .unwrap();

                                let is_leader = receiver
                                    .await
                                    .map_err(|_| "Channel closed".to_string())
                                    .unwrap()
                                    == "true";

                                if !is_leader {
                                    let response = TabMessage::QueryResponse {
                                        results: vec![],
                                        from_tab_id: original_requester.clone(),
                                        error: Some("Only leader can execute queries".to_string()),
                                    };
                                    port.post_message(
                                        &serde_wasm_bindgen::to_value(&response).unwrap(),
                                    )
                                    .unwrap();
                                    return;
                                }

                                // We are the leader, execute the query in our SQLite worker
                                let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                                    let handler = move |e: web_sys::MessageEvent| {
                                        resolve.call1(&JsValue::NULL, &e.data()).unwrap();
                                    };
                                    let closure = Closure::once(handler);
                                    worker.set_onmessage(Some(closure.as_ref().unchecked_ref()));
                                    worker
                                        .post_message(&JsValue::from_str(&format!("QUERY:{}", sql)))
                                        .unwrap();
                                    closure.forget();
                                });

                                match JsFuture::from(promise).await {
                                    Ok(result) => {
                                        // Parse the result array from SQLite worker
                                        let results = js_sys::Array::from(&result);
                                        let mut parsed_results = Vec::new();

                                        for i in 0..results.length() {
                                            let row = results.get(i);
                                            let row_array = js_sys::Array::from(&row);
                                            let mut parsed_row = Vec::new();

                                            for j in 0..row_array.length() {
                                                let cell = row_array.get(j);
                                                if !cell.is_undefined() && !cell.is_null() {
                                                    parsed_row
                                                        .push(cell.as_string().unwrap_or_default());
                                                }
                                            }

                                            parsed_results.push(parsed_row);
                                        }

                                        // Send query results back through the coordination system
                                        let response = TabMessage::QueryResponse {
                                            results: parsed_results,
                                            from_tab_id: original_requester,
                                            error: None,
                                        };
                                        port.post_message(
                                            &serde_wasm_bindgen::to_value(&response).unwrap(),
                                        )
                                        .unwrap();
                                    }
                                    Err(e) => {
                                        let response = TabMessage::QueryResponse {
                                            results: vec![],
                                            from_tab_id: original_requester,
                                            error: Some(format!("Query error: {:?}", e)),
                                        };
                                        port.post_message(
                                            &serde_wasm_bindgen::to_value(&response).unwrap(),
                                        )
                                        .unwrap();
                                    }
                                }
                            });
                        }
                        TabMessage::QueryResponse { results, error, .. } => {
                            console::log_1(&JsValue::from_str("Received query response"));
                            let sender = {
                                let state = state.borrow();
                                let sender = state.query_response_sender.borrow_mut().take();
                                drop(state);
                                sender
                            };
                            if let Some(sender) = sender {
                                let result = match error {
                                    Some(err) => Err(err),
                                    None => Ok(results),
                                };
                                let _ = sender.send(result);
                            }
                        }
                        _ => {}
                    }
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>)
        };

        port.set_onmessage(Some(port_message_handler.as_ref().unchecked_ref()));
        port_message_handler.forget();

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
            query_response_sender: Rc::new(RefCell::new(None)),
            worker,
        })
    }

    #[wasm_bindgen]
    pub async fn check_leader(&self) -> Result<bool, JsValue> {
        web_sys::console::log_1(&JsValue::from_str("TabManager: checking leader..."));

        // Create a new channel specifically for this check_leader call
        let (sender, receiver) = oneshot::channel();
        *self.response_sender.borrow_mut() = Some(sender);

        let msg = TabMessage::CheckLeader {
            tab_id: self.tab_id.clone(),
        };

        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg)?)?;

        // Wait for response
        let response = receiver
            .await
            .map_err(|_| JsValue::from_str("Channel closed"))?;

        Ok(response == "true")
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

    pub async fn route_query(&self, sql: &str) -> Result<JsValue, JsValue> {
        let (sender, receiver) = oneshot::channel();
        *self.query_response_sender.borrow_mut() = Some(sender);

        let msg = TabMessage::ExecuteQuery {
            sql: sql.to_string(),
            from_tab_id: self.tab_id.clone(),
        };
        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg)?)?;

        let response = receiver
            .await
            .map_err(|_| JsValue::from_str("Channel closed"))??;

        Ok(serde_wasm_bindgen::to_value(&response)?)
    }
}
