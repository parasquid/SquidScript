use crate::{
    ir::{IrExpr, IrProgram, IrStatement},
    profile::BuildProfile,
};
use std::collections::BTreeMap;

pub const SQBC_MAGIC: &[u8; 4] = b"SQBC";
const SQBC_HEADER_LEN: usize = 14;

const SECTION_STRINGS: u16 = 1;
const SECTION_STATE: u16 = 2;
const SECTION_FUNCTIONS: u16 = 3;
const SECTION_HANDLERS: u16 = 4;
const SECTION_CODE: u16 = 5;
const SECTION_SCREENS: u16 = 6;
const SECTION_APP_META: u16 = 7;
const SECTION_DEVICE_BINDINGS: u16 = 8;
const SECTION_TRIGGERS: u16 = 9;

const OP_PUSH_INT: u8 = 1;
const OP_PUSH_BOOL: u8 = 2;
const OP_PUSH_STRING: u8 = 3;
const OP_PUSH_NULL: u8 = 4;
const OP_GET_STATE: u8 = 10;
const OP_SET_STATE: u8 = 11;
const OP_GET_LOCAL: u8 = 12;
const OP_SET_LOCAL: u8 = 13;
const OP_GET_FIELD: u8 = 14;
const OP_ADD: u8 = 20;
const OP_SUB: u8 = 21;
const OP_EQ: u8 = 22;
const OP_NE: u8 = 23;
const OP_LT: u8 = 24;
const OP_LTE: u8 = 25;
const OP_GT: u8 = 26;
const OP_GTE: u8 = 27;
const OP_JUMP: u8 = 30;
const OP_JUMP_IF_FALSE: u8 = 31;
const OP_CALL_FUNCTION: u8 = 40;
const OP_RETURN: u8 = 41;
const OP_HALT: u8 = 42;
const OP_CALL_BUILTIN: u8 = 50;
const OP_POP: u8 = 60;
const OP_LIST_LEN: u8 = 61;
const OP_LIST_GET: u8 = 62;

const BUILTIN_STATE_LOAD: u8 = 0x01;
const BUILTIN_STATE_SAVE: u8 = 0x02;
const BUILTIN_STATE_RESET: u8 = 0x03;
const BUILTIN_DEBUG_PRINT: u8 = 0x04;
const BUILTIN_SYSTEM_MEMORY: u8 = 0x05;
const BUILTIN_SYSTEM_STORAGE: u8 = 0x06;
const BUILTIN_SYSTEM_START_REASON: u8 = 0x07;
const BUILTIN_APP_EXIT: u8 = 0x10;
const BUILTIN_APP_LAUNCH: u8 = 0x11;
const BUILTIN_APP_ARM: u8 = 0x12;
const BUILTIN_APP_DISARM: u8 = 0x13;
const BUILTIN_APP_REGISTRY_LIST: u8 = 0x14;
const BUILTIN_APP_REGISTRY_GET: u8 = 0x15;
const BUILTIN_APP_PROCESS_STACK: u8 = 0x16;
const BUILTIN_APP_ARMED_STACK: u8 = 0x17;
const BUILTIN_APP_ARMED_STACK_GET: u8 = 0x18;
const BUILTIN_SCREEN_OPEN: u8 = 0x20;
const BUILTIN_SCREEN_REFRESH: u8 = 0x21;
const BUILTIN_SERVICE_TIMER_EVERY: u8 = 0x22;
const BUILTIN_SERVICE_TIMER_AFTER: u8 = 0x23;
const BUILTIN_DISPLAY_CLEAR: u8 = 0x30;
const BUILTIN_DISPLAY_TEXT: u8 = 0x31;
const BUILTIN_DISPLAY_RECT: u8 = 0x32;
const BUILTIN_DISPLAY_LINE: u8 = 0x33;
const BUILTIN_DISPLAY_SELECT: u8 = 0x34;
const BUILTIN_DISPLAY_IMAGE: u8 = 0x35;
const BUILTIN_DISPLAY_DRAW: u8 = 0x36;
const BUILTIN_DISPLAY_INFO: u8 = 0x37;
const BUILTIN_HARDWARE_GPIO_WRITE: u8 = 0x40;
const BUILTIN_HARDWARE_GPIO_TOGGLE: u8 = 0x41;
const BUILTIN_HARDWARE_GPIO_READ: u8 = 0x42;
const BUILTIN_SERVICE_INDICATOR_WRITE: u8 = 0x48;
const BUILTIN_SERVICE_INDICATOR_TOGGLE: u8 = 0x49;
const BUILTIN_SERVICE_INDICATOR_READ: u8 = 0x4a;
const BUILTIN_SERVICE_INDICATOR_BREATHE: u8 = 0x4b;
const BUILTIN_SERVICE_INDICATOR_BLINK: u8 = 0x4c;
const BUILTIN_SERVICE_WIFI_START_AP: u8 = 0x50;
const BUILTIN_SERVICE_WIFI_STOP_AP: u8 = 0x51;
const BUILTIN_SERVICE_WIFI_STATUS: u8 = 0x52;
const BUILTIN_SERVICE_WIFI_GET_AP_IP: u8 = 0x53;
const BUILTIN_SERVICE_WIFI_CONNECT: u8 = 0x54;
const BUILTIN_SERVICE_WIFI_DISCONNECT: u8 = 0x55;
const BUILTIN_SERVICE_WIFI_SCAN: u8 = 0x56;
const BUILTIN_DEVICE_CONFIG_LOAD: u8 = 0x70;
const BUILTIN_DEVICE_CONFIG_SET: u8 = 0x71;
const BUILTIN_DEVICE_CONFIG_REBIND: u8 = 0x72;
const BUILTIN_DEVICE_CONFIG_SAVE: u8 = 0x73;
const BUILTIN_FILE_PICK_FILE: u8 = 0x90;
const BUILTIN_FILE_READ_TEXT: u8 = 0x91;
const BUILTIN_FILE_READ_LINES: u8 = 0x92;
const BUILTIN_SERVICE_POWER_SLEEP: u8 = 0xc0;

const VALUE_NULL: u8 = 0;
const VALUE_BOOL: u8 = 1;
const VALUE_I32: u8 = 2;
const VALUE_STRING: u8 = 3;

const STATE_TYPE_INT: u8 = 1;
const STATE_TYPE_BOOL: u8 = 2;
const STATE_TYPE_STRING: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqbcError {
    pub message: String,
}

impl SqbcError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Default)]
struct StringTable {
    ids: BTreeMap<String, u16>,
    values: Vec<String>,
}

