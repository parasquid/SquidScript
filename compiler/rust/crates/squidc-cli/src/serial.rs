use std::{
    env, fs,
    fs::{File, OpenOptions},
    io::{Read, Write},
    process::Command,
    time::{Duration, Instant},
};

use crate::app_id::fnv1a;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const CHUNK_SIZE: usize = 64;

pub struct SerialDevice {
    port: File,
}

impl SerialDevice {
    pub fn open(port: &str) -> Result<Self, String> {
        configure_tty(port)?;
        let port = OpenOptions::new()
            .read(true)
            .write(true)
            .open(port)
            .map_err(|error| format!("failed to open {port}: {error}"))?;
        Ok(Self { port })
    }

    pub fn probe(port: &str) -> Result<bool, String> {
        let mut device = Self::open(port)?;
        let response = device.send_line("HELLO")?;
        Ok(response.contains("target=") && response.contains("OK HELLO"))
    }

    pub fn install_app(&mut self, app_id: &str, bytes: &[u8]) -> Result<String, String> {
        self.write_install(
            &format!("INSTALL.APP {app_id}"),
            "READY install.app",
            "OK install.app",
            bytes,
        )
    }

    pub fn import_state(&mut self, bytes: &[u8]) -> Result<String, String> {
        self.write_install(
            "STATE.IMPORT",
            "READY STATE.IMPORT",
            "OK STATE.IMPORT",
            bytes,
        )
    }

    pub fn run_app(&mut self, app_id: &str) -> Result<String, String> {
        let response = self.send_line(&format!("RUN.APP {app_id}"))?;
        if !response.contains("OK RUN.APP") {
            return Err(response.trim().to_string());
        }
        Ok(response)
    }

    pub fn run_app_event(&mut self, app_id: &str, event: &str) -> Result<String, String> {
        self.send_line(&format!("RUN.EVENT {app_id} {event}"))
    }

    pub fn send_line(&mut self, line: &str) -> Result<String, String> {
        self.drain();
        self.write_all(format!("{line}\n").as_bytes())?;
        self.read_until_quiet(DEFAULT_TIMEOUT)
    }

    pub fn read_available_text(&mut self) -> Result<String, String> {
        let mut buf = [0u8; 512];
        match self.port.read(&mut buf) {
            Ok(count) if count > 0 => Ok(String::from_utf8_lossy(&buf[..count]).into_owned()),
            Ok(_) => Ok(String::new()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(String::new()),
            Err(error) => Err(format!("serial read failed: {error}")),
        }
    }

    fn write_install(
        &mut self,
        command: &str,
        ready_token: &str,
        ok_token: &str,
        bytes: &[u8],
    ) -> Result<String, String> {
        self.drain();
        let hash = fnv1a(bytes);
        self.write_all(format!("{command} {} {hash:08x}\n", bytes.len()).as_bytes())?;
        let ready = self.read_until(ready_token, DEFAULT_TIMEOUT)?;
        for chunk in bytes.chunks(CHUNK_SIZE) {
            self.write_all(chunk)?;
            std::thread::sleep(Duration::from_millis(2));
        }
        let ok = self.read_until(ok_token, DEFAULT_TIMEOUT)?;
        Ok(format!("{ready}{ok}"))
    }

    fn drain(&mut self) {
        let mut buf = [0u8; 256];
        while let Ok(count) = self.port.read(&mut buf) {
            if count == 0 {
                break;
            }
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.port
            .write_all(bytes)
            .map_err(|error| format!("serial write failed: {error}"))
    }

    fn read_until(&mut self, expected: &str, timeout: Duration) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        let mut response = Vec::new();
        let mut buf = [0u8; 256];
        while Instant::now() < deadline {
            match self.port.read(&mut buf) {
                Ok(count) if count > 0 => {
                    response.extend_from_slice(&buf[..count]);
                    let text = String::from_utf8_lossy(&response);
                    if text.contains("ERR ") {
                        return Err(text.into_owned());
                    }
                    if text.contains(expected) {
                        return Ok(text.into_owned());
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("serial read failed: {error}")),
            }
        }
        Err(format!(
            "timed out waiting for {expected:?}; got {}",
            String::from_utf8_lossy(&response)
        ))
    }

    fn read_until_quiet(&mut self, timeout: Duration) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        let mut quiet_deadline = None;
        let mut response = Vec::new();
        let mut buf = [0u8; 512];
        while Instant::now() < deadline {
            match self.port.read(&mut buf) {
                Ok(count) if count > 0 => {
                    response.extend_from_slice(&buf[..count]);
                    quiet_deadline = Some(Instant::now() + Duration::from_millis(250));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("serial read failed: {error}")),
            }
            if quiet_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                break;
            }
        }
        Ok(String::from_utf8_lossy(&response).into_owned())
    }
}

pub fn detect_port() -> Result<String, String> {
    if let Ok(port) = env::var("ESPFLASH_PORT") {
        return Ok(port);
    }
    let mut matches = Vec::new();
    let candidates = candidate_ports();
    for port in &candidates {
        if SerialDevice::probe(port).unwrap_or(false) {
            matches.push(port.clone());
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(format!(
            "no SquidScript firmware serial target found; candidates: {}; pass --port /dev/ttyACM0",
            candidates.join(", ")
        )),
        _ => Err(format!(
            "multiple SquidScript firmware serial targets found: {}; pass --port",
            matches.join(", ")
        )),
    }
}

pub fn candidate_ports() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            if name.starts_with("ttyACM") || name.starts_with("ttyUSB") {
                out.push(format!("/dev/{name}"));
            }
        }
    }
    out.sort();
    out
}

fn configure_tty(port: &str) -> Result<(), String> {
    let status = Command::new("stty")
        .args(["-F", port, "raw", "-echo", "min", "0", "time", "1"])
        .status()
        .map_err(|error| format!("failed to run stty for {port}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to configure {port} with stty"))
    }
}

#[derive(Default)]
pub struct OutputTail {
    seen: usize,
}

impl OutputTail {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_lines(&mut self, response: &str) -> Vec<String> {
        let lines = response
            .lines()
            .filter(|line| line.starts_with("output="))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if lines.len() < self.seen {
            self.seen = 0;
        }
        let out = lines.iter().skip(self.seen).cloned().collect::<Vec<_>>();
        self.seen = lines.len();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::OutputTail;

    #[test]
    fn output_tail_returns_only_new_output_lines() {
        let mut tail = OutputTail::new();
        let first =
            "BEGIN OUTPUT\noutput=\"ready\"\noutput=\"tick\" true\nEND OUTPUT\nOK OUTPUT.GET\n";
        assert_eq!(
            tail.next_lines(first),
            vec!["output=\"ready\"", "output=\"tick\" true"]
        );

        let second =
            "BEGIN OUTPUT\noutput=\"ready\"\noutput=\"tick\" true\noutput=\"tick\" false\nEND OUTPUT\n";
        assert_eq!(tail.next_lines(second), vec!["output=\"tick\" false"]);
        assert!(tail.next_lines(second).is_empty());
    }

    #[test]
    fn output_tail_resets_when_firmware_output_buffer_is_cleared() {
        let mut tail = OutputTail::new();
        tail.next_lines("BEGIN OUTPUT\noutput=\"old\"\noutput=\"older\"\nEND OUTPUT\n");

        assert_eq!(
            tail.next_lines("BEGIN OUTPUT\noutput=\"new\"\nEND OUTPUT\n"),
            vec!["output=\"new\""]
        );
    }
}
