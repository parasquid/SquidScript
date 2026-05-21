use core::fmt::{self, Write};

use squidvm_core::{strings::StringResolver, value::Value};

const LOG_LINE_CAP: usize = 80;

pub(super) fn write_human_bytes(
    out: &mut dyn fmt::Write,
    label: &str,
    bytes: usize,
) -> Result<(), fmt::Error> {
    if bytes >= 1024 * 1024 {
        write!(out, "{label} {} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        write!(out, "{label} {} KiB", bytes / 1024)
    } else {
        write!(out, "{label} {bytes} B")
    }
}

#[derive(Clone, Copy)]
pub(super) struct LogLine {
    bytes: [u8; LOG_LINE_CAP],
    len: usize,
}

impl LogLine {
    pub(super) const fn new() -> Self {
        Self {
            bytes: [0; LOG_LINE_CAP],
            len: 0,
        }
    }

    pub(super) fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("<bad-log>")
    }
}

impl Write for LogLine {
    fn write_str(&mut self, input: &str) -> fmt::Result {
        let remaining = self.bytes.len().saturating_sub(self.len);
        let bytes = input.as_bytes();
        let copy_len = remaining.min(bytes.len());
        self.bytes[self.len..self.len + copy_len].copy_from_slice(&bytes[..copy_len]);
        self.len += copy_len;
        Ok(())
    }
}

pub(super) fn write_value(
    out: &mut impl Write,
    strings: &StringResolver<'_>,
    value: Value,
) -> Result<(), fmt::Error> {
    match value {
        Value::Null => write!(out, "null"),
        Value::Bool(value) => write!(out, "{value}"),
        Value::I32(value) => write!(out, "{value}"),
        Value::String(_) | Value::RuntimeString(_) => {
            write!(
                out,
                "\"{}\"",
                strings.value_str(value).unwrap_or("<bad-string>")
            )
        }
        Value::Record(_) => write!(out, "<record>"),
    }
}

pub(super) fn stable_trace(message: &str) -> &'static str {
    match message {
        "state.load" => "state.load",
        "state.save" => "state.save",
        "app.exit" => "app.exit",
        "app.start" => "app.start",
        "app.arm" => "app.arm",
        "key.SELECT" => "key.SELECT",
        "key.BACK" => "key.BACK",
        "timer.clock" => "timer.clock",
        "timer.break" => "timer.break",
        "timer.debug" => "timer.debug",
        "app.launch" => "app.launch",
        _ => "unknown",
    }
}
