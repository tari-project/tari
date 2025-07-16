use wasm_bindgen::prelude::*;

//wasm-pack build examples/js-hello-world --target web

#[wasm_bindgen]
pub fn make_tari_address() -> Result<String, JsValue> {
    let f0 = Fee::new(TransactionWeight::latest());
    return Ok("".to_string())
}
