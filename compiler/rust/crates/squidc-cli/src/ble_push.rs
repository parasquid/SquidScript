use std::{env, fs, path::Path, time::Duration};

use btleplug::{
    api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType},
    platform::{Adapter, Manager, Peripheral},
};
use futures::StreamExt;
use uuid::Uuid;

pub const SVC_UUID: &str = "7e57c0de-0001-4a5b-8c6d-0123456789ab";
pub const CTRL_UUID: &str = "7e57c0de-0002-4a5b-8c6d-0123456789ab";
pub const DATA_UUID: &str = "7e57c0de-0003-4a5b-8c6d-0123456789ab";
pub const STAT_UUID: &str = "7e57c0de-0004-4a5b-8c6d-0123456789ab";

pub const OP_BEGIN: u8 = 0x01;
pub const OP_NAME: u8 = 0x02;
pub const OP_ABORT: u8 = 0x03;
pub const STATUS_COMPLETE: u8 = 0x00;
#[cfg(test)]
pub const STATUS_ERROR: u8 = 0x01;
pub const STATUS_ROUTE_AMBIGUOUS: u8 = 0x11;
pub const STATUS_PENDING: u8 = 0x7f;

const DEFAULT_CHUNK: usize = 180;
const MAX_CHUNK: usize = DEFAULT_CHUNK;
const NAME_CHUNK: usize = 18;
const CONNECT_ATTEMPTS: usize = 3;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(1500);
#[cfg(test)]
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);
const CHARACTERISTIC_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const CHARACTERISTIC_DISCOVERY_POLL: Duration = Duration::from_millis(100);
const SCAN_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlePushResult {
    pub extension: String,
    pub bytes_sent: usize,
}

#[cfg(test)]
pub trait BleTransferClient {
    fn find_device(&mut self, selector: &str) -> Result<(), String>;
    fn ensure_service(&mut self, service_uuid: &str) -> Result<(), String>;
    fn mtu(&self) -> Option<usize>;
    fn start_notify(&mut self, status_uuid: &str) -> Result<(), String>;
    fn stop_notify(&mut self, status_uuid: &str) -> Result<(), String>;
    fn write_control(&mut self, data: &[u8]) -> Result<(), String>;
    fn write_data(&mut self, data: &[u8]) -> Result<(), String>;
    fn wait_status(&mut self, timeout: Duration) -> Result<u8, String>;
}

pub fn push_sqbc(selector: &str, source: &Path) -> Result<BlePushResult, String> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to start BLE runtime: {error}"))?;
    runtime.block_on(push_sqbc_async(selector, source))
}

pub fn push_file(selector: &str, source: &Path, name: &str) -> Result<BlePushResult, String> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to start BLE runtime: {error}"))?;
    runtime.block_on(push_file_async(selector, source, name))
}

#[cfg(test)]
pub fn push_sqbc_with_client<C: BleTransferClient>(
    client: &mut C,
    selector: &str,
    source: &Path,
) -> Result<BlePushResult, String> {
    if source.extension().and_then(|value| value.to_str()) != Some("sqbc") {
        return Err("BLE app push currently accepts only .sqbc files".to_string());
    }
    push_file_with_client(client, selector, source, ".sqbc")
}

