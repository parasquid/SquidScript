use core::{
    fmt::{self, Write},
    str,
};

pub const MAX_STRINGS: usize = 64;
pub const MAX_STATE: usize = 16;
pub const MAX_FUNCTIONS: usize = 16;
pub const MAX_HANDLERS: usize = 16;
pub const MAX_SCREENS: usize = 16;
pub const MAX_LOCALS: usize = 16;
pub const MAX_STACK: usize = 32;
pub const MAX_CALL_DEPTH: usize = 4;
pub const MAX_INSTRUCTIONS_PER_EVENT: usize = 1000;
pub const MAX_APP_BYTES: usize = 4 * 1024;
pub const MAX_PROGRAM_STRING_BYTES: usize = 768;
pub const MAX_CODE_CHUNK_BYTES: usize = 1024;
pub const MAX_SAVED_STATE_BYTES: usize = 512;
pub const MAX_RUNTIME_STRINGS: usize = 4;
pub const MAX_RUNTIME_STRING_BYTES: usize = 48;

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
const BUILTIN_STATE_RESET: u8 = 14;
const BUILTIN_APP_ARM: u8 = 16;
const BUILTIN_APP_DISARM: u8 = 17;
const BUILTIN_SERVICE_TIMER_EVERY: u8 = 18;
const BUILTIN_SERVICE_TIMER_AFTER: u8 = 19;
const BUILTIN_SYSTEM_MEMORY: u8 = 20;
const BUILTIN_SYSTEM_STORAGE: u8 = 21;

const VALUE_NULL: u8 = 0;
const VALUE_BOOL: u8 = 1;
const VALUE_I32: u8 = 2;
const VALUE_STRING: u8 = 3;

const STATE_TYPE_INT: u8 = 1;
const STATE_TYPE_BOOL: u8 = 2;
const STATE_TYPE_STRING: u8 = 3;

const STATE_RECORD_MAGIC: &[u8; 4] = b"SQST";
const STATE_RECORD_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    I32(i32),
    String(u16),
    RuntimeString(u8),
}

impl Value {
    const fn truthy(self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(value) => value,
            Value::I32(value) => value != 0,
            Value::String(_) | Value::RuntimeString(_) => true,
        }
    }

    fn expect_i32(self) -> Result<i32, VmError> {
        match self {
            Value::I32(value) => Ok(value),
            _ => Err(VmError::InvalidOperand),
        }
    }

    const fn is_string(self) -> bool {
        matches!(self, Value::String(_) | Value::RuntimeString(_))
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
    ChunkTooLarge,
    ReadFailed,
    InvalidStateRecord,
    StateTypeMismatch,
    StateTooLarge,
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
    preload: bool,
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
struct StateType {
    tag: u8,
    nullable: bool,
}

#[derive(Clone, Copy)]
struct StateSlot {
    name_id: u16,
    value_type: StateType,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqbcHeader {
    pub header_len: usize,
    pub file_len: usize,
    pub section_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqbcSection {
    pub kind: u16,
    pub offset: usize,
    pub len: usize,
}

pub trait TraceSink {
    fn trace(&mut self, message: &str);
    fn debug_print(&mut self, _strings: &StringResolver<'_>, _values: &[Value]) {}
    fn draw_clear(&mut self, _color: &str) {}
    fn draw_text(&mut self, _strings: &StringResolver<'_>, _text: Value, _x: i32, _y: i32) {}
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
    fn app_arm(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_disarm(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_timer_every(&mut self, _event: &str, _interval_ms: i32) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_timer_after(&mut self, _event: &str, _delay_ms: i32) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn system_memory_text(&mut self, _out: &mut dyn fmt::Write) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn system_storage_text(
        &mut self,
        _name: &str,
        _out: &mut dyn fmt::Write,
    ) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn state_load(&mut self, _out: &mut [u8]) -> Result<Option<usize>, VmError> {
        Ok(None)
    }
    fn state_save(&mut self, _bytes: &[u8]) -> Result<(), VmError> {
        Ok(())
    }
    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        Ok(())
    }
}

pub trait StringTable {
    fn string(&self, id: u16) -> Result<&str, VmError>;
}

pub struct StringResolver<'a> {
    strings: &'a dyn StringTable,
    runtime_strings: &'a RuntimeStrings,
}

impl<'a> StringResolver<'a> {
    pub fn new(strings: &'a dyn StringTable, runtime_strings: &'a RuntimeStrings) -> Self {
        Self {
            strings,
            runtime_strings,
        }
    }

    pub fn value_str(&self, value: Value) -> Result<&str, VmError> {
        match value {
            Value::String(id) => self.strings.string(id),
            Value::RuntimeString(id) => self.runtime_strings.get(id),
            _ => Err(VmError::InvalidOperand),
        }
    }
}

pub struct RuntimeStrings {
    bytes: [[u8; MAX_RUNTIME_STRING_BYTES]; MAX_RUNTIME_STRINGS],
    lens: [usize; MAX_RUNTIME_STRINGS],
    next: usize,
}

impl RuntimeStrings {
    const fn new() -> Self {
        Self {
            bytes: [[0; MAX_RUNTIME_STRING_BYTES]; MAX_RUNTIME_STRINGS],
            lens: [0; MAX_RUNTIME_STRINGS],
            next: 0,
        }
    }

    fn alloc(&mut self) -> Result<RuntimeStringWriter<'_>, VmError> {
        let id = self.next;
        self.next = (self.next + 1) % MAX_RUNTIME_STRINGS;
        self.lens[id] = 0;
        Ok(RuntimeStringWriter { strings: self, id })
    }

    fn get(&self, id: u8) -> Result<&str, VmError> {
        let index = id as usize;
        if index >= MAX_RUNTIME_STRINGS {
            return Err(VmError::InvalidOperand);
        }
        str::from_utf8(&self.bytes[index][..self.lens[index]]).map_err(|_| VmError::InvalidUtf8)
    }
}

pub struct RuntimeStringWriter<'a> {
    strings: &'a mut RuntimeStrings,
    id: usize,
}

impl RuntimeStringWriter<'_> {
    fn value(&self) -> Value {
        Value::RuntimeString(self.id as u8)
    }
}

impl fmt::Write for RuntimeStringWriter<'_> {
    fn write_str(&mut self, input: &str) -> fmt::Result {
        let len = self.strings.lens[self.id];
        let remaining = MAX_RUNTIME_STRING_BYTES.saturating_sub(len);
        let bytes = input.as_bytes();
        let copy_len = remaining.min(bytes.len());
        self.strings.bytes[self.id][len..len + copy_len].copy_from_slice(&bytes[..copy_len]);
        self.strings.lens[self.id] += copy_len;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkKind {
    Handler,
    Function,
    Screen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkRef {
    pub app: u16,
    pub kind: ChunkKind,
    pub index: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChunkCacheSlot {
    key: ChunkRef,
    preload: bool,
    active: bool,
    last_used: u32,
    occupied: bool,
}

impl ChunkCacheSlot {
    const fn empty() -> Self {
        Self {
            key: ChunkRef {
                app: 0,
                kind: ChunkKind::Handler,
                index: 0,
            },
            preload: false,
            active: false,
            last_used: 0,
            occupied: false,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ChunkCacheError {
    Full,
    Missing,
}

pub struct ChunkCache<const N: usize> {
    slots: [ChunkCacheSlot; N],
    clock: u32,
}

impl<const N: usize> ChunkCache<N> {
    pub const fn new() -> Self {
        Self {
            slots: [ChunkCacheSlot::empty(); N],
            clock: 0,
        }
    }

    pub fn insert(&mut self, key: ChunkRef, preload: bool) -> Result<(), ChunkCacheError> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.occupied && slot.key == key)
        {
            slot.preload = preload;
            slot.last_used = self.clock;
            return Ok(());
        }
        if let Some(slot) = self.slots.iter_mut().find(|slot| !slot.occupied) {
            *slot = ChunkCacheSlot {
                key,
                preload,
                active: false,
                last_used: self.clock,
                occupied: true,
            };
            return Ok(());
        }
        let Some(index) = self.evict_candidate_index() else {
            return Err(ChunkCacheError::Full);
        };
        self.slots[index] = ChunkCacheSlot {
            key,
            preload,
            active: false,
            last_used: self.clock,
            occupied: true,
        };
        Ok(())
    }

    pub fn begin_execute(&mut self, key: ChunkRef) -> Result<(), ChunkCacheError> {
        self.clock = self.clock.wrapping_add(1);
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.occupied && slot.key == key)
        else {
            return Err(ChunkCacheError::Missing);
        };
        slot.active = true;
        slot.last_used = self.clock;
        Ok(())
    }

    pub fn end_execute(&mut self, key: ChunkRef) -> Result<(), ChunkCacheError> {
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.occupied && slot.key == key)
        else {
            return Err(ChunkCacheError::Missing);
        };
        slot.active = false;
        Ok(())
    }

    pub fn contains(&self, key: ChunkRef) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.occupied && slot.key == key)
    }

    pub fn drop_app(&mut self, app: u16) {
        for slot in &mut self.slots {
            if slot.occupied && slot.key.app == app {
                *slot = ChunkCacheSlot::empty();
            }
        }
    }

    fn evict_candidate_index(&self) -> Option<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.occupied && !slot.active)
            .min_by_key(|(_, slot)| (slot.preload, slot.last_used))
            .map(|(index, _)| index)
    }
}

