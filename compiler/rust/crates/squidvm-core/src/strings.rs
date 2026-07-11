use core::str;

use crate::{
    error::VmError,
    limits::{MAX_RUNTIME_STRING_BYTES, MAX_SERVICE_STRINGS, MAX_SERVICE_STRING_BYTES},
    value::{StringRef, Value},
};

pub trait StringTable {
    fn string(&self, id: u16) -> Result<&str, VmError>;
    fn find_string(&self, value: &str) -> Result<Option<u16>, VmError>;
}

pub struct StringResolver<'a> {
    strings: &'a dyn StringTable,
    interner: &'a StringInterner,
}

impl<'a> StringResolver<'a> {
    pub fn new(strings: &'a dyn StringTable, interner: &'a StringInterner) -> Self {
        Self { strings, interner }
    }

    pub fn value_str(&self, value: Value) -> Result<&str, VmError> {
        match value {
            Value::String(reference) => self.interner.get(self.strings, reference),
            _ => Err(VmError::InvalidOperand),
        }
    }
}

const STATIC_STRINGS: [&str; 28] = [
    "unsupported",
    "ready",
    "display.default",
    "ssd1306",
    "i2c",
    "mono",
    "MONO1_PACKED",
    "sim",
    "native",
    "esp",
    "ap",
    "sta",
    "started",
    "stopped",
    "idle",
    "unavailable",
    "configuring",
    "starting",
    "stopping",
    "scanning",
    "authenticating",
    "open",
    "wep",
    "wpa",
    "wpa2",
    "wpa3",
    "unknown",
    "wifi busy",
];

#[derive(Clone, Copy)]
struct DynamicStringRef {
    tag: u8,
    retained: u8,
    source: u16,
    offset: u16,
    len: u16,
}

const DYNAMIC_STRING_EMPTY: u8 = 0;
const DYNAMIC_STRING_INLINE: u8 = 1;
const DYNAMIC_STRING_SLICE: u8 = 2;

const STRING_SOURCE_SQBC: u16 = 0;
const STRING_SOURCE_STATIC: u16 = 1;
const STRING_SOURCE_DYNAMIC: u16 = 2;
const STRING_SOURCE_KIND_SHIFT: u16 = 14;
const STRING_SOURCE_ID_MASK: u16 = 0x3fff;

impl DynamicStringRef {
    const fn empty() -> Self {
        Self {
            tag: DYNAMIC_STRING_EMPTY,
            retained: 0,
            source: 0,
            offset: 0,
            len: 0,
        }
    }

    const fn inline(offset: u16, len: u16, retained: bool) -> Self {
        Self {
            tag: DYNAMIC_STRING_INLINE,
            retained: retained as u8,
            source: 0,
            offset,
            len,
        }
    }

    fn slice(source: StringRef, offset: u16, len: u16, retained: bool) -> Result<Self, VmError> {
        Ok(Self {
            tag: DYNAMIC_STRING_SLICE,
            retained: retained as u8,
            source: encode_string_source(source)?,
            offset,
            len,
        })
    }

    fn set_retained(&mut self) {
        self.retained = 1;
    }
}

pub struct StringInterner {
    refs: [DynamicStringRef; MAX_SERVICE_STRINGS],
    next: usize,
    bytes: [u8; MAX_SERVICE_STRING_BYTES],
    bytes_len: usize,
}

impl StringInterner {
    pub(crate) fn intern_runtime(
        &mut self,
        strings: &dyn StringTable,
        value: &str,
    ) -> Result<Value, VmError> {
        self.intern_dynamic(strings, value, false)
    }

    pub(crate) const fn new() -> Self {
        Self {
            refs: [DynamicStringRef::empty(); MAX_SERVICE_STRINGS],
            next: 0,
            bytes: [0; MAX_SERVICE_STRING_BYTES],
            bytes_len: 0,
        }
    }

    pub(crate) fn retain_state_values(
        &mut self,
        strings: &dyn StringTable,
        state: &mut [Value],
    ) -> Result<(), VmError> {
        let old_refs = self.refs;
        let old_bytes = self.bytes;
        let old_bytes_len = self.bytes_len;

        self.refs = [DynamicStringRef::empty(); MAX_SERVICE_STRINGS];
        self.bytes = [0; MAX_SERVICE_STRING_BYTES];
        self.next = 0;
        self.bytes_len = 0;

        for value in state {
            let Value::String(StringRef::Dynamic(id)) = *value else {
                continue;
            };
            let text = dynamic_str_from(strings, id, &old_refs, &old_bytes, old_bytes_len)?;
            *value = self.intern_dynamic(strings, text, true)?;
        }

        Ok(())
    }