#[cfg(test)]
pub fn push_file_with_client<C: BleTransferClient>(
    client: &mut C,
    selector: &str,
    source: &Path,
    name: &str,
) -> Result<BlePushResult, String> {
    validate_ble_file_name(name)?;
    let payload = fs::read(source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
    push_payload_with_client(client, selector, &payload, name)
}

#[cfg(test)]
fn push_payload_with_client<C: BleTransferClient>(
    client: &mut C,
    selector: &str,
    payload: &[u8],
    name: &str,
) -> Result<BlePushResult, String> {
    let file_name = name.as_bytes();
    let file_size = u32::try_from(payload.len())
        .map_err(|_| "payload is too large for BLE file transfer".to_string())?;

    client.find_device(selector)?;
    client.ensure_service(SVC_UUID)?;
    client.start_notify(STAT_UUID)?;

    let mut begin = Vec::with_capacity(7);
    begin.push(OP_BEGIN);
    begin.extend_from_slice(&file_size.to_le_bytes());
    begin.extend_from_slice(&(file_name.len() as u16).to_le_bytes());
    client.write_control(&begin)?;

    for chunk in file_name.chunks(NAME_CHUNK) {
        let mut name = Vec::with_capacity(chunk.len() + 1);
        name.push(OP_NAME);
        name.extend_from_slice(chunk);
        client.write_control(&name)?;
    }

    let chunk_size = client
        .mtu()
        .and_then(|mtu| mtu.checked_sub(3))
        .filter(|size| *size > 0)
        .map(|size| size.min(MAX_CHUNK))
        .unwrap_or(DEFAULT_CHUNK);
    for chunk in payload.chunks(chunk_size) {
        client.write_data(chunk)?;
    }

    match client.wait_status(COMPLETION_TIMEOUT) {
        Ok(STATUS_COMPLETE) => {
            client.stop_notify(STAT_UUID)?;
            Ok(BlePushResult {
                extension: name.to_string(),
                bytes_sent: payload.len(),
            })
        }
        Ok(status) => {
            let _ = client.stop_notify(STAT_UUID);
            Err(ble_status_error(status))
        }
        Err(error) => {
            let _ = client.write_control(&[OP_ABORT]);
            let _ = client.stop_notify(STAT_UUID);
            Err(error)
        }
    }
}

async fn push_sqbc_async(selector: &str, source: &Path) -> Result<BlePushResult, String> {
    if source.extension().and_then(|value| value.to_str()) != Some("sqbc") {
        return Err("BLE app push currently accepts only .sqbc files".to_string());
    }
    push_file_async(selector, source, ".sqbc").await
}

async fn push_file_async(
    selector: &str,
    source: &Path,
    name: &str,
) -> Result<BlePushResult, String> {
    validate_ble_file_name(name)?;
    let payload = fs::read(source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
    let file_size = u32::try_from(payload.len())
        .map_err(|_| format!("{} is too large for BLE file transfer", source.display()))?;

    let adapter = first_adapter().await?;
    let mut last_error = None;
    for attempt in 1..=CONNECT_ATTEMPTS {
        let peripheral = find_peripheral(&adapter, selector).await?;
        if let Err(error) = connect_peripheral(&peripheral).await {
            let _ = peripheral.disconnect().await;
            if attempt == CONNECT_ATTEMPTS || !ble_connect_error_is_retryable(&error) {
                return Err(error);
            }
            last_error = Some(error);
            tokio::time::sleep(CONNECT_RETRY_DELAY).await;
            continue;
        }
        let result = push_connected_peripheral(&peripheral, &payload, file_size, name).await;
        let _ = peripheral.disconnect().await;
        match result {
            Ok(result) => return Ok(result),
            Err(error) => {
                if attempt == CONNECT_ATTEMPTS || !ble_transfer_error_is_retryable(&error) {
                    return Err(error);
                }
                last_error = Some(error);
                tokio::time::sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "BLE transfer failed".to_string()))
}

async fn first_adapter() -> Result<Adapter, String> {
    let manager = Manager::new()
        .await
        .map_err(|error| format!("failed to initialize BLE manager: {error}"))?;
    let adapters = manager
        .adapters()
        .await
        .map_err(|error| format!("failed to list BLE adapters: {error}"))?;
    adapters
        .into_iter()
        .next()
        .ok_or_else(|| "no Bluetooth adapter found".to_string())
}

async fn find_peripheral(adapter: &Adapter, selector: &str) -> Result<Peripheral, String> {
    let service_uuid = parse_uuid(SVC_UUID)?;
    adapter
        .start_scan(ScanFilter {
            services: vec![service_uuid],
        })
        .await
        .map_err(|error| format!("failed to start BLE scan: {error}"))?;

    let deadline = tokio::time::Instant::now() + SCAN_TIMEOUT;
    loop {
        let peripherals = adapter
            .peripherals()
            .await
            .map_err(|error| format!("failed to list BLE peripherals: {error}"))?;
        for peripheral in peripherals {
            let properties = match peripheral.properties().await {
                Ok(properties) => properties,
                Err(error) if ble_property_error_is_stale_object(&error.to_string()) => continue,
                Err(error) => {
                    let _ = adapter.stop_scan().await;
                    return Err(format!("failed to read BLE peripheral properties: {error}"));
                }
            };
            let address = peripheral.address().to_string();
            let local_name = properties
                .as_ref()
                .and_then(|properties| properties.local_name.as_deref());
            if ble_selector_matches(&address, local_name, selector) {
                adapter
                    .stop_scan()
                    .await
                    .map_err(|error| format!("failed to stop BLE scan before connect: {error}"))?;
                return Ok(peripheral);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    adapter
        .stop_scan()
        .await
        .map_err(|error| format!("failed to stop BLE scan: {error}"))?;
    Err("BLE device not found".to_string())
}

async fn connect_peripheral(peripheral: &Peripheral) -> Result<(), String> {
    let mut last_error = None;

    for attempt in 1..=CONNECT_ATTEMPTS {
        match peripheral.connect().await {
            Ok(()) => return Ok(()),
            Err(error) => {
                let message = error.to_string();
                if attempt == CONNECT_ATTEMPTS || !ble_connect_error_is_retryable(&message) {
                    return Err(format!("failed to connect to BLE device: {message}"));
                }
                last_error = Some(message);
                let _ = peripheral.disconnect().await;
                tokio::time::sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(format!(
        "failed to connect to BLE device: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn ble_connect_error_is_retryable(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("le-connection-abort-by-local")
        || lower.contains("connection abort")
        || lower.contains("software caused connection abort")
        || lower.contains("connection timed out")
        || lower.contains("service discovery timed out")
        || lower.contains("already connected")
        || lower.contains("in progress")
}

fn ble_transfer_error_is_retryable(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("service discovery timed out")
        || lower.contains("characteristics did not finish resolving")
}

fn ble_property_error_is_stale_object(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("org.freedesktop.dbus.properties")
        && lower.contains("getall")
        && lower.contains("doesn't exist")
}

fn ble_selector_matches(address: &str, local_name: Option<&str>, selector: &str) -> bool {
    if address.eq_ignore_ascii_case(selector) {
        return true;
    }
    let Some(name) = local_name else {
        return false;
    };
    if name == selector || name.contains(selector) || (selector.contains(name) && name.len() >= 8) {
        return true;
    }
    let truncated_selector = selector
        .char_indices()
        .take_while(|(index, ch)| index + ch.len_utf8() <= 29)
        .map(|(_, ch)| ch)
        .collect::<String>();
    !truncated_selector.is_empty() && name == truncated_selector
}

async fn push_connected_peripheral(
    peripheral: &Peripheral,
    payload: &[u8],
    file_size: u32,
    name: &str,
) -> Result<BlePushResult, String> {
    let svc_uuid = parse_uuid(SVC_UUID)?;
    let ctrl_uuid = parse_uuid(CTRL_UUID)?;
    let data_uuid = parse_uuid(DATA_UUID)?;
    let stat_uuid = parse_uuid(STAT_UUID)?;
    let discovery_deadline = tokio::time::Instant::now() + CHARACTERISTIC_DISCOVERY_TIMEOUT;
    let (control, data, status) = loop {
        peripheral
            .discover_services()
            .await
            .map_err(|error| format!("failed to discover BLE services: {error}"))?;
        let characteristics = peripheral.characteristics();
        let control = characteristics
            .iter()
            .find(|characteristic| characteristic.uuid == ctrl_uuid)
            .cloned();
        let data = characteristics
            .iter()
            .find(|characteristic| characteristic.uuid == data_uuid)
            .cloned();
        let status = characteristics
            .iter()
            .find(|characteristic| characteristic.uuid == stat_uuid)
            .cloned();
        if let (Some(control), Some(data), Some(status)) = (control, data, status) {
            break (control, data, status);
        }
        if tokio::time::Instant::now() >= discovery_deadline {
            if !peripheral
                .services()
                .iter()
                .any(|service| service.uuid == svc_uuid)
            {
                return Err(format!(
                    "file-transfer service {SVC_UUID} not found on device"
                ));
            }
            return Err("file-transfer characteristics did not finish resolving".to_string());
        }
        tokio::time::sleep(CHARACTERISTIC_DISCOVERY_POLL).await;
    };
    let data_write_type = ble_data_write_type(data.properties);
    let mut notifications = peripheral
        .notifications()
        .await
        .map_err(|error| format!("failed to open BLE notification stream: {error}"))?;
    peripheral
        .subscribe(&status)
        .await
        .map_err(|error| format!("failed to subscribe to BLE status notifications: {error}"))?;

    let mut begin = Vec::with_capacity(7);
    begin.push(OP_BEGIN);
    begin.extend_from_slice(&file_size.to_le_bytes());
    begin.extend_from_slice(&(name.len() as u16).to_le_bytes());
    peripheral
        .write(&control, &begin, WriteType::WithResponse)
        .await
        .map_err(|error| format!("failed to send BLE BEGIN: {error}"))?;

    for chunk in name.as_bytes().chunks(NAME_CHUNK) {
        let mut name = Vec::with_capacity(chunk.len() + 1);
        name.push(OP_NAME);
        name.extend_from_slice(chunk);
        peripheral
            .write(&control, &name, WriteType::WithResponse)
            .await
            .map_err(|error| format!("failed to send BLE NAME: {error}"))?;
    }

    let chunk_size = negotiated_chunk_size(peripheral.mtu());
    for (chunk_index, chunk) in payload.chunks(chunk_size).enumerate() {
        let offset = chunk_index.saturating_mul(chunk_size);
        peripheral
            .write(&data, chunk, data_write_type)
            .await
            .map_err(|error| {
                format!(
                    "failed to send BLE data chunk {chunk_index} offset={offset} len={}: {error}",
                    chunk.len()
                )
            })?;
    }

    let mut status_poll = BleStatusPoll::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let result = loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break Ok(BlePushResult {
                extension: name.to_string(),
                bytes_sent: payload.len(),
            });
        }
        let wait = (deadline - now).min(Duration::from_millis(500));
        match tokio::time::timeout(wait, notifications.next()).await {
            Ok(Some(notification)) if notification.uuid == stat_uuid => {
                if let Some(result) = status_poll.observe_value(notification.value) {
                    break result.map(|()| BlePushResult {
                        extension: name.to_string(),
                        bytes_sent: payload.len(),
                    });
                }
            }
            Ok(Some(notification)) => {
                status_poll.observe_other_notification(notification.uuid, notification.value.len())
            }
            Ok(None) => {
                break Ok(BlePushResult {
                    extension: name.to_string(),
                    bytes_sent: payload.len(),
                });
            }
            Err(_) => {}
        }
    };

    if result.is_err() {
        let _ = peripheral
            .write(&control, &[OP_ABORT], WriteType::WithResponse)
            .await;
    }
    let _ = peripheral.unsubscribe(&status).await;
    result
}

fn ble_terminal_status(status: Option<u8>) -> Option<Result<(), String>> {
    match status {
        Some(STATUS_COMPLETE) => Some(Ok(())),
        Some(STATUS_PENDING) | None => None,
        Some(status) => Some(Err(ble_status_error(status))),
    }
}

struct BleStatusPoll {
    debug: bool,
    reads: usize,
}

impl BleStatusPoll {
    fn new() -> Self {
        Self {
            debug: env::var_os("SQUID_BLE_DEBUG_STATUS").is_some(),
            reads: 0,
        }
    }

    #[cfg(test)]
    fn observe_read(&mut self, read: Result<Vec<u8>, String>) -> Option<Result<(), String>> {
        self.reads = self.reads.saturating_add(1);
        match read {
            Ok(value) => self.observe_value_with_label("read", value),
            Err(error) => {
                if self.debug {
                    eprintln!("ble-status-read index={} error={}", self.reads, error);
                }
                None
            }
        }
    }

    fn observe_value(&mut self, value: Vec<u8>) -> Option<Result<(), String>> {
        self.reads = self.reads.saturating_add(1);
        self.observe_value_with_label("notify", value)
    }

    fn observe_other_notification(&mut self, uuid: Uuid, len: usize) {
        self.reads = self.reads.saturating_add(1);
        if self.debug {
            eprintln!(
                "ble-status-notify-other index={} uuid={} len={}",
                self.reads, uuid, len
            );
        }
    }

    fn observe_value_with_label(&self, label: &str, value: Vec<u8>) -> Option<Result<(), String>> {
        let status = value.first().copied();
        if self.debug {
            eprintln!(
                "ble-status-{label} index={} value={:?} len={}",
                self.reads,
                status,
                value.len()
            );
        }
        ble_terminal_status(status)
    }
}

fn validate_ble_file_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > u16::MAX as usize || name.as_bytes().contains(&0) {
        return Err(format!("invalid BLE file name: {name}"));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!("invalid BLE file name: {name}"));
    }
    if !name.starts_with('.') && !name.contains('.') {
        return Err(format!("BLE file name must include an extension: {name}"));
    }
    Ok(())
}

fn negotiated_chunk_size(mtu: u16) -> usize {
    usize::from(mtu)
        .checked_sub(4)
        .filter(|size| *size > 0)
        .map(|size| size.min(MAX_CHUNK))
        .unwrap_or(DEFAULT_CHUNK)
}

fn ble_data_write_type(properties: CharPropFlags) -> WriteType {
    if properties.contains(CharPropFlags::WRITE) {
        WriteType::WithResponse
    } else {
        WriteType::WithoutResponse
    }
}

fn ble_status_error(status: u8) -> String {
    match status {
        STATUS_ROUTE_AMBIGUOUS => {
            format!("device reported BLE route ambiguous status {status}")
        }
        _ => format!("device reported error status {status}"),
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| format!("invalid BLE UUID {value}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::PathBuf};

    struct FakeClient {
        mtu: Option<usize>,
        status: Result<u8, String>,
        writes: Vec<Write>,
        notify_started: bool,
        notify_stopped: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Write {
        Control(Vec<u8>),
        Data(Vec<u8>),
    }

    impl Default for FakeClient {
        fn default() -> Self {
            Self {
                mtu: Some(247),
                status: Ok(STATUS_COMPLETE),
                writes: Vec::new(),
                notify_started: false,
                notify_stopped: false,
            }
        }
    }

    impl BleTransferClient for FakeClient {
        fn find_device(&mut self, selector: &str) -> Result<(), String> {
            assert_eq!(selector, "SquidScript");
            Ok(())
        }

        fn ensure_service(&mut self, service_uuid: &str) -> Result<(), String> {
            assert_eq!(service_uuid, SVC_UUID);
            Ok(())
        }

        fn mtu(&self) -> Option<usize> {
            self.mtu
        }

        fn start_notify(&mut self, status_uuid: &str) -> Result<(), String> {
            assert_eq!(status_uuid, STAT_UUID);
            self.notify_started = true;
            Ok(())
        }

        fn stop_notify(&mut self, status_uuid: &str) -> Result<(), String> {
            assert_eq!(status_uuid, STAT_UUID);
            self.notify_stopped = true;
            Ok(())
        }

        fn write_control(&mut self, data: &[u8]) -> Result<(), String> {
            self.writes.push(Write::Control(data.to_vec()));
            Ok(())
        }

        fn write_data(&mut self, data: &[u8]) -> Result<(), String> {
            self.writes.push(Write::Data(data.to_vec()));
            Ok(())
        }

        fn wait_status(&mut self, _timeout: Duration) -> Result<u8, String> {
            self.status.clone()
        }
    }

    #[test]
    fn ble_selector_matches_exact_address_and_advertised_name() {
        assert!(ble_selector_matches(
            "AA:BB:CC:DD:EE:FF",
            Some("XIAO ESP32-C3 ePaper 4.26 + SD"),
            "aa:bb:cc:dd:ee:ff"
        ));
        assert!(ble_selector_matches(
            "AA:BB:CC:DD:EE:FF",
            Some("XIAO ESP32-C3 ePaper 4.26 + SD"),
            "XIAO ESP32-C3 ePaper 4.26 + SD"
        ));
    }

    #[test]
    fn ble_selector_matches_truncated_or_partial_advertised_name() {
        assert!(ble_selector_matches(
            "AA:BB:CC:DD:EE:FF",
            Some("XIAO ESP32-C3 ePaper 4.26"),
            "XIAO ESP32-C3 ePaper 4.26 + SD"
        ));
        assert!(ble_selector_matches(
            "AA:BB:CC:DD:EE:FF",
            Some("XIAO ESP32-C3 ePaper 4.26 + SD"),
            "ESP32-C3 ePaper"
        ));
    }

    #[test]
    fn ble_selector_rejects_unrelated_devices() {
        assert!(!ble_selector_matches(
            "AA:BB:CC:DD:EE:FF",
            Some("XIAO ESP32-C3 ePaper 4.26 + SD"),
            "Other Device"
        ));
        assert!(!ble_selector_matches(
            "AA:BB:CC:DD:EE:FF",
            None,
            "Other Device"
        ));
    }

    #[test]
    fn ble_connect_retry_classifier_covers_transient_host_abort_errors() {
        assert!(ble_connect_error_is_retryable(
            "le-connection-abort-by-local"
        ));
        assert!(ble_connect_error_is_retryable(
            "Software caused connection abort"
        ));
        assert!(ble_connect_error_is_retryable("Connection timed out"));
        assert!(ble_connect_error_is_retryable(
            "Service discovery timed out"
        ));
        assert!(ble_connect_error_is_retryable("In Progress"));
        assert!(!ble_connect_error_is_retryable("Authentication failed"));
        assert!(!ble_connect_error_is_retryable("device not found"));
    }

    #[test]
    fn ble_transfer_retry_classifier_retries_discovery_but_not_protocol_errors() {
        assert!(ble_transfer_error_is_retryable(
            "Service discovery timed out"
        ));
        assert!(ble_transfer_error_is_retryable(
            "file-transfer characteristics did not finish resolving"
        ));
        assert!(!ble_transfer_error_is_retryable(
            "Operation failed with ATT error: 0x0e"
        ));
        assert!(!ble_transfer_error_is_retryable(
            "failed to subscribe to BLE status notifications: Operation failed with ATT error: 0x0e"
        ));
        assert!(!ble_transfer_error_is_retryable(
            "device reported error status 1"
        ));
    }

    #[test]
    fn ble_property_error_classifier_covers_stale_bluez_objects() {
        assert!(ble_property_error_is_stale_object(
            r#"Method "GetAll" with signature "s" on interface "org.freedesktop.DBus.Properties" doesn't exist"#
        ));
        assert!(!ble_property_error_is_stale_object("permission denied"));
        assert!(!ble_property_error_is_stale_object("adapter powered off"));
    }

    #[test]
    fn push_sqbc_writes_begin_name_chunks_data_and_waits_for_complete() {
        let root = unique_test_dir("squidc-ble-push");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("app.sqbc");
        let payload = [b"SQBC".as_slice(), &[7u8; 600]].concat();
        fs::write(&source, &payload).unwrap();
        let mut client = FakeClient::default();

        let result = push_sqbc_with_client(&mut client, "SquidScript", &source).unwrap();

        assert_eq!(
            result,
            BlePushResult {
                extension: ".sqbc".to_string(),
                bytes_sent: payload.len()
            }
        );
        assert!(client.notify_started);
        assert!(client.notify_stopped);

        let begin = match &client.writes[0] {
            Write::Control(data) => data,
            Write::Data(_) => panic!("expected BEGIN control write"),
        };
        assert_eq!(begin[0], OP_BEGIN);
        assert_eq!(
            u32::from_le_bytes(begin[1..5].try_into().unwrap()) as usize,
            payload.len()
        );
        assert_eq!(u16::from_le_bytes(begin[5..7].try_into().unwrap()), 5);

        let name = client
            .writes
            .iter()
            .filter_map(|write| match write {
                Write::Control(data) if data[0] == OP_NAME => Some(&data[1..]),
                _ => None,
            })
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(name, b".sqbc");

        let data_writes = client
            .writes
            .iter()
            .filter_map(|write| match write {
                Write::Data(data) => Some(data.as_slice()),
                Write::Control(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            data_writes.iter().map(|data| data.len()).sum::<usize>(),
            payload.len()
        );
        assert!(data_writes.iter().all(|data| data.len() <= 244));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn push_file_writes_generic_file_name_and_payload_chunks() {
        let root = unique_test_dir("squidc-ble-file-push");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("transfer-smoke.dat");
        let payload = vec![0x5au8; 1200];
        fs::write(&source, &payload).unwrap();
        let mut client = FakeClient::default();

        let result =
            push_file_with_client(&mut client, "SquidScript", &source, "transfer-smoke.dat")
                .unwrap();

        assert_eq!(
            result,
            BlePushResult {
                extension: "transfer-smoke.dat".to_string(),
                bytes_sent: payload.len()
            }
        );
        let name = client
            .writes
            .iter()
            .filter_map(|write| match write {
                Write::Control(data) if data[0] == OP_NAME => Some(&data[1..]),
                _ => None,
            })
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(name, b"transfer-smoke.dat");
        let data_len = client
            .writes
            .iter()
            .filter_map(|write| match write {
                Write::Data(data) => Some(data.len()),
                Write::Control(_) => None,
            })
            .sum::<usize>();
        assert_eq!(data_len, payload.len());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn push_sqbc_rejects_non_sqbc_input() {
        let root = unique_test_dir("squidc-ble-push-ext");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("app.binbook");
        fs::write(&source, b"book").unwrap();

        let error =
            push_sqbc_with_client(&mut FakeClient::default(), "SquidScript", &source).unwrap_err();

        assert!(error.contains(".sqbc"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn push_sqbc_aborts_when_completion_times_out() {
        let root = unique_test_dir("squidc-ble-push-timeout");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("app.sqbc");
        fs::write(&source, b"SQBCxxxx").unwrap();
        let mut client = FakeClient {
            status: Err("timed out waiting for completion".to_string()),
            ..FakeClient::default()
        };

        let error = push_sqbc_with_client(&mut client, "SquidScript", &source).unwrap_err();

        assert!(error.contains("timed out"));
        assert!(matches!(
            client.writes.last(),
            Some(Write::Control(data)) if data == &[OP_ABORT]
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn push_sqbc_reports_device_error_status() {
        let root = unique_test_dir("squidc-ble-push-error");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("app.sqbc");
        fs::write(&source, b"SQBCxxxx").unwrap();
        let mut client = FakeClient {
            status: Ok(STATUS_ERROR),
            ..FakeClient::default()
        };

        let error = push_sqbc_with_client(&mut client, "SquidScript", &source).unwrap_err();

        assert!(error.contains("device reported error status 1"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn push_sqbc_reports_named_route_ambiguity_status() {
        let root = unique_test_dir("squidc-ble-push-ambiguous-route");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("app.sqbc");
        fs::write(&source, b"SQBCxxxx").unwrap();
        let mut client = FakeClient {
            status: Ok(STATUS_ROUTE_AMBIGUOUS),
            ..FakeClient::default()
        };

        let error = push_sqbc_with_client(&mut client, "SquidScript", &source).unwrap_err();

        assert!(error.contains("BLE route ambiguous"));
        assert!(error.contains("17"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ble_data_write_type_prefers_with_response_when_supported() {
        use btleplug::api::CharPropFlags;

        assert_eq!(
            ble_data_write_type(CharPropFlags::WRITE | CharPropFlags::WRITE_WITHOUT_RESPONSE),
            WriteType::WithResponse
        );
        assert_eq!(
            ble_data_write_type(CharPropFlags::WRITE),
            WriteType::WithResponse
        );
        assert_eq!(
            ble_data_write_type(CharPropFlags::WRITE_WITHOUT_RESPONSE),
            WriteType::WithoutResponse
        );
    }

    #[test]
    fn negotiated_chunk_size_stays_within_ble_put_wire_budget() {
        assert_eq!(negotiated_chunk_size(517), DEFAULT_CHUNK);
        assert_eq!(negotiated_chunk_size(23), 19);
    }

    #[test]
    fn ble_status_poll_ignores_pending_and_completes_on_terminal_success() {
        let mut poll = BleStatusPoll::new();

        assert!(poll.observe_read(Ok(vec![STATUS_PENDING])).is_none());
        assert_eq!(poll.observe_read(Ok(vec![STATUS_COMPLETE])), Some(Ok(())));
    }

    #[test]
    fn ble_status_poll_reports_terminal_error_status() {
        let mut poll = BleStatusPoll::new();

        let result = poll.observe_read(Ok(vec![STATUS_ROUTE_AMBIGUOUS]));

        assert_eq!(
            result,
            Some(Err(
                "device reported BLE route ambiguous status 17".to_string()
            ))
        );
    }

    #[test]
    fn ble_status_poll_treats_empty_and_failed_reads_as_non_terminal() {
        let mut poll = BleStatusPoll::new();

        assert!(poll.observe_read(Ok(Vec::new())).is_none());
        assert!(poll
            .observe_read(Err("dbus transient read failure".to_string()))
            .is_none());
        assert_eq!(poll.observe_read(Ok(vec![STATUS_COMPLETE])), Some(Ok(())));
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }
}
