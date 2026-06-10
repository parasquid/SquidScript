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
        AppArmedStack, AppArmedStackEntry, AppInstallResult, AppProcessStack, AppRegistryEntry,
        AppRegistryList, BinBookInfoResult, BinBookOpenResult, BinBookReadPageResult,
        ContentBinBookEntry, ContentBinBookListResult, DeviceConfigResult, DisplayInfo,
        DisplayLineOptions, DisplayRectOptions, DisplayResourceOptions, DisplayTextOptions,
        FilePickFileResult, FileReadLinesResult, FileReadTextResult,
        StorageCompletion as CoreStorageCompletion, StorageRequest, TraceSink, VmDispatch,
        WifiAccessPoint, WifiApIp, WifiOperation, WifiOperationResult, WifiScanNetwork, WifiStatus,
        MAX_STORAGE_TRANSFER_BYTES,
    },
    limits::{MAX_APP_BYTES, MAX_CODE_CHUNK_BYTES, MAX_SAVED_STATE_BYTES},
    program::{Program, ProgramIndex, SqbcSection},
    reader::{SliceSqbcReader, SqbcReader},
    strings::StringResolver,
    value::{Handle, HandleKind, Value},
    vm::{ChunkedVm, EventPayload, EventPayloadField},
};

const SECTION_STRINGS: u16 = 1;
const SECTION_BLE_TRIGGERS: u16 = 10;

