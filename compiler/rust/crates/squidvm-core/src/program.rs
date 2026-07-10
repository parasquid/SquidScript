use core::{mem::MaybeUninit, ptr, str};

use crate::{
    bytecode::{
        read_i32, read_u16, read_u32, BUILTIN_APP_ARM, BUILTIN_APP_ARMED_STACK,
        BUILTIN_APP_ARMED_STACK_GET, BUILTIN_APP_DISARM, BUILTIN_APP_EXIT, BUILTIN_APP_INSTALL,
        BUILTIN_APP_INSTALL_METADATA, BUILTIN_APP_LAUNCH, BUILTIN_APP_PROCESS_STACK,
        BUILTIN_APP_REGISTRY_GET, BUILTIN_APP_REGISTRY_LIST, BUILTIN_BINBOOK_CHAPTER,
        BUILTIN_BINBOOK_CHAPTERS, BUILTIN_BINBOOK_INFO, BUILTIN_BINBOOK_OPEN,
        BUILTIN_BINBOOK_READ_PAGE, BUILTIN_CONTENT_BINBOOK_LIST, BUILTIN_DEBUG_PRINT,
        BUILTIN_DISPLAY_CLEAR, BUILTIN_DISPLAY_DRAW, BUILTIN_DISPLAY_IMAGE, BUILTIN_DISPLAY_INFO,
        BUILTIN_DISPLAY_LINE, BUILTIN_DISPLAY_RECT, BUILTIN_DISPLAY_REFRESH_MODE,
        BUILTIN_DISPLAY_SELECT, BUILTIN_DISPLAY_TEXT, BUILTIN_FILE_COPY, BUILTIN_FILE_LIST,
        BUILTIN_FILE_PICK_FILE, BUILTIN_FILE_READ_LINES, BUILTIN_FILE_READ_TEXT,
        BUILTIN_HARDWARE_GPIO_READ, BUILTIN_HARDWARE_GPIO_TOGGLE, BUILTIN_HARDWARE_GPIO_WRITE,
        BUILTIN_SCREEN_OPEN, BUILTIN_SCREEN_REFRESH, BUILTIN_SERVICE_INDICATOR_BLINK,
        BUILTIN_SERVICE_INDICATOR_BREATHE, BUILTIN_SERVICE_INDICATOR_READ,
        BUILTIN_SERVICE_INDICATOR_TOGGLE, BUILTIN_SERVICE_INDICATOR_WRITE,
        BUILTIN_SERVICE_POWER_SLEEP, BUILTIN_SERVICE_TIMER_AFTER, BUILTIN_SERVICE_TIMER_EVERY,
        BUILTIN_SERVICE_UPLOAD_START, BUILTIN_SERVICE_UPLOAD_STATUS, BUILTIN_SERVICE_UPLOAD_STOP,
        BUILTIN_SERVICE_WIFI_CANCEL, BUILTIN_SERVICE_WIFI_CONNECT, BUILTIN_SERVICE_WIFI_DISCONNECT,
        BUILTIN_SERVICE_WIFI_GET_AP_IP, BUILTIN_SERVICE_WIFI_OPERATION,
        BUILTIN_SERVICE_WIFI_RESULT, BUILTIN_SERVICE_WIFI_SCAN, BUILTIN_SERVICE_WIFI_SCAN_NETWORK,
        BUILTIN_SERVICE_WIFI_START_AP, BUILTIN_SERVICE_WIFI_STATUS, BUILTIN_SERVICE_WIFI_STOP_AP,
        BUILTIN_STATE_LOAD, BUILTIN_STATE_RESET, BUILTIN_STATE_SAVE, OP_ADD, OP_CALL_BUILTIN,
        OP_CALL_FUNCTION, OP_EQ, OP_GET_FIELD, OP_GET_LOCAL, OP_GET_STATE, OP_GT, OP_GTE, OP_HALT,
        OP_JUMP, OP_JUMP_IF_FALSE, OP_LIST_GET, OP_LIST_LEN, OP_LT, OP_LTE, OP_NE, OP_POP,
        OP_PUSH_BOOL, OP_PUSH_INT, OP_PUSH_NULL, OP_PUSH_STRING, OP_RETURN, OP_SET_LOCAL,
        OP_SET_STATE, OP_SUB, SECTION_CODE, SECTION_DEVICE_BINDINGS, SECTION_FUNCTIONS,
        SECTION_HANDLERS, SECTION_SCREENS, SECTION_STATE, SECTION_STRINGS, SECTION_TRIGGERS,
        SECTION_UPLOAD_PROFILES,
    },
    error::VmError,
    limits::{
        MAX_APP_BYTES, MAX_CODE_CHUNK_BYTES, MAX_DEVICE_BINDINGS, MAX_FUNCTIONS, MAX_HANDLERS,
        MAX_LOCALS, MAX_PROGRAM_STRING_BYTES, MAX_SCREENS, MAX_STATE, MAX_STRINGS, MAX_TRIGGERS,
    },
    model::{Function, Handler, Screen, StateSlot, TriggerTimerMeta},
    reader::{SliceSqbcReader, SqbcReader},
    state::parse_state,
    strings::StringTable,
    value::{StringRef, Value},
};

const SQBC_HEADER_LEN: usize = 14;
const MAX_UPLOAD_PROFILES: usize = 16;
const MAX_UPLOAD_PROFILE_TEXT_ITEMS: usize = 4;
const MAX_UPLOAD_PROFILE_EVENTS: usize = 8;