pub struct Vm<'a> {
    program: Program<'a>,
    runtime_strings: RuntimeStrings,
    state: [Value; MAX_STATE],
    stack: [Value; MAX_STACK],
    stack_len: usize,
    exited: bool,
    instructions: usize,
}

impl<'a> Program<'a> {
    pub fn parse_header(bytes: &[u8]) -> Result<SqbcHeader, VmError> {
        if bytes.len() < 16 || &bytes[0..4] != b"SQBC" {
            return Err(VmError::InvalidHeader);
        }
        if read_u16(bytes, 4)? != 3 {
            return Err(VmError::UnsupportedVersion);
        }
        let header_len = read_u16(bytes, 6)? as usize;
        let file_len = read_u32(bytes, 8)? as usize;
        let section_count = read_u32(bytes, 12)? as usize;
        if header_len != 16 + section_count * 12 || header_len < 16 {
            return Err(VmError::InvalidHeader);
        }
        Ok(SqbcHeader {
            header_len,
            file_len,
            section_count,
        })
    }

    pub fn parse_section_record(header_bytes: &[u8], index: usize) -> Result<SqbcSection, VmError> {
        let header = Self::parse_header(header_bytes)?;
        if index >= header.section_count || header_bytes.len() < header.header_len {
            return Err(VmError::InvalidSection);
        }
        let base = 16 + index * 12;
        let kind = read_u16(header_bytes, base)?;
        let offset = read_u32(header_bytes, base + 4)? as usize;
        let len = read_u32(header_bytes, base + 8)? as usize;
        let end = offset.checked_add(len).ok_or(VmError::InvalidSection)?;
        if offset < header.header_len || end > header.file_len {
            return Err(VmError::InvalidSection);
        }
        Ok(SqbcSection { kind, offset, len })
    }

    pub fn parse(bytes: &'a [u8]) -> Result<Self, VmError> {
        if bytes.len() > MAX_APP_BYTES {
            return Err(VmError::TooLarge);
        }
        let header = Self::parse_header(bytes)?;
        let header_len = header.header_len;
        let file_len = header.file_len;
        let section_count = header.section_count;
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

    pub fn handler_preload(&self, event: &str) -> Result<bool, VmError> {
        Ok(self.handler(event)?.preload)
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

impl StringTable for Program<'_> {
    fn string(&self, id: u16) -> Result<&str, VmError> {
        Program::string(self, id)
    }
}

pub trait SqbcReader {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError>;
}

pub trait ChunkedVmHost: SqbcReader + TraceSink {}

impl<T: SqbcReader + TraceSink> ChunkedVmHost for T {}

pub struct SliceSqbcReader<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceSqbcReader<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl SqbcReader for SliceSqbcReader<'_> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let end = offset
            .checked_add(out.len())
            .ok_or(VmError::InvalidSection)?;
        let bytes = self.bytes.get(offset..end).ok_or(VmError::InvalidSection)?;
        out.copy_from_slice(bytes);
        Ok(())
    }
}