impl StringTable {
    fn intern(&mut self, value: &str) -> Result<u16, SqbcError> {
        if let Some(id) = self.ids.get(value) {
            return Ok(*id);
        }
        let id =
            u16::try_from(self.values.len()).map_err(|_| SqbcError::new("too many strings"))?;
        self.values.push(value.to_string());
        self.ids.insert(value.to_string(), id);
        Ok(id)
    }

    fn encode(&self) -> Result<Vec<u8>, SqbcError> {
        let mut out = Vec::new();
        write_u16(
            &mut out,
            u16::try_from(self.values.len()).map_err(|_| SqbcError::new("too many strings"))?,
        );
        for value in &self.values {
            let bytes = value.as_bytes();
            write_u16(
                &mut out,
                u16::try_from(bytes.len()).map_err(|_| SqbcError::new("string too long"))?,
            );
            out.extend_from_slice(bytes);
        }
        Ok(out)
    }
}

#[derive(Clone)]
struct FunctionMeta {
    name_id: u16,
    param_count: u16,
    local_count: u16,
    start: u32,
    len: u32,
}

#[derive(Clone)]
struct HandlerMeta {
    event_id: u16,
    preload: bool,
    start: u32,
    len: u32,
}

#[derive(Clone)]
struct ScreenMeta {
    name_id: u16,
    start: u32,
    len: u32,
}

#[derive(Default)]
struct CompileUnit {
    strings: StringTable,
    states: BTreeMap<String, u16>,
    functions: BTreeMap<String, u16>,
    function_metas: Vec<FunctionMeta>,
    handler_metas: Vec<HandlerMeta>,
    screen_metas: Vec<ScreenMeta>,
    code: Vec<u8>,
}

#[derive(Default)]
struct FrameCompiler {
    locals: BTreeMap<String, u16>,
    next_local: u16,
}

impl FrameCompiler {
    fn with_params(params: &[String]) -> Result<Self, SqbcError> {
        let mut frame = Self::default();
        for param in params {
            frame.define_local(param)?;
        }
        Ok(frame)
    }

    fn define_local(&mut self, name: &str) -> Result<u16, SqbcError> {
        if let Some(id) = self.locals.get(name) {
            return Ok(*id);
        }
        let id = self.next_local;
        self.next_local = self
            .next_local
            .checked_add(1)
            .ok_or_else(|| SqbcError::new("too many locals"))?;
        self.locals.insert(name.to_string(), id);
        Ok(id)
    }

    fn local(&self, name: &str) -> Option<u16> {
        self.locals.get(name).copied()
    }

    fn child_scope(&self) -> Self {
        Self {
            locals: self.locals.clone(),
            next_local: self.next_local,
        }
    }

    fn reserve_child_locals(&mut self, child: &Self) {
        self.next_local = self.next_local.max(child.next_local);
    }
}

pub fn encode_sqbc(ir: &IrProgram) -> Result<Vec<u8>, SqbcError> {
    encode_sqbc_with_profile(ir, BuildProfile::Dev)
}

pub fn encode_sqbc_with_profile(
    ir: &IrProgram,
    profile: BuildProfile,
) -> Result<Vec<u8>, SqbcError> {
    let mut unit = CompileUnit::default();
    collect_strings(ir, &mut unit.strings, profile)?;

    for (index, state) in ir.state.iter().enumerate() {
        let id = u16::try_from(index).map_err(|_| SqbcError::new("too many state slots"))?;
        unit.states.insert(state.name.clone(), id);
    }
    for (index, function) in ir.functions.iter().enumerate() {
        let id = u16::try_from(index).map_err(|_| SqbcError::new("too many functions"))?;
        unit.functions.insert(function.name.clone(), id);
    }

    for function in &ir.functions {
        let name_id = unit.strings.intern(&function.name)?;
        let start = u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
        let mut frame = FrameCompiler::with_params(&function.params)?;
        compile_statements(&mut unit, &mut frame, &function.statements, profile)?;
        emit(&mut unit.code, OP_PUSH_NULL);
        emit(&mut unit.code, OP_RETURN);
        let end = u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
        unit.function_metas.push(FunctionMeta {
            name_id,
            param_count: u16::try_from(function.params.len())
                .map_err(|_| SqbcError::new("too many params"))?,
            local_count: frame.next_local,
            start,
            len: end - start,
        });
    }

    for handler in &ir.handlers {
        let event_id = unit.strings.intern(&handler.event)?;
        let start = u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
        let mut frame = FrameCompiler::default();
        compile_statements(&mut unit, &mut frame, &handler.statements, profile)?;
        emit(&mut unit.code, OP_HALT);
        let end = u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
        unit.handler_metas.push(HandlerMeta {
            event_id,
            preload: handler.preload,
            start,
            len: end - start,
        });
    }

    for screen in &ir.screens {
        let name_id = unit.strings.intern(&screen.name)?;
        let start = u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
        let mut frame = FrameCompiler::default();
        compile_statements(&mut unit, &mut frame, &screen.statements, profile)?;
        emit(&mut unit.code, OP_HALT);
        let end = u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
        unit.screen_metas.push(ScreenMeta {
            name_id,
            start,
            len: end - start,
        });
    }

    let sections = vec![
        (SECTION_APP_META, encode_app_meta(ir, &unit.strings)?),
        (
            SECTION_DEVICE_BINDINGS,
            encode_device_bindings(ir, &unit.strings)?,
        ),
        (SECTION_STRINGS, unit.strings.encode()?),
        (SECTION_STATE, encode_state_section(ir, &unit.strings)?),
        (SECTION_FUNCTIONS, encode_functions(&unit.function_metas)),
        (SECTION_TRIGGERS, encode_triggers(ir, &unit.strings)?),
        (SECTION_HANDLERS, encode_handlers(&unit.handler_metas)),
        (SECTION_SCREENS, encode_screens(&unit.screen_metas)),
        (SECTION_CODE, unit.code),
    ];
    encode_container(sections)
}

