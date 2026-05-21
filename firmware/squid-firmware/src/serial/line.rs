const LINE_CAP: usize = 128;

pub fn trim_ascii(input: &str) -> &str {
    input.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

pub struct LineBuffer {
    bytes: [u8; LINE_CAP],
    len: usize,
}

impl LineBuffer {
    pub const fn new() -> Self {
        Self {
            bytes: [0; LINE_CAP],
            len: 0,
        }
    }

    pub fn push(&mut self, byte: u8) -> Option<&str> {
        if byte == b'\n' || byte == b'\r' {
            let line = core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("");
            self.len = 0;
            return Some(line);
        }
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
        None
    }
}