pub struct ProgramIndex {
    string_bytes: [u8; MAX_PROGRAM_STRING_BYTES],
    string_offsets: [u16; MAX_STRINGS],
    string_lens: [u16; MAX_STRINGS],
    string_count: usize,
    state_slots: [StateSlot; MAX_STATE],
    state_count: usize,
    functions: [Function; MAX_FUNCTIONS],
    function_count: usize,
    handlers: [Handler; MAX_HANDLERS],
    handler_count: usize,
    screens: [Screen; MAX_SCREENS],
    screen_count: usize,
    code_offset: usize,
    code_len: usize,
}

impl ProgramIndex {
    pub fn parse_from_reader(
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<Self, VmError> {
        let mut fixed_header = [0u8; 16];
        reader.read_exact_at(0, &mut fixed_header)?;
        let header = Program::parse_header(&fixed_header)?;
        if header.header_len > scratch.len() {
            return Err(VmError::InvalidHeader);
        }
        reader.read_exact_at(0, &mut scratch[..header.header_len])?;

        let mut strings_section = None;
        let mut state_section = None;
        let mut functions_section = None;
        let mut handlers_section = None;
        let mut screens_section = None;
        let mut code_section = None;
        for index in 0..header.section_count {
            let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
            match record.kind {
                SECTION_STRINGS => strings_section = Some(record),
                SECTION_STATE => state_section = Some(record),
                SECTION_FUNCTIONS => functions_section = Some(record),
                SECTION_HANDLERS => handlers_section = Some(record),
                SECTION_SCREENS => screens_section = Some(record),
                SECTION_CODE => code_section = Some(record),
                _ => {}
            }
        }

        let strings_section = strings_section.ok_or(VmError::MissingSection)?;
        let state_section = state_section.ok_or(VmError::MissingSection)?;
        let functions_section = functions_section.ok_or(VmError::MissingSection)?;
        let handlers_section = handlers_section.ok_or(VmError::MissingSection)?;
        let code_section = code_section.ok_or(VmError::MissingSection)?;

        if strings_section.len > scratch.len()
            || state_section.len > scratch.len()
            || functions_section.len > scratch.len()
            || handlers_section.len > scratch.len()
            || screens_section.is_some_and(|section| section.len > scratch.len())
        {
            return Err(VmError::InvalidSection);
        }

        reader.read_exact_at(strings_section.offset, &mut scratch[..strings_section.len])?;
        let (string_bytes, string_offsets, string_lens, string_count) =
            parse_owned_strings(&scratch[..strings_section.len])?;

        reader.read_exact_at(state_section.offset, &mut scratch[..state_section.len])?;
        let (state_slots, state_count) = parse_state(&scratch[..state_section.len])?;

        reader.read_exact_at(
            functions_section.offset,
            &mut scratch[..functions_section.len],
        )?;
        let (functions, function_count) =
            parse_functions(&scratch[..functions_section.len], code_section.len)?;

        reader.read_exact_at(
            handlers_section.offset,
            &mut scratch[..handlers_section.len],
        )?;
        let (handlers, handler_count) =
            parse_handlers(&scratch[..handlers_section.len], code_section.len)?;

        let (screens, screen_count) = if let Some(section) = screens_section {
            reader.read_exact_at(section.offset, &mut scratch[..section.len])?;
            parse_screens(Some(&scratch[..section.len]), code_section.len)?
        } else {
            parse_screens(None, code_section.len)?
        };

        Ok(Self {
            string_bytes,
            string_offsets,
            string_lens,
            string_count,
            state_slots,
            state_count,
            functions,
            function_count,
            handlers,
            handler_count,
            screens,
            screen_count,
            code_offset: code_section.offset,
            code_len: code_section.len,
        })
    }

    pub fn parse(bytes: &[u8], scratch: &mut [u8]) -> Result<Self, VmError> {
        let mut reader = SliceSqbcReader::new(bytes);
        Self::parse_from_reader(&mut reader, scratch)
    }

    pub fn string(&self, id: u16) -> Result<&str, VmError> {
        let index = id as usize;
        if index >= self.string_count {
            return Err(VmError::InvalidOperand);
        }
        let start = self.string_offsets[index] as usize;
        let len = self.string_lens[index] as usize;
        let end = start.checked_add(len).ok_or(VmError::InvalidSection)?;
        str::from_utf8(
            self.string_bytes
                .get(start..end)
                .ok_or(VmError::InvalidSection)?,
        )
        .map_err(|_| VmError::InvalidUtf8)
    }

    fn handler(&self, event: &str) -> Result<(usize, Handler), VmError> {
        for (index, handler) in self.handlers.iter().take(self.handler_count).enumerate() {
            if self.string(handler.event_id)? == event {
                return Ok((index, *handler));
            }
        }
        Err(VmError::HandlerNotFound)
    }

    pub fn handler_preload(&self, event: &str) -> Result<bool, VmError> {
        Ok(self.handler(event)?.1.preload)
    }

    fn screen(&self, name: &str) -> Result<(usize, Screen), VmError> {
        for (index, screen) in self.screens.iter().take(self.screen_count).enumerate() {
            if self.string(screen.name_id)? == name {
                return Ok((index, *screen));
            }
        }
        Err(VmError::InvalidOperand)
    }

    pub const fn code_cache_bytes(&self) -> usize {
        MAX_CODE_CHUNK_BYTES
    }
}

impl StringTable for ProgramIndex {
    fn string(&self, id: u16) -> Result<&str, VmError> {
        ProgramIndex::string(self, id)
    }
}

pub struct ChunkedVm {
    index: ProgramIndex,
    runtime_strings: RuntimeStrings,
    state: [Value; MAX_STATE],
    stack: [Value; MAX_STACK],
    stack_len: usize,
    exited: bool,
    instructions: usize,
    chunk_cache: ChunkCache<4>,
    code: [u8; MAX_CODE_CHUNK_BYTES],
    code_start: usize,
    code_len: usize,
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
            state,
            stack: [Value::Null; MAX_STACK],
            stack_len: 0,
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
        result
    }

