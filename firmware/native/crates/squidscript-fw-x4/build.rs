use std::{env, fs, path::PathBuf};

use squidc_core::compile::{compile, CompileRequest};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target = manifest.join("../../../../targets/xteink-x4.target.json");
    println!("cargo:rerun-if-changed={}", target.display());

    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read X4 target JSON"))
            .expect("parse X4 target JSON");
    let input = value.get("input").expect("target input object");
    let devices = value.get("devices").expect("target devices object");
    let sd = devices.get("storage.sd").expect("target storage.sd device");
    let timing = input.get("gestureTiming").expect("input gestureTiming");
    let buttons = input
        .get("buttons")
        .and_then(serde_json::Value::as_array)
        .expect("input buttons array");

    let mut records = String::new();
    for button in buttons {
        let logical = required_str(button, "logical");
        let kind = required_str(button, "type");
        let gestures = button
            .get("gestures")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.as_str().expect("gesture string"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for gesture in &gestures {
            assert!(
                matches!(*gesture, "longTap" | "doubleTap"),
                "unsupported input gesture {gesture}"
            );
        }
        let gpio = optional_gpio(button.get("gpio").and_then(serde_json::Value::as_str));
        let adc = optional_gpio(button.get("adc").and_then(serde_json::Value::as_str));
        let range = button.get("range");
        let min = range
            .and_then(|value| value.get("minExclusive"))
            .and_then(serde_json::Value::as_i64)
            .map(|value| value as i32);
        let max = range
            .and_then(|value| value.get("maxInclusive"))
            .and_then(serde_json::Value::as_i64)
            .map(|value| value as i32);
        records.push_str(&format!(
            "    GeneratedInputButton {{ logical: {logical:?}, kind: {kind:?}, gpio: {}, adc: {}, min_exclusive: {}, max_inclusive: {}, active_low: {}, long_tap: {}, double_tap: {} }},\n",
            option_u8(gpio),
            option_u8(adc),
            option_i32(min),
            option_i32(max),
            button.get("activeLow").and_then(serde_json::Value::as_bool).unwrap_or(false),
            gestures.contains(&"longTap"),
            gestures.contains(&"doubleTap"),
        ));
    }

    let generated = format!(
        "pub const INPUT_DEBOUNCE_MS: u32 = {};\n\
         pub const INPUT_LONG_TAP_MS: u32 = {};\n\
         pub const INPUT_DOUBLE_TAP_WINDOW_MS: u32 = {};\n\
         pub const INPUT_BUTTONS: [GeneratedInputButton; {}] = [\n{records}];\n",
        required_u64(input, "debounceMs"),
        required_u64(timing, "longTapMs"),
        required_u64(timing, "doubleTapWindowMs"),
        buttons.len(),
    );
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("target_input.rs"),
        generated,
    )
    .expect("write generated X4 input metadata");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("target_config.rs"),
        format!(
            "pub const SD_SPI_FREQUENCY_HZ: u32 = {};\n",
            required_u64(sd, "frequencyHz")
        ),
    )
    .expect("write generated X4 target config");

    let fallback = manifest.join("fallback/main.squid");
    println!("cargo:rerun-if-changed={}", fallback.display());
    let compiled = compile(CompileRequest {
        source: fs::read_to_string(&fallback).expect("read native fallback source"),
        target_id: "xteink-x4".into(),
    });
    assert!(
        compiled.ok,
        "native fallback compile failed: {:?}",
        compiled.diagnostics
    );
    let sqbc = squidc_core::sqbc::encode_sqbc(&compiled.ir.expect("native fallback IR"))
        .expect("encode native fallback SQBC");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("fallback-main.sqbc"),
        sqbc,
    )
    .expect("write native fallback SQBC");
}

fn required_u64(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_else(|| panic!("missing integer {key}"))
}

fn required_str<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("missing string {key}"))
}

fn optional_gpio(value: Option<&str>) -> Option<u8> {
    value.map(|value| {
        value
            .strip_prefix("GPIO")
            .expect("GPIO name prefix")
            .parse()
            .expect("GPIO number")
    })
}

fn option_u8(value: Option<u8>) -> String {
    value.map_or_else(|| "None".into(), |value| format!("Some({value})"))
}

fn option_i32(value: Option<i32>) -> String {
    value.map_or_else(|| "None".into(), |value| format!("Some({value})"))
}
