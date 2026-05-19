use squidc_core::{compile, CompileRequest};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compile_squidscript(source: &str, target_id: &str) -> String {
    let response = compile(CompileRequest {
        source: source.to_string(),
        target_id: target_id.to_string(),
    });

    serde_json::to_string(&response).expect("compile response must serialize")
}
