use core::{fmt::Write, str};

use crate::{
    bytecode::{
        read_i32, read_u16, read_u32, BUILTIN_APP_ARM, BUILTIN_APP_DISARM, BUILTIN_APP_EXIT,
        BUILTIN_APP_LAUNCH, BUILTIN_DEBUG_PRINT, BUILTIN_DISPLAY_CLEAR, BUILTIN_DISPLAY_LINE,
        BUILTIN_DISPLAY_RECT, BUILTIN_DISPLAY_TEXT, BUILTIN_HARDWARE_GPIO_READ,
        BUILTIN_HARDWARE_GPIO_TOGGLE, BUILTIN_HARDWARE_GPIO_WRITE, BUILTIN_SCREEN_OPEN,
        BUILTIN_SCREEN_REFRESH, BUILTIN_SERVICE_INDICATOR_BREATHE, BUILTIN_SERVICE_INDICATOR_READ,
        BUILTIN_SERVICE_INDICATOR_TOGGLE, BUILTIN_SERVICE_INDICATOR_WRITE,
        BUILTIN_SERVICE_TIMER_AFTER, BUILTIN_SERVICE_TIMER_EVERY, BUILTIN_SERVICE_WIFI_GET_AP_IP,
        BUILTIN_SERVICE_WIFI_START_AP, BUILTIN_SERVICE_WIFI_STATUS, BUILTIN_SERVICE_WIFI_STOP_AP,
        BUILTIN_STATE_LOAD, BUILTIN_STATE_RESET, BUILTIN_STATE_SAVE, BUILTIN_SYSTEM_MEMORY,
        BUILTIN_SYSTEM_STORAGE, OP_ADD, OP_CALL_BUILTIN, OP_CALL_FUNCTION, OP_EQ, OP_GET_FIELD,
        OP_GET_LOCAL, OP_GET_STATE, OP_GT, OP_GTE, OP_HALT, OP_JUMP, OP_JUMP_IF_FALSE, OP_LT,
        OP_LTE, OP_NE, OP_POP, OP_PUSH_BOOL, OP_PUSH_INT, OP_PUSH_NULL, OP_PUSH_STRING, OP_RETURN,
        OP_SET_LOCAL, OP_SET_STATE, OP_SUB,
    },
    chunk::{ChunkCache, ChunkKind, ChunkRef},
    error::VmError,
    host::{
        DisplayLineOptions, DisplayRectOptions, DisplayTextOptions, TraceSink, WifiActionResult,
        WifiApIp, WifiStatus,
    },
    limits::{
        MAX_CALL_DEPTH, MAX_CODE_CHUNK_BYTES, MAX_INSTRUCTIONS_PER_EVENT, MAX_LOCALS,
        MAX_RUNTIME_RECORDS, MAX_RUNTIME_RECORD_FIELDS, MAX_RUNTIME_STRING_BYTES,
        MAX_SAVED_STATE_BYTES, MAX_STACK, MAX_STATE,
    },
    program::{Program, ProgramIndex},
    reader::{ChunkedVmHost, SqbcReader},
    state::{
        apply_state_record, concat_value_strings, encode_state_record, state_value_matches,
        values_equal,
    },
    strings::{RuntimeStrings, StringResolver, StringTable},
    value::Value,
};

pub struct Vm<'a> {
    program: Program<'a>,
    runtime_strings: RuntimeStrings,
    runtime_records: RuntimeRecords,
    state: [Value; MAX_STATE],
    stack: [Value; MAX_STACK],
    stack_len: usize,
    current_screen: Option<u16>,
    exited: bool,
    instructions: usize,
}

pub struct ChunkedVm {
    index: ProgramIndex,
    runtime_strings: RuntimeStrings,
    runtime_records: RuntimeRecords,
    state: [Value; MAX_STATE],
    stack: [Value; MAX_STACK],
    stack_len: usize,
    current_screen: Option<u16>,
    exited: bool,
    instructions: usize,
    chunk_cache: ChunkCache<4>,
    code: [u8; MAX_CODE_CHUNK_BYTES],
    code_start: usize,
    code_len: usize,
}

#[derive(Clone, Copy)]
struct RuntimeRecordField {
    name: &'static str,
    value: Value,
}

impl RuntimeRecordField {
    const fn new(name: &'static str, value: Value) -> Self {
        Self { name, value }
    }
}

#[derive(Clone, Copy)]
struct RuntimeRecord {
    fields: [RuntimeRecordField; MAX_RUNTIME_RECORD_FIELDS],
    field_count: usize,
}

impl RuntimeRecord {
    const fn empty() -> Self {
        Self {
            fields: [RuntimeRecordField::new("", Value::Null); MAX_RUNTIME_RECORD_FIELDS],
            field_count: 0,
        }
    }
}

struct RuntimeRecords {
    records: [RuntimeRecord; MAX_RUNTIME_RECORDS],
    next: usize,
}

impl RuntimeRecords {
    const fn new() -> Self {
        Self {
            records: [RuntimeRecord::empty(); MAX_RUNTIME_RECORDS],
            next: 0,
        }
    }