pub fn read_app_id(bytes: &[u8]) -> Result<Option<String>, SqbcError> {
    if bytes.len() < SQBC_HEADER_LEN || &bytes[0..4] != SQBC_MAGIC {
        return Err(SqbcError::new("invalid SQBC header"));
    }
    let header_len = read_u16_at(bytes, 4)? as usize;
    let file_len = read_u32_at(bytes, 6)? as usize;
    let section_count = read_u32_at(bytes, 10)? as usize;
    if file_len != bytes.len()
        || header_len != SQBC_HEADER_LEN + section_count * 12
        || header_len > bytes.len()
    {
        return Err(SqbcError::new("invalid SQBC header"));
    }
    let Some(meta) = section(bytes, section_count, SECTION_APP_META)? else {
        return Ok(None);
    };
    if meta.len() < 2 {
        return Err(SqbcError::new("invalid app metadata section"));
    }
    let app_id_len = read_u16_at(meta, 0)? as usize;
    let app_id_start = 2usize;
    let app_id_end = app_id_start
        .checked_add(app_id_len)
        .ok_or_else(|| SqbcError::new("invalid app metadata section"))?;
    if app_id_end > meta.len() {
        return Err(SqbcError::new("invalid app metadata section"));
    }
    let app_id = std::str::from_utf8(&meta[app_id_start..app_id_end])
        .map_err(|_| SqbcError::new("app id is not utf-8"))?;
    Ok(Some(app_id.to_string()))
}

fn collect_strings(
    ir: &IrProgram,
    strings: &mut StringTable,
    profile: BuildProfile,
) -> Result<(), SqbcError> {
    strings.intern(&ir.app.id)?;
    for binding in &ir.device_bindings {
        strings.intern(&binding.service)?;
        strings.intern(&binding.binding)?;
        strings.intern(&binding.resource)?;
    }
    for trigger in &ir.triggers {
        strings.intern(&trigger.event)?;
    }
    for state in &ir.state {
        strings.intern(&state.name)?;
        collect_json_value(&state.value, strings)?;
    }
    for function in &ir.functions {
        strings.intern(&function.name)?;
        for param in &function.params {
            strings.intern(param)?;
        }
        collect_statement_strings(&function.statements, strings, profile)?;
    }
    for handler in &ir.handlers {
        strings.intern(&handler.event)?;
        collect_statement_strings(&handler.statements, strings, profile)?;
    }
    for screen in &ir.screens {
        strings.intern(&screen.name)?;
        collect_statement_strings(&screen.statements, strings, profile)?;
    }
    Ok(())
}

fn collect_statement_strings(
    statements: &[IrStatement],
    strings: &mut StringTable,
    profile: BuildProfile,
) -> Result<(), SqbcError> {
    for statement in statements {
        match statement {
            IrStatement::ScreenOpen { screen } => {
                strings.intern(screen)?;
            }
            IrStatement::Assign { name, expr }
            | IrStatement::StateAssign { name, expr }
            | IrStatement::Let { name, expr } => {
                strings.intern(name)?;
                collect_expr_strings(expr, strings)?;
            }
            IrStatement::If {
                condition,
                then_statements,
                else_statements,
            } => {
                collect_expr_strings(condition, strings)?;
                collect_statement_strings(then_statements, strings, profile)?;
                collect_statement_strings(else_statements, strings, profile)?;
            }
            IrStatement::Repeat { count, statements } => {
                collect_expr_strings(count, strings)?;
                collect_statement_strings(statements, strings, profile)?;
            }
            IrStatement::Return { expr } => {
                if let Some(expr) = expr {
                    collect_expr_strings(expr, strings)?;
                }
            }
            IrStatement::Call { name, args } => {
                strings.intern(name)?;
                for arg in args {
                    collect_expr_strings(arg, strings)?;
                }
            }
            IrStatement::DebugPrint { args } => {
                if profile == BuildProfile::Dev {
                    for arg in args {
                        collect_expr_strings(arg, strings)?;
                    }
                }
            }
            IrStatement::DebugBlock { statements } => {
                if profile == BuildProfile::Dev {
                    collect_statement_strings(statements, strings, profile)?;
                }
            }
            IrStatement::AppLaunch { app } => {
                strings.intern(app)?;
            }
            IrStatement::AppArm { app } | IrStatement::AppDisarm { app } => {
                strings.intern(app)?;
            }
            IrStatement::ServiceTimerEvery { event, interval_ms } => {
                strings.intern(event)?;
                collect_expr_strings(interval_ms, strings)?;
            }
            IrStatement::ServiceTimerAfter { event, delay_ms } => {
                strings.intern(event)?;
                collect_expr_strings(delay_ms, strings)?;
            }
            IrStatement::ServicePowerSleep { wake_after_ms } => {
                collect_expr_strings(wake_after_ms, strings)?;
            }
            IrStatement::HardwareGpioWrite { name, value } => {
                strings.intern(name)?;
                collect_expr_strings(value, strings)?;
            }
            IrStatement::HardwareGpioToggle { name } => {
                strings.intern(name)?;
            }
            IrStatement::ServiceIndicatorWrite { value } => {
                collect_expr_strings(value, strings)?;
            }
            IrStatement::ServiceIndicatorToggle => {}
            IrStatement::ServiceIndicatorBreathe => {}
            IrStatement::ServiceIndicatorBlink { on_ms, off_ms } => {
                collect_expr_strings(on_ms, strings)?;
                collect_expr_strings(off_ms, strings)?;
            }
            IrStatement::DisplayText { text, options } => {
                collect_expr_strings(text, strings)?;
                collect_option_strings(options, strings)?;
            }
            IrStatement::DisplayClear { color } => {
                strings.intern(color)?;
            }
            IrStatement::DisplayRect { options, .. } | IrStatement::DisplayLine { options, .. } => {
                collect_option_strings(options, strings)?;
            }
            IrStatement::DisplaySelect { name } => {
                strings.intern(name)?;
            }
            IrStatement::DisplayImage { path, options } => {
                strings.intern(path)?;
                collect_option_strings(options, strings)?;
            }
            IrStatement::DisplayDraw { drawable, options } => {
                collect_expr_strings(drawable, strings)?;
                collect_option_strings(options, strings)?;
            }
            IrStatement::For {
                list,
                max,
                statements,
                ..
            } => {
                collect_expr_strings(list, strings)?;
                if let Some(max) = max {
                    collect_expr_strings(max, strings)?;
                }
                collect_statement_strings(statements, strings, profile)?;
            }
            IrStatement::StateLoad
            | IrStatement::StateSave
            | IrStatement::StateReset
            | IrStatement::ScreenRefresh
            | IrStatement::AppExit => {}
        }
    }
    Ok(())
}

