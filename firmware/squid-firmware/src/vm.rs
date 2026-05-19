use core::str;

pub const MAX_STRINGS: usize = 64;
pub const MAX_STATE: usize = 16;
pub const MAX_FUNCTIONS: usize = 16;
pub const MAX_HANDLERS: usize = 16;
pub const MAX_LOCALS: usize = 16;
pub const MAX_STACK: usize = 32;
pub const MAX_CALL_DEPTH: usize = 4;
pub const MAX_INSTRUCTIONS_PER_EVENT: usize = 1000;
pub const MAX_APP_BYTES: usize = 16 * 1024;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    I32(i32),
    String(u16),
}

impl Value {
    const fn truthy(self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(value) => value,
            Value::I32(value) => value != 0,
            Value::String(_) => true,
        }
    }

    const fn as_i32(self) -> i32 {
        match self {
            Value::I32(value) => value,
            Value::Bool(value) => value as i32,
            Value::Null | Value::String(_) => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmError {
    TooLarge,
    InvalidHeader,
    UnsupportedVersion,
    MissingSection,
    InvalidSection,
    InvalidUtf8,
    TooManyStrings,
    TooManyStateSlots,
    TooManyFunctions,
    TooManyHandlers,
    UnknownOpcode,
    InvalidOperand,
    InvalidJump,
    StackOverflow,
    StackUnderflow,
    LocalOutOfBounds,
    StateOutOfBounds,
    FunctionOutOfBounds,
    HandlerNotFound,
    InstructionBudgetExceeded,
    CallDepthExceeded,
}

#[derive(Clone, Copy)]
struct Function {
    _name_id: u16,
    param_count: u16,
    local_count: u16,
    start: usize,
    len: usize,
}

#[derive(Clone, Copy)]
struct Handler {
    event_id: u16,
    start: usize,
    len: usize,
}

#[derive(Clone, Copy)]
struct StateSlot {
    name_id: u16,
    default: Value,
}

pub struct Program<'a> {
    strings: [&'a str; MAX_STRINGS],
    string_count: usize,
    state_slots: [StateSlot; MAX_STATE],
    state_count: usize,
    functions: [Function; MAX_FUNCTIONS],
    function_count: usize,
    handlers: [Handler; MAX_HANDLERS],
    handler_count: usize,
    code: &'a [u8],
}

pub trait TraceSink {
    fn trace(&mut self, message: &str);
}

pub struct Vm<'a> {
    program: Program<'a>,
    state: [Value; MAX_STATE],
    stack: [Value; MAX_STACK],
    stack_len: usize,
    exited: bool,
    instructions: usize,
}

impl<'a> Program<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, VmError> {
        if bytes.len() > MAX_APP_BYTES {
            return Err(VmError::TooLarge);
        }
        if bytes.len() < 16 || &bytes[0..4] != b"SQBC" {
            return Err(VmError::InvalidHeader);
        }
        if read_u16(bytes, 4)? != 2 {
            return Err(VmError::UnsupportedVersion);
        }
        let header_len = read_u16(bytes, 6)? as usize;
        let file_len = read_u32(bytes, 8)? as usize;
        let section_count = read_u32(bytes, 12)? as usize;
        if file_len != bytes.len()
            || header_len != 16 + section_count * 12
            || header_len > bytes.len()
        {
            return Err(VmError::InvalidHeader);
        }

        let strings = section(bytes, section_count, SECTION_STRINGS)?;
        let state = section(bytes, section_count, SECTION_STATE)?;
        let functions = section(bytes, section_count, SECTION_FUNCTIONS)?;
        let handlers = section(bytes, section_count, SECTION_HANDLERS)?;
        let code = section(bytes, section_count, SECTION_CODE)?;

        let (strings, string_count) = parse_strings(strings)?;
        let (state_slots, state_count) = parse_state(state)?;
        let (functions, function_count) = parse_functions(functions, code.len())?;
        let (handlers, handler_count) = parse_handlers(handlers, code.len())?;

        Ok(Self {
            strings,
            string_count,
            state_slots,
            state_count,
            functions,
            function_count,
            handlers,
            handler_count,
            code,
        })
    }

    pub fn string(&self, id: u16) -> Result<&str, VmError> {
        let index = id as usize;
        if index >= self.string_count {
            return Err(VmError::InvalidOperand);
        }
        Ok(self.strings[index])
    }

    fn handler(&self, event: &str) -> Result<Handler, VmError> {
        for handler in self.handlers.iter().take(self.handler_count) {
            if self.string(handler.event_id)? == event {
                return Ok(*handler);
            }
        }
        Err(VmError::HandlerNotFound)
    }
}

