use crate::value::Value;

#[derive(Clone, Copy)]
pub(crate) struct Function {
    pub(crate) _name_id: u16,
    pub(crate) param_count: u16,
    pub(crate) local_count: u16,
    pub(crate) start: usize,
    pub(crate) len: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Handler {
    pub(crate) event_id: u16,
    pub(crate) preload: bool,
    pub(crate) start: usize,
    pub(crate) len: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Screen {
    pub(crate) name_id: u16,
    pub(crate) start: usize,
    pub(crate) len: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct StateType {
    pub(crate) tag: u8,
    pub(crate) nullable: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct StateSlot {
    pub(crate) name_id: u16,
    pub(crate) value_type: StateType,
    pub(crate) default: Value,
}
