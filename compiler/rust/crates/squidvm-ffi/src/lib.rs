#![cfg_attr(feature = "zephyr", no_std)]

use core::{
    ffi::c_void,
    fmt::{self, Write},
    mem::{align_of, size_of, MaybeUninit},
    ptr, slice, str,
};

#[cfg(feature = "zephyr")]
use core::panic::PanicInfo;

use squidvm_core::{
    error::VmError,
    host::{
        AppArmedStack, AppArmedStackEntry, AppProcessStack, AppRegistryEntry, AppRegistryList,
        ContentPickFileResult, ContentReadLinesResult, ContentReadTextResult, DeviceConfigResult,
        DisplayLineOptions, DisplayRectOptions, DisplayResourceOptions, DisplayTextOptions,
        StorageCompletion as CoreStorageCompletion, StorageRequest, TraceSink, VmDispatch,
        WifiAccessPoint, WifiActionResult, WifiApIp, WifiScanResult, WifiStatus,
        MAX_STORAGE_TRANSFER_BYTES,
    },
    limits::{MAX_APP_BYTES, MAX_CODE_CHUNK_BYTES, MAX_SAVED_STATE_BYTES},
    program::ProgramIndex,
    reader::{SliceSqbcReader, SqbcReader},
    strings::StringResolver,
    value::Value,
    vm::ChunkedVm,
};