impl<'a> Vm<'a> {
    pub fn new(program: Program<'a>) -> Self {
        let mut state = [Value::Null; MAX_STATE];
        for (index, slot) in program
            .state_slots
            .iter()
            .take(program.state_count)
            .enumerate()
        {
            state[index] = slot.default;
        }
        Self {
            program,
            state,
            stack: [Value::Null; MAX_STACK],
            stack_len: 0,
            exited: false,
            instructions: 0,
        }
    }

    pub fn dispatch<T: TraceSink>(&mut self, event: &str, trace: &mut T) -> Result<(), VmError> {
        if self.exited {
            return Ok(());
        }
        let handler = self.program.handler(event)?;
        trace.trace(event);
        let mut locals = [Value::Null; MAX_LOCALS];
        self.instructions = 0;
        self.execute_range(handler.start, handler.len, &mut locals, 0, trace)
            .map(|_| ())
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn state_value(&self, name: &str) -> Result<Value, VmError> {
        for (index, slot) in self
            .program
            .state_slots
            .iter()
            .take(self.program.state_count)
            .enumerate()
        {
            if self.program.string(slot.name_id)? == name {
                return Ok(self.state[index]);
            }
        }
        Err(VmError::StateOutOfBounds)
    }

    pub fn program(&self) -> &Program<'a> {
        &self.program
    }

    pub fn state_count(&self) -> usize {
        self.program.state_count
    }

    pub fn state_name(&self, index: usize) -> Result<&str, VmError> {
        if index >= self.program.state_count {
            return Err(VmError::StateOutOfBounds);
        }
        self.program.string(self.program.state_slots[index].name_id)
    }

    pub fn state_at(&self, index: usize) -> Result<Value, VmError> {
        if index >= self.program.state_count {
            return Err(VmError::StateOutOfBounds);
        }
        Ok(self.state[index])
    }

