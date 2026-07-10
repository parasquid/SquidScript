use core::{
    fmt::{self, Write},
    mem::MaybeUninit,
    ptr,
};

pub use squidvm_core::host::{
    DisplayLineOptions, DisplayRectOptions, DisplayResourceOptions, DisplayTextOptions,
};

use squidvm_core::{
    error::VmError,
    host::{
        AppArmedStack, AppArmedStackEntry, AppInstallResult, AppProcessStack, AppRegistryEntry,
        AppRegistryList, BinBookChapterListResult, BinBookChapterListSummary,
        BinBookChapterListWriter, BinBookChapterResult, BinBookInfoResult, BinBookOpenResult,
        BinBookReadPageResult, ContentBinBookEntry, ContentBinBookListResult,
        ContentBinBookListSummary, ContentBinBookListWriter, DisplayInfo, FileCopyResult,
        FileListEntry, FileListSummary, FileListWriter, FilePickFileResult, FileReadLinesResult,
        FileReadLinesSummary, FileReadLinesWriter, FileReadTextResult, TraceSink,
        UploadStartResult, UploadStatus, WifiAccessPoint, WifiApIp, WifiOperation,
        WifiOperationResult, WifiScanNetwork, WifiStatus,
    },
    limits::{
        MAX_APP_BYTES, MAX_APP_ID_BYTES, MAX_EVENT_NAME_BYTES, MAX_FOREGROUND_TIMERS,
        MAX_SAVED_STATE_BYTES,
    },
    program::{CapabilityDemand, ProgramIndex},
    reader::{SliceSqbcReader, SqbcReader},
    strings::StringResolver,
    value::{Handle, Value},
    vm::{ChunkedVm, EventPayload, EventPayloadField},
};

use crate::{
    app_store::{AppStoreError, NativeAppStorage, NativeAppStore, VolatileAppStorage},
    lifecycle::{
        ForegroundLifecycle, LifecycleError, LifecyclePhase, StartReason,
        TriggerTimer as LifecycleTriggerTimer, MAX_ARMED_INPUTS, MAX_ARMED_TIMERS,
    },
    power::{DeferredNativePowerBackend, NativePowerBackend, NativePowerRequest, PowerCheckpoint},
    radio_lifecycle::RadioKind,
    radio_service::{RadioLeaseManager, RadioLeaseState, ServiceLeaseError},
};

pub const MAX_TEMP_SQBC_BYTES: usize = MAX_APP_BYTES;
const MAX_APP_RESOURCE_TEXT_BYTES: usize = 256;
const MAX_LINE_COUNT: usize = 8;
const MAX_LINE_BYTES: usize = 64;
const MAX_BLE_PROFILE_ID_BYTES: usize = 32;
const MAX_UPLOAD_NAME_BYTES: usize = 64;
const MAX_UPLOAD_REF_BYTES: usize = 128;
const MAX_UPLOAD_BYTES_TEXT_BYTES: usize = 20;
const UPLOAD_STAGE_CHUNK_BYTES: usize = 512;
const UPLOAD_HTTP_PATH: &str = "/upload/<safe-name>";
const NO_UPLOAD_TRANSPORTS: &[&str] = &[];
const HTTP_UPLOAD_TRANSPORTS: &[&str] = &["http"];
const BLE_UPLOAD_TRANSPORTS: &[&str] = &["ble"];
const HTTP_BLE_UPLOAD_TRANSPORTS: &[&str] = &["http", "ble"];
const MAX_WIFI_PROFILE_NAME_BYTES: usize = 16;
const MAX_WIFI_PROFILE_SSID_BYTES: usize = 32;
const MAX_WIFI_PROFILE_PASSWORD_BYTES: usize = 64;
const MAX_TIMER_EVENT_BYTES: usize = MAX_EVENT_NAME_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeTimer {
    event: [u8; MAX_TIMER_EVENT_BYTES],
    event_len: usize,
    interval_ms: u32,
    remaining_ms: u32,
    repeating: bool,
    active: bool,
}

impl NativeTimer {
    const fn empty() -> Self {
        Self {
            event: [0; MAX_TIMER_EVENT_BYTES],
            event_len: 0,
            interval_ms: 0,
            remaining_ms: 0,
            repeating: false,
            active: false,
        }
    }

    fn set_event(&mut self, event: &str) -> Result<(), VmError> {
        if event.is_empty() || event.len() > self.event.len() {
            return Err(VmError::InvalidOperand);
        }
        self.event[..event.len()].copy_from_slice(event.as_bytes());
        self.event_len = event.len();
        Ok(())
    }

    fn event_as_str(&self) -> &str {
        core::str::from_utf8(&self.event[..self.event_len]).unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRuntimeError {
    TooLarge,
    InvalidOffset,
    IncompleteTempRun,
    AppNotInstalled,
    AppIdMismatch,
    Inactive,
    UploadSessionActive,
    Vm(VmError),
}

fn native_app_store_error(error: AppStoreError) -> NativeRuntimeError {
    match error {
        AppStoreError::TooLarge | AppStoreError::RegistryFull => NativeRuntimeError::TooLarge,
        AppStoreError::Incomplete => NativeRuntimeError::IncompleteTempRun,
        AppStoreError::OutOfOrder => NativeRuntimeError::InvalidOffset,
        AppStoreError::AppIdMismatch => NativeRuntimeError::AppIdMismatch,
        AppStoreError::NotFound => NativeRuntimeError::AppNotInstalled,
        AppStoreError::InvalidAppId
        | AppStoreError::InvalidPath
        | AppStoreError::CorruptSqbc
        | AppStoreError::NoSpace
        | AppStoreError::Io => NativeRuntimeError::Vm(VmError::ReadFailed),
    }
}

fn app_store_error_name(error: AppStoreError) -> &'static str {
    match error {
        AppStoreError::NotFound => "not-found",
        AppStoreError::InvalidAppId => "invalid-app-id",
        AppStoreError::InvalidPath => "invalid-path",
        AppStoreError::TooLarge => "too-large",
        AppStoreError::RegistryFull => "registry-full",
        AppStoreError::Incomplete => "incomplete",
        AppStoreError::OutOfOrder => "out-of-order",
        AppStoreError::CorruptSqbc => "corrupt-sqbc",
        AppStoreError::AppIdMismatch => "app-id-mismatch",
        AppStoreError::NoSpace => "no-space",
        AppStoreError::Io => "io-error",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeUploadRouteError {
    NoActiveProfile,
    RouteMismatch,
    RouteAmbiguous,
    InvalidMetadata,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeUploadRoute {
    pub profile_id: FixedText<MAX_BLE_PROFILE_ID_BYTES>,
    pub complete_event: FixedText<MAX_LINE_BYTES>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeUploadProgress<'a> {
    pub path: &'a str,
    pub name: &'a str,
    pub id: &'a str,
    pub transport: NativeUploadTransport,
    pub bytes_received: usize,
    pub total_bytes: usize,
}

impl NativeUploadRoute {
    fn new(profile_id: &str, complete_event: &str) -> Result<Self, NativeUploadRouteError> {
        let mut route = Self {
            profile_id: FixedText::new(),
            complete_event: FixedText::new(),
        };
        route
            .profile_id
            .set(profile_id)
            .map_err(|_| NativeUploadRouteError::InvalidMetadata)?;
        route
            .complete_event
            .set(complete_event)
            .map_err(|_| NativeUploadRouteError::InvalidMetadata)?;
        Ok(route)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeUploadTransport {
    Http,
    Ble,
}

impl NativeUploadTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Ble => "ble",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWifiStatus<'a> {
    pub mode: Option<&'a str>,
    pub ssid: Option<&'a str>,
    pub ip_address: Option<&'a str>,
    pub state: &'a str,
    pub driver_started: bool,
    pub configured: bool,
    pub channel: i32,
    pub clients: i32,
    pub ap_start_events: i32,
    pub ap_stop_events: i32,
    pub probe_events: i32,
    pub sta_connected_events: i32,
    pub sta_disconnected_events: i32,
    pub last_backend_code: Option<&'a str>,
    pub connected: bool,
    pub scan_matches: i32,
    pub rssi: i32,
    pub auth: Option<&'a str>,
    pub bssid: Option<&'a str>,
    pub disconnect_reason: Option<&'a str>,
    pub disconnect_reason_code: i32,
}

impl<'a> NativeWifiStatus<'a> {
    pub const fn idle() -> Self {
        Self {
            mode: None,
            ssid: None,
            ip_address: None,
            state: "idle",
            driver_started: false,
            configured: false,
            channel: 0,
            clients: 0,
            ap_start_events: 0,
            ap_stop_events: 0,
            probe_events: 0,
            sta_connected_events: 0,
            sta_disconnected_events: 0,
            last_backend_code: None,
            connected: false,
            scan_matches: 0,
            rssi: 0,
            auth: None,
            bssid: None,
            disconnect_reason: None,
            disconnect_reason_code: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWifiApIp<'a> {
    pub ip: Option<&'a str>,
    pub gw: Option<&'a str>,
    pub netmask: Option<&'a str>,
    pub error: Option<&'a str>,
}

impl<'a> NativeWifiApIp<'a> {
    pub const fn unavailable() -> Self {
        Self {
            ip: None,
            gw: None,
            netmask: None,
            error: Some("unavailable"),
        }
    }
}

pub trait NativeRadioBackend {
    fn acquire(&mut self, radio: RadioKind) -> Result<(), ()>;
    fn release(&mut self, radio: RadioKind);

    fn start_wifi_ap(&mut self, _ssid: &str) -> Result<(), ()> {
        Ok(())
    }

    fn begin_start_wifi_ap(&mut self, ssid: &str) -> NativeWifiBackendOperation {
        match self.start_wifi_ap(ssid) {
            Ok(()) => NativeWifiBackendOperation::Done { count: 0 },
            Err(()) => NativeWifiBackendOperation::Error {
                error: "unavailable",
            },
        }
    }

    fn start_ble_profile(&mut self, _id: &str) -> Result<(), ()> {
        Ok(())
    }

    fn stop_ble_profile(&mut self) {}

    fn supports_upload_transport(&self, _transport: NativeUploadTransport) -> bool {
        true
    }

    fn connect_wifi_station(&mut self, _ssid: &str, _password: &str) -> Result<(), ()> {
        Ok(())
    }

    fn begin_connect_wifi_station(
        &mut self,
        ssid: &str,
        password: &str,
    ) -> NativeWifiBackendOperation {
        match self.connect_wifi_station(ssid, password) {
            Ok(()) => NativeWifiBackendOperation::Done { count: 0 },
            Err(()) => NativeWifiBackendOperation::Error {
                error: "unavailable",
            },
        }
    }

    fn wifi_mode(&self) -> Option<&'static str> {
        None
    }

    fn wifi_status(&self) -> NativeWifiStatus<'_> {
        NativeWifiStatus::idle()
    }

    fn wifi_ap_ip(&self) -> NativeWifiApIp<'_> {
        NativeWifiApIp::unavailable()
    }

    fn scan_wifi(&mut self) -> Result<i32, &'static str> {
        Err("unsupported")
    }

    fn begin_scan_wifi(&mut self) -> NativeWifiBackendOperation {
        match self.scan_wifi() {
            Ok(count) => NativeWifiBackendOperation::Done { count },
            Err(error) => NativeWifiBackendOperation::Error { error },
        }
    }

    fn wifi_scan_network(&self, _index: i32) -> Result<Option<WifiAccessPoint>, &'static str> {
        Err("unsupported")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeWifiBackendOperation {
    Pending,
    Done { count: i32 },
    Error { error: &'static str },
}

pub trait NativeDisplaySink {
    fn draw_clear(&mut self, _color: u8) {}
    fn draw_text(&mut self, _text: &str, _options: DisplayTextOptions<'_>) {}
    fn draw_rect(&mut self, _options: DisplayRectOptions) {}
    fn draw_line(&mut self, _options: DisplayLineOptions) {}
    fn draw_select(&mut self, _name: &str) {}
    fn draw_image(&mut self, _path: &str, _options: DisplayResourceOptions) {}
    fn draw_resource(&mut self, _drawable: &str, _options: DisplayResourceOptions) {}
    fn draw_drawable(&mut self, _drawable: Handle, _options: DisplayResourceOptions) {}
    fn draw_refresh_mode(&mut self, _mode: &str) {}
    fn screen_rendered(&mut self, _name: &str) {}
    fn pending_refreshes(&self) -> u32 {
        0
    }
    fn recorded_draws(&self) -> u32 {
        0
    }
    fn dropped_draws(&self) -> u32 {
        0
    }
}

pub trait NativeBinBookBackend {
    fn content_binbook_list<'a>(
        &'a mut self,
        _library: &str,
        _offset: i32,
        _limit: i32,
    ) -> Result<ContentBinBookListResult<'a>, VmError> {
        Ok(ContentBinBookListResult::unsupported())
    }

    fn binbook_open<'a>(&'a mut self, _path: &str) -> Result<BinBookOpenResult<'a>, VmError> {
        Ok(BinBookOpenResult::unsupported())
    }

    fn binbook_info<'a>(&'a mut self, _book: Handle) -> Result<BinBookInfoResult<'a>, VmError> {
        Ok(BinBookInfoResult::unsupported())
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        _book: Handle,
        _page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, VmError> {
        Ok(BinBookReadPageResult::unsupported())
    }

    fn binbook_chapters<'a>(
        &'a mut self,
        _book: Handle,
        _offset: i32,
        _limit: i32,
    ) -> Result<BinBookChapterListResult<'a>, VmError> {
        Ok(BinBookChapterListResult::unsupported())
    }

    fn binbook_chapter<'a>(
        &'a mut self,
        _book: Handle,
        _index: i32,
    ) -> Result<BinBookChapterResult<'a>, VmError> {
        Ok(BinBookChapterResult::unsupported())
    }
}

pub trait NativeFileBackend {
    fn reset_runtime_state(&mut self) {}

    fn file_pick_file<'a>(
        &'a mut self,
        _extension: &str,
    ) -> Result<FilePickFileResult<'a>, VmError> {
        Ok(FilePickFileResult::unsupported())
    }

    fn file_read_text<'a>(&'a mut self, _path: &str) -> Result<FileReadTextResult<'a>, VmError> {
        Ok(FileReadTextResult::unsupported())
    }

    fn file_read_lines<'a>(
        &'a mut self,
        _path: &str,
        _max_lines: i32,
    ) -> Result<FileReadLinesResult<'a>, VmError> {
        Ok(FileReadLinesResult::unsupported())
    }

    fn file_read_lines_into<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
        writer: &mut dyn FileReadLinesWriter,
    ) -> Result<FileReadLinesSummary<'a>, VmError> {
        let result = self.file_read_lines(path, max_lines)?;
        for line in result.lines {
            writer.push_line(line)?;
        }
        Ok(FileReadLinesSummary {
            ok: result.ok,
            error: result.error,
        })
    }

    fn file_copy<'a>(
        &'a mut self,
        _source: &str,
        _library: &str,
        _name: &str,
    ) -> Result<FileCopyResult<'a>, VmError> {
        Ok(FileCopyResult::unsupported())
    }

    fn file_list_into<'a>(
        &'a mut self,
        _library: &str,
        _offset: i32,
        _limit: i32,
        _writer: &mut dyn FileListWriter,
    ) -> Result<FileListSummary<'a>, VmError> {
        Ok(FileListSummary {
            ok: false,
            error: Some("unsupported"),
            count: 0,
            has_more: false,
        })
    }

    fn content_binbook_list_into<'a>(
        &'a mut self,
        _library: &str,
        _offset: i32,
        _limit: i32,
        _writer: &mut dyn ContentBinBookListWriter,
    ) -> Result<ContentBinBookListSummary<'a>, VmError> {
        Ok(ContentBinBookListSummary {
            ok: false,
            error: Some("unsupported"),
            warning: None,
            count: 0,
            has_more: false,
        })
    }

    fn binbook_open<'a>(&'a mut self, _path: &str) -> Result<BinBookOpenResult<'a>, VmError> {
        Ok(BinBookOpenResult::unsupported())
    }

    fn binbook_info<'a>(&'a mut self, _book: Handle) -> Result<BinBookInfoResult<'a>, VmError> {
        Ok(BinBookInfoResult::unsupported())
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        _book: Handle,
        _page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, VmError> {
        Ok(BinBookReadPageResult::unsupported())
    }

    fn binbook_chapters_into<'a>(
        &'a mut self,
        _book: Handle,
        _offset: i32,
        _limit: i32,
        _writer: &mut dyn BinBookChapterListWriter,
    ) -> Result<BinBookChapterListSummary<'a>, VmError> {
        Ok(BinBookChapterListSummary {
            ok: false,
            error: Some("unsupported"),
            count: 0,
            has_more: false,
        })
    }

    fn binbook_chapter<'a>(
        &'a mut self,
        _book: Handle,
        _index: i32,
    ) -> Result<BinBookChapterResult<'a>, VmError> {
        Ok(BinBookChapterResult::unsupported())
    }

    fn content_install_begin<'a>(
        &'a mut self,
        _name: &str,
        _total_len: usize,
    ) -> Result<&'a str, &'static str> {
        Err("unsupported")
    }

    fn content_install_chunk(
        &mut self,
        _path: &str,
        _offset: usize,
        _bytes: &[u8],
    ) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn content_install_commit(&mut self, _path: &str) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn content_check<'a>(
        &'a mut self,
        _name: &str,
    ) -> Result<NativeContentCheckResult<'a>, &'static str> {
        Err("unsupported")
    }

    fn content_delete<'a>(&'a mut self, _name: &str) -> Result<&'a str, &'static str> {
        Err("unsupported")
    }

    fn storage_format(&mut self) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn file_ref_size(&mut self, _path: &str) -> Result<u64, &'static str> {
        Err("unsupported")
    }

    fn file_ref_read_at(
        &mut self,
        _path: &str,
        _offset: u64,
        _out: &mut [u8],
    ) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn upload_stage_begin<'a>(
        &'a mut self,
        _safe_name: &str,
        _total_len: usize,
    ) -> Result<&'a str, &'static str> {
        Err("unsupported")
    }

    fn upload_stage_chunk(
        &mut self,
        _path: &str,
        _offset: usize,
        _bytes: &[u8],
    ) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn upload_stage_commit(&mut self, _path: &str) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn upload_stage_delete(&mut self, _path: &str) -> Result<(), &'static str> {
        Err("unsupported")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeContentCheckResult<'a> {
    pub name: &'a str,
    pub size: u64,
    pub crc32: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeFileStorageError {
    NotFound,
    VolumeMissing,
    VolumeChanged,
    InvalidName,
    NoSpace,
    Io,
}