    pub fn exited(&self) -> bool {
        self.exited
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
                let (screen_index, screen) = self.index.screen(self.index.string(name_id)?)?;
                let mut locals = [Value::Null; MAX_LOCALS];
                let key = ChunkRef {
                    app: 0,
                    kind: ChunkKind::Screen,
                    index: screen_index as u16,
                };
                self.chunk_cache.insert(key, false).ok();
                self.chunk_cache.begin_execute(key).ok();
                let result =
                    self.execute_range(host, screen.start, screen.len, &mut locals, depth + 1);
                self.chunk_cache.end_execute(key).ok();
                result?;
            }
            BUILTIN_DISPLAY_CLEAR => {
                let Value::String(color_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.draw_clear(self.index.string(color_id)?);
            }
            BUILTIN_DISPLAY_TEXT => {
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                let text = self.pop()?;
                let strings = StringResolver::new(&self.index, &self.runtime_strings);
                host.draw_text(&strings, text, x, y);
            }
            BUILTIN_DISPLAY_RECT => {
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                host.draw_rect(x, y, w, h);
            }
            BUILTIN_DISPLAY_LINE => {
                let y2 = self.pop()?.expect_i32()?;
                let x2 = self.pop()?.expect_i32()?;
                let y1 = self.pop()?.expect_i32()?;
                let x1 = self.pop()?.expect_i32()?;
                host.draw_line(x1, y1, x2, y2);
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
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                let text = self.pop()?;
                let strings = StringResolver::new(&self.program, &self.runtime_strings);
                trace.draw_text(&strings, text, x, y);
            }
            BUILTIN_DISPLAY_RECT => {
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                trace.draw_rect(x, y, w, h);
            }
            BUILTIN_DISPLAY_LINE => {
                let y2 = self.pop()?.expect_i32()?;
                let x2 = self.pop()?.expect_i32()?;
                let y1 = self.pop()?.expect_i32()?;
                let x1 = self.pop()?.expect_i32()?;
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

fn parse_owned_strings(
    bytes: &[u8],
) -> Result<
    (
        [u8; MAX_PROGRAM_STRING_BYTES],
        [u16; MAX_STRINGS],
        [u16; MAX_STRINGS],
        usize,
    ),
    VmError,
> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_STRINGS {
        return Err(VmError::TooManyStrings);
    }
    let mut string_bytes = [0u8; MAX_PROGRAM_STRING_BYTES];
    let mut string_offsets = [0u16; MAX_STRINGS];
    let mut string_lens = [0u16; MAX_STRINGS];
    let mut cursor = 2usize;
    let mut pool_len = 0usize;
    for index in 0..count {
        let len = read_u16(bytes, cursor)? as usize;
        cursor += 2;
        let end = cursor.checked_add(len).ok_or(VmError::InvalidSection)?;
        let raw = bytes.get(cursor..end).ok_or(VmError::InvalidSection)?;
        str::from_utf8(raw).map_err(|_| VmError::InvalidUtf8)?;
        let pool_end = pool_len.checked_add(len).ok_or(VmError::InvalidSection)?;
        if pool_end > string_bytes.len() {
            return Err(VmError::TooManyStrings);
        }
        string_offsets[index] = pool_len as u16;
        string_lens[index] = len as u16;
        string_bytes[pool_len..pool_end].copy_from_slice(raw);
        pool_len = pool_end;
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((string_bytes, string_offsets, string_lens, count))
}

fn parse_state(bytes: &[u8]) -> Result<([StateSlot; MAX_STATE], usize), VmError> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_STATE {
        return Err(VmError::TooManyStateSlots);
    }
    let mut slots = [StateSlot {
        name_id: 0,
        value_type: StateType {
            tag: STATE_TYPE_INT,
            nullable: false,
        },
        default: Value::Null,
    }; MAX_STATE];
    let mut cursor = 2usize;
    for slot in slots.iter_mut().take(count) {
        let name_id = read_u16(bytes, cursor)?;
        cursor += 2;
        let tag = *bytes.get(cursor).ok_or(VmError::InvalidSection)?;
        cursor += 1;
        if !matches!(tag, STATE_TYPE_INT | STATE_TYPE_BOOL | STATE_TYPE_STRING) {
            return Err(VmError::InvalidSection);
        }
        let nullable = match *bytes.get(cursor).ok_or(VmError::InvalidSection)? {
            0 => false,
            1 => true,
            _ => return Err(VmError::InvalidSection),
        };
        cursor += 1;
        let (value, next) = read_value(bytes, cursor)?;
        cursor = next;
        if !state_value_matches(tag, nullable, value) {
            return Err(VmError::InvalidSection);
        }
        *slot = StateSlot {
            name_id,
            value_type: StateType { tag, nullable },
            default: value,
        };
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((slots, count))
}

fn state_value_matches(tag: u8, nullable: bool, value: Value) -> bool {
    if value == Value::Null {
        return nullable;
    }
    matches!(
        (tag, value),
        (STATE_TYPE_INT, Value::I32(_))
            | (STATE_TYPE_BOOL, Value::Bool(_))
            | (STATE_TYPE_STRING, Value::String(_))
            | (STATE_TYPE_STRING, Value::RuntimeString(_))
    )
}

fn values_equal(
    strings: &dyn StringTable,
    runtime_strings: &RuntimeStrings,
    left: Value,
    right: Value,
) -> Result<bool, VmError> {
    if left.is_string() || right.is_string() {
        if !left.is_string() || !right.is_string() {
            return Ok(false);
        }
        let resolver = StringResolver::new(strings, runtime_strings);
        return Ok(resolver.value_str(left)? == resolver.value_str(right)?);
    }
    Ok(left == right)
}

fn concat_value_strings(
    strings: &dyn StringTable,
    runtime_strings: &RuntimeStrings,
    left: Value,
    right: Value,
    out: &mut [u8; MAX_RUNTIME_STRING_BYTES],
) -> Result<usize, VmError> {
    let resolver = StringResolver::new(strings, runtime_strings);
    let left = resolver.value_str(left)?.as_bytes();
    let right = resolver.value_str(right)?.as_bytes();
    let len = left
        .len()
        .checked_add(right.len())
        .ok_or(VmError::InvalidOperand)?;
    if len > out.len() {
        return Err(VmError::InvalidOperand);
    }
    out[..left.len()].copy_from_slice(left);
    out[left.len()..len].copy_from_slice(right);
    Ok(len)
}

fn encode_state_record(
    strings: &dyn StringTable,
    runtime_strings: &RuntimeStrings,
    slots: &[StateSlot],
    state: &[Value],
    out: &mut [u8; MAX_SAVED_STATE_BYTES],
) -> Result<usize, VmError> {
    let mut cursor = 0usize;
    write_bytes(out, &mut cursor, STATE_RECORD_MAGIC)?;
    write_byte(out, &mut cursor, STATE_RECORD_VERSION)?;
    write_byte(out, &mut cursor, slots.len() as u8)?;
    let resolver = StringResolver::new(strings, runtime_strings);
    for (slot, value) in slots.iter().zip(state.iter().copied()) {
        let name = strings.string(slot.name_id)?;
        write_len_prefixed(out, &mut cursor, name.as_bytes())?;
        write_byte(out, &mut cursor, slot.value_type.tag)?;
        write_byte(out, &mut cursor, u8::from(slot.value_type.nullable))?;
        encode_state_record_value(out, &mut cursor, &resolver, value)?;
    }
    Ok(cursor)
}

fn encode_state_record_value(
    out: &mut [u8; MAX_SAVED_STATE_BYTES],
    cursor: &mut usize,
    strings: &StringResolver<'_>,
    value: Value,
) -> Result<(), VmError> {
    match value {
        Value::Null => write_byte(out, cursor, VALUE_NULL),
        Value::Bool(value) => {
            write_byte(out, cursor, VALUE_BOOL)?;
            write_byte(out, cursor, u8::from(value))
        }
        Value::I32(value) => {
            write_byte(out, cursor, VALUE_I32)?;
            write_bytes(out, cursor, &value.to_le_bytes())
        }
        Value::String(_) | Value::RuntimeString(_) => {
            write_byte(out, cursor, VALUE_STRING)?;
            write_len_prefixed(out, cursor, strings.value_str(value)?.as_bytes())
        }
    }
}

fn apply_state_record(
    bytes: &[u8],
    strings: &dyn StringTable,
    slots: &[StateSlot],
    runtime_strings: &mut RuntimeStrings,
    state: &mut [Value],
) -> Result<(), VmError> {
    if bytes.len() > MAX_SAVED_STATE_BYTES || bytes.len() < 6 {
        return Err(VmError::InvalidStateRecord);
    }
    if bytes.get(0..4) != Some(&STATE_RECORD_MAGIC[..]) {
        return Err(VmError::InvalidStateRecord);
    }
    if *bytes.get(4).ok_or(VmError::InvalidStateRecord)? != STATE_RECORD_VERSION {
        return Err(VmError::InvalidStateRecord);
    }
    let count = *bytes.get(5).ok_or(VmError::InvalidStateRecord)? as usize;
    let mut cursor = 6usize;
    for _ in 0..count {
        let name = read_len_prefixed(bytes, &mut cursor)?;
        let tag = read_byte(bytes, &mut cursor)?;
        if !matches!(tag, STATE_TYPE_INT | STATE_TYPE_BOOL | STATE_TYPE_STRING) {
            return Err(VmError::InvalidStateRecord);
        }
        let nullable = match read_byte(bytes, &mut cursor)? {
            0 => false,
            1 => true,
            _ => return Err(VmError::InvalidStateRecord),
        };
        let value = read_state_record_value(bytes, &mut cursor, tag, nullable)?;
        let mut matched = None;
        for (index, slot) in slots.iter().enumerate() {
            if strings.string(slot.name_id)?.as_bytes() == name {
                matched = Some((index, slot));
                break;
            }
        }
        let Some((index, slot)) = matched else {
            continue;
        };
        if slot.value_type.tag != tag || slot.value_type.nullable != nullable {
            return Err(VmError::StateTypeMismatch);
        }
        state[index] = materialize_state_value(value, runtime_strings)?;
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidStateRecord);
    }
    Ok(())
}

enum SavedStateValue<'a> {
    Null,
    Bool(bool),
    I32(i32),
    String(&'a str),
}

fn read_state_record_value<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    tag: u8,
    nullable: bool,
) -> Result<SavedStateValue<'a>, VmError> {
    let value_tag = read_byte(bytes, cursor)?;
    let value = match value_tag {
        VALUE_NULL => SavedStateValue::Null,
        VALUE_BOOL => SavedStateValue::Bool(read_byte(bytes, cursor)? != 0),
        VALUE_I32 => {
            let end = cursor.checked_add(4).ok_or(VmError::InvalidStateRecord)?;
            let raw = bytes.get(*cursor..end).ok_or(VmError::InvalidStateRecord)?;
            *cursor = end;
            SavedStateValue::I32(i32::from_le_bytes(
                raw.try_into().map_err(|_| VmError::InvalidStateRecord)?,
            ))
        }
        VALUE_STRING => {
            let value = read_len_prefixed(bytes, cursor)?;
            let text = str::from_utf8(value).map_err(|_| VmError::InvalidStateRecord)?;
            SavedStateValue::String(text)
        }
        _ => return Err(VmError::InvalidStateRecord),
    };
    if !saved_state_value_matches(tag, nullable, &value) {
        return Err(VmError::StateTypeMismatch);
    }
    Ok(value)
}

