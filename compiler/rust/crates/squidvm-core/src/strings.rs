use core::{fmt, str};

use crate::{
    error::VmError,
    limits::{MAX_RUNTIME_STRINGS, MAX_RUNTIME_STRING_BYTES},
    value::Value,
};

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
    pub(crate) const fn new() -> Self {
        Self {
            bytes: [[0; MAX_RUNTIME_STRING_BYTES]; MAX_RUNTIME_STRINGS],
            lens: [0; MAX_RUNTIME_STRINGS],
            next: 0,
        }
    }

    pub(crate) fn alloc(&mut self) -> Result<RuntimeStringWriter<'_>, VmError> {
        if self.next >= MAX_RUNTIME_STRINGS {
            return Err(VmError::TooManyStrings);
        }
        let id = self.next;
        self.next += 1;
        self.lens[id] = 0;
        Ok(RuntimeStringWriter { strings: self, id })
    }

    pub(crate) fn get(&self, id: u8) -> Result<&str, VmError> {
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
    pub(crate) fn value(&self) -> Value {
        Value::RuntimeString(self.id as u8)
    }
}

impl fmt::Write for RuntimeStringWriter<'_> {
    fn write_str(&mut self, input: &str) -> fmt::Result {
        let len = self.strings.lens[self.id];
        let bytes = input.as_bytes();
        let new_len = len.checked_add(bytes.len()).ok_or(fmt::Error)?;
        if new_len > MAX_RUNTIME_STRING_BYTES {
            return Err(fmt::Error);
        }
        self.strings.bytes[self.id][len..new_len].copy_from_slice(bytes);
        self.strings.lens[self.id] = new_len;
        Ok(())
    }
}