    fn execute_range<T: TraceSink>(
        &mut self,
        start: usize,
        len: usize,
        locals: &mut [Value; MAX_LOCALS],
        depth: usize,
        trace: &mut T,
    ) -> Result<Option<Value>, VmError> {
        if depth > MAX_CALL_DEPTH {
            return Err(VmError::CallDepthExceeded);
        }
        let end = start.checked_add(len).ok_or(VmError::InvalidJump)?;
        if end > self.program.code.len() {
            return Err(VmError::InvalidJump);
        }
        let mut ip = start;
        while ip < end {
            self.instructions += 1;
            if self.instructions > MAX_INSTRUCTIONS_PER_EVENT {
                return Err(VmError::InstructionBudgetExceeded);
            }
            let op = self.program.code[ip];
            ip += 1;
            match op {
                OP_PUSH_INT => {
                    let value = read_i32(self.program.code, ip)?;
                    ip += 4;
                    self.push(Value::I32(value))?;
                }
                OP_PUSH_BOOL => {
                    let value = *self.program.code.get(ip).ok_or(VmError::InvalidOperand)? != 0;
                    ip += 1;
                    self.push(Value::Bool(value))?;
                }
                OP_PUSH_STRING => {
                    let value = read_u16(self.program.code, ip)?;
                    ip += 2;
                    self.push(Value::String(value))?;
                }
                OP_PUSH_NULL => self.push(Value::Null)?,
                OP_GET_STATE => {
                    let state = read_u16(self.program.code, ip)? as usize;
                    ip += 2;
                    self.push(*self.state.get(state).ok_or(VmError::StateOutOfBounds)?)?;
                }
                OP_SET_STATE => {
                    let state = read_u16(self.program.code, ip)? as usize;
                    ip += 2;
                    let value = self.pop()?;
                    let slot = self.state.get_mut(state).ok_or(VmError::StateOutOfBounds)?;
                    *slot = value;
                }
                OP_GET_LOCAL => {
                    let local = read_u16(self.program.code, ip)? as usize;
                    ip += 2;
                    self.push(*locals.get(local).ok_or(VmError::LocalOutOfBounds)?)?;
                }
                OP_SET_LOCAL => {
                    let local = read_u16(self.program.code, ip)? as usize;
                    ip += 2;
                    let value = self.pop()?;
                    let slot = locals.get_mut(local).ok_or(VmError::LocalOutOfBounds)?;
                    *slot = value;
                }
                OP_ADD | OP_SUB | OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE => {
                    self.binary(op)?
                }
                OP_JUMP => {
                    ip = read_u32(self.program.code, ip)? as usize;
                    if ip > end {
                        return Err(VmError::InvalidJump);
                    }
                }
                OP_JUMP_IF_FALSE => {
                    let target = read_u32(self.program.code, ip)? as usize;
                    ip += 4;
                    if !self.pop()?.truthy() {
                        if target > end {
                            return Err(VmError::InvalidJump);
                        }
                        ip = target;
                    }
                }
                OP_CALL_FUNCTION => {
                    let function_id = read_u16(self.program.code, ip)? as usize;
                    ip += 2;
                    let arg_count = read_u16(self.program.code, ip)? as usize;
                    ip += 2;
                    let function = *self
                        .program
                        .functions
                        .get(function_id)
                        .ok_or(VmError::FunctionOutOfBounds)?;
                    if function_id >= self.program.function_count
                        || arg_count != function.param_count as usize
                    {
                        return Err(VmError::FunctionOutOfBounds);
                    }
                    let mut child_locals = [Value::Null; MAX_LOCALS];
                    if function.local_count as usize > MAX_LOCALS {
                        return Err(VmError::LocalOutOfBounds);
                    }
                    for index in (0..arg_count).rev() {
                        child_locals[index] = self.pop()?;
                    }
                    let value = self
                        .execute_range(
                            function.start,
                            function.len,
                            &mut child_locals,
                            depth + 1,
                            trace,
                        )?
                        .unwrap_or(Value::Null);
                    self.push(value)?;
                }
                OP_RETURN => return Ok(Some(self.pop()?)),
                OP_HALT => return Ok(None),
                OP_CALL_BUILTIN => {
                    let builtin = *self.program.code.get(ip).ok_or(VmError::InvalidOperand)?;
                    ip += 1;
                    self.call_builtin(builtin, trace)?;
                }
                OP_POP => {
                    let _ = self.pop()?;
                }
                _ => return Err(VmError::UnknownOpcode),
            }
        }
        Ok(None)
    }

    fn call_builtin<T: TraceSink>(&mut self, builtin: u8, trace: &mut T) -> Result<(), VmError> {
        match builtin {
            BUILTIN_STATE_LOAD => trace.trace("state.load"),
            BUILTIN_STATE_SAVE => trace.trace("state.save"),
            BUILTIN_APP_EXIT => {
                self.exited = true;
                trace.trace("app.exit");
            }
            _ => return Err(VmError::InvalidOperand),
        }
        Ok(())
    }