    pub(crate) fn intern_event(
        &mut self,
        strings: &dyn StringTable,
        value: &str,
    ) -> Result<Value, VmError> {
        self.intern_dynamic(strings, value, false)
    }

    pub(crate) fn intern_retained(
        &mut self,
        strings: &dyn StringTable,
        value: &str,
    ) -> Result<Value, VmError> {
        self.intern_dynamic(strings, value, true)
    }

    pub(crate) fn retain_value(&mut self, value: Value) -> Result<Value, VmError> {
        let Value::String(StringRef::Dynamic(id)) = value else {
            return Ok(value);
        };
        let slot = self
            .refs
            .get_mut(id as usize)
            .ok_or(VmError::InvalidOperand)?;
        match slot {
            DynamicStringRef {
                tag: DYNAMIC_STRING_EMPTY,
                ..
            } => Err(VmError::InvalidOperand),
            slot => {
                slot.set_retained();
                Ok(value)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn dynamic_bytes_used(&self) -> usize {
        self.bytes_len
    }

    #[cfg(test)]
    pub(crate) fn value_str<'a>(
        &'a self,
        strings: &'a dyn StringTable,
        value: Value,
    ) -> Result<&'a str, VmError> {
        match value {
            Value::String(reference) => self.get(strings, reference),
            _ => Err(VmError::InvalidOperand),
        }
    }

    pub(crate) fn get<'a>(
        &'a self,
        strings: &'a dyn StringTable,
        reference: StringRef,
    ) -> Result<&'a str, VmError> {
        match reference {
            StringRef::Sqbc(id) => strings.string(id),
            StringRef::Static(id) => STATIC_STRINGS
                .get(id as usize)
                .copied()
                .ok_or(VmError::InvalidOperand),
            StringRef::Dynamic(id) => self.dynamic_str(strings, id),
        }
    }

    fn intern_dynamic(
        &mut self,
        strings: &dyn StringTable,
        value: &str,
        retained: bool,
    ) -> Result<Value, VmError> {
        if value.len() > MAX_RUNTIME_STRING_BYTES {
            return Err(VmError::InvalidOperand);
        }
        if let Some(id) = strings.find_string(value)? {
            return Ok(Value::String(StringRef::Sqbc(id)));
        }
        if let Some(id) = static_string_id(value) {
            return Ok(Value::String(StringRef::Static(id as u8)));
        }
        if let Some(id) = self.find_dynamic(strings, value)? {
            if retained {
                self.refs[id].set_retained();
            }
            return Ok(Value::String(StringRef::Dynamic(id as u8)));
        }
        if let Some((source, offset)) = self.find_substring(strings, value)? {
            return self.alloc_slice(source, offset, value.len(), retained);
        }
        self.alloc_dynamic(value, retained)
    }

    fn alloc_dynamic(&mut self, value: &str, retained: bool) -> Result<Value, VmError> {
        if self.next >= MAX_SERVICE_STRINGS {
            return Err(VmError::TooManyStrings);
        }
        let offset = self.bytes_len;
        let next_len = offset
            .checked_add(value.len())
            .ok_or(VmError::TooManyStrings)?;
        if next_len > self.bytes.len() || value.len() > u16::MAX as usize {
            return Err(VmError::TooManyStrings);
        }
        self.bytes[offset..next_len].copy_from_slice(value.as_bytes());
        self.bytes_len = next_len;

        let id = self.next;
        self.next += 1;
        self.refs[id] = DynamicStringRef::inline(offset as u16, value.len() as u16, retained);
        Ok(Value::String(StringRef::Dynamic(id as u8)))
    }

    fn alloc_slice(
        &mut self,
        source: StringRef,
        offset: usize,
        len: usize,
        retained: bool,
    ) -> Result<Value, VmError> {
        if self.next >= MAX_SERVICE_STRINGS {
            return Err(VmError::TooManyStrings);
        }
        if offset > u16::MAX as usize || len > u16::MAX as usize {
            return Err(VmError::InvalidOperand);
        }
        let id = self.next;
        self.next += 1;
        self.refs[id] = DynamicStringRef::slice(source, offset as u16, len as u16, retained)?;
        Ok(Value::String(StringRef::Dynamic(id as u8)))
    }

    fn find_dynamic(
        &self,
        strings: &dyn StringTable,
        value: &str,
    ) -> Result<Option<usize>, VmError> {
        for index in 0..self.next {
            if self.dynamic_str(strings, index as u8)? == value {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    fn find_substring(
        &self,
        strings: &dyn StringTable,
        value: &str,
    ) -> Result<Option<(StringRef, usize)>, VmError> {
        if value.is_empty() {
            return Ok(None);
        }
        for index in 0..self.next {
            let source = StringRef::Dynamic(index as u8);
            let text = self.get(strings, source)?;
            if let Some(offset) = text.find(value) {
                return Ok(Some((source, offset)));
            }
        }
        for (index, text) in STATIC_STRINGS.iter().enumerate() {
            if let Some(offset) = text.find(value) {
                return Ok(Some((StringRef::Static(index as u8), offset)));
            }
        }
        Ok(None)
    }

    fn dynamic_str<'a>(&'a self, strings: &'a dyn StringTable, id: u8) -> Result<&'a str, VmError> {
        dynamic_str_from(strings, id, &self.refs, &self.bytes, self.bytes_len)
    }
}

fn dynamic_str_from<'a>(
    strings: &'a dyn StringTable,
    id: u8,
    refs: &[DynamicStringRef; MAX_SERVICE_STRINGS],
    bytes: &'a [u8; MAX_SERVICE_STRING_BYTES],
    bytes_len: usize,
) -> Result<&'a str, VmError> {
    ref_str_from(strings, StringRef::Dynamic(id), refs, bytes, bytes_len)
}

