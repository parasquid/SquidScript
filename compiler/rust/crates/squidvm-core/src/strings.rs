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
    "zephyr",
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
enum DynamicStringRef {
    Empty,
    Inline {
        offset: u16,
        len: u16,
        retained: bool,
    },
}

pub struct StringInterner {
    refs: [DynamicStringRef; MAX_SERVICE_STRINGS],
    next: usize,
    bytes: [u8; MAX_SERVICE_STRING_BYTES],
    bytes_len: usize,
}

impl StringInterner {
    pub(crate) const fn new() -> Self {
        Self {
            refs: [DynamicStringRef::Empty; MAX_SERVICE_STRINGS],
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

        self.refs = [DynamicStringRef::Empty; MAX_SERVICE_STRINGS];
        self.bytes = [0; MAX_SERVICE_STRING_BYTES];
        self.next = 0;
        self.bytes_len = 0;

        for value in state {
            let Value::String(StringRef::Dynamic(id)) = *value else {
                continue;
            };
            let text = dynamic_str_from(id, &old_refs, &old_bytes, old_bytes_len)?;
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
            DynamicStringRef::Empty => Err(VmError::InvalidOperand),
            DynamicStringRef::Inline { retained, .. } => {
                *retained = true;
                Ok(value)
            }
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
            StringRef::Dynamic(id) => self.dynamic_str(id),
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
        if let Some(id) = self.find_dynamic(value)? {
            if retained {
                if let DynamicStringRef::Inline { retained, .. } = &mut self.refs[id] {
                    *retained = true;
                }
            }
            return Ok(Value::String(StringRef::Dynamic(id as u8)));
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
        self.refs[id] = DynamicStringRef::Inline {
            offset: offset as u16,
            len: value.len() as u16,
            retained,
        };
        Ok(Value::String(StringRef::Dynamic(id as u8)))
    }

    fn find_dynamic(&self, value: &str) -> Result<Option<usize>, VmError> {
        for index in 0..self.next {
            let DynamicStringRef::Inline { offset, len, .. } = self.refs[index] else {
                continue;
            };
            let start = offset as usize;
            let end = start
                .checked_add(len as usize)
                .ok_or(VmError::InvalidOperand)?;
            if end > self.bytes_len {
                return Err(VmError::InvalidOperand);
            }
            if &self.bytes[start..end] == value.as_bytes() {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    fn dynamic_str(&self, id: u8) -> Result<&str, VmError> {
        dynamic_str_from(id, &self.refs, &self.bytes, self.bytes_len)
    }
}

fn dynamic_str_from<'a>(
    id: u8,
    refs: &[DynamicStringRef; MAX_SERVICE_STRINGS],
    bytes: &'a [u8; MAX_SERVICE_STRING_BYTES],
    bytes_len: usize,
) -> Result<&'a str, VmError> {
    match *refs.get(id as usize).ok_or(VmError::InvalidOperand)? {
        DynamicStringRef::Empty => Err(VmError::InvalidOperand),
        DynamicStringRef::Inline { offset, len, .. } => {
            let start = offset as usize;
            let end = start
                .checked_add(len as usize)
                .ok_or(VmError::InvalidOperand)?;
            if end > bytes_len {
                return Err(VmError::InvalidOperand);
            }
            str::from_utf8(&bytes[start..end]).map_err(|_| VmError::InvalidUtf8)
        }
    }
}

fn static_string_id(value: &str) -> Option<usize> {
    STATIC_STRINGS
        .iter()
        .position(|candidate| *candidate == value)
}