impl NativeFileStorageError {
    pub const fn as_file_error(self) -> &'static str {
        match self {
            Self::NotFound => "not-found",
            Self::VolumeMissing => "volume-missing",
            Self::VolumeChanged => "volume-changed",
            Self::InvalidName => "invalid-name",
            Self::NoSpace => "no-space",
            Self::Io => "io-error",
        }
    }
}

pub trait NativeFileStorage {
    fn for_each_file(
        &mut self,
        visit: &mut dyn FnMut(&str, u64),
    ) -> Result<(), NativeFileStorageError>;

    fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError>;

    fn read_at(
        &mut self,
        path: &str,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), NativeFileStorageError>;

    fn create_or_truncate(&mut self, path: &str) -> Result<(), NativeFileStorageError>;

    fn begin_write(
        &mut self,
        path: &str,
        _expected_size: u64,
    ) -> Result<(), NativeFileStorageError> {
        self.create_or_truncate(path)
    }

    fn write_at(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<(), NativeFileStorageError>;

    fn write_chunk(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<(), NativeFileStorageError> {
        self.write_at(path, offset, data)
    }

    fn flush(&mut self, path: &str) -> Result<(), NativeFileStorageError>;

    fn commit_write(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        self.flush(path)
    }

    fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError>;

    fn format(&mut self) -> Result<(), NativeFileStorageError>;

    fn copy_file(
        &mut self,
        _source: &str,
        _destination: &str,
        _scratch: &mut [u8],
    ) -> Result<Option<u64>, NativeFileStorageError> {
        Ok(None)
    }
}

pub struct BoundedNativeFileBackend<
    S,
    const TEXT_BYTES: usize,
    const LINE_COUNT: usize,
    const LINE_BYTES: usize,
> {
    storage: S,
    text: [u8; TEXT_BYTES],
    upload_stage_path: [u8; TEXT_BYTES],
    upload_stage_path_len: usize,
    upload_stage_expected_len: usize,
    upload_stage_received_len: usize,
}

impl<S, const TEXT_BYTES: usize, const LINE_COUNT: usize, const LINE_BYTES: usize>
    BoundedNativeFileBackend<S, TEXT_BYTES, LINE_COUNT, LINE_BYTES>
{
    pub const fn new(storage: S) -> Self {
        Self {
            storage,
            text: [0; TEXT_BYTES],
            upload_stage_path: [0; TEXT_BYTES],
            upload_stage_path_len: 0,
            upload_stage_expected_len: 0,
            upload_stage_received_len: 0,
        }
    }

    pub const fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }
}

impl<S, const TEXT_BYTES: usize, const LINE_COUNT: usize, const LINE_BYTES: usize>
    BoundedNativeFileBackend<S, TEXT_BYTES, LINE_COUNT, LINE_BYTES>
where
    S: NativeFileStorage,
{
    fn pick_file_path(&mut self, extension: &str) -> Result<usize, &'static str> {
        validate_file_extension(extension)?;
        let mut picked_len = None;
        let mut pick_error = None;
        self.storage
            .for_each_file(&mut |path, _size| {
                if picked_len.is_some() || pick_error.is_some() || !path.ends_with(extension) {
                    return;
                }
                if let Err(error) = validate_file_ref(path) {
                    pick_error = Some(error);
                    return;
                }
                let bytes = path.as_bytes();
                if bytes.len() > self.text.len() {
                    pick_error = Some("too-large");
                    return;
                }
                self.text[..bytes.len()].copy_from_slice(bytes);
                picked_len = Some(bytes.len());
            })
            .map_err(NativeFileStorageError::as_file_error)?;
        if let Some(error) = pick_error {
            return Err(error);
        }
        picked_len.ok_or("not-found")
    }

    fn read_text_bytes(&mut self, path: &str) -> Result<usize, &'static str> {
        validate_file_ref(path)?;
        let len = self
            .storage
            .file_size(path)
            .map_err(NativeFileStorageError::as_file_error)?;
        let len = usize::try_from(len).map_err(|_| "too-large")?;
        if len > self.text.len() {
            return Err("too-large");
        }
        self.storage
            .read_at(path, 0, &mut self.text[..len])
            .map_err(NativeFileStorageError::as_file_error)?;
        Ok(len)
    }

    fn copy_file(
        &mut self,
        source: &str,
        library: &str,
        name: &str,
    ) -> Result<(usize, usize), &'static str> {
        validate_file_ref(source)?;
        let mut destination_buf = [0u8; 128];
        let destination_len = format_file_ref(library, name, &mut destination_buf)?;
        let destination = core::str::from_utf8(&destination_buf[..destination_len])
            .map_err(|_| "invalid-name")?;
        let source_len = self
            .storage
            .file_size(source)
            .map_err(NativeFileStorageError::as_file_error)?;
        if let Some(copied) = self
            .storage
            .copy_file(source, destination, &mut self.text)
            .map_err(NativeFileStorageError::as_file_error)?
        {
            if copied != source_len {
                return Err("io-error");
            }
            self.text[..destination_len].copy_from_slice(&destination_buf[..destination_len]);
            return usize::try_from(copied)
                .map(|bytes| (destination_len, bytes))
                .map_err(|_| "too-large");
        }
        self.storage
            .create_or_truncate(destination)
            .map_err(NativeFileStorageError::as_file_error)?;
        let mut copied = 0u64;
        while copied < source_len {
            let remaining = (source_len - copied) as usize;
            let chunk_len = remaining.min(self.text.len());
            self.storage
                .read_at(source, copied, &mut self.text[..chunk_len])
                .map_err(NativeFileStorageError::as_file_error)?;
            self.storage
                .write_at(destination, copied, &self.text[..chunk_len])
                .map_err(NativeFileStorageError::as_file_error)?;
            copied += chunk_len as u64;
        }
        self.storage
            .flush(destination)
            .map_err(NativeFileStorageError::as_file_error)?;
        self.text[..destination_len].copy_from_slice(&destination_buf[..destination_len]);
        usize::try_from(source_len)
            .map(|bytes| (destination_len, bytes))
            .map_err(|_| "too-large")
    }

    fn begin_content_install(
        &mut self,
        name: &str,
        total_len: usize,
    ) -> Result<usize, &'static str> {
        if total_len == 0 {
            return Err("invalid-request");
        }
        let mut destination_buf = [0u8; 128];
        let destination_len = format_file_ref("books", name, &mut destination_buf)?;
        let destination = core::str::from_utf8(&destination_buf[..destination_len])
            .map_err(|_| "invalid-name")?;
        self.storage
            .create_or_truncate(destination)
            .map_err(NativeFileStorageError::as_file_error)?;
        self.text[..destination_len].copy_from_slice(&destination_buf[..destination_len]);
        Ok(destination_len)
    }

    fn content_path_from_name(&mut self, name: &str) -> Result<usize, &'static str> {
        format_file_ref("books", name, &mut self.text)
    }

    fn check_content(&mut self, name: &str) -> Result<(usize, u64, u32), &'static str> {
        let path_len = self.content_path_from_name(name)?;
        let size = {
            let path = core::str::from_utf8(&self.text[..path_len]).map_err(|_| "invalid-name")?;
            self.storage
                .file_size(path)
                .map_err(NativeFileStorageError::as_file_error)?
        };
        let mut crc = Crc32::new();
        let mut offset = 0u64;
        while offset < size {
            let remaining = usize::try_from(size - offset).unwrap_or(usize::MAX);
            let read_len = remaining.min(self.text.len());
            let mut path_buf = [0u8; 128];
            let path_len = format_file_ref("books", name, &mut path_buf)?;
            let path = core::str::from_utf8(&path_buf[..path_len]).map_err(|_| "invalid-name")?;
            self.storage
                .read_at(path, offset, &mut self.text[..read_len])
                .map_err(NativeFileStorageError::as_file_error)?;
            crc.update(&self.text[..read_len]);
            offset += read_len as u64;
        }
        let name_len = name.len();
        if name_len > self.text.len() {
            return Err("too-large");
        }
        self.text[..name_len].copy_from_slice(name.as_bytes());
        Ok((name_len, size, crc.finish()))
    }

    fn delete_content(&mut self, name: &str) -> Result<usize, &'static str> {
        let mut path_buf = [0u8; 128];
        let path_len = format_file_ref("books", name, &mut path_buf)?;
        let path = core::str::from_utf8(&path_buf[..path_len]).map_err(|_| "invalid-name")?;
        self.storage
            .delete(path)
            .map_err(NativeFileStorageError::as_file_error)?;
        let name_len = name.len();
        if name_len > self.text.len() {
            return Err("too-large");
        }
        self.text[..name_len].copy_from_slice(name.as_bytes());
        Ok(name_len)
    }

    fn storage_format(&mut self) -> Result<(), &'static str> {
        self.storage
            .format()
            .map_err(NativeFileStorageError::as_file_error)
    }

    fn list_binbook_content(
        &mut self,
        library: &str,
        offset: i32,
        limit: i32,
        writer: &mut dyn ContentBinBookListWriter,
    ) -> Result<ContentBinBookListSummary<'static>, VmError> {
        let mut adapter = BinBookContentListAdapter { writer };
        let summary = self.list_files_filtered(library, offset, limit, &mut adapter, |path| {
            path.ends_with(".binbook")
        })?;
        Ok(ContentBinBookListSummary {
            ok: summary.ok,
            error: summary.error,
            warning: None,
            count: summary.count,
            has_more: summary.has_more,
        })
    }

    fn list_files(
        &mut self,
        library: &str,
        offset: i32,
        limit: i32,
        writer: &mut dyn FileListWriter,
    ) -> Result<FileListSummary<'static>, VmError> {
        self.list_files_filtered(library, offset, limit, writer, |_| true)
    }

    fn list_files_filtered(
        &mut self,
        library: &str,
        offset: i32,
        limit: i32,
        writer: &mut dyn FileListWriter,
        mut include: impl FnMut(&str) -> bool,
    ) -> Result<FileListSummary<'static>, VmError> {
        validate_file_segment(library).map_err(|_| VmError::InvalidOperand)?;
        let offset = if offset <= 0 { 0 } else { offset as usize };
        let limit = if limit <= 0 { 0 } else { limit as usize };
        let mut seen = 0usize;
        let mut emitted = 0usize;
        let mut total = 0i32;
        let mut error = None;
        let prefix_len = library.len() + 1;
        self.storage
            .for_each_file(&mut |path, size| {
                if error.is_some() {
                    return;
                }
                if !path.starts_with(library)
                    || path.as_bytes().get(library.len()).copied() != Some(b'/')
                {
                    return;
                }
                if !include(path) {
                    return;
                }
                if validate_file_ref(path).is_err() {
                    error = Some("invalid-name");
                    return;
                }
                total = total.saturating_add(1);
                if seen < offset {
                    seen += 1;
                    return;
                }
                if emitted >= limit {
                    return;
                }
                let name = &path[prefix_len..];
                let size = i32::try_from(size).unwrap_or(i32::MAX);
                if writer
                    .push_entry(FileListEntry {
                        name,
                        reference: path,
                        size,
                    })
                    .is_err()
                {
                    error = Some("io-error");
                    return;
                }
                emitted += 1;
            })
            .map_err(|_| VmError::InvalidOperand)?;
        if let Some(error) = error {
            return Ok(FileListSummary {
                ok: false,
                error: Some(error),
                count: total,
                has_more: false,
            });
        }
        let has_more = if limit == 0 {
            total as usize > offset
        } else {
            total as usize > offset.saturating_add(emitted)
        };
        Ok(FileListSummary {
            ok: true,
            error: None,
            count: total,
            has_more,
        })
    }
}

struct BinBookContentListAdapter<'a> {
    writer: &'a mut dyn ContentBinBookListWriter,
}

impl FileListWriter for BinBookContentListAdapter<'_> {
    fn push_entry(&mut self, entry: FileListEntry<'_>) -> Result<(), VmError> {
        if !entry.reference.ends_with(".binbook") {
            return Ok(());
        }
        self.writer.push_entry(ContentBinBookEntry {
            name: entry.name,
            reference: entry.reference,
            size: entry.size,
        })
    }
}

impl<S, const TEXT_BYTES: usize, const LINE_COUNT: usize, const LINE_BYTES: usize> NativeFileBackend
    for BoundedNativeFileBackend<S, TEXT_BYTES, LINE_COUNT, LINE_BYTES>
where
    S: NativeFileStorage,
{
    fn file_pick_file<'a>(
        &'a mut self,
        extension: &str,
    ) -> Result<FilePickFileResult<'a>, VmError> {
        let len = match self.pick_file_path(extension) {
            Ok(len) => len,
            Err(error) => {
                return Ok(FilePickFileResult {
                    ok: false,
                    error: Some(error),
                    path: None,
                })
            }
        };
        let path = match core::str::from_utf8(&self.text[..len]) {
            Ok(path) => path,
            Err(_) => {
                return Ok(FilePickFileResult {
                    ok: false,
                    error: Some("invalid-content"),
                    path: None,
                })
            }
        };
        Ok(FilePickFileResult {
            ok: true,
            error: None,
            path: Some(path),
        })
    }

    fn file_read_text<'a>(&'a mut self, path: &str) -> Result<FileReadTextResult<'a>, VmError> {
        let len = match self.read_text_bytes(path) {
            Ok(len) => len,
            Err(error) => {
                return Ok(FileReadTextResult {
                    ok: false,
                    error: Some(error),
                    text: None,
                })
            }
        };
        let text = match core::str::from_utf8(&self.text[..len]) {
            Ok(text) => text,
            Err(_) => {
                return Ok(FileReadTextResult {
                    ok: false,
                    error: Some("invalid-content"),
                    text: None,
                })
            }
        };
        Ok(FileReadTextResult {
            ok: true,
            error: None,
            text: Some(text),
        })
    }

    fn file_read_lines_into<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
        writer: &mut dyn FileReadLinesWriter,
    ) -> Result<FileReadLinesSummary<'a>, VmError> {
        if max_lines <= 0 {
            return Ok(FileReadLinesSummary {
                ok: true,
                error: None,
            });
        }
        let len = match self.read_text_bytes(path) {
            Ok(len) => len,
            Err(error) => {
                return Ok(FileReadLinesSummary {
                    ok: false,
                    error: Some(error),
                })
            }
        };
        let text = match core::str::from_utf8(&self.text[..len]) {
            Ok(text) => text,
            Err(_) => {
                return Ok(FileReadLinesSummary {
                    ok: false,
                    error: Some("invalid-content"),
                })
            }
        };
        let limit = (max_lines as usize).min(LINE_COUNT);
        for line in text.lines().take(limit) {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.len() > LINE_BYTES {
                return Ok(FileReadLinesSummary {
                    ok: false,
                    error: Some("too-large"),
                });
            }
            writer.push_line(line)?;
        }
        Ok(FileReadLinesSummary {
            ok: true,
            error: None,
        })
    }

    fn file_copy<'a>(
        &'a mut self,
        source: &str,
        library: &str,
        name: &str,
    ) -> Result<FileCopyResult<'a>, VmError> {
        let (destination_len, bytes_written) = match self.copy_file(source, library, name) {
            Ok(result) => result,
            Err(error) => {
                return Ok(FileCopyResult {
                    ok: false,
                    error: Some(error),
                    reference: None,
                    bytes_written: 0,
                })
            }
        };
        let reference = core::str::from_utf8(&self.text[..destination_len]).ok();
        Ok(FileCopyResult {
            ok: true,
            error: None,
            reference,
            bytes_written: bytes_written as i32,
        })
    }

    fn file_list_into<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
        writer: &mut dyn FileListWriter,
    ) -> Result<FileListSummary<'a>, VmError> {
        self.list_files(library, offset, limit, writer)
    }

    fn content_binbook_list_into<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
        writer: &mut dyn ContentBinBookListWriter,
    ) -> Result<ContentBinBookListSummary<'a>, VmError> {
        self.list_binbook_content(library, offset, limit, writer)
    }

    fn content_install_begin<'a>(
        &'a mut self,
        name: &str,
        total_len: usize,
    ) -> Result<&'a str, &'static str> {
        let len = self.begin_content_install(name, total_len)?;
        core::str::from_utf8(&self.text[..len]).map_err(|_| "invalid-name")
    }

    fn content_install_chunk(
        &mut self,
        path: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), &'static str> {
        validate_file_ref(path)?;
        self.storage
            .write_at(path, offset as u64, bytes)
            .map_err(NativeFileStorageError::as_file_error)
    }

    fn content_install_commit(&mut self, path: &str) -> Result<(), &'static str> {
        validate_file_ref(path)?;
        self.storage
            .flush(path)
            .map_err(NativeFileStorageError::as_file_error)
    }

    fn content_check<'a>(
        &'a mut self,
        name: &str,
    ) -> Result<NativeContentCheckResult<'a>, &'static str> {
        let (name_len, size, crc32) = self.check_content(name)?;
        let name = core::str::from_utf8(&self.text[..name_len]).map_err(|_| "invalid-name")?;
        Ok(NativeContentCheckResult { name, size, crc32 })
    }

    fn content_delete<'a>(&'a mut self, name: &str) -> Result<&'a str, &'static str> {
        let name_len = self.delete_content(name)?;
        core::str::from_utf8(&self.text[..name_len]).map_err(|_| "invalid-name")
    }

    fn storage_format(&mut self) -> Result<(), &'static str> {
        self.storage_format()
    }

    fn file_ref_size(&mut self, path: &str) -> Result<u64, &'static str> {
        validate_file_ref(path)?;
        self.storage
            .file_size(path)
            .map_err(NativeFileStorageError::as_file_error)
    }

    fn file_ref_read_at(
        &mut self,
        path: &str,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), &'static str> {
        validate_file_ref(path)?;
        self.storage
            .read_at(path, offset, out)
            .map_err(NativeFileStorageError::as_file_error)
    }

    fn upload_stage_begin<'a>(
        &'a mut self,
        safe_name: &str,
        total_len: usize,
    ) -> Result<&'a str, &'static str> {
        if total_len == 0 {
            return Err("invalid-request");
        }
        let len = format_file_ref("tmp", safe_name, &mut self.upload_stage_path)?;
        let path =
            core::str::from_utf8(&self.upload_stage_path[..len]).map_err(|_| "invalid-name")?;
        self.storage
            .begin_write(path, total_len as u64)
            .map_err(NativeFileStorageError::as_file_error)?;
        self.upload_stage_path_len = len;
        self.upload_stage_expected_len = total_len;
        self.upload_stage_received_len = 0;
        core::str::from_utf8(&self.upload_stage_path[..len]).map_err(|_| "invalid-name")
    }

    fn upload_stage_chunk(
        &mut self,
        path: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), &'static str> {
        validate_file_ref(path)?;
        let active_path =
            core::str::from_utf8(&self.upload_stage_path[..self.upload_stage_path_len])
                .map_err(|_| "invalid-name")?;
        let received = offset.checked_add(bytes.len()).ok_or("too-large")?;
        if path != active_path
            || offset != self.upload_stage_received_len
            || received > self.upload_stage_expected_len
        {
            return Err("invalid-offset");
        }
        self.storage
            .write_chunk(path, offset as u64, bytes)
            .map_err(NativeFileStorageError::as_file_error)?;
        self.upload_stage_received_len = received;
        Ok(())
    }

    fn upload_stage_commit(&mut self, path: &str) -> Result<(), &'static str> {
        validate_file_ref(path)?;
        let active_path =
            core::str::from_utf8(&self.upload_stage_path[..self.upload_stage_path_len])
                .map_err(|_| "invalid-name")?;
        if path != active_path || self.upload_stage_received_len != self.upload_stage_expected_len {
            return Err("invalid-offset");
        }
        self.storage
            .commit_write(path)
            .map_err(NativeFileStorageError::as_file_error)
    }

    fn upload_stage_delete(&mut self, path: &str) -> Result<(), &'static str> {
        validate_file_ref(path)?;
        let active_path =
            core::str::from_utf8(&self.upload_stage_path[..self.upload_stage_path_len])
                .unwrap_or("");
        if path == active_path {
            self.upload_stage_path_len = 0;
            self.upload_stage_expected_len = 0;
            self.upload_stage_received_len = 0;
        }
        self.storage
            .delete(path)
            .map_err(NativeFileStorageError::as_file_error)
    }
}