fn collect_option_strings(
    value: &serde_json::Value,
    strings: &mut StringTable,
) -> Result<(), SqbcError> {
    match value {
        serde_json::Value::String(text) => {
            strings.intern(text)?;
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_option_strings(value, strings)?;
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                collect_option_strings(value, strings)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_expr_strings(expr: &IrExpr, strings: &mut StringTable) -> Result<(), SqbcError> {
    match expr {
        IrExpr::Literal { value } => collect_json_value(value, strings),
        IrExpr::State { name } => strings.intern(name).map(|_| ()),
        IrExpr::Variable { .. } => Ok(()),
        IrExpr::Binary { left, right, .. } => {
            collect_expr_strings(left, strings)?;
            collect_expr_strings(right, strings)
        }
        IrExpr::Unary { expr, .. } => collect_expr_strings(expr, strings),
        IrExpr::Field { target, field } => {
            collect_expr_strings(target, strings)?;
            strings.intern(field).map(|_| ())
        }
        IrExpr::HardwareGpioRead { name } | IrExpr::SystemStorage { name } => {
            strings.intern(name).map(|_| ())
        }
        IrExpr::ServiceIndicatorRead => Ok(()),
        IrExpr::SystemMemory | IrExpr::SystemStartReason => Ok(()),
        IrExpr::Call { name, args } => {
            strings.intern(name)?;
            for arg in args {
                collect_expr_strings(arg, strings)?;
            }
            Ok(())
        }
    }
}

fn collect_json_value(
    value: &serde_json::Value,
    strings: &mut StringTable,
) -> Result<(), SqbcError> {
    if let Some(text) = value.as_str() {
        strings.intern(text)?;
    }
    Ok(())
}

fn compile_statements(
    unit: &mut CompileUnit,
    frame: &mut FrameCompiler,
    statements: &[IrStatement],
    profile: BuildProfile,
) -> Result<(), SqbcError> {
    compile_statements_with_mode(unit, frame, statements, profile, StatementMode::Normal)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatementMode {
    Normal,
    DebugBlock,
}

fn compile_statements_with_mode(
    unit: &mut CompileUnit,
    frame: &mut FrameCompiler,
    statements: &[IrStatement],
    profile: BuildProfile,
    mode: StatementMode,
) -> Result<(), SqbcError> {
    for statement in statements {
        compile_statement(unit, frame, statement, profile, mode)?;
    }
    Ok(())
}

fn compile_statement(
    unit: &mut CompileUnit,
    frame: &mut FrameCompiler,
    statement: &IrStatement,
    profile: BuildProfile,
    mode: StatementMode,
) -> Result<(), SqbcError> {
    match statement {
        IrStatement::StateLoad => emit_builtin(&mut unit.code, BUILTIN_STATE_LOAD),
        IrStatement::StateSave => emit_builtin(&mut unit.code, BUILTIN_STATE_SAVE),
        IrStatement::StateReset => emit_builtin(&mut unit.code, BUILTIN_STATE_RESET),
        IrStatement::AppExit => emit_builtin(&mut unit.code, BUILTIN_APP_EXIT),
        IrStatement::Assign { name, expr } => {
            compile_expr(unit, frame, expr)?;
            let local = frame
                .local(name)
                .ok_or_else(|| SqbcError::new(format!("unknown local {name}")))?;
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, local);
        }
        IrStatement::StateAssign { name, expr } => {
            compile_expr(unit, frame, expr)?;
            let state = state_id(unit, name)?;
            emit(&mut unit.code, OP_SET_STATE);
            write_u16(&mut unit.code, state);
        }
        IrStatement::Let { name, expr } => {
            compile_expr(unit, frame, expr)?;
            let local = frame.define_local(name)?;
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, local);
        }
        IrStatement::If {
            condition,
            then_statements,
            else_statements,
        } => {
            compile_expr(unit, frame, condition)?;
            emit(&mut unit.code, OP_JUMP_IF_FALSE);
            let else_patch = reserve_u32(&mut unit.code);
            compile_statements_with_mode(unit, frame, then_statements, profile, mode)?;
            emit(&mut unit.code, OP_JUMP);
            let end_patch = reserve_u32(&mut unit.code);
            patch_u32(&mut unit.code, else_patch)?;
            compile_statements_with_mode(unit, frame, else_statements, profile, mode)?;
            patch_u32(&mut unit.code, end_patch)?;
        }
        IrStatement::Repeat { count, statements } => {
            compile_expr(unit, frame, count)?;
            let counter = frame.define_local("__repeat_counter")?;
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, counter);
            let start =
                u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, counter);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, 0);
            emit(&mut unit.code, OP_GT);
            emit(&mut unit.code, OP_JUMP_IF_FALSE);
            let end_patch = reserve_u32(&mut unit.code);
            compile_statements_with_mode(unit, frame, statements, profile, mode)?;
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, counter);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, 1);
            emit(&mut unit.code, OP_SUB);
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, counter);
            emit(&mut unit.code, OP_JUMP);
            write_u32(&mut unit.code, start);
            patch_u32(&mut unit.code, end_patch)?;
        }
        IrStatement::Return { expr } => {
            if let Some(expr) = expr {
                compile_expr(unit, frame, expr)?;
            } else {
                emit(&mut unit.code, OP_PUSH_NULL);
            }
            emit(&mut unit.code, OP_RETURN);
        }
        IrStatement::Call { name, args } => {
            for arg in args {
                compile_expr(unit, frame, arg)?;
            }
            if let Some(builtin) = builtin_for_call(name) {
                validate_builtin_arg_count(name, args.len())?;
                emit_builtin(&mut unit.code, builtin);
            } else {
                let function = function_id(unit, name)?;
                emit(&mut unit.code, OP_CALL_FUNCTION);
                write_u16(&mut unit.code, function);
                write_u16(
                    &mut unit.code,
                    u16::try_from(args.len()).map_err(|_| SqbcError::new("too many args"))?,
                );
            }
            emit(&mut unit.code, OP_POP);
        }
        IrStatement::DebugPrint { args } => {
            if profile == BuildProfile::Dev {
                for arg in args {
                    compile_expr(unit, frame, arg)?;
                }
                emit(&mut unit.code, OP_CALL_BUILTIN);
                emit(&mut unit.code, BUILTIN_DEBUG_PRINT);
                emit(
                    &mut unit.code,
                    u8::try_from(args.len()).map_err(|_| SqbcError::new("too many args"))?,
                );
            }
        }
        IrStatement::DebugBlock { statements } => {
            if profile == BuildProfile::Dev {
                let mut debug_frame = frame.child_scope();
                compile_statements_with_mode(
                    unit,
                    &mut debug_frame,
                    statements,
                    profile,
                    StatementMode::DebugBlock,
                )?;
                frame.reserve_child_locals(&debug_frame);
            }
        }
        IrStatement::AppLaunch { app } => {
            emit_string(unit, app)?;
            emit_builtin(&mut unit.code, BUILTIN_APP_LAUNCH);
        }
        IrStatement::AppArm { app } => {
            emit_string(unit, app)?;
            emit_builtin(&mut unit.code, BUILTIN_APP_ARM);
        }
        IrStatement::AppDisarm { app } => {
            emit_string(unit, app)?;
            emit_builtin(&mut unit.code, BUILTIN_APP_DISARM);
        }
        IrStatement::ServiceTimerEvery { event, interval_ms } => {
            emit_string(unit, event)?;
            compile_expr(unit, frame, interval_ms)?;
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_TIMER_EVERY);
        }
        IrStatement::ServiceTimerAfter { event, delay_ms } => {
            emit_string(unit, event)?;
            compile_expr(unit, frame, delay_ms)?;
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_TIMER_AFTER);
        }
        IrStatement::ServicePowerSleep { wake_after_ms } => {
            compile_expr(unit, frame, wake_after_ms)?;
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_POWER_SLEEP);
        }
        IrStatement::HardwareGpioWrite { name, value } => {
            compile_expr(unit, frame, value)?;
            emit_string(unit, name)?;
            emit_builtin(&mut unit.code, BUILTIN_HARDWARE_GPIO_WRITE);
        }
        IrStatement::HardwareGpioToggle { name } => {
            emit_string(unit, name)?;
            emit_builtin(&mut unit.code, BUILTIN_HARDWARE_GPIO_TOGGLE);
        }
        IrStatement::ServiceIndicatorWrite { value } => {
            compile_expr(unit, frame, value)?;
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_INDICATOR_WRITE);
        }
        IrStatement::ServiceIndicatorToggle => {
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_INDICATOR_TOGGLE);
        }
        IrStatement::ServiceIndicatorBreathe => {
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_INDICATOR_BREATHE);
        }
        IrStatement::ServiceIndicatorBlink { on_ms, off_ms } => {
            compile_expr(unit, frame, on_ms)?;
            compile_expr(unit, frame, off_ms)?;
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_INDICATOR_BLINK);
        }
        IrStatement::ScreenOpen { screen } => {
            let screen_id = unit.strings.intern(screen)?;
            emit(&mut unit.code, OP_PUSH_STRING);
            write_u16(&mut unit.code, screen_id);
            emit_builtin(&mut unit.code, BUILTIN_SCREEN_OPEN);
        }
        IrStatement::ScreenRefresh => {
            emit_builtin(&mut unit.code, BUILTIN_SCREEN_REFRESH);
        }
        IrStatement::For {
            item,
            list,
            max,
            statements,
        } => {
            compile_expr(unit, frame, list)?;
            let list_local = frame.define_local("__for_list")?;
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, list_local);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, 0);
            let index_local = frame.define_local("__for_index")?;
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, index_local);
            if let Some(max) = max {
                compile_expr(unit, frame, max)?;
            } else {
                emit(&mut unit.code, OP_PUSH_INT);
                write_i32(&mut unit.code, i32::MAX);
            }
            let max_local = frame.define_local("__for_max")?;
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, max_local);
            let item_local = frame.define_local(item)?;
            let start =
                u32::try_from(unit.code.len()).map_err(|_| SqbcError::new("code too large"))?;
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, index_local);
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, list_local);
            emit(&mut unit.code, OP_LIST_LEN);
            emit(&mut unit.code, OP_LT);
            emit(&mut unit.code, OP_JUMP_IF_FALSE);
            let end_patch = reserve_u32(&mut unit.code);
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, index_local);
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, max_local);
            emit(&mut unit.code, OP_LT);
            emit(&mut unit.code, OP_JUMP_IF_FALSE);
            let max_end_patch = reserve_u32(&mut unit.code);
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, list_local);
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, index_local);
            emit(&mut unit.code, OP_LIST_GET);
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, item_local);
            compile_statements_with_mode(unit, frame, statements, profile, mode)?;
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, index_local);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, 1);
            emit(&mut unit.code, OP_ADD);
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, index_local);
            emit(&mut unit.code, OP_JUMP);
            write_u32(&mut unit.code, start);
            patch_u32(&mut unit.code, end_patch)?;
            patch_u32(&mut unit.code, max_end_patch)?;
        }
        IrStatement::DisplayClear { color } => {
            let color_id = unit.strings.intern(color)?;
            emit(&mut unit.code, OP_PUSH_STRING);
            write_u16(&mut unit.code, color_id);
            emit_builtin(&mut unit.code, BUILTIN_DISPLAY_CLEAR);
        }
        IrStatement::DisplayText { text, options } => {
            compile_expr(unit, frame, text)?;
            emit_i32_option(unit, frame, options, "x")?;
            emit_i32_option(unit, frame, options, "y")?;
            emit_i32_option(unit, frame, options, "w")?;
            emit_i32_option(unit, frame, options, "h")?;
            emit_i32_option(unit, frame, options, "fontHeight")?;
            emit_string_option(unit, options, "textColor")?;
            emit_string_option(unit, options, "backgroundColor")?;
            emit_string_option(unit, options, "align")?;
            emit_string_option(unit, options, "valign")?;
            emit_builtin(&mut unit.code, BUILTIN_DISPLAY_TEXT);
        }
        IrStatement::DisplayRect {
            x,
            y,
            w,
            h,
            options,
        } => {
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *x as i32);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *y as i32);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *w as i32);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *h as i32);
            emit_string_option(unit, options, "fillColor")?;
            emit_string_option(unit, options, "strokeColor")?;
            emit_builtin(&mut unit.code, BUILTIN_DISPLAY_RECT);
        }
        IrStatement::DisplayLine {
            x1,
            y1,
            x2,
            y2,
            options,
        } => {
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *x1 as i32);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *y1 as i32);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *x2 as i32);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, *y2 as i32);
            emit_string_option(unit, options, "color")?;
            emit_builtin(&mut unit.code, BUILTIN_DISPLAY_LINE);
        }
        IrStatement::DisplaySelect { name } => {
            let name_id = unit.strings.intern(name)?;
            emit(&mut unit.code, OP_PUSH_STRING);
            write_u16(&mut unit.code, name_id);
            emit_builtin(&mut unit.code, BUILTIN_DISPLAY_SELECT);
        }
        IrStatement::DisplayImage { path, options } => {
            let path_id = unit.strings.intern(path)?;
            emit(&mut unit.code, OP_PUSH_STRING);
            write_u16(&mut unit.code, path_id);
            emit_i32_option(unit, frame, options, "x")?;
            emit_i32_option(unit, frame, options, "y")?;
            emit_i32_option(unit, frame, options, "w")?;
            emit_i32_option(unit, frame, options, "h")?;
            emit_builtin(&mut unit.code, BUILTIN_DISPLAY_IMAGE);
        }
        IrStatement::DisplayDraw { drawable, options } => {
            compile_expr(unit, frame, drawable)?;
            emit_i32_option(unit, frame, options, "x")?;
            emit_i32_option(unit, frame, options, "y")?;
            emit_i32_option(unit, frame, options, "w")?;
            emit_i32_option(unit, frame, options, "h")?;
            emit_builtin(&mut unit.code, BUILTIN_DISPLAY_DRAW);
        }
    }
    Ok(())
}

