use std::{
    env, fs,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use squid_device_protocol::{
    app_install_begin_request, app_install_chunk_request_with_ack, app_install_commit_request,
    app_launch_request, app_list_entries, app_list_request, content_check_request,
    content_check_result, content_delete_request, content_delete_result,
    content_install_begin_request, content_install_chunk_request_with_ack,
    content_install_commit_request, debug_log_get_request, debug_log_lines,
    decode_frame_from_stream, display_window_probe_request, drawlog_get_request, drawlog_lines,
    encode_frame, error_lines, errors_get_request, event_dispatch_request, hello_identity,
    hello_request, key_request, lifecycle_get_request, lifecycle_lines, output_get_request,
    output_lines, protocol_error, reset_request, resource_install_begin_request,
    resource_install_chunk_request_with_ack, resource_install_commit_request, resource_values,
    resources_get_request, resources_get_request_with_heap_reset, runtime_cap_clear_request,
    runtime_cap_get_request, runtime_cap_lines, runtime_cap_set_request, state_bytes,
    state_get_request, state_import_request, storage_format_request, temp_run_begin_request,
    temp_run_chunk_request_with_ack, temp_run_commit_request, trace_get_request, trace_lines,
    wifi_profile_set_request, AppEntry, ContentCheckResult, DecodeError, Frame, FrameKind, Status,
    TransferCapabilities, HEADER_LEN, MAGIC,
};

const DEFAULT_TIMEOUT_SECONDS: u64 = 60;

fn default_timeout() -> Duration {
    env::var("SQUID_SERIAL_RESPONSE_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT_SECONDS))
}
#[cfg(test)]
const FIRMWARE_SERIAL_FRAME_BUDGET: usize = 4096;

pub struct SerialDevice {
    port: File,
    transfer_capabilities: TransferCapabilities,
}

impl SerialDevice {
    pub fn open(port: &str) -> Result<Self, String> {
        let mut device = Self::open_raw(port)?;
        device.wait_until_ready()?;
        Ok(device)
    }

    fn open_raw(port: &str) -> Result<Self, String> {
        configure_tty(port)?;
        let port = OpenOptions::new()
            .read(true)
            .write(true)
            .open(port)
            .map_err(|error| format!("failed to open {port}: {error}"))?;
        Ok(Self {
            port,
            transfer_capabilities: TransferCapabilities::default_serial(),
        })
    }

    pub fn probe(port: &str) -> Result<bool, String> {
        let mut device = Self::open_raw(port)?;
        let request = encode_frame(&hello_request(1));
        let response = device.send_bytes_until_quiet(&request)?;
        let frame = decode_frame_from_stream(&response)
            .map_err(|error| format!("invalid hello frame: {error:?}"))?;
        Ok(hello_identity(&frame).is_some())
    }

    pub fn install_app(&mut self, app_id: &str, bytes: &[u8]) -> Result<String, String> {
        let transfer = self.serial_transfer_plan(bytes.len());
        self.send_protocol_expect_ok(&app_install_begin_request(
            10,
            app_id,
            bytes.len() as u64,
            crc32fast::hash(bytes) as u64,
        ))
        .map_err(|error| format!("app install begin: {error}"))?;
        for (index, planned) in transfer.chunks.iter().enumerate() {
            let chunk = &bytes[planned.offset..planned.offset + planned.len];
            self.send_protocol_transfer_chunk(
                &app_install_chunk_request_with_ack(
                    11 + index as u32,
                    planned.offset as u64,
                    chunk.to_vec(),
                    planned.ack_requested,
                ),
                planned.ack_requested,
            )
            .map_err(|error| format!("app install chunk at offset {}: {error}", planned.offset))?;
        }
        self.send_protocol_expect_ok(&app_install_commit_request(
            11 + transfer.chunks.len() as u32,
        ))
        .map_err(|error| format!("app install commit: {error}"))?;
        Ok(format!("installed app {app_id} len={}\n", bytes.len()))
    }

    pub fn install_resource(
        &mut self,
        app_id: &str,
        path: &str,
        bytes: &[u8],
    ) -> Result<String, String> {
        let transfer = self.serial_transfer_plan(bytes.len());
        self.send_protocol_expect_ok(&resource_install_begin_request(
            50,
            app_id,
            path,
            bytes.len() as u64,
            crc32fast::hash(bytes) as u64,
        ))?;
        for (index, planned) in transfer.chunks.iter().enumerate() {
            let chunk = &bytes[planned.offset..planned.offset + planned.len];
            self.send_protocol_transfer_chunk(
                &resource_install_chunk_request_with_ack(
                    51 + index as u32,
                    planned.offset as u64,
                    chunk.to_vec(),
                    planned.ack_requested,
                ),
                planned.ack_requested,
            )?;
        }
        self.send_protocol_expect_ok(&resource_install_commit_request(
            51 + transfer.chunks.len() as u32,
        ))?;
        Ok(format!(
            "installed resource {app_id}/{path} len={}\n",
            bytes.len()
        ))
    }

    pub fn install_content(&mut self, name: &str, source: &Path) -> Result<String, String> {
        self.install_content_with_progress(name, source, |_, _, _| {})
    }

    pub fn install_content_with_progress(
        &mut self,
        name: &str,
        source: &Path,
        mut progress: impl FnMut(usize, usize, Duration),
    ) -> Result<String, String> {
        if !is_safe_content_name(name) {
            return Err(format!("invalid content name: {name}"));
        }
        let total_len = fs::metadata(source)
            .map_err(|error| format!("failed to stat {}: {error}", source.display()))?
            .len() as usize;
        let mut hasher = crc32fast::Hasher::new();
        let mut file = File::open(source)
            .map_err(|error| format!("failed to open {}: {error}", source.display()))?;
        let mut crc_buf = [0u8; 8192];
        loop {
            let read = file
                .read(&mut crc_buf)
                .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
            if read == 0 {
                break;
            }
            hasher.update(&crc_buf[..read]);
        }
        let expected_crc = hasher.finalize();
        let transfer = self.serial_transfer_plan(total_len);
        self.send_protocol_expect_ok(&content_install_begin_request(
            88,
            name,
            total_len as u64,
            expected_crc as u64,
        ))?;

        let started = Instant::now();
        let mut file = File::open(source)
            .map_err(|error| format!("failed to open {}: {error}", source.display()))?;
        for (index, planned) in transfer.chunks.iter().enumerate() {
            let mut chunk = vec![0u8; planned.len];
            file.read_exact(&mut chunk)
                .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
            self.send_protocol_transfer_chunk(
                &content_install_chunk_request_with_ack(
                    89 + index as u32,
                    planned.offset as u64,
                    chunk,
                    planned.ack_requested,
                ),
                planned.ack_requested,
            )?;
            progress(planned.offset + planned.len, total_len, started.elapsed());
        }
        self.send_protocol_expect_ok(&content_install_commit_request(
            89 + transfer.chunks.len() as u32,
        ))?;
        Ok(format!("installed content {name} len={total_len}\n"))
    }

    pub fn content_check(&mut self, name: &str) -> Result<ContentCheckResult, String> {
        if !is_safe_content_name(name) {
            return Err(format!("invalid content name: {name}"));
        }
        let request = content_check_request(91, name);
        let deadline = Instant::now() + default_timeout();
        loop {
            let frame = self.send_protocol_request(&request)?;
            if frame.kind == FrameKind::Response
                && frame.opcode == request.opcode
                && frame.status == Status::Pending
                && frame.sequence == request.sequence
            {
                if Instant::now() >= deadline {
                    return Err("content check did not complete before timeout".to_string());
                }
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            return content_check_result(&frame)
                .ok_or_else(|| "not a successful content check response".to_string());
        }
    }

    pub fn content_delete(&mut self, name: &str) -> Result<String, String> {
        if !is_safe_content_name(name) {
            return Err(format!("invalid content name: {name}"));
        }
        let frame = self.send_protocol_request(&content_delete_request(93, name))?;
        content_delete_result(&frame)
            .ok_or_else(|| "not a successful content delete response".to_string())
    }

    pub fn run_temp_app(&mut self, app_id: &str, bytes: &[u8]) -> Result<String, String> {
        let transfer = self.serial_transfer_plan(bytes.len());
        self.send_protocol_expect_ok(&temp_run_begin_request(
            30,
            app_id,
            bytes.len() as u64,
            crc32fast::hash(bytes) as u64,
        ))?;
        for (index, planned) in transfer.chunks.iter().enumerate() {
            let chunk = &bytes[planned.offset..planned.offset + planned.len];
            self.send_protocol_transfer_chunk(
                &temp_run_chunk_request_with_ack(
                    31 + index as u32,
                    planned.offset as u64,
                    chunk.to_vec(),
                    planned.ack_requested,
                ),
                planned.ack_requested,
            )?;
        }
        self.send_protocol_expect_ok(&temp_run_commit_request(31 + transfer.chunks.len() as u32))?;
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
        self.read_bytes_until_quiet(default_timeout())
    }

    pub fn app_list(&mut self) -> Result<Vec<AppEntry>, String> {
        let frame = self.send_protocol_request(&app_list_request(2))?;
        app_list_entries(&frame).ok_or_else(|| "not a successful app list response".to_string())
    }

    pub fn output_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&output_get_request(3))?;
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

    pub fn debug_log_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&debug_log_get_request(10))?;
        debug_log_lines(&frame).ok_or_else(|| "not a successful debug-log response".to_string())
    }

    pub fn error_lines(&mut self) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&errors_get_request(6))?;
        error_lines(&frame).ok_or_else(|| "not a successful errors response".to_string())
    }

    pub fn state_bytes(&mut self) -> Result<Vec<u8>, String> {
        let frame = self.send_protocol_request(&state_get_request(7))?;
        state_bytes(&frame).ok_or_else(|| "not a successful state response".to_string())
    }

    pub fn resource_values(&mut self, reset_heap_max: bool) -> Result<Vec<(String, u64)>, String> {
        let request = if reset_heap_max {
            resources_get_request_with_heap_reset(8)
        } else {
            resources_get_request(8)
        };
        let frame = self.send_protocol_request(&request)?;
        resource_values(&frame).ok_or_else(|| "not a successful resources response".to_string())
    }

    pub fn reset(&mut self) -> Result<String, String> {
        self.send_protocol_expect_ok(&reset_request(80))?;
        Ok("reset\n".to_string())
    }

    pub fn storage_format(&mut self) -> Result<String, String> {
        let request = storage_format_request(81);
        for _ in 0..256 {
            let response_frame = self.send_protocol_request(&request)?;
            if response_frame.kind != FrameKind::Response
                || response_frame.opcode != request.opcode
                || response_frame.sequence != request.sequence
            {
                return Err(format!("unexpected protocol response: {response_frame:?}"));
            }
            match response_frame.status {
                Status::Ok => return Ok("storage formatted\n".to_string()),
                Status::Pending => continue,
                Status::Error => {
                    return Err(format!("unexpected protocol response: {response_frame:?}"))
                }
            }
        }
        return Err("storage format did not complete after 256 bounded steps".to_string());
    }

    pub fn send_key(&mut self, key: &str) -> Result<String, String> {
        self.send_protocol_expect_ok(&key_request(48, key))?;
        Ok(format!("key {key}\n"))
    }

    pub fn display_window_probe(&mut self, pattern: &str) -> Result<String, String> {
        self.send_protocol_expect_ok(&display_window_probe_request(85, pattern))?;
        Ok(format!("display window probe {pattern}\n"))
    }

    pub fn set_wifi_profile(
        &mut self,
        profile: &str,
        ssid: &str,
        password: &str,
    ) -> Result<(), String> {
        self.send_protocol_expect_ok(&wifi_profile_set_request(76, profile, ssid, password))
    }

    pub fn runtime_cap_get(&mut self, key: Option<&str>) -> Result<Vec<String>, String> {
        let frame = self.send_protocol_request(&runtime_cap_get_request(82, key))?;
        runtime_cap_lines(&frame).ok_or_else(|| "not a successful runtime-cap response".to_string())
    }

    pub fn runtime_cap_set(&mut self, key: &str, value: u16) -> Result<(), String> {
        self.send_protocol_expect_ok(&runtime_cap_set_request(83, key, value))
    }

    pub fn runtime_cap_clear(&mut self, key: Option<&str>) -> Result<(), String> {
        self.send_protocol_expect_ok(&runtime_cap_clear_request(84, key))
    }

    pub fn send_protocol_request(&mut self, frame: &Frame) -> Result<Frame, String> {
        let request = encode_frame(frame);
        let response_frame = self.send_protocol_request_bytes(&request)?;
        if let Some(error) = protocol_error(&response_frame) {
            return Err(format!("{} ({})", error.message, error.code));
        }
        Ok(response_frame)
    }

    fn send_protocol_request_bytes(&mut self, request: &[u8]) -> Result<Frame, String> {
        self.drain();
        self.write_all(request)?;
        let response = self.read_protocol_frame(default_timeout())?;
        decode_frame_from_stream(&response).map_err(|error| format_decode_error(error, &response))
    }

    fn send_protocol_transfer_chunk(
        &mut self,
        frame: &Frame,
        wait_for_ack: bool,
    ) -> Result<(), String> {
        let request = encode_frame(frame);
        if wait_for_ack {
            self.write_all(&request)?;
            let response = self.read_protocol_frame(default_timeout())?;
            let response_frame = decode_frame_from_stream(&response)
                .map_err(|error| format_decode_error(error, &response))?;
            if let Some(error) = protocol_error(&response_frame) {
                return Err(format!("{} ({})", error.message, error.code));
            }
            if response_frame.kind != FrameKind::Response
                || response_frame.opcode != frame.opcode
                || response_frame.status != Status::Ok
                || response_frame.sequence != frame.sequence
            {
                return Err(format!("unexpected protocol response: {response_frame:?}"));
            }
            Ok(())
        } else {
            self.write_all(&request)
        }
    }

    fn wait_until_ready(&mut self) -> Result<(), String> {
        let request = encode_frame(&hello_request(1));
        let mut last_error = None;
        for _ in 0..10 {
            let response = self.send_bytes_until_quiet(&request)?;
            match decode_frame_from_stream(&response) {
                Ok(frame) => {
                    if let Some(identity) = hello_identity(&frame) {
                        self.transfer_capabilities = identity.transfer_capabilities;
                        return Ok(());
                    }
                    last_error = Some("unexpected hello response".to_string());
                }
                Err(error) if retryable_protocol_decode_error(&error) => {
                    last_error = Some(format!("{error:?}"));
                }
                Err(error) => return Err(format!("invalid hello frame: {error:?}")),
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(format!(
            "firmware did not become ready for protocol commands: {}",
            last_error.unwrap_or_else(|| "no response".to_string())
        ))
    }

    fn serial_transfer_plan(&self, total_len: usize) -> SerialTransferPlan {
        serial_transfer_plan(
            total_len,
            self.transfer_capabilities.max_payload_bytes,
            self.transfer_capabilities.ack_window_bytes,
        )
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

    fn read_protocol_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, String> {
        let deadline = Instant::now() + timeout;
        let mut response = Vec::new();
        let mut buf = [0u8; 512];
        while Instant::now() < deadline {
            match self.port.read(&mut buf) {
                Ok(count) if count > 0 => {
                    response.extend_from_slice(&buf[..count]);
                    if let Some(end) = complete_frame_end_from_stream(&response) {
                        response.truncate(end);
                        break;
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("serial read failed: {error}")),
            }
        }
        Ok(response)
    }
}

fn format_decode_error(error: DecodeError, response: &[u8]) -> String {
    if env::var("SQUID_SERIAL_DUMP_RESPONSE").ok().as_deref() == Some("1") {
        format!(
            "invalid protocol response frame: {error:?}; response_len={} response_hex={}",
            response.len(),
            hex_string(response)
        )
    } else {
        format!("invalid protocol response frame: {error:?}")
    }
}

fn complete_frame_end_from_stream(bytes: &[u8]) -> Option<usize> {
    let start = bytes
        .windows(MAGIC.len())
        .position(|window| window == MAGIC)?;
    if bytes.len() - start < HEADER_LEN {
        return None;
    }
    let payload_len = u32::from_le_bytes(
        bytes[start + 12..start + 16]
            .try_into()
            .expect("slice length checked"),
    ) as usize;
    let end = start.checked_add(HEADER_LEN)?.checked_add(payload_len)?;
    (bytes.len() >= end).then_some(end)
}

#[cfg(test)]
fn max_transfer_chunk_size() -> usize {
    let chunk_size = max_transfer_chunk_size_for_frame_budget(FIRMWARE_SERIAL_FRAME_BUDGET);
    assert!(
        chunk_size > 0,
        "firmware serial frame budget is too small for transfer chunks"
    );
    chunk_size
}

#[cfg(test)]
fn max_transfer_chunk_size_for_frame_budget(frame_budget: usize) -> usize {
    let mut chunk_size = 0usize;
    for candidate in 1..=frame_budget {
        let frame = squid_device_protocol::app_install_chunk_request(11, 0, vec![0; candidate]);
        match squid_device_protocol::encoded_frame_len(&frame) {
            Ok(len) if len <= frame_budget => chunk_size = candidate,
            _ => break,
        }
    }
    chunk_size
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SerialTransferPlan {
    chunk_size: usize,
    chunks: Vec<SerialTransferChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SerialTransferChunk {
    offset: usize,
    len: usize,
    ack_requested: bool,
}

pub fn content_install_progress_line(
    name: &str,
    received: usize,
    total: usize,
    elapsed: Duration,
) -> String {
    let percent = if total == 0 {
        100.0
    } else {
        (received as f64 / total as f64) * 100.0
    };
    let seconds = elapsed.as_secs_f64().max(0.001);
    let bytes_per_second = received as f64 / seconds;
    let remaining = total.saturating_sub(received) as f64;
    let eta_seconds = if bytes_per_second > 0.0 {
        (remaining / bytes_per_second).ceil() as u64
    } else {
        0
    };
    format!(
        "content {name} {percent:.1}% {}/{} {}/s eta {}",
        format_bytes(received as f64),
        format_bytes(total as f64),
        format_bytes(bytes_per_second),
        format_duration_seconds(eta_seconds)
    )
}

fn format_bytes(bytes: f64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;

    if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn format_duration_seconds(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn serial_transfer_plan(
    total_len: usize,
    max_payload_bytes: usize,
    ack_window_bytes: usize,
) -> SerialTransferPlan {
    let chunk_size = max_payload_bytes.max(1);
    let ack_window_bytes = ack_window_bytes.max(chunk_size);
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    let mut bytes_since_ack = 0usize;
    while offset < total_len {
        let len = chunk_size.min(total_len - offset);
        bytes_since_ack += len;
        let end = offset + len == total_len;
        let ack_requested = bytes_since_ack >= ack_window_bytes || end;
        chunks.push(SerialTransferChunk {
            offset,
            len,
            ack_requested,
        });
        if ack_requested {
            bytes_since_ack = 0;
        }
        offset += len;
    }
    SerialTransferPlan { chunk_size, chunks }
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

fn is_safe_content_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && name.len() < squid_device_protocol::MAX_PATH_LEN
}

fn configure_tty(port: &str) -> Result<(), String> {
    let status = Command::new("stty")
        .args(["-F", port])
        .args(configure_tty_args())
        .status()
        .map_err(|error| format!("failed to run stty for {port}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to configure {port} with stty"))
    }
}

fn configure_tty_args() -> [&'static str; 8] {
    ["raw", "-echo", "min", "0", "time", "1", "-hupcl", "clocal"]
}

fn retryable_protocol_decode_error(error: &DecodeError) -> bool {
    matches!(
        error,
        DecodeError::BadMagic | DecodeError::TruncatedHeader | DecodeError::LengthMismatch { .. }
    )
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

pub fn format_raw_lines(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
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
    use std::time::Duration;

    use super::{
        complete_frame_end_from_stream, configure_tty_args, content_install_progress_line,
        format_lines, format_raw_lines, max_transfer_chunk_size,
        max_transfer_chunk_size_for_frame_budget, retryable_protocol_decode_error,
        serial_transfer_plan, OutputTail, FIRMWARE_SERIAL_FRAME_BUDGET,
    };
    use squid_device_protocol::{
        app_install_begin_request, app_install_chunk_request, content_install_begin_request,
        encoded_frame_len, resource_install_begin_request, resource_install_chunk_request,
        temp_run_chunk_request, DecodeError, TransferCapabilities, MAX_APP_ID_LEN, MAX_PATH_LEN,
    };

    #[test]
    fn formats_drawlog_lines_without_adding_a_second_draw_prefix() {
        let lines = vec![
            "draw=clear color=0".to_string(),
            "draw=text text=\"Hello\" x=10 y=20".to_string(),
        ];

        assert_eq!(
            format_raw_lines(&lines),
            "draw=clear color=0\ndraw=text text=\"Hello\" x=10 y=20\n"
        );
    }

    #[test]
    fn formats_other_line_responses_with_command_prefix() {
        let lines = vec!["ready".to_string(), "tick".to_string()];

        assert_eq!(
            format_lines("output", &lines),
            "output=ready\noutput=tick\n"
        );
    }

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
    fn detects_complete_protocol_frame_after_serial_noise() {
        let frame = squid_device_protocol::encode_frame(&squid_device_protocol::hello_request(7));
        let mut stream = b"boot log before response\n".to_vec();
        stream.extend_from_slice(&frame);
        stream.extend_from_slice(b"trailing log");

        assert_eq!(
            complete_frame_end_from_stream(&stream),
            Some(b"boot log before response\n".len() + frame.len())
        );
    }

    #[test]
    fn waits_for_complete_protocol_frame_after_serial_noise() {
        let frame = squid_device_protocol::encode_frame(&squid_device_protocol::hello_request(7));
        let mut partial = b"wifi log before response\n".to_vec();
        partial.extend_from_slice(&frame[..frame.len() - 1]);

        assert_eq!(complete_frame_end_from_stream(&partial), None);
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

    #[test]
    fn transfer_chunk_size_uses_current_firmware_frame_budget() {
        assert_eq!(FIRMWARE_SERIAL_FRAME_BUDGET, 4096);
        let chunk_size = max_transfer_chunk_size();

        assert!(chunk_size > 3900);
        assert_transfer_chunk_fits(|bytes| app_install_chunk_request(11, 0, bytes), chunk_size);
        assert_transfer_chunk_fits(
            |bytes| resource_install_chunk_request(51, 0, bytes),
            chunk_size,
        );
        assert_transfer_chunk_fits(|bytes| temp_run_chunk_request(31, 0, bytes), chunk_size);

        let too_large = app_install_chunk_request(11, 0, vec![0; chunk_size + 1]);
        assert!(encoded_frame_len(&too_large).unwrap() > FIRMWARE_SERIAL_FRAME_BUDGET);
    }

    #[test]
    fn current_firmware_frame_budget_fits_largest_transfer_begin_requests() {
        let max_app_id = "a".repeat(MAX_APP_ID_LEN);
        let max_resource_path = "r".repeat(MAX_PATH_LEN);

        let app_begin = app_install_begin_request(10, max_app_id.as_str(), u64::MAX, u64::MAX);
        assert!(encoded_frame_len(&app_begin).unwrap() <= FIRMWARE_SERIAL_FRAME_BUDGET);

        let resource_begin = resource_install_begin_request(
            50,
            max_app_id.as_str(),
            max_resource_path.as_str(),
            u64::MAX,
            u64::MAX,
        );
        assert!(encoded_frame_len(&resource_begin).unwrap() <= FIRMWARE_SERIAL_FRAME_BUDGET);

        let max_content_name = format!("{}.binbook", "b".repeat(80));
        let content_begin =
            content_install_begin_request(88, max_content_name.as_str(), u64::MAX, u64::MAX);
        assert!(encoded_frame_len(&content_begin).unwrap() <= FIRMWARE_SERIAL_FRAME_BUDGET);
    }

    #[test]
    fn tty_configuration_prevents_hangup_reset_between_protocol_commands() {
        let args = configure_tty_args();

        assert!(args.contains(&"-hupcl"));
        assert!(args.contains(&"clocal"));
    }

    #[test]
    fn retries_when_serial_response_contains_no_protocol_frame() {
        assert!(retryable_protocol_decode_error(&DecodeError::BadMagic));
        assert!(retryable_protocol_decode_error(
            &DecodeError::LengthMismatch {
                expected: 32,
                actual: 12,
            }
        ));
        assert!(retryable_protocol_decode_error(
            &DecodeError::TruncatedHeader
        ));
        assert!(!retryable_protocol_decode_error(&DecodeError::PayloadCrc));
    }

    #[test]
    fn transfer_chunk_size_tracks_smaller_frame_budgets() {
        let small_budget = 128;
        let chunk_size = max_transfer_chunk_size_for_frame_budget(small_budget);

        assert!(chunk_size > 0);
        assert!(chunk_size < max_transfer_chunk_size());
        let frame = app_install_chunk_request(11, 0, vec![0; chunk_size]);
        assert!(encoded_frame_len(&frame).unwrap() <= small_budget);
    }

    #[test]
    fn default_serial_transfer_plan_acks_each_max_size_chunk() {
        let caps = TransferCapabilities::default_serial();
        let plan = serial_transfer_plan(
            3 * caps.max_payload_bytes,
            caps.max_payload_bytes,
            caps.ack_window_bytes,
        );

        assert_eq!(plan.chunks.len(), 3);
        assert!(plan.chunks.iter().all(|chunk| chunk.ack_requested));
    }

    #[test]
    fn transfer_plan_batches_acknowledgements_by_window() {
        let plan = serial_transfer_plan(12 * 1024, 4096, 16 * 1024);

        assert_eq!(plan.chunk_size, 4096);
        assert_eq!(plan.chunks.len(), 3);
        assert!(!plan.chunks[0].ack_requested);
        assert!(!plan.chunks[1].ack_requested);
        assert!(plan.chunks[2].ack_requested);
    }

    #[test]
    fn content_install_progress_line_reports_percent_speed_and_eta() {
        let line = content_install_progress_line(
            "book.binbook",
            512 * 1024,
            2 * 1024 * 1024,
            Duration::from_secs(4),
        );

        assert!(line.contains("content book.binbook"));
        assert!(line.contains("25.0%"));
        assert!(line.contains("512.0 KiB/2.0 MiB"));
        assert!(line.contains("128.0 KiB/s"));
        assert!(line.contains("eta 12s"));
    }

    fn assert_transfer_chunk_fits(
        build: impl Fn(Vec<u8>) -> squid_device_protocol::Frame,
        len: usize,
    ) {
        let frame = build(vec![0; len]);
        assert!(encoded_frame_len(&frame).unwrap() <= FIRMWARE_SERIAL_FRAME_BUDGET);
    }
}