struct Crc32 {
    value: u32,
}

impl Crc32 {
    const fn new() -> Self {
        Self { value: 0xffff_ffff }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let mut value = self.value ^ u32::from(*byte);
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(value & 1);
                value = (value >> 1) ^ (0xedb8_8320 & mask);
            }
            self.value = value;
        }
    }

    const fn finish(self) -> u32 {
        !self.value
    }
}

fn validate_file_ref(path: &str) -> Result<(), &'static str> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
        })
    {
        return Err("invalid-name");
    }
    Ok(())
}

fn validate_file_extension(extension: &str) -> Result<(), &'static str> {
    if extension.len() < 2
        || !extension.starts_with('.')
        || extension.contains('/')
        || extension.contains('\\')
        || extension.contains(':')
    {
        return Err("invalid-name");
    }
    Ok(())
}

fn format_file_ref(library: &str, name: &str, out: &mut [u8]) -> Result<usize, &'static str> {
    validate_file_segment(library)?;
    validate_file_segment(name)?;
    if library == "books"
        && (name.starts_with('.') || name.len() > squid_device_protocol::MAX_CONTENT_NAME_BYTES)
    {
        return Err("invalid-name");
    }
    let required = library
        .len()
        .checked_add(1)
        .and_then(|len| len.checked_add(name.len()))
        .ok_or("too-large")?;
    if required >= out.len() {
        return Err("too-large");
    }
    out[..library.len()].copy_from_slice(library.as_bytes());
    out[library.len()] = b'/';
    out[library.len() + 1..required].copy_from_slice(name.as_bytes());
    let reference = core::str::from_utf8(&out[..required]).map_err(|_| "invalid-name")?;
    validate_file_ref(reference)?;
    Ok(required)
}

fn validate_file_segment(segment: &str) -> Result<(), &'static str> {
    if segment.is_empty()
        || !segment.is_ascii()
        || segment == "."
        || segment == ".."
        || segment.contains('/')
        || segment.contains('\\')
        || segment.contains(':')
    {
        return Err("invalid-name");
    }
    Ok(())
}

