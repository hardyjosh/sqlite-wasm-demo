use base64::Engine;
use tab_coordinator::TabManager;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, BlobPropertyBag, Url, Worker};

const SQLITE_WORKER_BUNDLE_B64: &str =
    include_str!("../../../demo/pkg/sqlite_wrapper/worker_bundle.js.b64");

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

#[wasm_bindgen]
pub struct BrowserSQLite {
    worker: Worker,
    tab_manager: TabManager,
}

#[wasm_bindgen]
impl BrowserSQLite {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<BrowserSQLite, JsValue> {
        let url = blob_url_from_js_b64(SQLITE_WORKER_BUNDLE_B64)?;
        let worker = Worker::new(&url)?;
        let tab_manager = TabManager::new(worker.clone())?;
        Ok(BrowserSQLite {
            worker,
            tab_manager,
        })
    }

    pub async fn execute(&self, sql: &str) -> Result<(), JsValue> {
        // Check if we're the leader first
        let is_leader = self.tab_manager.check_leader().await?;
        if !is_leader {
            return Err(JsValue::from_str(
                "Only leader can execute write operations",
            ));
        }

        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let handler = move |e: web_sys::MessageEvent| {
                resolve.call1(&JsValue::NULL, &e.data()).unwrap();
            };
            let closure = Closure::once(handler);
            self.worker
                .set_onmessage(Some(closure.as_ref().unchecked_ref()));
            self.worker.post_message(&JsValue::from_str(sql)).unwrap();
            closure.forget();
        });

        JsFuture::from(promise).await?;
        Ok(())
    }

    pub async fn query(&self, sql: &str) -> Result<JsValue, JsValue> {
        let is_leader = self.tab_manager.check_leader().await?;
        web_sys::console::log_1(&JsValue::from_str(&format!(
            "BrowserSQLite: Is leader? {}",
            is_leader
        )));

        if is_leader {
            // We're the leader, execute query directly
            let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                let handler = move |e: web_sys::MessageEvent| {
                    resolve.call1(&JsValue::NULL, &e.data()).unwrap();
                };
                let closure = Closure::once(handler);
                self.worker
                    .set_onmessage(Some(closure.as_ref().unchecked_ref()));
                self.worker
                    .post_message(&JsValue::from_str(&format!("QUERY:{}", sql)))
                    .unwrap();
                closure.forget();
            });

            JsFuture::from(promise).await
        } else {
            self.tab_manager.route_query(sql).await
        }
    }

    pub fn get_tab_id(&self) -> String {
        self.tab_manager.get_tab_id()
    }

    pub async fn check_leader(&self) -> Result<bool, JsValue> {
        self.tab_manager.check_leader().await
    }
}