    fn alloc(&mut self, fields: &[RuntimeRecordField]) -> Result<Value, VmError> {
        if fields.len() > MAX_RUNTIME_RECORD_FIELDS {
            return Err(VmError::InvalidOperand);
        }
        let id = self.next;
        self.next = (self.next + 1) % MAX_RUNTIME_RECORDS;
        let mut record = RuntimeRecord::empty();
        record.field_count = fields.len();
        for (index, field) in fields.iter().enumerate() {
            record.fields[index] = *field;
        }
        self.records[id] = record;
        Ok(Value::Record(id as u8))
    }

    fn field(&self, record_id: u8, field_name: &str) -> Result<Value, VmError> {
        let record = self
            .records
            .get(record_id as usize)
            .ok_or(VmError::InvalidOperand)?;
        for field in record.fields.iter().take(record.field_count) {
            if field.name == field_name {
                return Ok(field.value);
            }
        }
        Err(VmError::InvalidOperand)
    }
}

impl ChunkedVm {
    pub fn new(index: ProgramIndex) -> Self {
        let mut state = [Value::Null; MAX_STATE];
        for (slot_index, slot) in index.state_slots.iter().take(index.state_count).enumerate() {
            state[slot_index] = slot.default;
        }
        Self {
            index,
            runtime_strings: RuntimeStrings::new(),
            runtime_records: RuntimeRecords::new(),
            state,
            stack: [Value::Null; MAX_STACK],
            stack_len: 0,
            current_screen: None,
            exited: false,
            instructions: 0,
            chunk_cache: ChunkCache::new(),
            code: [0; MAX_CODE_CHUNK_BYTES],
            code_start: usize::MAX,
            code_len: 0,
        }
    }

    pub fn dispatch(&mut self, host: &mut impl ChunkedVmHost, event: &str) -> Result<(), VmError> {
        if self.exited {
            return Ok(());
        }
        let (index, handler) = self.index.handler(event)?;
        let key = ChunkRef {
            app: 0,
            kind: ChunkKind::Handler,
            index: index as u16,
        };
        self.chunk_cache.insert(key, handler.preload).ok();
        self.chunk_cache.begin_execute(key).ok();
        host.trace(event);
        let mut locals = [Value::Null; MAX_LOCALS];
        self.instructions = 0;
        let result = self
            .execute_range(host, handler.start, handler.len, &mut locals, 0)
            .map(|_| ());
        self.chunk_cache.end_execute(key).ok();
        if result.is_err() {
            host.service_wifi_teardown()?;
        }
        result
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn current_screen(&self) -> Result<Option<&str>, VmError> {
        self.current_screen
            .map(|id| self.index.string(id))
            .transpose()
    }

    pub fn state_value(&self, name: &str) -> Result<Value, VmError> {
        for (index, slot) in self
            .index
            .state_slots
            .iter()
            .take(self.index.state_count)
            .enumerate()
        {
            if self.index.string(slot.name_id)? == name {
                return Ok(self.state[index]);
            }
        }
        Err(VmError::StateOutOfBounds)
    }

    pub fn state_count(&self) -> usize {
        self.index.state_count
    }

    pub fn state_name(&self, index: usize) -> Result<&str, VmError> {
        if index >= self.index.state_count {
            return Err(VmError::StateOutOfBounds);
        }
        self.index.string(self.index.state_slots[index].name_id)
    }

    pub fn state_at(&self, index: usize) -> Result<Value, VmError> {
        if index >= self.index.state_count {
            return Err(VmError::StateOutOfBounds);
        }
        Ok(self.state[index])
    }

    pub fn set_state_value(&mut self, name: &str, value: Value) -> Result<(), VmError> {
        for (index, slot) in self
            .index
            .state_slots
            .iter()
            .take(self.index.state_count)
            .enumerate()
        {
            if self.index.string(slot.name_id)? == name {
                if !state_value_matches(slot.value_type.tag, slot.value_type.nullable, value) {
                    return Err(VmError::InvalidOperand);
                }
                self.state[index] = value;
                return Ok(());
            }
        }
        Err(VmError::StateOutOfBounds)
    }

    pub fn string_resolver(&self) -> StringResolver<'_> {
        StringResolver::new(&self.index, &self.runtime_strings)
    }

    pub fn string_table(&self) -> &dyn StringTable {
        &self.index
    }

    pub const fn installed_code_cache_bytes(&self) -> usize {
        MAX_CODE_CHUNK_BYTES
    }

    fn reset_state(&mut self) {
        for (slot_index, slot) in self
            .index
            .state_slots
            .iter()
            .take(self.index.state_count)
            .enumerate()
        {
            self.state[slot_index] = slot.default;
        }
    }

    fn load_state_from_host(&mut self, host: &mut impl TraceSink) -> Result<(), VmError> {
        let mut bytes = [0u8; MAX_SAVED_STATE_BYTES];
        if let Some(len) = host.state_load(&mut bytes)? {
            apply_state_record(
                &bytes[..len],
                &self.index,
                &self.index.state_slots[..self.index.state_count],
                &mut self.runtime_strings,
                &mut self.state[..self.index.state_count],
            )?;
        }
        Ok(())
    }