fn safe_upload_name(name: &str) -> Option<&str> {
    let candidate = name
        .rsplit(|ch| ch == '/' || ch == '\\')
        .find(|segment| !segment.is_empty())?;
    validate_file_segment(candidate).ok()?;
    Some(candidate)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopDisplaySink;

impl NativeDisplaySink for NoopDisplaySink {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopBinBookBackend;

impl NativeBinBookBackend for NoopBinBookBackend {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopFileBackend;

impl NativeFileBackend for NoopFileBackend {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopRadioBackend;

impl NativeRadioBackend for NoopRadioBackend {
    fn acquire(&mut self, _radio: RadioKind) -> Result<(), ()> {
        Ok(())
    }

    fn release(&mut self, _radio: RadioKind) {}
}

impl From<VmError> for NativeRuntimeError {
    fn from(value: VmError) -> Self {
        Self::Vm(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceMetric {
    pub key: &'static str,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceMetrics {
    metrics: [ResourceMetric; 25],
    len: usize,
}

impl ResourceMetrics {
    pub fn iter(&self) -> impl Clone + Iterator<Item = ResourceMetric> + '_ {
        self.metrics[..self.len].iter().copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineView<'a> {
    lines: [&'a str; MAX_LINE_COUNT],
    len: usize,
}

impl<'a> LineView<'a> {
    pub fn iter(&self) -> impl Clone + Iterator<Item = &'a str> + '_ {
        self.lines[..self.len].iter().copied()
    }

    pub fn as_slice(&self) -> &[&'a str] {
        &self.lines[..self.len]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeWifiOperationState {
    active: bool,
    kind: Option<&'static str>,
    state: &'static str,
    done: bool,
    cancelled: bool,
    ok: bool,
    error: Option<&'static str>,
    count: i32,
}

impl NativeWifiOperationState {
    const fn idle() -> Self {
        Self {
            active: false,
            kind: None,
            state: "idle",
            done: false,
            cancelled: false,
            ok: true,
            error: None,
            count: 0,
        }
    }

    const fn done(kind: &'static str) -> Self {
        Self {
            active: true,
            kind: Some(kind),
            state: "done",
            done: true,
            cancelled: false,
            ok: true,
            error: None,
            count: 0,
        }
    }

    const fn done_with_count(kind: &'static str, count: i32) -> Self {
        Self {
            active: false,
            kind: Some(kind),
            state: "done",
            done: true,
            cancelled: false,
            ok: true,
            error: None,
            count,
        }
    }

    const fn running(kind: &'static str) -> Self {
        Self {
            active: true,
            kind: Some(kind),
            state: "running",
            done: false,
            cancelled: false,
            ok: true,
            error: None,
            count: 0,
        }
    }

    const fn error(kind: &'static str, error: &'static str) -> Self {
        Self {
            active: false,
            kind: Some(kind),
            state: "error",
            done: true,
            cancelled: false,
            ok: false,
            error: Some(error),
            count: 0,
        }
    }

    const fn operation(self) -> WifiOperation<'static> {
        WifiOperation {
            active: self.active,
            kind: self.kind,
            state: self.state,
            done: self.done,
            cancelled: self.cancelled,
            ok: self.ok,
            error: self.error,
        }
    }

    const fn result(self) -> WifiOperationResult<'static> {
        WifiOperationResult {
            ready: self.done,
            kind: self.kind,
            state: self.state,
            ok: self.ok,
            error: self.error,
            cancelled: self.cancelled,
            count: self.count,
        }
    }
}

pub struct NativeRuntime<
    B = NoopRadioBackend,
    D = NoopDisplaySink,
    C = NoopBinBookBackend,
    F = NoopFileBackend,
    A = VolatileAppStorage,
> {
    host: RuntimeHost<B, D, C, F, A>,
    vm: MaybeUninit<ChunkedVm>,
    vm_active: bool,
    scratch: [u8; MAX_TEMP_SQBC_BYTES],
}

impl
    NativeRuntime<
        NoopRadioBackend,
        NoopDisplaySink,
        NoopBinBookBackend,
        NoopFileBackend,
        VolatileAppStorage,
    >
{
    pub const fn new() -> Self {
        Self::with_radio_display_binbook_file_and_app_store(
            NoopRadioBackend,
            NoopDisplaySink,
            NoopBinBookBackend,
            NoopFileBackend,
            VolatileAppStorage::new(),
        )
    }
}

impl<B: NativeRadioBackend>
    NativeRuntime<B, NoopDisplaySink, NoopBinBookBackend, NoopFileBackend, VolatileAppStorage>
{
    pub const fn with_radio_backend(radio_backend: B) -> Self {
        Self::with_radio_display_binbook_file_and_app_store(
            radio_backend,
            NoopDisplaySink,
            NoopBinBookBackend,
            NoopFileBackend,
            VolatileAppStorage::new(),
        )
    }
}

impl<B: NativeRadioBackend, D: NativeDisplaySink>
    NativeRuntime<B, D, NoopBinBookBackend, NoopFileBackend, VolatileAppStorage>
{
    pub const fn with_radio_and_display(radio_backend: B, display_sink: D) -> Self {
        Self::with_radio_display_binbook_file_and_app_store(
            radio_backend,
            display_sink,
            NoopBinBookBackend,
            NoopFileBackend,
            VolatileAppStorage::new(),
        )
    }
}

impl<B: NativeRadioBackend, D: NativeDisplaySink, C: NativeBinBookBackend>
    NativeRuntime<B, D, C, NoopFileBackend, VolatileAppStorage>
{
    pub const fn with_radio_display_and_binbook(
        radio_backend: B,
        display_sink: D,
        binbook_backend: C,
    ) -> Self {
        Self::with_radio_display_binbook_file_and_app_store(
            radio_backend,
            display_sink,
            binbook_backend,
            NoopFileBackend,
            VolatileAppStorage::new(),
        )
    }
}

impl<
        B: NativeRadioBackend,
        D: NativeDisplaySink,
        C: NativeBinBookBackend,
        F: NativeFileBackend,
        A: NativeAppStorage,
    > NativeRuntime<B, D, C, F, A>
{
    #[inline(always)]
    pub const fn with_radio_display_binbook_file_and_app_store(
        radio_backend: B,
        display_sink: D,
        binbook_backend: C,
        file_backend: F,
        app_storage: A,
    ) -> Self {
        Self {
            host: RuntimeHost::new(
                radio_backend,
                display_sink,
                binbook_backend,
                file_backend,
                app_storage,
            ),
            vm: MaybeUninit::uninit(),
            vm_active: false,
            scratch: [0; MAX_TEMP_SQBC_BYTES],
        }
    }

    /// Initializes a runtime directly at `out` without materializing the large
    /// aggregate on the caller's stack.
    ///
    /// # Safety
    ///
    /// `out` must be valid, aligned, writable uninitialized storage for one
    /// `Self`. The caller must not read or drop it unless this method returns.
    pub unsafe fn init_in_place(
        out: *mut Self,
        radio_backend: B,
        display_sink: D,
        binbook_backend: C,
        file_backend: F,
        app_storage: A,
    ) {
        unsafe {
            RuntimeHost::init_in_place(
                ptr::addr_of_mut!((*out).host),
                radio_backend,
                display_sink,
                binbook_backend,
                file_backend,
                app_storage,
            );
            ptr::write(ptr::addr_of_mut!((*out).vm), MaybeUninit::uninit());
            ptr::write(ptr::addr_of_mut!((*out).vm_active), false);
            ptr::write(ptr::addr_of_mut!((*out).scratch), [0; MAX_TEMP_SQBC_BYTES]);
        }
    }

    pub fn rebuild_app_registry(&mut self) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .rebuild(&mut self.scratch)
            .map_err(native_app_store_error)
    }

    pub fn reset(&mut self) {
        self.host.reset_runtime_state();
        self.vm_active = false;
    }

    pub fn take_prepared_sleep_checkpoint(
        &mut self,
    ) -> Result<Option<PowerCheckpoint>, NativeRuntimeError> {
        let Some(request) = self.host.power_backend.take_prepared_sleep() else {
            return Ok(None);
        };
        if self.host.active_sqbc != ActiveSqbc::Installed {
            return Err(NativeRuntimeError::Vm(VmError::InvalidOperand));
        }
        let active = self
            .host
            .foreground
            .active()
            .ok_or(NativeRuntimeError::Inactive)?;
        let mut checkpoint = PowerCheckpoint::new(active, request.wake_after_ms)
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        for index in 0..self.host.foreground.return_stack_len() {
            checkpoint
                .push_return_app(
                    self.host
                        .foreground
                        .return_stack_at(index)
                        .ok_or(NativeRuntimeError::InvalidOffset)?,
                )
                .map_err(|_| NativeRuntimeError::TooLarge)?;
        }
        for index in 0..self.host.foreground.armed_len() {
            checkpoint
                .push_armed_app(
                    self.host
                        .foreground
                        .armed_at(index)
                        .ok_or(NativeRuntimeError::InvalidOffset)?
                        .app_id,
                )
                .map_err(|_| NativeRuntimeError::TooLarge)?;
        }
        Ok(Some(checkpoint))
    }

    pub fn restore_power_checkpoint(
        &mut self,
        checkpoint: &PowerCheckpoint,
    ) -> Result<(), NativeRuntimeError> {
        if self
            .host
            .app_store
            .find(checkpoint.active_app.as_str())
            .is_none()
        {
            return Err(NativeRuntimeError::AppNotInstalled);
        }
        self.host.foreground.reset();
        let mut returns = [""; crate::lifecycle::MAX_RETURN_STACK];
        for (index, app_id) in checkpoint.return_apps[..checkpoint.return_len]
            .iter()
            .enumerate()
        {
            let value = app_id.as_str();
            if value != "main" && self.host.app_store.find(value).is_none() {
                return Err(NativeRuntimeError::AppNotInstalled);
            }
            returns[index] = value;
        }
        self.host
            .foreground
            .restore_return_stack(&returns[..checkpoint.return_len])
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        self.start_installed_app(checkpoint.active_app.as_str(), StartReason::Wake, false)?;
        for app_id in &checkpoint.armed_apps[..checkpoint.armed_len] {
            if self.host.app_store.find(app_id.as_str()).is_none() {
                return Err(NativeRuntimeError::AppNotInstalled);
            }
            self.host.arm_app(app_id.as_str())?;
        }
        Ok(())
    }

    pub fn save_power_checkpoint(
        &mut self,
        checkpoint: &PowerCheckpoint,
        out: &mut [u8],
    ) -> Result<(), NativeRuntimeError> {
        let len = checkpoint
            .encode(out)
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        self.host
            .app_store
            .storage_mut()
            .save_power_checkpoint_atomic(&out[..len])
            .map_err(native_app_store_error)
    }

    pub fn load_power_checkpoint(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<Option<PowerCheckpoint>, NativeRuntimeError> {
        let Some(len) = self
            .host
            .app_store
            .storage_mut()
            .load_power_checkpoint(buffer)
            .map_err(native_app_store_error)?
        else {
            return Ok(None);
        };
        PowerCheckpoint::decode(&buffer[..len])
            .map(Some)
            .map_err(|_| NativeRuntimeError::Vm(VmError::ReadFailed))
    }

    pub fn delete_power_checkpoint(&mut self) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .storage_mut()
            .delete_power_checkpoint()
            .map_err(native_app_store_error)
    }

    pub fn flush_app_storage(&mut self) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .storage_mut()
            .flush_app_storage()
            .map_err(native_app_store_error)
    }

    pub fn prepare_hardware_sleep(&mut self) -> Result<(), NativeRuntimeError> {
        self.host.stop_upload_profile();
        self.host.discard_upload_stage();
        self.host.release_all_radios();
        self.host.clear_timers();
        self.flush_app_storage()
    }

    pub fn begin_content_install(
        &mut self,
        name: &str,
        total_len: usize,
    ) -> Result<&str, &'static str> {
        self.host
            .file_backend
            .content_install_begin(name, total_len)
    }

    pub fn write_content_install_chunk(
        &mut self,
        path: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), &'static str> {
        self.host
            .file_backend
            .content_install_chunk(path, offset, bytes)
    }

    pub fn commit_content_install(&mut self, path: &str) -> Result<(), &'static str> {
        self.host.file_backend.content_install_commit(path)
    }

    pub fn check_content(
        &mut self,
        name: &str,
    ) -> Result<NativeContentCheckResult<'_>, &'static str> {
        self.host.file_backend.content_check(name)
    }

    pub fn file_ref_size(&mut self, path: &str) -> Result<u64, &'static str> {
        self.host.file_backend.file_ref_size(path)
    }

    pub fn file_ref_read_at(
        &mut self,
        path: &str,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), &'static str> {
        self.host.file_backend.file_ref_read_at(path, offset, out)
    }

    pub fn delete_content(&mut self, name: &str) -> Result<&str, &'static str> {
        self.host.file_backend.content_delete(name)
    }

    pub fn storage_format(&mut self) -> Result<(), &'static str> {
        self.reset();
        self.host.app_store.format().map_err(app_store_error_name)?;
        Ok(())
    }

    pub fn stage_ephemeral_upload(
        &mut self,
        name: &str,
        bytes: &[u8],
        id: &str,
        transport: NativeUploadTransport,
    ) -> Result<&str, NativeRuntimeError> {
        self.host.stage_ephemeral_upload(name, bytes, id, transport)
    }

    pub fn begin_ephemeral_upload(
        &mut self,
        name: &str,
        total_len: usize,
        id: &str,
        transport: NativeUploadTransport,
    ) -> Result<&str, NativeRuntimeError> {
        self.host
            .begin_ephemeral_upload(name, total_len, id, transport)
    }

    pub fn write_ephemeral_upload_chunk(
        &mut self,
        upload_path: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        self.host
            .write_ephemeral_upload_chunk(upload_path, offset, bytes)
    }

    pub fn commit_ephemeral_upload(
        &mut self,
        upload_path: &str,
        bytes_received: usize,
    ) -> Result<(), NativeRuntimeError> {
        self.host
            .commit_ephemeral_upload(upload_path, bytes_received)
    }

    pub fn abort_ephemeral_upload(&mut self, upload_path: &str) -> Result<(), NativeRuntimeError> {
        if upload_path != self.host.upload_path.as_str() {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        let _ = self.host.file_backend.upload_stage_delete(upload_path);
        self.host.clear_upload();
        Ok(())
    }

    pub fn abort_active_ephemeral_upload(&mut self) {
        self.host.discard_upload_stage();
    }

    pub fn active_upload_progress(&self) -> Option<NativeUploadProgress<'_>> {
        let transport = self.host.upload_transport?;
        Some(NativeUploadProgress {
            path: self.host.upload_path.as_str(),
            name: self.host.upload_name.as_str(),
            id: self.host.upload_id.as_str(),
            transport,
            bytes_received: self.host.upload_received_bytes,
            total_bytes: self.host.upload_total_bytes,
        })
    }

    pub fn set_wifi_profile(
        &mut self,
        profile: &str,
        ssid: &str,
        password: &str,
    ) -> Result<(), NativeRuntimeError> {
        self.host.set_wifi_profile(profile, ssid, password)
    }

    pub fn begin_temp_run(
        &mut self,
        app_id: &str,
        total_len: usize,
    ) -> Result<(), NativeRuntimeError> {
        if total_len == 0 || total_len > self.host.temp_sqbc.len() {
            return Err(NativeRuntimeError::TooLarge);
        }
        self.host.begin_temp_run(app_id, total_len)
    }

    pub fn write_temp_run_chunk(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        self.host.write_temp_run_chunk(offset, bytes)
    }

    pub fn commit_temp_run(&mut self) -> Result<(), NativeRuntimeError> {
        if self.host.temp_received != self.host.temp_expected_len {
            return Err(NativeRuntimeError::IncompleteTempRun);
        }
        let push_active = self.vm_active && self.host.active_sqbc != ActiveSqbc::Temp;
        if push_active && !self.host.foreground.can_push_active() {
            return Err(NativeRuntimeError::TooLarge);
        }
        if self.vm_active {
            self.host.foreground.set_phase(LifecyclePhase::Exiting);
            self.host.refresh_lifecycle_lines();
            let vm = unsafe { self.vm.assume_init_mut() };
            match vm.dispatch(&mut self.host, "app.exit") {
                Ok(()) | Err(VmError::HandlerNotFound) => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.host
            .foreground
            .begin_foreground(
                self.host.temp_app_id.as_str(),
                StartReason::Launch,
                push_active,
            )
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        self.host.release_all_radios();
        self.host.clear_timers();
        self.host.clear_upload_profile();
        self.host.discard_upload_stage();
        self.host.clear_diagnostics();
        self.host.app_id = self.host.temp_app_id;
        self.host.state_cache_len = None;
        self.host.active_sqbc = ActiveSqbc::Temp;
        let mut reader = SliceSqbcReader::new(self.host.temp_bytes());
        self.host.active_demand =
            ProgramIndex::capability_demand_from_reader(&mut reader, &mut self.scratch)?;
        let mut reader = SliceSqbcReader::new(self.host.temp_bytes());
        unsafe {
            ChunkedVm::init_in_place_from_reader(
                self.vm.as_mut_ptr(),
                &mut reader,
                &mut self.scratch,
            )?;
        }
        self.vm_active = true;
        match self.dispatch_app_start() {
            Ok(()) | Err(NativeRuntimeError::Vm(VmError::HandlerNotFound)) => {}
            Err(error) => return Err(error),
        }
        self.host.foreground.set_phase(LifecyclePhase::Idle);
        self.host.refresh_lifecycle_lines();
        Ok(())
    }

    pub fn begin_app_install(
        &mut self,
        app_id: &str,
        total_len: usize,
    ) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .begin_install(app_id, total_len)
            .map_err(native_app_store_error)
    }

    pub fn write_app_install_chunk(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .write_install_chunk(offset, bytes)
            .map_err(native_app_store_error)
    }

    pub fn commit_app_install(&mut self) -> Result<(), NativeRuntimeError> {
        let mut replaced = FixedText::<MAX_APP_ID_BYTES>::new();
        if let Some(app_id) = self.host.app_store.pending_app_id() {
            replaced.set(app_id)?;
        }
        self.host
            .app_store
            .commit_install(&mut self.scratch)
            .map_err(native_app_store_error)?;
        if !replaced.as_str().is_empty() {
            self.host.foreground.disarm(replaced.as_str());
            self.host.refresh_lifecycle_lines();
        }
        Ok(())
    }

    pub fn begin_resource_install(
        &mut self,
        app_id: &str,
        path: &str,
        total_len: usize,
    ) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .begin_resource_install(app_id, path, total_len)
            .map_err(native_app_store_error)
    }

    pub fn write_resource_install_chunk(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .write_resource_chunk(offset, bytes)
            .map_err(native_app_store_error)
    }

    pub fn commit_resource_install(&mut self) -> Result<(), NativeRuntimeError> {
        self.host
            .app_store
            .commit_resource_install()
            .map_err(native_app_store_error)
    }

    pub fn launch_app(&mut self, app_id: &str) -> Result<(), NativeRuntimeError> {
        if self.host.app_store.find(app_id).is_none() {
            return Err(NativeRuntimeError::AppNotInstalled);
        }
        let push_active = self.vm_active;
        if push_active && !self.host.foreground.can_push_active() {
            return Err(NativeRuntimeError::TooLarge);
        }
        if push_active {
            self.host.foreground.set_phase(LifecyclePhase::Exiting);
            self.host.refresh_lifecycle_lines();
            let vm = unsafe { self.vm.assume_init_mut() };
            match vm.dispatch(&mut self.host, "app.exit") {
                Ok(()) | Err(VmError::HandlerNotFound) => {}
                Err(error) => return Err(error.into()),
            }
            if self.host.take_pending_launch()?.is_some() {
                return Err(NativeRuntimeError::InvalidOffset);
            }
        }
        self.start_installed_app(app_id, StartReason::Launch, push_active)
    }

    pub fn boot_app(&mut self, app_id: &str) -> Result<(), NativeRuntimeError> {
        self.start_installed_app(app_id, StartReason::Boot, false)
    }

    fn start_installed_app(
        &mut self,
        app_id: &str,
        reason: StartReason,
        push_active: bool,
    ) -> Result<(), NativeRuntimeError> {
        let installed = self
            .host
            .app_store
            .find(app_id)
            .ok_or(NativeRuntimeError::AppNotInstalled)?;
        self.host
            .foreground
            .begin_foreground(app_id, reason, push_active)
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        self.host.release_all_radios();
        self.host.clear_diagnostics();
        self.host.clear_timers();
        self.host.app_id.set(app_id)?;
        self.host.state_cache_len = self
            .host
            .app_store
            .load_state(app_id, &mut self.host.state_cache)
            .map_err(native_app_store_error)?;
        self.host.active_sqbc = ActiveSqbc::Installed;
        self.host.refresh_lifecycle_lines();
        let mut reader =
            ActiveAppReader::new(&mut self.host.app_store, app_id, installed.sqbc_bytes);
        self.host.active_demand =
            ProgramIndex::capability_demand_from_reader(&mut reader, &mut self.scratch)?;
        let mut reader =
            ActiveAppReader::new(&mut self.host.app_store, app_id, installed.sqbc_bytes);
        unsafe {
            ChunkedVm::init_in_place_from_reader(
                self.vm.as_mut_ptr(),
                &mut reader,
                &mut self.scratch,
            )?;
        }
        self.vm_active = true;
        match self.dispatch_app_start() {
            Ok(()) | Err(NativeRuntimeError::Vm(VmError::HandlerNotFound)) => {}
            Err(error) => return Err(error),
        }
        self.host.foreground.set_phase(LifecyclePhase::Idle);
        self.host.refresh_lifecycle_lines();
        Ok(())
    }

    pub fn launch_fallback(&mut self, sqbc: &'static [u8]) -> Result<(), NativeRuntimeError> {
        self.start_fallback(sqbc, StartReason::Boot, false)
    }

    fn start_fallback(
        &mut self,
        sqbc: &'static [u8],
        reason: StartReason,
        push_active: bool,
    ) -> Result<(), NativeRuntimeError> {
        self.host
            .foreground
            .begin_foreground("main", reason, push_active)
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        self.host.release_all_radios();
        self.host.clear_diagnostics();
        self.host.clear_timers();
        self.host.app_id.set("main")?;
        self.host.state_cache_len = None;
        self.host.fallback_sqbc = sqbc;
        self.host.active_sqbc = ActiveSqbc::Fallback;
        self.host.refresh_lifecycle_lines();
        let mut reader = SliceSqbcReader::new(sqbc);
        self.host.active_demand =
            ProgramIndex::capability_demand_from_reader(&mut reader, &mut self.scratch)?;
        let mut reader = SliceSqbcReader::new(sqbc);
        unsafe {
            ChunkedVm::init_in_place_from_reader(
                self.vm.as_mut_ptr(),
                &mut reader,
                &mut self.scratch,
            )?;
        }
        self.vm_active = true;
        match self.dispatch_app_start() {
            Ok(()) | Err(NativeRuntimeError::Vm(VmError::HandlerNotFound)) => {}
            Err(error) => return Err(error),
        }
        self.host.foreground.set_phase(LifecyclePhase::Idle);
        self.host.refresh_lifecycle_lines();
        Ok(())
    }

    pub fn set_system_memory_metrics(
        &mut self,
        total_ram_bytes: usize,
        heap_used_bytes: usize,
        heap_free_bytes: usize,
    ) {
        self.host.total_ram_bytes = total_ram_bytes;
        self.host.heap_used_bytes = heap_used_bytes;
        self.host.heap_free_bytes = heap_free_bytes;
    }

    pub fn installed_app(&self) -> Option<(&str, usize)> {
        let app_id = self.host.app_id.as_str();
        self.host
            .app_store
            .find(app_id)
            .map(|entry| (app_id, entry.sqbc_bytes))
    }

    pub fn app_registry(&self) -> &[Option<crate::app_store::AppRegistryEntry>] {
        self.host.app_store.registry()
    }

    pub fn lifecycle_process_len(&self) -> usize {
        self.host.foreground.return_stack_len()
    }

    pub fn lifecycle_process_at(&self, index: usize) -> Option<&str> {
        self.host.foreground.return_stack_at(index)
    }

    pub fn lifecycle_armed_len(&self) -> usize {
        self.host.foreground.armed_len()
    }

    pub fn lifecycle_armed_at(&self, index: usize) -> Option<(&str, &str)> {
        self.host
            .foreground
            .armed_at(index)
            .map(|route| (route.app_id, route.event))
    }

    pub fn lifecycle_phase(&self) -> &'static str {
        self.host.foreground.phase().as_str()
    }

    pub fn lifecycle_start_reason(&self) -> &'static str {
        self.host.foreground.start_reason().as_str()
    }

    pub fn lifecycle_queue_len(&self) -> usize {
        self.host.foreground.queue_len()
    }

    pub fn lifecycle_queue_overflowed(&self) -> bool {
        self.host.foreground.queue_overflowed()
    }

    pub fn output_lines(&self) -> LineView<'_> {
        self.host.output.view()
    }

    pub fn trace_lines(&self) -> LineView<'_> {
        self.host.trace.view()
    }

    pub fn record_trace(&mut self, message: &str) {
        self.host.trace.push(message);
    }

    pub fn drawlog_lines(&self) -> LineView<'_> {
        self.host.drawlog.view()
    }

    pub fn error_lines(&self) -> LineView<'_> {
        self.host.errors.view()
    }

    pub fn record_error(&mut self, error: &str) {
        self.host.errors.push(error);
    }

    pub fn record_debug_trace(&mut self, line: &str) {
        self.host.trace.push(line);
    }

    pub fn clear_errors(&mut self) {
        self.host.errors.clear();
    }

    pub fn complete_wifi_scan(&mut self, count: i32) -> Result<(), NativeRuntimeError> {
        if self.host.wifi_operation.kind != Some("scan") || !self.host.wifi_operation.active {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.host
            .record_wifi_operation(NativeWifiOperationState::done_with_count("scan", count));
        self.host.ensure_radio_inactive(RadioKind::Wifi)?;
        Ok(())
    }

    pub fn fail_wifi_scan(&mut self, error: &'static str) -> Result<(), NativeRuntimeError> {
        if self.host.wifi_operation.kind != Some("scan") || !self.host.wifi_operation.active {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.host
            .record_wifi_operation(NativeWifiOperationState::error("scan", error));
        self.host.ensure_radio_inactive(RadioKind::Wifi)?;
        Ok(())
    }

    pub fn complete_wifi_connect(&mut self) -> Result<(), NativeRuntimeError> {
        if self.host.wifi_operation.kind != Some("connect") || !self.host.wifi_operation.active {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.host
            .record_wifi_operation(NativeWifiOperationState::done("connect"));
        Ok(())
    }

    pub fn fail_wifi_connect(&mut self, error: &'static str) -> Result<(), NativeRuntimeError> {
        if self.host.wifi_operation.kind != Some("connect") || !self.host.wifi_operation.active {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.host
            .record_wifi_operation(NativeWifiOperationState::error("connect", error));
        self.host.ensure_radio_inactive(RadioKind::Wifi)?;
        Ok(())
    }

    pub fn complete_wifi_start_ap(&mut self) -> Result<(), NativeRuntimeError> {
        if self.host.wifi_operation.kind != Some("startAP") || !self.host.wifi_operation.active {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.host
            .record_wifi_operation(NativeWifiOperationState::done("startAP"));
        Ok(())
    }

    pub fn fail_wifi_start_ap(&mut self, error: &'static str) -> Result<(), NativeRuntimeError> {
        if self.host.wifi_operation.kind != Some("startAP") || !self.host.wifi_operation.active {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.host
            .record_wifi_operation(NativeWifiOperationState::error("startAP", error));
        self.host.ensure_radio_inactive(RadioKind::Wifi)?;
        Ok(())
    }

    pub fn wifi_operation_result(&self) -> WifiOperationResult<'static> {
        self.host.wifi_operation.result()
    }

    pub fn wifi_operation_active_kind(&self) -> Option<&'static str> {
        if self.host.wifi_operation.active && !self.host.wifi_operation.done {
            self.host.wifi_operation.kind
        } else {
            None
        }
    }

    pub fn state_bytes(&self) -> &[u8] {
        self.host.state_bytes()
    }

    pub fn import_state(&mut self, bytes: &[u8]) -> Result<(), NativeRuntimeError> {
        self.host.import_state(bytes)
    }

    pub fn active_app(&self) -> Option<&str> {
        self.vm_active.then(|| self.host.app_id.as_str())
    }

    pub fn resolve_upload_route(
        &mut self,
        name: &str,
        transport: NativeUploadTransport,
    ) -> Result<NativeUploadRoute, NativeUploadRouteError> {
        if !self.vm_active
            || self.host.upload_profile_id.as_str().is_empty()
            || !self.host.upload_transport_enabled(transport)
        {
            return Err(NativeUploadRouteError::NoActiveProfile);
        }
        let active_profile_id = self.host.upload_profile_id;
        let count =
            ProgramIndex::upload_profile_count_from_reader(&mut self.host, &mut self.scratch)
                .map_err(|_| NativeUploadRouteError::InvalidMetadata)?;
        let mut matched = None;
        for index in 0..count {
            let profile =
                ProgramIndex::upload_profile_from_reader(&mut self.host, &mut self.scratch, index)
                    .map_err(|_| NativeUploadRouteError::InvalidMetadata)?;
            if profile.role != "server" || profile.id != active_profile_id.as_str() {
                continue;
            }
            if !(0..profile.transports.len())
                .any(|index| profile.transports.get(index) == Some(transport.as_str()))
            {
                continue;
            }
            let Some(complete_route) = (0..profile.events.len())
                .filter_map(|event_index| profile.events.get(event_index))
                .find(|event| event.kind == "complete")
            else {
                return Err(NativeUploadRouteError::InvalidMetadata);
            };
            for accept_index in 0..profile.accept.len() {
                let Some(extension) = profile.accept.get(accept_index) else {
                    continue;
                };
                if !extension.is_empty() && name.ends_with(extension) {
                    let route = NativeUploadRoute::new(profile.id, complete_route.event)?;
                    if matched.replace(route).is_some() {
                        return Err(NativeUploadRouteError::RouteAmbiguous);
                    }
                }
            }
        }
        matched.ok_or(NativeUploadRouteError::RouteMismatch)
    }

    pub fn resource_metrics(&self) -> ResourceMetrics {
        let metrics = [
            ResourceMetric {
                key: "ram_total_bytes",
                value: 400 * 1024,
            },
            ResourceMetric {
                key: "runtime_static_bytes",
                value: core::mem::size_of::<Self>() as u64,
            },
            ResourceMetric {
                key: "vm_sqbc_chunk_bytes",
                value: squidvm_core::limits::MAX_CODE_CHUNK_BYTES as u64,
            },
            ResourceMetric {
                key: "runtime_current_app_present",
                value: u64::from(self.vm_active),
            },
            ResourceMetric {
                key: "runtime_lifecycle_phase",
                value: self.host.foreground.phase().code(),
            },
            ResourceMetric {
                key: "last_sqbc_reads",
                value: self.host.sqbc_reads as u64,
            },
            ResourceMetric {
                key: "last_sqbc_bytes",
                value: self.host.sqbc_bytes as u64,
            },
            ResourceMetric {
                key: "radio_active_leases",
                value: self.host.radio_leases.active_count() as u64,
            },
            ResourceMetric {
                key: "radio_wifi_active",
                value: u64::from(
                    self.host.radio_leases.state(RadioKind::Wifi) == RadioLeaseState::Active,
                ),
            },
            ResourceMetric {
                key: "radio_ble_active",
                value: u64::from(
                    self.host.radio_leases.state(RadioKind::Ble) == RadioLeaseState::Active,
                ),
            },
            ResourceMetric {
                key: "upload_profile_active",
                value: u64::from(!self.host.upload_profile_id.as_str().is_empty()),
            },
            ResourceMetric {
                key: "upload_profile_id_len",
                value: self.host.upload_profile_id.as_str().len() as u64,
            },
            ResourceMetric {
                key: "upload_profile_start_events",
                value: self.host.upload_profile_start_events as u64,
            },
            ResourceMetric {
                key: "upload_profile_stop_events",
                value: self.host.upload_profile_stop_events as u64,
            },
            ResourceMetric {
                key: "upload_transport_http_active",
                value: u64::from(self.host.upload_profile_http),
            },
            ResourceMetric {
                key: "upload_transport_ble_active",
                value: u64::from(self.host.upload_profile_ble),
            },
            ResourceMetric {
                key: "display_pending_refreshes",
                value: self.host.display_sink.pending_refreshes() as u64,
            },
            ResourceMetric {
                key: "display_recorded_draws",
                value: self.host.display_sink.recorded_draws() as u64,
            },
            ResourceMetric {
                key: "display_dropped_draws",
                value: self.host.display_sink.dropped_draws() as u64,
            },
            ResourceMetric {
                key: "demand_wifi",
                value: u64::from(self.host.active_demand.wifi),
            },
            ResourceMetric {
                key: "demand_ble",
                value: u64::from(self.host.active_demand.ble),
            },
            ResourceMetric {
                key: "demand_http",
                value: u64::from(self.host.active_demand.http),
            },
            ResourceMetric {
                key: "demand_display",
                value: u64::from(self.host.active_demand.display),
            },
            ResourceMetric {
                key: "demand_storage",
                value: u64::from(self.host.active_demand.storage),
            },
            ResourceMetric {
                key: "demand_binbook",
                value: u64::from(self.host.active_demand.binbook),
            },
        ];
        ResourceMetrics {
            metrics,
            len: metrics.len(),
        }
    }

    fn dispatch_app_start(&mut self) -> Result<(), NativeRuntimeError> {
        self.dispatch_event("app.start")
    }

    pub fn dispatch_event(&mut self, event: &str) -> Result<(), NativeRuntimeError> {
        if !self.vm_active {
            return Err(NativeRuntimeError::Inactive);
        }
        self.host.foreground.set_phase(LifecyclePhase::Dispatching);
        self.host.refresh_lifecycle_lines();
        let vm = unsafe { self.vm.assume_init_mut() };
        let result = vm.dispatch(&mut self.host, event);
        self.host.foreground.set_phase(LifecyclePhase::Idle);
        self.host.refresh_lifecycle_lines();
        if let Err(error) = result {
            self.host.power_backend.abort_sleep();
            return Err(error.into());
        }
        if event != "power.sleep" {
            if let Some(request) = self.host.power_backend.take_requested_sleep() {
                let vm = unsafe { self.vm.assume_init_mut() };
                match vm.dispatch(&mut self.host, "power.sleep") {
                    Ok(()) | Err(VmError::HandlerNotFound) => {
                        self.host.power_backend.prepare_sleep(request);
                    }
                    Err(error) => {
                        self.host.power_backend.abort_sleep();
                        return Err(error.into());
                    }
                }
            }
        }
        if let Err(error) = self.complete_pending_launch() {
            self.host.power_backend.abort_sleep();
            return Err(error);
        }
        Ok(())
    }

    pub fn enqueue_input_event(&mut self, event: &str) -> Result<(), NativeRuntimeError> {
        if let Err(error) = self.host.foreground.enqueue_input(event) {
            if error == LifecycleError::EventQueueFull {
                self.host.errors.push("lifecycle_event_queue_overflow");
            }
            return Err(NativeRuntimeError::TooLarge);
        }
        self.drain_pending_events()
    }

    fn drain_pending_events(&mut self) -> Result<(), NativeRuntimeError> {
        while self.vm_active && self.host.foreground.phase() == LifecyclePhase::Idle {
            let Some(pending) = self.host.foreground.pop_event() else {
                break;
            };
            let mut owner = FixedText::<MAX_APP_ID_BYTES>::new();
            if let Some(app_id) = pending.owner {
                owner.set(app_id)?;
            }
            let mut event = FixedText::<MAX_EVENT_NAME_BYTES>::new();
            event.set(pending.event)?;
            if !owner.as_str().is_empty() && self.active_app() != Some(owner.as_str()) {
                self.launch_app(owner.as_str())?;
            }
            self.dispatch_event(event.as_str())?;
        }
        self.host.refresh_lifecycle_lines();
        Ok(())
    }

    pub fn tick_timers(&mut self, elapsed_ms: u32) -> Result<(), NativeRuntimeError> {
        if !self.vm_active || elapsed_ms == 0 {
            return Ok(());
        }
        let mut event = [0u8; MAX_TIMER_EVENT_BYTES];
        while let Some(event_len) = self.host.tick_timers(elapsed_ms, &mut event) {
            let event = core::str::from_utf8(&event[..event_len]).unwrap_or("");
            self.host
                .foreground
                .enqueue_foreground(event)
                .map_err(|_| NativeRuntimeError::TooLarge)?;
        }
        if let Err(error) = self.host.foreground.tick(elapsed_ms) {
            if error == LifecycleError::EventQueueFull {
                self.host.errors.push("lifecycle_event_queue_overflow");
            }
            return Err(NativeRuntimeError::TooLarge);
        }
        self.drain_pending_events()
    }

    pub fn dispatch_app_event(
        &mut self,
        app_id: &str,
        event: &str,
    ) -> Result<(), NativeRuntimeError> {
        if self.active_app() != Some(app_id) {
            return Err(NativeRuntimeError::AppIdMismatch);
        }
        self.dispatch_event(event)
    }

    pub fn dispatch_upload_complete(
        &mut self,
        app_id: &str,
        event: &str,
        upload_path: &str,
    ) -> Result<(), NativeRuntimeError> {
        if self.active_app() != Some(app_id) {
            return Err(NativeRuntimeError::AppIdMismatch);
        }
        if upload_path != self.host.upload_path.as_str() {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        let result = self.dispatch_upload_event(event);
        let _ = self.host.file_backend.upload_stage_delete(upload_path);
        self.host.clear_upload();
        result
    }

    pub fn dispatch_active_upload_complete(
        &mut self,
        event: &str,
        upload_path: &str,
    ) -> Result<(), NativeRuntimeError> {
        let mut app_id = FixedText::<MAX_APP_ID_BYTES>::new();
        app_id.set(self.active_app().ok_or(NativeRuntimeError::Inactive)?)?;
        self.dispatch_upload_complete(app_id.as_str(), event, upload_path)
    }

    fn complete_pending_launch(&mut self) -> Result<(), NativeRuntimeError> {
        if let Some(app_id) = self.host.take_pending_launch()? {
            self.launch_app(app_id.as_str())?;
        } else if self.vm_active && unsafe { self.vm.assume_init_ref() }.exited() {
            let mut target = FixedText::<MAX_APP_ID_BYTES>::new();
            target.set(
                self.host
                    .foreground
                    .return_target()
                    .map_err(|_| NativeRuntimeError::InvalidOffset)?
                    .app_id,
            )?;
            if self.host.app_store.find(target.as_str()).is_some() {
                self.start_installed_app(target.as_str(), StartReason::Return, false)?;
            } else if target.as_str() == "main" && !self.host.fallback_sqbc.is_empty() {
                self.start_fallback(self.host.fallback_sqbc, StartReason::Return, false)?;
            } else if target.as_str() == "main" {
                self.vm_active = false;
                self.host.set_inactive_lifecycle();
            } else {
                return Err(NativeRuntimeError::AppNotInstalled);
            }
        }
        Ok(())
    }

    fn dispatch_upload_event(&mut self, event: &str) -> Result<(), NativeRuntimeError> {
        if !self.vm_active {
            return Err(NativeRuntimeError::Inactive);
        }
        let mut upload_path = FixedText::<MAX_UPLOAD_REF_BYTES>::new();
        let mut upload_name = FixedText::<MAX_UPLOAD_NAME_BYTES>::new();
        let mut upload_id = FixedText::<MAX_BLE_PROFILE_ID_BYTES>::new();
        let mut upload_bytes = FixedText::<MAX_UPLOAD_BYTES_TEXT_BYTES>::new();
        let mut upload_total_bytes = FixedText::<MAX_UPLOAD_BYTES_TEXT_BYTES>::new();
        let upload_transport = self
            .host
            .upload_transport
            .ok_or(NativeRuntimeError::InvalidOffset)?;
        upload_path.set(self.host.upload_path.as_str())?;
        upload_name.set(self.host.upload_name.as_str())?;
        upload_id.set(self.host.upload_id.as_str())?;
        upload_bytes.set(self.host.upload_received_bytes_text.as_str())?;
        upload_total_bytes.set(self.host.upload_total_bytes_text.as_str())?;
        let fields = [
            EventPayloadField {
                name: "upload",
                value: upload_path.as_str(),
            },
            EventPayloadField {
                name: "name",
                value: upload_name.as_str(),
            },
            EventPayloadField {
                name: "bytesReceived",
                value: upload_bytes.as_str(),
            },
            EventPayloadField {
                name: "totalBytes",
                value: upload_total_bytes.as_str(),
            },
            EventPayloadField {
                name: "id",
                value: upload_id.as_str(),
            },
            EventPayloadField {
                name: "transport",
                value: upload_transport.as_str(),
            },
        ];
        let payload = EventPayload { fields: &fields };
        let vm = unsafe { self.vm.assume_init_mut() };
        vm.dispatch_with_payload(&mut self.host, event, payload)?;
        self.complete_pending_launch()?;
        Ok(())
    }

    pub fn radio_backend(&self) -> &B {
        &self.host.radio_backend
    }

    pub fn radio_backend_mut(&mut self) -> &mut B {
        &mut self.host.radio_backend
    }

    pub fn display_sink(&self) -> &D {
        &self.host.display_sink
    }

    pub fn display_sink_mut(&mut self) -> &mut D {
        &mut self.host.display_sink
    }

    pub fn display_sink_and_file_backend_mut(&mut self) -> (&mut D, &mut F) {
        (&mut self.host.display_sink, &mut self.host.file_backend)
    }

    pub fn binbook_backend(&self) -> &C {
        &self.host.binbook_backend
    }

    pub fn file_backend(&self) -> &F {
        &self.host.file_backend
    }
}

impl<
        B: NativeRadioBackend,
        D: NativeDisplaySink,
        C: NativeBinBookBackend,
        F: NativeFileBackend,
    > NativeRuntime<B, D, C, F, VolatileAppStorage>
{
    pub const fn with_radio_display_binbook_and_file(
        radio_backend: B,
        display_sink: D,
        binbook_backend: C,
        file_backend: F,
    ) -> Self {
        Self::with_radio_display_binbook_file_and_app_store(
            radio_backend,
            display_sink,
            binbook_backend,
            file_backend,
            VolatileAppStorage::new(),
        )
    }
}

impl Default
    for NativeRuntime<
        NoopRadioBackend,
        NoopDisplaySink,
        NoopBinBookBackend,
        NoopFileBackend,
        VolatileAppStorage,
    >
{
    fn default() -> Self {
        Self::new()
    }
}

impl<B, D, C, F> NativeRuntime<B, D, C, F, VolatileAppStorage> {
    pub fn app_storage_write_calls(&self) -> usize {
        self.host.app_store.storage().storage_write_calls()
    }
}

struct RuntimeHost<
    B = NoopRadioBackend,
    D = NoopDisplaySink,
    C = NoopBinBookBackend,
    F = NoopFileBackend,
    A = VolatileAppStorage,
> {
    temp_sqbc: [u8; MAX_TEMP_SQBC_BYTES],
    temp_expected_len: usize,
    temp_received: usize,
    temp_app_id: FixedText<MAX_APP_ID_BYTES>,
    pending_launch_app_id: FixedText<MAX_APP_ID_BYTES>,
    last_installed_app_id: FixedText<MAX_APP_ID_BYTES>,
    active_sqbc: ActiveSqbc,
    fallback_sqbc: &'static [u8],
    app_id: FixedText<MAX_APP_ID_BYTES>,
    state_cache: [u8; MAX_SAVED_STATE_BYTES],
    state_cache_len: Option<usize>,
    resource_text: [u8; MAX_APP_RESOURCE_TEXT_BYTES],
    registry_view: [AppRegistryEntry<'static>; squidvm_core::limits::MAX_INSTALLED_APPS],
    process_view: [&'static str; crate::lifecycle::MAX_RETURN_STACK],
    armed_view: [AppArmedStackEntry<'static>; MAX_ARMED_TIMERS + MAX_ARMED_INPUTS],
    output: LineStore,
    trace: LineStore,
    drawlog: LineStore,
    foreground: ForegroundLifecycle,
    errors: LineStore,
    active_demand: CapabilityDemand,
    radio_leases: RadioLeaseManager,
    wifi_operation: NativeWifiOperationState,
    wifi_profile_name: FixedText<MAX_WIFI_PROFILE_NAME_BYTES>,
    wifi_profile_ssid: FixedText<MAX_WIFI_PROFILE_SSID_BYTES>,
    wifi_profile_password: FixedText<MAX_WIFI_PROFILE_PASSWORD_BYTES>,
    wifi_station_profile: FixedText<MAX_WIFI_PROFILE_NAME_BYTES>,
    upload_profile_id: FixedText<MAX_BLE_PROFILE_ID_BYTES>,
    upload_profile_http: bool,
    upload_profile_ble: bool,
    upload_profile_start_events: u32,
    upload_profile_stop_events: u32,
    upload_last_error: FixedText<MAX_LINE_BYTES>,
    timers: [NativeTimer; MAX_FOREGROUND_TIMERS],
    upload_path: FixedText<MAX_UPLOAD_REF_BYTES>,
    upload_name: FixedText<MAX_UPLOAD_NAME_BYTES>,
    upload_id: FixedText<MAX_BLE_PROFILE_ID_BYTES>,
    upload_transport: Option<NativeUploadTransport>,
    upload_total_bytes: usize,
    upload_received_bytes: usize,
    upload_total_bytes_text: FixedText<MAX_UPLOAD_BYTES_TEXT_BYTES>,
    upload_received_bytes_text: FixedText<MAX_UPLOAD_BYTES_TEXT_BYTES>,
    power_backend: DeferredNativePowerBackend,
    radio_backend: B,
    display_sink: D,
    binbook_backend: C,
    file_backend: F,
    app_store: NativeAppStore<A>,
    sqbc_reads: usize,
    sqbc_bytes: usize,
    total_ram_bytes: usize,
    heap_used_bytes: usize,
    heap_free_bytes: usize,
}

impl<
        B: NativeRadioBackend,
        D: NativeDisplaySink,
        C: NativeBinBookBackend,
        F: NativeFileBackend,
        A: NativeAppStorage,
    > RuntimeHost<B, D, C, F, A>
{
    #[inline(always)]
    const fn new(
        radio_backend: B,
        display_sink: D,
        binbook_backend: C,
        file_backend: F,
        app_storage: A,
    ) -> Self {
        Self {
            temp_sqbc: [0; MAX_TEMP_SQBC_BYTES],
            temp_expected_len: 0,
            temp_received: 0,
            temp_app_id: FixedText::new(),
            pending_launch_app_id: FixedText::new(),
            last_installed_app_id: FixedText::new(),
            active_sqbc: ActiveSqbc::Temp,
            fallback_sqbc: &[],
            app_id: FixedText::new(),
            state_cache: [0; MAX_SAVED_STATE_BYTES],
            state_cache_len: None,
            resource_text: [0; MAX_APP_RESOURCE_TEXT_BYTES],
            registry_view: [AppRegistryEntry {
                id: "",
                name: "",
                build: "",
                description: "",
            }; squidvm_core::limits::MAX_INSTALLED_APPS],
            process_view: [""; crate::lifecycle::MAX_RETURN_STACK],
            armed_view: [AppArmedStackEntry {
                app_id: "",
                event: "",
            }; MAX_ARMED_TIMERS + MAX_ARMED_INPUTS],
            output: LineStore::new(),
            trace: LineStore::new(),
            drawlog: LineStore::new(),
            foreground: ForegroundLifecycle::new(),
            errors: LineStore::new(),
            active_demand: CapabilityDemand::none(),
            radio_leases: RadioLeaseManager::new(),
            wifi_operation: NativeWifiOperationState::idle(),
            wifi_profile_name: FixedText::new(),
            wifi_profile_ssid: FixedText::new(),
            wifi_profile_password: FixedText::new(),
            wifi_station_profile: FixedText::new(),
            upload_profile_id: FixedText::new(),
            upload_profile_http: false,
            upload_profile_ble: false,
            upload_profile_start_events: 0,
            upload_profile_stop_events: 0,
            upload_last_error: FixedText::new(),
            timers: [NativeTimer::empty(); MAX_FOREGROUND_TIMERS],
            upload_path: FixedText::new(),
            upload_name: FixedText::new(),
            upload_id: FixedText::new(),
            upload_transport: None,
            upload_total_bytes: 0,
            upload_received_bytes: 0,
            upload_total_bytes_text: FixedText::new(),
            upload_received_bytes_text: FixedText::new(),
            power_backend: DeferredNativePowerBackend::new(),
            radio_backend,
            display_sink,
            binbook_backend,
            file_backend,
            app_store: NativeAppStore::new(app_storage),
            sqbc_reads: 0,
            sqbc_bytes: 0,
            total_ram_bytes: 0,
            heap_used_bytes: 0,
            heap_free_bytes: 0,
        }
    }

    unsafe fn init_in_place(
        out: *mut Self,
        radio_backend: B,
        display_sink: D,
        binbook_backend: C,
        file_backend: F,
        app_storage: A,
    ) {
        macro_rules! write_field {
            ($field:ident, $value:expr) => {
                ptr::write(ptr::addr_of_mut!((*out).$field), $value)
            };
        }
        unsafe {
            write_field!(temp_sqbc, [0; MAX_TEMP_SQBC_BYTES]);
            write_field!(temp_expected_len, 0);
            write_field!(temp_received, 0);
            write_field!(temp_app_id, FixedText::new());
            write_field!(pending_launch_app_id, FixedText::new());
            write_field!(last_installed_app_id, FixedText::new());
            write_field!(active_sqbc, ActiveSqbc::Temp);
            write_field!(fallback_sqbc, &[]);
            write_field!(app_id, FixedText::new());
            write_field!(state_cache, [0; MAX_SAVED_STATE_BYTES]);
            write_field!(state_cache_len, None);
            write_field!(resource_text, [0; MAX_APP_RESOURCE_TEXT_BYTES]);
            write_field!(
                registry_view,
                [AppRegistryEntry {
                    id: "",
                    name: "",
                    build: "",
                    description: "",
                }; squidvm_core::limits::MAX_INSTALLED_APPS]
            );
            write_field!(process_view, [""; crate::lifecycle::MAX_RETURN_STACK]);
            write_field!(
                armed_view,
                [AppArmedStackEntry {
                    app_id: "",
                    event: "",
                }; MAX_ARMED_TIMERS + MAX_ARMED_INPUTS]
            );
            write_field!(output, LineStore::new());
            write_field!(trace, LineStore::new());
            write_field!(drawlog, LineStore::new());
            write_field!(foreground, ForegroundLifecycle::new());
            write_field!(errors, LineStore::new());
            write_field!(active_demand, CapabilityDemand::none());
            write_field!(radio_leases, RadioLeaseManager::new());
            write_field!(wifi_operation, NativeWifiOperationState::idle());
            write_field!(wifi_profile_name, FixedText::new());
            write_field!(wifi_profile_ssid, FixedText::new());
            write_field!(wifi_profile_password, FixedText::new());
            write_field!(wifi_station_profile, FixedText::new());
            write_field!(upload_profile_id, FixedText::new());
            write_field!(upload_profile_http, false);
            write_field!(upload_profile_ble, false);
            write_field!(upload_profile_start_events, 0);
            write_field!(upload_profile_stop_events, 0);
            write_field!(upload_last_error, FixedText::new());
            write_field!(timers, [NativeTimer::empty(); MAX_FOREGROUND_TIMERS]);
            write_field!(upload_path, FixedText::new());
            write_field!(upload_name, FixedText::new());
            write_field!(upload_id, FixedText::new());
            write_field!(upload_transport, None);
            write_field!(upload_total_bytes, 0);
            write_field!(upload_received_bytes, 0);
            write_field!(upload_total_bytes_text, FixedText::new());
            write_field!(upload_received_bytes_text, FixedText::new());
            write_field!(power_backend, DeferredNativePowerBackend::new());
            write_field!(radio_backend, radio_backend);
            write_field!(display_sink, display_sink);
            write_field!(binbook_backend, binbook_backend);
            write_field!(file_backend, file_backend);
            write_field!(app_store, NativeAppStore::new(app_storage));
            write_field!(sqbc_reads, 0);
            write_field!(sqbc_bytes, 0);
            write_field!(total_ram_bytes, 0);
            write_field!(heap_used_bytes, 0);
            write_field!(heap_free_bytes, 0);
        }
    }

    fn reset_runtime_state(&mut self) {
        self.file_backend.reset_runtime_state();
        self.temp_expected_len = 0;
        self.temp_received = 0;
        self.temp_app_id.clear();
        self.app_id.clear();
        self.pending_launch_app_id.clear();
        self.active_demand = CapabilityDemand::none();
        self.wifi_operation = NativeWifiOperationState::idle();
        self.wifi_station_profile.clear();
        self.stop_upload_profile();
        self.power_backend.abort_sleep();
        self.state_cache_len = None;
        self.foreground.reset();
        self.clear_timers();
        self.release_all_radios();
        self.clear_diagnostics();
        self.set_inactive_lifecycle();
    }

    fn clear_diagnostics(&mut self) {
        self.output.clear();
        self.trace.clear();
        self.drawlog.clear();
        self.sqbc_reads = 0;
        self.sqbc_bytes = 0;
    }

    fn begin_temp_run(&mut self, app_id: &str, total_len: usize) -> Result<(), NativeRuntimeError> {
        self.temp_expected_len = total_len;
        self.temp_received = 0;
        self.temp_app_id.set(app_id)?;
        Ok(())
    }

    fn write_temp_run_chunk(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        let end = offset
            .checked_add(bytes.len())
            .ok_or(NativeRuntimeError::InvalidOffset)?;
        if end > self.temp_expected_len || end > self.temp_sqbc.len() {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.temp_sqbc[offset..end].copy_from_slice(bytes);
        self.temp_received = self.temp_received.max(end);
        Ok(())
    }

    fn temp_bytes(&self) -> &[u8] {
        &self.temp_sqbc[..self.temp_expected_len]
    }

    fn request_app_launch(&mut self, app_id: &str) -> Result<(), NativeRuntimeError> {
        self.pending_launch_app_id.set(app_id)
    }

    fn arm_app(&mut self, app_id: &str) -> Result<(), VmError> {
        let entry = self.app_store.find(app_id).ok_or(VmError::InvalidOperand)?;
        let mut scratch = [0u8; 1024];
        let mut timer_events = [FixedText::<MAX_EVENT_NAME_BYTES>::new(); MAX_ARMED_TIMERS];
        let mut timer_intervals = [0u32; MAX_ARMED_TIMERS];
        let mut timer_repeating = [false; MAX_ARMED_TIMERS];
        let timer_count = {
            let mut reader = ActiveAppReader::new(&mut self.app_store, app_id, entry.sqbc_bytes);
            ProgramIndex::trigger_timer_count_from_reader(&mut reader, &mut scratch)?
        };
        if timer_count > MAX_ARMED_TIMERS {
            return Err(VmError::TooLarge);
        }
        for index in 0..timer_count {
            let trigger = {
                let mut reader =
                    ActiveAppReader::new(&mut self.app_store, app_id, entry.sqbc_bytes);
                ProgramIndex::trigger_timer_from_reader(&mut reader, &mut scratch, index)?
            };
            timer_events[index]
                .set(trigger.event)
                .map_err(|_| VmError::InvalidOperand)?;
            timer_intervals[index] = trigger
                .interval_ms
                .try_into()
                .map_err(|_| VmError::InvalidOperand)?;
            timer_repeating[index] = trigger.repeating;
        }

        let mut input_events = [FixedText::<MAX_EVENT_NAME_BYTES>::new(); MAX_ARMED_INPUTS];
        let input_count = {
            let mut reader = ActiveAppReader::new(&mut self.app_store, app_id, entry.sqbc_bytes);
            ProgramIndex::trigger_input_count_from_reader(&mut reader, &mut scratch)?
        };
        if input_count > MAX_ARMED_INPUTS {
            return Err(VmError::TooLarge);
        }
        for index in 0..input_count {
            let trigger = {
                let mut reader =
                    ActiveAppReader::new(&mut self.app_store, app_id, entry.sqbc_bytes);
                ProgramIndex::trigger_input_from_reader(&mut reader, &mut scratch, index)?
            };
            input_events[index]
                .set(trigger.event)
                .map_err(|_| VmError::InvalidOperand)?;
        }

        let mut timers = [LifecycleTriggerTimer {
            event: "",
            interval_ms: 0,
            repeating: false,
        }; MAX_ARMED_TIMERS];
        for index in 0..timer_count {
            timers[index] = LifecycleTriggerTimer {
                event: timer_events[index].as_str(),
                interval_ms: timer_intervals[index],
                repeating: timer_repeating[index],
            };
        }
        let mut inputs = [""; MAX_ARMED_INPUTS];
        for index in 0..input_count {
            inputs[index] = input_events[index].as_str();
        }
        match self
            .foreground
            .arm(app_id, &timers[..timer_count], &inputs[..input_count])
        {
            Ok(()) => {
                self.refresh_lifecycle_lines();
                Ok(())
            }
            Err(LifecycleError::DuplicateInputOwner) => {
                self.errors.push("armed_input_owner_conflict");
                #[cfg(debug_assertions)]
                self.trace.push("diag.armed-input-owner-conflict");
                Err(VmError::InvalidOperand)
            }
            Err(_) => Err(VmError::TooLarge),
        }
    }

    fn take_pending_launch(
        &mut self,
    ) -> Result<Option<FixedText<MAX_APP_ID_BYTES>>, NativeRuntimeError> {
        if self.pending_launch_app_id.as_str().is_empty() {
            return Ok(None);
        }
        let mut app_id = FixedText::<MAX_APP_ID_BYTES>::new();
        app_id.set(self.pending_launch_app_id.as_str())?;
        self.pending_launch_app_id.clear();
        Ok(Some(app_id))
    }

    fn state_bytes(&self) -> &[u8] {
        self.state_cache_len
            .map(|len| &self.state_cache[..len])
            .unwrap_or(&[])
    }

    fn set_wifi_profile(
        &mut self,
        profile: &str,
        ssid: &str,
        password: &str,
    ) -> Result<(), NativeRuntimeError> {
        self.wifi_profile_name.set(profile)?;
        self.wifi_profile_ssid.set(ssid)?;
        self.wifi_profile_password.set(password)?;
        Ok(())
    }

    fn wifi_profile_matches(&self, profile: &str) -> bool {
        !profile.is_empty()
            && !self.wifi_profile_name.as_str().is_empty()
            && self.wifi_profile_name.as_str() == profile
    }

    fn import_state(&mut self, bytes: &[u8]) -> Result<(), NativeRuntimeError> {
        if bytes.len() > self.state_cache.len() {
            return Err(NativeRuntimeError::TooLarge);
        }
        self.state_cache[..bytes.len()].copy_from_slice(bytes);
        self.state_cache_len = Some(bytes.len());
        let mut target = FixedText::<MAX_APP_ID_BYTES>::new();
        if self.app_store.find(self.app_id.as_str()).is_some() {
            target.set(self.app_id.as_str())?;
        } else if let Some(app_id) = self.app_store.app_id_at(0) {
            target.set(app_id)?;
        }
        if !target.as_str().is_empty() {
            self.app_store
                .save_state(target.as_str(), bytes)
                .map_err(native_app_store_error)?;
        }
        Ok(())
    }

    fn set_inactive_lifecycle(&mut self) {
        self.foreground.reset();
    }

    fn refresh_lifecycle_lines(&mut self) {}

    fn ensure_radio_active(&mut self, radio: RadioKind) -> Result<(), VmError> {
        match self.radio_leases.acquire(radio) {
            Ok(()) => {
                if self.radio_backend.acquire(radio).is_err() {
                    let _ = self.radio_leases.release(radio);
                    return Err(VmError::InvalidOperand);
                }
                Ok(())
            }
            Err(ServiceLeaseError::AlreadyActive) => Ok(()),
            Err(ServiceLeaseError::NotActive) => Err(VmError::InvalidOperand),
        }
    }

    fn ensure_radio_inactive(&mut self, radio: RadioKind) -> Result<(), VmError> {
        match self.radio_leases.release(radio) {
            Ok(()) => {
                self.radio_backend.release(radio);
                Ok(())
            }
            Err(ServiceLeaseError::NotActive) => Ok(()),
            Err(ServiceLeaseError::AlreadyActive) => Err(VmError::InvalidOperand),
        }
    }

    fn release_all_radios(&mut self) {
        if self.radio_leases.state(RadioKind::Wifi) == RadioLeaseState::Active {
            self.radio_backend.release(RadioKind::Wifi);
        }
        if self.radio_leases.state(RadioKind::Ble) == RadioLeaseState::Active {
            self.stop_upload_profile();
        }
        self.radio_leases.release_all();
    }

    fn stop_upload_profile(&mut self) {
        if !self.upload_profile_id.as_str().is_empty() {
            self.discard_upload_stage();
        }
        if self.upload_profile_ble {
            self.radio_backend.stop_ble_profile();
            let _ = self.ensure_radio_inactive(RadioKind::Ble);
        }
        if !self.upload_profile_id.as_str().is_empty() {
            self.upload_profile_stop_events = self.upload_profile_stop_events.saturating_add(1);
        }
        self.clear_upload_profile();
    }

    fn clear_upload_profile(&mut self) {
        self.upload_profile_id.clear();
        self.upload_profile_http = false;
        self.upload_profile_ble = false;
        self.upload_last_error.clear();
    }

    fn clear_upload(&mut self) {
        self.upload_path.clear();
        self.upload_name.clear();
        self.upload_id.clear();
        self.upload_transport = None;
        self.upload_total_bytes = 0;
        self.upload_received_bytes = 0;
        self.upload_total_bytes_text.clear();
        self.upload_received_bytes_text.clear();
    }

    fn upload_transport_enabled(&self, transport: NativeUploadTransport) -> bool {
        match transport {
            NativeUploadTransport::Http => self.upload_profile_http,
            NativeUploadTransport::Ble => self.upload_profile_ble,
        }
    }

    fn upload_transports(&self) -> &'static [&'static str] {
        match (self.upload_profile_http, self.upload_profile_ble) {
            (true, true) => HTTP_BLE_UPLOAD_TRANSPORTS,
            (true, false) => HTTP_UPLOAD_TRANSPORTS,
            (false, true) => BLE_UPLOAD_TRANSPORTS,
            (false, false) => NO_UPLOAD_TRANSPORTS,
        }
    }

    fn timer_event_index(&self, event: &str) -> Option<usize> {
        self.timers
            .iter()
            .enumerate()
            .find(|(_, timer)| timer.active && timer.event_as_str() == event)
            .map(|(index, _)| index)
    }

    fn begin_timer(
        &mut self,
        event: &str,
        interval_ms: u32,
        repeating: bool,
    ) -> Result<(), VmError> {
        if let Some(index) = self.timer_event_index(event) {
            let timer = &mut self.timers[index];
            timer.interval_ms = interval_ms;
            timer.remaining_ms = interval_ms;
            timer.repeating = repeating;
            timer.active = true;
            return Ok(());
        }
        let Some(index) = self.timers.iter().position(|timer| !timer.active) else {
            return Err(VmError::InvalidOperand);
        };
        let timer = &mut self.timers[index];
        timer.interval_ms = interval_ms;
        timer.remaining_ms = interval_ms;
        timer.repeating = repeating;
        timer.event_len = 0;
        timer.set_event(event)?;
        timer.active = true;
        Ok(())
    }

    fn tick_timers(
        &mut self,
        elapsed_ms: u32,
        out: &mut [u8; MAX_TIMER_EVENT_BYTES],
    ) -> Option<usize> {
        let mut due_event = None;

        for timer in &mut self.timers {
            if !timer.active {
                continue;
            }
            if timer.remaining_ms <= elapsed_ms {
                out[..timer.event_len].copy_from_slice(&timer.event[..timer.event_len]);
                due_event.replace(timer.event_len);
                if timer.repeating {
                    timer.remaining_ms = timer.interval_ms;
                } else {
                    timer.active = false;
                }
                break;
            }
            timer.remaining_ms -= elapsed_ms;
        }

        if let Some(event_len) = due_event {
            return Some(event_len);
        }
        None
    }

    fn discard_upload_stage(&mut self) {
        if !self.upload_path.as_str().is_empty() {
            let mut upload_path = FixedText::<MAX_UPLOAD_REF_BYTES>::new();
            if upload_path.set(self.upload_path.as_str()).is_ok() {
                let _ = self.file_backend.upload_stage_delete(upload_path.as_str());
            }
        }
        self.clear_upload();
    }

    fn clear_timers(&mut self) {
        for timer in &mut self.timers {
            timer.active = false;
            timer.repeating = false;
            timer.interval_ms = 0;
            timer.remaining_ms = 0;
            timer.event_len = 0;
        }
    }

    fn stage_ephemeral_upload(
        &mut self,
        name: &str,
        bytes: &[u8],
        id: &str,
        transport: NativeUploadTransport,
    ) -> Result<&str, NativeRuntimeError> {
        let safe_name = safe_upload_name(name).ok_or(NativeRuntimeError::InvalidOffset)?;
        let mut staged_path = FixedText::<MAX_UPLOAD_REF_BYTES>::new();
        let path = match self.file_backend.upload_stage_begin(safe_name, bytes.len()) {
            Ok(path) => path,
            Err(error) => {
                self.errors.push(error);
                return Err(NativeRuntimeError::InvalidOffset);
            }
        };
        staged_path.set(path)?;
        for (index, chunk) in bytes.chunks(UPLOAD_STAGE_CHUNK_BYTES).enumerate() {
            if let Err(error) = self.file_backend.upload_stage_chunk(
                staged_path.as_str(),
                index.saturating_mul(UPLOAD_STAGE_CHUNK_BYTES),
                chunk,
            ) {
                self.errors.push(error);
                return Err(NativeRuntimeError::InvalidOffset);
            }
        }
        if let Err(error) = self.file_backend.upload_stage_commit(staged_path.as_str()) {
            self.errors.push(error);
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.upload_path = staged_path;
        self.upload_name.set(safe_name)?;
        self.upload_id.set(id)?;
        self.upload_transport = Some(transport);
        self.upload_total_bytes = bytes.len();
        self.upload_received_bytes = bytes.len();
        self.upload_total_bytes_text.clear();
        self.upload_received_bytes_text.clear();
        write!(&mut self.upload_total_bytes_text, "{}", bytes.len())
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        write!(&mut self.upload_received_bytes_text, "{}", bytes.len())
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        Ok(self.upload_path.as_str())
    }

    fn begin_ephemeral_upload(
        &mut self,
        name: &str,
        total_len: usize,
        id: &str,
        transport: NativeUploadTransport,
    ) -> Result<&str, NativeRuntimeError> {
        if total_len == 0 {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        let safe_name = safe_upload_name(name).ok_or(NativeRuntimeError::InvalidOffset)?;
        if !self.upload_path.as_str().is_empty() {
            if self.upload_name.as_str() == safe_name
                && self.upload_id.as_str() == id
                && self.upload_transport == Some(transport)
                && self.upload_total_bytes == total_len
            {
                return Ok(self.upload_path.as_str());
            }
            return Err(NativeRuntimeError::UploadSessionActive);
        }
        let mut staged_path = FixedText::<MAX_UPLOAD_REF_BYTES>::new();
        let path = match self.file_backend.upload_stage_begin(safe_name, total_len) {
            Ok(path) => path,
            Err(error) => {
                self.errors.push(error);
                return Err(NativeRuntimeError::InvalidOffset);
            }
        };
        staged_path.set(path)?;
        self.upload_path = staged_path;
        self.upload_name.set(safe_name)?;
        self.upload_id.set(id)?;
        self.upload_transport = Some(transport);
        self.upload_total_bytes = total_len;
        self.upload_received_bytes = 0;
        self.upload_total_bytes_text.clear();
        self.upload_received_bytes_text.clear();
        write!(&mut self.upload_total_bytes_text, "{}", total_len)
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        Ok(self.upload_path.as_str())
    }

    fn write_ephemeral_upload_chunk(
        &mut self,
        upload_path: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), NativeRuntimeError> {
        if upload_path != self.upload_path.as_str()
            || bytes.is_empty()
            || offset != self.upload_received_bytes
            || offset.saturating_add(bytes.len()) > self.upload_total_bytes
        {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        if let Err(error) = self
            .file_backend
            .upload_stage_chunk(upload_path, offset, bytes)
        {
            self.errors.push(error);
            return Err(NativeRuntimeError::InvalidOffset);
        }
        let received = offset.saturating_add(bytes.len());
        self.upload_received_bytes = received;
        self.upload_received_bytes_text.clear();
        write!(&mut self.upload_received_bytes_text, "{}", received)
            .map_err(|_| NativeRuntimeError::TooLarge)?;
        Ok(())
    }

    fn commit_ephemeral_upload(
        &mut self,
        upload_path: &str,
        bytes_received: usize,
    ) -> Result<(), NativeRuntimeError> {
        if upload_path != self.upload_path.as_str()
            || bytes_received == 0
            || bytes_received != self.upload_received_bytes
            || bytes_received != self.upload_total_bytes
        {
            return Err(NativeRuntimeError::InvalidOffset);
        }
        if let Err(error) = self.file_backend.upload_stage_commit(upload_path) {
            self.errors.push(error);
            return Err(NativeRuntimeError::InvalidOffset);
        }
        self.upload_received_bytes_text.clear();
        write!(&mut self.upload_received_bytes_text, "{}", bytes_received)
            .map_err(|_| NativeRuntimeError::TooLarge)
    }

    fn record_wifi_operation(&mut self, operation: NativeWifiOperationState) {
        self.wifi_operation = operation;
    }
}

impl<
        B: NativeRadioBackend,
        D: NativeDisplaySink,
        C: NativeBinBookBackend,
        F: NativeFileBackend,
        A: NativeAppStorage,
    > SqbcReader for RuntimeHost<B, D, C, F, A>
{
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let end = offset.checked_add(out.len()).ok_or(VmError::ReadFailed)?;
        match self.active_sqbc {
            ActiveSqbc::Temp => {
                let bytes = self.temp_sqbc.get(offset..end).ok_or(VmError::ReadFailed)?;
                out.copy_from_slice(bytes);
            }
            ActiveSqbc::Installed => self
                .app_store
                .read_app_at(self.app_id.as_str(), offset, out)
                .map_err(|_| VmError::ReadFailed)?,
            ActiveSqbc::Fallback => {
                let bytes = self
                    .fallback_sqbc
                    .get(offset..end)
                    .ok_or(VmError::ReadFailed)?;
                out.copy_from_slice(bytes);
            }
        }
        self.sqbc_reads += 1;
        self.sqbc_bytes += out.len();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveSqbc {
    Temp,
    Installed,
    Fallback,
}

fn write_human_bytes(out: &mut dyn fmt::Write, label: &str, bytes: usize) -> Result<(), VmError> {
    if bytes >= 1024 * 1024 {
        write!(out, "{label} {} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        write!(out, "{label} {} KiB", bytes / 1024)
    } else {
        write!(out, "{label} {bytes} B")
    }
    .map_err(|_| VmError::InvalidOperand)
}

struct ActiveAppReader<'a, S> {
    store: &'a mut NativeAppStore<S>,
    app_id: &'a str,
    size: usize,
}

impl<'a, S: NativeAppStorage> ActiveAppReader<'a, S> {
    fn new(store: &'a mut NativeAppStore<S>, app_id: &'a str, size: usize) -> Self {
        Self {
            store,
            app_id,
            size,
        }
    }
}

impl<S: NativeAppStorage> SqbcReader for ActiveAppReader<'_, S> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        if offset
            .checked_add(out.len())
            .is_none_or(|end| end > self.size)
        {
            return Err(VmError::ReadFailed);
        }
        self.store
            .read_app_at(self.app_id, offset, out)
            .map_err(|_| VmError::ReadFailed)
    }
}

struct FileRefReader<'a, F> {
    backend: &'a mut F,
    file_ref: &'a str,
    size: usize,
}

impl<F: NativeFileBackend> SqbcReader for FileRefReader<'_, F> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        if offset
            .checked_add(out.len())
            .is_none_or(|end| end > self.size)
        {
            return Err(VmError::ReadFailed);
        }
        self.backend
            .file_ref_read_at(self.file_ref, offset as u64, out)
            .map_err(|_| VmError::ReadFailed)
    }
}

impl<
        B: NativeRadioBackend,
        D: NativeDisplaySink,
        C: NativeBinBookBackend,
        F: NativeFileBackend,
        A: NativeAppStorage,
    > TraceSink for RuntimeHost<B, D, C, F, A>
{
    fn trace(&mut self, message: &str) {
        self.trace.push(message);
    }

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
        self.output.push_fmt(|line| {
            for (index, value) in values.iter().copied().enumerate() {
                if index > 0 {
                    line.write_str(" ")?;
                }
                write_value(line, strings, value)?;
            }
            Ok(())
        });
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write!(
            out,
            "RAM {} KiB heap {} used {} free",
            self.total_ram_bytes / 1024,
            self.heap_used_bytes,
            self.heap_free_bytes
        )
        .map_err(|_| VmError::InvalidOperand)
    }

    fn system_storage_text(&mut self, name: &str, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        if name != "apps" {
            return Err(VmError::InvalidOperand);
        }
        let (_, available) = self
            .app_store
            .storage_mut()
            .capacity()
            .map_err(|_| VmError::ReadFailed)?;
        write_human_bytes(out, "Apps", available)
    }

    fn system_start_reason_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        out.write_str(self.foreground.start_reason().as_str())
            .map_err(|_| VmError::InvalidOperand)
    }

    fn service_power_sleep(&mut self, wake_after_ms: i32) -> Result<(), VmError> {
        if self.active_sqbc != ActiveSqbc::Installed || wake_after_ms < 0 {
            return Err(VmError::InvalidOperand);
        }
        self.power_backend
            .request_sleep(NativePowerRequest {
                wake_after_ms: wake_after_ms as u32,
            })
            .map_err(|_| VmError::InvalidOperand)
    }

    fn draw_clear(&mut self, color: u8) {
        self.drawlog.push_fmt(|line| {
            write!(line, "clear {color}")?;
            Ok(())
        });
        self.display_sink.draw_clear(color);
    }

    fn draw_text(
        &mut self,
        strings: &StringResolver<'_>,
        text: Value,
        options: DisplayTextOptions<'_>,
    ) {
        let text = strings.value_str(text).unwrap_or("<text>");
        self.drawlog.push_fmt(|line| {
            write!(
                line,
                "text {} x={} y={} w={} h={} font={}",
                text, options.x, options.y, options.w, options.h, options.font_height
            )?;
            if let Some(color) = options.text_color {
                write!(line, " fg={color}")?;
            }
            if let Some(color) = options.background_color {
                write!(line, " bg={color}")?;
            }
            Ok(())
        });
        self.display_sink.draw_text(text, options);
    }

    fn draw_rect(&mut self, options: DisplayRectOptions) {
        self.drawlog.push_fmt(|line| {
            write!(
                line,
                "rect x={} y={} w={} h={}",
                options.x, options.y, options.w, options.h
            )?;
            if let Some(color) = options.fill_color {
                write!(line, " fill={color}")?;
            }
            if let Some(color) = options.stroke_color {
                write!(line, " stroke={color}")?;
            }
            Ok(())
        });
        self.display_sink.draw_rect(options);
    }

    fn draw_line(&mut self, options: DisplayLineOptions) {
        self.drawlog.push_fmt(|line| {
            write!(
                line,
                "line x1={} y1={} x2={} y2={}",
                options.x1, options.y1, options.x2, options.y2
            )?;
            if let Some(color) = options.color {
                write!(line, " color={color}")?;
            }
            Ok(())
        });
        self.display_sink.draw_line(options);
    }

    fn draw_select(&mut self, name: &str) -> Result<(), VmError> {
        self.drawlog.push_fmt(|line| {
            write!(line, "select {name}")?;
            Ok(())
        });
        self.display_sink.draw_select(name);
        Ok(())
    }

    fn draw_refresh_mode(&mut self, mode: &str) {
        self.drawlog.push_fmt(|line| {
            write!(line, "refreshMode {mode}")?;
            Ok(())
        });
        self.display_sink.draw_refresh_mode(mode);
    }

    fn draw_image(&mut self, path: &str, options: DisplayResourceOptions) {
        self.drawlog.push_fmt(|line| {
            write!(
                line,
                "image {path} x={} y={} w={} h={}",
                options.x, options.y, options.w, options.h
            )?;
            Ok(())
        });
        self.display_sink.draw_image(path, options);
    }

    fn draw_resource(
        &mut self,
        strings: &StringResolver<'_>,
        drawable: Value,
        options: DisplayResourceOptions,
    ) {
        if let Value::Handle(handle) = drawable {
            self.drawlog.push_fmt(|line| {
                write!(
                    line,
                    "draw {:?}:{} x={} y={} w={} h={}",
                    handle.kind, handle.id, options.x, options.y, options.w, options.h
                )?;
                Ok(())
            });
            self.display_sink.draw_drawable(handle, options);
            return;
        }
        let drawable = strings.value_str(drawable).unwrap_or("<drawable>");
        self.drawlog.push_fmt(|line| {
            write!(
                line,
                "draw {} x={} y={} w={} h={}",
                drawable, options.x, options.y, options.w, options.h
            )?;
            Ok(())
        });
        self.display_sink.draw_resource(drawable, options);
    }

    fn screen_rendered(&mut self, name: &str) {
        self.display_sink.screen_rendered(name);
    }

    fn display_info<'a>(&'a mut self) -> Result<DisplayInfo<'a>, VmError> {
        Ok(DisplayInfo {
            ok: true,
            error: None,
            warning: None,
            available: true,
            status: "ready",
            binding: "display.default",
            driver: "xteink-x4-display",
            transport: "native",
            width: 480,
            height: 800,
            physical_width: 800,
            physical_height: 480,
            rotation: 270,
            color_model: "gray",
            logical_gray_levels: 16,
            native_bpp: 2,
            native_pixel_format: "GRAY2_PACKED",
            default_font_height: 20,
            supports_partial_refresh: true,
            supports_fast_refresh: true,
        })
    }

    fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
        let Some(len) = self.state_cache_len else {
            return Ok(None);
        };
        if len > out.len() {
            return Err(VmError::StateTooLarge);
        }
        out[..len].copy_from_slice(&self.state_cache[..len]);
        Ok(Some(len))
    }

    fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        if bytes.len() > self.state_cache.len() {
            return Err(VmError::StateTooLarge);
        }
        self.state_cache[..bytes.len()].copy_from_slice(bytes);
        self.state_cache_len = Some(bytes.len());
        if self.active_sqbc == ActiveSqbc::Installed {
            self.app_store
                .save_state(self.app_id.as_str(), bytes)
                .map_err(|_| VmError::ReadFailed)?;
        }
        Ok(())
    }

    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        self.state_cache_len = None;
        if self.active_sqbc == ActiveSqbc::Installed {
            self.app_store
                .storage_mut()
                .delete_state(self.app_id.as_str())
                .map_err(|_| VmError::ReadFailed)?;
        }
        Ok(())
    }

    fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
        let Some(interval_ms) = interval_ms.try_into().ok().filter(|ms| *ms > 0) else {
            return Err(VmError::InvalidOperand);
        };
        self.begin_timer(event, interval_ms, true)
    }

    fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
        let Some(delay_ms) = delay_ms.try_into().ok().filter(|ms| *ms > 0) else {
            return Err(VmError::InvalidOperand);
        };
        self.begin_timer(event, delay_ms, false)
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        self.request_app_launch(app)
            .map_err(|_| VmError::InvalidOperand)
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        self.arm_app(app)
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        self.foreground.disarm(app);
        self.refresh_lifecycle_lines();
        Ok(())
    }

    fn app_registry_list<'a>(&'a mut self) -> Result<AppRegistryList<'a>, VmError> {
        let len = self.app_store.registry().len();
        for index in 0..len {
            let id = self
                .app_store
                .app_id_at(index)
                .ok_or(VmError::InvalidSection)?;
            let id = unsafe { core::mem::transmute::<&str, &'static str>(id) };
            self.registry_view[index] = AppRegistryEntry {
                id,
                name: id,
                build: "",
                description: "",
            };
        }
        Ok(AppRegistryList {
            apps: &self.registry_view[..len],
        })
    }

    fn app_registry_get<'a>(&'a mut self, app_id: &str) -> Result<AppRegistryEntry<'a>, VmError> {
        let id = self
            .app_store
            .registry()
            .iter()
            .flatten()
            .find(|entry| entry.app_id() == app_id)
            .map(|entry| entry.app_id())
            .ok_or(VmError::InvalidOperand)?;
        Ok(AppRegistryEntry {
            id,
            name: id,
            build: "",
            description: "",
        })
    }

    fn app_process_stack<'a>(&'a mut self) -> Result<AppProcessStack<'a>, VmError> {
        let len = self.foreground.return_stack_len();
        for index in 0..len {
            let app_id = self
                .foreground
                .return_stack_at(index)
                .ok_or(VmError::InvalidSection)?;
            self.process_view[index] =
                unsafe { core::mem::transmute::<&str, &'static str>(app_id) };
        }
        Ok(AppProcessStack {
            apps: &self.process_view[..len],
        })
    }

    fn app_armed_stack<'a>(&'a mut self) -> Result<AppArmedStack<'a>, VmError> {
        let len = self.foreground.armed_len();
        for index in 0..len {
            let route = self
                .foreground
                .armed_at(index)
                .ok_or(VmError::InvalidSection)?;
            self.armed_view[index] = AppArmedStackEntry {
                app_id: unsafe { core::mem::transmute::<&str, &'static str>(route.app_id) },
                event: unsafe { core::mem::transmute::<&str, &'static str>(route.event) },
            };
        }
        Ok(AppArmedStack {
            entries: &self.armed_view[..len],
        })
    }

    fn app_install<'a>(
        &'a mut self,
        file_ref: &str,
        app_id: Option<&str>,
    ) -> Result<AppInstallResult<'a>, VmError> {
        let total_len = usize::try_from(
            self.file_backend
                .file_ref_size(file_ref)
                .map_err(|_| VmError::ReadFailed)?,
        )
        .map_err(|_| VmError::TooLarge)?;
        if total_len == 0 || total_len > MAX_APP_BYTES {
            return Err(VmError::TooLarge);
        }

        let mut resolved_app_id = FixedText::<MAX_APP_ID_BYTES>::new();
        if let Some(app_id) = app_id {
            resolved_app_id
                .set(app_id)
                .map_err(|_| VmError::InvalidOperand)?;
        } else {
            let mut reader = FileRefReader {
                backend: &mut self.file_backend,
                file_ref,
                size: total_len,
            };
            let mut scratch = [0u8; 1024];
            let app_id = ProgramIndex::app_id_from_reader(&mut reader, &mut scratch)?;
            resolved_app_id
                .set(app_id)
                .map_err(|_| VmError::InvalidOperand)?;
        }

        self.app_store
            .begin_install(resolved_app_id.as_str(), total_len)
            .map_err(|_| VmError::InvalidOperand)?;
        let mut offset = 0usize;
        let mut chunk = [0u8; MAX_LINE_BYTES];
        while offset < total_len {
            let chunk_len = (total_len - offset).min(chunk.len());
            self.file_backend
                .file_ref_read_at(file_ref, offset as u64, &mut chunk[..chunk_len])
                .map_err(|_| VmError::ReadFailed)?;
            self.app_store
                .write_install_chunk(offset, &chunk[..chunk_len])
                .map_err(|_| VmError::ReadFailed)?;
            offset += chunk_len;
        }
        self.app_store
            .commit_install(&mut [0; 1024])
            .map_err(|_| VmError::ReadFailed)?;
        self.last_installed_app_id = resolved_app_id;
        Ok(AppInstallResult {
            id: self.last_installed_app_id.as_str(),
        })
    }

    fn service_upload_start<'a>(&'a mut self, id: &str) -> Result<UploadStartResult<'a>, VmError> {
        let mut scratch = [0u8; 1024];
        let count = ProgramIndex::upload_profile_count_from_reader(self, &mut scratch)?;
        let mut found = false;
        let mut http = false;
        let mut ble = false;
        for index in 0..count {
            let profile = ProgramIndex::upload_profile_from_reader(self, &mut scratch, index)?;
            if profile.id != id || profile.role != "server" {
                continue;
            }
            if found {
                return Err(VmError::InvalidSection);
            }
            found = true;
            for transport_index in 0..profile.transports.len() {
                match profile.transports.get(transport_index) {
                    Some("http") => http = true,
                    Some("ble") => ble = true,
                    _ => return Err(VmError::InvalidSection),
                }
            }
        }
        if !found || (!http && !ble) {
            return Err(VmError::InvalidOperand);
        }
        if (http
            && !self
                .radio_backend
                .supports_upload_transport(NativeUploadTransport::Http))
            || (ble
                && !self
                    .radio_backend
                    .supports_upload_transport(NativeUploadTransport::Ble))
        {
            self.upload_last_error
                .set("unsupported")
                .map_err(|_| VmError::InvalidOperand)?;
            return Ok(UploadStartResult {
                ok: false,
                error: Some(self.upload_last_error.as_str()),
                id: None,
                transports: NO_UPLOAD_TRANSPORTS,
                http_path: None,
            });
        }

        self.stop_upload_profile();
        if ble {
            self.ensure_radio_active(RadioKind::Ble)?;
            if self.radio_backend.start_ble_profile(id).is_err() {
                self.ensure_radio_inactive(RadioKind::Ble)?;
                self.upload_last_error
                    .set("unsupported")
                    .map_err(|_| VmError::InvalidOperand)?;
                return Ok(UploadStartResult {
                    ok: false,
                    error: Some(self.upload_last_error.as_str()),
                    id: None,
                    transports: NO_UPLOAD_TRANSPORTS,
                    http_path: None,
                });
            }
        }
        if self.upload_profile_id.set(id).is_err() {
            if ble {
                self.radio_backend.stop_ble_profile();
                self.ensure_radio_inactive(RadioKind::Ble)?;
            }
            return Err(VmError::InvalidOperand);
        }
        self.upload_profile_http = http;
        self.upload_profile_ble = ble;
        self.upload_profile_start_events = self.upload_profile_start_events.saturating_add(1);
        self.upload_last_error.clear();
        Ok(UploadStartResult {
            ok: true,
            error: None,
            id: Some(self.upload_profile_id.as_str()),
            transports: self.upload_transports(),
            http_path: http.then_some(UPLOAD_HTTP_PATH),
        })
    }

    fn service_upload_stop(&mut self) -> Result<(), VmError> {
        self.stop_upload_profile();
        Ok(())
    }

    fn service_upload_status<'a>(&'a mut self) -> Result<UploadStatus<'a>, VmError> {
        let active = !self.upload_profile_id.as_str().is_empty();
        let in_flight = !self.upload_path.as_str().is_empty();
        Ok(UploadStatus {
            active,
            id: active.then_some(self.upload_profile_id.as_str()),
            transports: self.upload_transports(),
            http_path: (active && self.upload_profile_http).then_some(UPLOAD_HTTP_PATH),
            in_flight,
            bytes_received: in_flight.then_some(self.upload_received_bytes_text.as_str()),
            total_bytes: in_flight.then_some(self.upload_total_bytes_text.as_str()),
            error: (!self.upload_last_error.as_str().is_empty())
                .then_some(self.upload_last_error.as_str()),
        })
    }

    fn service_wifi_start_ap<'a>(&'a mut self, _ssid: &str) -> Result<WifiOperation<'a>, VmError> {
        self.ensure_radio_active(RadioKind::Wifi)?;
        let operation = match self.radio_backend.begin_start_wifi_ap(_ssid) {
            NativeWifiBackendOperation::Pending => NativeWifiOperationState::running("startAP"),
            NativeWifiBackendOperation::Done { .. } => NativeWifiOperationState::done("startAP"),
            NativeWifiBackendOperation::Error { error } => {
                self.ensure_radio_inactive(RadioKind::Wifi)?;
                NativeWifiOperationState::error("startAP", error)
            }
        };
        self.record_wifi_operation(operation);
        Ok(operation.operation())
    }

    fn service_wifi_stop_ap<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.ensure_radio_inactive(RadioKind::Wifi)?;
        let operation = NativeWifiOperationState::done("stopAP");
        self.record_wifi_operation(operation);
        Ok(operation.operation())
    }

    fn service_wifi_connect<'a>(&'a mut self, profile: &str) -> Result<WifiOperation<'a>, VmError> {
        if !self.wifi_profile_matches(profile) {
            let operation = NativeWifiOperationState::error("connect", "profile missing");
            self.record_wifi_operation(operation);
            return Ok(operation.operation());
        }
        if !self.wifi_profile_password.as_str().is_empty()
            && self.wifi_profile_password.as_str().len() < 8
        {
            let operation = NativeWifiOperationState::error("connect", "invalid password");
            self.record_wifi_operation(operation);
            return Ok(operation.operation());
        }
        self.ensure_radio_active(RadioKind::Wifi)?;
        let operation = match self.radio_backend.begin_connect_wifi_station(
            self.wifi_profile_ssid.as_str(),
            self.wifi_profile_password.as_str(),
        ) {
            NativeWifiBackendOperation::Pending | NativeWifiBackendOperation::Done { .. } => {
                self.wifi_station_profile
                    .set(profile)
                    .map_err(|_| VmError::InvalidOperand)?;
                NativeWifiOperationState::running("connect")
            }
            NativeWifiBackendOperation::Error { error } => {
                self.ensure_radio_inactive(RadioKind::Wifi)?;
                NativeWifiOperationState::error("connect", error)
            }
        };
        self.record_wifi_operation(operation);
        Ok(operation.operation())
    }

    fn service_wifi_disconnect<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        self.ensure_radio_inactive(RadioKind::Wifi)?;
        self.wifi_station_profile.clear();
        let operation = NativeWifiOperationState::done("disconnect");
        self.record_wifi_operation(operation);
        Ok(operation.operation())
    }

    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        let active = self.radio_leases.state(RadioKind::Wifi) == RadioLeaseState::Active;
        let backend_status = self.radio_backend.wifi_status();
        let mode = backend_status
            .mode
            .or_else(|| self.radio_backend.wifi_mode());
        Ok(WifiStatus {
            active,
            mode,
            ip_address: backend_status.ip_address,
            ssid: backend_status.ssid,
            clients: backend_status.clients,
            error: None,
            state: backend_status.state,
            backend: "native-x4",
            driver_started: backend_status.driver_started,
            configured: backend_status.configured,
            driver_mode: mode,
            channel: backend_status.channel,
            ap_start_events: backend_status.ap_start_events,
            ap_stop_events: backend_status.ap_stop_events,
            probe_events: backend_status.probe_events,
            sta_connected_events: backend_status.sta_connected_events,
            sta_disconnected_events: backend_status.sta_disconnected_events,
            last_backend_code: backend_status.last_backend_code,
            profile: if self.wifi_station_profile.as_str().is_empty() {
                None
            } else {
                Some(self.wifi_station_profile.as_str())
            },
            connected: backend_status.connected,
            scan_matches: backend_status.scan_matches,
            rssi: backend_status.rssi,
            auth: backend_status.auth,
            bssid: backend_status.bssid,
            disconnect_reason: backend_status.disconnect_reason,
            disconnect_reason_code: backend_status.disconnect_reason_code,
        })
    }

    fn service_wifi_get_ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        let ip = self.radio_backend.wifi_ap_ip();
        Ok(WifiApIp {
            ip: ip.ip,
            gw: ip.gw,
            netmask: ip.netmask,
            error: ip.error,
        })
    }

    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        if self.radio_leases.state(RadioKind::Wifi) == RadioLeaseState::Active {
            let operation = NativeWifiOperationState::error("scan", "wifi busy");
            self.record_wifi_operation(operation);
            return Ok(operation.operation());
        }
        self.ensure_radio_active(RadioKind::Wifi)?;
        let operation = match self.radio_backend.begin_scan_wifi() {
            NativeWifiBackendOperation::Pending => NativeWifiOperationState::running("scan"),
            NativeWifiBackendOperation::Done { count } => {
                self.ensure_radio_inactive(RadioKind::Wifi)?;
                NativeWifiOperationState::done_with_count("scan", count)
            }
            NativeWifiBackendOperation::Error { error } => {
                self.ensure_radio_inactive(RadioKind::Wifi)?;
                NativeWifiOperationState::error("scan", error)
            }
        };
        self.record_wifi_operation(operation);
        Ok(operation.operation())
    }

    fn service_wifi_operation<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        Ok(self.wifi_operation.operation())
    }

    fn service_wifi_result<'a>(&'a mut self) -> Result<WifiOperationResult<'a>, VmError> {
        Ok(self.wifi_operation.result())
    }

    fn service_wifi_cancel<'a>(&'a mut self) -> Result<WifiOperation<'a>, VmError> {
        let mut operation = self.wifi_operation;
        operation.cancelled = true;
        operation.done = true;
        operation.active = false;
        operation.state = "cancelled";
        operation.ok = true;
        operation.error = None;
        self.record_wifi_operation(operation);
        Ok(operation.operation())
    }

    fn service_wifi_scan_network<'a>(
        &'a mut self,
        index: i32,
    ) -> Result<WifiScanNetwork<'a>, VmError> {
        match self.radio_backend.wifi_scan_network(index) {
            Ok(Some(network)) => Ok(WifiScanNetwork {
                ok: true,
                error: None,
                network: Some(network),
            }),
            Ok(None) => Ok(WifiScanNetwork {
                ok: false,
                error: Some("not-found"),
                network: None,
            }),
            Err(error) => Ok(WifiScanNetwork {
                ok: false,
                error: Some(error),
                network: None,
            }),
        }
    }

    fn service_wifi_teardown(&mut self) -> Result<(), VmError> {
        self.ensure_radio_inactive(RadioKind::Wifi)
    }

    fn service_teardown_all(&mut self) -> Result<(), VmError> {
        self.clear_timers();
        self.release_all_radios();
        Ok(())
    }

    fn file_pick_file<'a>(
        &'a mut self,
        extension: &str,
    ) -> Result<FilePickFileResult<'a>, VmError> {
        self.file_backend.file_pick_file(extension)
    }

    fn file_read_text<'a>(&'a mut self, path: &str) -> Result<FileReadTextResult<'a>, VmError> {
        if self.active_sqbc == ActiveSqbc::Installed && !path.contains(':') {
            match self
                .app_store
                .storage_mut()
                .resource_size(self.app_id.as_str(), path)
            {
                Ok(len) if len <= self.resource_text.len() => {
                    self.app_store
                        .storage_mut()
                        .read_resource_at(
                            self.app_id.as_str(),
                            path,
                            0,
                            &mut self.resource_text[..len],
                        )
                        .map_err(|_| VmError::ReadFailed)?;
                    let text = core::str::from_utf8(&self.resource_text[..len])
                        .map_err(|_| VmError::InvalidOperand)?;
                    return Ok(FileReadTextResult {
                        ok: true,
                        error: None,
                        text: Some(text),
                    });
                }
                Ok(_) => {
                    return Ok(FileReadTextResult {
                        ok: false,
                        error: Some("too-large"),
                        text: None,
                    });
                }
                Err(AppStoreError::NotFound) => {}
                Err(_) => return Err(VmError::ReadFailed),
            }
        }
        self.file_backend.file_read_text(path)
    }

    fn file_read_lines<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<FileReadLinesResult<'a>, VmError> {
        self.file_backend.file_read_lines(path, max_lines)
    }

    fn file_read_lines_into<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
        writer: &mut dyn FileReadLinesWriter,
    ) -> Result<FileReadLinesSummary<'a>, VmError> {
        self.file_backend
            .file_read_lines_into(path, max_lines, writer)
    }

    fn file_copy<'a>(
        &'a mut self,
        source: &str,
        library: &str,
        name: &str,
    ) -> Result<FileCopyResult<'a>, VmError> {
        self.file_backend.file_copy(source, library, name)
    }

    fn file_list_into<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
        writer: &mut dyn FileListWriter,
    ) -> Result<FileListSummary<'a>, VmError> {
        self.file_backend
            .file_list_into(library, offset, limit, writer)
    }

    fn content_binbook_list_into<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
        writer: &mut dyn ContentBinBookListWriter,
    ) -> Result<ContentBinBookListSummary<'a>, VmError> {
        let result = self
            .file_backend
            .content_binbook_list_into(library, offset, limit, writer)?;
        if result.ok || result.error != Some("unsupported") {
            return Ok(result);
        }
        let result = self
            .binbook_backend
            .content_binbook_list(library, offset, limit)?;
        for entry in result.items {
            writer.push_entry(*entry)?;
        }
        Ok(ContentBinBookListSummary {
            ok: result.ok,
            error: result.error,
            warning: result.warning,
            count: result.count,
            has_more: result.has_more,
        })
    }

    fn content_binbook_list<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
    ) -> Result<ContentBinBookListResult<'a>, VmError> {
        self.binbook_backend
            .content_binbook_list(library, offset, limit)
    }

    fn binbook_open<'a>(&'a mut self, path: &str) -> Result<BinBookOpenResult<'a>, VmError> {
        let result = self.file_backend.binbook_open(path)?;
        if result.ok || result.error != Some("unsupported") {
            return Ok(result);
        }
        self.binbook_backend.binbook_open(path)
    }

    fn binbook_info<'a>(&'a mut self, book: Handle) -> Result<BinBookInfoResult<'a>, VmError> {
        let result = self.file_backend.binbook_info(book)?;
        if result.ok || result.error != Some("unsupported") {
            return Ok(result);
        }
        self.binbook_backend.binbook_info(book)
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, VmError> {
        let result = self.file_backend.binbook_read_page(book, page_index)?;
        if result.ok || result.error != Some("unsupported") {
            return Ok(result);
        }
        self.binbook_backend.binbook_read_page(book, page_index)
    }

    fn binbook_chapters<'a>(
        &'a mut self,
        book: Handle,
        offset: i32,
        limit: i32,
    ) -> Result<BinBookChapterListResult<'a>, VmError> {
        self.binbook_backend.binbook_chapters(book, offset, limit)
    }

    fn binbook_chapters_into<'a>(
        &'a mut self,
        book: Handle,
        offset: i32,
        limit: i32,
        writer: &mut dyn BinBookChapterListWriter,
    ) -> Result<BinBookChapterListSummary<'a>, VmError> {
        let result = self
            .file_backend
            .binbook_chapters_into(book, offset, limit, writer)?;
        if result.ok || result.error != Some("unsupported") {
            return Ok(result);
        }
        let result = self.binbook_backend.binbook_chapters(book, offset, limit)?;
        for entry in result.items {
            writer.push_entry(*entry)?;
        }
        Ok(BinBookChapterListSummary {
            ok: result.ok,
            error: result.error,
            count: result.count,
            has_more: result.has_more,
        })
    }

    fn binbook_chapter<'a>(
        &'a mut self,
        book: Handle,
        index: i32,
    ) -> Result<BinBookChapterResult<'a>, VmError> {
        let result = self.file_backend.binbook_chapter(book, index)?;
        if result.ok || result.error != Some("unsupported") {
            return Ok(result);
        }
        self.binbook_backend.binbook_chapter(book, index)
    }
}