    fn binary(&mut self, op: u8) -> Result<(), VmError> {
        let right = self.pop()?;
        let left = self.pop()?;
        let value = match op {
            OP_ADD => Value::I32(left.as_i32() + right.as_i32()),
            OP_SUB => Value::I32(left.as_i32() - right.as_i32()),
            OP_EQ => Value::Bool(left == right),
            OP_NE => Value::Bool(left != right),
            OP_LT => Value::Bool(left.as_i32() < right.as_i32()),
            OP_LTE => Value::Bool(left.as_i32() <= right.as_i32()),
            OP_GT => Value::Bool(left.as_i32() > right.as_i32()),
            OP_GTE => Value::Bool(left.as_i32() >= right.as_i32()),
            _ => return Err(VmError::UnknownOpcode),
        };
        self.push(value)
    }

    fn push(&mut self, value: Value) -> Result<(), VmError> {
        if self.stack_len == MAX_STACK {
            return Err(VmError::StackOverflow);
        }
        self.stack[self.stack_len] = value;
        self.stack_len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, VmError> {
        if self.stack_len == 0 {
            return Err(VmError::StackUnderflow);
        }
        self.stack_len -= 1;
        Ok(self.stack[self.stack_len])
    }
}

fn section<'a>(bytes: &'a [u8], section_count: usize, kind: u16) -> Result<&'a [u8], VmError> {
    for index in 0..section_count {
        let base = 16 + index * 12;
        let record_kind = read_u16(bytes, base)?;
        if record_kind == kind {
            let offset = read_u32(bytes, base + 4)? as usize;
            let len = read_u32(bytes, base + 8)? as usize;
            let end = offset.checked_add(len).ok_or(VmError::InvalidSection)?;
            return bytes.get(offset..end).ok_or(VmError::InvalidSection);
        }
    }
    Err(VmError::MissingSection)
}

fn parse_strings(bytes: &[u8]) -> Result<([&str; MAX_STRINGS], usize), VmError> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_STRINGS {
        return Err(VmError::TooManyStrings);
    }
    let mut strings = [""; MAX_STRINGS];
    let mut cursor = 2usize;
    for slot in strings.iter_mut().take(count) {
        let len = read_u16(bytes, cursor)? as usize;
        cursor += 2;
        let end = cursor.checked_add(len).ok_or(VmError::InvalidSection)?;
        let raw = bytes.get(cursor..end).ok_or(VmError::InvalidSection)?;
        *slot = str::from_utf8(raw).map_err(|_| VmError::InvalidUtf8)?;
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((strings, count))
}

fn parse_state(bytes: &[u8]) -> Result<([StateSlot; MAX_STATE], usize), VmError> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_STATE {
        return Err(VmError::TooManyStateSlots);
    }
    let mut slots = [StateSlot {
        name_id: 0,
        default: Value::Null,
    }; MAX_STATE];
    let mut cursor = 2usize;
    for slot in slots.iter_mut().take(count) {
        let name_id = read_u16(bytes, cursor)?;
        cursor += 2;
        let (value, next) = read_value(bytes, cursor)?;
        cursor = next;
        *slot = StateSlot {
            name_id,
            default: value,
        };
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((slots, count))
}

fn parse_functions(
    bytes: &[u8],
    code_len: usize,
) -> Result<([Function; MAX_FUNCTIONS], usize), VmError> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_FUNCTIONS {
        return Err(VmError::TooManyFunctions);
    }
    let mut functions = [Function {
        _name_id: 0,
        param_count: 0,
        local_count: 0,
        start: 0,
        len: 0,
    }; MAX_FUNCTIONS];
    let mut cursor = 2usize;
    for function in functions.iter_mut().take(count) {
        let name_id = read_u16(bytes, cursor)?;
        let param_count = read_u16(bytes, cursor + 2)?;
        let local_count = read_u16(bytes, cursor + 4)?;
        let start = read_u32(bytes, cursor + 6)? as usize;
        let len = read_u32(bytes, cursor + 10)? as usize;
        cursor += 14;
        validate_range(start, len, code_len)?;
        *function = Function {
            _name_id: name_id,
            param_count,
            local_count,
            start,
            len,
        };
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((functions, count))
}

