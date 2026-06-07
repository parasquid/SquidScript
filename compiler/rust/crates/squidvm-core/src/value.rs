use crate::error::VmError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringRef {
    Sqbc(u16),
    Static(u8),
    Dynamic(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandleKind {
    BinBook,
    Drawable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Handle {
    pub kind: HandleKind,
    pub id: u16,
}

impl Handle {
    pub const fn new(kind: HandleKind, id: u16) -> Self {
        Self { kind, id }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    I32(i32),
    String(StringRef),
    Record(u8),
    List(u8),
    Handle(Handle),
}

impl Value {
    pub(crate) const fn truthy(self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(value) => value,
            Value::I32(value) => value != 0,
            Value::String(_) | Value::Record(_) | Value::List(_) | Value::Handle(_) => true,
        }
    }

    pub(crate) fn expect_i32(self) -> Result<i32, VmError> {
        match self {
            Value::I32(value) => Ok(value),
            _ => Err(VmError::InvalidOperand),
        }
    }

    pub(crate) const fn is_string(self) -> bool {
        matches!(self, Value::String(_))
    }
}
