use base64::engine::Engine;
use futures::channel::oneshot;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, JsFuture};
use web_sys::{
    console, Blob, BlobPropertyBag, MessagePort, Request, Response, SharedWorker, Storage, Url,
};

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

const TC_SHARED_WORKER_BUNDLE_B64: &str =
    include_str!("../../../demo/pkg/worker/shared_worker_bundle.js.b64");

fn blob_url_from_js_b64(b64: &str) -> Result<String, JsValue> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let parts = js_sys::Array::new();

    parts.push(&js_sys::Uint8Array::from(bytes.as_slice()));

    let mut bag = BlobPropertyBag::new();

    bag.type_("application/javascript");

    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &bag)?;

    Url::create_object_url_with_blob(&blob)
}

fn calculate_content_hash(base_js_content: &str, additional_code: &str) -> String {
    let mut hasher = DefaultHasher::new();
    base_js_content.hash(&mut hasher);
    additional_code.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

async fn is_blob_url_valid(blob_url: &str) -> bool {
    // Test if a blob URL is still accessible by trying to fetch from it
    console::log_1(&format!("🔍 Testing blob URL validity: {}", blob_url).into());

    let window = match web_sys::window() {
        Some(w) => w,
        None => {
            console::log_1(&"❌ No window available for blob test".into());
            return false;
        }
    };

    // Try to create a request to the blob URL
    let request = match Request::new_with_str(blob_url) {
        Ok(r) => r,
        Err(_) => {
            console::log_1(&"❌ Failed to create request for blob URL".into());
            return false;
        }
    };

    // Try to fetch the blob
    match JsFuture::from(window.fetch_with_request(&request)).await {
        Ok(resp_value) => match resp_value.dyn_into::<Response>() {
            Ok(response) => {
                let is_ok = response.ok();
                console::log_1(&format!("✓ Blob URL test result: ok={}", is_ok).into());
                is_ok
            }
            Err(_) => {
                console::log_1(&"❌ Failed to convert fetch response".into());
                false
            }
        },
        Err(_) => {
            console::log_1(&"❌ Blob URL fetch failed - blob is invalid/dead".into());
            false
        }
    }
}

async fn get_or_create_shared_worker_blob(additional_code: &str) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let storage = window.local_storage()?.ok_or("No localStorage")?;
    // Fetch the base SharedWorker content
    // let request = Request::new_with_str("/pkg/worker/tab_coordinator_shared_worker.js")?;
    // let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    // let resp: Response = resp_value.dyn_into()?;
    // let text = JsFuture::from(resp.text()?).await?;

    // Calculate current content hash
    let current_hash = calculate_content_hash(TC_SHARED_WORKER_BUNDLE_B64, additional_code);

    // Check localStorage for cached version
    if let Some(cached_json) = storage.get_item("shared_worker_cache")? {
        if let Ok(cached_data) = js_sys::JSON::parse(&cached_json) {
            if let (Some(cached_hash), Some(cached_url)) = (
                js_sys::Reflect::get(&cached_data, &"contentHash".into())
                    .ok()
                    .and_then(|v| v.as_string()),
                js_sys::Reflect::get(&cached_data, &"blobUrl".into())
                    .ok()
                    .and_then(|v| v.as_string()),
            ) {
                if cached_hash == current_hash && !cached_url.is_empty() {
                    console::log_1(
                        &format!(
                            "🔍 Found cached blob (hash: {}), validating...",
                            cached_hash
                        )
                        .into(),
                    );

                    // CRITICAL: Test if the cached blob URL is still valid
                    if is_blob_url_valid(&cached_url).await {
                        console::log_1(
                            &format!("✅ Cached blob is valid, reusing SharedWorker").into(),
                        );
                        return Ok(cached_url);
                    } else {
                        console::log_1(&format!("❌ Cached blob is dead, creating new one").into());
                        // Continue to create a new blob
                    }
                } else {
                    console::log_1(
                        &format!(
                            "🔄 Content changed (old: {}, new: {}), creating new blob",
                            cached_hash, current_hash
                        )
                        .into(),
                    );
                }
            }
        }
    }

    // Create new blob with all content (either because no cache, content changed, or cached blob is dead)
    console::log_1(&format!("🆕 Creating new SharedWorker blob (hash: {})", current_hash).into());
    let blob_url = blob_url_from_js_b64(TC_SHARED_WORKER_BUNDLE_B64)?;

    // Cache the new blob URL with its hash
    let cache_data = js_sys::Object::new();
    js_sys::Reflect::set(&cache_data, &"contentHash".into(), &current_hash.into())?;
    js_sys::Reflect::set(&cache_data, &"blobUrl".into(), &blob_url.clone().into())?;

    let cache_json = js_sys::JSON::stringify(&cache_data)?;
    storage.set_item("shared_worker_cache", &cache_json.as_string().unwrap())?;

    console::log_1(&format!("💾 Cached new blob URL: {}", blob_url).into());
    Ok(blob_url)
}

