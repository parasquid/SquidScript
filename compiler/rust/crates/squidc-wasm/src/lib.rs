use squidc_core::{
    compile::{compile, compile_with_profile, CompileRequest},
    profile::BuildProfile,
    sqbc,
};
use squidvm_core::{
    error::VmError,
    host::{
        DisplayLineOptions, DisplayRectOptions, DisplayTextOptions, SimWifiBackend, TraceSink,
        WifiApIp, WifiBackend, WifiOperation, WifiOperationResult, WifiScanNetwork, WifiStatus,
    },
    limits::MAX_SAVED_STATE_BYTES,
    program::Program,
    strings::StringResolver,
    value::Value,
    vm::Vm,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compile_squidscript(source: &str, target_id: &str) -> String {
    let response = compile(CompileRequest {
        source: source.to_string(),
        target_id: target_id.to_string(),
    });

    serde_json::to_string(&response).expect("compile response must serialize")
}

#[wasm_bindgen]
pub fn compile_sqbc(source: &str, target_id: &str) -> Result<Vec<u8>, JsValue> {
    let response = compile_with_profile(
        CompileRequest {
            source: source.to_string(),
            target_id: target_id.to_string(),
        },
        BuildProfile::Dev,
    );
    if !response.ok {
        return Err(JsValue::from_str("compile failed"));
    }
    let ir = response
        .ir
        .ok_or_else(|| JsValue::from_str("compiler returned no IR"))?;
    sqbc::encode_sqbc_with_profile(&ir, BuildProfile::Dev)
        .map_err(|error| JsValue::from_str(&error.message))
}

#[wasm_bindgen]
pub fn read_sqbc_app_id(bytes: &[u8]) -> Result<String, JsValue> {
    sqbc::read_app_id(bytes)
        .map_err(|error| JsValue::from_str(&error.message))?
        .ok_or_else(|| JsValue::from_str("SQBC missing app id"))
}

#[wasm_bindgen]
pub struct WasmVm {
    vm: Vm<'static>,
    _program_bytes: &'static [u8],
    host: BrowserHost,
}

#[wasm_bindgen]
impl WasmVm {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: Vec<u8>, state_bytes: Vec<u8>) -> Result<WasmVm, JsValue> {
        let program_bytes = Box::leak(bytes.into_boxed_slice()) as &'static [u8];
        let program = Program::parse(program_bytes).map_err(js_vm_error)?;
        Ok(Self {
            vm: Vm::new(program),
            _program_bytes: program_bytes,
            host: BrowserHost::new(state_bytes),
        })
    }

    pub fn dispatch(&mut self, event: &str) -> Result<String, JsValue> {
        self.host.clear_turn();
        match self.vm.dispatch(event, &mut self.host) {
            Ok(()) | Err(VmError::HandlerNotFound) => self.snapshot(),
            Err(error) => Err(js_vm_error(error)),
        }
    }

    pub fn snapshot(&self) -> Result<String, JsValue> {
        let state = state_json(&self.vm)?;
        let value = serde_json::json!({
            "running": !self.vm.exited(),
            "exited": self.vm.exited(),
            "currentScreen": self.vm.current_screen().map_err(js_vm_error)?.unwrap_or(""),
            "state": state,
            "drawCommands": self.host.draw_commands,
            "debugOutput": self.host.debug_output,
            "stateDirty": self.host.state_dirty,
        });
        Ok(value.to_string())
    }

    pub fn state_bytes(&self) -> Vec<u8> {
        self.host.state_bytes.clone()
    }

    pub fn clear_state_dirty(&mut self) {
        self.host.state_dirty = false;
    }
}

struct BrowserHost {
    state_bytes: Vec<u8>,
    state_dirty: bool,
    draw_commands: Vec<serde_json::Value>,
    debug_output: Vec<String>,
    wifi: SimWifiBackend,
    indicator: bool,
}

impl BrowserHost {
    fn new(state_bytes: Vec<u8>) -> Self {
        Self {
            state_bytes,
            state_dirty: false,
            draw_commands: Vec::new(),
            debug_output: Vec::new(),
            wifi: SimWifiBackend::new(),
            indicator: false,
        }
    }

    fn clear_turn(&mut self) {
        self.draw_commands.clear();
        self.debug_output.clear();
    }
}