fn emit_i32_option(
    unit: &mut CompileUnit,
    frame: &FrameCompiler,
    options: &serde_json::Value,
    key: &str,
) -> Result<(), SqbcError> {
    let Some(value) = options.get(key) else {
        emit(&mut unit.code, OP_PUSH_INT);
        write_i32(&mut unit.code, 0);
        return Ok(());
    };

    if let Some(literal) = expr_literal_i64(value) {
        emit(&mut unit.code, OP_PUSH_INT);
        write_i32(
            &mut unit.code,
            i32::try_from(literal)
                .map_err(|_| SqbcError::new("display option out of i32 range"))?,
        );
        return Ok(());
    }

    let expr = serde_json::from_value::<IrExpr>(value.clone())
        .map_err(|_| SqbcError::new("display numeric option must be an expression"))?;
    compile_expr(unit, frame, &expr)?;
    Ok(())
}

fn emit_string_option(
    unit: &mut CompileUnit,
    options: &serde_json::Value,
    key: &str,
) -> Result<(), SqbcError> {
    if let Some(value) = options.get(key).and_then(expr_literal_string) {
        let id = unit.strings.intern(value)?;
        emit(&mut unit.code, OP_PUSH_STRING);
        write_u16(&mut unit.code, id);
    } else {
        emit(&mut unit.code, OP_PUSH_NULL);
    }
    Ok(())
}

