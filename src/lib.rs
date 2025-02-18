use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, MessagePort, SharedWorker};

#[wasm_bindgen]
pub struct TabManager {
    port: MessagePort,
    _onmessage: Closure<dyn FnMut(web_sys::MessageEvent)>,
}

#[wasm_bindgen]
impl TabManager {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<TabManager, JsValue> {
        web_sys::console::log_1(&"About to load the worker".into());
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
            web_sys::console::log_2(&"Main got message:".into(), &e.data());

            // If we get the ready message, send our hello
            if e.data().as_string() == Some("worker_ready".to_string()) {
                web_sys::console::log_1(&"Worker is ready, sending message".into());
                port_clone
                    .post_message(&JsValue::from_str("hello from main"))
                    .unwrap();
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);

        port.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        port.start();

        web_sys::console::log_1(&"TabManager created in main wasm".into());
        Ok(TabManager {
            port,
            _onmessage: onmessage,
        })
    }

    #[wasm_bindgen]
    pub fn send_test_message(&self) {
        web_sys::console::log_1(&"Sending test message to worker".into());
        self.port
            .post_message(&JsValue::from_str("test message from UI"))
            .unwrap();
    }
}