use squid_device_protocol::{
    encode_app_list_response_into, encode_empty_response_into, encode_error_response_into,
    encode_hello_response_into, encode_lifecycle_response_into, encode_line_response_into,
    encode_resources_response_into, encode_state_response_into, key_event_from_request_into,
    AppListEntry, DecodeError, DeviceRequest, LifecycleTimer, Opcode, ResourceMetric,
    Status as SqdpFrameStatus, MAX_APP_BYTES as SQDP_MAX_APP_BYTES,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqvmStatus {
    Ok = 0,
    InvalidArgument = 1,
    VmError = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqdpStatus {
    Ok = 0,
    InvalidArgument = 1,
    BufferTooSmall = 2,
    EncodeError = 3,
}

const SQDP_APP_ID_CAP: usize = 40;
const SQDP_PATH_CAP: usize = 80;
pub const SQVM_DEVICE_BINDING_NAME_CAP: usize = 32;
pub const SQVM_DEVICE_BINDING_RESOURCE_CAP: usize = 128;
pub const SQDP_STAGING_PATH_CAP: usize = 80;
pub const SQDC_CONFIG_MAX_RECORDS: usize = 6;
pub const SQDC_CONFIG_KEY_CAP: usize = 32;
pub const SQDC_CONFIG_STRING_CAP: usize = 48;

const SQDC_MAGIC: &[u8; 4] = b"SQDC";
const SQDEVICE_HEADER: &[u8] = b"SQDEVICE";
const SQDC_TAG_NULL: u8 = 0;
const SQDC_TAG_BOOL: u8 = 1;
const SQDC_TAG_I32: u8 = 2;
const SQDC_TAG_STRING: u8 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqdcStatus {
    Ok = 0,
    InvalidArgument = 1,
    BufferTooSmall = 2,
    ParseError = 3,
    TooManyRecords = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqdcValueKind {
    Null = 0,
    Bool = 1,
    I32 = 2,
    String = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdcValue {
    pub kind: SqdcValueKind,
    pub bool_value: bool,
    pub i32_value: i32,
    pub string: [u8; SQDC_CONFIG_STRING_CAP],
    pub string_len: usize,
}

impl Default for SqdcValue {
    fn default() -> Self {
        Self {
            kind: SqdcValueKind::Null,
            bool_value: false,
            i32_value: 0,
            string: [0; SQDC_CONFIG_STRING_CAP],
            string_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdcRecord {
    pub present: bool,
    pub key: [u8; SQDC_CONFIG_KEY_CAP],
    pub key_len: usize,
    pub value: SqdcValue,
}

impl Default for SqdcRecord {
    fn default() -> Self {
        Self {
            present: false,
            key: [0; SQDC_CONFIG_KEY_CAP],
            key_len: 0,
            value: SqdcValue::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdcConfig {
    pub records: [SqdcRecord; SQDC_CONFIG_MAX_RECORDS],
    pub count: usize,
}

impl Default for SqdcConfig {
    fn default() -> Self {
        Self {
            records: [SqdcRecord::default(); SQDC_CONFIG_MAX_RECORDS],
            count: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqdcDeviceBindingResourceKind {
    Unsupported = 0,
    PackageSqdevice = 1,
    InlineGpio = 2,
    InlineGpioButton = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdcDeviceBindingPlan {
    pub kind: SqdcDeviceBindingResourceKind,
    pub alias: [u8; SQVM_DEVICE_BINDING_NAME_CAP],
    pub alias_len: usize,
    pub resource: [u8; SQVM_DEVICE_BINDING_RESOURCE_CAP],
    pub resource_len: usize,
}

impl Default for SqdcDeviceBindingPlan {
    fn default() -> Self {
        Self {
            kind: SqdcDeviceBindingResourceKind::Unsupported,
            alias: [0; SQVM_DEVICE_BINDING_NAME_CAP],
            alias_len: 0,
            resource: [0; SQVM_DEVICE_BINDING_RESOURCE_CAP],
            resource_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdpAppListEntry {
    pub app_id: [u8; SQDP_APP_ID_CAP],
    pub sqbc_len: usize,
}

impl Default for SqdpAppListEntry {
    fn default() -> Self {
        Self {
            app_id: [0; SQDP_APP_ID_CAP],
            sqbc_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdpLineSlice {
    pub bytes: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdpResourceMetric {
    pub key: *const u8,
    pub key_len: usize,
    pub value: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqdpLifecycleTimer {
    pub app_id: [u8; SQDP_APP_ID_CAP],
    pub event: [u8; 32],
}

impl Default for SqdpLifecycleTimer {
    fn default() -> Self {
        Self {
            app_id: [0; SQDP_APP_ID_CAP],
            event: [0; 32],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqdpActionKind {
    None = 0,
    BeginInstall = 1,
    WriteInstallChunk = 2,
    CommitInstall = 3,
    BeginTempRun = 4,
    WriteTempRunChunk = 5,
    CommitTempRun = 6,
    BeginResourceInstall = 7,
    WriteResourceChunk = 8,
    CommitResourceInstall = 9,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SqdpAction {
    pub kind: SqdpActionKind,
    pub app_id: *const u8,
    pub app_id_len: usize,
    pub resource_path: *const u8,
    pub resource_path_len: usize,
    pub staging_path: *const u8,
    pub staging_path_len: usize,
    pub offset: usize,
    pub bytes: *const u8,
    pub bytes_len: usize,
    pub total_len: usize,
}

impl Default for SqdpAction {
    fn default() -> Self {
        Self {
            kind: SqdpActionKind::None,
            app_id: ptr::null(),
            app_id_len: 0,
            resource_path: ptr::null(),
            resource_path_len: 0,
            staging_path: ptr::null(),
            staging_path_len: 0,
            offset: 0,
            bytes: ptr::null(),
            bytes_len: 0,
            total_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SqdpWifiProfile {
    pub profile: *const u8,
    pub profile_len: usize,
    pub ssid: *const u8,
    pub ssid_len: usize,
    pub password: *const u8,
    pub password_len: usize,
}

impl Default for SqdpWifiProfile {
    fn default() -> Self {
        Self {
            profile: ptr::null(),
            profile_len: 0,
            ssid: ptr::null(),
            ssid_len: 0,
            password: ptr::null(),
            password_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SqdpStateImport {
    pub bytes: *const u8,
    pub bytes_len: usize,
}

impl Default for SqdpStateImport {
    fn default() -> Self {
        Self {
            bytes: ptr::null(),
            bytes_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SqdpAppLaunch {
    pub app_id: *const u8,
    pub app_id_len: usize,
}

impl Default for SqdpAppLaunch {
    fn default() -> Self {
        Self {
            app_id: ptr::null(),
            app_id_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SqdpEventDispatch {
    pub app_id: *const u8,
    pub app_id_len: usize,
    pub event: *const u8,
    pub event_len: usize,
}

impl Default for SqdpEventDispatch {
    fn default() -> Self {
        Self {
            app_id: ptr::null(),
            app_id_len: 0,
            event: ptr::null(),
            event_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmTriggerTimer {
    pub event: [u8; 32],
    pub interval_ms: i32,
    pub repeating: bool,
}

impl Default for SqvmTriggerTimer {
    fn default() -> Self {
        Self {
            event: [0; 32],
            interval_ms: 0,
            repeating: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SqdpTransferSession {
    pub active: bool,
    pub app_id: [u8; SQDP_APP_ID_CAP],
    pub total_len: usize,
    pub received: usize,
    pub expected_crc: u32,
    pub running_crc: u32,
    pub staging_path: [u8; SQDP_STAGING_PATH_CAP],
}

impl Default for SqdpTransferSession {
    fn default() -> Self {
        Self {
            active: false,
            app_id: [0; SQDP_APP_ID_CAP],
            total_len: 0,
            received: 0,
            expected_crc: 0,
            running_crc: 0xffff_ffff,
            staging_path: [0; SQDP_STAGING_PATH_CAP],
        }
    }
}

impl SqdpTransferSession {
    pub fn app_id_string(&self) -> &str {
        str::from_utf8(c_string_bytes(&self.app_id)).unwrap_or("")
    }

    pub fn set_staging_path_for_test(&mut self, path: &str) -> SqdpStatus {
        set_c_string(&mut self.staging_path, path.as_bytes())
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SqdpResourceSession {
    pub active: bool,
    pub app_id: [u8; SQDP_APP_ID_CAP],
    pub resource_path: [u8; SQDP_PATH_CAP],
    pub total_len: usize,
    pub received: usize,
    pub expected_crc: u32,
    pub running_crc: u32,
    pub staging_path: [u8; SQDP_STAGING_PATH_CAP],
}

impl Default for SqdpResourceSession {
    fn default() -> Self {
        Self {
            active: false,
            app_id: [0; SQDP_APP_ID_CAP],
            resource_path: [0; SQDP_PATH_CAP],
            total_len: 0,
            received: 0,
            expected_crc: 0,
            running_crc: 0xffff_ffff,
            staging_path: [0; SQDP_STAGING_PATH_CAP],
        }
    }
}

impl SqdpResourceSession {
    pub fn app_id_string(&self) -> &str {
        str::from_utf8(c_string_bytes(&self.app_id)).unwrap_or("")
    }

    pub fn resource_path_string(&self) -> &str {
        str::from_utf8(c_string_bytes(&self.resource_path)).unwrap_or("")
    }

    pub fn set_staging_path_for_test(&mut self, path: &str) -> SqdpStatus {
        set_c_string(&mut self.staging_path, path.as_bytes())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqvmDispatchOutcome {
    Complete = 0,
    PendingStorage = 1,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqvmStorageRequestKind {
    None = 0,
    SqbcRead = 1,
    StateLoad = 2,
    StateSave = 3,
    StateReset = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmStorageRequest {
    pub kind: SqvmStorageRequestKind,
    pub offset: usize,
    pub len: usize,
    pub bytes: [u8; MAX_STORAGE_TRANSFER_BYTES],
}

impl Default for SqvmStorageRequest {
    fn default() -> Self {
        Self {
            kind: SqvmStorageRequestKind::None,
            offset: 0,
            len: 0,
            bytes: [0; MAX_STORAGE_TRANSFER_BYTES],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmStorageCompletion {
    pub has_len: bool,
    pub len: usize,
    pub bytes: [u8; MAX_STORAGE_TRANSFER_BYTES],
}

impl Default for SqvmStorageCompletion {
    fn default() -> Self {
        Self {
            has_len: false,
            len: 0,
            bytes: [0; MAX_STORAGE_TRANSFER_BYTES],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDispatchResult {
    pub status: SqvmStatus,
    pub outcome: SqvmDispatchOutcome,
    pub exited: bool,
    pub storage: SqvmStorageRequest,
}

impl Default for SqvmDispatchResult {
    fn default() -> Self {
        Self {
            status: SqvmStatus::Ok,
            outcome: SqvmDispatchOutcome::Complete,
            exited: false,
            storage: SqvmStorageRequest::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDisplayTextOptions {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub font_height: i32,
    pub text_color: *const u8,
    pub text_color_len: usize,
    pub background_color: *const u8,
    pub background_color_len: usize,
    pub align: *const u8,
    pub align_len: usize,
    pub valign: *const u8,
    pub valign_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDisplayRectOptions {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub fill_color: *const u8,
    pub fill_color_len: usize,
    pub stroke_color: *const u8,
    pub stroke_color_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDisplayLineOptions {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub color: *const u8,
    pub color_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDisplayResourceOptions {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmAppRegistryEntry {
    pub id: *const u8,
    pub id_len: usize,
    pub name: *const u8,
    pub name_len: usize,
    pub build: *const u8,
    pub build_len: usize,
    pub description: *const u8,
    pub description_len: usize,
}

impl Default for SqvmAppRegistryEntry {
    fn default() -> Self {
        Self {
            id: ptr::null(),
            id_len: 0,
            name: ptr::null(),
            name_len: 0,
            build: ptr::null(),
            build_len: 0,
            description: ptr::null(),
            description_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmAppStackEntry {
    pub app_id: *const u8,
    pub app_id_len: usize,
    pub event: *const u8,
    pub event_len: usize,
}

impl Default for SqvmAppStackEntry {
    fn default() -> Self {
        Self {
            app_id: ptr::null(),
            app_id_len: 0,
            event: ptr::null(),
            event_len: 0,
        }
    }
}

pub const SQVM_WIFI_SCAN_MAX_NETWORKS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmWifiStatus {
    pub active: bool,
    pub mode: *const u8,
    pub mode_len: usize,
    pub ip_address: *const u8,
    pub ip_address_len: usize,
    pub ssid: *const u8,
    pub ssid_len: usize,
    pub clients: i32,
    pub error: *const u8,
    pub error_len: usize,
    pub state: *const u8,
    pub state_len: usize,
    pub backend: *const u8,
    pub backend_len: usize,
    pub driver_started: bool,
    pub configured: bool,
    pub driver_mode: *const u8,
    pub driver_mode_len: usize,
    pub channel: i32,
    pub ap_start_events: i32,
    pub ap_stop_events: i32,
    pub probe_events: i32,
    pub sta_connected_events: i32,
    pub sta_disconnected_events: i32,
    pub last_backend_code: *const u8,
    pub last_backend_code_len: usize,
    pub profile: *const u8,
    pub profile_len: usize,
    pub connected: bool,
    pub scan_matches: i32,
    pub rssi: i32,
    pub auth: *const u8,
    pub auth_len: usize,
    pub bssid: *const u8,
    pub bssid_len: usize,
    pub disconnect_reason: *const u8,
    pub disconnect_reason_len: usize,
    pub disconnect_reason_code: i32,
}

impl Default for SqvmWifiStatus {
    fn default() -> Self {
        Self {
            active: false,
            mode: ptr::null(),
            mode_len: 0,
            ip_address: ptr::null(),
            ip_address_len: 0,
            ssid: ptr::null(),
            ssid_len: 0,
            clients: 0,
            error: ptr::null(),
            error_len: 0,
            state: b"stopped".as_ptr(),
            state_len: b"stopped".len(),
            backend: b"zephyr".as_ptr(),
            backend_len: b"zephyr".len(),
            driver_started: false,
            configured: false,
            driver_mode: ptr::null(),
            driver_mode_len: 0,
            channel: 0,
            ap_start_events: 0,
            ap_stop_events: 0,
            probe_events: 0,
            sta_connected_events: 0,
            sta_disconnected_events: 0,
            last_backend_code: ptr::null(),
            last_backend_code_len: 0,
            profile: ptr::null(),
            profile_len: 0,
            connected: false,
            scan_matches: 0,
            rssi: 0,
            auth: ptr::null(),
            auth_len: 0,
            bssid: ptr::null(),
            bssid_len: 0,
            disconnect_reason: ptr::null(),
            disconnect_reason_len: 0,
            disconnect_reason_code: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmWifiAccessPoint {
    pub ssid: *const u8,
    pub ssid_len: usize,
    pub bssid: *const u8,
    pub bssid_len: usize,
    pub ssid_length: i32,
    pub channel: i32,
    pub rssi: i32,
    pub auth: *const u8,
    pub auth_len: usize,
    pub hidden: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmWifiScanResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub networks: *const SqvmWifiAccessPoint,
    pub network_count: usize,
}

impl Default for SqvmWifiScanResult {
    fn default() -> Self {
        Self {
            ok: false,
            error: b"unsupported".as_ptr(),
            error_len: b"unsupported".len(),
            networks: ptr::null(),
            network_count: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmWifiActionResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
}

impl Default for SqvmWifiActionResult {
    fn default() -> Self {
        Self {
            ok: false,
            error: b"unsupported".as_ptr(),
            error_len: b"unsupported".len(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqvmDeviceConfigValueKind {
    Null = 0,
    Bool = 1,
    I32 = 2,
    String = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDeviceConfigValue {
    pub kind: SqvmDeviceConfigValueKind,
    pub bool_value: bool,
    pub i32_value: i32,
    pub string: *const u8,
    pub string_len: usize,
}

impl Default for SqvmDeviceConfigValue {
    fn default() -> Self {
        Self {
            kind: SqvmDeviceConfigValueKind::Null,
            bool_value: false,
            i32_value: 0,
            string: ptr::null(),
            string_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDeviceConfigResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub warning: *const u8,
    pub warning_len: usize,
}

impl Default for SqvmDeviceConfigResult {
    fn default() -> Self {
        Self {
            ok: false,
            error: b"unsupported".as_ptr(),
            error_len: b"unsupported".len(),
            warning: ptr::null(),
            warning_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmContentPickFileResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub path: *const u8,
    pub path_len: usize,
}

impl Default for SqvmContentPickFileResult {
    fn default() -> Self {
        Self {
            ok: false,
            error: b"unsupported".as_ptr(),
            error_len: b"unsupported".len(),
            path: ptr::null(),
            path_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmContentReadTextResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub text: *const u8,
    pub text_len: usize,
}

impl Default for SqvmContentReadTextResult {
    fn default() -> Self {
        Self {
            ok: false,
            error: b"unsupported".as_ptr(),
            error_len: b"unsupported".len(),
            text: ptr::null(),
            text_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmContentReadLinesResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
}

impl Default for SqvmContentReadLinesResult {
    fn default() -> Self {
        Self {
            ok: false,
            error: b"unsupported".as_ptr(),
            error_len: b"unsupported".len(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmWifiApIp {
    pub ip: *const u8,
    pub ip_len: usize,
    pub gw: *const u8,
    pub gw_len: usize,
    pub netmask: *const u8,
    pub netmask_len: usize,
    pub error: *const u8,
    pub error_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDeviceBinding {
    pub service: [u8; SQVM_DEVICE_BINDING_NAME_CAP],
    pub binding: [u8; SQVM_DEVICE_BINDING_NAME_CAP],
    pub resource: [u8; SQVM_DEVICE_BINDING_RESOURCE_CAP],
}

impl Default for SqvmDeviceBinding {
    fn default() -> Self {
        Self {
            service: [0; SQVM_DEVICE_BINDING_NAME_CAP],
            binding: [0; SQVM_DEVICE_BINDING_NAME_CAP],
            resource: [0; SQVM_DEVICE_BINDING_RESOURCE_CAP],
        }
    }
}

impl Default for SqvmWifiApIp {
    fn default() -> Self {
        Self {
            ip: ptr::null(),
            ip_len: 0,
            gw: ptr::null(),
            gw_len: 0,
            netmask: ptr::null(),
            netmask_len: 0,
            error: b"unsupported".as_ptr(),
            error_len: b"unsupported".len(),
        }
    }
}

pub type SqvmReadExactAtCallback = Option<
    unsafe extern "C" fn(
        user_data: *mut c_void,
        offset: usize,
        out: *mut u8,
        out_len: usize,
    ) -> i32,
>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SqvmCallbacks {
    pub user_data: *mut c_void,
    pub trace: Option<
        unsafe extern "C" fn(user_data: *mut c_void, message: *const u8, message_len: usize),
    >,
    pub read_exact_at: SqvmReadExactAtCallback,
    pub debug_output: Option<
        unsafe extern "C" fn(user_data: *mut c_void, message: *const u8, message_len: usize),
    >,
    pub display_clear:
        Option<unsafe extern "C" fn(user_data: *mut c_void, color: *const u8, color_len: usize)>,
    pub display_text: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            text: *const u8,
            text_len: usize,
            options: *const SqvmDisplayTextOptions,
        ),
    >,
    pub display_rect: Option<
        unsafe extern "C" fn(user_data: *mut c_void, options: *const SqvmDisplayRectOptions),
    >,
    pub display_line: Option<
        unsafe extern "C" fn(user_data: *mut c_void, options: *const SqvmDisplayLineOptions),
    >,
    pub display_select: Option<
        unsafe extern "C" fn(user_data: *mut c_void, name: *const u8, name_len: usize) -> i32,
    >,
    pub display_image: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            path: *const u8,
            path_len: usize,
            options: *const SqvmDisplayResourceOptions,
        ),
    >,
    pub display_draw: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            drawable: *const u8,
            drawable_len: usize,
            options: *const SqvmDisplayResourceOptions,
        ),
    >,
    pub indicator_write: Option<unsafe extern "C" fn(user_data: *mut c_void, value: bool) -> i32>,
    pub indicator_toggle: Option<unsafe extern "C" fn(user_data: *mut c_void) -> i32>,
    pub indicator_read: Option<unsafe extern "C" fn(user_data: *mut c_void, out: *mut bool) -> i32>,
    pub indicator_breathe: Option<unsafe extern "C" fn(user_data: *mut c_void) -> i32>,
    pub indicator_blink:
        Option<unsafe extern "C" fn(user_data: *mut c_void, on_ms: i32, off_ms: i32) -> i32>,
    pub hardware_gpio_write: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            name: *const u8,
            name_len: usize,
            value: bool,
        ) -> i32,
    >,
    pub hardware_gpio_toggle: Option<
        unsafe extern "C" fn(user_data: *mut c_void, name: *const u8, name_len: usize) -> i32,
    >,
    pub hardware_gpio_read: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            name: *const u8,
            name_len: usize,
            out: *mut bool,
        ) -> i32,
    >,
    pub app_launch:
        Option<unsafe extern "C" fn(user_data: *mut c_void, app: *const u8, app_len: usize) -> i32>,
    pub app_arm:
        Option<unsafe extern "C" fn(user_data: *mut c_void, app: *const u8, app_len: usize) -> i32>,
    pub app_disarm:
        Option<unsafe extern "C" fn(user_data: *mut c_void, app: *const u8, app_len: usize) -> i32>,
    pub app_registry_list: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            out: *mut SqvmAppRegistryEntry,
            out_cap: usize,
            out_count: *mut usize,
        ) -> i32,
    >,
    pub app_registry_get: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            app: *const u8,
            app_len: usize,
            out: *mut SqvmAppRegistryEntry,
        ) -> i32,
    >,
    pub app_process_stack: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            out: *mut SqvmAppStackEntry,
            out_cap: usize,
            out_count: *mut usize,
        ) -> i32,
    >,
    pub app_armed_stack: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            out: *mut SqvmAppStackEntry,
            out_cap: usize,
            out_count: *mut usize,
        ) -> i32,
    >,
    pub timer_every: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            event: *const u8,
            event_len: usize,
            interval_ms: i32,
        ) -> i32,
    >,
    pub timer_after: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            event: *const u8,
            event_len: usize,
            delay_ms: i32,
        ) -> i32,
    >,
    pub wifi_start_ap: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            ssid: *const u8,
            ssid_len: usize,
            out: *mut SqvmWifiActionResult,
        ) -> i32,
    >,
    pub wifi_stop_ap:
        Option<unsafe extern "C" fn(user_data: *mut c_void, out: *mut SqvmWifiActionResult) -> i32>,
    pub wifi_connect: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            profile: *const u8,
            profile_len: usize,
            out: *mut SqvmWifiActionResult,
        ) -> i32,
    >,
    pub wifi_disconnect:
        Option<unsafe extern "C" fn(user_data: *mut c_void, out: *mut SqvmWifiActionResult) -> i32>,
    pub wifi_get_ap_ip:
        Option<unsafe extern "C" fn(user_data: *mut c_void, out: *mut SqvmWifiApIp) -> i32>,
    pub wifi_status:
        Option<unsafe extern "C" fn(user_data: *mut c_void, out: *mut SqvmWifiStatus) -> i32>,
    pub wifi_scan:
        Option<unsafe extern "C" fn(user_data: *mut c_void, out: *mut SqvmWifiScanResult) -> i32>,
    pub device_config_load: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            source: *const u8,
            source_len: usize,
            out: *mut SqvmDeviceConfigResult,
        ) -> i32,
    >,
    pub device_config_set: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            key: *const u8,
            key_len: usize,
            value: SqvmDeviceConfigValue,
            out: *mut SqvmDeviceConfigResult,
        ) -> i32,
    >,
    pub device_config_rebind: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            alias: *const u8,
            alias_len: usize,
            out: *mut SqvmDeviceConfigResult,
        ) -> i32,
    >,
    pub device_config_save: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            destination: *const u8,
            destination_len: usize,
            out: *mut SqvmDeviceConfigResult,
        ) -> i32,
    >,
    pub content_pick_file: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            extension: *const u8,
            extension_len: usize,
            out: *mut SqvmContentPickFileResult,
        ) -> i32,
    >,
    pub content_read_text: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            path: *const u8,
            path_len: usize,
            out: *mut SqvmContentReadTextResult,
        ) -> i32,
    >,
    pub content_read_lines: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            path: *const u8,
            path_len: usize,
            max_lines: i32,
            out: *mut SqvmContentReadLinesResult,
        ) -> i32,
    >,
    pub system_memory_text: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            out: *mut u8,
            out_cap: usize,
            out_len: *mut usize,
        ) -> i32,
    >,
    pub system_storage_text: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            name: *const u8,
            name_len: usize,
            out: *mut u8,
            out_cap: usize,
            out_len: *mut usize,
        ) -> i32,
    >,
}

impl Default for SqvmCallbacks {
    fn default() -> Self {
        Self {
            user_data: ptr::null_mut(),
            trace: None,
            read_exact_at: None,
            debug_output: None,
            display_clear: None,
            display_text: None,
            display_rect: None,
            display_line: None,
            display_select: None,
            display_image: None,
            display_draw: None,
            indicator_write: None,
            indicator_toggle: None,
            indicator_read: None,
            indicator_breathe: None,
            indicator_blink: None,
            hardware_gpio_write: None,
            hardware_gpio_toggle: None,
            hardware_gpio_read: None,
            app_launch: None,
            app_arm: None,
            app_disarm: None,
            app_registry_list: None,
            app_registry_get: None,
            app_process_stack: None,
            app_armed_stack: None,
            timer_every: None,
            timer_after: None,
            wifi_start_ap: None,
            wifi_stop_ap: None,
            wifi_connect: None,
            wifi_disconnect: None,
            wifi_get_ap_ip: None,
            wifi_status: None,
            wifi_scan: None,
            device_config_load: None,
            device_config_set: None,
            device_config_rebind: None,
            device_config_save: None,
            content_pick_file: None,
            content_read_text: None,
            content_read_lines: None,
            system_memory_text: None,
            system_storage_text: None,
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_config_clear(config: *mut SqdcConfig) -> SqdcStatus {
    if config.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    *config = SqdcConfig::default();
    SqdcStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_is_safe_sqdevice_path(
    path: *const u8,
    path_len: usize,
) -> SqdcStatus {
    let Some(path) = ffi_bytes(path, path_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if is_safe_sqdevice_path_bytes(path) {
        SqdcStatus::Ok
    } else {
        SqdcStatus::InvalidArgument
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_parse_sqdevice(
    input: *const u8,
    input_len: usize,
    out: *mut SqdcConfig,
) -> SqdcStatus {
    let Some(input) = ffi_bytes(input, input_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if out.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    let mut parsed = SqdcConfig::default();
    match parse_sqdevice_bytes(input, &mut parsed) {
        SqdcStatus::Ok => {
            *out = parsed;
            SqdcStatus::Ok
        }
        status => status,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_config_set_null(
    config: *mut SqdcConfig,
    key: *const u8,
    key_len: usize,
) -> SqdcStatus {
    let Some(key) = ffi_bytes(key, key_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if config.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    config_set_value(&mut *config, key, SqdcValue::default())
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_config_set_bool(
    config: *mut SqdcConfig,
    key: *const u8,
    key_len: usize,
    value: bool,
) -> SqdcStatus {
    let Some(key) = ffi_bytes(key, key_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if config.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    config_set_value(
        &mut *config,
        key,
        SqdcValue {
            kind: SqdcValueKind::Bool,
            bool_value: value,
            ..SqdcValue::default()
        },
    )
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_config_set_i32(
    config: *mut SqdcConfig,
    key: *const u8,
    key_len: usize,
    value: i32,
) -> SqdcStatus {
    let Some(key) = ffi_bytes(key, key_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if config.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    config_set_value(
        &mut *config,
        key,
        SqdcValue {
            kind: SqdcValueKind::I32,
            i32_value: value,
            ..SqdcValue::default()
        },
    )
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_config_set_string(
    config: *mut SqdcConfig,
    key: *const u8,
    key_len: usize,
    value: *const u8,
    value_len: usize,
) -> SqdcStatus {
    let Some(key) = ffi_bytes(key, key_len) else {
        return SqdcStatus::InvalidArgument;
    };
    let Some(value) = ffi_bytes(value, value_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if config.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    if value.len() > SQDC_CONFIG_STRING_CAP || str::from_utf8(value).is_err() {
        return SqdcStatus::BufferTooSmall;
    }
    let mut stored = SqdcValue {
        kind: SqdcValueKind::String,
        string_len: value.len(),
        ..SqdcValue::default()
    };
    stored.string[..value.len()].copy_from_slice(value);
    config_set_value(&mut *config, key, stored)
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_encode_sqdc(
    config: *const SqdcConfig,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdcStatus {
    if config.is_null() || out_len.is_null() || out.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    *out_len = 0;
    let out = slice::from_raw_parts_mut(out, out_cap);
    encode_sqdc_bytes(&*config, out, &mut *out_len)
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_decode_sqdc(
    input: *const u8,
    input_len: usize,
    out: *mut SqdcConfig,
) -> SqdcStatus {
    let Some(input) = ffi_bytes(input, input_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if out.is_null() {
        return SqdcStatus::InvalidArgument;
    }
    let mut decoded = SqdcConfig::default();
    match decode_sqdc_bytes(input, &mut decoded) {
        SqdcStatus::Ok => {
            *out = decoded;
            SqdcStatus::Ok
        }
        status => status,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdc_plan_device_binding(
    service: *const u8,
    service_len: usize,
    binding: *const u8,
    binding_len: usize,
    resource: *const u8,
    resource_len: usize,
    out: *mut SqdcDeviceBindingPlan,
    out_inline_config: *mut SqdcConfig,
) -> SqdcStatus {
    let Some(service) = ffi_bytes(service, service_len) else {
        return SqdcStatus::InvalidArgument;
    };
    let Some(binding) = ffi_bytes(binding, binding_len) else {
        return SqdcStatus::InvalidArgument;
    };
    let Some(resource) = ffi_bytes(resource, resource_len) else {
        return SqdcStatus::InvalidArgument;
    };
    if out.is_null() {
        return SqdcStatus::InvalidArgument;
    }

    let inline_config = if out_inline_config.is_null() {
        None
    } else {
        Some(&mut *out_inline_config)
    };
    plan_device_binding_bytes(service, binding, resource, &mut *out, inline_config)
}

#[repr(C)]
pub struct SqvmContext {
    initialized: bool,
    vm_words: [MaybeUninit<usize>; SQVM_CONTEXT_WORDS],
}

impl Drop for SqvmContext {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                self.vm_ptr().drop_in_place();
            }
        }
    }
}

const SQVM_CONTEXT_WORDS: usize =
    (size_of::<ChunkedVm>() + size_of::<usize>() - 1) / size_of::<usize>();

const _: [(); 1] = [(); (align_of::<ChunkedVm>() <= align_of::<usize>()) as usize];

impl SqvmContext {
    fn vm_ptr(&mut self) -> *mut ChunkedVm {
        self.vm_words.as_mut_ptr().cast::<ChunkedVm>()
    }
}

#[no_mangle]
pub extern "C" fn sqvm_context_size() -> usize {
    size_of::<SqvmContext>()
}

#[no_mangle]
pub extern "C" fn sqvm_context_align() -> usize {
    align_of::<SqvmContext>()
}

#[no_mangle]
pub extern "C" fn sqvm_storage_transfer_capacity() -> usize {
    MAX_STORAGE_TRANSFER_BYTES
}

#[no_mangle]
pub extern "C" fn sqvm_saved_state_capacity() -> usize {
    MAX_SAVED_STATE_BYTES
}

pub fn sqvm_context_init() -> SqvmContext {
    const UNINIT: MaybeUninit<usize> = MaybeUninit::uninit();
    SqvmContext {
        initialized: false,
        vm_words: [UNINIT; SQVM_CONTEXT_WORDS],
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_context_prepare(context: *mut u8, context_len: usize) -> SqvmStatus {
    if context.is_null() || context_len < size_of::<SqvmContext>() {
        return SqvmStatus::InvalidArgument;
    }
    ptr::addr_of_mut!((*context.cast::<SqvmContext>()).initialized).write(false);
    SqvmStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_context_init_in_place(
    context: *mut SqvmContext,
    callbacks: SqvmCallbacks,
    scratch: *mut u8,
    scratch_len: usize,
) -> SqvmStatus {
    if context.is_null() || scratch.is_null() || scratch_len < MAX_CODE_CHUNK_BYTES {
        return SqvmStatus::InvalidArgument;
    }

    let context = &mut *context;
    if context.initialized {
        context.vm_ptr().drop_in_place();
        context.initialized = false;
    }

    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    let mut host = FfiHost::new(callbacks, false);
    match ChunkedVm::init_in_place_from_reader(context.vm_ptr(), &mut host, scratch) {
        Ok(()) => {
            context.initialized = true;
            SqvmStatus::Ok
        }
        Err(_) => SqvmStatus::VmError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_timer_count(
    sqbc: *const u8,
    sqbc_len: usize,
    out_count: *mut usize,
) -> SqvmStatus {
    if sqbc.is_null() || out_count.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let mut scratch = [0u8; MAX_APP_BYTES];
    let mut reader = SliceSqbcReader::new(slice::from_raw_parts(sqbc, sqbc_len));
    trigger_timer_count_from_reader(&mut reader, &mut scratch, out_count)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_timer_read(
    sqbc: *const u8,
    sqbc_len: usize,
    index: usize,
    out_timer: *mut SqvmTriggerTimer,
) -> SqvmStatus {
    if sqbc.is_null() || out_timer.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let mut scratch = [0u8; MAX_APP_BYTES];
    let mut reader = SliceSqbcReader::new(slice::from_raw_parts(sqbc, sqbc_len));
    trigger_timer_read_from_reader(&mut reader, &mut scratch, index, out_timer)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_timer_count_from_reader(
    user_data: *mut c_void,
    read_exact_at: SqvmReadExactAtCallback,
    scratch: *mut u8,
    scratch_len: usize,
    out_count: *mut usize,
) -> SqvmStatus {
    if read_exact_at.is_none() || scratch.is_null() || out_count.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let callbacks = SqvmCallbacks {
        user_data,
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    trigger_timer_count_from_reader(&mut reader, scratch, out_count)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_timer_read_from_reader(
    user_data: *mut c_void,
    read_exact_at: SqvmReadExactAtCallback,
    scratch: *mut u8,
    scratch_len: usize,
    index: usize,
    out_timer: *mut SqvmTriggerTimer,
) -> SqvmStatus {
    if read_exact_at.is_none() || scratch.is_null() || out_timer.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let callbacks = SqvmCallbacks {
        user_data,
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    trigger_timer_read_from_reader(&mut reader, scratch, index, out_timer)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_device_binding_count_from_reader(
    user_data: *mut c_void,
    read_exact_at: SqvmReadExactAtCallback,
    scratch: *mut u8,
    scratch_len: usize,
    out_count: *mut usize,
) -> SqvmStatus {
    if read_exact_at.is_none() || scratch.is_null() || out_count.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let callbacks = SqvmCallbacks {
        user_data,
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    device_binding_count_from_reader(&mut reader, scratch, out_count)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_device_binding_read_from_reader(
    user_data: *mut c_void,
    read_exact_at: SqvmReadExactAtCallback,
    scratch: *mut u8,
    scratch_len: usize,
    index: usize,
    out_binding: *mut SqvmDeviceBinding,
) -> SqvmStatus {
    if read_exact_at.is_none() || scratch.is_null() || out_binding.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let callbacks = SqvmCallbacks {
        user_data,
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    device_binding_read_from_reader(&mut reader, scratch, index, out_binding)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_dispatch(
    context: *mut SqvmContext,
    callbacks: SqvmCallbacks,
    event: *const u8,
    event_len: usize,
) -> SqvmStatus {
    if context.is_null() || event.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let context = &mut *context;
    if !context.initialized {
        return SqvmStatus::InvalidArgument;
    }
    let Ok(event) = str::from_utf8(slice::from_raw_parts(event, event_len)) else {
        return SqvmStatus::InvalidArgument;
    };
    let vm = &mut *context.vm_ptr();
    let mut host = FfiHost::new(callbacks, false);
    status_from_vm(vm.dispatch(&mut host, event))
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_dispatch_start_resumable(
    context: *mut SqvmContext,
    callbacks: SqvmCallbacks,
    event: *const u8,
    event_len: usize,
    out_result: *mut SqvmDispatchResult,
) -> SqvmStatus {
    if context.is_null() || event.is_null() || out_result.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let context = &mut *context;
    if !context.initialized {
        return SqvmStatus::InvalidArgument;
    }
    let Ok(event) = str::from_utf8(slice::from_raw_parts(event, event_len)) else {
        return SqvmStatus::InvalidArgument;
    };
    let vm = &mut *context.vm_ptr();
    let mut host = FfiHost::new(callbacks, true);
    let result = vm.dispatch_resumable(&mut host, event);
    write_dispatch_result(out_result, vm.exited(), result)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_dispatch_resume_storage(
    context: *mut SqvmContext,
    callbacks: SqvmCallbacks,
    completion: *const SqvmStorageCompletion,
    out_result: *mut SqvmDispatchResult,
) -> SqvmStatus {
    if context.is_null() || completion.is_null() || out_result.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let context = &mut *context;
    if !context.initialized {
        return SqvmStatus::InvalidArgument;
    }
    let Ok(completion) = core_storage_completion(&*completion) else {
        return SqvmStatus::InvalidArgument;
    };
    let vm = &mut *context.vm_ptr();
    let mut host = FfiHost::new(callbacks, true);
    let result = vm.resume_storage(&mut host, completion);
    write_dispatch_result(out_result, vm.exited(), result)
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_empty_response(
    opcode: u8,
    status: u8,
    sequence: u32,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null() || out_len.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let Ok(opcode) = Opcode::try_from(opcode) else {
        return SqdpStatus::InvalidArgument;
    };
    let Ok(status) = SqdpFrameStatus::try_from(status) else {
        return SqdpStatus::InvalidArgument;
    };
    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_empty_response_into(opcode, status, sequence, out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_hello_response(
    opcode: u8,
    sequence: u32,
    target: *const u8,
    target_len: usize,
    firmware: *const u8,
    firmware_len: usize,
    diagnostic: bool,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if target.is_null() || firmware.is_null() || out.is_null() || out_len.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let Ok(opcode) = Opcode::try_from(opcode) else {
        return SqdpStatus::InvalidArgument;
    };
    let Ok(target) = str::from_utf8(slice::from_raw_parts(target, target_len)) else {
        return SqdpStatus::InvalidArgument;
    };
    let Ok(firmware) = str::from_utf8(slice::from_raw_parts(firmware, firmware_len)) else {
        return SqdpStatus::InvalidArgument;
    };
    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_hello_response_into(opcode, sequence, target, firmware, diagnostic, out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_error_response(
    opcode: u8,
    sequence: u32,
    code: i64,
    message: *const u8,
    message_len: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if message.is_null() || out.is_null() || out_len.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let Ok(opcode) = Opcode::try_from(opcode) else {
        return SqdpStatus::InvalidArgument;
    };
    let Ok(message) = str::from_utf8(slice::from_raw_parts(message, message_len)) else {
        return SqdpStatus::InvalidArgument;
    };
    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_error_response_into(opcode, sequence, code, message, out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

fn protocol_error_message_for_code(code: i64) -> &'static str {
    match code {
        -95 => "unsupported",
        -19 => "device unavailable",
        -22 => "invalid request",
        -16 => "busy",
        _ => "command failed",
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_error_response_for_code(
    opcode: u8,
    sequence: u32,
    code: i64,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null() || out_len.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let Ok(opcode) = Opcode::try_from(opcode) else {
        return SqdpStatus::InvalidArgument;
    };
    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_error_response_into(
        opcode,
        sequence,
        code,
        protocol_error_message_for_code(code),
        out,
    ) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_app_list_response(
    sequence: u32,
    entries: *const SqdpAppListEntry,
    entry_count: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null() || out_len.is_null() || (entries.is_null() && entry_count > 0) {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let raw_entries = if entry_count == 0 {
        &[]
    } else {
        slice::from_raw_parts(entries, entry_count)
    };
    if raw_entries.len() > 16 {
        return SqdpStatus::InvalidArgument;
    }
    for entry in raw_entries {
        if str::from_utf8(c_string_bytes(&entry.app_id)).is_err() {
            return SqdpStatus::InvalidArgument;
        }
    }

    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_app_list_response_into(
        sequence,
        raw_entries.iter().map(|entry| AppListEntry {
            app_id: str::from_utf8(c_string_bytes(&entry.app_id))
                .expect("validated app id utf-8 before encoding"),
            sqbc_len: entry.sqbc_len as u64,
        }),
        out,
    ) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_line_response(
    opcode: u8,
    sequence: u32,
    fixed_lines: *const u8,
    fixed_count: usize,
    fixed_stride: usize,
    extra_lines: *const SqdpLineSlice,
    extra_count: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null()
        || out_len.is_null()
        || (fixed_lines.is_null() && fixed_count > 0)
        || (extra_lines.is_null() && extra_count > 0)
        || (fixed_count > 0 && fixed_stride == 0)
    {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let Ok(opcode) = Opcode::try_from(opcode) else {
        return SqdpStatus::InvalidArgument;
    };
    if fixed_count > 32 || extra_count > 8 {
        return SqdpStatus::InvalidArgument;
    }
    let fixed = if fixed_count == 0 {
        &[]
    } else {
        slice::from_raw_parts(fixed_lines, fixed_count.saturating_mul(fixed_stride))
    };
    let extra = if extra_count == 0 {
        &[]
    } else {
        slice::from_raw_parts(extra_lines, extra_count)
    };
    for index in 0..fixed_count {
        let line = fixed_line_bytes(fixed, index, fixed_stride);
        if str::from_utf8(line).is_err() {
            return SqdpStatus::InvalidArgument;
        }
    }
    for line in extra {
        if line.bytes.is_null()
            || str::from_utf8(slice::from_raw_parts(line.bytes, line.len)).is_err()
        {
            return SqdpStatus::InvalidArgument;
        }
    }

    let out = slice::from_raw_parts_mut(out, out_cap);
    let fixed_iter = (0..fixed_count).map(|index| {
        str::from_utf8(fixed_line_bytes(fixed, index, fixed_stride))
            .expect("validated fixed line utf-8 before encoding")
    });
    let extra_iter = extra.iter().map(|line| {
        str::from_utf8(slice::from_raw_parts(line.bytes, line.len))
            .expect("validated extra line utf-8 before encoding")
    });
    match encode_line_response_into(opcode, sequence, fixed_iter.chain(extra_iter), out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_lifecycle_response(
    sequence: u32,
    active_app: *const u8,
    active_app_len: usize,
    process_stack: *const u8,
    process_count: usize,
    process_stride: usize,
    armed_timers: *const SqdpLifecycleTimer,
    armed_count: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null()
        || out_len.is_null()
        || (active_app.is_null() && active_app_len > 0)
        || (process_stack.is_null() && process_count > 0)
        || (process_count > 0 && process_stride == 0)
        || (armed_timers.is_null() && armed_count > 0)
    {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    if process_count > 8 || armed_count > 8 {
        return SqdpStatus::InvalidArgument;
    }
    let active = if active_app_len == 0 {
        None
    } else {
        match str::from_utf8(slice::from_raw_parts(active_app, active_app_len)) {
            Ok(value) => Some(value),
            Err(_) => return SqdpStatus::InvalidArgument,
        }
    };
    let process = if process_count == 0 {
        &[]
    } else {
        slice::from_raw_parts(process_stack, process_count.saturating_mul(process_stride))
    };
    let armed = if armed_count == 0 {
        &[]
    } else {
        slice::from_raw_parts(armed_timers, armed_count)
    };
    for index in 0..process_count {
        if str::from_utf8(fixed_line_bytes(process, index, process_stride)).is_err() {
            return SqdpStatus::InvalidArgument;
        }
    }
    for timer in armed {
        if str::from_utf8(c_string_bytes(&timer.app_id)).is_err()
            || str::from_utf8(c_string_bytes(&timer.event)).is_err()
        {
            return SqdpStatus::InvalidArgument;
        }
    }

    let process_iter = (0..process_count).map(|index| {
        str::from_utf8(fixed_line_bytes(process, index, process_stride))
            .expect("validated process stack utf-8 before encoding")
    });
    let armed_iter = armed.iter().map(|timer| LifecycleTimer {
        app_id: str::from_utf8(c_string_bytes(&timer.app_id))
            .expect("validated armed app id utf-8 before encoding"),
        event: str::from_utf8(c_string_bytes(&timer.event))
            .expect("validated armed event utf-8 before encoding"),
    });
    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_lifecycle_response_into(sequence, active, process_iter, armed_iter, out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_resources_response(
    sequence: u32,
    metrics: *const SqdpResourceMetric,
    metric_count: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null() || out_len.is_null() || (metrics.is_null() && metric_count > 0) {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    if metric_count > 32 {
        return SqdpStatus::InvalidArgument;
    }
    let metrics = if metric_count == 0 {
        &[]
    } else {
        slice::from_raw_parts(metrics, metric_count)
    };
    for metric in metrics {
        if metric.key.is_null()
            || metric.key_len == 0
            || str::from_utf8(slice::from_raw_parts(metric.key, metric.key_len)).is_err()
        {
            return SqdpStatus::InvalidArgument;
        }
    }

    let metric_iter = metrics.iter().map(|metric| ResourceMetric {
        key: str::from_utf8(slice::from_raw_parts(metric.key, metric.key_len))
            .expect("validated resource metric key utf-8 before encoding"),
        value: metric.value,
    });
    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_resources_response_into(sequence, metric_iter, out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_state_response(
    sequence: u32,
    bytes: *const u8,
    bytes_len: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null() || out_len.is_null() || (bytes.is_null() && bytes_len > 0) {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let bytes = if bytes_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(bytes, bytes_len)
    };
    let out = slice::from_raw_parts_mut(out, out_cap);
    match encode_state_response_into(sequence, bytes, out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::EncodeError,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_prepare_key_event(
    request: *const u8,
    request_len: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if request.is_null() || out.is_null() || out_len.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    let request = slice::from_raw_parts(request, request_len);
    let out = slice::from_raw_parts_mut(out, out_cap);
    match key_event_from_request_into(request, out) {
        Ok(len) => {
            *out_len = len;
            SqdpStatus::Ok
        }
        Err(DecodeError::OutputTooSmall { .. }) => SqdpStatus::BufferTooSmall,
        Err(_) => SqdpStatus::InvalidArgument,
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_parse_wifi_profile_set_request(
    request: *const u8,
    request_len: usize,
    out_profile: *mut SqdpWifiProfile,
) -> SqdpStatus {
    if request.is_null() || out_profile.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    if request.opcode != Opcode::WifiProfileSet {
        return SqdpStatus::InvalidArgument;
    }

    let mut profile = None;
    let mut ssid = None;
    let mut password = None;
    let mut offset = 0usize;
    while offset < request.payload().len() {
        let Some((tag, field_type, value, next_offset)) = next_tlv_field(request.payload(), offset)
        else {
            return SqdpStatus::InvalidArgument;
        };
        if field_type != 1 {
            return SqdpStatus::InvalidArgument;
        }
        match tag {
            1 if profile.is_none() => profile = Some(value),
            2 if ssid.is_none() => ssid = Some(value),
            3 if password.is_none() => password = Some(value),
            _ => return SqdpStatus::InvalidArgument,
        }
        offset = next_offset;
    }

    let (Some(profile), Some(ssid), Some(password)) = (profile, ssid, password) else {
        return SqdpStatus::InvalidArgument;
    };
    *out_profile = SqdpWifiProfile {
        profile: profile.as_ptr(),
        profile_len: profile.len(),
        ssid: ssid.as_ptr(),
        ssid_len: ssid.len(),
        password: password.as_ptr(),
        password_len: password.len(),
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_parse_state_import_request(
    request: *const u8,
    request_len: usize,
    out_import: *mut SqdpStateImport,
) -> SqdpStatus {
    if request.is_null() || out_import.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    if request.opcode != Opcode::StateImport {
        return SqdpStatus::InvalidArgument;
    }

    let mut bytes = None;
    let mut offset = 0usize;
    while offset < request.payload().len() {
        let Some((tag, field_type, value, next_offset)) = next_tlv_field(request.payload(), offset)
        else {
            return SqdpStatus::InvalidArgument;
        };
        match (tag, field_type) {
            (1, 0) if bytes.is_none() => bytes = Some(value),
            _ => return SqdpStatus::InvalidArgument,
        }
        offset = next_offset;
    }

    let Some(bytes) = bytes else {
        return SqdpStatus::InvalidArgument;
    };
    *out_import = SqdpStateImport {
        bytes: bytes.as_ptr(),
        bytes_len: bytes.len(),
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_parse_app_launch_request(
    request: *const u8,
    request_len: usize,
    out_launch: *mut SqdpAppLaunch,
) -> SqdpStatus {
    if request.is_null() || out_launch.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    if request.opcode != Opcode::AppLaunch {
        return SqdpStatus::InvalidArgument;
    }

    let mut app_id = None;
    let mut offset = 0usize;
    while offset < request.payload().len() {
        let Some((tag, field_type, value, next_offset)) = next_tlv_field(request.payload(), offset)
        else {
            return SqdpStatus::InvalidArgument;
        };
        match (tag, field_type) {
            (1, 1) if app_id.is_none() => app_id = Some(value),
            _ => return SqdpStatus::InvalidArgument,
        }
        offset = next_offset;
    }

    let Some(app_id) = app_id else {
        return SqdpStatus::InvalidArgument;
    };
    if app_id.is_empty() || app_id.len() >= SQDP_APP_ID_CAP || str::from_utf8(app_id).is_err() {
        return SqdpStatus::InvalidArgument;
    }
    *out_launch = SqdpAppLaunch {
        app_id: app_id.as_ptr(),
        app_id_len: app_id.len(),
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_parse_event_dispatch_request(
    request: *const u8,
    request_len: usize,
    out_event: *mut SqdpEventDispatch,
) -> SqdpStatus {
    if request.is_null() || out_event.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    if request.opcode != Opcode::EventDispatch {
        return SqdpStatus::InvalidArgument;
    }

    let mut app_id = None;
    let mut event = None;
    let mut offset = 0usize;
    while offset < request.payload().len() {
        let Some((tag, field_type, value, next_offset)) = next_tlv_field(request.payload(), offset)
        else {
            return SqdpStatus::InvalidArgument;
        };
        match (tag, field_type) {
            (1, 1) if app_id.is_none() => app_id = Some(value),
            (2, 1) if event.is_none() => event = Some(value),
            _ => return SqdpStatus::InvalidArgument,
        }
        offset = next_offset;
    }

    let (Some(app_id), Some(event)) = (app_id, event) else {
        return SqdpStatus::InvalidArgument;
    };
    if app_id.is_empty()
        || app_id.len() >= SQDP_APP_ID_CAP
        || event.is_empty()
        || str::from_utf8(app_id).is_err()
        || str::from_utf8(event).is_err()
    {
        return SqdpStatus::InvalidArgument;
    }
    *out_event = SqdpEventDispatch {
        app_id: app_id.as_ptr(),
        app_id_len: app_id.len(),
        event: event.as_ptr(),
        event_len: event.len(),
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_prepare_transfer_begin(
    request: *const u8,
    request_len: usize,
    session: *mut SqdpTransferSession,
    out_action: *mut SqdpAction,
) -> SqdpStatus {
    if request.is_null() || session.is_null() || out_action.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    let kind = match request.opcode {
        Opcode::AppInstallBegin => SqdpActionKind::BeginInstall,
        Opcode::TempRunBegin => SqdpActionKind::BeginTempRun,
        _ => return SqdpStatus::InvalidArgument,
    };
    let app_id = match field_bytes(request.payload(), 1, 1) {
        Some(bytes) if !bytes.is_empty() && bytes.len() < SQDP_APP_ID_CAP => bytes,
        _ => return SqdpStatus::InvalidArgument,
    };
    if str::from_utf8(app_id).is_err() {
        return SqdpStatus::InvalidArgument;
    }
    let Some(total_len) = field_u64(request.payload(), 2) else {
        return SqdpStatus::InvalidArgument;
    };
    let Some(expected_crc) = field_u64(request.payload(), 3) else {
        return SqdpStatus::InvalidArgument;
    };
    if total_len == 0
        || total_len > SQDP_MAX_APP_BYTES as u64
        || total_len > usize::MAX as u64
        || expected_crc > u32::MAX as u64
    {
        return SqdpStatus::InvalidArgument;
    }

    let session = &mut *session;
    *session = SqdpTransferSession::default();
    if set_c_string(&mut session.app_id, app_id) != SqdpStatus::Ok {
        return SqdpStatus::InvalidArgument;
    }
    session.total_len = total_len as usize;
    session.expected_crc = expected_crc as u32;
    session.running_crc = 0xffff_ffff;
    *out_action = SqdpAction {
        kind,
        app_id: session.app_id.as_ptr(),
        app_id_len: c_string_bytes(&session.app_id).len(),
        total_len: session.total_len,
        ..SqdpAction::default()
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_prepare_transfer_chunk(
    request: *const u8,
    request_len: usize,
    session: *const SqdpTransferSession,
    out_action: *mut SqdpAction,
) -> SqdpStatus {
    if request.is_null() || session.is_null() || out_action.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    let kind = match request.opcode {
        Opcode::AppInstallChunk => SqdpActionKind::WriteInstallChunk,
        Opcode::TempRunChunk => SqdpActionKind::WriteTempRunChunk,
        Opcode::ResourceInstallChunk => SqdpActionKind::WriteResourceChunk,
        _ => return SqdpStatus::InvalidArgument,
    };
    let session = &*session;
    let Some(offset) = field_u64(request.payload(), 1) else {
        return SqdpStatus::InvalidArgument;
    };
    let Some(bytes) = field_bytes(request.payload(), 2, 0) else {
        return SqdpStatus::InvalidArgument;
    };
    if offset > usize::MAX as u64 {
        return SqdpStatus::InvalidArgument;
    }
    let offset = offset as usize;
    if !session.active
        || offset != session.received
        || bytes.len() > session.total_len.saturating_sub(session.received)
    {
        return SqdpStatus::InvalidArgument;
    }
    *out_action = SqdpAction {
        kind,
        staging_path: session.staging_path.as_ptr(),
        staging_path_len: c_string_bytes(&session.staging_path).len(),
        offset,
        bytes: bytes.as_ptr(),
        bytes_len: bytes.len(),
        ..SqdpAction::default()
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_complete_transfer_chunk(
    session: *mut SqdpTransferSession,
    bytes: *const u8,
    bytes_len: usize,
) -> SqdpStatus {
    if session.is_null() || (bytes.is_null() && bytes_len > 0) {
        return SqdpStatus::InvalidArgument;
    }
    let session = &mut *session;
    let bytes = slice::from_raw_parts(bytes, bytes_len);
    if !session.active || bytes.len() > session.total_len.saturating_sub(session.received) {
        return SqdpStatus::InvalidArgument;
    }
    session.running_crc = sqdp_crc32_update(session.running_crc, bytes);
    session.received += bytes.len();
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_prepare_transfer_commit(
    request: *const u8,
    request_len: usize,
    session: *const SqdpTransferSession,
    out_action: *mut SqdpAction,
) -> SqdpStatus {
    if request.is_null() || session.is_null() || out_action.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    let kind = match request.opcode {
        Opcode::AppInstallCommit => SqdpActionKind::CommitInstall,
        Opcode::TempRunCommit => SqdpActionKind::CommitTempRun,
        Opcode::ResourceInstallCommit => SqdpActionKind::CommitResourceInstall,
        _ => return SqdpStatus::InvalidArgument,
    };
    let session = &*session;
    if !session.active || session.received != session.total_len || !session_crc_matches(session) {
        return SqdpStatus::InvalidArgument;
    }
    *out_action = SqdpAction {
        kind,
        app_id: session.app_id.as_ptr(),
        app_id_len: c_string_bytes(&session.app_id).len(),
        staging_path: session.staging_path.as_ptr(),
        staging_path_len: c_string_bytes(&session.staging_path).len(),
        total_len: session.total_len,
        ..SqdpAction::default()
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_clear_transfer_session(session: *mut SqdpTransferSession) {
    if !session.is_null() {
        *session = SqdpTransferSession::default();
    }
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_prepare_resource_begin(
    request: *const u8,
    request_len: usize,
    session: *mut SqdpResourceSession,
    out_action: *mut SqdpAction,
) -> SqdpStatus {
    if request.is_null() || session.is_null() || out_action.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    if request.opcode != Opcode::ResourceInstallBegin {
        return SqdpStatus::InvalidArgument;
    }
    let app_id = match field_bytes(request.payload(), 1, 1) {
        Some(bytes) if !bytes.is_empty() && bytes.len() < SQDP_APP_ID_CAP => bytes,
        _ => return SqdpStatus::InvalidArgument,
    };
    let resource_path = match field_bytes(request.payload(), 2, 1) {
        Some(bytes) if !bytes.is_empty() && bytes.len() < SQDP_PATH_CAP => bytes,
        _ => return SqdpStatus::InvalidArgument,
    };
    if str::from_utf8(app_id).is_err() || str::from_utf8(resource_path).is_err() {
        return SqdpStatus::InvalidArgument;
    }
    let Some(total_len) = field_u64(request.payload(), 3) else {
        return SqdpStatus::InvalidArgument;
    };
    let Some(expected_crc) = field_u64(request.payload(), 4) else {
        return SqdpStatus::InvalidArgument;
    };
    if total_len == 0
        || total_len > SQDP_MAX_APP_BYTES as u64
        || total_len > usize::MAX as u64
        || expected_crc > u32::MAX as u64
    {
        return SqdpStatus::InvalidArgument;
    }

    let session = &mut *session;
    *session = SqdpResourceSession::default();
    if set_c_string(&mut session.app_id, app_id) != SqdpStatus::Ok
        || set_c_string(&mut session.resource_path, resource_path) != SqdpStatus::Ok
    {
        return SqdpStatus::InvalidArgument;
    }
    session.total_len = total_len as usize;
    session.expected_crc = expected_crc as u32;
    session.running_crc = 0xffff_ffff;
    *out_action = SqdpAction {
        kind: SqdpActionKind::BeginResourceInstall,
        app_id: session.app_id.as_ptr(),
        app_id_len: c_string_bytes(&session.app_id).len(),
        resource_path: session.resource_path.as_ptr(),
        resource_path_len: c_string_bytes(&session.resource_path).len(),
        total_len: session.total_len,
        ..SqdpAction::default()
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_prepare_resource_chunk(
    request: *const u8,
    request_len: usize,
    session: *const SqdpResourceSession,
    out_action: *mut SqdpAction,
) -> SqdpStatus {
    if request.is_null() || session.is_null() || out_action.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    if request.opcode != Opcode::ResourceInstallChunk {
        return SqdpStatus::InvalidArgument;
    }
    let session = &*session;
    let Some(offset) = field_u64(request.payload(), 1) else {
        return SqdpStatus::InvalidArgument;
    };
    let Some(bytes) = field_bytes(request.payload(), 2, 0) else {
        return SqdpStatus::InvalidArgument;
    };
    if offset > usize::MAX as u64 {
        return SqdpStatus::InvalidArgument;
    }
    let offset = offset as usize;
    if !session.active
        || offset != session.received
        || bytes.len() > session.total_len.saturating_sub(session.received)
    {
        return SqdpStatus::InvalidArgument;
    }
    *out_action = SqdpAction {
        kind: SqdpActionKind::WriteResourceChunk,
        staging_path: session.staging_path.as_ptr(),
        staging_path_len: c_string_bytes(&session.staging_path).len(),
        offset,
        bytes: bytes.as_ptr(),
        bytes_len: bytes.len(),
        ..SqdpAction::default()
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_complete_resource_chunk(
    session: *mut SqdpResourceSession,
    bytes: *const u8,
    bytes_len: usize,
) -> SqdpStatus {
    if session.is_null() || (bytes.is_null() && bytes_len > 0) {
        return SqdpStatus::InvalidArgument;
    }
    let session = &mut *session;
    let bytes = slice::from_raw_parts(bytes, bytes_len);
    if !session.active || bytes.len() > session.total_len.saturating_sub(session.received) {
        return SqdpStatus::InvalidArgument;
    }
    session.running_crc = sqdp_crc32_update(session.running_crc, bytes);
    session.received += bytes.len();
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_prepare_resource_commit(
    request: *const u8,
    request_len: usize,
    session: *const SqdpResourceSession,
    out_action: *mut SqdpAction,
) -> SqdpStatus {
    if request.is_null() || session.is_null() || out_action.is_null() {
        return SqdpStatus::InvalidArgument;
    }
    let request = match DeviceRequest::decode(slice::from_raw_parts(request, request_len)) {
        Ok(request) => request,
        Err(_) => return SqdpStatus::InvalidArgument,
    };
    if request.opcode != Opcode::ResourceInstallCommit {
        return SqdpStatus::InvalidArgument;
    }
    let session = &*session;
    if !session.active || session.received != session.total_len || !resource_crc_matches(session) {
        return SqdpStatus::InvalidArgument;
    }
    *out_action = SqdpAction {
        kind: SqdpActionKind::CommitResourceInstall,
        app_id: session.app_id.as_ptr(),
        app_id_len: c_string_bytes(&session.app_id).len(),
        resource_path: session.resource_path.as_ptr(),
        resource_path_len: c_string_bytes(&session.resource_path).len(),
        staging_path: session.staging_path.as_ptr(),
        staging_path_len: c_string_bytes(&session.staging_path).len(),
        total_len: session.total_len,
        ..SqdpAction::default()
    };
    SqdpStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqdp_clear_resource_session(session: *mut SqdpResourceSession) {
    if !session.is_null() {
        *session = SqdpResourceSession::default();
    }
}

// Mirrors the current Zephyr runtime return-stack and armed-timer capacities.
const SQVM_FFI_APP_STACK_CAP: usize = 2;

struct FfiHost {
    callbacks: SqvmCallbacks,
    defer_sqbc_reads: bool,
    app_registry_entries: [SqvmAppRegistryEntry; 8],
    app_registry_core_entries: [AppRegistryEntry<'static>; 8],
    app_registry_count: usize,
    app_stack_entries: [SqvmAppStackEntry; SQVM_FFI_APP_STACK_CAP],
    app_process_stack_apps: [&'static str; SQVM_FFI_APP_STACK_CAP],
    app_armed_stack_entries: [AppArmedStackEntry<'static>; SQVM_FFI_APP_STACK_CAP],
    app_stack_count: usize,
    wifi_scan_networks: [WifiAccessPoint; SQVM_WIFI_SCAN_MAX_NETWORKS],
    wifi_scan_network_count: usize,
}

impl FfiHost {
    fn new(callbacks: SqvmCallbacks, defer_sqbc_reads: bool) -> Self {
        Self {
            callbacks,
            defer_sqbc_reads,
            app_registry_entries: [SqvmAppRegistryEntry::default(); 8],
            app_registry_core_entries: [AppRegistryEntry {
                id: "",
                name: "",
                build: "",
                description: "",
            }; 8],
            app_registry_count: 0,
            app_stack_entries: [SqvmAppStackEntry::default(); SQVM_FFI_APP_STACK_CAP],
            app_process_stack_apps: [""; SQVM_FFI_APP_STACK_CAP],
            app_armed_stack_entries: [AppArmedStackEntry {
                app_id: "",
                event: "",
            }; SQVM_FFI_APP_STACK_CAP],
            app_stack_count: 0,
            wifi_scan_networks: [WifiAccessPoint::empty(); SQVM_WIFI_SCAN_MAX_NETWORKS],
            wifi_scan_network_count: 0,
        }
    }
}

impl SqbcReader for FfiHost {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let Some(read_exact_at) = self.callbacks.read_exact_at else {
            return Err(VmError::ReadFailed);
        };
        let status = unsafe {
            read_exact_at(
                self.callbacks.user_data,
                offset,
                out.as_mut_ptr(),
                out.len(),
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(VmError::ReadFailed)
        }
    }

    fn should_defer_read(&mut self, _offset: usize, _len: usize) -> Result<bool, VmError> {
        Ok(self.defer_sqbc_reads)
    }
}

impl TraceSink for FfiHost {
    fn trace(&mut self, message: &str) {
        if let Some(trace) = self.callbacks.trace {
            unsafe {
                trace(self.callbacks.user_data, message.as_ptr(), message.len());
            }
        }
    }

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
        let Some(debug_output) = self.callbacks.debug_output else {
            return;
        };
        let mut line = FixedLine::<128>::default();
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                let _ = line.write_str(" ");
            }
            match value {
                Value::String(_) | Value::RuntimeString(_) => {
                    let text = strings.value_str(*value).unwrap_or("<string>");
                    let _ = line.write_str(text);
                }
                Value::I32(value) => {
                    let _ = write!(line, "{value}");
                }
                Value::Bool(value) => {
                    let _ = write!(line, "{value}");
                }
                Value::Null => {
                    let _ = line.write_str("null");
                }
                Value::Record(_) => {
                    let _ = line.write_str("<record>");
                }
                Value::List(_) => {
                    let _ = line.write_str("<list>");
                }
            }
        }
        unsafe {
            debug_output(self.callbacks.user_data, line.as_ptr(), line.len());
        }
    }

    fn draw_clear(&mut self, color: &str) {
        if let Some(display_clear) = self.callbacks.display_clear {
            unsafe {
                display_clear(self.callbacks.user_data, color.as_ptr(), color.len());
            }
        }
    }

    fn draw_text(
        &mut self,
        strings: &StringResolver<'_>,
        text: Value,
        options: DisplayTextOptions<'_>,
    ) {
        let Some(display_text) = self.callbacks.display_text else {
            return;
        };
        let mut rendered = FixedLine::<128>::default();
        write_value(&mut rendered, strings, text);
        let options = SqvmDisplayTextOptions {
            x: options.x,
            y: options.y,
            w: options.w,
            h: options.h,
            font_height: options.font_height,
            text_color: option_ptr(options.text_color),
            text_color_len: option_len(options.text_color),
            background_color: option_ptr(options.background_color),
            background_color_len: option_len(options.background_color),
            align: option_ptr(options.align),
            align_len: option_len(options.align),
            valign: option_ptr(options.valign),
            valign_len: option_len(options.valign),
        };
        unsafe {
            display_text(
                self.callbacks.user_data,
                rendered.as_ptr(),
                rendered.len(),
                &options,
            );
        }
    }

    fn draw_rect(&mut self, options: DisplayRectOptions<'_>) {
        let Some(display_rect) = self.callbacks.display_rect else {
            return;
        };
        let options = SqvmDisplayRectOptions {
            x: options.x,
            y: options.y,
            w: options.w,
            h: options.h,
            fill_color: option_ptr(options.fill_color),
            fill_color_len: option_len(options.fill_color),
            stroke_color: option_ptr(options.stroke_color),
            stroke_color_len: option_len(options.stroke_color),
        };
        unsafe {
            display_rect(self.callbacks.user_data, &options);
        }
    }

    fn draw_line(&mut self, options: DisplayLineOptions<'_>) {
        let Some(display_line) = self.callbacks.display_line else {
            return;
        };
        let options = SqvmDisplayLineOptions {
            x1: options.x1,
            y1: options.y1,
            x2: options.x2,
            y2: options.y2,
            color: option_ptr(options.color),
            color_len: option_len(options.color),
        };
        unsafe {
            display_line(self.callbacks.user_data, &options);
        }
    }

    fn draw_select(&mut self, name: &str) -> Result<(), VmError> {
        let Some(display_select) = self.callbacks.display_select else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe {
            display_select(self.callbacks.user_data, name.as_ptr(), name.len())
        })
    }

    fn draw_image(&mut self, path: &str, options: DisplayResourceOptions) {
        let Some(display_image) = self.callbacks.display_image else {
            return;
        };
        let options = SqvmDisplayResourceOptions {
            x: options.x,
            y: options.y,
            w: options.w,
            h: options.h,
        };
        unsafe {
            display_image(
                self.callbacks.user_data,
                path.as_ptr(),
                path.len(),
                &options,
            );
        }
    }

    fn draw_resource(
        &mut self,
        strings: &StringResolver<'_>,
        drawable: Value,
        options: DisplayResourceOptions,
    ) {
        let Some(display_draw) = self.callbacks.display_draw else {
            return;
        };
        let mut rendered = FixedLine::<128>::default();
        write_value(&mut rendered, strings, drawable);
        let options = SqvmDisplayResourceOptions {
            x: options.x,
            y: options.y,
            w: options.w,
            h: options.h,
        };
        unsafe {
            display_draw(
                self.callbacks.user_data,
                rendered.as_ptr(),
                rendered.len(),
                &options,
            );
        }
    }

    fn service_indicator_write(&mut self, value: bool) -> Result<(), VmError> {
        let Some(indicator_write) = self.callbacks.indicator_write else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_write(self.callbacks.user_data, value) })
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        let Some(indicator_toggle) = self.callbacks.indicator_toggle else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_toggle(self.callbacks.user_data) })
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        let Some(indicator_read) = self.callbacks.indicator_read else {
            return Err(VmError::InvalidOperand);
        };
        let mut value = false;
        callback_status(unsafe { indicator_read(self.callbacks.user_data, &mut value) })?;
        Ok(value)
    }

    fn service_indicator_breathe(&mut self) -> Result<(), VmError> {
        let Some(indicator_breathe) = self.callbacks.indicator_breathe else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_breathe(self.callbacks.user_data) })
    }

    fn service_indicator_blink(&mut self, on_ms: i32, off_ms: i32) -> Result<(), VmError> {
        let Some(indicator_blink) = self.callbacks.indicator_blink else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_blink(self.callbacks.user_data, on_ms, off_ms) })
    }

    fn hardware_gpio_write(&mut self, name: &str, value: bool) -> Result<(), VmError> {
        let Some(hardware_gpio_write) = self.callbacks.hardware_gpio_write else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe {
            hardware_gpio_write(self.callbacks.user_data, name.as_ptr(), name.len(), value)
        })
    }

    fn hardware_gpio_toggle(&mut self, name: &str) -> Result<(), VmError> {
        let Some(hardware_gpio_toggle) = self.callbacks.hardware_gpio_toggle else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe {
            hardware_gpio_toggle(self.callbacks.user_data, name.as_ptr(), name.len())
        })
    }

    fn hardware_gpio_read(&mut self, name: &str) -> Result<bool, VmError> {
        let Some(hardware_gpio_read) = self.callbacks.hardware_gpio_read else {
            return Err(VmError::InvalidOperand);
        };
        let mut value = false;
        callback_status(unsafe {
            hardware_gpio_read(
                self.callbacks.user_data,
                name.as_ptr(),
                name.len(),
                &mut value,
            )
        })?;
        Ok(value)
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        let Some(app_launch) = self.callbacks.app_launch else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { app_launch(self.callbacks.user_data, app.as_ptr(), app.len()) })
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        let Some(app_arm) = self.callbacks.app_arm else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { app_arm(self.callbacks.user_data, app.as_ptr(), app.len()) })
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        let Some(app_disarm) = self.callbacks.app_disarm else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { app_disarm(self.callbacks.user_data, app.as_ptr(), app.len()) })
    }

    fn app_registry_list<'a>(&'a mut self) -> Result<AppRegistryList<'a>, VmError> {
        let Some(app_registry_list) = self.callbacks.app_registry_list else {
            return Err(VmError::InvalidOperand);
        };
        let mut count = 0usize;
        self.app_registry_entries = [SqvmAppRegistryEntry::default(); 8];
        callback_status(unsafe {
            app_registry_list(
                self.callbacks.user_data,
                self.app_registry_entries.as_mut_ptr(),
                self.app_registry_entries.len(),
                &mut count,
            )
        })?;
        self.app_registry_count = count.min(self.app_registry_entries.len());
        for index in 0..self.app_registry_count {
            self.app_registry_core_entries[index] =
                unsafe { app_registry_entry_from_ffi(&self.app_registry_entries[index])? };
        }
        Ok(AppRegistryList {
            apps: &self.app_registry_core_entries[..self.app_registry_count],
        })
    }

    fn app_registry_get<'a>(&'a mut self, app: &str) -> Result<AppRegistryEntry<'a>, VmError> {
        let Some(app_registry_get) = self.callbacks.app_registry_get else {
            return Err(VmError::InvalidOperand);
        };
        let mut out = SqvmAppRegistryEntry::default();
        callback_status(unsafe {
            app_registry_get(self.callbacks.user_data, app.as_ptr(), app.len(), &mut out)
        })?;
        unsafe { app_registry_entry_from_ffi(&out) }
    }

    fn app_process_stack<'a>(&'a mut self) -> Result<AppProcessStack<'a>, VmError> {
        let Some(app_process_stack) = self.callbacks.app_process_stack else {
            return Err(VmError::InvalidOperand);
        };
        let mut count = 0usize;
        self.app_stack_entries = [SqvmAppStackEntry::default(); SQVM_FFI_APP_STACK_CAP];
        callback_status(unsafe {
            app_process_stack(
                self.callbacks.user_data,
                self.app_stack_entries.as_mut_ptr(),
                self.app_stack_entries.len(),
                &mut count,
            )
        })?;
        self.app_stack_count = count.min(self.app_stack_entries.len());
        for index in 0..self.app_stack_count {
            self.app_process_stack_apps[index] = unsafe {
                required_ffi_str(
                    self.app_stack_entries[index].app_id,
                    self.app_stack_entries[index].app_id_len,
                )?
            };
        }
        Ok(AppProcessStack {
            apps: &self.app_process_stack_apps[..self.app_stack_count],
        })
    }

    fn app_armed_stack<'a>(&'a mut self) -> Result<AppArmedStack<'a>, VmError> {
        let Some(app_armed_stack) = self.callbacks.app_armed_stack else {
            return Err(VmError::InvalidOperand);
        };
        let mut count = 0usize;
        self.app_stack_entries = [SqvmAppStackEntry::default(); SQVM_FFI_APP_STACK_CAP];
        callback_status(unsafe {
            app_armed_stack(
                self.callbacks.user_data,
                self.app_stack_entries.as_mut_ptr(),
                self.app_stack_entries.len(),
                &mut count,
            )
        })?;
        self.app_stack_count = count.min(self.app_stack_entries.len());
        for index in 0..self.app_stack_count {
            self.app_armed_stack_entries[index] =
                unsafe { app_armed_stack_entry_from_ffi(&self.app_stack_entries[index])? };
        }
        Ok(AppArmedStack {
            entries: &self.app_armed_stack_entries[..self.app_stack_count],
        })
    }

    fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
        let Some(timer_every) = self.callbacks.timer_every else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe {
            timer_every(
                self.callbacks.user_data,
                event.as_ptr(),
                event.len(),
                interval_ms,
            )
        })
    }

    fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
        let Some(timer_after) = self.callbacks.timer_after else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe {
            timer_after(
                self.callbacks.user_data,
                event.as_ptr(),
                event.len(),
                delay_ms,
            )
        })
    }

    fn service_wifi_start_ap<'a>(
        &'a mut self,
        ssid: &str,
    ) -> Result<WifiActionResult<'a>, VmError> {
        let Some(wifi_start_ap) = self.callbacks.wifi_start_ap else {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("unsupported"),
            });
        };
        let mut out = SqvmWifiActionResult::default();
        callback_status(unsafe {
            wifi_start_ap(
                self.callbacks.user_data,
                ssid.as_ptr(),
                ssid.len(),
                &mut out,
            )
        })?;
        unsafe { wifi_action_result_from_ffi(&out) }
    }

    fn service_wifi_stop_ap<'a>(&'a mut self) -> Result<WifiActionResult<'a>, VmError> {
        let Some(wifi_stop_ap) = self.callbacks.wifi_stop_ap else {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("unsupported"),
            });
        };
        let mut out = SqvmWifiActionResult::default();
        callback_status(unsafe { wifi_stop_ap(self.callbacks.user_data, &mut out) })?;
        unsafe { wifi_action_result_from_ffi(&out) }
    }

    fn service_wifi_connect<'a>(
        &'a mut self,
        profile: &str,
    ) -> Result<WifiActionResult<'a>, VmError> {
        let Some(wifi_connect) = self.callbacks.wifi_connect else {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("unsupported"),
            });
        };
        let mut out = SqvmWifiActionResult::default();
        callback_status(unsafe {
            wifi_connect(
                self.callbacks.user_data,
                profile.as_ptr(),
                profile.len(),
                &mut out,
            )
        })?;
        unsafe { wifi_action_result_from_ffi(&out) }
    }

    fn service_wifi_disconnect<'a>(&'a mut self) -> Result<WifiActionResult<'a>, VmError> {
        let Some(wifi_disconnect) = self.callbacks.wifi_disconnect else {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("unsupported"),
            });
        };
        let mut out = SqvmWifiActionResult::default();
        callback_status(unsafe { wifi_disconnect(self.callbacks.user_data, &mut out) })?;
        unsafe { wifi_action_result_from_ffi(&out) }
    }

    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        let Some(wifi_status) = self.callbacks.wifi_status else {
            return Err(VmError::InvalidOperand);
        };
        let mut out = SqvmWifiStatus::default();
        callback_status(unsafe { wifi_status(self.callbacks.user_data, &mut out) })?;
        unsafe { wifi_status_from_ffi(&out) }
    }

    fn service_wifi_get_ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        let Some(wifi_get_ap_ip) = self.callbacks.wifi_get_ap_ip else {
            return Ok(WifiApIp {
                ip: None,
                gw: None,
                netmask: None,
                error: Some("unsupported"),
            });
        };
        let mut out = SqvmWifiApIp::default();
        callback_status(unsafe { wifi_get_ap_ip(self.callbacks.user_data, &mut out) })?;
        unsafe { wifi_ap_ip_from_ffi(&out) }
    }

    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiScanResult<'a>, VmError> {
        let Some(wifi_scan) = self.callbacks.wifi_scan else {
            return Ok(WifiScanResult {
                ok: false,
                error: Some("unsupported"),
                networks: &[],
            });
        };
        let mut out = SqvmWifiScanResult::default();
        callback_status(unsafe { wifi_scan(self.callbacks.user_data, &mut out) })?;
        self.wifi_scan_network_count = 0;
        let count = out.network_count.min(SQVM_WIFI_SCAN_MAX_NETWORKS);
        if count > 0 {
            if out.networks.is_null() {
                return Err(VmError::InvalidOperand);
            }
            let networks = unsafe { slice::from_raw_parts(out.networks, count) };
            for (index, network) in networks.iter().enumerate() {
                self.wifi_scan_networks[index] = wifi_access_point_from_ffi(network)?;
            }
            self.wifi_scan_network_count = count;
        }
        Ok(WifiScanResult {
            ok: out.ok,
            error: unsafe { optional_ffi_str(out.error, out.error_len)? },
            networks: &self.wifi_scan_networks[..self.wifi_scan_network_count],
        })
    }

    fn device_config_load<'a>(
        &'a mut self,
        source: &str,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        let Some(device_config_load) = self.callbacks.device_config_load else {
            return Ok(DeviceConfigResult::unsupported());
        };
        let mut out = SqvmDeviceConfigResult::default();
        callback_status(unsafe {
            device_config_load(
                self.callbacks.user_data,
                source.as_ptr(),
                source.len(),
                &mut out,
            )
        })?;
        unsafe { device_config_result_from_ffi(&out) }
    }

    fn device_config_set<'a>(
        &'a mut self,
        key: &str,
        value: Value,
        strings: &StringResolver<'_>,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        let Some(device_config_set) = self.callbacks.device_config_set else {
            return Ok(DeviceConfigResult::unsupported());
        };
        let value = device_config_value_to_ffi(value, strings)?;
        let mut out = SqvmDeviceConfigResult::default();
        callback_status(unsafe {
            device_config_set(
                self.callbacks.user_data,
                key.as_ptr(),
                key.len(),
                value,
                &mut out,
            )
        })?;
        unsafe { device_config_result_from_ffi(&out) }
    }

    fn device_config_rebind<'a>(
        &'a mut self,
        alias: &str,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        let Some(device_config_rebind) = self.callbacks.device_config_rebind else {
            return Ok(DeviceConfigResult::unsupported());
        };
        let mut out = SqvmDeviceConfigResult::default();
        callback_status(unsafe {
            device_config_rebind(
                self.callbacks.user_data,
                alias.as_ptr(),
                alias.len(),
                &mut out,
            )
        })?;
        unsafe { device_config_result_from_ffi(&out) }
    }

    fn device_config_save<'a>(
        &'a mut self,
        destination: &str,
    ) -> Result<DeviceConfigResult<'a>, VmError> {
        let Some(device_config_save) = self.callbacks.device_config_save else {
            return Ok(DeviceConfigResult::unsupported());
        };
        let mut out = SqvmDeviceConfigResult::default();
        callback_status(unsafe {
            device_config_save(
                self.callbacks.user_data,
                destination.as_ptr(),
                destination.len(),
                &mut out,
            )
        })?;
        unsafe { device_config_result_from_ffi(&out) }
    }

    fn content_pick_file<'a>(
        &'a mut self,
        extension: &str,
    ) -> Result<ContentPickFileResult<'a>, VmError> {
        let Some(content_pick_file) = self.callbacks.content_pick_file else {
            return Ok(ContentPickFileResult::unsupported());
        };
        let mut out = SqvmContentPickFileResult::default();
        callback_status(unsafe {
            content_pick_file(
                self.callbacks.user_data,
                extension.as_ptr(),
                extension.len(),
                &mut out,
            )
        })?;
        unsafe { content_pick_file_result_from_ffi(&out) }
    }

    fn content_read_text<'a>(
        &'a mut self,
        path: &str,
    ) -> Result<ContentReadTextResult<'a>, VmError> {
        let Some(content_read_text) = self.callbacks.content_read_text else {
            return Ok(ContentReadTextResult::unsupported());
        };
        let mut out = SqvmContentReadTextResult::default();
        callback_status(unsafe {
            content_read_text(
                self.callbacks.user_data,
                path.as_ptr(),
                path.len(),
                &mut out,
            )
        })?;
        unsafe { content_read_text_result_from_ffi(&out) }
    }

    fn content_read_lines<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<ContentReadLinesResult<'a>, VmError> {
        let Some(content_read_lines) = self.callbacks.content_read_lines else {
            return Ok(ContentReadLinesResult::unsupported());
        };
        let mut out = SqvmContentReadLinesResult::default();
        callback_status(unsafe {
            content_read_lines(
                self.callbacks.user_data,
                path.as_ptr(),
                path.len(),
                max_lines,
                &mut out,
            )
        })?;
        unsafe { content_read_lines_result_from_ffi(&out) }
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        let Some(system_memory_text) = self.callbacks.system_memory_text else {
            return Err(VmError::InvalidOperand);
        };
        let mut line = FixedLine::<96>::default();
        let mut line_len = 0usize;
        callback_status(unsafe {
            system_memory_text(
                self.callbacks.user_data,
                line.as_mut_ptr(),
                line.cap(),
                &mut line_len,
            )
        })?;
        line.set_len(line_len)?;
        out.write_str(line.as_str()?)
            .map_err(|_| VmError::InvalidOperand)
    }

    fn system_storage_text(&mut self, name: &str, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        let Some(system_storage_text) = self.callbacks.system_storage_text else {
            return Err(VmError::InvalidOperand);
        };
        let mut line = FixedLine::<96>::default();
        let mut line_len = 0usize;
        callback_status(unsafe {
            system_storage_text(
                self.callbacks.user_data,
                name.as_ptr(),
                name.len(),
                line.as_mut_ptr(),
                line.cap(),
                &mut line_len,
            )
        })?;
        line.set_len(line_len)?;
        out.write_str(line.as_str()?)
            .map_err(|_| VmError::InvalidOperand)
    }

    fn state_load(&mut self, _out: &mut [u8]) -> Result<Option<usize>, VmError> {
        Ok(None)
    }

    fn state_save(&mut self, _bytes: &[u8]) -> Result<(), VmError> {
        Ok(())
    }
}

fn status_from_vm(result: Result<(), VmError>) -> SqvmStatus {
    match result {
        Ok(()) => SqvmStatus::Ok,
        Err(_) => SqvmStatus::VmError,
    }
}

fn trigger_timer_count_from_reader(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
    out_count: *mut usize,
) -> SqvmStatus {
    let count = match ProgramIndex::trigger_timer_count_from_reader(reader, scratch) {
        Ok(count) => count,
        Err(_) => return SqvmStatus::VmError,
    };
    unsafe {
        *out_count = count;
    }
    SqvmStatus::Ok
}

fn trigger_timer_read_from_reader(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
    timer_index: usize,
    out_timer: *mut SqvmTriggerTimer,
) -> SqvmStatus {
    let timer = match ProgramIndex::trigger_timer_from_reader(reader, scratch, timer_index) {
        Ok(timer) => timer,
        Err(VmError::InvalidOperand) => return SqvmStatus::InvalidArgument,
        Err(_) => return SqvmStatus::VmError,
    };
    if timer.event.len() >= 32 {
        return SqvmStatus::VmError;
    }
    let mut out = SqvmTriggerTimer {
        interval_ms: timer.interval_ms,
        repeating: timer.repeating,
        ..SqvmTriggerTimer::default()
    };
    out.event[..timer.event.len()].copy_from_slice(timer.event.as_bytes());
    unsafe {
        *out_timer = out;
    }
    SqvmStatus::Ok
}

fn device_binding_count_from_reader(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
    out_count: *mut usize,
) -> SqvmStatus {
    let count = match ProgramIndex::device_binding_count_from_reader(reader, scratch) {
        Ok(count) => count,
        Err(_) => return SqvmStatus::VmError,
    };
    unsafe {
        *out_count = count;
    }
    SqvmStatus::Ok
}

fn device_binding_read_from_reader(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
    binding_index: usize,
    out_binding: *mut SqvmDeviceBinding,
) -> SqvmStatus {
    let binding = match ProgramIndex::device_binding_from_reader(reader, scratch, binding_index) {
        Ok(binding) => binding,
        Err(VmError::InvalidOperand) => return SqvmStatus::InvalidArgument,
        Err(_) => return SqvmStatus::VmError,
    };
    if binding.service.len() >= SQVM_DEVICE_BINDING_NAME_CAP
        || binding.binding.len() >= SQVM_DEVICE_BINDING_NAME_CAP
        || binding.resource.len() >= SQVM_DEVICE_BINDING_RESOURCE_CAP
    {
        return SqvmStatus::VmError;
    }
    let mut out = SqvmDeviceBinding::default();
    out.service[..binding.service.len()].copy_from_slice(binding.service.as_bytes());
    out.binding[..binding.binding.len()].copy_from_slice(binding.binding.as_bytes());
    out.resource[..binding.resource.len()].copy_from_slice(binding.resource.as_bytes());
    unsafe {
        *out_binding = out;
    }
    SqvmStatus::Ok
}

unsafe fn ffi_bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    if ptr.is_null() {
        return None;
    }
    Some(slice::from_raw_parts(ptr, len))
}

fn parse_sqdevice_bytes(input: &[u8], out: &mut SqdcConfig) -> SqdcStatus {
    *out = SqdcConfig::default();
    let mut saw_header = false;
    for raw_line in input.split(|byte| *byte == b'\n') {
        let line = trim_ascii(raw_line);
        if line.is_empty() || line[0] == b'#' {
            continue;
        }
        if !saw_header {
            if line != SQDEVICE_HEADER {
                return SqdcStatus::ParseError;
            }
            saw_header = true;
            continue;
        }
        let Some((key, rest)) = split_ascii_word(line) else {
            return SqdcStatus::ParseError;
        };
        let (value_type, value_text) = split_ascii_word(rest).unwrap_or((trim_ascii(rest), &[]));
        let value = match parse_sqdevice_value(value_type, value_text) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let status = config_set_new_value(out, key, value);
        if status != SqdcStatus::Ok {
            return status;
        }
    }
    if saw_header {
        SqdcStatus::Ok
    } else {
        SqdcStatus::ParseError
    }
}

fn parse_sqdevice_value(value_type: &[u8], value_text: &[u8]) -> Result<SqdcValue, SqdcStatus> {
    match value_type {
        b"null" if value_text.is_empty() => Ok(SqdcValue::default()),
        b"null" => Err(SqdcStatus::ParseError),
        b"bool" => match value_text {
            b"true" => Ok(SqdcValue {
                kind: SqdcValueKind::Bool,
                bool_value: true,
                ..SqdcValue::default()
            }),
            b"false" => Ok(SqdcValue {
                kind: SqdcValueKind::Bool,
                bool_value: false,
                ..SqdcValue::default()
            }),
            _ => Err(SqdcStatus::ParseError),
        },
        b"int" => {
            let value = parse_i32_ascii(value_text)?;
            Ok(SqdcValue {
                kind: SqdcValueKind::I32,
                i32_value: value,
                ..SqdcValue::default()
            })
        }
        b"string" => {
            let Some(colon) = value_text.iter().position(|byte| *byte == b':') else {
                return Err(SqdcStatus::ParseError);
            };
            let len = parse_usize_ascii(&value_text[..colon])?;
            let value = &value_text[colon + 1..];
            if value.len() != len || value.len() > SQDC_CONFIG_STRING_CAP {
                return Err(SqdcStatus::ParseError);
            }
            if str::from_utf8(value).is_err() {
                return Err(SqdcStatus::ParseError);
            }
            let mut stored = SqdcValue {
                kind: SqdcValueKind::String,
                string_len: value.len(),
                ..SqdcValue::default()
            };
            stored.string[..value.len()].copy_from_slice(value);
            Ok(stored)
        }
        _ => Err(SqdcStatus::ParseError),
    }
}

fn config_set_value(config: &mut SqdcConfig, key: &[u8], value: SqdcValue) -> SqdcStatus {
    if !valid_sqdc_key(key) {
        return SqdcStatus::InvalidArgument;
    }
    if let Some(index) = config_record_index(config, key) {
        config.records[index].value = value;
        return SqdcStatus::Ok;
    }
    append_config_record(config, key, value)
}

fn config_set_new_value(config: &mut SqdcConfig, key: &[u8], value: SqdcValue) -> SqdcStatus {
    if config_record_index(config, key).is_some() {
        return SqdcStatus::ParseError;
    }
    append_config_record(config, key, value)
}

fn append_config_record(config: &mut SqdcConfig, key: &[u8], value: SqdcValue) -> SqdcStatus {
    if !valid_sqdc_key(key) {
        return SqdcStatus::InvalidArgument;
    }
    if config.count >= SQDC_CONFIG_MAX_RECORDS {
        return SqdcStatus::TooManyRecords;
    }
    let record = &mut config.records[config.count];
    *record = SqdcRecord::default();
    record.present = true;
    record.key_len = key.len();
    record.key[..key.len()].copy_from_slice(key);
    record.value = value;
    config.count += 1;
    SqdcStatus::Ok
}

fn config_record_index(config: &SqdcConfig, key: &[u8]) -> Option<usize> {
    config.records[..config.count]
        .iter()
        .position(|record| record.present && &record.key[..record.key_len] == key)
}

fn valid_sqdc_key(key: &[u8]) -> bool {
    !key.is_empty()
        && key.len() <= SQDC_CONFIG_KEY_CAP
        && str::from_utf8(key).is_ok()
        && key
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'_' | b'-'))
}

fn plan_device_binding_bytes(
    service: &[u8],
    binding: &[u8],
    resource: &[u8],
    out: &mut SqdcDeviceBindingPlan,
    out_inline_config: Option<&mut SqdcConfig>,
) -> SqdcStatus {
    if !valid_device_binding_name(service) || !valid_device_binding_name(binding) {
        return SqdcStatus::InvalidArgument;
    }
    if resource.is_empty() || resource.len() >= SQVM_DEVICE_BINDING_RESOURCE_CAP {
        return SqdcStatus::InvalidArgument;
    }

    *out = SqdcDeviceBindingPlan::default();
    let Some(alias_len) = build_device_binding_alias(service, binding, &mut out.alias) else {
        return SqdcStatus::InvalidArgument;
    };
    out.alias_len = alias_len;
    out.resource[..resource.len()].copy_from_slice(resource);
    out.resource_len = resource.len();

    if let Some(pin_name) = parse_inline_gpio_resource(resource) {
        if service != b"indicator" || binding != b"default" {
            *out = SqdcDeviceBindingPlan::default();
            return SqdcStatus::InvalidArgument;
        }
        let Some(out_inline_config) = out_inline_config else {
            *out = SqdcDeviceBindingPlan::default();
            return SqdcStatus::InvalidArgument;
        };
        out.kind = SqdcDeviceBindingResourceKind::InlineGpio;
        let status =
            build_inline_gpio_config(&out.alias[..out.alias_len], pin_name, out_inline_config);
        if status != SqdcStatus::Ok {
            *out = SqdcDeviceBindingPlan::default();
            *out_inline_config = SqdcConfig::default();
        }
        return status;
    }

    if let Some((pin_name, event, active_low)) = parse_inline_gpio_button_resource(resource) {
        if service != b"input" {
            *out = SqdcDeviceBindingPlan::default();
            return SqdcStatus::InvalidArgument;
        }
        let Some(out_inline_config) = out_inline_config else {
            *out = SqdcDeviceBindingPlan::default();
            return SqdcStatus::InvalidArgument;
        };
        out.kind = SqdcDeviceBindingResourceKind::InlineGpioButton;
        let status = build_inline_gpio_button_config(
            &out.alias[..out.alias_len],
            pin_name,
            event,
            active_low,
            out_inline_config,
        );
        if status != SqdcStatus::Ok {
            *out = SqdcDeviceBindingPlan::default();
            *out_inline_config = SqdcConfig::default();
        }
        return status;
    }

    if is_safe_sqdevice_path_bytes(resource) {
        if !supports_package_device_binding(service, binding) {
            *out = SqdcDeviceBindingPlan::default();
            return SqdcStatus::InvalidArgument;
        }
        if let Some(out_inline_config) = out_inline_config {
            *out_inline_config = SqdcConfig::default();
        }
        out.kind = SqdcDeviceBindingResourceKind::PackageSqdevice;
        return SqdcStatus::Ok;
    }

    *out = SqdcDeviceBindingPlan::default();
    SqdcStatus::InvalidArgument
}

fn valid_device_binding_name(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() < SQVM_DEVICE_BINDING_NAME_CAP
        && str::from_utf8(value).is_ok()
        && value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-'))
}

fn build_device_binding_alias(
    service: &[u8],
    binding: &[u8],
    out: &mut [u8; SQVM_DEVICE_BINDING_NAME_CAP],
) -> Option<usize> {
    let len = service.len().checked_add(1)?.checked_add(binding.len())?;
    if len >= out.len() {
        return None;
    }
    out[..service.len()].copy_from_slice(service);
    out[service.len()] = b'.';
    out[service.len() + 1..len].copy_from_slice(binding);
    Some(len)
}

fn supports_package_device_binding(service: &[u8], binding: &[u8]) -> bool {
    matches!((service, binding), (b"indicator", b"default")) || service == b"display"
}

fn parse_inline_gpio_resource(resource: &[u8]) -> Option<&[u8]> {
    let pin_name = resource.strip_prefix(b"gpio:")?;
    let digits = pin_name.strip_prefix(b"GPIO")?;
    if digits.is_empty() || digits.len() > 2 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(pin_name)
}

fn parse_inline_gpio_button_resource(resource: &[u8]) -> Option<(&[u8], &[u8], bool)> {
    let rest = resource.strip_prefix(b"gpio-button:")?;
    let (pin_name, rest) = split_once_byte(rest, b':')?;
    let (event, polarity) = split_once_byte(rest, b':')?;
    let digits = pin_name.strip_prefix(b"GPIO")?;
    if digits.is_empty() || digits.len() > 2 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if !valid_logical_key_event(event) {
        return None;
    }
    let active_low = match polarity {
        b"activeLow" => true,
        b"activeHigh" => false,
        _ => return None,
    };
    Some((pin_name, event, active_low))
}

fn split_once_byte(input: &[u8], delimiter: u8) -> Option<(&[u8], &[u8])> {
    let index = input.iter().position(|byte| *byte == delimiter)?;
    Some((&input[..index], &input[index + 1..]))
}

fn valid_logical_key_event(event: &[u8]) -> bool {
    let Some(key) = event.strip_prefix(b"key.") else {
        return false;
    };
    matches!(
        key,
        b"UP" | b"DOWN" | b"LEFT" | b"RIGHT" | b"SELECT" | b"BACK" | b"MENU" | b"HOME" | b"POWER"
    )
}

fn build_inline_gpio_config(alias: &[u8], pin_name: &[u8], out: &mut SqdcConfig) -> SqdcStatus {
    *out = SqdcConfig::default();
    for (key, value) in [
        (b"service".as_slice(), alias),
        (b"mode".as_slice(), b"gpio".as_slice()),
        (b"pinName".as_slice(), pin_name),
    ] {
        let value = match sqdc_string_value(value) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let status = config_set_value(out, key, value);
        if status != SqdcStatus::Ok {
            return status;
        }
    }
    config_set_value(
        out,
        b"activeLow",
        SqdcValue {
            kind: SqdcValueKind::Bool,
            bool_value: false,
            ..SqdcValue::default()
        },
    )
}

fn build_inline_gpio_button_config(
    alias: &[u8],
    pin_name: &[u8],
    event: &[u8],
    active_low: bool,
    out: &mut SqdcConfig,
) -> SqdcStatus {
    *out = SqdcConfig::default();
    for (key, value) in [
        (b"service".as_slice(), alias),
        (b"mode".as_slice(), b"gpio-button".as_slice()),
        (b"pinName".as_slice(), pin_name),
        (b"event".as_slice(), event),
    ] {
        let value = match sqdc_string_value(value) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let status = config_set_value(out, key, value);
        if status != SqdcStatus::Ok {
            return status;
        }
    }
    config_set_value(
        out,
        b"activeLow",
        SqdcValue {
            kind: SqdcValueKind::Bool,
            bool_value: active_low,
            ..SqdcValue::default()
        },
    )
}

fn sqdc_string_value(value: &[u8]) -> Result<SqdcValue, SqdcStatus> {
    if value.len() > SQDC_CONFIG_STRING_CAP || str::from_utf8(value).is_err() {
        return Err(SqdcStatus::InvalidArgument);
    }
    let mut stored = SqdcValue {
        kind: SqdcValueKind::String,
        string_len: value.len(),
        ..SqdcValue::default()
    };
    stored.string[..value.len()].copy_from_slice(value);
    Ok(stored)
}

fn encode_sqdc_bytes(config: &SqdcConfig, out: &mut [u8], out_len: &mut usize) -> SqdcStatus {
    let mut cursor = 0usize;
    if write_bytes(out, &mut cursor, SQDC_MAGIC) != SqdcStatus::Ok
        || write_u16_bytes(out, &mut cursor, config.count as u16) != SqdcStatus::Ok
    {
        return SqdcStatus::BufferTooSmall;
    }
    for record in &config.records[..config.count] {
        if !record.present || !valid_sqdc_key(&record.key[..record.key_len]) {
            return SqdcStatus::InvalidArgument;
        }
        if write_len_bytes(out, &mut cursor, &record.key[..record.key_len]) != SqdcStatus::Ok {
            return SqdcStatus::BufferTooSmall;
        }
        let status = match record.value.kind {
            SqdcValueKind::Null => write_u8(out, &mut cursor, SQDC_TAG_NULL),
            SqdcValueKind::Bool => {
                let status = write_u8(out, &mut cursor, SQDC_TAG_BOOL);
                if status == SqdcStatus::Ok {
                    write_u8(out, &mut cursor, u8::from(record.value.bool_value))
                } else {
                    status
                }
            }
            SqdcValueKind::I32 => {
                let status = write_u8(out, &mut cursor, SQDC_TAG_I32);
                if status == SqdcStatus::Ok {
                    write_bytes(out, &mut cursor, &record.value.i32_value.to_le_bytes())
                } else {
                    status
                }
            }
            SqdcValueKind::String => {
                if record.value.string_len > SQDC_CONFIG_STRING_CAP {
                    return SqdcStatus::InvalidArgument;
                }
                let status = write_u8(out, &mut cursor, SQDC_TAG_STRING);
                if status == SqdcStatus::Ok {
                    write_len_bytes(
                        out,
                        &mut cursor,
                        &record.value.string[..record.value.string_len],
                    )
                } else {
                    status
                }
            }
        };
        if status != SqdcStatus::Ok {
            return status;
        }
    }
    *out_len = cursor;
    SqdcStatus::Ok
}

fn decode_sqdc_bytes(input: &[u8], out: &mut SqdcConfig) -> SqdcStatus {
    *out = SqdcConfig::default();
    if input.len() < 6 || &input[..4] != SQDC_MAGIC {
        return SqdcStatus::ParseError;
    }
    let Some(count) = read_u16_bytes(input, 4) else {
        return SqdcStatus::ParseError;
    };
    let mut cursor = 6usize;
    for _ in 0..count {
        let Some(key) = read_len_bytes(input, &mut cursor) else {
            return SqdcStatus::ParseError;
        };
        let Some(tag) = input.get(cursor).copied() else {
            return SqdcStatus::ParseError;
        };
        cursor += 1;
        let value = match tag {
            SQDC_TAG_NULL => SqdcValue::default(),
            SQDC_TAG_BOOL => {
                let Some(value) = input.get(cursor).copied() else {
                    return SqdcStatus::ParseError;
                };
                cursor += 1;
                SqdcValue {
                    kind: SqdcValueKind::Bool,
                    bool_value: value != 0,
                    ..SqdcValue::default()
                }
            }
            SQDC_TAG_I32 => {
                let Some(bytes) = input.get(cursor..cursor + 4) else {
                    return SqdcStatus::ParseError;
                };
                cursor += 4;
                SqdcValue {
                    kind: SqdcValueKind::I32,
                    i32_value: i32::from_le_bytes(bytes.try_into().unwrap()),
                    ..SqdcValue::default()
                }
            }
            SQDC_TAG_STRING => {
                let Some(value) = read_len_bytes(input, &mut cursor) else {
                    return SqdcStatus::ParseError;
                };
                if value.len() > SQDC_CONFIG_STRING_CAP || str::from_utf8(value).is_err() {
                    return SqdcStatus::ParseError;
                }
                let mut stored = SqdcValue {
                    kind: SqdcValueKind::String,
                    string_len: value.len(),
                    ..SqdcValue::default()
                };
                stored.string[..value.len()].copy_from_slice(value);
                stored
            }
            _ => return SqdcStatus::ParseError,
        };
        let status = config_set_new_value(out, key, value);
        if status != SqdcStatus::Ok {
            return status;
        }
    }
    if cursor == input.len() {
        SqdcStatus::Ok
    } else {
        SqdcStatus::ParseError
    }
}

fn is_safe_sqdevice_path_bytes(path: &[u8]) -> bool {
    if path.is_empty()
        || path.starts_with(b"/")
        || path.starts_with(b"sd/")
        || path.starts_with(b"system/")
        || path.iter().any(|byte| *byte == b'\\')
        || !path.ends_with(b".sqdevice")
    {
        return false;
    }
    path.split(|byte| *byte == b'/')
        .all(|part| !part.is_empty() && part != b"." && part != b"..")
}

fn trim_ascii(mut input: &[u8]) -> &[u8] {
    if input.ends_with(b"\r") {
        input = &input[..input.len() - 1];
    }
    while matches!(input.first(), Some(b' ' | b'\t')) {
        input = &input[1..];
    }
    while matches!(input.last(), Some(b' ' | b'\t')) {
        input = &input[..input.len() - 1];
    }
    input
}

fn split_ascii_word(input: &[u8]) -> Option<(&[u8], &[u8])> {
    let input = trim_ascii(input);
    let split = input.iter().position(|byte| byte.is_ascii_whitespace())?;
    Some((&input[..split], trim_ascii(&input[split..])))
}

fn parse_i32_ascii(input: &[u8]) -> Result<i32, SqdcStatus> {
    if input.is_empty() {
        return Err(SqdcStatus::ParseError);
    }
    let negative = input[0] == b'-';
    let digits = if negative { &input[1..] } else { input };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err(SqdcStatus::ParseError);
    }
    let mut value: i64 = 0;
    for digit in digits {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add((digit - b'0') as i64))
            .ok_or(SqdcStatus::ParseError)?;
    }
    if negative {
        let value = value.checked_neg().ok_or(SqdcStatus::ParseError)?;
        i32::try_from(value).map_err(|_| SqdcStatus::ParseError)
    } else {
        i32::try_from(value).map_err(|_| SqdcStatus::ParseError)
    }
}

fn parse_usize_ascii(input: &[u8]) -> Result<usize, SqdcStatus> {
    if input.is_empty() || !input.iter().all(u8::is_ascii_digit) {
        return Err(SqdcStatus::ParseError);
    }
    let mut value = 0usize;
    for digit in input {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add((digit - b'0') as usize))
            .ok_or(SqdcStatus::ParseError)?;
    }
    Ok(value)
}

fn write_u8(out: &mut [u8], cursor: &mut usize, value: u8) -> SqdcStatus {
    if *cursor >= out.len() {
        return SqdcStatus::BufferTooSmall;
    }
    out[*cursor] = value;
    *cursor += 1;
    SqdcStatus::Ok
}

fn write_u16_bytes(out: &mut [u8], cursor: &mut usize, value: u16) -> SqdcStatus {
    write_bytes(out, cursor, &value.to_le_bytes())
}

fn write_len_bytes(out: &mut [u8], cursor: &mut usize, value: &[u8]) -> SqdcStatus {
    let Ok(len) = u16::try_from(value.len()) else {
        return SqdcStatus::BufferTooSmall;
    };
    let status = write_u16_bytes(out, cursor, len);
    if status == SqdcStatus::Ok {
        write_bytes(out, cursor, value)
    } else {
        status
    }
}

fn write_bytes(out: &mut [u8], cursor: &mut usize, bytes: &[u8]) -> SqdcStatus {
    let Some(end) = cursor.checked_add(bytes.len()) else {
        return SqdcStatus::BufferTooSmall;
    };
    let Some(dest) = out.get_mut(*cursor..end) else {
        return SqdcStatus::BufferTooSmall;
    };
    dest.copy_from_slice(bytes);
    *cursor = end;
    SqdcStatus::Ok
}

fn read_u16_bytes(input: &[u8], offset: usize) -> Option<u16> {
    let bytes = input.get(offset..offset + 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

fn read_len_bytes<'a>(input: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    let len = read_u16_bytes(input, *cursor)? as usize;
    *cursor = (*cursor).checked_add(2)?;
    let end = (*cursor).checked_add(len)?;
    let bytes = input.get(*cursor..end)?;
    *cursor = end;
    Some(bytes)
}

fn callback_status(status: i32) -> Result<(), VmError> {
    if status == 0 {
        Ok(())
    } else {
        Err(VmError::InvalidOperand)
    }
}

unsafe fn optional_ffi_str<'a>(ptr: *const u8, len: usize) -> Result<Option<&'a str>, VmError> {
    if len == 0 {
        return Ok(None);
    }
    if ptr.is_null() {
        return Err(VmError::InvalidOperand);
    }
    str::from_utf8(slice::from_raw_parts(ptr, len))
        .map(Some)
        .map_err(|_| VmError::InvalidUtf8)
}

unsafe fn required_ffi_str<'a>(ptr: *const u8, len: usize) -> Result<&'a str, VmError> {
    optional_ffi_str(ptr, len)?.ok_or(VmError::InvalidOperand)
}

unsafe fn app_registry_entry_from_ffi<'a>(
    entry: &SqvmAppRegistryEntry,
) -> Result<AppRegistryEntry<'a>, VmError> {
    Ok(AppRegistryEntry {
        id: required_ffi_str(entry.id, entry.id_len)?,
        name: optional_ffi_str(entry.name, entry.name_len)?.unwrap_or(""),
        build: optional_ffi_str(entry.build, entry.build_len)?.unwrap_or(""),
        description: optional_ffi_str(entry.description, entry.description_len)?.unwrap_or(""),
    })
}

unsafe fn app_armed_stack_entry_from_ffi<'a>(
    entry: &SqvmAppStackEntry,
) -> Result<AppArmedStackEntry<'a>, VmError> {
    Ok(AppArmedStackEntry {
        app_id: required_ffi_str(entry.app_id, entry.app_id_len)?,
        event: required_ffi_str(entry.event, entry.event_len)?,
    })
}

unsafe fn wifi_status_from_ffi<'a>(status: &SqvmWifiStatus) -> Result<WifiStatus<'a>, VmError> {
    Ok(WifiStatus {
        active: status.active,
        mode: optional_ffi_str(status.mode, status.mode_len)?,
        ip_address: optional_ffi_str(status.ip_address, status.ip_address_len)?,
        ssid: optional_ffi_str(status.ssid, status.ssid_len)?,
        clients: status.clients,
        error: optional_ffi_str(status.error, status.error_len)?,
        state: required_ffi_str(status.state, status.state_len)?,
        backend: required_ffi_str(status.backend, status.backend_len)?,
        driver_started: status.driver_started,
        configured: status.configured,
        driver_mode: optional_ffi_str(status.driver_mode, status.driver_mode_len)?,
        channel: status.channel,
        ap_start_events: status.ap_start_events,
        ap_stop_events: status.ap_stop_events,
        probe_events: status.probe_events,
        sta_connected_events: status.sta_connected_events,
        sta_disconnected_events: status.sta_disconnected_events,
        last_backend_code: optional_ffi_str(
            status.last_backend_code,
            status.last_backend_code_len,
        )?,
        profile: optional_ffi_str(status.profile, status.profile_len)?,
        connected: status.connected,
        scan_matches: status.scan_matches,
        rssi: status.rssi,
        auth: optional_ffi_str(status.auth, status.auth_len)?,
        bssid: optional_ffi_str(status.bssid, status.bssid_len)?,
        disconnect_reason: optional_ffi_str(
            status.disconnect_reason,
            status.disconnect_reason_len,
        )?,
        disconnect_reason_code: status.disconnect_reason_code,
    })
}

unsafe fn wifi_action_result_from_ffi<'a>(
    result: &SqvmWifiActionResult,
) -> Result<WifiActionResult<'a>, VmError> {
    Ok(WifiActionResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
    })
}

unsafe fn device_config_result_from_ffi<'a>(
    result: &SqvmDeviceConfigResult,
) -> Result<DeviceConfigResult<'a>, VmError> {
    Ok(DeviceConfigResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        warning: optional_ffi_str(result.warning, result.warning_len)?,
    })
}

unsafe fn content_pick_file_result_from_ffi<'a>(
    result: &SqvmContentPickFileResult,
) -> Result<ContentPickFileResult<'a>, VmError> {
    Ok(ContentPickFileResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        path: optional_ffi_str(result.path, result.path_len)?,
    })
}

unsafe fn content_read_text_result_from_ffi<'a>(
    result: &SqvmContentReadTextResult,
) -> Result<ContentReadTextResult<'a>, VmError> {
    Ok(ContentReadTextResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        text: optional_ffi_str(result.text, result.text_len)?,
    })
}

unsafe fn content_read_lines_result_from_ffi<'a>(
    result: &SqvmContentReadLinesResult,
) -> Result<ContentReadLinesResult<'a>, VmError> {
    Ok(ContentReadLinesResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        lines: &[],
    })
}

fn device_config_value_to_ffi(
    value: Value,
    strings: &StringResolver<'_>,
) -> Result<SqvmDeviceConfigValue, VmError> {
    match value {
        Value::Null => Ok(SqvmDeviceConfigValue {
            kind: SqvmDeviceConfigValueKind::Null,
            ..SqvmDeviceConfigValue::default()
        }),
        Value::Bool(value) => Ok(SqvmDeviceConfigValue {
            kind: SqvmDeviceConfigValueKind::Bool,
            bool_value: value,
            ..SqvmDeviceConfigValue::default()
        }),
        Value::I32(value) => Ok(SqvmDeviceConfigValue {
            kind: SqvmDeviceConfigValueKind::I32,
            i32_value: value,
            ..SqvmDeviceConfigValue::default()
        }),
        Value::String(_) | Value::RuntimeString(_) => {
            let text = strings.value_str(value)?;
            Ok(SqvmDeviceConfigValue {
                kind: SqvmDeviceConfigValueKind::String,
                string: text.as_ptr(),
                string_len: text.len(),
                ..SqvmDeviceConfigValue::default()
            })
        }
        Value::Record(_) | Value::List(_) => Err(VmError::InvalidOperand),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_dispatch_host_keeps_input_dispatch_stack_scratch_bounded() {
        assert!(
            size_of::<FfiHost>() <= 2048,
            "FfiHost is {} bytes and is placed on the VM worker stack for input app dispatch",
            size_of::<FfiHost>()
        );
    }

    #[test]
    #[cfg(target_pointer_width = "32")]
    fn sqvm_context_still_fits_zephyr_runtime_context_buffer() {
        const ZEPHYR_RUNTIME_CONTEXT_BYTES: usize = 12_032;
        assert!(
            sqvm_context_size() <= ZEPHYR_RUNTIME_CONTEXT_BYTES,
            "SqvmContext is {} bytes and must fit firmware/zephyr/src/vm_runtime.h context_words",
            sqvm_context_size()
        );
    }
}

unsafe fn wifi_ap_ip_from_ffi<'a>(result: &SqvmWifiApIp) -> Result<WifiApIp<'a>, VmError> {
    Ok(WifiApIp {
        ip: optional_ffi_str(result.ip, result.ip_len)?,
        gw: optional_ffi_str(result.gw, result.gw_len)?,
        netmask: optional_ffi_str(result.netmask, result.netmask_len)?,
        error: optional_ffi_str(result.error, result.error_len)?,
    })
}

fn wifi_access_point_from_ffi(network: &SqvmWifiAccessPoint) -> Result<WifiAccessPoint, VmError> {
    let ssid = unsafe {
        if network.ssid_len == 0 {
            &[][..]
        } else if network.ssid.is_null() {
            return Err(VmError::InvalidOperand);
        } else {
            slice::from_raw_parts(network.ssid, network.ssid_len)
        }
    };
    let bssid = if network.bssid_len == 0 {
        None
    } else {
        Some(parse_bssid_text(unsafe {
            required_ffi_str(network.bssid, network.bssid_len)?
        })?)
    };
    WifiAccessPoint::new(
        ssid,
        bssid,
        network.channel,
        network.rssi,
        wifi_auth_static(unsafe { optional_ffi_str(network.auth, network.auth_len)? }),
        network.hidden,
    )
}

fn wifi_auth_static(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("open") => Some("open"),
        Some("wep") => Some("wep"),
        Some("wpa") => Some("wpa"),
        Some("wpa2") => Some("wpa2"),
        Some("wpa3") => Some("wpa3"),
        Some("unknown") => Some("unknown"),
        _ => None,
    }
}

fn parse_bssid_text(value: &str) -> Result<[u8; 6], VmError> {
    let bytes = value.as_bytes();
    if bytes.len() != 17 {
        return Err(VmError::InvalidOperand);
    }
    let mut out = [0u8; 6];
    let mut index = 0usize;
    while index < 6 {
        let pos = index * 3;
        if index > 0 && bytes[pos - 1] != b':' {
            return Err(VmError::InvalidOperand);
        }
        out[index] = (hex_nibble(bytes[pos])? << 4) | hex_nibble(bytes[pos + 1])?;
        index += 1;
    }
    Ok(out)
}

fn hex_nibble(value: u8) -> Result<u8, VmError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(VmError::InvalidOperand),
    }
}

fn option_ptr(value: Option<&str>) -> *const u8 {
    value.map(|value| value.as_ptr()).unwrap_or(ptr::null())
}

fn option_len(value: Option<&str>) -> usize {
    value.map(|value| value.len()).unwrap_or(0)
}

fn write_value<const N: usize>(
    line: &mut FixedLine<N>,
    strings: &StringResolver<'_>,
    value: Value,
) {
    match value {
        Value::String(_) | Value::RuntimeString(_) => {
            let text = strings.value_str(value).unwrap_or("<string>");
            let _ = line.write_str(text);
        }
        Value::I32(value) => {
            let _ = write!(line, "{value}");
        }
        Value::Bool(value) => {
            let _ = write!(line, "{value}");
        }
        Value::Null => {
            let _ = line.write_str("null");
        }
        Value::Record(_) => {
            let _ = line.write_str("<record>");
        }
        Value::List(_) => {
            let _ = line.write_str("<list>");
        }
    }
}

struct FixedLine<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Default for FixedLine<N> {
    fn default() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
}

impl<const N: usize> FixedLine<N> {
    fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.bytes.as_mut_ptr()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn cap(&self) -> usize {
        N
    }

    fn set_len(&mut self, len: usize) -> Result<(), VmError> {
        if len > N {
            return Err(VmError::InvalidOperand);
        }
        self.len = len;
        Ok(())
    }

    fn as_str(&self) -> Result<&str, VmError> {
        str::from_utf8(&self.bytes[..self.len]).map_err(|_| VmError::InvalidUtf8)
    }
}

impl<const N: usize> Write for FixedLine<N> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = N.saturating_sub(self.len);
        if remaining == 0 {
            return Ok(());
        }
        let copy_len = core::cmp::min(remaining, value.len());
        self.bytes[self.len..self.len + copy_len].copy_from_slice(&value.as_bytes()[..copy_len]);
        self.len += copy_len;
        Ok(())
    }
}

fn core_storage_completion(
    completion: &SqvmStorageCompletion,
) -> Result<CoreStorageCompletion<'_>, VmError> {
    if completion.has_len {
        let bytes = completion
            .bytes
            .get(..completion.len)
            .ok_or(VmError::InvalidStateRecord)?;
        CoreStorageCompletion::bytes(bytes)
    } else {
        Ok(CoreStorageCompletion::empty())
    }
}

unsafe fn write_dispatch_result(
    out_result: *mut SqvmDispatchResult,
    exited: bool,
    result: Result<VmDispatch, VmError>,
) -> SqvmStatus {
    let out = &mut *out_result;
    match result {
        Ok(VmDispatch::Complete) => {
            *out = SqvmDispatchResult {
                exited,
                ..SqvmDispatchResult::default()
            };
            SqvmStatus::Ok
        }
        Ok(VmDispatch::PendingStorage(request)) => {
            *out = SqvmDispatchResult {
                status: SqvmStatus::Ok,
                outcome: SqvmDispatchOutcome::PendingStorage,
                exited,
                storage: storage_request_from_core(request),
            };
            SqvmStatus::Ok
        }
        Err(_) => {
            *out = SqvmDispatchResult {
                status: SqvmStatus::VmError,
                outcome: SqvmDispatchOutcome::Complete,
                exited,
                storage: SqvmStorageRequest::default(),
            };
            SqvmStatus::VmError
        }
    }
}

fn storage_request_from_core(request: StorageRequest) -> SqvmStorageRequest {
    let mut out = SqvmStorageRequest::default();
    match request {
        StorageRequest::SqbcRead { offset, len } => {
            out.kind = SqvmStorageRequestKind::SqbcRead;
            out.offset = offset;
            out.len = len;
        }
        StorageRequest::StateLoad => {
            out.kind = SqvmStorageRequestKind::StateLoad;
        }
        StorageRequest::StateSave { len, bytes } => {
            out.kind = SqvmStorageRequestKind::StateSave;
            out.len = len;
            let bytes = unsafe { slice::from_raw_parts(bytes, len) };
            out.bytes[..len].copy_from_slice(bytes);
        }
        StorageRequest::StateReset => {
            out.kind = SqvmStorageRequestKind::StateReset;
        }
    }
    out
}

fn set_c_string<const N: usize>(out: &mut [u8; N], bytes: &[u8]) -> SqdpStatus {
    if bytes.is_empty() || bytes.len() >= N {
        return SqdpStatus::InvalidArgument;
    }
    *out = [0; N];
    out[..bytes.len()].copy_from_slice(bytes);
    SqdpStatus::Ok
}

fn c_string_bytes(bytes: &[u8]) -> &[u8] {
    let len = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    &bytes[..len]
}

fn fixed_line_bytes(bytes: &[u8], index: usize, stride: usize) -> &[u8] {
    let start = index * stride;
    c_string_bytes(&bytes[start..start + stride])
}

fn session_crc_matches(session: &SqdpTransferSession) -> bool {
    !session.running_crc == session.expected_crc
}

fn resource_crc_matches(session: &SqdpResourceSession) -> bool {
    !session.running_crc == session.expected_crc
}

fn field_bytes(payload: &[u8], tag: u8, field_type: u8) -> Option<&[u8]> {
    let mut offset = 0usize;
    while offset < payload.len() {
        if payload.len().saturating_sub(offset) < 4 {
            return None;
        }
        let current_tag = payload[offset];
        let current_type = payload[offset + 1];
        let len = u16::from_le_bytes([payload[offset + 2], payload[offset + 3]]) as usize;
        let value_start = offset + 4;
        let value_end = value_start.checked_add(len)?;
        if value_end > payload.len() {
            return None;
        }
        if current_tag == tag && current_type == field_type {
            return Some(&payload[value_start..value_end]);
        }
        offset = value_end;
    }
    None
}

fn next_tlv_field(payload: &[u8], offset: usize) -> Option<(u8, u8, &[u8], usize)> {
    if payload.len().saturating_sub(offset) < 4 {
        return None;
    }
    let tag = payload[offset];
    let field_type = payload[offset + 1];
    let len = u16::from_le_bytes([payload[offset + 2], payload[offset + 3]]) as usize;
    let value_start = offset.checked_add(4)?;
    let value_end = value_start.checked_add(len)?;
    if value_end > payload.len() {
        return None;
    }
    Some((tag, field_type, &payload[value_start..value_end], value_end))
}

fn field_u64(payload: &[u8], tag: u8) -> Option<u64> {
    let bytes = field_bytes(payload, tag, 5)?;
    if bytes.len() != 8 {
        return None;
    }
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

fn sqdp_crc32_update(crc: u32, bytes: &[u8]) -> u32 {
    let mut crc = crc;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    crc
}

#[cfg(feature = "zephyr")]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
