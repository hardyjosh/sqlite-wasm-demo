use tab_coordinator::TabManager;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::Worker;

#[wasm_bindgen]
pub struct BrowserSQLite {
    worker: Worker,
    tab_manager: TabManager,
}

#[wasm_bindgen]
impl BrowserSQLite {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<BrowserSQLite, JsValue> {
        let worker = Worker::new("./pkg/sqlite_wrapper/sqlite_wrapper.js")?;
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
