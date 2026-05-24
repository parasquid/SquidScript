use crate::{error::VmError, value::Value};

pub(crate) const SECTION_STRINGS: u16 = 1;
pub(crate) const SECTION_STATE: u16 = 2;
pub(crate) const SECTION_FUNCTIONS: u16 = 3;
pub(crate) const SECTION_HANDLERS: u16 = 4;
pub(crate) const SECTION_CODE: u16 = 5;
pub(crate) const SECTION_SCREENS: u16 = 6;
pub(crate) const SECTION_TRIGGERS: u16 = 9;

pub(crate) const OP_PUSH_INT: u8 = 1;
pub(crate) const OP_PUSH_BOOL: u8 = 2;
pub(crate) const OP_PUSH_STRING: u8 = 3;
pub(crate) const OP_PUSH_NULL: u8 = 4;
pub(crate) const OP_GET_STATE: u8 = 10;
pub(crate) const OP_SET_STATE: u8 = 11;
pub(crate) const OP_GET_LOCAL: u8 = 12;
pub(crate) const OP_SET_LOCAL: u8 = 13;
pub(crate) const OP_GET_FIELD: u8 = 14;
pub(crate) const OP_ADD: u8 = 20;
pub(crate) const OP_SUB: u8 = 21;
pub(crate) const OP_EQ: u8 = 22;
pub(crate) const OP_NE: u8 = 23;
pub(crate) const OP_LT: u8 = 24;
pub(crate) const OP_LTE: u8 = 25;
pub(crate) const OP_GT: u8 = 26;
pub(crate) const OP_GTE: u8 = 27;
pub(crate) const OP_JUMP: u8 = 30;
pub(crate) const OP_JUMP_IF_FALSE: u8 = 31;
pub(crate) const OP_CALL_FUNCTION: u8 = 40;
pub(crate) const OP_RETURN: u8 = 41;
pub(crate) const OP_HALT: u8 = 42;
pub(crate) const OP_CALL_BUILTIN: u8 = 50;
pub(crate) const OP_POP: u8 = 60;
pub(crate) const OP_LIST_LEN: u8 = 61;
pub(crate) const OP_LIST_GET: u8 = 62;

pub(crate) const BUILTIN_STATE_LOAD: u8 = 1;
pub(crate) const BUILTIN_STATE_SAVE: u8 = 2;
pub(crate) const BUILTIN_APP_EXIT: u8 = 3;
pub(crate) const BUILTIN_DEBUG_PRINT: u8 = 4;
pub(crate) const BUILTIN_SCREEN_OPEN: u8 = 5;
pub(crate) const BUILTIN_DISPLAY_CLEAR: u8 = 6;
pub(crate) const BUILTIN_DISPLAY_TEXT: u8 = 7;
pub(crate) const BUILTIN_DISPLAY_RECT: u8 = 8;
pub(crate) const BUILTIN_DISPLAY_LINE: u8 = 9;
pub(crate) const BUILTIN_HARDWARE_GPIO_WRITE: u8 = 10;
pub(crate) const BUILTIN_HARDWARE_GPIO_TOGGLE: u8 = 11;
pub(crate) const BUILTIN_HARDWARE_GPIO_READ: u8 = 12;
pub(crate) const BUILTIN_APP_LAUNCH: u8 = 13;
pub(crate) const BUILTIN_STATE_RESET: u8 = 14;
pub(crate) const BUILTIN_SCREEN_REFRESH: u8 = 15;
pub(crate) const BUILTIN_APP_ARM: u8 = 16;
pub(crate) const BUILTIN_APP_DISARM: u8 = 17;
pub(crate) const BUILTIN_SERVICE_TIMER_EVERY: u8 = 18;
pub(crate) const BUILTIN_SERVICE_TIMER_AFTER: u8 = 19;
pub(crate) const BUILTIN_SYSTEM_MEMORY: u8 = 20;
pub(crate) const BUILTIN_SYSTEM_STORAGE: u8 = 21;
pub(crate) const BUILTIN_DISPLAY_SELECT: u8 = 22;
pub(crate) const BUILTIN_DISPLAY_IMAGE: u8 = 23;
pub(crate) const BUILTIN_DISPLAY_DRAW: u8 = 24;
pub(crate) const BUILTIN_DEVICE_CONFIG_LOAD: u8 = 25;
pub(crate) const BUILTIN_DEVICE_CONFIG_SET: u8 = 26;
pub(crate) const BUILTIN_SERVICE_INDICATOR_WRITE: u8 = 27;
pub(crate) const BUILTIN_SERVICE_INDICATOR_TOGGLE: u8 = 28;
pub(crate) const BUILTIN_SERVICE_INDICATOR_READ: u8 = 29;
pub(crate) const BUILTIN_SERVICE_INDICATOR_BREATHE: u8 = 34;
pub(crate) const BUILTIN_SERVICE_WIFI_START_AP: u8 = 30;
pub(crate) const BUILTIN_SERVICE_WIFI_STOP_AP: u8 = 31;
pub(crate) const BUILTIN_SERVICE_WIFI_STATUS: u8 = 32;
pub(crate) const BUILTIN_SERVICE_WIFI_GET_AP_IP: u8 = 33;
pub(crate) const BUILTIN_SERVICE_WIFI_CONNECT: u8 = 35;
pub(crate) const BUILTIN_SERVICE_WIFI_DISCONNECT: u8 = 36;
pub(crate) const BUILTIN_SERVICE_WIFI_SCAN: u8 = 37;
pub(crate) const BUILTIN_APP_REGISTRY_LIST: u8 = 38;
pub(crate) const BUILTIN_APP_REGISTRY_GET: u8 = 39;
pub(crate) const BUILTIN_APP_PROCESS_STACK: u8 = 40;
pub(crate) const BUILTIN_APP_ARMED_STACK: u8 = 41;
pub(crate) const BUILTIN_APP_ARMED_STACK_GET: u8 = 42;
pub(crate) const BUILTIN_DEVICE_CONFIG_REBIND: u8 = 43;
pub(crate) const BUILTIN_DEVICE_CONFIG_SAVE: u8 = 44;
pub(crate) const BUILTIN_SERVICE_INDICATOR_BLINK: u8 = 45;

pub(crate) const VALUE_NULL: u8 = 0;
pub(crate) const VALUE_BOOL: u8 = 1;
pub(crate) const VALUE_I32: u8 = 2;
pub(crate) const VALUE_STRING: u8 = 3;

pub(crate) const STATE_TYPE_INT: u8 = 1;
pub(crate) const STATE_TYPE_BOOL: u8 = 2;
pub(crate) const STATE_TYPE_STRING: u8 = 3;

pub(crate) const STATE_RECORD_MAGIC: &[u8; 4] = b"SQST";

pub(crate) fn read_value(bytes: &[u8], cursor: usize) -> Result<(Value, usize), VmError> {
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

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, VmError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(VmError::InvalidSection)?
            .try_into()
            .map_err(|_| VmError::InvalidSection)?,
    ))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, VmError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(VmError::InvalidSection)?
            .try_into()
            .map_err(|_| VmError::InvalidSection)?,
    ))
}

pub(crate) fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, VmError> {
    Ok(i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(VmError::InvalidSection)?
            .try_into()
            .map_err(|_| VmError::InvalidSection)?,
    ))
}