use squid_device_protocol::{
    encode_app_list_response_into, encode_empty_response_into, encode_error_response_into,
    encode_hello_response_into, encode_lifecycle_response_into, encode_line_response_into,
    encode_resources_response_into, encode_state_response_into, key_event_from_request_into,
    AppListEntry, DecodeError, DeviceRequest, LifecycleTimer, Opcode, ResourceMetric,
    Status as SqdpFrameStatus, MAX_APP_BYTES as SQDP_MAX_APP_BYTES,
    MAX_RESOURCE_BYTES as SQDP_MAX_RESOURCE_BYTES,
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
pub const SQDC_CONFIG_MAX_RECORDS: usize = 5;
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
    pub sqbc_len: u32,
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

#[derive(Clone, Copy)]
struct RuntimeLifecycleTimerIter {
    base: *const u8,
    count: usize,
    stride: usize,
    active_offset: usize,
    app_id_offset: usize,
    app_id_cap: usize,
    event_offset: usize,
    event_cap: usize,
    index: usize,
}

impl Iterator for RuntimeLifecycleTimerIter {
    type Item = LifecycleTimer<'static>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.count {
            let index = self.index;
            self.index += 1;
            unsafe {
                let slot = self.base.add(index.saturating_mul(self.stride));
                if ptr::read(slot.add(self.active_offset)) == 0 {
                    continue;
                }
                let app_id = slice::from_raw_parts(slot.add(self.app_id_offset), self.app_id_cap);
                let event = slice::from_raw_parts(slot.add(self.event_offset), self.event_cap);
                return Some(LifecycleTimer {
                    app_id: str::from_utf8(c_string_bytes(app_id))
                        .expect("validated runtime timer app id utf-8 before encoding"),
                    event: str::from_utf8(c_string_bytes(event))
                        .expect("validated runtime timer event utf-8 before encoding"),
                });
            }
        }
        None
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

pub const SQVM_BLE_PROFILE_TEXT_CAP: usize = 32;
pub const SQVM_BLE_PROFILE_ACCEPT_MAX: usize = 4;
pub const SQVM_BLE_PROFILE_EVENT_MAX: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmBleProfileEventRoute {
    pub kind: [u8; SQVM_BLE_PROFILE_TEXT_CAP],
    pub event: [u8; SQVM_BLE_PROFILE_TEXT_CAP],
}

impl Default for SqvmBleProfileEventRoute {
    fn default() -> Self {
        Self {
            kind: [0; SQVM_BLE_PROFILE_TEXT_CAP],
            event: [0; SQVM_BLE_PROFILE_TEXT_CAP],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmBleProfileTrigger {
    pub profile: [u8; SQVM_BLE_PROFILE_TEXT_CAP],
    pub id: [u8; SQVM_BLE_PROFILE_TEXT_CAP],
    pub role: [u8; SQVM_BLE_PROFILE_TEXT_CAP],
    pub accept_count: usize,
    pub accept: [[u8; SQVM_BLE_PROFILE_TEXT_CAP]; SQVM_BLE_PROFILE_ACCEPT_MAX],
    pub event_count: usize,
    pub events: [SqvmBleProfileEventRoute; SQVM_BLE_PROFILE_EVENT_MAX],
}

pub const SQVM_EVENT_PAYLOAD_FIELD_MAX: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SqvmEventPayloadField {
    pub name: *const u8,
    pub name_len: usize,
    pub value: *const u8,
    pub value_len: usize,
}

impl Default for SqvmEventPayloadField {
    fn default() -> Self {
        Self {
            name: ptr::null(),
            name_len: 0,
            value: ptr::null(),
            value_len: 0,
        }
    }
}

impl Default for SqvmBleProfileTrigger {
    fn default() -> Self {
        Self {
            profile: [0; SQVM_BLE_PROFILE_TEXT_CAP],
            id: [0; SQVM_BLE_PROFILE_TEXT_CAP],
            role: [0; SQVM_BLE_PROFILE_TEXT_CAP],
            accept_count: 0,
            accept: [[0; SQVM_BLE_PROFILE_TEXT_CAP]; SQVM_BLE_PROFILE_ACCEPT_MAX],
            event_count: 0,
            events: [SqvmBleProfileEventRoute::default(); SQVM_BLE_PROFILE_EVENT_MAX],
        }
    }
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
pub enum SqvmHandleKind {
    None = 0,
    BinBook = 1,
    Drawable = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmHandle {
    pub kind: SqvmHandleKind,
    pub id: u16,
}

impl Default for SqvmHandle {
    fn default() -> Self {
        Self {
            kind: SqvmHandleKind::None,
            id: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmDisplayInfo {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub warning: *const u8,
    pub warning_len: usize,
    pub available: bool,
    pub status: *const u8,
    pub status_len: usize,
    pub binding: *const u8,
    pub binding_len: usize,
    pub driver: *const u8,
    pub driver_len: usize,
    pub transport: *const u8,
    pub transport_len: usize,
    pub width: i32,
    pub height: i32,
    pub physical_width: i32,
    pub physical_height: i32,
    pub rotation: i32,
    pub color_model: *const u8,
    pub color_model_len: usize,
    pub logical_gray_levels: i32,
    pub native_bpp: i32,
    pub native_pixel_format: *const u8,
    pub native_pixel_format_len: usize,
    pub default_font_height: i32,
    pub supports_partial_refresh: bool,
    pub supports_fast_refresh: bool,
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
pub struct SqvmWifiOperation {
    pub active: bool,
    pub kind: *const u8,
    pub kind_len: usize,
    pub state: *const u8,
    pub state_len: usize,
    pub done: bool,
    pub cancelled: bool,
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmWifiOperationResult {
    pub ready: bool,
    pub kind: *const u8,
    pub kind_len: usize,
    pub state: *const u8,
    pub state_len: usize,
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub cancelled: bool,
    pub count: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmWifiScanNetworkResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub network: SqvmWifiAccessPoint,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmFilePickFileResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub path: *const u8,
    pub path_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmFileReadTextResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub text: *const u8,
    pub text_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmFileReadLinesResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmBinBookOpenResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub book: SqvmHandle,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmBinBookInfoResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub title: *const u8,
    pub title_len: usize,
    pub page_count: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmBinBookReadPageResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub drawable: SqvmHandle,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmContentBinBookEntry {
    pub name: *const u8,
    pub name_len: usize,
    pub reference: *const u8,
    pub reference_len: usize,
    pub size: i32,
}

impl Default for SqvmContentBinBookEntry {
    fn default() -> Self {
        Self {
            name: ptr::null(),
            name_len: 0,
            reference: ptr::null(),
            reference_len: 0,
            size: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqvmContentBinBookListResult {
    pub ok: bool,
    pub error: *const u8,
    pub error_len: usize,
    pub warning: *const u8,
    pub warning_len: usize,
    pub count: i32,
    pub has_more: bool,
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

pub type SqvmReadExactAtCallback = Option<
    unsafe extern "C" fn(
        user_data: *mut c_void,
        offset: usize,
        out: *mut u8,
        out_len: usize,
    ) -> i32,
>;

mod generated_callbacks;
mod generated_result_defaults;
pub use generated_callbacks::SqvmCallbacks;

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
pub unsafe extern "C" fn sqvm_context_reset_in_place(
    context: *mut u8,
    context_len: usize,
) -> SqvmStatus {
    if context.is_null() || context_len < size_of::<SqvmContext>() {
        return SqvmStatus::InvalidArgument;
    }

    let context = &mut *context.cast::<SqvmContext>();
    if context.initialized {
        context.vm_ptr().drop_in_place();
        context.initialized = false;
    }
    SqvmStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_context_init_in_place(
    context: *mut SqvmContext,
    user_data: *mut c_void,
    callbacks: *const SqvmCallbacks,
    scratch: *mut u8,
    scratch_len: usize,
) -> SqvmStatus {
    if context.is_null()
        || callbacks.is_null()
        || scratch.is_null()
        || scratch_len < MAX_CODE_CHUNK_BYTES
    {
        return SqvmStatus::InvalidArgument;
    }

    let context = &mut *context;
    if context.initialized {
        context.vm_ptr().drop_in_place();
        context.initialized = false;
    }

    let callbacks = &*callbacks;
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    let mut host = FfiHost::new(user_data, callbacks, false);
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
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(user_data, &callbacks, false);
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
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(user_data, &callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    trigger_timer_read_from_reader(&mut reader, scratch, index, out_timer)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_ble_profile_count(
    sqbc: *const u8,
    sqbc_len: usize,
    out_count: *mut usize,
) -> SqvmStatus {
    if sqbc.is_null() || out_count.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let mut scratch = [0u8; MAX_APP_BYTES];
    let mut reader = SliceSqbcReader::new(slice::from_raw_parts(sqbc, sqbc_len));
    ble_profile_count_from_reader(&mut reader, &mut scratch, out_count)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_ble_profile_read(
    sqbc: *const u8,
    sqbc_len: usize,
    index: usize,
    out_profile: *mut SqvmBleProfileTrigger,
) -> SqvmStatus {
    if sqbc.is_null() || out_profile.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let mut scratch = [0u8; MAX_APP_BYTES];
    let mut reader = SliceSqbcReader::new(slice::from_raw_parts(sqbc, sqbc_len));
    ble_profile_read_from_reader(&mut reader, &mut scratch, index, out_profile)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_ble_profile_count_from_reader(
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
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(user_data, &callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    ble_profile_count_from_reader(&mut reader, scratch, out_count)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_trigger_ble_profile_read_from_reader(
    user_data: *mut c_void,
    read_exact_at: SqvmReadExactAtCallback,
    scratch: *mut u8,
    scratch_len: usize,
    index: usize,
    out_profile: *mut SqvmBleProfileTrigger,
) -> SqvmStatus {
    if read_exact_at.is_none() || scratch.is_null() || out_profile.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let callbacks = SqvmCallbacks {
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(user_data, &callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    ble_profile_read_from_reader(&mut reader, scratch, index, out_profile)
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
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(user_data, &callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    device_binding_count_from_reader(&mut reader, scratch, out_count)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_event_handler_exists_from_reader(
    user_data: *mut c_void,
    read_exact_at: SqvmReadExactAtCallback,
    scratch: *mut u8,
    scratch_len: usize,
    event: *const u8,
    event_len: usize,
    out_exists: *mut bool,
) -> SqvmStatus {
    if read_exact_at.is_none() || scratch.is_null() || event.is_null() || out_exists.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let callbacks = SqvmCallbacks {
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(user_data, &callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    let Ok(event) = str::from_utf8(slice::from_raw_parts(event, event_len)) else {
        return SqvmStatus::InvalidArgument;
    };
    let index = match ProgramIndex::parse_from_reader(&mut reader, scratch) {
        Ok(index) => index,
        Err(_) => return SqvmStatus::VmError,
    };
    let exists = match index.handler_preload(event) {
        Ok(_) => true,
        Err(VmError::HandlerNotFound) => false,
        Err(_) => return SqvmStatus::VmError,
    };
    *out_exists = exists;
    SqvmStatus::Ok
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
        read_exact_at,
        ..SqvmCallbacks::default()
    };
    let mut reader = FfiHost::new(user_data, &callbacks, false);
    let scratch = slice::from_raw_parts_mut(scratch, scratch_len);
    device_binding_read_from_reader(&mut reader, scratch, index, out_binding)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_dispatch(
    context: *mut SqvmContext,
    user_data: *mut c_void,
    callbacks: *const SqvmCallbacks,
    event: *const u8,
    event_len: usize,
) -> SqvmStatus {
    if context.is_null() || callbacks.is_null() || event.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let context = &mut *context;
    if !context.initialized {
        return SqvmStatus::InvalidArgument;
    }
    let Ok(event) = str::from_utf8(slice::from_raw_parts(event, event_len)) else {
        return SqvmStatus::InvalidArgument;
    };
    let callbacks = &*callbacks;
    let vm = &mut *context.vm_ptr();
    let mut host = FfiHost::new(user_data, callbacks, false);
    status_from_vm(vm.dispatch(&mut host, event))
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_dispatch_start_resumable(
    context: *mut SqvmContext,
    user_data: *mut c_void,
    callbacks: *const SqvmCallbacks,
    event: *const u8,
    event_len: usize,
    out_result: *mut SqvmDispatchResult,
) -> SqvmStatus {
    if context.is_null() || callbacks.is_null() || event.is_null() || out_result.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let context = &mut *context;
    if !context.initialized {
        return SqvmStatus::InvalidArgument;
    }
    let Ok(event) = str::from_utf8(slice::from_raw_parts(event, event_len)) else {
        return SqvmStatus::InvalidArgument;
    };
    let callbacks = &*callbacks;
    let vm = &mut *context.vm_ptr();
    let mut host = FfiHost::new(user_data, callbacks, true);
    let result = vm.dispatch_resumable(&mut host, event);
    write_dispatch_result(out_result, vm.exited(), result)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_dispatch_start_resumable_with_payload(
    context: *mut SqvmContext,
    user_data: *mut c_void,
    callbacks: *const SqvmCallbacks,
    event: *const u8,
    event_len: usize,
    payload_fields: *const SqvmEventPayloadField,
    payload_field_count: usize,
    out_result: *mut SqvmDispatchResult,
) -> SqvmStatus {
    if context.is_null()
        || callbacks.is_null()
        || event.is_null()
        || out_result.is_null()
        || (payload_field_count > 0 && payload_fields.is_null())
    {
        return SqvmStatus::InvalidArgument;
    }
    if payload_field_count > SQVM_EVENT_PAYLOAD_FIELD_MAX {
        return SqvmStatus::InvalidArgument;
    }
    let context = &mut *context;
    if !context.initialized {
        return SqvmStatus::InvalidArgument;
    }
    let Ok(event) = str::from_utf8(slice::from_raw_parts(event, event_len)) else {
        return SqvmStatus::InvalidArgument;
    };
    let mut fields = [EventPayloadField {
        name: "",
        value: "",
    }; SQVM_EVENT_PAYLOAD_FIELD_MAX];
    let raw_fields = slice::from_raw_parts(payload_fields, payload_field_count);
    for (index, raw) in raw_fields.iter().enumerate() {
        if raw.name.is_null() || raw.value.is_null() {
            return SqvmStatus::InvalidArgument;
        }
        let Ok(name) = str::from_utf8(slice::from_raw_parts(raw.name, raw.name_len)) else {
            return SqvmStatus::InvalidArgument;
        };
        let Some(name) = payload_field_name(name) else {
            return SqvmStatus::InvalidArgument;
        };
        let Ok(value) = str::from_utf8(slice::from_raw_parts(raw.value, raw.value_len)) else {
            return SqvmStatus::InvalidArgument;
        };
        fields[index] = EventPayloadField { name, value };
    }
    let callbacks = &*callbacks;
    let vm = &mut *context.vm_ptr();
    let mut host = FfiHost::new(user_data, callbacks, true);
    let payload = EventPayload {
        fields: &fields[..payload_field_count],
    };
    let result = vm.dispatch_resumable_with_payload(&mut host, event, Some(payload));
    write_dispatch_result(out_result, vm.exited(), result)
}

#[no_mangle]
pub unsafe extern "C" fn sqvm_dispatch_resume_storage(
    context: *mut SqvmContext,
    user_data: *mut c_void,
    callbacks: *const SqvmCallbacks,
    completion: *const SqvmStorageCompletion,
    out_result: *mut SqvmDispatchResult,
) -> SqvmStatus {
    if context.is_null() || callbacks.is_null() || completion.is_null() || out_result.is_null() {
        return SqvmStatus::InvalidArgument;
    }
    let context = &mut *context;
    if !context.initialized {
        return SqvmStatus::InvalidArgument;
    }
    let Ok(completion) = core_storage_completion(&*completion) else {
        return SqvmStatus::InvalidArgument;
    };
    let callbacks = &*callbacks;
    let vm = &mut *context.vm_ptr();
    let mut host = FfiHost::new(user_data, callbacks, true);
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

#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub unsafe extern "C" fn sqdp_encode_lifecycle_response_from_runtime_timers(
    sequence: u32,
    active_app: *const u8,
    active_app_len: usize,
    process_stack: *const u8,
    process_count: usize,
    process_stride: usize,
    armed_timer_base: *const u8,
    armed_timer_count: usize,
    armed_timer_stride: usize,
    armed_timer_active_offset: usize,
    armed_timer_app_id_offset: usize,
    armed_timer_app_id_cap: usize,
    armed_timer_event_offset: usize,
    armed_timer_event_cap: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> SqdpStatus {
    if out.is_null()
        || out_len.is_null()
        || (active_app.is_null() && active_app_len > 0)
        || (process_stack.is_null() && process_count > 0)
        || (process_count > 0 && process_stride == 0)
        || (armed_timer_base.is_null() && armed_timer_count > 0)
        || (armed_timer_count > 0 && armed_timer_stride == 0)
        || armed_timer_active_offset >= armed_timer_stride
        || armed_timer_app_id_cap == 0
        || armed_timer_event_cap == 0
        || armed_timer_app_id_offset
            .checked_add(armed_timer_app_id_cap)
            .is_none_or(|end| end > armed_timer_stride)
        || armed_timer_event_offset
            .checked_add(armed_timer_event_cap)
            .is_none_or(|end| end > armed_timer_stride)
    {
        return SqdpStatus::InvalidArgument;
    }
    *out_len = 0;
    if process_count > 8 || armed_timer_count > 8 {
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
    for index in 0..process_count {
        if str::from_utf8(fixed_line_bytes(process, index, process_stride)).is_err() {
            return SqdpStatus::InvalidArgument;
        }
    }
    for index in 0..armed_timer_count {
        let slot = armed_timer_base.add(index.saturating_mul(armed_timer_stride));
        if ptr::read(slot.add(armed_timer_active_offset)) == 0 {
            continue;
        }
        let app_id =
            slice::from_raw_parts(slot.add(armed_timer_app_id_offset), armed_timer_app_id_cap);
        let event =
            slice::from_raw_parts(slot.add(armed_timer_event_offset), armed_timer_event_cap);
        if str::from_utf8(c_string_bytes(app_id)).is_err()
            || str::from_utf8(c_string_bytes(event)).is_err()
        {
            return SqdpStatus::InvalidArgument;
        }
    }

    let process_iter = (0..process_count).map(|index| {
        str::from_utf8(fixed_line_bytes(process, index, process_stride))
            .expect("validated process stack utf-8 before encoding")
    });
    let armed_iter = RuntimeLifecycleTimerIter {
        base: armed_timer_base,
        count: armed_timer_count,
        stride: armed_timer_stride,
        active_offset: armed_timer_active_offset,
        app_id_offset: armed_timer_app_id_offset,
        app_id_cap: armed_timer_app_id_cap,
        event_offset: armed_timer_event_offset,
        event_cap: armed_timer_event_cap,
        index: 0,
    };
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
    if request.is_null() || session.is_null() {
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
    if !out_action.is_null() {
        *out_action = SqdpAction {
            kind,
            app_id: session.app_id.as_ptr(),
            app_id_len: c_string_bytes(&session.app_id).len(),
            total_len: session.total_len,
            ..SqdpAction::default()
        };
    }
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
    if request.is_null() || session.is_null() {
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
    if !out_action.is_null() {
        *out_action = SqdpAction {
            kind,
            app_id: session.app_id.as_ptr(),
            app_id_len: c_string_bytes(&session.app_id).len(),
            staging_path: session.staging_path.as_ptr(),
            staging_path_len: c_string_bytes(&session.staging_path).len(),
            total_len: session.total_len,
            ..SqdpAction::default()
        };
    }
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
    if request.is_null() || session.is_null() {
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
        || total_len > SQDP_MAX_RESOURCE_BYTES as u64
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
    if !out_action.is_null() {
        *out_action = SqdpAction {
            kind: SqdpActionKind::BeginResourceInstall,
            app_id: session.app_id.as_ptr(),
            app_id_len: c_string_bytes(&session.app_id).len(),
            resource_path: session.resource_path.as_ptr(),
            resource_path_len: c_string_bytes(&session.resource_path).len(),
            total_len: session.total_len,
            ..SqdpAction::default()
        };
    }
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
    if request.is_null() || session.is_null() {
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
    if !out_action.is_null() {
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
    }
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
const SQVM_FFI_APP_INSTALL_ID_CAP: usize = 40;
const SQVM_FFI_CONTENT_BINBOOK_CAP: usize = 8;

struct FfiHost<'a> {
    user_data: *mut c_void,
    callbacks: &'a SqvmCallbacks,
    defer_sqbc_reads: bool,
    app_registry_entries: [SqvmAppRegistryEntry; 8],
    app_registry_core_entries: [AppRegistryEntry<'static>; 8],
    app_registry_count: usize,
    app_stack_entries: [SqvmAppStackEntry; SQVM_FFI_APP_STACK_CAP],
    app_process_stack_apps: [&'static str; SQVM_FFI_APP_STACK_CAP],
    app_armed_stack_entries: [AppArmedStackEntry<'static>; SQVM_FFI_APP_STACK_CAP],
    app_stack_count: usize,
    app_install_id: [u8; SQVM_FFI_APP_INSTALL_ID_CAP],
    app_install_id_len: usize,
    content_binbook_entries: [SqvmContentBinBookEntry; SQVM_FFI_CONTENT_BINBOOK_CAP],
    content_binbook_core_entries: [ContentBinBookEntry<'static>; SQVM_FFI_CONTENT_BINBOOK_CAP],
    content_binbook_count: usize,
}

impl<'a> FfiHost<'a> {
    fn new(user_data: *mut c_void, callbacks: &'a SqvmCallbacks, defer_sqbc_reads: bool) -> Self {
        Self {
            user_data,
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
            app_install_id: [0; SQVM_FFI_APP_INSTALL_ID_CAP],
            app_install_id_len: 0,
            content_binbook_entries: [SqvmContentBinBookEntry::default();
                SQVM_FFI_CONTENT_BINBOOK_CAP],
            content_binbook_core_entries: [ContentBinBookEntry {
                name: "",
                reference: "",
                size: 0,
            }; SQVM_FFI_CONTENT_BINBOOK_CAP],
            content_binbook_count: 0,
        }
    }
}

impl SqbcReader for FfiHost<'_> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let Some(read_exact_at) = self.callbacks.read_exact_at else {
            return Err(VmError::ReadFailed);
        };
        let status = unsafe { read_exact_at(self.user_data, offset, out.as_mut_ptr(), out.len()) };
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

impl TraceSink for FfiHost<'_> {
    fn trace(&mut self, message: &str) {
        if let Some(trace) = self.callbacks.trace {
            unsafe {
                trace(self.user_data, message.as_ptr(), message.len());
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
                Value::String(_) => {
                    let text = strings.value_str(*value).unwrap_or("<string>");
                    let _ = line.write_str(text);
                }
                Value::I32(value) => {
                    line.write_i32(*value);
                }
                Value::Bool(value) => {
                    line.write_bool(*value);
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
                Value::Handle(_) => {
                    let _ = line.write_str("<handle>");
                }
            }
        }
        unsafe {
            debug_output(self.user_data, line.as_ptr(), line.len());
        }
    }

    fn draw_clear(&mut self, color: &str) {
        if let Some(display_clear) = self.callbacks.display_clear {
            unsafe {
                display_clear(self.user_data, color.as_ptr(), color.len());
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
            display_text(self.user_data, rendered.as_ptr(), rendered.len(), &options);
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
            display_rect(self.user_data, &options);
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
            display_line(self.user_data, &options);
        }
    }

    fn draw_select(&mut self, name: &str) -> Result<(), VmError> {
        let Some(display_select) = self.callbacks.display_select else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { display_select(self.user_data, name.as_ptr(), name.len()) })
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
            display_image(self.user_data, path.as_ptr(), path.len(), &options);
        }
    }

    fn draw_resource(
        &mut self,
        _strings: &StringResolver<'_>,
        drawable: Value,
        options: DisplayResourceOptions,
    ) {
        let Some(display_draw) = self.callbacks.display_draw else {
            return;
        };
        let Value::Handle(handle) = drawable else {
            return;
        };
        let options = SqvmDisplayResourceOptions {
            x: options.x,
            y: options.y,
            w: options.w,
            h: options.h,
        };
        unsafe {
            display_draw(self.user_data, handle_to_ffi(handle), &options);
        }
    }

    fn display_info<'a>(&'a mut self) -> Result<DisplayInfo<'a>, VmError> {
        let Some(display_info) = self.callbacks.display_info else {
            return Ok(DisplayInfo::unsupported());
        };
        let mut out = SqvmDisplayInfo::default();
        callback_status(unsafe { display_info(self.user_data, &mut out) })?;
        unsafe { display_info_from_ffi(&out) }
    }

    fn service_indicator_write(&mut self, value: bool) -> Result<(), VmError> {
        let Some(indicator_write) = self.callbacks.indicator_write else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_write(self.user_data, value) })
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        let Some(indicator_toggle) = self.callbacks.indicator_toggle else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_toggle(self.user_data) })
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        let Some(indicator_read) = self.callbacks.indicator_read else {
            return Err(VmError::InvalidOperand);
        };
        let mut value = false;
        callback_status(unsafe { indicator_read(self.user_data, &mut value) })?;
        Ok(value)
    }

    fn service_indicator_breathe(&mut self) -> Result<(), VmError> {
        let Some(indicator_breathe) = self.callbacks.indicator_breathe else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_breathe(self.user_data) })
    }

    fn service_indicator_blink(&mut self, on_ms: i32, off_ms: i32) -> Result<(), VmError> {
        let Some(indicator_blink) = self.callbacks.indicator_blink else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { indicator_blink(self.user_data, on_ms, off_ms) })
    }

    fn hardware_gpio_write(&mut self, name: &str, value: bool) -> Result<(), VmError> {
        let Some(hardware_gpio_write) = self.callbacks.hardware_gpio_write else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe {
            hardware_gpio_write(self.user_data, name.as_ptr(), name.len(), value)
        })
    }

    fn hardware_gpio_toggle(&mut self, name: &str) -> Result<(), VmError> {
        let Some(hardware_gpio_toggle) = self.callbacks.hardware_gpio_toggle else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { hardware_gpio_toggle(self.user_data, name.as_ptr(), name.len()) })
    }

    fn hardware_gpio_read(&mut self, name: &str) -> Result<bool, VmError> {
        let Some(hardware_gpio_read) = self.callbacks.hardware_gpio_read else {
            return Err(VmError::InvalidOperand);
        };
        let mut value = false;
        callback_status(unsafe {
            hardware_gpio_read(self.user_data, name.as_ptr(), name.len(), &mut value)
        })?;
        Ok(value)
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        let Some(app_launch) = self.callbacks.app_launch else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { app_launch(self.user_data, app.as_ptr(), app.len()) })
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        let Some(app_arm) = self.callbacks.app_arm else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { app_arm(self.user_data, app.as_ptr(), app.len()) })
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        let Some(app_disarm) = self.callbacks.app_disarm else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { app_disarm(self.user_data, app.as_ptr(), app.len()) })
    }

    fn app_install<'b>(
        &'b mut self,
        file_ref: &str,
        app_id: Option<&str>,
    ) -> Result<AppInstallResult<'b>, VmError> {
        let Some(app_install_file) = self.callbacks.app_install_file else {
            return Err(VmError::InvalidOperand);
        };
        let (app_id_ptr, app_id_len) = match app_id {
            Some(app_id) => (app_id.as_ptr(), app_id.len()),
            None => (ptr::null(), 0),
        };
        self.app_install_id = [0; SQVM_FFI_APP_INSTALL_ID_CAP];
        self.app_install_id_len = 0;
        callback_status(unsafe {
            app_install_file(
                self.user_data,
                file_ref.as_ptr(),
                file_ref.len(),
                app_id_ptr,
                app_id_len,
                self.app_install_id.as_mut_ptr(),
                self.app_install_id.len(),
                &mut self.app_install_id_len,
            )
        })?;
        if self.app_install_id_len > self.app_install_id.len() {
            return Err(VmError::InvalidOperand);
        }
        let id = str::from_utf8(&self.app_install_id[..self.app_install_id_len])
            .map_err(|_| VmError::InvalidUtf8)?;
        Ok(AppInstallResult { id })
    }

    fn app_registry_list<'a>(&'a mut self) -> Result<AppRegistryList<'a>, VmError> {
        let Some(app_registry_list) = self.callbacks.app_registry_list else {
            return Err(VmError::InvalidOperand);
        };
        let mut count = 0usize;
        self.app_registry_entries = [SqvmAppRegistryEntry::default(); 8];
        callback_status(unsafe {
            app_registry_list(
                self.user_data,
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
            app_registry_get(self.user_data, app.as_ptr(), app.len(), &mut out)
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
                self.user_data,
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
                self.user_data,
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
            timer_every(self.user_data, event.as_ptr(), event.len(), interval_ms)
        })
    }

    fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
        let Some(timer_after) = self.callbacks.timer_after else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe {
            timer_after(self.user_data, event.as_ptr(), event.len(), delay_ms)
        })
    }

    fn service_ble_start(&mut self, id: &str) -> Result<(), VmError> {
        let Some(ble_start) = self.callbacks.ble_start else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { ble_start(self.user_data, id.as_ptr(), id.len()) })
    }

    fn service_ble_stop(&mut self) -> Result<(), VmError> {
        let Some(ble_stop) = self.callbacks.ble_stop else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { ble_stop(self.user_data) })
    }

    fn service_wifi_start_ap<'a>(&'a mut self, ssid: &str) -> Result<WifiOperation<'a>, VmError> {
        let Some(wifi_start_ap) = self.callbacks.wifi_start_ap else {
            return Ok(WifiOperation::unsupported());
        };
        let mut out = SqvmWifiOperation::default();
        callback_status(unsafe {
            wifi_start_ap(self.user_data, ssid.as_ptr(), ssid.len(), &mut out)
        })?;
        unsafe { wifi_operation_from_ffi(&out) }
    }

    fn service_wifi_stop_ap<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        let Some(wifi_stop_ap) = self.callbacks.wifi_stop_ap else {
            return Ok(WifiOperation::unsupported());
        };
        let mut out = SqvmWifiOperation::default();
        callback_status(unsafe { wifi_stop_ap(self.user_data, &mut out) })?;
        unsafe { wifi_operation_from_ffi(&out) }
    }

    fn service_wifi_connect<'a>(&'a mut self, profile: &str) -> Result<WifiOperation<'a>, VmError> {
        let Some(wifi_connect) = self.callbacks.wifi_connect else {
            return Ok(WifiOperation::unsupported());
        };
        let mut out = SqvmWifiOperation::default();
        callback_status(unsafe {
            wifi_connect(self.user_data, profile.as_ptr(), profile.len(), &mut out)
        })?;
        unsafe { wifi_operation_from_ffi(&out) }
    }

    fn service_wifi_disconnect<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        let Some(wifi_disconnect) = self.callbacks.wifi_disconnect else {
            return Ok(WifiOperation::unsupported());
        };
        let mut out = SqvmWifiOperation::default();
        callback_status(unsafe { wifi_disconnect(self.user_data, &mut out) })?;
        unsafe { wifi_operation_from_ffi(&out) }
    }

    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        let Some(wifi_status) = self.callbacks.wifi_status else {
            return Err(VmError::InvalidOperand);
        };
        let mut out = SqvmWifiStatus::default();
        callback_status(unsafe { wifi_status(self.user_data, &mut out) })?;
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
        callback_status(unsafe { wifi_get_ap_ip(self.user_data, &mut out) })?;
        unsafe { wifi_ap_ip_from_ffi(&out) }
    }

    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        let Some(wifi_scan) = self.callbacks.wifi_scan else {
            return Ok(WifiOperation::unsupported());
        };
        let mut out = SqvmWifiOperation::default();
        callback_status(unsafe { wifi_scan(self.user_data, &mut out) })?;
        unsafe { wifi_operation_from_ffi(&out) }
    }

    fn service_wifi_operation<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        let Some(wifi_operation) = self.callbacks.wifi_operation else {
            return Ok(WifiOperation::idle());
        };
        let mut out = SqvmWifiOperation::default();
        callback_status(unsafe { wifi_operation(self.user_data, &mut out) })?;
        unsafe { wifi_operation_from_ffi(&out) }
    }

    fn service_wifi_result<'a>(&'a mut self) -> Result<WifiOperationResult<'a>, VmError> {
        let Some(wifi_result) = self.callbacks.wifi_result else {
            return Ok(WifiOperationResult::unsupported());
        };
        let mut out = SqvmWifiOperationResult::default();
        callback_status(unsafe { wifi_result(self.user_data, &mut out) })?;
        unsafe { wifi_operation_result_from_ffi(&out) }
    }

    fn service_wifi_cancel<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        let Some(wifi_cancel) = self.callbacks.wifi_cancel else {
            return Ok(WifiOperation::idle());
        };
        let mut out = SqvmWifiOperation::default();
        callback_status(unsafe { wifi_cancel(self.user_data, &mut out) })?;
        unsafe { wifi_operation_from_ffi(&out) }
    }

    fn service_wifi_scan_network<'a>(
        &'a mut self,
        index: i32,
    ) -> Result<WifiScanNetwork<'a>, VmError> {
        let Some(wifi_scan_network) = self.callbacks.wifi_scan_network else {
            return Ok(WifiScanNetwork::unsupported());
        };
        let mut out = SqvmWifiScanNetworkResult::default();
        callback_status(unsafe { wifi_scan_network(self.user_data, index, &mut out) })?;
        wifi_scan_network_from_ffi(&out)
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
            device_config_load(self.user_data, source.as_ptr(), source.len(), &mut out)
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
            device_config_set(self.user_data, key.as_ptr(), key.len(), value, &mut out)
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
            device_config_rebind(self.user_data, alias.as_ptr(), alias.len(), &mut out)
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
                self.user_data,
                destination.as_ptr(),
                destination.len(),
                &mut out,
            )
        })?;
        unsafe { device_config_result_from_ffi(&out) }
    }

    fn file_pick_file<'a>(
        &'a mut self,
        extension: &str,
    ) -> Result<FilePickFileResult<'a>, VmError> {
        let Some(file_pick_file) = self.callbacks.file_pick_file else {
            return Ok(FilePickFileResult::unsupported());
        };
        let mut out = SqvmFilePickFileResult::default();
        callback_status(unsafe {
            file_pick_file(
                self.user_data,
                extension.as_ptr(),
                extension.len(),
                &mut out,
            )
        })?;
        unsafe { file_pick_file_result_from_ffi(&out) }
    }

    fn file_read_text<'a>(&'a mut self, path: &str) -> Result<FileReadTextResult<'a>, VmError> {
        let Some(file_read_text) = self.callbacks.file_read_text else {
            return Ok(FileReadTextResult::unsupported());
        };
        let mut out = SqvmFileReadTextResult::default();
        callback_status(unsafe {
            file_read_text(self.user_data, path.as_ptr(), path.len(), &mut out)
        })?;
        unsafe { file_read_text_result_from_ffi(&out) }
    }

    fn file_read_lines<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<FileReadLinesResult<'a>, VmError> {
        let Some(file_read_lines) = self.callbacks.file_read_lines else {
            return Ok(FileReadLinesResult::unsupported());
        };
        let mut out = SqvmFileReadLinesResult::default();
        callback_status(unsafe {
            file_read_lines(
                self.user_data,
                path.as_ptr(),
                path.len(),
                max_lines,
                &mut out,
            )
        })?;
        unsafe { file_read_lines_result_from_ffi(&out) }
    }

    fn binbook_open<'a>(&'a mut self, path: &str) -> Result<BinBookOpenResult<'a>, VmError> {
        let Some(binbook_open) = self.callbacks.binbook_open else {
            return Ok(BinBookOpenResult::unsupported());
        };
        let mut out = SqvmBinBookOpenResult::default();
        callback_status(unsafe {
            binbook_open(self.user_data, path.as_ptr(), path.len(), &mut out)
        })?;
        unsafe { binbook_open_result_from_ffi(&out) }
    }

    fn binbook_info<'a>(&'a mut self, book: Handle) -> Result<BinBookInfoResult<'a>, VmError> {
        let Some(binbook_info) = self.callbacks.binbook_info else {
            return Ok(BinBookInfoResult::unsupported());
        };
        let mut out = SqvmBinBookInfoResult::default();
        callback_status(unsafe { binbook_info(self.user_data, handle_to_ffi(book), &mut out) })?;
        unsafe { binbook_info_result_from_ffi(&out) }
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, VmError> {
        let Some(binbook_read_page) = self.callbacks.binbook_read_page else {
            return Ok(BinBookReadPageResult::unsupported());
        };
        let mut out = SqvmBinBookReadPageResult::default();
        callback_status(unsafe {
            binbook_read_page(self.user_data, handle_to_ffi(book), page_index, &mut out)
        })?;
        unsafe { binbook_read_page_result_from_ffi(&out) }
    }

    fn content_binbook_list<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
    ) -> Result<ContentBinBookListResult<'a>, VmError> {
        let Some(content_binbook_list) = self.callbacks.content_binbook_list else {
            return Ok(ContentBinBookListResult::unsupported());
        };
        let mut out = SqvmContentBinBookListResult::default();
        let mut count = 0usize;
        self.content_binbook_entries =
            [SqvmContentBinBookEntry::default(); SQVM_FFI_CONTENT_BINBOOK_CAP];
        callback_status(unsafe {
            content_binbook_list(
                self.user_data,
                library.as_ptr(),
                library.len(),
                offset,
                limit,
                self.content_binbook_entries.as_mut_ptr(),
                self.content_binbook_entries.len(),
                &mut count,
                &mut out,
            )
        })?;
        self.content_binbook_count = count.min(self.content_binbook_entries.len());
        for index in 0..self.content_binbook_count {
            self.content_binbook_core_entries[index] =
                unsafe { content_binbook_entry_from_ffi(&self.content_binbook_entries[index])? };
        }
        unsafe {
            content_binbook_list_result_from_ffi(
                &out,
                &self.content_binbook_core_entries[..self.content_binbook_count],
            )
        }
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        let Some(system_memory_text) = self.callbacks.system_memory_text else {
            return Err(VmError::InvalidOperand);
        };
        let mut line = FixedLine::<96>::default();
        let mut line_len = 0usize;
        callback_status(unsafe {
            system_memory_text(self.user_data, line.as_mut_ptr(), line.cap(), &mut line_len)
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
                self.user_data,
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

    fn system_start_reason_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        let Some(system_start_reason_text) = self.callbacks.system_start_reason_text else {
            return Err(VmError::InvalidOperand);
        };
        let mut line = FixedLine::<32>::default();
        let mut line_len = 0usize;
        callback_status(unsafe {
            system_start_reason_text(self.user_data, line.as_mut_ptr(), line.cap(), &mut line_len)
        })?;
        line.set_len(line_len)?;
        out.write_str(line.as_str()?)
            .map_err(|_| VmError::InvalidOperand)
    }

    fn service_power_sleep(&mut self, wake_after_ms: i32) -> Result<(), VmError> {
        let Some(power_sleep) = self.callbacks.power_sleep else {
            return Err(VmError::InvalidOperand);
        };
        callback_status(unsafe { power_sleep(self.user_data, wake_after_ms) })
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

fn ble_profile_count_from_reader(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
    out_count: *mut usize,
) -> SqvmStatus {
    let (_, section) = match ble_trigger_reader_sections(reader, scratch) {
        Ok(sections) => sections,
        Err(_) => return SqvmStatus::VmError,
    };
    let Some(section) = section else {
        unsafe {
            *out_count = 0;
        }
        return SqvmStatus::Ok;
    };
    if section.len > scratch.len() {
        return SqvmStatus::VmError;
    }
    if reader
        .read_exact_at(section.offset, &mut scratch[..section.len])
        .is_err()
    {
        return SqvmStatus::VmError;
    }
    let count = match read_u16_slice(scratch, 0) {
        Some(count) => count as usize,
        None => return SqvmStatus::VmError,
    };
    if count > 16 {
        return SqvmStatus::VmError;
    }
    unsafe {
        *out_count = count;
    }
    SqvmStatus::Ok
}

fn ble_profile_read_from_reader(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
    profile_index: usize,
    out_profile: *mut SqvmBleProfileTrigger,
) -> SqvmStatus {
    let (strings_section, section) = match ble_trigger_reader_sections(reader, scratch) {
        Ok(sections) => sections,
        Err(_) => return SqvmStatus::VmError,
    };
    let Some(section) = section else {
        return SqvmStatus::InvalidArgument;
    };
    if section.len > scratch.len() || strings_section.len > scratch.len() {
        return SqvmStatus::VmError;
    }
    if reader
        .read_exact_at(section.offset, &mut scratch[..section.len])
        .is_err()
    {
        return SqvmStatus::VmError;
    }
    let count = match read_u16_slice(scratch, 0) {
        Some(count) => count as usize,
        None => return SqvmStatus::VmError,
    };
    if profile_index >= count {
        return SqvmStatus::InvalidArgument;
    }
    let mut cursor = 2usize;
    let mut selected = None;
    for index in 0..count {
        let profile_id = match read_u16_slice(scratch, cursor) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
        let id_id = match read_u16_slice(scratch, cursor + 2) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
        let role_id = match read_u16_slice(scratch, cursor + 4) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
        let accept_count = match read_u16_slice(scratch, cursor + 6) {
            Some(value) => value as usize,
            None => return SqvmStatus::VmError,
        };
        cursor += 8;
        if accept_count > SQVM_BLE_PROFILE_ACCEPT_MAX {
            return SqvmStatus::VmError;
        }
        let accept_start = cursor;
        cursor = match cursor.checked_add(accept_count * 2) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
        let event_count = match read_u16_slice(scratch, cursor) {
            Some(value) => value as usize,
            None => return SqvmStatus::VmError,
        };
        cursor += 2;
        if event_count > SQVM_BLE_PROFILE_EVENT_MAX {
            return SqvmStatus::VmError;
        }
        let events_start = cursor;
        cursor = match cursor.checked_add(event_count * 4) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
        if cursor > section.len {
            return SqvmStatus::VmError;
        }
        if index == profile_index {
            selected = Some((
                profile_id,
                id_id,
                role_id,
                accept_count,
                accept_start,
                event_count,
                events_start,
            ));
        }
    }
    if cursor != section.len {
        return SqvmStatus::VmError;
    }

    let Some((profile_id, id_id, role_id, accept_count, accept_start, event_count, events_start)) =
        selected
    else {
        return SqvmStatus::InvalidArgument;
    };
    let mut accept_ids = [0u16; SQVM_BLE_PROFILE_ACCEPT_MAX];
    for (index, slot) in accept_ids.iter_mut().enumerate().take(accept_count) {
        *slot = match read_u16_slice(scratch, accept_start + index * 2) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
    }
    let mut event_ids = [(0u16, 0u16); SQVM_BLE_PROFILE_EVENT_MAX];
    for (index, slot) in event_ids.iter_mut().enumerate().take(event_count) {
        let base = events_start + index * 4;
        let kind_id = match read_u16_slice(scratch, base) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
        let event_id = match read_u16_slice(scratch, base + 2) {
            Some(value) => value,
            None => return SqvmStatus::VmError,
        };
        *slot = (kind_id, event_id);
    }
    if reader
        .read_exact_at(strings_section.offset, &mut scratch[..strings_section.len])
        .is_err()
    {
        return SqvmStatus::VmError;
    }
    let strings_len = strings_section.len;
    // Construct the result in place in the caller-owned buffer. This runs deep
    // inside VM builtin dispatch (service.ble.start), so a 640-byte
    // SqvmBleProfileTrigger temporary on this stack would overflow the VM work
    // stack. Zero the out struct, then copy string fields directly into it.
    unsafe {
        core::ptr::write_bytes(out_profile, 0u8, 1);
    }
    let out = unsafe { &mut *out_profile };
    out.accept_count = accept_count;
    out.event_count = event_count;
    if copy_string_id(&scratch[..strings_len], profile_id, &mut out.profile).is_err()
        || copy_string_id(&scratch[..strings_len], id_id, &mut out.id).is_err()
        || copy_string_id(&scratch[..strings_len], role_id, &mut out.role).is_err()
    {
        return SqvmStatus::VmError;
    }
    for index in 0..accept_count {
        if copy_string_id(
            &scratch[..strings_len],
            accept_ids[index],
            &mut out.accept[index],
        )
        .is_err()
        {
            return SqvmStatus::VmError;
        }
    }
    for index in 0..event_count {
        let (kind_id, event_id) = event_ids[index];
        if copy_string_id(
            &scratch[..strings_len],
            kind_id,
            &mut out.events[index].kind,
        )
        .is_err()
            || copy_string_id(
                &scratch[..strings_len],
                event_id,
                &mut out.events[index].event,
            )
            .is_err()
        {
            return SqvmStatus::VmError;
        }
    }
    SqvmStatus::Ok
}

fn ble_trigger_reader_sections(
    reader: &mut impl SqbcReader,
    scratch: &mut [u8],
) -> Result<(SqbcSection, Option<SqbcSection>), VmError> {
    let mut fixed_header = [0u8; 14];
    reader.read_exact_at(0, &mut fixed_header)?;
    let header = Program::parse_header(&fixed_header)?;
    if header.header_len > scratch.len() {
        return Err(VmError::InvalidHeader);
    }
    reader.read_exact_at(0, &mut scratch[..header.header_len])?;
    let mut strings_section = None;
    let mut ble_section = None;
    for index in 0..header.section_count {
        let record = Program::parse_section_record(&scratch[..header.header_len], index)?;
        match record.kind {
            SECTION_STRINGS => strings_section = Some(record),
            SECTION_BLE_TRIGGERS => ble_section = Some(record),
            _ => {}
        }
    }
    Ok((strings_section.ok_or(VmError::MissingSection)?, ble_section))
}

fn read_u16_slice(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
    ]))
}

