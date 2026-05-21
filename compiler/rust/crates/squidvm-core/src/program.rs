use core::str;

use crate::{
    bytecode::{
        read_u16, read_u32, SECTION_CODE, SECTION_FUNCTIONS, SECTION_HANDLERS, SECTION_SCREENS,
        SECTION_STATE, SECTION_STRINGS,
    },
    error::VmError,
    limits::{
        MAX_APP_BYTES, MAX_CODE_CHUNK_BYTES, MAX_FUNCTIONS, MAX_HANDLERS, MAX_PROGRAM_STRING_BYTES,
        MAX_SCREENS, MAX_STATE, MAX_STRINGS,
    },
    model::{Function, Handler, Screen, StateSlot},
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
    pub(crate) screens: [Screen; MAX_SCREENS],
    pub(crate) screen_count: usize,
    pub(crate) code_offset: usize,
    pub(crate) code_len: usize,
}

impl ProgramIndex {
    pub fn parse_from_reader(
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<Self, VmError> {
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
