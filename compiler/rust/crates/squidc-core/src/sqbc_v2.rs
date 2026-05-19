use crate::{IrExpr, IrProgram, IrStatement};
use std::collections::BTreeMap;

pub const SQBC_V2_MAGIC: &[u8; 4] = b"SQBC";
pub const SQBC_V2_VERSION: u16 = 2;

const SECTION_STRINGS: u16 = 1;
const SECTION_STATE: u16 = 2;
const SECTION_FUNCTIONS: u16 = 3;
const SECTION_HANDLERS: u16 = 4;
const SECTION_CODE: u16 = 5;

const OP_PUSH_INT: u8 = 1;
const OP_PUSH_BOOL: u8 = 2;
const OP_PUSH_STRING: u8 = 3;
const OP_PUSH_NULL: u8 = 4;
const OP_GET_STATE: u8 = 10;
const OP_SET_STATE: u8 = 11;
const OP_GET_LOCAL: u8 = 12;
const OP_SET_LOCAL: u8 = 13;
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

const BUILTIN_STATE_LOAD: u8 = 1;
const BUILTIN_STATE_SAVE: u8 = 2;
const BUILTIN_APP_EXIT: u8 = 3;

const VALUE_NULL: u8 = 0;
const VALUE_BOOL: u8 = 1;
const VALUE_I32: u8 = 2;
const VALUE_STRING: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqbcV2Error {
    pub message: String,
}

impl SqbcV2Error {
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
    fn intern(&mut self, value: &str) -> Result<u16, SqbcV2Error> {
        if let Some(id) = self.ids.get(value) {
            return Ok(*id);
        }
        let id =
            u16::try_from(self.values.len()).map_err(|_| SqbcV2Error::new("too many strings"))?;
        self.values.push(value.to_string());
        self.ids.insert(value.to_string(), id);
        Ok(id)
    }

