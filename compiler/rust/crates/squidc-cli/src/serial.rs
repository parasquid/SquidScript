use std::{
    env, fs,
    fs::{File, OpenOptions},
    io::{Read, Write},
    process::Command,
    time::{Duration, Instant},
};

use squid_device_protocol::{
    app_install_begin_request, app_install_chunk_request, app_install_commit_request,
    app_launch_request, app_list_entries, app_list_request, decode_frame_from_stream,
    drawlog_get_request, drawlog_lines, encode_frame, error_lines, errors_get_request,
    event_dispatch_request, hello_identity, hello_request, key_request, lifecycle_get_request,
    lifecycle_lines, output_get_request, output_lines, protocol_error, reset_request,
    resource_install_begin_request, resource_install_chunk_request,
    resource_install_commit_request, resource_values, resources_get_request, state_bytes,
    state_get_request, state_import_request, storage_format_request, temp_run_begin_request,
    temp_run_chunk_request, temp_run_commit_request, trace_get_request, trace_lines,
    wifi_profile_set_request, AppEntry, Frame, FrameKind, Status,
};

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
        let request = encode_frame(&hello_request(1));
        let response = device.send_bytes_until_quiet(&request)?;
        let frame = decode_frame_from_stream(&response)
            .map_err(|error| format!("invalid hello frame: {error:?}"))?;
        Ok(hello_identity(&frame).is_some())
    }

    pub fn install_app(&mut self, app_id: &str, bytes: &[u8]) -> Result<String, String> {
        self.send_protocol_expect_ok(&app_install_begin_request(
            10,
            app_id,
            bytes.len() as u64,
            crc32fast::hash(bytes) as u64,
        ))?;
        for (index, chunk) in bytes.chunks(CHUNK_SIZE).enumerate() {
            self.send_protocol_expect_ok(&app_install_chunk_request(
                11 + index as u32,
                (index * CHUNK_SIZE) as u64,
                chunk.to_vec(),
            ))?;
        }
        self.send_protocol_expect_ok(&app_install_commit_request(
            11 + bytes.chunks(CHUNK_SIZE).count() as u32,
        ))?;
        Ok(format!("installed app {app_id} len={}\n", bytes.len()))
    }

    pub fn install_resource(
        &mut self,
        app_id: &str,
        path: &str,
        bytes: &[u8],
    ) -> Result<String, String> {
        self.send_protocol_expect_ok(&resource_install_begin_request(
            50,
            app_id,
            path,
            bytes.len() as u64,
            crc32fast::hash(bytes) as u64,
        ))?;
        for (index, chunk) in bytes.chunks(CHUNK_SIZE).enumerate() {
            self.send_protocol_expect_ok(&resource_install_chunk_request(
                51 + index as u32,
                (index * CHUNK_SIZE) as u64,
                chunk.to_vec(),
            ))?;
        }
        self.send_protocol_expect_ok(&resource_install_commit_request(
            51 + bytes.chunks(CHUNK_SIZE).count() as u32,
        ))?;
        Ok(format!(
            "installed resource {app_id}/{path} len={}\n",
            bytes.len()
        ))
    }

    pub fn run_temp_app(&mut self, app_id: &str, bytes: &[u8]) -> Result<String, String> {
        self.send_protocol_expect_ok(&temp_run_begin_request(
            30,
            app_id,
            bytes.len() as u64,
            crc32fast::hash(bytes) as u64,
        ))?;
        for (index, chunk) in bytes.chunks(CHUNK_SIZE).enumerate() {
            self.send_protocol_expect_ok(&temp_run_chunk_request(
                31 + index as u32,
                (index * CHUNK_SIZE) as u64,
                chunk.to_vec(),
            ))?;
        }
        self.send_protocol_expect_ok(&temp_run_commit_request(
            31 + bytes.chunks(CHUNK_SIZE).count() as u32,
        ))?;
        Ok(format!("ran temp app {app_id} len={}\n", bytes.len()))
    }

    pub fn import_state(&mut self, bytes: &[u8]) -> Result<String, String> {
        self.send_protocol_expect_ok(&state_import_request(72, bytes.to_vec()))?;
        Ok(format!("imported state len={}\n", bytes.len()))
    }

    pub fn run_app(&mut self, app_id: &str) -> Result<String, String> {
        self.send_protocol_expect_ok(&app_launch_request(20, app_id))?;
        Ok(format!("launched app {app_id}\n"))
    }

    pub fn run_app_event(&mut self, app_id: &str, event: &str) -> Result<String, String> {
        self.send_protocol_expect_ok(&event_dispatch_request(49, app_id, event))?;
        Ok(format!("dispatched event {event} for {app_id}\n"))
    }

    pub fn send_bytes_until_quiet(&mut self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        self.drain();
        self.write_all(bytes)?;
        self.read_bytes_until_quiet(DEFAULT_TIMEOUT)
    }

    pub fn app_list(&mut self) -> Result<Vec<AppEntry>, String> {
        let request = encode_frame(&app_list_request(2));
        let response = self.send_bytes_until_quiet(&request)?;
        let frame = decode_frame_from_stream(&response)
            .map_err(|error| format!("invalid app list response frame: {error:?}"))?;
        app_list_entries(&frame).ok_or_else(|| "not a successful app list response".to_string())
    }

    pub fn output_lines(&mut self) -> Result<Vec<String>, String> {
        let request = encode_frame(&output_get_request(3));
        let response = self.send_bytes_until_quiet(&request)?;
        let frame = decode_frame_from_stream(&response)
            .map_err(|error| format!("invalid output response frame: {error:?}"))?;
        output_lines(&frame).ok_or_else(|| "not a successful output response".to_string())
    }

    pub fn trace_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&trace_get_request(4))?;
        trace_lines(&frame).ok_or_else(|| "not a successful trace response".to_string())
    }

    pub fn lifecycle_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&lifecycle_get_request(9))?;
        lifecycle_lines(&frame).ok_or_else(|| "not a successful lifecycle response".to_string())
    }

    pub fn drawlog_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&drawlog_get_request(5))?;
        drawlog_lines(&frame).ok_or_else(|| "not a successful drawlog response".to_string())
    }

    pub fn error_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&errors_get_request(6))?;
        error_lines(&frame).ok_or_else(|| "not a successful errors response".to_string())
    }

    pub fn state_bytes(&mut self) -> Result<Vec<u8>, String> {
        let frame = self.send_protocol_request(&state_get_request(7))?;
        state_bytes(&frame).ok_or_else(|| "not a successful state response".to_string())
    }

    pub fn resource_values(&mut self) -> Result<Vec<(String, u64)>, String> {
        let frame = self.send_protocol_request(&resources_get_request(8))?;
        resource_values(&frame).ok_or_else(|| "not a successful resources response".to_string())
    }

    pub fn reset(&mut self) -> Result<String, String> {
        self.send_protocol_expect_ok(&reset_request(80))?;
        Ok("reset\n".to_string())
    }

    pub fn storage_format(&mut self) -> Result<String, String> {
        self.send_protocol_expect_ok(&storage_format_request(81))?;
        Ok("storage formatted\n".to_string())
    }

    pub fn send_key(&mut self, key: &str) -> Result<String, String> {
        self.send_protocol_expect_ok(&key_request(48, key))?;
        Ok(format!("key {key}\n"))
    }

    pub fn set_wifi_profile(
        &mut self,
        profile: &str,
        ssid: &str,
        password: &str,
    ) -> Result<(), String> {
        self.send_protocol_expect_ok(&wifi_profile_set_request(76, profile, ssid, password))
    }

    pub fn send_protocol_request(&mut self, frame: &Frame) -> Result<Frame, String> {
        let request = encode_frame(frame);
        let response = self.send_bytes_until_quiet(&request)?;
        let response_frame = decode_frame_from_stream(&response)
            .map_err(|error| format!("invalid protocol response frame: {error:?}"))?;
        if let Some(error) = protocol_error(&response_frame) {
            return Err(format!("{} ({})", error.message, error.code));
        }
        Ok(response_frame)
    }

    fn send_protocol_expect_ok(&mut self, frame: &Frame) -> Result<(), String> {
        let response_frame = self.send_protocol_request(frame)?;
        if response_frame.kind != FrameKind::Response
            || response_frame.opcode != frame.opcode
            || response_frame.status != Status::Ok
            || response_frame.sequence != frame.sequence
        {
            return Err(format!("unexpected protocol response: {response_frame:?}"));
        }
        Ok(())
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

    fn read_bytes_until_quiet(&mut self, timeout: Duration) -> Result<Vec<u8>, String> {
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
        Ok(response)
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
            "no SquidScript firmware serial target found; candidates: {}; pass --port or set ESPFLASH_PORT",
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
    if let Ok(entries) = fs::read_dir("/dev/serial/by-id") {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.contains("Espressif") {
                out.push(path.display().to_string());
            }
        }
    }
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            if name.starts_with("ttyACM")
                || name.starts_with("ttyUSB")
                || name.starts_with("cu.usbmodem")
                || name.starts_with("cu.SLAB_USBtoUART")
            {
                out.push(format!("/dev/{name}"));
            }
        }
    }
    out.sort();
    let mut unique = Vec::new();
    let mut seen = Vec::new();
    for path in out {
        let real = fs::canonicalize(&path)
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| path.clone());
        if seen.iter().any(|existing| existing == &real) {
            continue;
        }
        seen.push(real);
        unique.push(path);
    }
    unique
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