    fn save_state_to_host(&self, host: &mut impl TraceSink) -> Result<(), VmError> {
        let mut bytes = [0u8; MAX_SAVED_STATE_BYTES];
        let len = encode_state_record(
            &self.index,
            &self.runtime_strings,
            &self.index.state_slots[..self.index.state_count],
            &self.state[..self.index.state_count],
            &mut bytes,
        )?;
        host.state_save(&bytes[..len])
    }

    fn load_chunk(
        &mut self,
        reader: &mut impl SqbcReader,
        start: usize,
        len: usize,
    ) -> Result<(), VmError> {
        let end = start.checked_add(len).ok_or(VmError::InvalidJump)?;
        if end > self.index.code_len {
            return Err(VmError::InvalidJump);
        }
        if len > self.code.len() {
            return Err(VmError::ChunkTooLarge);
        }
        if self.code_start == start && self.code_len == len {
            return Ok(());
        }
        reader.read_exact_at(self.index.code_offset + start, &mut self.code[..len])?;
        self.code_start = start;
        self.code_len = len;
        Ok(())
    }

    fn render_screen(
        &mut self,
        host: &mut impl ChunkedVmHost,
        screen_id: u16,
        depth: usize,
    ) -> Result<(), VmError> {
        let (screen_index, screen) = self.index.screen(self.index.string(screen_id)?)?;
        let mut locals = [Value::Null; MAX_LOCALS];
        let key = ChunkRef {
            app: 0,
            kind: ChunkKind::Screen,
            index: screen_index as u16,
        };
        self.chunk_cache.insert(key, false).ok();
        self.chunk_cache.begin_execute(key).ok();
        let result = self.execute_range(host, screen.start, screen.len, &mut locals, depth + 1);
        self.chunk_cache.end_execute(key).ok();
        result.map(|_| ())
    }

    fn pop_optional_string(&mut self) -> Result<Option<u16>, VmError> {
        match self.pop()? {
            Value::Null => Ok(None),
            Value::String(id) => Ok(Some(id)),
            _ => Err(VmError::InvalidOperand),
        }
    }

    fn code_byte(&self, offset: usize) -> Result<u8, VmError> {
        if offset < self.code_start {
            return Err(VmError::InvalidJump);
        }
        let relative = offset - self.code_start;
        if relative >= self.code_len {
            return Err(VmError::InvalidJump);
        }
        Ok(self.code[relative])
    }

    fn read_u16_code(&self, offset: usize) -> Result<u16, VmError> {
        Ok(u16::from_le_bytes([
            self.code_byte(offset)?,
            self.code_byte(offset + 1)?,
        ]))
    }

    fn read_u32_code(&self, offset: usize) -> Result<u32, VmError> {
        Ok(u32::from_le_bytes([
            self.code_byte(offset)?,
            self.code_byte(offset + 1)?,
            self.code_byte(offset + 2)?,
            self.code_byte(offset + 3)?,
        ]))
    }

    fn read_i32_code(&self, offset: usize) -> Result<i32, VmError> {
        Ok(i32::from_le_bytes([
            self.code_byte(offset)?,
            self.code_byte(offset + 1)?,
            self.code_byte(offset + 2)?,
            self.code_byte(offset + 3)?,
        ]))
    }