async fn create_complete_worker_blob(
    base_js: &str,
    additional_code: &str,
) -> Result<String, JsValue> {
    // Combine base JS with additional code
    let complete_js = format!("{}\n\n{}", base_js, additional_code);

    // Create JS blob
    let array = js_sys::Array::new();
    array.push(&complete_js.into());

    let blob_options = BlobPropertyBag::new();
    blob_options.set_type("application/javascript");

    let blob = Blob::new_with_str_sequence_and_options(&array, &blob_options)?;
    Url::create_object_url_with_blob(&blob)
}

#[wasm_bindgen]
impl TabManager {
    #[wasm_bindgen(constructor)]
    pub async fn new(worker: web_sys::Worker) -> Result<TabManager, JsValue> {
        let tab_id = Uuid::new_v4().to_string();
        let leader_data = Rc::new(RefCell::new(String::new()));
        let response_sender = Rc::new(RefCell::new(None::<oneshot::Sender<String>>));
        let leader_callback = Rc::new(RefCell::new(None::<js_sys::Function>));
        let query_response_sender = Rc::new(RefCell::new(
            None::<oneshot::Sender<Result<Vec<Vec<String>>, String>>>,
        ));

        // Define the additional initialization code for SharedWorker
        let additional_code = r#"
// Save the real onconnect handler
let realOnconnect = null;

// Set up onconnect immediately
self.onconnect = (e) => {
    console.log("JS onconnect fired");
    if (realOnconnect) {
        realOnconnect(e);
    } else {
        console.log("WASM not ready yet, connection will be handled after init");
        // Store the event to handle after WASM init
        self._pendingConnection = e;
    }
};

// Initialize WASM
wasm_bindgen(self.location.origin + "/pkg/worker/tab_coordinator_shared_worker_bg.wasm")
    .then(() => {
        console.log("Worker WASM loaded");
        wasm_bindgen.main();
        
        // Store the real handler
        realOnconnect = wasm_bindgen.handle_connect;
        
        // Handle any pending connection
        if (self._pendingConnection) {
            console.log("Handling pending connection");
            realOnconnect(self._pendingConnection);
            self._pendingConnection = null;
        }
    })
    .catch(console.error);
"#;

        // Use content-hashed cached blob URL for proper SharedWorker sharing
        let blob_url = get_or_create_shared_worker_blob(additional_code).await?;
        console::log_1(&format!("Using SharedWorker blob URL: {}", blob_url).into());

        // Create the shared worker using the cached blob URL
        let shared_worker = SharedWorker::new(&blob_url)?;
        let port = shared_worker.port();
        port.start();

        // Use the provided SQLite worker
        let worker = Rc::new(worker);

        // Set up message handler
        let port_clone = port.clone();
        let leader_data_clone = leader_data.clone();
        let tab_id_clone = tab_id.clone();
        let response_sender_clone = response_sender.clone();
        let leader_callback_clone = leader_callback.clone();
        let query_response_sender_clone = query_response_sender.clone();
        let query_response_sender_closure = query_response_sender_clone.clone();

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
                response_sender: response_sender_clone,
                leader_data: leader_data_clone,
                port: port_clone,
                tab_id: tab_id_clone,
                worker: worker.clone(),
                query_response_sender: query_response_sender_clone,
            }));

            Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                if let Ok(msg) = serde_wasm_bindgen::from_value::<TabMessage>(e.data()) {
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
                            console::log_1(&JsValue::from_str("ExecuteQuery received by tab"));

                            // Clone everything we need from state
                            let (port, tab_id, worker, response_sender, query_response_sender) = {
                                let state = state.borrow();
                                (
                                    state.port.clone(),
                                    state.tab_id.clone(),
                                    state.worker.clone(),
                                    state.response_sender.clone(),
                                    state.query_response_sender.clone(),
                                )
                            };
                            let original_requester = from_tab_id.clone();

                            wasm_bindgen_futures::spawn_local(async move {
                                // Create a separate channel for leader check
                                let (leader_sender, leader_receiver) = oneshot::channel::<String>();
                                let msg = TabMessage::CheckLeader {
                                    tab_id: tab_id.clone(),
                                };

                                // Store the sender in response_sender
                                *response_sender.borrow_mut() = Some(leader_sender);

                                port.post_message(&serde_wasm_bindgen::to_value(&msg).unwrap())
                                    .unwrap();

                                let is_leader = leader_receiver
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

                                    // Only send through query_response_sender if we're the original requester
                                    if tab_id == original_requester {
                                        if let Some(sender) =
                                            query_response_sender.borrow_mut().take()
                                        {
                                            let _ =
                                                sender
                                                    .send(Err("Only leader can execute queries"
                                                        .to_string()));
                                        }
                                    }
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

                                        // Send results through both channels
                                        // 1. Back to the original requester through the shared worker
                                        let response = TabMessage::QueryResponse {
                                            results: parsed_results.clone(),
                                            from_tab_id: original_requester.clone(),
                                            error: None,
                                        };
                                        port.post_message(
                                            &serde_wasm_bindgen::to_value(&response).unwrap(),
                                        )
                                        .unwrap();

                                        // 2. If we're the leader AND the original requester, send through our query_response_sender
                                        if tab_id == original_requester {
                                            if let Some(sender) =
                                                query_response_sender.borrow_mut().take()
                                            {
                                                let _ = sender.send(Ok(parsed_results));
                                            }
                                        }

                                        console::log_1(&JsValue::from_str(&format!(
                                            "Sent query response to tab: {}",
                                            original_requester
                                        )));
                                    }
                                    Err(e) => {
                                        let error_msg = format!("Query error: {:?}", e);

                                        // Send error through both channels
                                        // 1. Back to the original requester through the shared worker
                                        let response = TabMessage::QueryResponse {
                                            results: vec![],
                                            from_tab_id: original_requester.clone(),
                                            error: Some(error_msg.clone()),
                                        };
                                        port.post_message(
                                            &serde_wasm_bindgen::to_value(&response).unwrap(),
                                        )
                                        .unwrap();

                                        // 2. If we're the leader AND the original requester, send through our query_response_sender
                                        if tab_id == original_requester {
                                            if let Some(sender) =
                                                query_response_sender.borrow_mut().take()
                                            {
                                                let _ = sender.send(Err(error_msg));
                                            }
                                        }
                                    }
                                }
                            });
                        }
                        TabMessage::QueryResponse {
                            results,
                            error,
                            from_tab_id,
                        } => {
                            console::log_1(&JsValue::from_str(&format!(
                                "Received query response for tab: {}",
                                from_tab_id
                            )));

                            // First get the current tab ID
                            let current_tab_id = {
                                let state = state.borrow();
                                state.tab_id.clone()
                            };

                            // Only process if we're the original requester
                            if current_tab_id == from_tab_id {
                                let sender = query_response_sender_closure.borrow_mut().take();

                                if let Some(s) = sender {
                                    let result = match error {
                                        Some(err) => Err(err),
                                        None => Ok(results),
                                    };
                                    let _ = s.send(result);
                                }
                            }
                        }
                        TabMessage::FreshWorkerNotification { is_fresh } => {
                            if is_fresh {
                                console::log_1(&JsValue::from_str("🆕 Received fresh worker notification - this SharedWorker is newly created!"));
                                console::log_1(&JsValue::from_str("👑 Immediately claiming leadership as first tab in fresh worker"));

                                // Since this is a fresh worker, we should immediately claim leadership
                                // without waiting for the normal 3-second timeout
                                let immediate_claim_msg = TabMessage::ClaimLeadership {
                                    tab_id: {
                                        let state = state.borrow();
                                        state.tab_id.clone()
                                    },
                                };

                                let port = {
                                    let state = state.borrow();
                                    state.port.clone()
                                };

                                if let Ok(msg) = serde_wasm_bindgen::to_value(&immediate_claim_msg)
                                {
                                    let _ = port.post_message(&msg);
                                }
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

        // Immediately after registering, probe for existing leaders
        // This helps detect if we're connecting to an existing SharedWorker with active tabs
        let probe_msg = TabMessage::QueryLeader {
            from_tab_id: tab_id.clone(),
        };
        port.post_message(&serde_wasm_bindgen::to_value(&probe_msg).unwrap())
            .unwrap();

        // Set up heartbeat timer
        let port_for_heartbeat = port.clone();
        let tab_id_for_heartbeat = tab_id.clone();
        let heartbeat_closure = Closure::wrap(Box::new(move || {
            let heartbeat_msg = TabMessage::Heartbeat {
                tab_id: tab_id_for_heartbeat.clone(),
            };
            let _ = port_for_heartbeat
                .post_message(&serde_wasm_bindgen::to_value(&heartbeat_msg).unwrap());
        }) as Box<dyn Fn()>);

        // Send heartbeat every 5 seconds
        web_sys::window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                heartbeat_closure.as_ref().unchecked_ref(),
                5000,
            )
            .unwrap();
        heartbeat_closure.forget();

        // Set up leadership claim timer - try to claim leadership after a short delay to handle refresh scenarios
        let port_for_claim = port.clone();
        let tab_id_for_claim = tab_id.clone();
        let claim_closure = Closure::wrap(Box::new(move || {
            console::log_1(&JsValue::from_str(
                "⏰ Leadership claim timeout - attempting to claim leadership",
            ));
            let claim_msg = TabMessage::ClaimLeadership {
                tab_id: tab_id_for_claim.clone(),
            };
            let _ = port_for_claim.post_message(&serde_wasm_bindgen::to_value(&claim_msg).unwrap());
        }) as Box<dyn Fn()>);

        // Try to claim leadership after 1 second (reduced from 3 seconds for better refresh handling)
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                claim_closure.as_ref().unchecked_ref(),
                1000,
            )
            .unwrap();
        claim_closure.forget();

        Ok(TabManager {
            port,
            tab_id,
            leader_data,
            response_sender,
            leader_callback,
            query_response_sender,
            worker,
        })
    }

    #[wasm_bindgen]
    pub async fn check_leader(&self) -> Result<bool, JsValue> {
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
        // Create a new channel for this query
        let (sender, receiver) = oneshot::channel();

        // Store the sender in query_response_sender
        {
            let mut query_sender = self.query_response_sender.borrow_mut();
            *query_sender = Some(sender);
        } // ensure the borrow is dropped

        // Send the query request
        let msg = TabMessage::ExecuteQuery {
            sql: sql.to_string(),
            from_tab_id: self.tab_id.clone(),
        };
        self.port
            .post_message(&serde_wasm_bindgen::to_value(&msg)?)?;

        // Wait for response
        let response = receiver
            .await
            .map_err(|_| JsValue::from_str("Channel closed"))?;

        // Convert the response to JsValue
        match response {
            Ok(results) => Ok(serde_wasm_bindgen::to_value(&results)?),
            Err(err) => Err(JsValue::from_str(&err)),
        }
    }
}