fn string_from_section(bytes: &[u8], id: u16) -> Result<&str, VmError> {
    let count = read_u16_slice(bytes, 0).ok_or(VmError::InvalidSection)? as usize;
    let mut cursor = 2usize;
    for index in 0..count {
        let len = read_u16_slice(bytes, cursor).ok_or(VmError::InvalidSection)? as usize;
        cursor = cursor.checked_add(2).ok_or(VmError::InvalidSection)?;
        let end = cursor.checked_add(len).ok_or(VmError::InvalidSection)?;
        if end > bytes.len() {
            return Err(VmError::InvalidSection);
        }
        if index == id as usize {
            return str::from_utf8(&bytes[cursor..end]).map_err(|_| VmError::InvalidSection);
        }
        cursor = end;
    }
    Err(VmError::InvalidSection)
}

fn copy_string_id(bytes: &[u8], id: u16, out: &mut [u8]) -> Result<(), VmError> {
    let value = string_from_section(bytes, id)?;
    let value = value.as_bytes();
    if value.len() >= out.len() {
        return Err(VmError::InvalidSection);
    }
    out.fill(0);
    out[..value.len()].copy_from_slice(value);
    Ok(())
}

fn payload_field_name(name: &str) -> Option<&'static str> {
    match name {
        "profile" => Some("profile"),
        "id" => Some("id"),
        "objectName" => Some("objectName"),
        "bytesReceived" => Some("bytesReceived"),
        "totalBytes" => Some("totalBytes"),
        "upload" => Some("upload"),
        "error" => Some("error"),
        _ => None,
    }
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