    fn execute_range(
        &mut self,
        host: &mut impl ChunkedVmHost,
        start: usize,
        len: usize,
        locals: &mut [Value; MAX_LOCALS],
        depth: usize,
    ) -> Result<Option<Value>, VmError> {
        if depth > MAX_CALL_DEPTH {
            return Err(VmError::CallDepthExceeded);
        }
        let end = start.checked_add(len).ok_or(VmError::InvalidJump)?;
        if end > self.index.code_len {
            return Err(VmError::InvalidJump);
        }
        self.load_chunk(host, start, len)?;
        let mut ip = start;
        while ip < end {
            self.load_chunk(host, start, len)?;
            self.instructions += 1;
            if self.instructions > MAX_INSTRUCTIONS_PER_EVENT {
                return Err(VmError::InstructionBudgetExceeded);
            }
            let op = self.code_byte(ip)?;
            ip += 1;
            match op {
                OP_PUSH_INT => {
                    let value = self.read_i32_code(ip)?;
                    ip += 4;
                    self.push(Value::I32(value))?;
                }
                OP_PUSH_BOOL => {
                    let value = self.code_byte(ip)? != 0;
                    ip += 1;
                    self.push(Value::Bool(value))?;
                }
                OP_PUSH_STRING => {
                    let value = self.read_u16_code(ip)?;
                    ip += 2;
                    self.push(Value::String(value))?;
                }
                OP_PUSH_NULL => self.push(Value::Null)?,
                OP_GET_STATE => {
                    let state = self.read_u16_code(ip)? as usize;
                    ip += 2;
                    self.push(*self.state.get(state).ok_or(VmError::StateOutOfBounds)?)?;
                }
                OP_SET_STATE => {
                    let state = self.read_u16_code(ip)? as usize;
                    ip += 2;
                    let value = self.pop()?;
                    let state_slot = self
                        .index
                        .state_slots
                        .get(state)
                        .ok_or(VmError::StateOutOfBounds)?;
                    if state >= self.index.state_count
                        || !state_value_matches(
                            state_slot.value_type.tag,
                            state_slot.value_type.nullable,
                            value,
                        )
                    {
                        return Err(VmError::InvalidOperand);
                    }
                    let slot = self.state.get_mut(state).ok_or(VmError::StateOutOfBounds)?;
                    *slot = value;
                }
                OP_GET_LOCAL => {
                    let local = self.read_u16_code(ip)? as usize;
                    ip += 2;
                    self.push(*locals.get(local).ok_or(VmError::LocalOutOfBounds)?)?;
                }
                OP_SET_LOCAL => {
                    let local = self.read_u16_code(ip)? as usize;
                    ip += 2;
                    let value = self.pop()?;
                    let slot = locals.get_mut(local).ok_or(VmError::LocalOutOfBounds)?;
                    *slot = value;
                }
                OP_GET_FIELD => {
                    let field_id = self.read_u16_code(ip)?;
                    ip += 2;
                    let Value::Record(record_id) = self.pop()? else {
                        return Err(VmError::InvalidOperand);
                    };
                    let field = self.index.string(field_id)?;
                    let value = self.runtime_records.field(record_id, field)?;
                    self.push(value)?;
                }
                OP_ADD | OP_SUB | OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE => {
                    self.binary(op)?
                }
                OP_JUMP => {
                    ip = self.read_u32_code(ip)? as usize;
                    if ip > end {
                        return Err(VmError::InvalidJump);
                    }
                }
                OP_JUMP_IF_FALSE => {
                    let target = self.read_u32_code(ip)? as usize;
                    ip += 4;
                    if !self.pop()?.truthy() {
                        if target > end {
                            return Err(VmError::InvalidJump);
                        }
                        ip = target;
                    }
                }
                OP_CALL_FUNCTION => {
                    let function_id = self.read_u16_code(ip)? as usize;
                    ip += 2;
                    let arg_count = self.read_u16_code(ip)? as usize;
                    ip += 2;
                    let function = *self
                        .index
                        .functions
                        .get(function_id)
                        .ok_or(VmError::FunctionOutOfBounds)?;
                    if function_id >= self.index.function_count
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
                    let key = ChunkRef {
                        app: 0,
                        kind: ChunkKind::Function,
                        index: function_id as u16,
                    };
                    self.chunk_cache.insert(key, false).ok();
                    self.chunk_cache.begin_execute(key).ok();
                    let value = self
                        .execute_range(
                            host,
                            function.start,
                            function.len,
                            &mut child_locals,
                            depth + 1,
                        )?
                        .unwrap_or(Value::Null);
                    self.chunk_cache.end_execute(key).ok();
                    self.push(value)?;
                }
                OP_RETURN => return Ok(Some(self.pop()?)),
                OP_HALT => return Ok(None),
                OP_CALL_BUILTIN => {
                    let builtin = self.code_byte(ip)?;
                    ip += 1;
                    let arg_count = if builtin == BUILTIN_DEBUG_PRINT {
                        let count = self.code_byte(ip)?;
                        ip += 1;
                        count
                    } else {
                        0
                    };
                    self.call_builtin(host, builtin, arg_count, depth)?;
                }
                OP_POP => {
                    let _ = self.pop()?;
                }
                _ => return Err(VmError::UnknownOpcode),
            }
        }
        Ok(None)
    }