fn parse_handlers(
    bytes: &[u8],
    code_len: usize,
) -> Result<([Handler; MAX_HANDLERS], usize), VmError> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_HANDLERS {
        return Err(VmError::TooManyHandlers);
    }
    let mut handlers = [Handler {
        event_id: 0,
        start: 0,
        len: 0,
    }; MAX_HANDLERS];
    let mut cursor = 2usize;
    for handler in handlers.iter_mut().take(count) {
        let event_id = read_u16(bytes, cursor)?;
        let start = read_u32(bytes, cursor + 2)? as usize;
        let len = read_u32(bytes, cursor + 6)? as usize;
        cursor += 10;
        validate_range(start, len, code_len)?;
        *handler = Handler {
            event_id,
            start,
            len,
        };
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((handlers, count))
}

fn validate_range(start: usize, len: usize, total: usize) -> Result<(), VmError> {
    let end = start.checked_add(len).ok_or(VmError::InvalidSection)?;
    if end > total {
        return Err(VmError::InvalidSection);
    }
    Ok(())
}

fn read_value(bytes: &[u8], cursor: usize) -> Result<(Value, usize), VmError> {
    let tag = *bytes.get(cursor).ok_or(VmError::InvalidSection)?;
    match tag {
        VALUE_NULL => Ok((Value::Null, cursor + 1)),
        VALUE_BOOL => Ok((
            Value::Bool(*bytes.get(cursor + 1).ok_or(VmError::InvalidSection)? != 0),
            cursor + 2,
        )),
        VALUE_I32 => Ok((Value::I32(read_i32(bytes, cursor + 1)?), cursor + 5)),
        VALUE_STRING => Ok((Value::String(read_u16(bytes, cursor + 1)?), cursor + 3)),
        _ => Err(VmError::InvalidSection),
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, VmError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(VmError::InvalidSection)?
            .try_into()
            .map_err(|_| VmError::InvalidSection)?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, VmError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(VmError::InvalidSection)?
            .try_into()
            .map_err(|_| VmError::InvalidSection)?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, VmError> {
    Ok(i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(VmError::InvalidSection)?
            .try_into()
            .map_err(|_| VmError::InvalidSection)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Trace {
        events: Vec<String>,
    }

    impl TraceSink for Trace {
        fn trace(&mut self, message: &str) {
            self.events.push(message.to_string());
        }
    }

    #[test]
    fn runs_headless_counter_fixture_from_real_bytecode() {
        let bytes = fixture_counter_sqbc();
        let program = Program::parse(&bytes).expect("valid fixture");
        let mut vm = Vm::new(program);
        let mut trace = Trace::default();

        vm.dispatch("onStart", &mut trace).unwrap();
        assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
        assert_eq!(vm.state_value("count"), Ok(Value::I32(0)));

        vm.dispatch("onKey.SELECT", &mut trace).unwrap();
        vm.dispatch("onKey.SELECT", &mut trace).unwrap();
        assert_eq!(vm.state_value("count"), Ok(Value::I32(2)));

        vm.dispatch("onKey.BACK", &mut trace).unwrap();
        assert!(vm.exited());
        assert_eq!(
            trace.events,
            vec![
                "onStart",
                "state.load",
                "state.save",
                "onKey.SELECT",
                "state.save",
                "onKey.SELECT",
                "state.save",
                "onKey.BACK",
                "app.exit",
            ]
        );
    }

    #[test]
    fn rejects_ir_json_sqbc_v1_container() {
        let bytes = b"SQBC\x01\0\x0c\0\x0e\0\0\0\x00\0\0\0{}";
        assert!(matches!(
            Program::parse(bytes),
            Err(VmError::UnsupportedVersion)
        ));
    }

    #[test]
    fn runs_sqbc_v2_emitted_by_squidc_core() {
        let source =
            include_str!("../../../compiler/rust/fixtures/conformance/headless_counter.squid");
        let compiled = squidc_core::compile(squidc_core::CompileRequest {
            source: source.to_string(),
            target_id: "esp32c3-super-mini".to_string(),
        });
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
        let program = Program::parse(&bytes).unwrap();
        let mut vm = Vm::new(program);
        let mut trace = Trace::default();

        vm.dispatch("onStart", &mut trace).unwrap();
        vm.dispatch("onKey.SELECT", &mut trace).unwrap();

        assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
        assert_eq!(vm.state_value("count"), Ok(Value::I32(1)));
        assert_eq!(
            trace.events,
            vec![
                "onStart",
                "state.load",
                "state.save",
                "onKey.SELECT",
                "state.save",
            ]
        );
    }

    fn fixture_counter_sqbc() -> Vec<u8> {
        let strings = encode_strings(&[
            "headless-counter",
            "count",
            "started",
            "onStart",
            "onKey.SELECT",
            "onKey.BACK",
        ]);
        let mut state = Vec::new();
        push_u16(&mut state, 2);
        push_u16(&mut state, 1);
        state.push(VALUE_I32);
        push_i32(&mut state, 0);
        push_u16(&mut state, 2);
        state.push(VALUE_I32);
        push_i32(&mut state, 0);

        let functions = vec![0, 0];
        let mut code = Vec::new();
        let on_start = code.len();
        code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_STATE_LOAD, OP_PUSH_INT]);
        push_i32(&mut code, 1);
        code.push(OP_SET_STATE);
        push_u16(&mut code, 1);
        code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_STATE_SAVE, OP_HALT]);
        let select = code.len();
        code.push(OP_GET_STATE);
        push_u16(&mut code, 0);
        code.push(OP_PUSH_INT);
        push_i32(&mut code, 1);
        code.push(OP_ADD);
        code.push(OP_SET_STATE);
        push_u16(&mut code, 0);
        code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_STATE_SAVE, OP_HALT]);
        let back = code.len();
        code.extend_from_slice(&[OP_CALL_BUILTIN, BUILTIN_APP_EXIT, OP_HALT]);

        let mut handlers = Vec::new();
        push_u16(&mut handlers, 3);
        push_u16(&mut handlers, 3);
        push_u32(&mut handlers, on_start as u32);
        push_u32(&mut handlers, (select - on_start) as u32);
        push_u16(&mut handlers, 4);
        push_u32(&mut handlers, select as u32);
        push_u32(&mut handlers, (back - select) as u32);
        push_u16(&mut handlers, 5);
        push_u32(&mut handlers, back as u32);
        push_u32(&mut handlers, (code.len() - back) as u32);

        encode_container(vec![
            (SECTION_STRINGS, strings),
            (SECTION_STATE, state),
            (SECTION_FUNCTIONS, functions),
            (SECTION_HANDLERS, handlers),
            (SECTION_CODE, code),
        ])
    }

    fn encode_strings(values: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        push_u16(&mut out, values.len() as u16);
        for value in values {
            push_u16(&mut out, value.len() as u16);
            out.extend_from_slice(value.as_bytes());
        }
        out
    }

    fn encode_container(sections: Vec<(u16, Vec<u8>)>) -> Vec<u8> {
        let header_len = 16 + sections.len() * 12;
        let file_len = header_len + sections.iter().map(|(_, data)| data.len()).sum::<usize>();
        let mut out = Vec::new();
        out.extend_from_slice(b"SQBC");
        push_u16(&mut out, 2);
        push_u16(&mut out, header_len as u16);
        push_u32(&mut out, file_len as u32);
        push_u32(&mut out, sections.len() as u32);
        let mut offset = header_len;
        for (kind, data) in &sections {
            push_u16(&mut out, *kind);
            push_u16(&mut out, 0);
            push_u32(&mut out, offset as u32);
            push_u32(&mut out, data.len() as u32);
            offset += data.len();
        }
        for (_, data) in sections {
            out.extend_from_slice(&data);
        }
        out
    }

    fn push_u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i32(out: &mut Vec<u8>, value: i32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
}