fn handle_to_ffi(handle: Handle) -> SqvmHandle {
    SqvmHandle {
        kind: match handle.kind {
            HandleKind::BinBook => SqvmHandleKind::BinBook,
            HandleKind::Drawable => SqvmHandleKind::Drawable,
        },
        id: handle.id,
    }
}

fn handle_from_ffi(handle: SqvmHandle) -> Result<Option<Handle>, VmError> {
    match handle.kind {
        SqvmHandleKind::None => Ok(None),
        SqvmHandleKind::BinBook => Ok(Some(Handle::new(HandleKind::BinBook, handle.id))),
        SqvmHandleKind::Drawable => Ok(Some(Handle::new(HandleKind::Drawable, handle.id))),
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

unsafe fn content_binbook_entry_from_ffi<'a>(
    entry: &SqvmContentBinBookEntry,
) -> Result<ContentBinBookEntry<'a>, VmError> {
    Ok(ContentBinBookEntry {
        name: required_ffi_str(entry.name, entry.name_len)?,
        reference: required_ffi_str(entry.reference, entry.reference_len)?,
        size: entry.size,
    })
}

unsafe fn content_binbook_list_result_from_ffi<'a>(
    result: &SqvmContentBinBookListResult,
    items: &'a [ContentBinBookEntry<'a>],
) -> Result<ContentBinBookListResult<'a>, VmError> {
    Ok(ContentBinBookListResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        warning: optional_ffi_str(result.warning, result.warning_len)?,
        items,
        count: result.count,
        has_more: result.has_more,
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

unsafe fn wifi_operation_from_ffi<'a>(
    result: &SqvmWifiOperation,
) -> Result<WifiOperation<'a>, VmError> {
    Ok(WifiOperation {
        active: result.active,
        kind: optional_ffi_str(result.kind, result.kind_len)?,
        state: required_ffi_str(result.state, result.state_len)?,
        done: result.done,
        cancelled: result.cancelled,
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
    })
}