fn saved_state_value_matches(tag: u8, nullable: bool, value: &SavedStateValue<'_>) -> bool {
    match value {
        SavedStateValue::Null => nullable,
        SavedStateValue::Bool(_) => tag == STATE_TYPE_BOOL,
        SavedStateValue::I32(_) => tag == STATE_TYPE_INT,
        SavedStateValue::String(value) => {
            tag == STATE_TYPE_STRING && value.len() <= MAX_RUNTIME_STRING_BYTES
        }
    }
}

fn materialize_state_value(
    value: SavedStateValue<'_>,
    runtime_strings: &mut RuntimeStrings,
) -> Result<Value, VmError> {
    match value {
        SavedStateValue::Null => Ok(Value::Null),
        SavedStateValue::Bool(value) => Ok(Value::Bool(value)),
        SavedStateValue::I32(value) => Ok(Value::I32(value)),
        SavedStateValue::String(value) => {
            let mut writer = runtime_strings.alloc()?;
            writer
                .write_str(value)
                .map_err(|_| VmError::InvalidStateRecord)?;
            Ok(writer.value())
        }
    }
}

fn write_byte(
    out: &mut [u8; MAX_SAVED_STATE_BYTES],
    cursor: &mut usize,
    byte: u8,
) -> Result<(), VmError> {
    if *cursor >= out.len() {
        return Err(VmError::StateTooLarge);
    }
    out[*cursor] = byte;
    *cursor += 1;
    Ok(())
}

