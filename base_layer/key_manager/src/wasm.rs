use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub unsafe fn alert(s: &str);
}

#[wasm_bindgen]
pub fn greet(name: &str) {
    alert(&format!("Hello, {}!", name));
}