unsafe fn wifi_operation_result_from_ffi<'a>(
    result: &SqvmWifiOperationResult,
) -> Result<WifiOperationResult<'a>, VmError> {
    Ok(WifiOperationResult {
        ready: result.ready,
        kind: optional_ffi_str(result.kind, result.kind_len)?,
        state: required_ffi_str(result.state, result.state_len)?,
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        cancelled: result.cancelled,
        count: result.count,
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

unsafe fn display_info_from_ffi<'a>(result: &SqvmDisplayInfo) -> Result<DisplayInfo<'a>, VmError> {
    Ok(DisplayInfo {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        warning: optional_ffi_str(result.warning, result.warning_len)?,
        available: result.available,
        status: required_ffi_str(result.status, result.status_len)?,
        binding: required_ffi_str(result.binding, result.binding_len)?,
        driver: optional_ffi_str(result.driver, result.driver_len)?.unwrap_or(""),
        transport: optional_ffi_str(result.transport, result.transport_len)?.unwrap_or(""),
        width: result.width,
        height: result.height,
        physical_width: result.physical_width,
        physical_height: result.physical_height,
        rotation: result.rotation,
        color_model: optional_ffi_str(result.color_model, result.color_model_len)?.unwrap_or(""),
        logical_gray_levels: result.logical_gray_levels,
        native_bpp: result.native_bpp,
        native_pixel_format: optional_ffi_str(
            result.native_pixel_format,
            result.native_pixel_format_len,
        )?
        .unwrap_or(""),
        default_font_height: result.default_font_height,
        supports_partial_refresh: result.supports_partial_refresh,
        supports_fast_refresh: result.supports_fast_refresh,
    })
}

unsafe fn file_pick_file_result_from_ffi<'a>(
    result: &SqvmFilePickFileResult,
) -> Result<FilePickFileResult<'a>, VmError> {
    Ok(FilePickFileResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        path: optional_ffi_str(result.path, result.path_len)?,
    })
}

