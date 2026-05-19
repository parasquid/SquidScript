use core::str;

pub const MAX_STRINGS: usize = 64;
pub const MAX_STATE: usize = 16;
pub const MAX_FUNCTIONS: usize = 16;
pub const MAX_HANDLERS: usize = 16;
pub const MAX_SCREENS: usize = 16;
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
const SECTION_SCREENS: u16 = 6;

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
const BUILTIN_DEBUG_PRINT: u8 = 4;
const BUILTIN_SCREEN_OPEN: u8 = 5;
const BUILTIN_DISPLAY_CLEAR: u8 = 6;
const BUILTIN_DISPLAY_TEXT: u8 = 7;
const BUILTIN_DISPLAY_RECT: u8 = 8;
const BUILTIN_DISPLAY_LINE: u8 = 9;
const BUILTIN_HARDWARE_GPIO_WRITE: u8 = 10;
const BUILTIN_HARDWARE_GPIO_TOGGLE: u8 = 11;
const BUILTIN_HARDWARE_GPIO_READ: u8 = 12;
const BUILTIN_APP_LAUNCH: u8 = 13;
const BUILTIN_APP_START: u8 = 15;
const BUILTIN_APP_ARM: u8 = 16;
const BUILTIN_APP_DISARM: u8 = 17;
const BUILTIN_EVENT_ADD_SOURCE: u8 = 18;

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
    TooManyScreens,
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
struct Screen {
    name_id: u16,
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
    screens: [Screen; MAX_SCREENS],
    screen_count: usize,
    code: &'a [u8],
}

