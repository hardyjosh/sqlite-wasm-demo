use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;
use web_sys::SharedWorkerGlobalScope;

#[wasm_bindgen]
pub fn handle_connect(e: MessageEvent) {
    web_sys::console::log_1(&"Got connect event from JS".into());
    let ports = js_sys::Array::from(&e.ports());
    let port = Rc::new(ports.get(0).dyn_into::<web_sys::MessagePort>().unwrap());

    port.start();
    port.post_message(&JsValue::from_str("worker_ready"))
        .unwrap();

    let port_clone = port.clone();
    let port_message_handler = Closure::wrap(Box::new(move |e: MessageEvent| {
        web_sys::console::log_1(
            &format!("SharedWorker received message from UI: {:?}", e.data()).into(),
        );
        port_clone.post_message(&e.data()).unwrap();
    }) as Box<dyn FnMut(MessageEvent)>);

    port.set_onmessage(Some(port_message_handler.as_ref().unchecked_ref()));
    port_message_handler.forget();
}

#[wasm_bindgen(start)]
pub fn main() {
    web_sys::console::log_1(&"SharedWorker WASM initialized".into());
}
