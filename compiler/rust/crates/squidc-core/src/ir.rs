use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrProgram {
    pub format: String,
    pub app: IrApp,
    #[serde(default = "default_state_store")]
    pub state_store: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_bindings: Vec<IrDeviceBinding>,
    pub state: Vec<IrStateValue>,
    pub functions: Vec<IrFunction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<IrTrigger>,
    pub handlers: Vec<IrHandler>,
    pub screens: Vec<IrScreen>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrApp {
    pub id: String,
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrStateValue {
    pub name: String,
    pub value_type: String,
    pub nullable: bool,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrDeviceBinding {
    pub service: String,
    pub binding: String,
    pub resource: String,
}

pub(crate) fn default_state_store() -> String {
    "default".to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrHandler {
    pub event: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub preload: bool,
    pub statements: Vec<IrStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<String>,
    pub statements: Vec<IrStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrTrigger {
    pub event: String,
    pub repeating: bool,
    pub interval_ms: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrScreen {
    pub name: String,
    pub render: String,
    pub statements: Vec<IrStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op")]
pub enum IrStatement {
    #[serde(rename = "state.load")]
    StateLoad,
    #[serde(rename = "state.save")]
    StateSave,
    #[serde(rename = "state.reset")]
    StateReset,
    #[serde(rename = "screen.open")]
    ScreenOpen { screen: String },
    #[serde(rename = "screen.refresh")]
    ScreenRefresh,
    #[serde(rename = "app.exit")]
    AppExit,
    #[serde(rename = "app.launch")]
    AppLaunch { app: String },
    #[serde(rename = "app.arm")]
    AppArm { app: String },
    #[serde(rename = "app.disarm")]
    AppDisarm { app: String },
    #[serde(rename = "service.timer.every")]
    ServiceTimerEvery { event: String, interval_ms: IrExpr },
    #[serde(rename = "service.timer.after")]
    ServiceTimerAfter { event: String, delay_ms: IrExpr },
    #[serde(rename = "assign")]
    Assign { name: String, expr: IrExpr },
    #[serde(rename = "let")]
    Let { name: String, expr: IrExpr },
    #[serde(rename = "if")]
    If {
        condition: IrExpr,
        then_statements: Vec<IrStatement>,
        else_statements: Vec<IrStatement>,
    },
    #[serde(rename = "repeat")]
    Repeat {
        count: IrExpr,
        statements: Vec<IrStatement>,
    },
    #[serde(rename = "for")]
    For {
        item: String,
        list: IrExpr,
        max: Option<IrExpr>,
        statements: Vec<IrStatement>,
    },
    #[serde(rename = "return")]
    Return { expr: Option<IrExpr> },
    #[serde(rename = "call")]
    Call { name: String, args: Vec<IrExpr> },
    #[serde(rename = "debug.print")]
    DebugPrint { args: Vec<IrExpr> },
    #[serde(rename = "debug.block")]
    DebugBlock { statements: Vec<IrStatement> },
    #[serde(rename = "hardware.gpio.write")]
    HardwareGpioWrite { name: String, value: IrExpr },
    #[serde(rename = "hardware.gpio.toggle")]
    HardwareGpioToggle { name: String },
    #[serde(rename = "service.indicator.write")]
    ServiceIndicatorWrite { value: IrExpr },
    #[serde(rename = "service.indicator.toggle")]
    ServiceIndicatorToggle,
    #[serde(rename = "service.indicator.breathe")]
    ServiceIndicatorBreathe,
    #[serde(rename = "service.display.clear")]
    DisplayClear { color: String },
    #[serde(rename = "service.display.text")]
    DisplayText {
        text: IrExpr,
        options: serde_json::Value,
    },
    #[serde(rename = "service.display.rect")]
    DisplayRect {
        x: i64,
        y: i64,
        w: i64,
        h: i64,
        options: serde_json::Value,
    },
    #[serde(rename = "service.display.line")]
    DisplayLine {
        x1: i64,
        y1: i64,
        x2: i64,
        y2: i64,
        options: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op")]
pub enum IrExpr {
    #[serde(rename = "literal")]
    Literal { value: serde_json::Value },
    #[serde(rename = "state")]
    State { name: String },
    #[serde(rename = "binary")]
    Binary {
        left: Box<IrExpr>,
        operator: String,
        right: Box<IrExpr>,
    },
    #[serde(rename = "unary")]
    Unary { operator: String, expr: Box<IrExpr> },
    #[serde(rename = "field")]
    Field { target: Box<IrExpr>, field: String },
    #[serde(rename = "hardware.gpio.read")]
    HardwareGpioRead { name: String },
    #[serde(rename = "service.indicator.read")]
    ServiceIndicatorRead,
    #[serde(rename = "system.memory")]
    SystemMemory,
    #[serde(rename = "system.storage")]
    SystemStorage { name: String },
    #[serde(rename = "call")]
    Call { name: String, args: Vec<IrExpr> },
}

pub const SQBC_MAGIC: &[u8; 4] = b"SQBC";

pub fn encode_sqbc(ir: &IrProgram) -> Vec<u8> {
    let payload = serde_json::to_vec(ir).expect("IR must serialize for SQBC payload");
    let mut bytes = Vec::with_capacity(8 + payload.len());
    bytes.extend_from_slice(SQBC_MAGIC);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}