pub fn format_lines(prefix: &str, lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| format!("{prefix}={line}\n"))
        .collect::<String>()
}

pub fn format_state_bytes(bytes: &[u8]) -> String {
    format!("state={}\n", hex_string(bytes))
}

pub fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

impl OutputTail {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_lines(&mut self, lines: &[String]) -> Vec<String> {
        if lines.len() < self.seen {
            self.seen = 0;
        }
        let out = lines
            .iter()
            .skip(self.seen)
            .map(|line| format!("output={line}"))
            .collect::<Vec<_>>();
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
        let first = vec!["\"ready\"".to_string(), "\"tick\" true".to_string()];
        assert_eq!(
            tail.next_lines(&first),
            vec!["output=\"ready\"", "output=\"tick\" true"]
        );

        let second = vec![
            "\"ready\"".to_string(),
            "\"tick\" true".to_string(),
            "\"tick\" false".to_string(),
        ];
        assert_eq!(tail.next_lines(&second), vec!["output=\"tick\" false"]);
        assert!(tail.next_lines(&second).is_empty());
    }

    #[test]
    fn output_tail_resets_when_firmware_output_buffer_is_cleared() {
        let mut tail = OutputTail::new();
        tail.next_lines(&["\"old\"".to_string(), "\"older\"".to_string()]);

        assert_eq!(
            tail.next_lines(&["\"new\"".to_string()]),
            vec!["output=\"new\""]
        );
    }
}