pub trait TraceSink {
    fn trace(&mut self, message: &str);
    fn debug_print(&mut self, _program: &Program<'_>, _values: &[Value]) {}
    fn draw_clear(&mut self, _color: &str) {}
    fn draw_text(&mut self, _program: &Program<'_>, _text: Value, _x: i32, _y: i32) {}
    fn draw_rect(&mut self, _x: i32, _y: i32, _w: i32, _h: i32) {}
    fn draw_line(&mut self, _x1: i32, _y1: i32, _x2: i32, _y2: i32) {}
    fn hardware_gpio_write(&mut self, _name: &str, _value: bool) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn hardware_gpio_toggle(&mut self, _name: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn hardware_gpio_read(&mut self, _name: &str) -> Result<bool, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_launch(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_start(&mut self, app: &str) -> Result<(), VmError> {
        self.app_launch(app)
    }
    fn app_arm(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_disarm(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn event_add_source(&mut self, _event: &str, _every_ms: Option<i32>) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
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
        let screens = optional_section(bytes, section_count, SECTION_SCREENS)?;
        let code = section(bytes, section_count, SECTION_CODE)?;

        let (strings, string_count) = parse_strings(strings)?;
        let (state_slots, state_count) = parse_state(state)?;
        let (functions, function_count) = parse_functions(functions, code.len())?;
        let (handlers, handler_count) = parse_handlers(handlers, code.len())?;
        let (screens, screen_count) = parse_screens(screens, code.len())?;

        Ok(Self {
            strings,
            string_count,
            state_slots,
            state_count,
            functions,
            function_count,
            handlers,
            handler_count,
            screens,
            screen_count,
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

    fn screen(&self, name: &str) -> Result<Screen, VmError> {
        for screen in self.screens.iter().take(self.screen_count) {
            if self.string(screen.name_id)? == name {
                return Ok(*screen);
            }
        }
        Err(VmError::InvalidOperand)
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

    pub fn set_state_value(&mut self, name: &str, value: Value) -> Result<(), VmError> {
        for (index, slot) in self
            .program
            .state_slots
            .iter()
            .take(self.program.state_count)
            .enumerate()
        {
            if self.program.string(slot.name_id)? == name {
                self.state[index] = value;
                return Ok(());
            }
        }
        Err(VmError::StateOutOfBounds)
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
                    let arg_count = if builtin == BUILTIN_DEBUG_PRINT {
                        let count = *self.program.code.get(ip).ok_or(VmError::InvalidOperand)?;
                        ip += 1;
                        count
                    } else {
                        0
                    };
                    self.call_builtin(builtin, arg_count, depth, trace)?;
                }
                OP_POP => {
                    let _ = self.pop()?;
                }
                _ => return Err(VmError::UnknownOpcode),
            }
        }
        Ok(None)
    }

    fn call_builtin<T: TraceSink>(
        &mut self,
        builtin: u8,
        arg_count: u8,
        depth: usize,
        trace: &mut T,
    ) -> Result<(), VmError> {
        match builtin {
            BUILTIN_STATE_LOAD => trace.trace("state.load"),
            BUILTIN_STATE_SAVE => trace.trace("state.save"),
            BUILTIN_APP_EXIT => {
                self.exited = true;
                trace.trace("app.exit");
            }
            BUILTIN_DEBUG_PRINT => {
                let count = arg_count as usize;
                if count > self.stack_len {
                    return Err(VmError::StackUnderflow);
                }
                let start = self.stack_len - count;
                trace.debug_print(&self.program, &self.stack[start..self.stack_len]);
                self.stack_len = start;
            }
            BUILTIN_SCREEN_OPEN => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let screen = self.program.screen(self.program.string(name_id)?)?;
                let mut locals = [Value::Null; MAX_LOCALS];
                self.execute_range(screen.start, screen.len, &mut locals, depth + 1, trace)?;
            }
            BUILTIN_DISPLAY_CLEAR => {
                let Value::String(color_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                trace.draw_clear(self.program.string(color_id)?);
            }
            BUILTIN_DISPLAY_TEXT => {
                let y = self.pop()?.as_i32();
                let x = self.pop()?.as_i32();
                let text = self.pop()?;
                trace.draw_text(&self.program, text, x, y);
            }
            BUILTIN_DISPLAY_RECT => {
                let h = self.pop()?.as_i32();
                let w = self.pop()?.as_i32();
                let y = self.pop()?.as_i32();
                let x = self.pop()?.as_i32();
                trace.draw_rect(x, y, w, h);
            }
            BUILTIN_DISPLAY_LINE => {
                let y2 = self.pop()?.as_i32();
                let x2 = self.pop()?.as_i32();
                let y1 = self.pop()?.as_i32();
                let x1 = self.pop()?.as_i32();
                trace.draw_line(x1, y1, x2, y2);
            }
            BUILTIN_HARDWARE_GPIO_WRITE => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let Value::Bool(value) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let name = self.program.string(name_id)?;
                trace.hardware_gpio_write(name, value)?;
            }
            BUILTIN_HARDWARE_GPIO_TOGGLE => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let name = self.program.string(name_id)?;
                trace.hardware_gpio_toggle(name)?;
            }
            BUILTIN_HARDWARE_GPIO_READ => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let name = self.program.string(name_id)?;
                let value = trace.hardware_gpio_read(name)?;
                self.push(Value::Bool(value))?;
            }
            BUILTIN_APP_LAUNCH => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let app = self.program.string(app_id)?;
                trace.app_launch(app)?;
            }
            BUILTIN_APP_START => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let app = self.program.string(app_id)?;
                trace.app_start(app)?;
            }
            BUILTIN_APP_ARM => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let app = self.program.string(app_id)?;
                trace.app_arm(app)?;
            }
            BUILTIN_APP_DISARM => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let app = self.program.string(app_id)?;
                trace.app_disarm(app)?;
            }
            BUILTIN_EVENT_ADD_SOURCE => {
                let every_ms = match self.pop()? {
                    Value::Null => None,
                    value => Some(value.as_i32()),
                };
                let Value::String(event_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let event = self.program.string(event_id)?;
                trace.event_add_source(event, every_ms)?;
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

fn optional_section<'a>(
    bytes: &'a [u8],
    section_count: usize,
    kind: u16,
) -> Result<Option<&'a [u8]>, VmError> {
    match section(bytes, section_count, kind) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(VmError::MissingSection) => Ok(None),
        Err(error) => Err(error),
    }
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

fn parse_screens(
    bytes: Option<&[u8]>,
    code_len: usize,
) -> Result<([Screen; MAX_SCREENS], usize), VmError> {
    let Some(bytes) = bytes else {
        return Ok((
            [Screen {
                name_id: 0,
                start: 0,
                len: 0,
            }; MAX_SCREENS],
            0,
        ));
    };
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_SCREENS {
        return Err(VmError::TooManyScreens);
    }
    let mut screens = [Screen {
        name_id: 0,
        start: 0,
        len: 0,
    }; MAX_SCREENS];
    let mut cursor = 2usize;
    for screen in screens.iter_mut().take(count) {
        let name_id = read_u16(bytes, cursor)?;
        let start = read_u32(bytes, cursor + 2)? as usize;
        let len = read_u32(bytes, cursor + 6)? as usize;
        cursor += 10;
        validate_range(start, len, code_len)?;
        *screen = Screen {
            name_id,
            start,
            len,
        };
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((screens, count))
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

    #[derive(Default)]
    struct GpioTrace {
        events: Vec<String>,
        led: bool,
    }

    impl TraceSink for GpioTrace {
        fn trace(&mut self, message: &str) {
            self.events.push(message.to_string());
        }

        fn hardware_gpio_write(&mut self, name: &str, value: bool) -> Result<(), VmError> {
            self.events.push(format!("write {name}={value}"));
            self.led = value;
            Ok(())
        }

        fn hardware_gpio_toggle(&mut self, name: &str) -> Result<(), VmError> {
            self.events.push(format!("toggle {name}"));
            self.led = !self.led;
            Ok(())
        }

        fn hardware_gpio_read(&mut self, name: &str) -> Result<bool, VmError> {
            self.events.push(format!("read {name}"));
            Ok(self.led)
        }
    }

    #[derive(Default)]
    struct RuntimeTrace {
        events: Vec<String>,
    }

    impl TraceSink for RuntimeTrace {
        fn trace(&mut self, message: &str) {
            self.events.push(message.to_string());
        }

        fn debug_print(&mut self, program: &Program<'_>, values: &[Value]) {
            let mut line = String::new();
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    line.push(' ');
                }
                match value {
                    Value::String(id) => line.push_str(program.string(*id).unwrap()),
                    Value::I32(value) => line.push_str(&value.to_string()),
                    Value::Bool(value) => line.push_str(&value.to_string()),
                    Value::Null => line.push_str("null"),
                }
            }
            self.events.push(format!("debug {line}"));
        }

        fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
            self.events.push(format!("launch {app}"));
            Ok(())
        }

        fn app_start(&mut self, app: &str) -> Result<(), VmError> {
            self.events.push(format!("start {app}"));
            Ok(())
        }

        fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
            self.events.push(format!("arm {app}"));
            Ok(())
        }

        fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
            self.events.push(format!("disarm {app}"));
            Ok(())
        }

        fn event_add_source(&mut self, event: &str, every_ms: Option<i32>) -> Result<(), VmError> {
            self.events.push(format!(
                "event.addSource {event} {}",
                every_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ));
            Ok(())
        }
    }

    #[test]
    fn runs_headless_counter_fixture_from_real_bytecode() {
        let bytes = fixture_counter_sqbc();
        let program = Program::parse(&bytes).expect("valid fixture");
        let mut vm = Vm::new(program);
        let mut trace = Trace::default();

        vm.dispatch("app.start", &mut trace).unwrap();
        assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
        assert_eq!(vm.state_value("count"), Ok(Value::I32(0)));

        vm.dispatch("key.SELECT", &mut trace).unwrap();
        vm.dispatch("key.SELECT", &mut trace).unwrap();
        assert_eq!(vm.state_value("count"), Ok(Value::I32(2)));

        vm.dispatch("key.BACK", &mut trace).unwrap();
        assert!(vm.exited());
        assert_eq!(
            trace.events,
            vec![
                "app.start",
                "state.load",
                "state.save",
                "key.SELECT",
                "state.save",
                "key.SELECT",
                "state.save",
                "key.BACK",
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

        vm.dispatch("app.start", &mut trace).unwrap();
        vm.dispatch("key.SELECT", &mut trace).unwrap();

        assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
        assert_eq!(vm.state_value("count"), Ok(Value::I32(1)));
        assert_eq!(
            trace.events,
            vec![
                "app.start",
                "state.load",
                "state.save",
                "key.SELECT",
                "state.save",
            ]
        );
    }

    #[test]
    fn runs_hardware_gpio_builtins_from_real_bytecode() {
        let source = r#"app "gpio" target "esp32c3-super-mini"
state { led: false }
event.on("app.start") {
  hardware.gpio.write("status_led", true)
  led = hardware.gpio.read("status_led")
  hardware.gpio.toggle("status_led")
  led = hardware.gpio.read("status_led")
}
screen("main") {}
"#;
        let compiled = squidc_core::compile(squidc_core::CompileRequest {
            source: source.to_string(),
            target_id: "esp32c3-super-mini".to_string(),
        });
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
        let program = Program::parse(&bytes).unwrap();
        let mut vm = Vm::new(program);
        let mut trace = GpioTrace::default();

        vm.dispatch("app.start", &mut trace).unwrap();

        assert_eq!(vm.state_value("led"), Ok(Value::Bool(false)));
        assert_eq!(
            trace.events,
            vec![
                "app.start",
                "write status_led=true",
                "read status_led",
                "toggle status_led",
                "read status_led",
            ]
        );
    }

    #[test]
    fn runs_app_launch_event_source_and_timer_handler_from_real_bytecode() {
        let source = r#"app "timer-demo"
state { count: 0 }
event.on("app.start") {
  app.launch("timer-background")
  event.addSource("timer.debug", { every: 1000 })
}
event.on("timer.debug") {
  debug.print("timer", count)
}
screen("main") {}
"#;
        let compiled = squidc_core::compile(squidc_core::CompileRequest {
            source: source.to_string(),
            target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
        });
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
        let program = Program::parse(&bytes).unwrap();
        let mut vm = Vm::new(program);
        let mut trace = RuntimeTrace::default();

        vm.dispatch("app.start", &mut trace).unwrap();
        vm.dispatch("timer.debug", &mut trace).unwrap();

        assert_eq!(
            trace.events,
            vec![
                "app.start",
                "launch timer-background",
                "event.addSource timer.debug 1000",
                "timer.debug",
                "debug timer 0",
            ]
        );
    }

    #[test]
    fn runs_generic_event_lifecycle_builtins_from_real_bytecode() {
        let source = r#"app "event-demo"
state { count: 0 }
event.on("app.start") {
  app.arm("break-reminder")
  app.start("reader")
  event.addSource("timer.clock", { every: 60000 })
}
event.on("timer.clock") {
  count = count + 1
  app.disarm("break-reminder")
}
screen("main") {}
"#;
        let compiled = squidc_core::compile(squidc_core::CompileRequest {
            source: source.to_string(),
            target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
        });
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
        let program = Program::parse(&bytes).unwrap();
        let mut vm = Vm::new(program);
        let mut trace = RuntimeTrace::default();

        vm.dispatch("app.start", &mut trace).unwrap();
        vm.dispatch("timer.clock", &mut trace).unwrap();

        assert_eq!(vm.state_value("count"), Ok(Value::I32(1)));
        assert_eq!(
            trace.events,
            vec![
                "app.start",
                "arm break-reminder",
                "start reader",
                "event.addSource timer.clock 60000",
                "timer.clock",
                "disarm break-reminder",
            ]
        );
    }

    fn fixture_counter_sqbc() -> Vec<u8> {
        let strings = encode_strings(&[
            "headless-counter",
            "count",
            "started",
            "app.start",
            "key.SELECT",
            "key.BACK",
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