fn expr_literal_i64(value: &serde_json::Value) -> Option<i64> {
    if let Some(number) = value.as_i64() {
        return Some(number);
    }
    if let serde_json::Value::Object(object) = value {
        if object.get("op")?.as_str()? == "literal" {
            return object.get("value")?.as_i64();
        }
    }
    None
}

fn expr_literal_string(value: &serde_json::Value) -> Option<&str> {
    if let Some(text) = value.as_str() {
        return Some(text);
    }
    if let serde_json::Value::Object(object) = value {
        if object.get("op").and_then(serde_json::Value::as_str) == Some("literal") {
            return object.get("value").and_then(serde_json::Value::as_str);
        }
    }
    None
}

fn compile_expr(
    unit: &mut CompileUnit,
    frame: &FrameCompiler,
    expr: &IrExpr,
) -> Result<(), SqbcError> {
    match expr {
        IrExpr::Literal { value } => compile_literal(&mut unit.code, &mut unit.strings, value),
        IrExpr::State { name } => {
            let state = state_id(unit, name)?;
            emit(&mut unit.code, OP_GET_STATE);
            write_u16(&mut unit.code, state);
            Ok(())
        }
        IrExpr::Variable { name } => {
            let local = frame
                .local(name)
                .ok_or_else(|| SqbcError::new(format!("unknown local {name}")))?;
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, local);
            Ok(())
        }
        IrExpr::Binary {
            left,
            operator,
            right,
        } => {
            compile_expr(unit, frame, left)?;
            compile_expr(unit, frame, right)?;
            emit(&mut unit.code, opcode_for_operator(operator)?);
            Ok(())
        }
        IrExpr::Unary { operator, expr } => {
            let _ = (operator, expr);
            Err(SqbcError::new(
                "unary result-record expressions are not in the reference firmware subset yet",
            ))
        }
        IrExpr::Field { target, field } => {
            compile_expr(unit, frame, target)?;
            let field_id = unit.strings.intern(field)?;
            emit(&mut unit.code, OP_GET_FIELD);
            write_u16(&mut unit.code, field_id);
            Ok(())
        }
        IrExpr::HardwareGpioRead { name } => {
            emit_string(unit, name)?;
            emit_builtin(&mut unit.code, BUILTIN_HARDWARE_GPIO_READ);
            Ok(())
        }
        IrExpr::ServiceIndicatorRead => {
            emit_builtin(&mut unit.code, BUILTIN_SERVICE_INDICATOR_READ);
            Ok(())
        }
        IrExpr::SystemMemory => {
            emit_builtin(&mut unit.code, BUILTIN_SYSTEM_MEMORY);
            Ok(())
        }
        IrExpr::SystemStorage { name } => {
            emit_string(unit, name)?;
            emit_builtin(&mut unit.code, BUILTIN_SYSTEM_STORAGE);
            Ok(())
        }
        IrExpr::SystemStartReason => {
            emit_builtin(&mut unit.code, BUILTIN_SYSTEM_START_REASON);
            Ok(())
        }
        IrExpr::Call { name, args } => {
            for arg in args {
                compile_expr(unit, frame, arg)?;
            }
            if let Some(builtin) = builtin_for_call(name) {
                validate_builtin_arg_count(name, args.len())?;
                emit_builtin(&mut unit.code, builtin);
            } else {
                let function = function_id(unit, name)?;
                emit(&mut unit.code, OP_CALL_FUNCTION);
                write_u16(&mut unit.code, function);
                write_u16(
                    &mut unit.code,
                    u16::try_from(args.len()).map_err(|_| SqbcError::new("too many args"))?,
                );
            }
            Ok(())
        }
    }
}

fn emit_string(unit: &mut CompileUnit, value: &str) -> Result<(), SqbcError> {
    let id = unit.strings.intern(value)?;
    emit(&mut unit.code, OP_PUSH_STRING);
    write_u16(&mut unit.code, id);
    Ok(())
}