fn write_value(
    out: &mut impl fmt::Write,
    strings: &StringResolver<'_>,
    value: Value,
) -> fmt::Result {
    match value {
        Value::Null => out.write_str("null"),
        Value::Bool(true) => out.write_str("true"),
        Value::Bool(false) => out.write_str("false"),
        Value::I32(value) => write!(out, "{value}"),
        Value::String(_) => out.write_str(strings.value_str(value).unwrap_or("<string>")),
        Value::Record(_) => out.write_str("<record>"),
        Value::List(_) => out.write_str("<list>"),
        Value::Handle(_) => out.write_str("<handle>"),
    }
}

struct LineStore {
    lines: [FixedText<MAX_LINE_BYTES>; MAX_LINE_COUNT],
    len: usize,
}

impl LineStore {
    const fn new() -> Self {
        Self {
            lines: [FixedText::new(); MAX_LINE_COUNT],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
        for line in &mut self.lines {
            line.clear();
        }
    }

    fn push(&mut self, value: &str) {
        if self.len == self.lines.len() {
            self.lines.rotate_left(1);
            self.len -= 1;
        }
        let _ = self.lines[self.len].set(value);
        self.len += 1;
    }

    fn push_fmt(&mut self, write: impl FnOnce(&mut FixedText<MAX_LINE_BYTES>) -> fmt::Result) {
        if self.len == self.lines.len() {
            self.lines.rotate_left(1);
            self.len -= 1;
        }
        self.lines[self.len].clear();
        let _ = write(&mut self.lines[self.len]);
        self.len += 1;
    }

    fn view(&self) -> LineView<'_> {
        let mut lines = [""; MAX_LINE_COUNT];
        let mut index = 0;
        while index < self.len {
            lines[index] = self.lines[index].as_str();
            index += 1;
        }
        LineView {
            lines,
            len: self.len,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct FixedText<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> FixedText<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn set(&mut self, value: &str) -> Result<(), NativeRuntimeError> {
        if value.len() > self.bytes.len() {
            return Err(NativeRuntimeError::TooLarge);
        }
        self.bytes[..value.len()].copy_from_slice(value.as_bytes());
        self.len = value.len();
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> fmt::Debug for FixedText<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FixedText").field(&self.as_str()).finish()
    }
}

impl<const N: usize> PartialEq<&str> for FixedText<N> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<const N: usize> fmt::Write for FixedText<N> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}