impl TraceSink for BrowserHost {
    fn trace(&mut self, _message: &str) {}

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
        let mut parts = Vec::new();
        for value in values {
            parts.push(value_text(strings, *value));
        }
        self.debug_output.push(parts.join(" "));
    }

    fn draw_clear(&mut self, color: &str) {
        self.draw_commands
            .push(serde_json::json!({ "op": "clear", "gray": color_to_gray(color) }));
    }

    fn draw_text(
        &mut self,
        strings: &StringResolver<'_>,
        text: Value,
        options: DisplayTextOptions<'_>,
    ) {
        if let Some(background) = options.background_color {
            if options.h > 0 {
                self.draw_commands.push(serde_json::json!({
                    "op": "rect",
                    "x": options.x,
                    "y": options.y,
                    "width": options.w,
                    "height": options.h,
                    "gray": color_to_gray(background),
                    "fill": true,
                }));
            }
        }
        let align = options.align.unwrap_or("left");
        let x = match align {
            "center" => options.x + options.w / 2,
            "right" => options.x + options.w,
            _ => options.x,
        };
        let mut command = serde_json::json!({
            "op": "text",
            "x": x,
            "y": options.y,
            "text": value_text(strings, text),
            "gray": color_to_gray(options.text_color.unwrap_or("gray15")),
            "maxWidth": options.w,
        });
        if options.font_height > 0 {
            command["fontHeight"] = serde_json::json!(options.font_height);
        }
        if options.h > 0 {
            command["boxHeight"] = serde_json::json!(options.h);
        }
        if let Some(align) = options.align {
            command["align"] = serde_json::json!(align);
        }
        if let Some(valign) = options.valign {
            command["valign"] = serde_json::json!(valign);
        }
        self.draw_commands.push(command);
    }

    fn draw_rect(&mut self, options: DisplayRectOptions<'_>) {
        let fill = options.fill_color.is_some();
        let color = options
            .fill_color
            .or(options.stroke_color)
            .unwrap_or("gray15");
        self.draw_commands.push(serde_json::json!({
            "op": "rect",
            "x": options.x,
            "y": options.y,
            "width": options.w,
            "height": options.h,
            "gray": color_to_gray(color),
            "fill": fill,
        }));
    }

    fn draw_line(&mut self, options: DisplayLineOptions<'_>) {
        self.draw_commands.push(serde_json::json!({
            "op": "line",
            "x1": options.x1,
            "y1": options.y1,
            "x2": options.x2,
            "y2": options.y2,
            "gray": color_to_gray(options.color.unwrap_or("gray15")),
        }));
    }

    fn service_indicator_write(&mut self, value: bool) -> Result<(), VmError> {
        self.indicator = value;
        Ok(())
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        self.indicator = !self.indicator;
        Ok(())
    }

    fn service_indicator_breathe(&mut self) -> Result<(), VmError> {
        Ok(())
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        Ok(self.indicator)
    }

    fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
        if self.state_bytes.is_empty() {
            return Ok(None);
        }
        if out.len() < self.state_bytes.len() {
            return Err(VmError::StateTooLarge);
        }
        out[..self.state_bytes.len()].copy_from_slice(&self.state_bytes);
        Ok(Some(self.state_bytes.len()))
    }

    fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        if bytes.len() > MAX_SAVED_STATE_BYTES {
            return Err(VmError::StateTooLarge);
        }
        self.state_bytes.clear();
        self.state_bytes.extend_from_slice(bytes);
        self.state_dirty = true;
        Ok(())
    }

    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        self.state_bytes.clear();
        self.state_dirty = true;
        Ok(())
    }

    fn service_wifi_start_ap<'a>(&'a mut self, ssid: &str) -> Result<WifiOperation<'a>, VmError> {
        self.wifi.start_ap(ssid)
    }

    fn service_wifi_stop_ap<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.wifi.stop_ap()
    }

    fn service_wifi_connect<'a>(&'a mut self, profile: &str) -> Result<WifiOperation<'a>, VmError> {
        self.wifi.connect(profile)
    }

    fn service_wifi_disconnect<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.wifi.disconnect()
    }

    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        self.wifi.status()
    }

    fn service_wifi_get_ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        self.wifi.ap_ip()
    }

    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.wifi.scan()
    }

    fn service_wifi_operation<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.wifi.operation()
    }

    fn service_wifi_result<'a>(&'a mut self) -> Result<WifiOperationResult<'a>, VmError> {
        self.wifi.result()
    }

    fn service_wifi_cancel<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.wifi.cancel()
    }

    fn service_wifi_scan_network<'a>(
        &'a mut self,
        index: i32,
    ) -> Result<WifiScanNetwork<'a>, VmError> {
        self.wifi.scan_network(index)
    }

    fn service_wifi_teardown(&mut self) -> Result<(), VmError> {
        self.wifi.teardown()?;
        Ok(())
    }
}

fn state_json(vm: &Vm<'_>) -> Result<serde_json::Value, JsValue> {
    let resolver = vm.string_resolver();
    let mut object = serde_json::Map::new();
    for index in 0..vm.state_count() {
        let name = vm.state_name(index).map_err(js_vm_error)?.to_string();
        let value = vm.state_at(index).map_err(js_vm_error)?;
        object.insert(name, value_json(&resolver, value));
    }
    Ok(serde_json::Value::Object(object))
}

fn value_json(strings: &StringResolver<'_>, value: Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => serde_json::json!(value),
        Value::I32(value) => serde_json::json!(value),
        Value::String(_) => {
            serde_json::json!(value_text(strings, value))
        }
        Value::Record(_) => serde_json::json!("<record>"),
        Value::List(_) => serde_json::json!("<list>"),
    }
}

fn value_text(strings: &StringResolver<'_>, value: Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::I32(value) => value.to_string(),
        Value::String(_) => strings.value_str(value).unwrap_or("").to_string(),
        Value::Record(_) => "<record>".to_string(),
        Value::List(_) => "<list>".to_string(),
    }
}

fn color_to_gray(color: &str) -> i32 {
    if color == "black" {
        return 15;
    }
    if color == "white" {
        return 0;
    }
    color
        .strip_prefix("gray")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(15)
        .clamp(0, 15)
}

fn js_vm_error(error: VmError) -> JsValue {
    JsValue::from_str(&format!("{error:?}"))
}