fn compile_literal(
    code: &mut Vec<u8>,
    strings: &mut StringTable,
    value: &serde_json::Value,
) -> Result<(), SqbcError> {
    if value.is_null() {
        emit(code, OP_PUSH_NULL);
    } else if let Some(value) = value.as_bool() {
        emit(code, OP_PUSH_BOOL);
        emit(code, u8::from(value));
    } else if let Some(value) = value.as_i64() {
        emit(code, OP_PUSH_INT);
        write_i32(
            code,
            i32::try_from(value).map_err(|_| SqbcError::new("integer literal out of i32 range"))?,
        );
    } else if let Some(value) = value.as_str() {
        let id = strings.intern(value)?;
        emit(code, OP_PUSH_STRING);
        write_u16(code, id);
    } else {
        return Err(SqbcError::new(
            "unsupported literal in reference bytecode subset",
        ));
    }
    Ok(())
}

fn opcode_for_operator(operator: &str) -> Result<u8, SqbcError> {
    match operator {
        "+" => Ok(OP_ADD),
        "-" => Ok(OP_SUB),
        "==" => Ok(OP_EQ),
        "!=" => Ok(OP_NE),
        "<" => Ok(OP_LT),
        "<=" => Ok(OP_LTE),
        ">" => Ok(OP_GT),
        ">=" => Ok(OP_GTE),
        _ => Err(SqbcError::new(format!("unsupported operator {operator}"))),
    }
}

fn state_id(unit: &CompileUnit, name: &str) -> Result<u16, SqbcError> {
    unit.states
        .get(name)
        .copied()
        .ok_or_else(|| SqbcError::new(format!("unknown state {name}")))
}

fn function_id(unit: &CompileUnit, name: &str) -> Result<u16, SqbcError> {
    unit.functions
        .get(name)
        .copied()
        .ok_or_else(|| SqbcError::new(format!("unknown function {name}")))
}

fn emit_builtin(code: &mut Vec<u8>, builtin: u8) {
    emit(code, OP_CALL_BUILTIN);
    emit(code, builtin);
}

fn builtin_for_call(name: &str) -> Option<u8> {
    match name {
        "service.wifi.startAP" => Some(BUILTIN_SERVICE_WIFI_START_AP),
        "service.wifi.stopAP" => Some(BUILTIN_SERVICE_WIFI_STOP_AP),
        "service.wifi.status" => Some(BUILTIN_SERVICE_WIFI_STATUS),
        "service.wifi.getAPIP" => Some(BUILTIN_SERVICE_WIFI_GET_AP_IP),
        "service.wifi.connect" => Some(BUILTIN_SERVICE_WIFI_CONNECT),
        "service.wifi.disconnect" => Some(BUILTIN_SERVICE_WIFI_DISCONNECT),
        "service.wifi.scan" => Some(BUILTIN_SERVICE_WIFI_SCAN),
        "app.registry" => Some(BUILTIN_APP_REGISTRY_LIST),
        "app.registry.get" => Some(BUILTIN_APP_REGISTRY_GET),
        "app.processStack" => Some(BUILTIN_APP_PROCESS_STACK),
        "app.armedStack" => Some(BUILTIN_APP_ARMED_STACK),
        "app.armedStack.get" => Some(BUILTIN_APP_ARMED_STACK_GET),
        "device.config.load" => Some(BUILTIN_DEVICE_CONFIG_LOAD),
        "device.config.set" => Some(BUILTIN_DEVICE_CONFIG_SET),
        "device.config.rebind" => Some(BUILTIN_DEVICE_CONFIG_REBIND),
        "device.config.save" => Some(BUILTIN_DEVICE_CONFIG_SAVE),
        "file.pickFile" => Some(BUILTIN_FILE_PICK_FILE),
        "file.readText" => Some(BUILTIN_FILE_READ_TEXT),
        "file.readLines" => Some(BUILTIN_FILE_READ_LINES),
        "service.display.info" => Some(BUILTIN_DISPLAY_INFO),
        _ => None,
    }
}

fn validate_builtin_arg_count(name: &str, count: usize) -> Result<(), SqbcError> {
    let valid = match name {
        "service.wifi.startAP" | "service.wifi.connect" => count == 1,
        "service.wifi.stopAP"
        | "service.wifi.status"
        | "service.wifi.getAPIP"
        | "service.wifi.disconnect"
        | "service.wifi.scan"
        | "app.registry"
        | "app.processStack"
        | "app.armedStack" => count == 0,
        "app.registry.get" | "app.armedStack.get" => count == 2,
        "device.config.load" | "device.config.rebind" | "device.config.save" => count == 1,
        "device.config.set" => count == 2,
        "file.pickFile" => count == 1,
        "file.readText" => count == 1,
        "file.readLines" => count == 2,
        "service.display.info" => count == 0,
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(SqbcError::new(format!("invalid argument count for {name}")))
    }
}

fn encode_state_section(ir: &IrProgram, strings: &StringTable) -> Result<Vec<u8>, SqbcError> {
    let mut out = Vec::new();
    write_u16(
        &mut out,
        u16::try_from(ir.state.len()).map_err(|_| SqbcError::new("too many state slots"))?,
    );
    for state in &ir.state {
        write_u16(
            &mut out,
            *strings
                .ids
                .get(&state.name)
                .ok_or_else(|| SqbcError::new("missing state name string"))?,
        );
        emit(&mut out, state_type_tag(&state.value_type)?);
        emit(&mut out, u8::from(state.nullable));
        encode_value(&mut out, strings, &state.value)?;
    }
    Ok(out)
}

fn state_type_tag(value_type: &str) -> Result<u8, SqbcError> {
    match value_type {
        "int" => Ok(STATE_TYPE_INT),
        "bool" => Ok(STATE_TYPE_BOOL),
        "string" => Ok(STATE_TYPE_STRING),
        _ => Err(SqbcError::new("unsupported state type")),
    }
}

fn encode_value(
    out: &mut Vec<u8>,
    strings: &StringTable,
    value: &serde_json::Value,
) -> Result<(), SqbcError> {
    if value.is_null() {
        emit(out, VALUE_NULL);
    } else if let Some(value) = value.as_bool() {
        emit(out, VALUE_BOOL);
        emit(out, u8::from(value));
    } else if let Some(value) = value.as_i64() {
        emit(out, VALUE_I32);
        write_i32(
            out,
            i32::try_from(value).map_err(|_| SqbcError::new("state integer out of i32 range"))?,
        );
    } else if let Some(value) = value.as_str() {
        emit(out, VALUE_STRING);
        write_u16(
            out,
            *strings
                .ids
                .get(value)
                .ok_or_else(|| SqbcError::new("missing string value"))?,
        );
    } else {
        return Err(SqbcError::new("unsupported state value"));
    }
    Ok(())
}