pub struct Program<'a> {
    pub(crate) strings: [&'a str; MAX_STRINGS],
    pub(crate) string_count: usize,
    pub(crate) state_slots: [StateSlot; MAX_STATE],
    pub(crate) state_count: usize,
    pub(crate) functions: [Function; MAX_FUNCTIONS],
    pub(crate) function_count: usize,
    pub(crate) handlers: [Handler; MAX_HANDLERS],
    pub(crate) handler_count: usize,
    pub(crate) trigger_timers: [TriggerTimerMeta; MAX_TRIGGERS],
    pub(crate) trigger_timer_count: usize,
    pub(crate) screens: [Screen; MAX_SCREENS],
    pub(crate) screen_count: usize,
    pub(crate) code: &'a [u8],
    upload_ble: bool,
    upload_http: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapabilityDemand {
    pub wifi: bool,
    pub ble: bool,
    pub http: bool,
    pub display: bool,
    pub storage: bool,
    pub binbook: bool,
    pub hardware_gpio: bool,
    pub indicator: bool,
    pub timers: bool,
    pub power: bool,
    pub app_lifecycle: bool,
}

impl CapabilityDemand {
    pub const fn none() -> Self {
        Self {
            wifi: false,
            ble: false,
            http: false,
            display: false,
            storage: false,
            binbook: false,
            hardware_gpio: false,
            indicator: false,
            timers: false,
            power: false,
            app_lifecycle: false,
        }
    }

    fn apply_builtin(&mut self, builtin: u8) {
        match builtin {
            BUILTIN_STATE_LOAD | BUILTIN_STATE_SAVE | BUILTIN_STATE_RESET => {
                self.storage = true;
            }
            BUILTIN_APP_EXIT
            | BUILTIN_APP_LAUNCH
            | BUILTIN_APP_ARM
            | BUILTIN_APP_DISARM
            | BUILTIN_APP_REGISTRY_LIST
            | BUILTIN_APP_REGISTRY_GET
            | BUILTIN_APP_PROCESS_STACK
            | BUILTIN_APP_ARMED_STACK
            | BUILTIN_APP_ARMED_STACK_GET => {
                self.app_lifecycle = true;
            }
            BUILTIN_APP_INSTALL | BUILTIN_APP_INSTALL_METADATA => {
                self.app_lifecycle = true;
                self.storage = true;
            }
            BUILTIN_SCREEN_OPEN
            | BUILTIN_SCREEN_REFRESH
            | BUILTIN_DISPLAY_CLEAR
            | BUILTIN_DISPLAY_TEXT
            | BUILTIN_DISPLAY_RECT
            | BUILTIN_DISPLAY_LINE
            | BUILTIN_DISPLAY_SELECT
            | BUILTIN_DISPLAY_IMAGE
            | BUILTIN_DISPLAY_DRAW
            | BUILTIN_DISPLAY_INFO
            | BUILTIN_DISPLAY_REFRESH_MODE => {
                self.display = true;
            }
            BUILTIN_SERVICE_TIMER_EVERY | BUILTIN_SERVICE_TIMER_AFTER => {
                self.timers = true;
            }
            BUILTIN_HARDWARE_GPIO_WRITE
            | BUILTIN_HARDWARE_GPIO_TOGGLE
            | BUILTIN_HARDWARE_GPIO_READ => {
                self.hardware_gpio = true;
            }
            BUILTIN_SERVICE_INDICATOR_WRITE
            | BUILTIN_SERVICE_INDICATOR_TOGGLE
            | BUILTIN_SERVICE_INDICATOR_READ
            | BUILTIN_SERVICE_INDICATOR_BREATHE
            | BUILTIN_SERVICE_INDICATOR_BLINK => {
                self.indicator = true;
            }
            BUILTIN_SERVICE_WIFI_START_AP
            | BUILTIN_SERVICE_WIFI_STOP_AP
            | BUILTIN_SERVICE_WIFI_STATUS
            | BUILTIN_SERVICE_WIFI_GET_AP_IP
            | BUILTIN_SERVICE_WIFI_CONNECT
            | BUILTIN_SERVICE_WIFI_DISCONNECT
            | BUILTIN_SERVICE_WIFI_SCAN
            | BUILTIN_SERVICE_WIFI_OPERATION
            | BUILTIN_SERVICE_WIFI_RESULT
            | BUILTIN_SERVICE_WIFI_CANCEL
            | BUILTIN_SERVICE_WIFI_SCAN_NETWORK => {
                self.wifi = true;
            }
            BUILTIN_BINBOOK_OPEN
            | BUILTIN_BINBOOK_INFO
            | BUILTIN_BINBOOK_READ_PAGE
            | BUILTIN_BINBOOK_CHAPTERS
            | BUILTIN_BINBOOK_CHAPTER => {
                self.binbook = true;
                self.storage = true;
            }
            BUILTIN_CONTENT_BINBOOK_LIST => {
                self.binbook = true;
                self.storage = true;
            }
            BUILTIN_FILE_PICK_FILE
            | BUILTIN_FILE_READ_TEXT
            | BUILTIN_FILE_READ_LINES
            | BUILTIN_FILE_COPY
            | BUILTIN_FILE_LIST => {
                self.storage = true;
            }
            BUILTIN_SERVICE_POWER_SLEEP => {
                self.power = true;
            }
            BUILTIN_SERVICE_UPLOAD_START
            | BUILTIN_SERVICE_UPLOAD_STOP
            | BUILTIN_SERVICE_UPLOAD_STATUS => {}
            BUILTIN_DEBUG_PRINT => {}
            _ => {}
        }
    }
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

impl<'a> Program<'a> {
    pub fn parse_header(bytes: &[u8]) -> Result<SqbcHeader, VmError> {
        if bytes.len() < SQBC_HEADER_LEN || &bytes[0..4] != b"SQBC" {
            return Err(VmError::InvalidHeader);
        }
        let header_len = read_u16(bytes, 4)? as usize;
        let file_len = read_u32(bytes, 6)? as usize;
        let section_count = read_u32(bytes, 10)? as usize;
        if header_len != SQBC_HEADER_LEN + section_count * 12 || header_len < SQBC_HEADER_LEN {
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
        let base = SQBC_HEADER_LEN + index * 12;
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
            || header_len != SQBC_HEADER_LEN + section_count * 12
            || header_len > bytes.len()
        {
            return Err(VmError::InvalidHeader);
        }
        validate_unique_section_kinds(&bytes[..header_len], section_count)?;

        let strings = section(bytes, section_count, SECTION_STRINGS)?;
        let state = section(bytes, section_count, SECTION_STATE)?;
        let functions = section(bytes, section_count, SECTION_FUNCTIONS)?;
        let handlers = section(bytes, section_count, SECTION_HANDLERS)?;
        let triggers = optional_section(bytes, section_count, SECTION_TRIGGERS)?;
        let screens = optional_section(bytes, section_count, SECTION_SCREENS)?;
        let code = section(bytes, section_count, SECTION_CODE)?;
        let upload_profiles = optional_section(bytes, section_count, SECTION_UPLOAD_PROFILES)?;

        parse_owned_strings(strings)?;
        let (strings, string_count) = parse_strings(strings)?;
        let (state_slots, state_count) = parse_state(state)?;
        let (functions, function_count) = parse_functions(functions, code.len())?;
        let (handlers, handler_count) = parse_handlers(handlers, code.len())?;
        let (trigger_timers, trigger_timer_count) = parse_trigger_timers(triggers)?;
        let (screens, screen_count) = parse_screens(screens, code.len())?;
        let (upload_ble, upload_http) =
            upload_transport_demand(upload_profiles, &strings, string_count)?;
        validate_program_tables(
            &strings,
            string_count,
            &state_slots,
            state_count,
            &functions,
            function_count,
            &handlers,
            handler_count,
            &trigger_timers,
            trigger_timer_count,
            &screens,
            screen_count,
        )?;

        Ok(Self {
            strings,
            string_count,
            state_slots,
            state_count,
            functions,
            function_count,
            handlers,
            handler_count,
            trigger_timers,
            trigger_timer_count,
            screens,
            screen_count,
            code,
            upload_ble,
            upload_http,
        })
    }

    pub fn string(&self, id: u16) -> Result<&str, VmError> {
        let index = id as usize;
        if index >= self.string_count {
            return Err(VmError::InvalidOperand);
        }
        Ok(self.strings[index])
    }

    pub(crate) fn handler(&self, event: &str) -> Result<Handler, VmError> {
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

    pub fn trigger_timers(&self) -> Result<TriggerTimers<'_>, VmError> {
        TriggerTimers::new(self, &self.trigger_timers, self.trigger_timer_count)
    }

    pub fn capability_demand(&self) -> Result<CapabilityDemand, VmError> {
        let (mut demand, uses_upload) = capability_demand_from_code(self.code)?;
        if uses_upload {
            demand.ble = self.upload_ble;
            demand.http = self.upload_http;
        }
        Ok(demand)
    }

    #[allow(dead_code)]
    pub(crate) fn screen(&self, name: &str) -> Result<Screen, VmError> {
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

    fn find_string(&self, value: &str) -> Result<Option<u16>, VmError> {
        for index in 0..self.string_count {
            if self.strings[index] == value {
                return Ok(Some(index as u16));
            }
        }
        Ok(None)
    }
}

pub struct ProgramIndex {
    pub(crate) string_bytes: [u8; MAX_PROGRAM_STRING_BYTES],
    pub(crate) string_offsets: [u16; MAX_STRINGS],
    pub(crate) string_lens: [u16; MAX_STRINGS],
    pub(crate) string_count: usize,
    pub(crate) state_slots: [StateSlot; MAX_STATE],
    pub(crate) state_count: usize,
    pub(crate) functions: [Function; MAX_FUNCTIONS],
    pub(crate) function_count: usize,
    pub(crate) handlers: [Handler; MAX_HANDLERS],
    pub(crate) handler_count: usize,
    pub(crate) trigger_timers: [TriggerTimerMeta; MAX_TRIGGERS],
    pub(crate) trigger_timer_count: usize,
    pub(crate) screens: [Screen; MAX_SCREENS],
    pub(crate) screen_count: usize,
    pub(crate) code_offset: usize,
    pub(crate) code_len: usize,
}

impl ProgramIndex {
    pub(crate) fn from_program(program: &Program<'_>) -> Result<Self, VmError> {
        let mut string_bytes = [0u8; MAX_PROGRAM_STRING_BYTES];
        let mut string_offsets = [0u16; MAX_STRINGS];
        let mut string_lens = [0u16; MAX_STRINGS];
        let mut pool_len = 0usize;
        for (index, value) in program
            .strings
            .iter()
            .take(program.string_count)
            .enumerate()
        {
            let raw = value.as_bytes();
            let pool_end = pool_len
                .checked_add(raw.len())
                .ok_or(VmError::InvalidSection)?;
            if pool_end > string_bytes.len() || raw.len() > u16::MAX as usize {
                return Err(VmError::TooManyStrings);
            }
            string_offsets[index] = pool_len as u16;
            string_lens[index] = raw.len() as u16;
            string_bytes[pool_len..pool_end].copy_from_slice(raw);
            pool_len = pool_end;
        }

        Ok(Self {
            string_bytes,
            string_offsets,
            string_lens,
            string_count: program.string_count,
            state_slots: program.state_slots,
            state_count: program.state_count,
            functions: program.functions,
            function_count: program.function_count,
            handlers: program.handlers,
            handler_count: program.handler_count,
            trigger_timers: program.trigger_timers,
            trigger_timer_count: program.trigger_timer_count,
            screens: program.screens,
            screen_count: program.screen_count,
            code_offset: 0,
            code_len: program.code.len(),
        })
    }

    pub fn parse_from_reader(
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<Self, VmError> {
        let mut out = MaybeUninit::<Self>::uninit();
        unsafe {
            Self::parse_from_reader_in_place(out.as_mut_ptr(), reader, scratch)?;
            Ok(out.assume_init())
        }
    }

    pub unsafe fn parse_from_reader_in_place(
        out: *mut Self,
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<(), VmError> {
        let mut fixed_header = [0u8; SQBC_HEADER_LEN];
        reader.read_exact_at(0, &mut fixed_header)?;
        let header = Program::parse_header(&fixed_header)?;
        if header.header_len > scratch.len() {
            return Err(VmError::InvalidHeader);
        }
        reader.read_exact_at(0, &mut scratch[..header.header_len])?;
        validate_unique_section_kinds(&scratch[..header.header_len], header.section_count)?;

        let mut strings_section = None;
        let mut state_section = None;
        let mut functions_section = None;
        let mut handlers_section = None;
        let mut triggers_section = None;
        let mut screens_section = None;
        let mut code_section = None;
        for index in 0..header.section_count {
            let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
            match record.kind {
                SECTION_STRINGS => strings_section = Some(record),
                SECTION_STATE => state_section = Some(record),
                SECTION_FUNCTIONS => functions_section = Some(record),
                SECTION_HANDLERS => handlers_section = Some(record),
                SECTION_TRIGGERS => triggers_section = Some(record),
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
            || triggers_section.is_some_and(|section| section.len > scratch.len())
            || screens_section.is_some_and(|section| section.len > scratch.len())
        {
            return Err(VmError::InvalidSection);
        }

        reader.read_exact_at(strings_section.offset, &mut scratch[..strings_section.len])?;
        let (string_bytes, string_offsets, string_lens, string_count) =
            parse_owned_strings(&scratch[..strings_section.len])?;
        ptr::copy_nonoverlapping(
            string_bytes.as_ptr(),
            ptr::addr_of_mut!((*out).string_bytes).cast::<u8>(),
            MAX_PROGRAM_STRING_BYTES,
        );
        ptr::copy_nonoverlapping(
            string_offsets.as_ptr(),
            ptr::addr_of_mut!((*out).string_offsets).cast::<u16>(),
            MAX_STRINGS,
        );
        ptr::copy_nonoverlapping(
            string_lens.as_ptr(),
            ptr::addr_of_mut!((*out).string_lens).cast::<u16>(),
            MAX_STRINGS,
        );
        ptr::addr_of_mut!((*out).string_count).write(string_count);

        reader.read_exact_at(state_section.offset, &mut scratch[..state_section.len])?;
        let (state_slots, state_count) = parse_state(&scratch[..state_section.len])?;
        ptr::copy_nonoverlapping(
            state_slots.as_ptr(),
            ptr::addr_of_mut!((*out).state_slots).cast::<StateSlot>(),
            MAX_STATE,
        );
        ptr::addr_of_mut!((*out).state_count).write(state_count);

        reader.read_exact_at(
            functions_section.offset,
            &mut scratch[..functions_section.len],
        )?;
        let (functions, function_count) =
            parse_functions(&scratch[..functions_section.len], code_section.len)?;
        ptr::copy_nonoverlapping(
            functions.as_ptr(),
            ptr::addr_of_mut!((*out).functions).cast::<Function>(),
            MAX_FUNCTIONS,
        );
        ptr::addr_of_mut!((*out).function_count).write(function_count);

        reader.read_exact_at(
            handlers_section.offset,
            &mut scratch[..handlers_section.len],
        )?;
        let (handlers, handler_count) =
            parse_handlers(&scratch[..handlers_section.len], code_section.len)?;
        ptr::copy_nonoverlapping(
            handlers.as_ptr(),
            ptr::addr_of_mut!((*out).handlers).cast::<Handler>(),
            MAX_HANDLERS,
        );
        ptr::addr_of_mut!((*out).handler_count).write(handler_count);

        let (trigger_timers, trigger_timer_count) = if let Some(section) = triggers_section {
            reader.read_exact_at(section.offset, &mut scratch[..section.len])?;
            parse_trigger_timers(Some(&scratch[..section.len]))?
        } else {
            parse_trigger_timers(None)?
        };
        ptr::copy_nonoverlapping(
            trigger_timers.as_ptr(),
            ptr::addr_of_mut!((*out).trigger_timers).cast::<TriggerTimerMeta>(),
            MAX_TRIGGERS,
        );
        ptr::addr_of_mut!((*out).trigger_timer_count).write(trigger_timer_count);

        let (screens, screen_count) = if let Some(section) = screens_section {
            reader.read_exact_at(section.offset, &mut scratch[..section.len])?;
            parse_screens(Some(&scratch[..section.len]), code_section.len)?
        } else {
            parse_screens(None, code_section.len)?
        };
        ptr::copy_nonoverlapping(
            screens.as_ptr(),
            ptr::addr_of_mut!((*out).screens).cast::<Screen>(),
            MAX_SCREENS,
        );
        ptr::addr_of_mut!((*out).screen_count).write(screen_count);
        ptr::addr_of_mut!((*out).code_offset).write(code_section.offset);
        ptr::addr_of_mut!((*out).code_len).write(code_section.len);
        validate_program_index_tables(&*out)?;

        Ok(())
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

    pub(crate) fn handler(&self, event: &str) -> Result<(usize, Handler), VmError> {
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

    pub fn trigger_timers(&self) -> Result<TriggerTimers<'_>, VmError> {
        TriggerTimers::new(self, &self.trigger_timers, self.trigger_timer_count)
    }

    pub fn capability_demand_from_reader(
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<CapabilityDemand, VmError> {
        let code_section = code_reader_section(reader, scratch)?;
        if code_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }
        reader.read_exact_at(code_section.offset, &mut scratch[..code_section.len])?;
        let (mut demand, uses_upload) = capability_demand_from_code(&scratch[..code_section.len])?;
        if uses_upload {
            let count = Self::upload_profile_count_from_reader(reader, scratch)?;
            for index in 0..count {
                let profile = Self::upload_profile_from_reader(reader, scratch, index)?;
                for transport_index in 0..profile.transports.len() {
                    match profile.transports.get(transport_index) {
                        Some("ble") => demand.ble = true,
                        Some("http") => demand.http = true,
                        _ => {}
                    }
                }
            }
        }
        Ok(demand)
    }

    pub fn app_id_from_reader<'a>(
        reader: &mut impl SqbcReader,
        scratch: &'a mut [u8],
    ) -> Result<&'a str, VmError> {
        let strings_section = strings_reader_section(reader, scratch)?;
        if strings_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }
        reader.read_exact_at(strings_section.offset, &mut scratch[..strings_section.len])?;
        string_from_section(&scratch[..strings_section.len], 0)
    }

    pub fn trigger_timer_count_from_reader(
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<usize, VmError> {
        let (_, triggers_section) = trigger_reader_sections(reader, scratch)?;
        let Some(triggers_section) = triggers_section else {
            return Ok(0);
        };
        if triggers_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }
        reader.read_exact_at(
            triggers_section.offset,
            &mut scratch[..triggers_section.len],
        )?;
        let count = read_u16(scratch, 0)? as usize;
        if count > MAX_TRIGGERS {
            return Err(VmError::InvalidSection);
        }
        let expected_len = 2usize
            .checked_add(count.checked_mul(8).ok_or(VmError::InvalidSection)?)
            .ok_or(VmError::InvalidSection)?;
        if expected_len != triggers_section.len {
            return Err(VmError::InvalidSection);
        }
        Ok(count)
    }

    pub fn trigger_timer_from_reader<'a>(
        reader: &mut impl SqbcReader,
        scratch: &'a mut [u8],
        timer_index: usize,
    ) -> Result<TriggerTimer<'a>, VmError> {
        let (strings_section, triggers_section) = trigger_reader_sections(reader, scratch)?;
        let triggers_section = triggers_section.ok_or(VmError::InvalidOperand)?;
        if triggers_section.len > scratch.len() || strings_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }

        reader.read_exact_at(
            triggers_section.offset,
            &mut scratch[..triggers_section.len],
        )?;
        let count = read_u16(scratch, 0)? as usize;
        if count > MAX_TRIGGERS || timer_index >= count {
            return Err(VmError::InvalidOperand);
        }
        let expected_len = 2usize
            .checked_add(count.checked_mul(8).ok_or(VmError::InvalidSection)?)
            .ok_or(VmError::InvalidSection)?;
        if expected_len != triggers_section.len {
            return Err(VmError::InvalidSection);
        }
        let cursor = 2 + timer_index * 8;
        let event_id = read_u16(scratch, cursor)?;
        let repeating = *scratch.get(cursor + 2).ok_or(VmError::InvalidSection)? != 0;
        let reserved = *scratch.get(cursor + 3).ok_or(VmError::InvalidSection)?;
        let interval_ms = read_i32(scratch, cursor + 4)?;
        if reserved != 0 || interval_ms <= 0 {
            return Err(VmError::InvalidSection);
        }

        reader.read_exact_at(strings_section.offset, &mut scratch[..strings_section.len])?;
        let event = string_from_section(&scratch[..strings_section.len], event_id)?;
        Ok(TriggerTimer {
            event,
            interval_ms,
            repeating,
        })
    }

    pub fn device_binding_count_from_reader(
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<usize, VmError> {
        let (_, bindings_section) = device_binding_reader_sections(reader, scratch)?;
        let Some(bindings_section) = bindings_section else {
            return Ok(0);
        };
        if bindings_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }
        reader.read_exact_at(
            bindings_section.offset,
            &mut scratch[..bindings_section.len],
        )?;
        let count = read_u16(scratch, 0)? as usize;
        if count > MAX_DEVICE_BINDINGS {
            return Err(VmError::InvalidSection);
        }
        let expected_len = 2usize
            .checked_add(count.checked_mul(6).ok_or(VmError::InvalidSection)?)
            .ok_or(VmError::InvalidSection)?;
        if expected_len != bindings_section.len {
            return Err(VmError::InvalidSection);
        }
        Ok(count)
    }

    pub fn device_binding_from_reader<'a>(
        reader: &mut impl SqbcReader,
        scratch: &'a mut [u8],
        binding_index: usize,
    ) -> Result<DeviceBinding<'a>, VmError> {
        let (strings_section, bindings_section) = device_binding_reader_sections(reader, scratch)?;
        let bindings_section = bindings_section.ok_or(VmError::InvalidOperand)?;
        if bindings_section.len > scratch.len() || strings_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }

        reader.read_exact_at(
            bindings_section.offset,
            &mut scratch[..bindings_section.len],
        )?;
        let count = read_u16(scratch, 0)? as usize;
        if count > MAX_DEVICE_BINDINGS || binding_index >= count {
            return Err(VmError::InvalidOperand);
        }
        let expected_len = 2usize
            .checked_add(count.checked_mul(6).ok_or(VmError::InvalidSection)?)
            .ok_or(VmError::InvalidSection)?;
        if expected_len != bindings_section.len {
            return Err(VmError::InvalidSection);
        }
        let cursor = 2 + binding_index * 6;
        let service_id = read_u16(scratch, cursor)?;
        let binding_id = read_u16(scratch, cursor + 2)?;
        let resource_id = read_u16(scratch, cursor + 4)?;

        reader.read_exact_at(strings_section.offset, &mut scratch[..strings_section.len])?;
        Ok(DeviceBinding {
            service: string_from_section(&scratch[..strings_section.len], service_id)?,
            binding: string_from_section(&scratch[..strings_section.len], binding_id)?,
            resource: string_from_section(&scratch[..strings_section.len], resource_id)?,
        })
    }

    pub fn upload_profile_count_from_reader(
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<usize, VmError> {
        let (_, profile_section) = upload_profile_reader_sections(reader, scratch)?;
        let Some(profile_section) = profile_section else {
            return Ok(0);
        };
        if profile_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }
        reader.read_exact_at(profile_section.offset, &mut scratch[..profile_section.len])?;
        let count = read_u16(scratch, 0)? as usize;
        if count > MAX_UPLOAD_PROFILES {
            return Err(VmError::InvalidSection);
        }
        validate_upload_profile_section(&scratch[..profile_section.len])?;
        Ok(count)
    }

    pub fn upload_profile_from_reader<'a>(
        reader: &mut impl SqbcReader,
        scratch: &'a mut [u8],
        profile_index: usize,
    ) -> Result<UploadProfile<'a>, VmError> {
        let (strings_section, profile_section) = upload_profile_reader_sections(reader, scratch)?;
        let profile_section = profile_section.ok_or(VmError::InvalidOperand)?;
        if profile_section.len > scratch.len() || strings_section.len > scratch.len() {
            return Err(VmError::InvalidSection);
        }

        reader.read_exact_at(profile_section.offset, &mut scratch[..profile_section.len])?;
        let count = read_u16(scratch, 0)? as usize;
        if count > MAX_UPLOAD_PROFILES || profile_index >= count {
            return Err(VmError::InvalidOperand);
        }

        let mut cursor = 2usize;
        let mut selected = None;
        for index in 0..count {
            let id_id = read_u16(scratch, cursor)?;
            let role_id = read_u16(scratch, cursor + 2)?;
            let accept_count = read_u16(scratch, cursor + 4)? as usize;
            cursor = cursor.checked_add(6).ok_or(VmError::InvalidSection)?;
            if accept_count > MAX_UPLOAD_PROFILE_TEXT_ITEMS {
                return Err(VmError::InvalidSection);
            }
            let accept_start = cursor;
            cursor = cursor
                .checked_add(accept_count.checked_mul(2).ok_or(VmError::InvalidSection)?)
                .ok_or(VmError::InvalidSection)?;
            let transport_count = read_u16(scratch, cursor)? as usize;
            cursor = cursor.checked_add(2).ok_or(VmError::InvalidSection)?;
            if transport_count > MAX_UPLOAD_PROFILE_TEXT_ITEMS {
                return Err(VmError::InvalidSection);
            }
            let transports_start = cursor;
            cursor = cursor
                .checked_add(
                    transport_count
                        .checked_mul(2)
                        .ok_or(VmError::InvalidSection)?,
                )
                .ok_or(VmError::InvalidSection)?;
            let event_count = read_u16(scratch, cursor)? as usize;
            cursor = cursor.checked_add(2).ok_or(VmError::InvalidSection)?;
            if event_count > MAX_UPLOAD_PROFILE_EVENTS {
                return Err(VmError::InvalidSection);
            }
            let events_start = cursor;
            cursor = cursor
                .checked_add(event_count.checked_mul(4).ok_or(VmError::InvalidSection)?)
                .ok_or(VmError::InvalidSection)?;
            if cursor > profile_section.len {
                return Err(VmError::InvalidSection);
            }
            if index == profile_index {
                selected = Some((
                    id_id,
                    role_id,
                    accept_count,
                    accept_start,
                    transport_count,
                    transports_start,
                    event_count,
                    events_start,
                ));
            }
        }
        if cursor != profile_section.len {
            return Err(VmError::InvalidSection);
        }

        let Some((
            id_id,
            role_id,
            accept_count,
            accept_start,
            transport_count,
            transports_start,
            event_count,
            events_start,
        )) = selected
        else {
            return Err(VmError::InvalidOperand);
        };

        let mut accept_ids = [0u16; MAX_UPLOAD_PROFILE_TEXT_ITEMS];
        for (index, slot) in accept_ids.iter_mut().enumerate().take(accept_count) {
            *slot = read_u16(scratch, accept_start + index * 2)?;
        }
        let mut transport_ids = [0u16; MAX_UPLOAD_PROFILE_TEXT_ITEMS];
        for (index, slot) in transport_ids.iter_mut().enumerate().take(transport_count) {
            *slot = read_u16(scratch, transports_start + index * 2)?;
        }
        let mut event_ids = [(0u16, 0u16); MAX_UPLOAD_PROFILE_EVENTS];
        for (index, slot) in event_ids.iter_mut().enumerate().take(event_count) {
            let base = events_start + index * 4;
            *slot = (read_u16(scratch, base)?, read_u16(scratch, base + 2)?);
        }

        reader.read_exact_at(strings_section.offset, &mut scratch[..strings_section.len])?;
        let strings = &scratch[..strings_section.len];
        let mut accept = [""; MAX_UPLOAD_PROFILE_TEXT_ITEMS];
        for (index, slot) in accept.iter_mut().enumerate().take(accept_count) {
            *slot = string_from_section(strings, accept_ids[index])?;
        }
        let mut transports = [""; MAX_UPLOAD_PROFILE_TEXT_ITEMS];
        for (index, slot) in transports.iter_mut().enumerate().take(transport_count) {
            *slot = string_from_section(strings, transport_ids[index])?;
        }
        let mut events = [UploadProfileEventRoute {
            kind: "",
            event: "",
        }; MAX_UPLOAD_PROFILE_EVENTS];
        for (index, slot) in events.iter_mut().enumerate().take(event_count) {
            let (kind_id, event_id) = event_ids[index];
            *slot = UploadProfileEventRoute {
                kind: string_from_section(strings, kind_id)?,
                event: string_from_section(strings, event_id)?,
            };
        }

        Ok(UploadProfile {
            id: string_from_section(strings, id_id)?,
            role: string_from_section(strings, role_id)?,
            accept: UploadProfileTextList {
                values: accept,
                count: accept_count,
            },
            transports: UploadProfileTextList {
                values: transports,
                count: transport_count,
            },
            events: UploadProfileEventRoutes {
                values: events,
                count: event_count,
            },
        })
    }

    pub(crate) fn screen(&self, name: &str) -> Result<(usize, Screen), VmError> {
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

    fn find_string(&self, value: &str) -> Result<Option<u16>, VmError> {
        for index in 0..self.string_count {
            if self.string(index as u16)? == value {
                return Ok(Some(index as u16));
            }
        }
        Ok(None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TriggerTimer<'a> {
    pub event: &'a str,
    pub interval_ms: i32,
    pub repeating: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceBinding<'a> {
    pub service: &'a str,
    pub binding: &'a str,
    pub resource: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadProfileEventRoute<'a> {
    pub kind: &'a str,
    pub event: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadProfile<'a> {
    pub id: &'a str,
    pub role: &'a str,
    pub accept: UploadProfileTextList<'a>,
    pub transports: UploadProfileTextList<'a>,
    pub events: UploadProfileEventRoutes<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadProfileTextList<'a> {
    values: [&'a str; MAX_UPLOAD_PROFILE_TEXT_ITEMS],
    count: usize,
}

impl<'a> UploadProfileTextList<'a> {
    pub const fn len(&self) -> usize {
        self.count
    }

    pub fn get(&self, index: usize) -> Option<&'a str> {
        if index < self.count {
            Some(self.values[index])
        } else {
            None
        }
    }
}

impl<'a, const N: usize> PartialEq<[&'a str; N]> for UploadProfileTextList<'a> {
    fn eq(&self, other: &[&'a str; N]) -> bool {
        self.count == N
            && self
                .values
                .iter()
                .take(self.count)
                .zip(other.iter())
                .all(|(left, right)| left == right)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadProfileEventRoutes<'a> {
    values: [UploadProfileEventRoute<'a>; MAX_UPLOAD_PROFILE_EVENTS],
    count: usize,
}

impl<'a> UploadProfileEventRoutes<'a> {
    pub const fn len(&self) -> usize {
        self.count
    }

    pub fn get(&self, index: usize) -> Option<UploadProfileEventRoute<'a>> {
        if index < self.count {
            Some(self.values[index])
        } else {
            None
        }
    }
}

impl<'a, const N: usize> PartialEq<[UploadProfileEventRoute<'a>; N]>
    for UploadProfileEventRoutes<'a>
{
    fn eq(&self, other: &[UploadProfileEventRoute<'a>; N]) -> bool {
        self.count == N
            && self
                .values
                .iter()
                .take(self.count)
                .zip(other.iter())
                .all(|(left, right)| left == right)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TriggerTimers<'a> {
    timers: [TriggerTimer<'a>; MAX_TRIGGERS],
    count: usize,
}

impl<'a> TriggerTimers<'a> {
    fn new(
        strings: &'a impl StringTable,
        metas: &[TriggerTimerMeta; MAX_TRIGGERS],
        count: usize,
    ) -> Result<Self, VmError> {
        let mut timers = [TriggerTimer {
            event: "",
            interval_ms: 0,
            repeating: false,
        }; MAX_TRIGGERS];
        for (index, meta) in metas.iter().take(count).enumerate() {
            timers[index] = TriggerTimer {
                event: strings.string(meta.event_id)?,
                interval_ms: meta.interval_ms,
                repeating: meta.repeating,
            };
        }
        Ok(Self { timers, count })
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub fn get(&self, index: usize) -> Option<TriggerTimer<'a>> {
        if index < self.count {
            Some(self.timers[index])
        } else {
            None
        }
    }
}

impl<'a> core::ops::Index<usize> for TriggerTimers<'a> {
    type Output = TriggerTimer<'a>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.timers[index]
    }
}

impl<'a, const N: usize> PartialEq<[TriggerTimer<'a>; N]> for TriggerTimers<'a> {
    fn eq(&self, other: &[TriggerTimer<'a>; N]) -> bool {
        self.count == N && self.timers[..self.count] == other[..]
    }
}

fn validate_unique_section_kinds(header_bytes: &[u8], section_count: usize) -> Result<(), VmError> {
    for index in 0..section_count {
        let kind = read_u16(header_bytes, SQBC_HEADER_LEN + index * 12)?;
        for previous in 0..index {
            let previous_kind = read_u16(header_bytes, SQBC_HEADER_LEN + previous * 12)?;
            if previous_kind == kind {
                return Err(VmError::DuplicateSection);
            }
        }
    }
    Ok(())
}

fn validate_program_tables(
    strings: &[&str; MAX_STRINGS],
    string_count: usize,
    state_slots: &[StateSlot; MAX_STATE],
    state_count: usize,
    functions: &[Function; MAX_FUNCTIONS],
    function_count: usize,
    handlers: &[Handler; MAX_HANDLERS],
    handler_count: usize,
    trigger_timers: &[TriggerTimerMeta; MAX_TRIGGERS],
    trigger_timer_count: usize,
    screens: &[Screen; MAX_SCREENS],
    screen_count: usize,
) -> Result<(), VmError> {
    for slot in state_slots.iter().take(state_count) {
        validate_string_id(slot.name_id, string_count)?;
        validate_value_string_refs(&slot.default, string_count)?;
    }
    for function in functions.iter().take(function_count) {
        validate_string_id(function._name_id, string_count)?;
    }
    for handler in handlers.iter().take(handler_count) {
        validate_string_id(handler.event_id, string_count)?;
    }
    for trigger in trigger_timers.iter().take(trigger_timer_count) {
        validate_string_id(trigger.event_id, string_count)?;
    }
    for screen in screens.iter().take(screen_count) {
        validate_string_id(screen.name_id, string_count)?;
    }

    validate_unique_state_names(strings, string_count, state_slots, state_count)?;
    validate_unique_function_names(strings, string_count, functions, function_count)?;
    validate_unique_handler_events(strings, string_count, handlers, handler_count)?;
    validate_unique_trigger_events(strings, string_count, trigger_timers, trigger_timer_count)?;
    validate_unique_screen_names(strings, string_count, screens, screen_count)?;
    Ok(())
}

fn validate_program_index_tables(index: &ProgramIndex) -> Result<(), VmError> {
    for slot in index.state_slots.iter().take(index.state_count) {
        validate_index_string_id(index, slot.name_id)?;
        validate_value_string_refs(&slot.default, index.string_count)?;
    }
    for function in index.functions.iter().take(index.function_count) {
        validate_index_string_id(index, function._name_id)?;
    }
    for handler in index.handlers.iter().take(index.handler_count) {
        validate_index_string_id(index, handler.event_id)?;
    }
    for trigger in index.trigger_timers.iter().take(index.trigger_timer_count) {
        validate_index_string_id(index, trigger.event_id)?;
    }
    for screen in index.screens.iter().take(index.screen_count) {
        validate_index_string_id(index, screen.name_id)?;
    }

    validate_unique_index_state_names(index)?;
    validate_unique_index_function_names(index)?;
    validate_unique_index_handler_events(index)?;
    validate_unique_index_trigger_events(index)?;
    validate_unique_index_screen_names(index)?;
    Ok(())
}

fn validate_string_id(id: u16, string_count: usize) -> Result<(), VmError> {
    if id as usize >= string_count {
        return Err(VmError::InvalidStringRef);
    }
    Ok(())
}

fn validate_index_string_id(index: &ProgramIndex, id: u16) -> Result<(), VmError> {
    validate_string_id(id, index.string_count)?;
    index.string(id)?;
    Ok(())
}

fn validate_value_string_refs(value: &Value, string_count: usize) -> Result<(), VmError> {
    if let Value::String(StringRef::Sqbc(id)) = value {
        validate_string_id(*id, string_count)?;
    }
    Ok(())
}

fn table_string<'a>(
    strings: &[&'a str; MAX_STRINGS],
    string_count: usize,
    id: u16,
) -> Result<&'a str, VmError> {
    validate_string_id(id, string_count)?;
    Ok(strings[id as usize])
}

fn validate_unique_state_names(
    strings: &[&str; MAX_STRINGS],
    string_count: usize,
    slots: &[StateSlot; MAX_STATE],
    count: usize,
) -> Result<(), VmError> {
    for index in 0..count {
        let name = table_string(strings, string_count, slots[index].name_id)?;
        for previous in 0..index {
            if table_string(strings, string_count, slots[previous].name_id)? == name {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_function_names(
    strings: &[&str; MAX_STRINGS],
    string_count: usize,
    functions: &[Function; MAX_FUNCTIONS],
    count: usize,
) -> Result<(), VmError> {
    for index in 0..count {
        let name = table_string(strings, string_count, functions[index]._name_id)?;
        for previous in 0..index {
            if table_string(strings, string_count, functions[previous]._name_id)? == name {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_handler_events(
    strings: &[&str; MAX_STRINGS],
    string_count: usize,
    handlers: &[Handler; MAX_HANDLERS],
    count: usize,
) -> Result<(), VmError> {
    for index in 0..count {
        let event = table_string(strings, string_count, handlers[index].event_id)?;
        for previous in 0..index {
            if table_string(strings, string_count, handlers[previous].event_id)? == event {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_trigger_events(
    strings: &[&str; MAX_STRINGS],
    string_count: usize,
    triggers: &[TriggerTimerMeta; MAX_TRIGGERS],
    count: usize,
) -> Result<(), VmError> {
    for index in 0..count {
        let event = table_string(strings, string_count, triggers[index].event_id)?;
        for previous in 0..index {
            if table_string(strings, string_count, triggers[previous].event_id)? == event {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_screen_names(
    strings: &[&str; MAX_STRINGS],
    string_count: usize,
    screens: &[Screen; MAX_SCREENS],
    count: usize,
) -> Result<(), VmError> {
    for index in 0..count {
        let name = table_string(strings, string_count, screens[index].name_id)?;
        for previous in 0..index {
            if table_string(strings, string_count, screens[previous].name_id)? == name {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_index_state_names(index: &ProgramIndex) -> Result<(), VmError> {
    for slot_index in 0..index.state_count {
        let name = index.string(index.state_slots[slot_index].name_id)?;
        for previous in 0..slot_index {
            if index.string(index.state_slots[previous].name_id)? == name {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_index_function_names(index: &ProgramIndex) -> Result<(), VmError> {
    for function_index in 0..index.function_count {
        let name = index.string(index.functions[function_index]._name_id)?;
        for previous in 0..function_index {
            if index.string(index.functions[previous]._name_id)? == name {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_index_handler_events(index: &ProgramIndex) -> Result<(), VmError> {
    for handler_index in 0..index.handler_count {
        let event = index.string(index.handlers[handler_index].event_id)?;
        for previous in 0..handler_index {
            if index.string(index.handlers[previous].event_id)? == event {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_index_trigger_events(index: &ProgramIndex) -> Result<(), VmError> {
    for trigger_index in 0..index.trigger_timer_count {
        let event = index.string(index.trigger_timers[trigger_index].event_id)?;
        for previous in 0..trigger_index {
            if index.string(index.trigger_timers[previous].event_id)? == event {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn validate_unique_index_screen_names(index: &ProgramIndex) -> Result<(), VmError> {
    for screen_index in 0..index.screen_count {
        let name = index.string(index.screens[screen_index].name_id)?;
        for previous in 0..screen_index {
            if index.string(index.screens[previous].name_id)? == name {
                return Err(VmError::DuplicateTableKey);
            }
        }
    }
    Ok(())
}

fn section<'a>(bytes: &'a [u8], section_count: usize, kind: u16) -> Result<&'a [u8], VmError> {
    for index in 0..section_count {
        let base = SQBC_HEADER_LEN + index * 12;
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

fn code_reader_section(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
) -> Result<SqbcSection, VmError> {
    let mut fixed_header = [0u8; SQBC_HEADER_LEN];
    reader.read_exact_at(0, &mut fixed_header)?;
    let header = Program::parse_header(&fixed_header)?;
    if header.header_len > scratch.len() {
        return Err(VmError::InvalidHeader);
    }
    reader.read_exact_at(0, &mut scratch[..header.header_len])?;
    validate_unique_section_kinds(&scratch[..header.header_len], header.section_count)?;

    for index in 0..header.section_count {
        let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
        if record.kind == SECTION_CODE {
            return Ok(record);
        }
    }
    Err(VmError::MissingSection)
}

fn strings_reader_section(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
) -> Result<SqbcSection, VmError> {
    let mut fixed_header = [0u8; SQBC_HEADER_LEN];
    reader.read_exact_at(0, &mut fixed_header)?;
    let header = Program::parse_header(&fixed_header)?;
    if header.header_len > scratch.len() {
        return Err(VmError::InvalidHeader);
    }
    reader.read_exact_at(0, &mut scratch[..header.header_len])?;
    validate_unique_section_kinds(&scratch[..header.header_len], header.section_count)?;

    for index in 0..header.section_count {
        let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
        if record.kind == SECTION_STRINGS {
            return Ok(record);
        }
    }
    Err(VmError::MissingSection)
}

fn capability_demand_from_code(code: &[u8]) -> Result<(CapabilityDemand, bool), VmError> {
    let mut demand = CapabilityDemand::none();
    let mut uses_upload = false;
    let mut cursor = 0usize;
    while cursor < code.len() {
        let op = *code.get(cursor).ok_or(VmError::InvalidSection)?;
        cursor += 1;
        match op {
            OP_PUSH_INT => skip_code_bytes(code, &mut cursor, 4)?,
            OP_PUSH_BOOL => skip_code_bytes(code, &mut cursor, 1)?,
            OP_PUSH_STRING | OP_GET_STATE | OP_SET_STATE | OP_GET_LOCAL | OP_SET_LOCAL
            | OP_GET_FIELD => skip_code_bytes(code, &mut cursor, 2)?,
            OP_PUSH_NULL | OP_ADD | OP_SUB | OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE
            | OP_RETURN | OP_HALT | OP_POP | OP_LIST_LEN | OP_LIST_GET => {}
            OP_JUMP | OP_JUMP_IF_FALSE => skip_code_bytes(code, &mut cursor, 4)?,
            OP_CALL_FUNCTION => skip_code_bytes(code, &mut cursor, 4)?,
            OP_CALL_BUILTIN => {
                let builtin = *code.get(cursor).ok_or(VmError::InvalidSection)?;
                cursor += 1;
                demand.apply_builtin(builtin);
                uses_upload |= matches!(
                    builtin,
                    BUILTIN_SERVICE_UPLOAD_START
                        | BUILTIN_SERVICE_UPLOAD_STOP
                        | BUILTIN_SERVICE_UPLOAD_STATUS
                );
                if builtin == BUILTIN_DEBUG_PRINT {
                    skip_code_bytes(code, &mut cursor, 1)?;
                }
            }
            _ => return Err(VmError::UnknownOpcode),
        }
    }
    Ok((demand, uses_upload))
}

fn upload_transport_demand(
    bytes: Option<&[u8]>,
    strings: &[&str; MAX_STRINGS],
    string_count: usize,
) -> Result<(bool, bool), VmError> {
    let Some(bytes) = bytes else {
        return Ok((false, false));
    };
    validate_upload_profile_section(bytes)?;
    let count = read_u16(bytes, 0)? as usize;
    let mut cursor = 2usize;
    let mut ble = false;
    let mut http = false;
    for _ in 0..count {
        let accept_count = read_u16(bytes, cursor + 4)? as usize;
        cursor = cursor
            .checked_add(6 + accept_count * 2)
            .ok_or(VmError::InvalidSection)?;
        let transport_count = read_u16(bytes, cursor)? as usize;
        cursor = cursor.checked_add(2).ok_or(VmError::InvalidSection)?;
        for _ in 0..transport_count {
            let id = read_u16(bytes, cursor)? as usize;
            if id >= string_count {
                return Err(VmError::InvalidSection);
            }
            match strings[id] {
                "ble" => ble = true,
                "http" => http = true,
                _ => {}
            }
            cursor = cursor.checked_add(2).ok_or(VmError::InvalidSection)?;
        }
        let event_count = read_u16(bytes, cursor)? as usize;
        cursor = cursor
            .checked_add(2 + event_count * 4)
            .ok_or(VmError::InvalidSection)?;
    }
    Ok((ble, http))
}

fn skip_code_bytes(code: &[u8], cursor: &mut usize, len: usize) -> Result<(), VmError> {
    let end = cursor.checked_add(len).ok_or(VmError::InvalidSection)?;
    if end > code.len() {
        return Err(VmError::InvalidSection);
    }
    *cursor = end;
    Ok(())
}

fn trigger_reader_sections(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
) -> Result<(SqbcSection, Option<SqbcSection>), VmError> {
    let mut fixed_header = [0u8; SQBC_HEADER_LEN];
    reader.read_exact_at(0, &mut fixed_header)?;
    let header = Program::parse_header(&fixed_header)?;
    if header.header_len > scratch.len() {
        return Err(VmError::InvalidHeader);
    }
    reader.read_exact_at(0, &mut scratch[..header.header_len])?;
    validate_unique_section_kinds(&scratch[..header.header_len], header.section_count)?;

    let mut strings_section = None;
    let mut triggers_section = None;
    for index in 0..header.section_count {
        let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
        match record.kind {
            SECTION_STRINGS => strings_section = Some(record),
            SECTION_TRIGGERS => triggers_section = Some(record),
            _ => {}
        }
    }

    Ok((
        strings_section.ok_or(VmError::MissingSection)?,
        triggers_section,
    ))
}

fn device_binding_reader_sections(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
) -> Result<(SqbcSection, Option<SqbcSection>), VmError> {
    let mut fixed_header = [0u8; SQBC_HEADER_LEN];
    reader.read_exact_at(0, &mut fixed_header)?;
    let header = Program::parse_header(&fixed_header)?;
    if header.header_len > scratch.len() {
        return Err(VmError::InvalidHeader);
    }
    reader.read_exact_at(0, &mut scratch[..header.header_len])?;
    validate_unique_section_kinds(&scratch[..header.header_len], header.section_count)?;

    let mut strings_section = None;
    let mut bindings_section = None;
    for index in 0..header.section_count {
        let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
        match record.kind {
            SECTION_STRINGS => strings_section = Some(record),
            SECTION_DEVICE_BINDINGS => bindings_section = Some(record),
            _ => {}
        }
    }

    Ok((
        strings_section.ok_or(VmError::MissingSection)?,
        bindings_section,
    ))
}

fn upload_profile_reader_sections(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
) -> Result<(SqbcSection, Option<SqbcSection>), VmError> {
    let mut fixed_header = [0u8; SQBC_HEADER_LEN];
    reader.read_exact_at(0, &mut fixed_header)?;
    let header = Program::parse_header(&fixed_header)?;
    if header.header_len > scratch.len() {
        return Err(VmError::InvalidHeader);
    }
    reader.read_exact_at(0, &mut scratch[..header.header_len])?;
    validate_unique_section_kinds(&scratch[..header.header_len], header.section_count)?;

    let mut strings_section = None;
    let mut profile_section = None;
    for index in 0..header.section_count {
        let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
        match record.kind {
            SECTION_STRINGS => strings_section = Some(record),
            SECTION_UPLOAD_PROFILES => profile_section = Some(record),
            _ => {}
        }
    }

    Ok((
        strings_section.ok_or(VmError::MissingSection)?,
        profile_section,
    ))
}

fn validate_upload_profile_section(bytes: &[u8]) -> Result<(), VmError> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_UPLOAD_PROFILES {
        return Err(VmError::InvalidSection);
    }
    let mut cursor = 2usize;
    for _ in 0..count {
        let accept_count = read_u16(bytes, cursor + 4)? as usize;
        cursor = cursor.checked_add(6).ok_or(VmError::InvalidSection)?;
        if accept_count > MAX_UPLOAD_PROFILE_TEXT_ITEMS {
            return Err(VmError::InvalidSection);
        }
        cursor = cursor
            .checked_add(accept_count.checked_mul(2).ok_or(VmError::InvalidSection)?)
            .ok_or(VmError::InvalidSection)?;
        let transport_count = read_u16(bytes, cursor)? as usize;
        cursor = cursor.checked_add(2).ok_or(VmError::InvalidSection)?;
        if transport_count > MAX_UPLOAD_PROFILE_TEXT_ITEMS {
            return Err(VmError::InvalidSection);
        }
        cursor = cursor
            .checked_add(
                transport_count
                    .checked_mul(2)
                    .ok_or(VmError::InvalidSection)?,
            )
            .ok_or(VmError::InvalidSection)?;
        let event_count = read_u16(bytes, cursor)? as usize;
        cursor = cursor.checked_add(2).ok_or(VmError::InvalidSection)?;
        if event_count > MAX_UPLOAD_PROFILE_EVENTS {
            return Err(VmError::InvalidSection);
        }
        cursor = cursor
            .checked_add(event_count.checked_mul(4).ok_or(VmError::InvalidSection)?)
            .ok_or(VmError::InvalidSection)?;
        if cursor > bytes.len() {
            return Err(VmError::InvalidSection);
        }
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok(())
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

fn string_from_section(bytes: &[u8], id: u16) -> Result<&str, VmError> {
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_STRINGS || id as usize >= count {
        return Err(VmError::InvalidOperand);
    }
    let mut cursor = 2usize;
    for index in 0..count {
        let len = read_u16(bytes, cursor)? as usize;
        cursor += 2;
        let end = cursor.checked_add(len).ok_or(VmError::InvalidSection)?;
        let raw = bytes.get(cursor..end).ok_or(VmError::InvalidSection)?;
        if index == id as usize {
            return str::from_utf8(raw).map_err(|_| VmError::InvalidUtf8);
        }
        cursor = end;
    }
    Err(VmError::InvalidOperand)
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
        param_count: 0,
        local_count: 0,
        start: 0,
        len: 0,
    }; MAX_HANDLERS];
    let mut cursor = 2usize;
    for handler in handlers.iter_mut().take(count) {
        let event_id = read_u16(bytes, cursor)?;
        let preload = *bytes.get(cursor + 2).ok_or(VmError::InvalidSection)? != 0;
        let reserved = *bytes.get(cursor + 3).ok_or(VmError::InvalidSection)?;
        let param_count = read_u16(bytes, cursor + 4)?;
        let local_count = read_u16(bytes, cursor + 6)?;
        let start = read_u32(bytes, cursor + 8)? as usize;
        let len = read_u32(bytes, cursor + 12)? as usize;
        cursor += 16;
        if reserved != 0 || param_count > local_count || local_count as usize > MAX_LOCALS {
            return Err(VmError::InvalidSection);
        }
        validate_range(start, len, code_len)?;
        *handler = Handler {
            event_id,
            preload,
            param_count,
            local_count,
            start,
            len,
        };
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((handlers, count))
}

fn parse_trigger_timers(
    bytes: Option<&[u8]>,
) -> Result<([TriggerTimerMeta; MAX_TRIGGERS], usize), VmError> {
    let Some(bytes) = bytes else {
        return Ok((
            [TriggerTimerMeta {
                event_id: 0,
                interval_ms: 0,
                repeating: false,
            }; MAX_TRIGGERS],
            0,
        ));
    };
    let count = read_u16(bytes, 0)? as usize;
    if count > MAX_TRIGGERS {
        return Err(VmError::InvalidSection);
    }
    let mut timers = [TriggerTimerMeta {
        event_id: 0,
        interval_ms: 0,
        repeating: false,
    }; MAX_TRIGGERS];
    let mut cursor = 2usize;
    for timer in timers.iter_mut().take(count) {
        let event_id = read_u16(bytes, cursor)?;
        let repeating = *bytes.get(cursor + 2).ok_or(VmError::InvalidSection)? != 0;
        let reserved = *bytes.get(cursor + 3).ok_or(VmError::InvalidSection)?;
        let interval_ms = read_i32(bytes, cursor + 4)?;
        cursor += 8;
        if reserved != 0 || interval_ms <= 0 {
            return Err(VmError::InvalidSection);
        }
        *timer = TriggerTimerMeta {
            event_id,
            interval_ms,
            repeating,
        };
    }
    if cursor != bytes.len() {
        return Err(VmError::InvalidSection);
    }
    Ok((timers, count))
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
