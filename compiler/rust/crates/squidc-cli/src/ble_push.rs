use std::{fs, path::Path, time::Duration};

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

const DEFAULT_CHUNK: usize = 180;
const MAX_CHUNK: usize = 512;
const NAME_CHUNK: usize = 18;

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

#[cfg(test)]
pub fn push_sqbc_with_client<C: BleTransferClient>(
    client: &mut C,
    selector: &str,
    source: &Path,
) -> Result<BlePushResult, String> {
    if source.extension().and_then(|value| value.to_str()) != Some("sqbc") {
        return Err("BLE app push currently accepts only .sqbc files".to_string());
    }
    let payload = fs::read(source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
    let file_name = b".sqbc";
    let file_size = u32::try_from(payload.len())
        .map_err(|_| format!("{} is too large for BLE file transfer", source.display()))?;

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

    match client.wait_status(Duration::from_secs(30)) {
        Ok(STATUS_COMPLETE) => {
            client.stop_notify(STAT_UUID)?;
            Ok(BlePushResult {
                extension: ".sqbc".to_string(),
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
    let payload = fs::read(source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
    let file_size = u32::try_from(payload.len())
        .map_err(|_| format!("{} is too large for BLE file transfer", source.display()))?;

    let adapter = first_adapter().await?;
    let peripheral = find_peripheral(&adapter, selector).await?;
    peripheral
        .connect()
        .await
        .map_err(|error| format!("failed to connect to BLE device: {error}"))?;
    let result = push_connected_peripheral(&peripheral, &payload, file_size).await;
    let _ = peripheral.disconnect().await;
    result
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
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(|error| format!("failed to start BLE scan: {error}"))?;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let peripherals = adapter
        .peripherals()
        .await
        .map_err(|error| format!("failed to list BLE peripherals: {error}"))?;

    for peripheral in peripherals {
        let address = peripheral.address().to_string();
        let local_name = peripheral
            .properties()
            .await
            .map_err(|error| format!("failed to read BLE peripheral properties: {error}"))?
            .and_then(|properties| properties.local_name);
        if ble_selector_matches(&address, local_name.as_deref(), selector) {
            return Ok(peripheral);
        }
    }
    Err("BLE device not found".to_string())
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
) -> Result<BlePushResult, String> {
    let svc_uuid = parse_uuid(SVC_UUID)?;
    let ctrl_uuid = parse_uuid(CTRL_UUID)?;
    let data_uuid = parse_uuid(DATA_UUID)?;
    let stat_uuid = parse_uuid(STAT_UUID)?;

    peripheral
        .discover_services()
        .await
        .map_err(|error| format!("failed to discover BLE services: {error}"))?;
    if !peripheral
        .services()
        .iter()
        .any(|service| service.uuid == svc_uuid)
    {
        return Err(format!(
            "file-transfer service {SVC_UUID} not found on device"
        ));
    }

    let control = peripheral
        .characteristics()
        .into_iter()
        .find(|characteristic| characteristic.uuid == ctrl_uuid)
        .ok_or_else(|| format!("control characteristic {CTRL_UUID} not found on device"))?;
    let data = peripheral
        .characteristics()
        .into_iter()
        .find(|characteristic| characteristic.uuid == data_uuid)
        .ok_or_else(|| format!("data characteristic {DATA_UUID} not found on device"))?;
    let data_write_type = ble_data_write_type(data.properties);
    let status = peripheral
        .characteristics()
        .into_iter()
        .find(|characteristic| characteristic.uuid == stat_uuid)
        .ok_or_else(|| format!("status characteristic {STAT_UUID} not found on device"))?;

    let mut notifications = peripheral
        .notifications()
        .await
        .map_err(|error| format!("failed to open BLE notifications: {error}"))?;
    peripheral
        .subscribe(&status)
        .await
        .map_err(|error| format!("failed to subscribe to BLE status notifications: {error}"))?;

    let mut begin = Vec::with_capacity(7);
    begin.push(OP_BEGIN);
    begin.extend_from_slice(&file_size.to_le_bytes());
    begin.extend_from_slice(&(b".sqbc".len() as u16).to_le_bytes());
    peripheral
        .write(&control, &begin, WriteType::WithResponse)
        .await
        .map_err(|error| format!("failed to send BLE BEGIN: {error}"))?;

    for chunk in b".sqbc".chunks(NAME_CHUNK) {
        let mut name = Vec::with_capacity(chunk.len() + 1);
        name.push(OP_NAME);
        name.extend_from_slice(chunk);
        peripheral
            .write(&control, &name, WriteType::WithResponse)
            .await
            .map_err(|error| format!("failed to send BLE NAME: {error}"))?;
    }

    let chunk_size = negotiated_chunk_size(peripheral.mtu());
    for chunk in payload.chunks(chunk_size) {
        peripheral
            .write(&data, chunk, data_write_type)
            .await
            .map_err(|error| format!("failed to send BLE data chunk: {error}"))?;
    }

    let status_result = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(notification) = notifications.next().await {
            if notification.uuid == stat_uuid {
                return notification.value.first().copied();
            }
        }
        None
    })
    .await;

    let _ = peripheral.unsubscribe(&status).await;

    match status_result {
        Ok(Some(STATUS_COMPLETE)) => Ok(BlePushResult {
            extension: ".sqbc".to_string(),
            bytes_sent: payload.len(),
        }),
        Ok(Some(status)) => Err(ble_status_error(status)),
        Ok(None) => Err("BLE status notification stream ended before completion".to_string()),
        Err(_) => {
            let _ = peripheral
                .write(&control, &[OP_ABORT], WriteType::WithResponse)
                .await;
            Err("timed out waiting for completion".to_string())
        }
    }
}

fn negotiated_chunk_size(mtu: u16) -> usize {
    usize::from(mtu)
        .checked_sub(3)
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