unsafe fn file_read_text_result_from_ffi<'a>(
    result: &SqvmFileReadTextResult,
) -> Result<FileReadTextResult<'a>, VmError> {
    Ok(FileReadTextResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        text: optional_ffi_str(result.text, result.text_len)?,
    })
}

unsafe fn file_read_lines_result_from_ffi<'a>(
    result: &SqvmFileReadLinesResult,
) -> Result<FileReadLinesResult<'a>, VmError> {
    Ok(FileReadLinesResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        lines: &[],
    })
}

unsafe fn binbook_open_result_from_ffi<'a>(
    result: &SqvmBinBookOpenResult,
) -> Result<BinBookOpenResult<'a>, VmError> {
    Ok(BinBookOpenResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        book: handle_from_ffi(result.book)?,
    })
}

unsafe fn binbook_info_result_from_ffi<'a>(
    result: &SqvmBinBookInfoResult,
) -> Result<BinBookInfoResult<'a>, VmError> {
    Ok(BinBookInfoResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        title: optional_ffi_str(result.title, result.title_len)?,
        page_count: result.page_count,
    })
}

unsafe fn binbook_read_page_result_from_ffi<'a>(
    result: &SqvmBinBookReadPageResult,
) -> Result<BinBookReadPageResult<'a>, VmError> {
    Ok(BinBookReadPageResult {
        ok: result.ok,
        error: optional_ffi_str(result.error, result.error_len)?,
        drawable: handle_from_ffi(result.drawable)?,
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
        Value::String(_) => {
            let text = strings.value_str(value)?;
            Ok(SqvmDeviceConfigValue {
                kind: SqvmDeviceConfigValueKind::String,
                string: text.as_ptr(),
                string_len: text.len(),
                ..SqvmDeviceConfigValue::default()
            })
        }
        Value::Record(_) | Value::List(_) | Value::Handle(_) => Err(VmError::InvalidOperand),
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
        const ZEPHYR_RUNTIME_CONTEXT_BYTES: usize = 7_872;
        assert!(
            sqvm_context_size() <= ZEPHYR_RUNTIME_CONTEXT_BYTES,
            "SqvmContext is {} bytes and must fit firmware/zephyr/src/vm_runtime.h context_words",
            sqvm_context_size()
        );
    }

    #[test]
    fn fixed_line_writes_i32_without_fmt_runtime() {
        let mut line = FixedLine::<64>::default();

        line.write_i32(0);
        let _ = line.write_str(" ");
        line.write_i32(42);
        let _ = line.write_str(" ");
        line.write_i32(-17);
        let _ = line.write_str(" ");
        line.write_i32(i32::MIN);

        assert_eq!(line.as_str().unwrap(), "0 42 -17 -2147483648");
    }

    #[test]
    fn fixed_line_writes_bool_without_fmt_runtime() {
        let mut line = FixedLine::<16>::default();

        line.write_bool(true);
        let _ = line.write_str(" ");
        line.write_bool(false);

        assert_eq!(line.as_str().unwrap(), "true false");
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
    let mut access_point = WifiAccessPoint::new(
        ssid,
        bssid,
        network.channel,
        network.rssi,
        wifi_auth_static(unsafe { optional_ffi_str(network.auth, network.auth_len)? }),
        network.hidden,
    )?;
    access_point.ssid_length = network.ssid_length;
    Ok(access_point)
}

fn wifi_scan_network_from_ffi<'a>(
    result: &SqvmWifiScanNetworkResult,
) -> Result<WifiScanNetwork<'a>, VmError> {
    let network = if result.ok {
        Some(wifi_access_point_from_ffi(&result.network)?)
    } else {
        None
    };
    Ok(WifiScanNetwork {
        ok: result.ok,
        error: unsafe { optional_ffi_str(result.error, result.error_len)? },
        network,
    })
}

