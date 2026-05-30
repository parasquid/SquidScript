use core::{fmt::Write, str};

use crate::{
    bytecode::{
        read_u16, read_value, STATE_RECORD_MAGIC, STATE_TYPE_BOOL, STATE_TYPE_INT,
        STATE_TYPE_STRING, VALUE_BOOL, VALUE_I32, VALUE_NULL, VALUE_STRING,
    },
    error::VmError,
    limits::{MAX_RUNTIME_STRING_BYTES, MAX_SAVED_STATE_BYTES, MAX_STATE},
    model::{StateSlot, StateType},
    strings::{RuntimeServiceStrings, RuntimeStrings, StringResolver, StringTable},
    value::Value,
};

pub(crate) fn parse_state(bytes: &[u8]) -> Result<([StateSlot; MAX_STATE], usize), VmError> {
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

pub(crate) fn state_value_matches(tag: u8, nullable: bool, value: Value) -> bool {
    if value == Value::Null {
        return nullable;
    }
    matches!(
        (tag, value),
        (STATE_TYPE_INT, Value::I32(_))
            | (STATE_TYPE_BOOL, Value::Bool(_))
            | (STATE_TYPE_STRING, Value::String(_))
            | (STATE_TYPE_STRING, Value::RuntimeString(_))
            | (STATE_TYPE_STRING, Value::ServiceString(_))
    )
}

pub(crate) fn values_equal(
    strings: &dyn StringTable,
    runtime_strings: &RuntimeStrings,
    service_strings: &RuntimeServiceStrings,
    left: Value,
    right: Value,
) -> Result<bool, VmError> {
    if left.is_string() || right.is_string() {
        if !left.is_string() || !right.is_string() {
            return Ok(false);
        }
        let resolver =
            StringResolver::with_service_strings(strings, runtime_strings, service_strings);
        return Ok(resolver.value_str(left)? == resolver.value_str(right)?);
    }
    Ok(left == right)
}

pub(crate) fn concat_value_strings(
    strings: &dyn StringTable,
    runtime_strings: &RuntimeStrings,
    service_strings: &RuntimeServiceStrings,
    left: Value,
    right: Value,
    out: &mut [u8; MAX_RUNTIME_STRING_BYTES],
) -> Result<usize, VmError> {
    let resolver = StringResolver::with_service_strings(strings, runtime_strings, service_strings);
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

pub(crate) fn encode_state_record(
    strings: &dyn StringTable,
    runtime_strings: &RuntimeStrings,
    service_strings: &RuntimeServiceStrings,
    slots: &[StateSlot],
    state: &[Value],
    out: &mut [u8; MAX_SAVED_STATE_BYTES],
) -> Result<usize, VmError> {
    let mut cursor = 0usize;
    write_bytes(out, &mut cursor, STATE_RECORD_MAGIC)?;
    write_byte(out, &mut cursor, slots.len() as u8)?;
    let resolver = StringResolver::with_service_strings(strings, runtime_strings, service_strings);
    for (slot, value) in slots.iter().zip(state.iter().copied()) {
        let name = strings.string(slot.name_id)?;
        write_len_prefixed(out, &mut cursor, name.as_bytes())?;
        write_byte(out, &mut cursor, slot.value_type.tag)?;
        write_byte(out, &mut cursor, u8::from(slot.value_type.nullable))?;
        encode_state_record_value(out, &mut cursor, &resolver, value)?;
    }
    Ok(cursor)
}

pub(crate) fn encode_state_record_value(
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
        Value::String(_) | Value::RuntimeString(_) | Value::ServiceString(_) => {
            write_byte(out, cursor, VALUE_STRING)?;
            write_len_prefixed(out, cursor, strings.value_str(value)?.as_bytes())
        }
        Value::Record(_) | Value::List(_) => Err(VmError::InvalidOperand),
    }
}

pub(crate) fn apply_state_record(
    bytes: &[u8],
    strings: &dyn StringTable,
    slots: &[StateSlot],
    runtime_strings: &mut RuntimeStrings,
    state: &mut [Value],
) -> Result<(), VmError> {
    if bytes.len() > MAX_SAVED_STATE_BYTES || bytes.len() < 5 {
        return Err(VmError::InvalidStateRecord);
    }
    if bytes.get(0..4) != Some(&STATE_RECORD_MAGIC[..]) {
        return Err(VmError::InvalidStateRecord);
    }
    let count = *bytes.get(4).ok_or(VmError::InvalidStateRecord)? as usize;
    let mut cursor = 5usize;
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