    fn encode(&self) -> Result<Vec<u8>, SqbcV2Error> {
        let mut out = Vec::new();
        write_u16(
            &mut out,
            u16::try_from(self.values.len()).map_err(|_| SqbcV2Error::new("too many strings"))?,
        );
        for value in &self.values {
            let bytes = value.as_bytes();
            write_u16(
                &mut out,
                u16::try_from(bytes.len()).map_err(|_| SqbcV2Error::new("string too long"))?,
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
    code: Vec<u8>,
}

#[derive(Default)]
struct FrameCompiler {
    locals: BTreeMap<String, u16>,
    next_local: u16,
}

impl FrameCompiler {
    fn with_params(params: &[String]) -> Result<Self, SqbcV2Error> {
        let mut frame = Self::default();
        for param in params {
            frame.define_local(param)?;
        }
        Ok(frame)
    }

    fn define_local(&mut self, name: &str) -> Result<u16, SqbcV2Error> {
        if let Some(id) = self.locals.get(name) {
            return Ok(*id);
        }
        let id = self.next_local;
        self.next_local = self
            .next_local
            .checked_add(1)
            .ok_or_else(|| SqbcV2Error::new("too many locals"))?;
        self.locals.insert(name.to_string(), id);
        Ok(id)
    }

    fn local(&self, name: &str) -> Option<u16> {
        self.locals.get(name).copied()
    }
}

pub fn encode_sqbc_v2(ir: &IrProgram) -> Result<Vec<u8>, SqbcV2Error> {
    let mut unit = CompileUnit::default();
    collect_strings(ir, &mut unit.strings)?;

    for (index, state) in ir.state.iter().enumerate() {
        let id = u16::try_from(index).map_err(|_| SqbcV2Error::new("too many state slots"))?;
        unit.states.insert(state.name.clone(), id);
    }
    for (index, function) in ir.functions.iter().enumerate() {
        let id = u16::try_from(index).map_err(|_| SqbcV2Error::new("too many functions"))?;
        unit.functions.insert(function.name.clone(), id);
    }

    for function in &ir.functions {
        let name_id = unit.strings.intern(&function.name)?;
        let start =
            u32::try_from(unit.code.len()).map_err(|_| SqbcV2Error::new("code too large"))?;
        let mut frame = FrameCompiler::with_params(&function.params)?;
        compile_statements(&mut unit, &mut frame, &function.statements)?;
        emit(&mut unit.code, OP_PUSH_NULL);
        emit(&mut unit.code, OP_RETURN);
        let end = u32::try_from(unit.code.len()).map_err(|_| SqbcV2Error::new("code too large"))?;
        unit.function_metas.push(FunctionMeta {
            name_id,
            param_count: u16::try_from(function.params.len())
                .map_err(|_| SqbcV2Error::new("too many params"))?,
            local_count: frame.next_local,
            start,
            len: end - start,
        });
    }

    for handler in &ir.handlers {
        let event_id = unit.strings.intern(&handler.event)?;
        let start =
            u32::try_from(unit.code.len()).map_err(|_| SqbcV2Error::new("code too large"))?;
        let mut frame = FrameCompiler::default();
        compile_statements(&mut unit, &mut frame, &handler.statements)?;
        emit(&mut unit.code, OP_HALT);
        let end = u32::try_from(unit.code.len()).map_err(|_| SqbcV2Error::new("code too large"))?;
        unit.handler_metas.push(HandlerMeta {
            event_id,
            start,
            len: end - start,
        });
    }

    let sections = vec![
        (SECTION_STRINGS, unit.strings.encode()?),
        (SECTION_STATE, encode_state_section(ir, &unit.strings)?),
        (SECTION_FUNCTIONS, encode_functions(&unit.function_metas)),
        (SECTION_HANDLERS, encode_handlers(&unit.handler_metas)),
        (SECTION_CODE, unit.code),
    ];
    encode_container(sections)
}

fn collect_strings(ir: &IrProgram, strings: &mut StringTable) -> Result<(), SqbcV2Error> {
    strings.intern(&ir.app.id)?;
    for state in &ir.state {
        strings.intern(&state.name)?;
        collect_json_value(&state.value, strings)?;
    }
    for function in &ir.functions {
        strings.intern(&function.name)?;
        for param in &function.params {
            strings.intern(param)?;
        }
        collect_statement_strings(&function.statements, strings)?;
    }
    for handler in &ir.handlers {
        strings.intern(&handler.event)?;
        collect_statement_strings(&handler.statements, strings)?;
    }
    Ok(())
}

fn collect_statement_strings(
    statements: &[IrStatement],
    strings: &mut StringTable,
) -> Result<(), SqbcV2Error> {
    for statement in statements {
        match statement {
            IrStatement::ScreenOpen { screen } => {
                strings.intern(screen)?;
            }
            IrStatement::Assign { name, expr } | IrStatement::Let { name, expr } => {
                strings.intern(name)?;
                collect_expr_strings(expr, strings)?;
            }
            IrStatement::If {
                condition,
                then_statements,
                else_statements,
            } => {
                collect_expr_strings(condition, strings)?;
                collect_statement_strings(then_statements, strings)?;
                collect_statement_strings(else_statements, strings)?;
            }
            IrStatement::Repeat { count, statements } => {
                collect_expr_strings(count, strings)?;
                collect_statement_strings(statements, strings)?;
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
            IrStatement::StateLoad
            | IrStatement::StateSave
            | IrStatement::ScreenRefresh
            | IrStatement::AppExit
            | IrStatement::For { .. }
            | IrStatement::DisplayClear { .. }
            | IrStatement::DisplayText { .. }
            | IrStatement::DisplayRect { .. }
            | IrStatement::DisplayLine { .. } => {}
        }
    }
    Ok(())
}

fn collect_expr_strings(expr: &IrExpr, strings: &mut StringTable) -> Result<(), SqbcV2Error> {
    match expr {
        IrExpr::Literal { value } => collect_json_value(value, strings),
        IrExpr::State { name } => strings.intern(name).map(|_| ()),
        IrExpr::Binary { left, right, .. } => {
            collect_expr_strings(left, strings)?;
            collect_expr_strings(right, strings)
        }
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
) -> Result<(), SqbcV2Error> {
    if let Some(text) = value.as_str() {
        strings.intern(text)?;
    }
    Ok(())
}

fn compile_statements(
    unit: &mut CompileUnit,
    frame: &mut FrameCompiler,
    statements: &[IrStatement],
) -> Result<(), SqbcV2Error> {
    for statement in statements {
        compile_statement(unit, frame, statement)?;
    }
    Ok(())
}

fn compile_statement(
    unit: &mut CompileUnit,
    frame: &mut FrameCompiler,
    statement: &IrStatement,
) -> Result<(), SqbcV2Error> {
    match statement {
        IrStatement::StateLoad => emit_builtin(&mut unit.code, BUILTIN_STATE_LOAD),
        IrStatement::StateSave => emit_builtin(&mut unit.code, BUILTIN_STATE_SAVE),
        IrStatement::AppExit => emit_builtin(&mut unit.code, BUILTIN_APP_EXIT),
        IrStatement::Assign { name, expr } => {
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
            compile_statements(unit, frame, then_statements)?;
            emit(&mut unit.code, OP_JUMP);
            let end_patch = reserve_u32(&mut unit.code);
            patch_u32(&mut unit.code, else_patch)?;
            compile_statements(unit, frame, else_statements)?;
            patch_u32(&mut unit.code, end_patch)?;
        }
        IrStatement::Repeat { count, statements } => {
            compile_expr(unit, frame, count)?;
            let counter = frame.define_local("__repeat_counter")?;
            emit(&mut unit.code, OP_SET_LOCAL);
            write_u16(&mut unit.code, counter);
            let start =
                u32::try_from(unit.code.len()).map_err(|_| SqbcV2Error::new("code too large"))?;
            emit(&mut unit.code, OP_GET_LOCAL);
            write_u16(&mut unit.code, counter);
            emit(&mut unit.code, OP_PUSH_INT);
            write_i32(&mut unit.code, 0);
            emit(&mut unit.code, OP_GT);
            emit(&mut unit.code, OP_JUMP_IF_FALSE);
            let end_patch = reserve_u32(&mut unit.code);
            compile_statements(unit, frame, statements)?;
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
            let function = function_id(unit, name)?;
            emit(&mut unit.code, OP_CALL_FUNCTION);
            write_u16(&mut unit.code, function);
            write_u16(
                &mut unit.code,
                u16::try_from(args.len()).map_err(|_| SqbcV2Error::new("too many args"))?,
            );
            emit(&mut unit.code, OP_POP);
        }
        IrStatement::ScreenOpen { .. } | IrStatement::ScreenRefresh => {}
        IrStatement::For { .. } => {
            return Err(SqbcV2Error::new(
                "for loops are not in the reference firmware subset yet",
            ))
        }
        IrStatement::DisplayClear { .. }
        | IrStatement::DisplayText { .. }
        | IrStatement::DisplayRect { .. }
        | IrStatement::DisplayLine { .. } => {
            return Err(SqbcV2Error::new(
                "display statements are not in the reference firmware subset",
            ))
        }
    }
    Ok(())
}

fn compile_expr(
    unit: &mut CompileUnit,
    frame: &FrameCompiler,
    expr: &IrExpr,
) -> Result<(), SqbcV2Error> {
    match expr {
        IrExpr::Literal { value } => compile_literal(&mut unit.code, &mut unit.strings, value),
        IrExpr::State { name } => {
            if let Some(local) = frame.local(name) {
                emit(&mut unit.code, OP_GET_LOCAL);
                write_u16(&mut unit.code, local);
            } else {
                let state = state_id(unit, name)?;
                emit(&mut unit.code, OP_GET_STATE);
                write_u16(&mut unit.code, state);
            }
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
        IrExpr::Call { name, args } => {
            for arg in args {
                compile_expr(unit, frame, arg)?;
            }
            let function = function_id(unit, name)?;
            emit(&mut unit.code, OP_CALL_FUNCTION);
            write_u16(&mut unit.code, function);
            write_u16(
                &mut unit.code,
                u16::try_from(args.len()).map_err(|_| SqbcV2Error::new("too many args"))?,
            );
            Ok(())
        }
    }
}

fn compile_literal(
    code: &mut Vec<u8>,
    strings: &mut StringTable,
    value: &serde_json::Value,
) -> Result<(), SqbcV2Error> {
    if value.is_null() {
        emit(code, OP_PUSH_NULL);
    } else if let Some(value) = value.as_bool() {
        emit(code, OP_PUSH_BOOL);
        emit(code, u8::from(value));
    } else if let Some(value) = value.as_i64() {
        emit(code, OP_PUSH_INT);
        write_i32(
            code,
            i32::try_from(value)
                .map_err(|_| SqbcV2Error::new("integer literal out of i32 range"))?,
        );
    } else if let Some(value) = value.as_str() {
        let id = strings.intern(value)?;
        emit(code, OP_PUSH_STRING);
        write_u16(code, id);
    } else {
        return Err(SqbcV2Error::new(
            "unsupported literal in reference bytecode subset",
        ));
    }
    Ok(())
}

fn opcode_for_operator(operator: &str) -> Result<u8, SqbcV2Error> {
    match operator {
        "+" => Ok(OP_ADD),
        "-" => Ok(OP_SUB),
        "==" => Ok(OP_EQ),
        "!=" => Ok(OP_NE),
        "<" => Ok(OP_LT),
        "<=" => Ok(OP_LTE),
        ">" => Ok(OP_GT),
        ">=" => Ok(OP_GTE),
        _ => Err(SqbcV2Error::new(format!("unsupported operator {operator}"))),
    }
}

fn state_id(unit: &CompileUnit, name: &str) -> Result<u16, SqbcV2Error> {
    unit.states
        .get(name)
        .copied()
        .ok_or_else(|| SqbcV2Error::new(format!("unknown state {name}")))
}

fn function_id(unit: &CompileUnit, name: &str) -> Result<u16, SqbcV2Error> {
    unit.functions
        .get(name)
        .copied()
        .ok_or_else(|| SqbcV2Error::new(format!("unknown function {name}")))
}

fn emit_builtin(code: &mut Vec<u8>, builtin: u8) {
    emit(code, OP_CALL_BUILTIN);
    emit(code, builtin);
}

fn encode_state_section(ir: &IrProgram, strings: &StringTable) -> Result<Vec<u8>, SqbcV2Error> {
    let mut out = Vec::new();
    write_u16(
        &mut out,
        u16::try_from(ir.state.len()).map_err(|_| SqbcV2Error::new("too many state slots"))?,
    );
    for state in &ir.state {
        write_u16(
            &mut out,
            *strings
                .ids
                .get(&state.name)
                .ok_or_else(|| SqbcV2Error::new("missing state name string"))?,
        );
        encode_value(&mut out, strings, &state.value)?;
    }
    Ok(out)
}

fn encode_value(
    out: &mut Vec<u8>,
    strings: &StringTable,
    value: &serde_json::Value,
) -> Result<(), SqbcV2Error> {
    if value.is_null() {
        emit(out, VALUE_NULL);
    } else if let Some(value) = value.as_bool() {
        emit(out, VALUE_BOOL);
        emit(out, u8::from(value));
    } else if let Some(value) = value.as_i64() {
        emit(out, VALUE_I32);
        write_i32(
            out,
            i32::try_from(value).map_err(|_| SqbcV2Error::new("state integer out of i32 range"))?,
        );
    } else if let Some(value) = value.as_str() {
        emit(out, VALUE_STRING);
        write_u16(
            out,
            *strings
                .ids
                .get(value)
                .ok_or_else(|| SqbcV2Error::new("missing string value"))?,
        );
    } else {
        return Err(SqbcV2Error::new("unsupported state value"));
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
        write_u32(&mut out, handler.start);
        write_u32(&mut out, handler.len);
    }
    out
}

fn encode_container(sections: Vec<(u16, Vec<u8>)>) -> Result<Vec<u8>, SqbcV2Error> {
    let section_count =
        u32::try_from(sections.len()).map_err(|_| SqbcV2Error::new("too many sections"))?;
    let header_len = 16usize + sections.len() * 12usize;
    let mut offset = u32::try_from(header_len).map_err(|_| SqbcV2Error::new("header too large"))?;
    let mut records = Vec::new();
    for (kind, data) in &sections {
        records.push((
            *kind,
            offset,
            u32::try_from(data.len()).map_err(|_| SqbcV2Error::new("section too large"))?,
        ));
        offset = offset
            .checked_add(
                u32::try_from(data.len()).map_err(|_| SqbcV2Error::new("section too large"))?,
            )
            .ok_or_else(|| SqbcV2Error::new("file too large"))?;
    }

    let mut out = Vec::with_capacity(offset as usize);
    out.extend_from_slice(SQBC_V2_MAGIC);
    write_u16(&mut out, SQBC_V2_VERSION);
    write_u16(
        &mut out,
        u16::try_from(header_len).map_err(|_| SqbcV2Error::new("header too large"))?,
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

fn patch_u32(out: &mut [u8], offset: usize) -> Result<(), SqbcV2Error> {
    let value = u32::try_from(out.len()).map_err(|_| SqbcV2Error::new("code too large"))?;
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
