use core::{mem::MaybeUninit, ptr, str};

use crate::{
    bytecode::{
        read_i32, read_u16, read_u32, SECTION_CODE, SECTION_DEVICE_BINDINGS, SECTION_FUNCTIONS,
        SECTION_HANDLERS, SECTION_SCREENS, SECTION_STATE, SECTION_STRINGS, SECTION_TRIGGERS,
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
};

const SQBC_HEADER_LEN: usize = 14;

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

        let strings = section(bytes, section_count, SECTION_STRINGS)?;
        let state = section(bytes, section_count, SECTION_STATE)?;
        let functions = section(bytes, section_count, SECTION_FUNCTIONS)?;
        let handlers = section(bytes, section_count, SECTION_HANDLERS)?;
        let triggers = optional_section(bytes, section_count, SECTION_TRIGGERS)?;
        let screens = optional_section(bytes, section_count, SECTION_SCREENS)?;
        let code = section(bytes, section_count, SECTION_CODE)?;

        parse_owned_strings(strings)?;
        let (strings, string_count) = parse_strings(strings)?;
        let (state_slots, state_count) = parse_state(state)?;
        let (functions, function_count) = parse_functions(functions, code.len())?;
        let (handlers, handler_count) = parse_handlers(handlers, code.len())?;
        let (trigger_timers, trigger_timer_count) = parse_trigger_timers(triggers)?;
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
            trigger_timers,
            trigger_timer_count,
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