    fn call_builtin(
        &mut self,
        host: &mut impl ChunkedVmHost,
        builtin: u8,
        arg_count: u8,
        depth: usize,
    ) -> Result<(), VmError> {
        match builtin {
            BUILTIN_STATE_LOAD => {
                self.load_state_from_host(host)?;
                host.trace("state.load");
            }
            BUILTIN_STATE_SAVE => {
                self.save_state_to_host(host)?;
                host.trace("state.save");
            }
            BUILTIN_STATE_RESET => {
                self.reset_state();
                host.state_reset_persistent()?;
                host.trace("state.reset");
            }
            BUILTIN_APP_EXIT => {
                host.service_wifi_teardown()?;
                self.exited = true;
                host.trace("app.exit");
            }
            BUILTIN_DEBUG_PRINT => {
                let count = arg_count as usize;
                if count > self.stack_len {
                    return Err(VmError::StackUnderflow);
                }
                let start = self.stack_len - count;
                let strings = StringResolver::new(&self.index, &self.runtime_strings);
                host.debug_print(&strings, &self.stack[start..self.stack_len]);
                self.stack_len = start;
            }
            BUILTIN_SCREEN_OPEN => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                self.current_screen = Some(name_id);
                self.render_screen(host, name_id, depth)?;
            }
            BUILTIN_SCREEN_REFRESH => {
                let screen_id = self.current_screen.ok_or(VmError::InvalidOperand)?;
                self.render_screen(host, screen_id, depth)?;
            }
            BUILTIN_DISPLAY_CLEAR => {
                let Value::String(color_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.draw_clear(self.index.string(color_id)?);
            }
            BUILTIN_DISPLAY_TEXT => {
                let valign_id = self.pop_optional_string()?;
                let align_id = self.pop_optional_string()?;
                let background_color_id = self.pop_optional_string()?;
                let text_color_id = self.pop_optional_string()?;
                let font_height = self.pop()?.expect_i32()?;
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                let text = self.pop()?;
                let strings = StringResolver::new(&self.index, &self.runtime_strings);
                host.draw_text(
                    &strings,
                    text,
                    DisplayTextOptions {
                        x,
                        y,
                        w,
                        h,
                        font_height,
                        text_color: text_color_id.map(|id| self.index.string(id)).transpose()?,
                        background_color: background_color_id
                            .map(|id| self.index.string(id))
                            .transpose()?,
                        align: align_id.map(|id| self.index.string(id)).transpose()?,
                        valign: valign_id.map(|id| self.index.string(id)).transpose()?,
                    },
                );
            }
            BUILTIN_DISPLAY_RECT => {
                let stroke_color_id = self.pop_optional_string()?;
                let fill_color_id = self.pop_optional_string()?;
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                host.draw_rect(DisplayRectOptions {
                    x,
                    y,
                    w,
                    h,
                    fill_color: fill_color_id.map(|id| self.index.string(id)).transpose()?,
                    stroke_color: stroke_color_id
                        .map(|id| self.index.string(id))
                        .transpose()?,
                });
            }
            BUILTIN_DISPLAY_LINE => {
                let color_id = self.pop_optional_string()?;
                let y2 = self.pop()?.expect_i32()?;
                let x2 = self.pop()?.expect_i32()?;
                let y1 = self.pop()?.expect_i32()?;
                let x1 = self.pop()?.expect_i32()?;
                host.draw_line(DisplayLineOptions {
                    x1,
                    y1,
                    x2,
                    y2,
                    color: color_id.map(|id| self.index.string(id)).transpose()?,
                });
            }
            BUILTIN_HARDWARE_GPIO_WRITE => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let Value::Bool(value) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.hardware_gpio_write(self.index.string(name_id)?, value)?;
            }
            BUILTIN_HARDWARE_GPIO_TOGGLE => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.hardware_gpio_toggle(self.index.string(name_id)?)?;
            }
            BUILTIN_HARDWARE_GPIO_READ => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let value = host.hardware_gpio_read(self.index.string(name_id)?)?;
                self.push(Value::Bool(value))?;
            }
            BUILTIN_SERVICE_INDICATOR_WRITE => {
                let Value::Bool(value) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.service_indicator_write(value)?;
            }
            BUILTIN_SERVICE_INDICATOR_TOGGLE => {
                host.service_indicator_toggle()?;
            }
            BUILTIN_SERVICE_INDICATOR_BREATHE => {
                host.service_indicator_breathe()?;
            }
            BUILTIN_SERVICE_INDICATOR_READ => {
                let value = host.service_indicator_read()?;
                self.push(Value::Bool(value))?;
            }
            BUILTIN_APP_LAUNCH => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.app_launch(self.index.string(app_id)?)?;
            }
            BUILTIN_APP_ARM => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.app_arm(self.index.string(app_id)?)?;
            }
            BUILTIN_APP_DISARM => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.app_disarm(self.index.string(app_id)?)?;
            }
            BUILTIN_SERVICE_TIMER_EVERY => {
                let interval_ms = self.pop()?.expect_i32()?;
                let Value::String(event_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.service_timer_every(self.index.string(event_id)?, interval_ms)?;
            }
            BUILTIN_SERVICE_TIMER_AFTER => {
                let delay_ms = self.pop()?.expect_i32()?;
                let Value::String(event_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.service_timer_after(self.index.string(event_id)?, delay_ms)?;
            }
            BUILTIN_SERVICE_WIFI_START_AP => {
                let Value::String(ssid_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let result = host.service_wifi_start_ap(self.index.string(ssid_id)?)?;
                let value = self.wifi_action_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_STOP_AP => {
                let result = host.service_wifi_stop_ap()?;
                let value = self.wifi_action_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_STATUS => {
                let result = host.service_wifi_status()?;
                let value = self.wifi_status_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_GET_AP_IP => {
                let result = host.service_wifi_get_ap_ip()?;
                let value = self.wifi_ap_ip_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SYSTEM_MEMORY => {
                let mut writer = self.runtime_strings.alloc()?;
                host.system_memory_text(&mut writer)?;
                let value = writer.value();
                self.push(value)?;
            }
            BUILTIN_SYSTEM_STORAGE => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let name = self.index.string(name_id)?;
                let mut writer = self.runtime_strings.alloc()?;
                host.system_storage_text(name, &mut writer)?;
                let value = writer.value();
                self.push(value)?;
            }
            _ => return Err(VmError::InvalidOperand),
        }
        Ok(())
    }

    fn runtime_string_value(&mut self, value: Option<&str>) -> Result<Value, VmError> {
        let Some(value) = value else {
            return Ok(Value::Null);
        };
        let mut writer = self.runtime_strings.alloc()?;
        writer
            .write_str(value)
            .map_err(|_| VmError::InvalidOperand)?;
        Ok(writer.value())
    }

    fn wifi_action_record(&mut self, result: WifiActionResult<'_>) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new("ok", Value::Bool(result.ok)),
            RuntimeRecordField::new("error", error),
        ])
    }

    fn wifi_status_record(&mut self, result: WifiStatus<'_>) -> Result<Value, VmError> {
        let mode = self.runtime_string_value(result.mode)?;
        let ip_address = self.runtime_string_value(result.ip_address)?;
        let ssid = self.runtime_string_value(result.ssid)?;
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new("active", Value::Bool(result.active)),
            RuntimeRecordField::new("mode", mode),
            RuntimeRecordField::new("ipAddress", ip_address),
            RuntimeRecordField::new("ssid", ssid),
            RuntimeRecordField::new("clients", Value::I32(result.clients)),
            RuntimeRecordField::new("error", error),
        ])
    }

    fn wifi_ap_ip_record(&mut self, result: WifiApIp<'_>) -> Result<Value, VmError> {
        let ip = self.runtime_string_value(result.ip)?;
        let gw = self.runtime_string_value(result.gw)?;
        let netmask = self.runtime_string_value(result.netmask)?;
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new("ip", ip),
            RuntimeRecordField::new("gw", gw),
            RuntimeRecordField::new("netmask", netmask),
            RuntimeRecordField::new("error", error),
        ])
    }

    fn binary(&mut self, op: u8) -> Result<(), VmError> {
        let right = self.pop()?;
        let left = self.pop()?;
        let value = match op {
            OP_ADD => self.add_values(left, right)?,
            OP_SUB => Value::I32(left.expect_i32()? - right.expect_i32()?),
            OP_EQ => Value::Bool(values_equal(
                &self.index,
                &self.runtime_strings,
                left,
                right,
            )?),
            OP_NE => Value::Bool(!values_equal(
                &self.index,
                &self.runtime_strings,
                left,
                right,
            )?),
            OP_LT => Value::Bool(left.expect_i32()? < right.expect_i32()?),
            OP_LTE => Value::Bool(left.expect_i32()? <= right.expect_i32()?),
            OP_GT => Value::Bool(left.expect_i32()? > right.expect_i32()?),
            OP_GTE => Value::Bool(left.expect_i32()? >= right.expect_i32()?),
            _ => return Err(VmError::UnknownOpcode),
        };
        self.push(value)
    }

    fn add_values(&mut self, left: Value, right: Value) -> Result<Value, VmError> {
        if let (Value::I32(left), Value::I32(right)) = (left, right) {
            return Ok(Value::I32(left + right));
        }
        if left.is_string() && right.is_string() {
            let mut bytes = [0u8; MAX_RUNTIME_STRING_BYTES];
            let len =
                concat_value_strings(&self.index, &self.runtime_strings, left, right, &mut bytes)?;
            let text = str::from_utf8(&bytes[..len]).map_err(|_| VmError::InvalidUtf8)?;
            let mut writer = self.runtime_strings.alloc()?;
            writer
                .write_str(text)
                .map_err(|_| VmError::InvalidOperand)?;
            return Ok(writer.value());
        }
        Err(VmError::InvalidOperand)
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
            runtime_strings: RuntimeStrings::new(),
            runtime_records: RuntimeRecords::new(),
            state,
            stack: [Value::Null; MAX_STACK],
            stack_len: 0,
            current_screen: None,
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
        let result = self
            .execute_range(handler.start, handler.len, &mut locals, 0, trace)
            .map(|_| ());
        if result.is_err() {
            trace.service_wifi_teardown()?;
        }
        result
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn current_screen(&self) -> Result<Option<&str>, VmError> {
        self.current_screen
            .map(|id| self.program.string(id))
            .transpose()
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

    pub fn string_resolver(&self) -> StringResolver<'_> {
        StringResolver::new(&self.program, &self.runtime_strings)
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
                if !state_value_matches(slot.value_type.tag, slot.value_type.nullable, value) {
                    return Err(VmError::InvalidOperand);
                }
                self.state[index] = value;
                return Ok(());
            }
        }
        Err(VmError::StateOutOfBounds)
    }

    fn reset_state(&mut self) {
        for (slot_index, slot) in self
            .program
            .state_slots
            .iter()
            .take(self.program.state_count)
            .enumerate()
        {
            self.state[slot_index] = slot.default;
        }
    }

    fn load_state_from_host(&mut self, trace: &mut impl TraceSink) -> Result<(), VmError> {
        let mut bytes = [0u8; MAX_SAVED_STATE_BYTES];
        if let Some(len) = trace.state_load(&mut bytes)? {
            apply_state_record(
                &bytes[..len],
                &self.program,
                &self.program.state_slots[..self.program.state_count],
                &mut self.runtime_strings,
                &mut self.state[..self.program.state_count],
            )?;
        }
        Ok(())
    }

    fn render_screen<T: TraceSink>(
        &mut self,
        screen_id: u16,
        depth: usize,
        trace: &mut T,
    ) -> Result<(), VmError> {
        let screen = self.program.screen(self.program.string(screen_id)?)?;
        let mut locals = [Value::Null; MAX_LOCALS];
        self.execute_range(screen.start, screen.len, &mut locals, depth + 1, trace)?;
        Ok(())
    }

    fn pop_optional_string(&mut self) -> Result<Option<u16>, VmError> {
        match self.pop()? {
            Value::Null => Ok(None),
            Value::String(id) => Ok(Some(id)),
            _ => Err(VmError::InvalidOperand),
        }
    }

    fn save_state_to_host(&self, trace: &mut impl TraceSink) -> Result<(), VmError> {
        let mut bytes = [0u8; MAX_SAVED_STATE_BYTES];
        let len = encode_state_record(
            &self.program,
            &self.runtime_strings,
            &self.program.state_slots[..self.program.state_count],
            &self.state[..self.program.state_count],
            &mut bytes,
        )?;
        trace.state_save(&bytes[..len])
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
                    let state_slot = self
                        .program
                        .state_slots
                        .get(state)
                        .ok_or(VmError::StateOutOfBounds)?;
                    if state >= self.program.state_count
                        || !state_value_matches(
                            state_slot.value_type.tag,
                            state_slot.value_type.nullable,
                            value,
                        )
                    {
                        return Err(VmError::InvalidOperand);
                    }
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
                OP_GET_FIELD => {
                    let field_id = read_u16(self.program.code, ip)?;
                    ip += 2;
                    let Value::Record(record_id) = self.pop()? else {
                        return Err(VmError::InvalidOperand);
                    };
                    let field = self.program.string(field_id)?;
                    let value = self.runtime_records.field(record_id, field)?;
                    self.push(value)?;
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
            BUILTIN_STATE_LOAD => {
                self.load_state_from_host(trace)?;
                trace.trace("state.load");
            }
            BUILTIN_STATE_SAVE => {
                self.save_state_to_host(trace)?;
                trace.trace("state.save");
            }
            BUILTIN_STATE_RESET => {
                self.reset_state();
                trace.state_reset_persistent()?;
                trace.trace("state.reset");
            }
            BUILTIN_APP_EXIT => {
                trace.service_wifi_teardown()?;
                self.exited = true;
                trace.trace("app.exit");
            }
            BUILTIN_DEBUG_PRINT => {
                let count = arg_count as usize;
                if count > self.stack_len {
                    return Err(VmError::StackUnderflow);
                }
                let start = self.stack_len - count;
                let strings = StringResolver::new(&self.program, &self.runtime_strings);
                trace.debug_print(&strings, &self.stack[start..self.stack_len]);
                self.stack_len = start;
            }
            BUILTIN_SCREEN_OPEN => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                self.current_screen = Some(name_id);
                self.render_screen(name_id, depth, trace)?;
            }
            BUILTIN_SCREEN_REFRESH => {
                let screen_id = self.current_screen.ok_or(VmError::InvalidOperand)?;
                self.render_screen(screen_id, depth, trace)?;
            }
            BUILTIN_DISPLAY_CLEAR => {
                let Value::String(color_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                trace.draw_clear(self.program.string(color_id)?);
            }
            BUILTIN_DISPLAY_TEXT => {
                let valign_id = self.pop_optional_string()?;
                let align_id = self.pop_optional_string()?;
                let background_color_id = self.pop_optional_string()?;
                let text_color_id = self.pop_optional_string()?;
                let font_height = self.pop()?.expect_i32()?;
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                let text = self.pop()?;
                let strings = StringResolver::new(&self.program, &self.runtime_strings);
                trace.draw_text(
                    &strings,
                    text,
                    DisplayTextOptions {
                        x,
                        y,
                        w,
                        h,
                        font_height,
                        text_color: text_color_id
                            .map(|id| self.program.string(id))
                            .transpose()?,
                        background_color: background_color_id
                            .map(|id| self.program.string(id))
                            .transpose()?,
                        align: align_id.map(|id| self.program.string(id)).transpose()?,
                        valign: valign_id.map(|id| self.program.string(id)).transpose()?,
                    },
                );
            }
            BUILTIN_DISPLAY_RECT => {
                let stroke_color_id = self.pop_optional_string()?;
                let fill_color_id = self.pop_optional_string()?;
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                trace.draw_rect(DisplayRectOptions {
                    x,
                    y,
                    w,
                    h,
                    fill_color: fill_color_id
                        .map(|id| self.program.string(id))
                        .transpose()?,
                    stroke_color: stroke_color_id
                        .map(|id| self.program.string(id))
                        .transpose()?,
                });
            }
            BUILTIN_DISPLAY_LINE => {
                let color_id = self.pop_optional_string()?;
                let y2 = self.pop()?.expect_i32()?;
                let x2 = self.pop()?.expect_i32()?;
                let y1 = self.pop()?.expect_i32()?;
                let x1 = self.pop()?.expect_i32()?;
                trace.draw_line(DisplayLineOptions {
                    x1,
                    y1,
                    x2,
                    y2,
                    color: color_id.map(|id| self.program.string(id)).transpose()?,
                });
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
            BUILTIN_SERVICE_INDICATOR_WRITE => {
                let Value::Bool(value) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                trace.service_indicator_write(value)?;
            }
            BUILTIN_SERVICE_INDICATOR_TOGGLE => {
                trace.service_indicator_toggle()?;
            }
            BUILTIN_SERVICE_INDICATOR_BREATHE => {
                trace.service_indicator_breathe()?;
            }
            BUILTIN_SERVICE_INDICATOR_READ => {
                let value = trace.service_indicator_read()?;
                self.push(Value::Bool(value))?;
            }
            BUILTIN_APP_LAUNCH => {
                let Value::String(app_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let app = self.program.string(app_id)?;
                trace.app_launch(app)?;
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
            BUILTIN_SERVICE_TIMER_EVERY => {
                let interval_ms = self.pop()?.expect_i32()?;
                let Value::String(event_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let event = self.program.string(event_id)?;
                trace.service_timer_every(event, interval_ms)?;
            }
            BUILTIN_SERVICE_TIMER_AFTER => {
                let delay_ms = self.pop()?.expect_i32()?;
                let Value::String(event_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let event = self.program.string(event_id)?;
                trace.service_timer_after(event, delay_ms)?;
            }
            BUILTIN_SERVICE_WIFI_START_AP => {
                let Value::String(ssid_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let ssid = self.program.string(ssid_id)?;
                let result = trace.service_wifi_start_ap(ssid)?;
                let value = self.wifi_action_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_STOP_AP => {
                let result = trace.service_wifi_stop_ap()?;
                let value = self.wifi_action_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_STATUS => {
                let result = trace.service_wifi_status()?;
                let value = self.wifi_status_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_GET_AP_IP => {
                let result = trace.service_wifi_get_ap_ip()?;
                let value = self.wifi_ap_ip_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SYSTEM_MEMORY => {
                let mut writer = self.runtime_strings.alloc()?;
                trace.system_memory_text(&mut writer)?;
                let value = writer.value();
                self.push(value)?;
            }
            BUILTIN_SYSTEM_STORAGE => {
                let Value::String(name_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let name = self.program.string(name_id)?;
                let mut writer = self.runtime_strings.alloc()?;
                trace.system_storage_text(name, &mut writer)?;
                let value = writer.value();
                self.push(value)?;
            }
            _ => return Err(VmError::InvalidOperand),
        }
        Ok(())
    }

    fn runtime_string_value(&mut self, value: Option<&str>) -> Result<Value, VmError> {
        let Some(value) = value else {
            return Ok(Value::Null);
        };
        let mut writer = self.runtime_strings.alloc()?;
        writer
            .write_str(value)
            .map_err(|_| VmError::InvalidOperand)?;
        Ok(writer.value())
    }

    fn wifi_action_record(&mut self, result: WifiActionResult<'_>) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new("ok", Value::Bool(result.ok)),
            RuntimeRecordField::new("error", error),
        ])
    }

    fn wifi_status_record(&mut self, result: WifiStatus<'_>) -> Result<Value, VmError> {
        let mode = self.runtime_string_value(result.mode)?;
        let ip_address = self.runtime_string_value(result.ip_address)?;
        let ssid = self.runtime_string_value(result.ssid)?;
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new("active", Value::Bool(result.active)),
            RuntimeRecordField::new("mode", mode),
            RuntimeRecordField::new("ipAddress", ip_address),
            RuntimeRecordField::new("ssid", ssid),
            RuntimeRecordField::new("clients", Value::I32(result.clients)),
            RuntimeRecordField::new("error", error),
        ])
    }

    fn wifi_ap_ip_record(&mut self, result: WifiApIp<'_>) -> Result<Value, VmError> {
        let ip = self.runtime_string_value(result.ip)?;
        let gw = self.runtime_string_value(result.gw)?;
        let netmask = self.runtime_string_value(result.netmask)?;
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new("ip", ip),
            RuntimeRecordField::new("gw", gw),
            RuntimeRecordField::new("netmask", netmask),
            RuntimeRecordField::new("error", error),
        ])
    }

    fn binary(&mut self, op: u8) -> Result<(), VmError> {
        let right = self.pop()?;
        let left = self.pop()?;
        let value = match op {
            OP_ADD => self.add_values(left, right)?,
            OP_SUB => Value::I32(left.expect_i32()? - right.expect_i32()?),
            OP_EQ => Value::Bool(values_equal(
                &self.program,
                &self.runtime_strings,
                left,
                right,
            )?),
            OP_NE => Value::Bool(!values_equal(
                &self.program,
                &self.runtime_strings,
                left,
                right,
            )?),
            OP_LT => Value::Bool(left.expect_i32()? < right.expect_i32()?),
            OP_LTE => Value::Bool(left.expect_i32()? <= right.expect_i32()?),
            OP_GT => Value::Bool(left.expect_i32()? > right.expect_i32()?),
            OP_GTE => Value::Bool(left.expect_i32()? >= right.expect_i32()?),
            _ => return Err(VmError::UnknownOpcode),
        };
        self.push(value)
    }

    fn add_values(&mut self, left: Value, right: Value) -> Result<Value, VmError> {
        if let (Value::I32(left), Value::I32(right)) = (left, right) {
            return Ok(Value::I32(left + right));
        }
        if left.is_string() && right.is_string() {
            let mut bytes = [0u8; MAX_RUNTIME_STRING_BYTES];
            let len = concat_value_strings(
                &self.program,
                &self.runtime_strings,
                left,
                right,
                &mut bytes,
            )?;
            let text = str::from_utf8(&bytes[..len]).map_err(|_| VmError::InvalidUtf8)?;
            let mut writer = self.runtime_strings.alloc()?;
            writer
                .write_str(text)
                .map_err(|_| VmError::InvalidOperand)?;
            return Ok(writer.value());
        }
        Err(VmError::InvalidOperand)
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