fn write_bytes(
    out: &mut [u8; MAX_SAVED_STATE_BYTES],
    cursor: &mut usize,
    bytes: &[u8],
) -> Result<(), VmError> {
    let end = (*cursor)
        .checked_add(bytes.len())
        .ok_or(VmError::StateTooLarge)?;
    if end > out.len() {
        return Err(VmError::StateTooLarge);
    }
    out[*cursor..end].copy_from_slice(bytes);
    *cursor = end;
    Ok(())
}

fn write_len_prefixed(
    out: &mut [u8; MAX_SAVED_STATE_BYTES],
    cursor: &mut usize,
    bytes: &[u8],
) -> Result<(), VmError> {
    let len = u8::try_from(bytes.len()).map_err(|_| VmError::StateTooLarge)?;
    write_byte(out, cursor, len)?;
    write_bytes(out, cursor, bytes)
}

fn read_byte(bytes: &[u8], cursor: &mut usize) -> Result<u8, VmError> {
    let byte = *bytes.get(*cursor).ok_or(VmError::InvalidStateRecord)?;
    *cursor += 1;
    Ok(byte)
}

fn read_len_prefixed<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], VmError> {
    let len = read_byte(bytes, cursor)? as usize;
    let end = (*cursor)
        .checked_add(len)
        .ok_or(VmError::InvalidStateRecord)?;
    let value = bytes.get(*cursor..end).ok_or(VmError::InvalidStateRecord)?;
    *cursor = end;
    Ok(value)
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
        preload: false,
        start: 0,
        len: 0,
    }; MAX_HANDLERS];
    let mut cursor = 2usize;
    for handler in handlers.iter_mut().take(count) {
        let event_id = read_u16(bytes, cursor)?;
        let preload = read_u16(bytes, cursor + 2)? != 0;
        let start = read_u32(bytes, cursor + 4)? as usize;
        let len = read_u32(bytes, cursor + 8)? as usize;
        cursor += 12;
        validate_range(start, len, code_len)?;
        *handler = Handler {
            event_id,
            preload,
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

        fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
            let mut line = String::new();
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    line.push(' ');
                }
                match value {
                    Value::String(_) | Value::RuntimeString(_) => {
                        line.push_str(strings.value_str(*value).unwrap())
                    }
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

        fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
            self.events.push(format!("arm {app}"));
            Ok(())
        }

        fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
            self.events.push(format!("disarm {app}"));
            Ok(())
        }

        fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
            self.events
                .push(format!("service.timer.every {event} {interval_ms}"));
            Ok(())
        }

        fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
            self.events
                .push(format!("service.timer.after {event} {delay_ms}"));
            Ok(())
        }

        fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
            write!(out, "RAM 292 KiB").map_err(|_| VmError::InvalidOperand)
        }

        fn system_storage_text(
            &mut self,
            name: &str,
            out: &mut dyn fmt::Write,
        ) -> Result<(), VmError> {
            write!(out, "{name} 1 MiB").map_err(|_| VmError::InvalidOperand)
        }
    }

    #[derive(Default)]
    struct StateTrace {
        events: Vec<String>,
        saved_state: Vec<u8>,
        reset_count: usize,
    }

    impl TraceSink for StateTrace {
        fn trace(&mut self, message: &str) {
            self.events.push(message.to_string());
        }

        fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
            if self.saved_state.is_empty() {
                return Ok(None);
            }
            out[..self.saved_state.len()].copy_from_slice(&self.saved_state);
            Ok(Some(self.saved_state.len()))
        }

        fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
            self.saved_state = bytes.to_vec();
            Ok(())
        }

        fn state_reset_persistent(&mut self) -> Result<(), VmError> {
            self.saved_state.clear();
            self.reset_count += 1;
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
    fn runs_sqbc_v3_emitted_by_squidc_core() {
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
    fn state_save_load_and_reset_use_typed_persistent_record() {
        let source = r#"app "state-demo"
state {
  count: int = 0
  enabled: bool = false
  label: string = "cold"
  retryAt: int? = null
}
event.on("app.start") {
  state.load()
}
event.on("key.SELECT") {
  count = count + 7
  enabled = true
  label = label + "-hot"
  retryAt = 42
  state.save()
}
event.on("key.BACK") {
  state.reset()
}
screen("main") {}
"#;
        let bytes = compile_sqbc(source);
        let mut trace = StateTrace::default();
        {
            let program = Program::parse(&bytes).unwrap();
            let mut vm = Vm::new(program);
            vm.dispatch("app.start", &mut trace).unwrap();
            vm.dispatch("key.SELECT", &mut trace).unwrap();
            assert_eq!(vm.state_value("count"), Ok(Value::I32(7)));
            assert!(!trace.saved_state.is_empty());
        }

        let program = Program::parse(&bytes).unwrap();
        let mut restored = Vm::new(program);
        restored.dispatch("app.start", &mut trace).unwrap();
        assert_eq!(restored.state_value("count"), Ok(Value::I32(7)));
        assert_eq!(restored.state_value("enabled"), Ok(Value::Bool(true)));
        assert_eq!(restored.state_value("retryAt"), Ok(Value::I32(42)));
        assert_eq!(
            restored
                .string_resolver()
                .value_str(restored.state_value("label").unwrap()),
            Ok("cold-hot")
        );

        restored.dispatch("key.BACK", &mut trace).unwrap();
        assert_eq!(restored.state_value("count"), Ok(Value::I32(0)));
        assert!(trace.saved_state.is_empty());
        assert_eq!(trace.reset_count, 1);
    }

    #[test]
    fn state_load_rejects_malformed_record_and_type_mismatch() {
        let source = r#"app "state-demo"
state { count: int = 0 }
event.on("app.start") {
  state.load()
}
screen("main") {}
"#;
        let bytes = compile_sqbc(source);
        let mut trace = StateTrace {
            saved_state: b"bad".to_vec(),
            ..StateTrace::default()
        };
        let program = Program::parse(&bytes).unwrap();
        let mut vm = Vm::new(program);
        assert_eq!(
            vm.dispatch("app.start", &mut trace),
            Err(VmError::InvalidStateRecord)
        );

        trace.saved_state = mismatched_count_state_record();
        let program = Program::parse(&bytes).unwrap();
        let mut vm = Vm::new(program);
        assert_eq!(
            vm.dispatch("app.start", &mut trace),
            Err(VmError::StateTypeMismatch)
        );
    }

    #[test]
    fn parses_sqbc_v3_header_and_section_records_for_partial_loading() {
        let bytes = fixture_counter_sqbc();
        let header = Program::parse_header(&bytes[..16]).unwrap();
        let header_bytes = &bytes[..header.header_len];

        assert_eq!(header.file_len, bytes.len());
        assert_eq!(header.section_count, 5);
        let first = Program::parse_section_record(header_bytes, 0).unwrap();
        assert_eq!(first.kind, SECTION_STRINGS);
        assert!(first.offset >= header.header_len);
        assert!(first.len > 0);
    }

    #[test]
    fn parses_preload_handler_metadata_from_real_bytecode() {
        let source = r#"app "preload-demo"
@preload
event.on("key.SELECT") {
  debug.print("fast")
}
event.on("key.BACK") {
  app.exit()
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

        assert_eq!(program.handler_preload("key.SELECT"), Ok(true));
        assert_eq!(program.handler_preload("key.BACK"), Ok(false));
    }

    #[test]
    fn program_index_parses_metadata_without_full_app_read() {
        let bytes = fixture_counter_sqbc();
        let mut reader = CountingReader::new(&bytes);
        let mut scratch = [0u8; MAX_APP_BYTES];

        let index = ProgramIndex::parse_from_reader(&mut reader, &mut scratch).unwrap();

        assert_eq!(index.string(3), Ok("app.start"));
        assert!(reader.reads.iter().all(|(_, len)| *len < bytes.len()));
    }

    #[test]
    fn chunked_vm_reads_handler_code_range_from_reader() {
        let bytes = fixture_counter_sqbc();
        let mut scratch = [0u8; MAX_APP_BYTES];
        let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
        let mut reader = CountingReader::new(&bytes);
        let mut vm = ChunkedVm::new(index);

        vm.dispatch(&mut reader, "app.start").unwrap();

        assert_eq!(vm.state_value("started"), Ok(Value::I32(1)));
        assert!(reader
            .reads
            .iter()
            .any(|(_, len)| *len > 0 && *len <= MAX_CODE_CHUNK_BYTES));
    }

    #[test]
    fn chunked_vm_rejects_oversized_handler_chunk() {
        let strings = encode_strings(&["oversized", "app.start"]);
        let state = vec![0, 0];
        let functions = vec![0, 0];
        let mut handlers = Vec::new();
        push_u16(&mut handlers, 1);
        push_u16(&mut handlers, 1);
        push_u16(&mut handlers, 0);
        push_u32(&mut handlers, 0);
        push_u32(&mut handlers, (MAX_CODE_CHUNK_BYTES + 1) as u32);
        let code = vec![OP_HALT; MAX_CODE_CHUNK_BYTES + 1];
        let bytes = encode_container(vec![
            (SECTION_STRINGS, strings),
            (SECTION_STATE, state),
            (SECTION_FUNCTIONS, functions),
            (SECTION_HANDLERS, handlers),
            (SECTION_CODE, code),
        ]);
        let mut scratch = [0u8; MAX_APP_BYTES];
        let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
        let mut reader = CountingReader::new(&bytes);
        let mut vm = ChunkedVm::new(index);

        assert_eq!(
            vm.dispatch(&mut reader, "app.start"),
            Err(VmError::ChunkTooLarge)
        );
    }

    #[test]
    fn chunked_vm_dispatches_functions_and_screens_on_demand() {
        let source = r#"app "chunk-demo"
state { count: int = 0 }
function bump() {
  count = count + 1
}
event.on("app.start") {
  bump()
  screen.open("main")
}
screen("main") {
  bump()
}
"#;
        let compiled = squidc_core::compile(squidc_core::CompileRequest {
            source: source.to_string(),
            target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
        });
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        let bytes = squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap();
        let mut scratch = [0u8; MAX_APP_BYTES];
        let index = ProgramIndex::parse(&bytes, &mut scratch).unwrap();
        let mut reader = CountingReader::new(&bytes);
        let mut vm = ChunkedVm::new(index);

        vm.dispatch(&mut reader, "app.start").unwrap();

        assert_eq!(vm.state_value("count"), Ok(Value::I32(2)));
        assert_eq!(reader.events, vec!["app.start"]);
    }

    #[test]
    fn chunk_cache_prefers_evicting_cold_unhinted_chunks() {
        let hot = ChunkRef {
            app: 1,
            kind: ChunkKind::Handler,
            index: 0,
        };
        let cold = ChunkRef {
            app: 1,
            kind: ChunkKind::Handler,
            index: 1,
        };
        let incoming = ChunkRef {
            app: 1,
            kind: ChunkKind::Handler,
            index: 2,
        };
        let mut cache = ChunkCache::<2>::new();

        cache.insert(hot, true).unwrap();
        cache.insert(cold, false).unwrap();
        cache.begin_execute(hot).unwrap();
        cache.insert(incoming, false).unwrap();

        assert!(cache.contains(hot));
        assert!(cache.contains(incoming));
        assert!(!cache.contains(cold));
    }

    #[test]
    fn chunk_cache_drops_all_chunks_for_replaced_app() {
        let app_one = ChunkRef {
            app: 1,
            kind: ChunkKind::Handler,
            index: 0,
        };
        let app_two = ChunkRef {
            app: 2,
            kind: ChunkKind::Handler,
            index: 0,
        };
        let mut cache = ChunkCache::<2>::new();

        cache.insert(app_one, true).unwrap();
        cache.insert(app_two, false).unwrap();
        cache.drop_app(1);

        assert!(!cache.contains(app_one));
        assert!(cache.contains(app_two));
    }

    #[test]
    fn runs_hardware_gpio_builtins_from_real_bytecode() {
        let source = r#"app "gpio" target "esp32c3-super-mini"
state { led: bool = false }
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

    struct CountingReader<'a> {
        bytes: &'a [u8],
        reads: Vec<(usize, usize)>,
        events: Vec<String>,
        saved_state: Vec<u8>,
    }

    impl<'a> CountingReader<'a> {
        fn new(bytes: &'a [u8]) -> Self {
            Self {
                bytes,
                reads: Vec::new(),
                events: Vec::new(),
                saved_state: Vec::new(),
            }
        }
    }

    impl SqbcReader for CountingReader<'_> {
        fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
            let end = offset
                .checked_add(out.len())
                .ok_or(VmError::InvalidSection)?;
            let bytes = self.bytes.get(offset..end).ok_or(VmError::InvalidSection)?;
            out.copy_from_slice(bytes);
            self.reads.push((offset, out.len()));
            Ok(())
        }
    }

    impl TraceSink for CountingReader<'_> {
        fn trace(&mut self, message: &str) {
            self.events.push(message.to_string());
        }

        fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
            if self.saved_state.is_empty() {
                return Ok(None);
            }
            out[..self.saved_state.len()].copy_from_slice(&self.saved_state);
            Ok(Some(self.saved_state.len()))
        }

        fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
            self.saved_state = bytes.to_vec();
            Ok(())
        }

        fn state_reset_persistent(&mut self) -> Result<(), VmError> {
            self.saved_state.clear();
            Ok(())
        }
    }

    #[test]
    fn runs_app_launch_timer_service_and_timer_handler_from_real_bytecode() {
        let source = r#"app "timer-demo"
state { count: int = 0 }
event.on("app.start") {
  app.launch("timer-armed-app")
  service.timer.every("timer.debug", 1000)
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
                "launch timer-armed-app",
                "service.timer.every timer.debug 1000",
                "timer.debug",
                "debug timer 0",
            ]
        );
    }

    #[test]
    fn runs_generic_event_lifecycle_builtins_from_real_bytecode() {
        let source = r#"app "event-demo"
state { count: int = 0 }
event.on("app.start") {
  app.arm("break-reminder")
  app.launch("reader")
  service.timer.every("timer.clock", 60000)
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
                "launch reader",
                "service.timer.every timer.clock 60000",
                "timer.clock",
                "disarm break-reminder",
            ]
        );
    }

    #[test]
    fn runs_system_resource_string_builtins_from_real_bytecode() {
        let source = r#"app "resources"
event.on("app.start") {
  debug.print(system.memory())
  debug.print(system.storage("apps"))
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

        assert_eq!(
            trace.events,
            vec!["app.start", "debug RAM 292 KiB", "debug apps 1 MiB"]
        );
    }

    #[test]
    fn state_reset_restores_typed_defaults() {
        let source = r#"app "reset-demo"
state {
  count: int = 4
}
event.on("app.start") {
  count = count + 1
  state.reset()
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
        let mut trace = Trace::default();

        vm.dispatch("app.start", &mut trace).unwrap();

        assert_eq!(vm.state_value("count"), Ok(Value::I32(4)));
        assert_eq!(trace.events, vec!["app.start", "state.reset"]);
    }

    #[test]
    fn rejects_mixed_arithmetic_without_null_or_bool_coercion() {
        let source = r#"app "bad-math"
state {
  count: int = 0
  label: string = "count"
}
event.on("app.start") {
  count = label + 1
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
        let mut trace = Trace::default();

        assert_eq!(
            vm.dispatch("app.start", &mut trace),
            Err(VmError::InvalidOperand)
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
        state.push(STATE_TYPE_INT);
        state.push(0);
        state.push(VALUE_I32);
        push_i32(&mut state, 0);
        push_u16(&mut state, 2);
        state.push(STATE_TYPE_INT);
        state.push(0);
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
        push_u16(&mut handlers, 0);
        push_u32(&mut handlers, on_start as u32);
        push_u32(&mut handlers, (select - on_start) as u32);
        push_u16(&mut handlers, 4);
        push_u16(&mut handlers, 0);
        push_u32(&mut handlers, select as u32);
        push_u32(&mut handlers, (back - select) as u32);
        push_u16(&mut handlers, 5);
        push_u16(&mut handlers, 0);
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

    fn compile_sqbc(source: &str) -> Vec<u8> {
        let compiled = squidc_core::compile(squidc_core::CompileRequest {
            source: source.to_string(),
            target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
        });
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        squidc_core::sqbc_v2::encode_sqbc_v2(&compiled.ir.unwrap()).unwrap()
    }

    fn mismatched_count_state_record() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(STATE_RECORD_MAGIC);
        out.push(STATE_RECORD_VERSION);
        out.push(1);
        out.push(5);
        out.extend_from_slice(b"count");
        out.push(STATE_TYPE_BOOL);
        out.push(0);
        out.push(VALUE_BOOL);
        out.push(1);
        out
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
        push_u16(&mut out, 3);
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