fn wifi_auth_static(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("open") | Some("OPEN") => Some("open"),
        Some("wep") | Some("WEP") | Some("WEP-OPEN") | Some("WEP-SHARED") => Some("wep"),
        Some("wpa") | Some("WPA-PSK") | Some("WPA/WPA2/WPA3 PSK") => Some("wpa"),
        Some("wpa2") | Some("WPA2-PSK") | Some("WPA2-PSK-SHA256") | Some("FT-PSK") => Some("wpa2"),
        Some("wpa3")
        | Some("WPA3-SAE-HNP")
        | Some("WPA3-SAE-H2E")
        | Some("WPA3-SAE-AUTO")
        | Some("WPA3-SAE-EXT-KEY")
        | Some("FT-SAE") => Some("wpa3"),
        Some("unknown") | Some("UNKNOWN") => Some("unknown"),
        Some(_) => Some("unknown"),
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
        Value::String(_) => {
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
        Value::Handle(_) => {
            let _ = line.write_str("<handle>");
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

    fn write_bool(&mut self, value: bool) {
        let _ = self.write_str(if value { "true" } else { "false" });
    }

    fn write_i32(&mut self, value: i32) {
        if value == 0 {
            let _ = self.write_str("0");
            return;
        }

        let mut magnitude = if value < 0 {
            let _ = self.write_str("-");
            value.wrapping_neg() as u32
        } else {
            value as u32
        };
        let mut digits = [0u8; 10];
        let mut len = 0usize;
        while magnitude > 0 && len < digits.len() {
            digits[len] = b'0' + (magnitude % 10) as u8;
            magnitude /= 10;
            len += 1;
        }
        while len > 0 {
            len -= 1;
            let digit = [digits[len]];
            let text = str::from_utf8(&digit).unwrap_or("");
            let _ = self.write_str(text);
        }
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
    unsafe { sqvm_ffi_panic_abort() }
}

#[cfg(feature = "zephyr")]
unsafe extern "C" {
    fn sqvm_ffi_panic_abort() -> !;
}
