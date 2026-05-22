use crate::error::VmError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    I32(i32),
    String(u16),
    RuntimeString(u8),
    Record(u8),
    List(u8),
}

impl Value {
    pub(crate) const fn truthy(self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(value) => value,
            Value::I32(value) => value != 0,
            Value::String(_) | Value::RuntimeString(_) | Value::Record(_) | Value::List(_) => true,
        }
    }

    pub(crate) fn expect_i32(self) -> Result<i32, VmError> {
        match self {
            Value::I32(value) => Ok(value),
            _ => Err(VmError::InvalidOperand),
        }
    }

    pub(crate) const fn is_string(self) -> bool {
        matches!(self, Value::String(_) | Value::RuntimeString(_))
    }
}