fn ref_str_from<'a>(
    strings: &'a dyn StringTable,
    reference: StringRef,
    refs: &[DynamicStringRef; MAX_SERVICE_STRINGS],
    bytes: &'a [u8; MAX_SERVICE_STRING_BYTES],
    bytes_len: usize,
) -> Result<&'a str, VmError> {
    match reference {
        StringRef::Sqbc(id) => strings.string(id),
        StringRef::Static(id) => STATIC_STRINGS
            .get(id as usize)
            .copied()
            .ok_or(VmError::InvalidOperand),
        StringRef::Dynamic(id) => {
            let reference = *refs.get(id as usize).ok_or(VmError::InvalidOperand)?;
            match reference.tag {
                DYNAMIC_STRING_EMPTY => Err(VmError::InvalidOperand),
                DYNAMIC_STRING_INLINE => {
                    let start = reference.offset as usize;
                    let end = start
                        .checked_add(reference.len as usize)
                        .ok_or(VmError::InvalidOperand)?;
                    if end > bytes_len {
                        return Err(VmError::InvalidOperand);
                    }
                    str::from_utf8(&bytes[start..end]).map_err(|_| VmError::InvalidUtf8)
                }
                DYNAMIC_STRING_SLICE => {
                    let text = ref_str_from(
                        strings,
                        decode_string_source(reference.source)?,
                        refs,
                        bytes,
                        bytes_len,
                    )?;
                    let start = reference.offset as usize;
                    let end = start
                        .checked_add(reference.len as usize)
                        .ok_or(VmError::InvalidOperand)?;
                    text.get(start..end).ok_or(VmError::InvalidUtf8)
                }
                _ => Err(VmError::InvalidOperand),
            }
        }
    }
}

fn encode_string_source(reference: StringRef) -> Result<u16, VmError> {
    let (kind, id) = match reference {
        StringRef::Sqbc(id) => (STRING_SOURCE_SQBC, id),
        StringRef::Static(id) => (STRING_SOURCE_STATIC, id as u16),
        StringRef::Dynamic(id) => (STRING_SOURCE_DYNAMIC, id as u16),
    };
    if id > STRING_SOURCE_ID_MASK {
        return Err(VmError::InvalidOperand);
    }
    Ok((kind << STRING_SOURCE_KIND_SHIFT) | id)
}

fn decode_string_source(value: u16) -> Result<StringRef, VmError> {
    let kind = value >> STRING_SOURCE_KIND_SHIFT;
    let id = value & STRING_SOURCE_ID_MASK;
    match kind {
        STRING_SOURCE_SQBC => Ok(StringRef::Sqbc(id)),
        STRING_SOURCE_STATIC => u8::try_from(id)
            .map(StringRef::Static)
            .map_err(|_| VmError::InvalidOperand),
        STRING_SOURCE_DYNAMIC => u8::try_from(id)
            .map(StringRef::Dynamic)
            .map_err(|_| VmError::InvalidOperand),
        _ => Err(VmError::InvalidOperand),
    }
}

fn static_string_id(value: &str) -> Option<usize> {
    STATIC_STRINGS
        .iter()
        .position(|candidate| *candidate == value)
}