fn encode_functions(functions: &[FunctionMeta]) -> Vec<u8> {
    let mut out = Vec::new();
    write_u16(&mut out, functions.len() as u16);
    for function in functions {
        write_u16(&mut out, function.name_id);
        write_u16(&mut out, function.param_count);
        write_u16(&mut out, function.local_count);
        write_u32(&mut out, function.start);
        write_u32(&mut out, function.len);
    }
    out
}

fn encode_handlers(handlers: &[HandlerMeta]) -> Vec<u8> {
    let mut out = Vec::new();
    write_u16(&mut out, handlers.len() as u16);
    for handler in handlers {
        write_u16(&mut out, handler.event_id);
        write_u16(&mut out, u16::from(handler.preload));
        write_u32(&mut out, handler.start);
        write_u32(&mut out, handler.len);
    }
    out
}

fn encode_triggers(ir: &IrProgram, strings: &StringTable) -> Result<Vec<u8>, SqbcError> {
    let mut out = Vec::new();
    write_u16(
        &mut out,
        u16::try_from(ir.triggers.len()).map_err(|_| SqbcError::new("too many triggers"))?,
    );
    for trigger in &ir.triggers {
        let event_id = strings
            .ids
            .get(&trigger.event)
            .copied()
            .ok_or_else(|| SqbcError::new("unknown trigger event"))?;
        write_u16(&mut out, event_id);
        out.push(u8::from(trigger.repeating));
        out.push(0);
        write_i32(&mut out, trigger.interval_ms);
    }
    Ok(out)
}

fn encode_screens(screens: &[ScreenMeta]) -> Vec<u8> {
    let mut out = Vec::new();
    write_u16(&mut out, screens.len() as u16);
    for screen in screens {
        write_u16(&mut out, screen.name_id);
        write_u32(&mut out, screen.start);
        write_u32(&mut out, screen.len);
    }
    out
}

fn encode_app_meta(ir: &IrProgram, _strings: &StringTable) -> Result<Vec<u8>, SqbcError> {
    let mut out = Vec::new();
    let app_id = ir.app.id.as_bytes();
    write_u16(
        &mut out,
        u16::try_from(app_id.len()).map_err(|_| SqbcError::new("app id too long"))?,
    );
    out.extend_from_slice(app_id);
    let state_store = ir.state_store.as_bytes();
    write_u16(
        &mut out,
        u16::try_from(state_store.len()).map_err(|_| SqbcError::new("state store too long"))?,
    );
    out.extend_from_slice(state_store);
    Ok(out)
}

fn encode_device_bindings(ir: &IrProgram, strings: &StringTable) -> Result<Vec<u8>, SqbcError> {
    let mut out = Vec::new();
    write_u16(
        &mut out,
        u16::try_from(ir.device_bindings.len())
            .map_err(|_| SqbcError::new("too many device bindings"))?,
    );
    for binding in &ir.device_bindings {
        write_u16(
            &mut out,
            *strings
                .ids
                .get(&binding.service)
                .ok_or_else(|| SqbcError::new("missing device service string"))?,
        );
        write_u16(
            &mut out,
            *strings
                .ids
                .get(&binding.binding)
                .ok_or_else(|| SqbcError::new("missing device binding string"))?,
        );
        write_u16(
            &mut out,
            *strings
                .ids
                .get(&binding.resource)
                .ok_or_else(|| SqbcError::new("missing device resource string"))?,
        );
    }
    Ok(out)
}

fn encode_container(sections: Vec<(u16, Vec<u8>)>) -> Result<Vec<u8>, SqbcError> {
    let section_count =
        u32::try_from(sections.len()).map_err(|_| SqbcError::new("too many sections"))?;
    let header_len = SQBC_HEADER_LEN + sections.len() * 12usize;
    let mut offset = u32::try_from(header_len).map_err(|_| SqbcError::new("header too large"))?;
    let mut records = Vec::new();
    for (kind, data) in &sections {
        records.push((
            *kind,
            offset,
            u32::try_from(data.len()).map_err(|_| SqbcError::new("section too large"))?,
        ));
        offset = offset
            .checked_add(
                u32::try_from(data.len()).map_err(|_| SqbcError::new("section too large"))?,
            )
            .ok_or_else(|| SqbcError::new("file too large"))?;
    }

    let mut out = Vec::with_capacity(offset as usize);
    out.extend_from_slice(SQBC_MAGIC);
    write_u16(
        &mut out,
        u16::try_from(header_len).map_err(|_| SqbcError::new("header too large"))?,
    );
    write_u32(&mut out, offset);
    write_u32(&mut out, section_count);
    for (kind, section_offset, len) in records {
        write_u16(&mut out, kind);
        write_u16(&mut out, 0);
        write_u32(&mut out, section_offset);
        write_u32(&mut out, len);
    }
    for (_, data) in sections {
        out.extend_from_slice(&data);
    }
    Ok(out)
}

fn emit(out: &mut Vec<u8>, byte: u8) {
    out.push(byte);
}

fn reserve_u32(out: &mut Vec<u8>) -> usize {
    let offset = out.len();
    write_u32(out, 0);
    offset
}

fn patch_u32(out: &mut [u8], offset: usize) -> Result<(), SqbcError> {
    let value = u32::try_from(out.len()).map_err(|_| SqbcError::new("code too large"))?;
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Result<u16, SqbcError> {
    let Some(slice) = bytes.get(offset..offset + 2) else {
        return Err(SqbcError::new("unexpected end of SQBC"));
    };
    Ok(u16::from_le_bytes(slice.try_into().unwrap()))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Result<u32, SqbcError> {
    let Some(slice) = bytes.get(offset..offset + 4) else {
        return Err(SqbcError::new("unexpected end of SQBC"));
    };
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

fn section(bytes: &[u8], section_count: usize, kind: u16) -> Result<Option<&[u8]>, SqbcError> {
    for index in 0..section_count {
        let record_offset = SQBC_HEADER_LEN + index * 12;
        if read_u16_at(bytes, record_offset)? != kind {
            continue;
        }
        let offset = read_u32_at(bytes, record_offset + 4)? as usize;
        let len = read_u32_at(bytes, record_offset + 8)? as usize;
        let end = offset
            .checked_add(len)
            .ok_or_else(|| SqbcError::new("invalid section bounds"))?;
        let Some(section) = bytes.get(offset..end) else {
            return Err(SqbcError::new("invalid section bounds"));
        };
        return Ok(Some(section));
    }
    Ok(None)
}
