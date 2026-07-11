#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::{format, string::String, vec, vec::Vec};
use core::str;

pub const MAGIC: [u8; 4] = *b"SQDP";
pub const HEADER_LEN: usize = 20;
pub const MAX_APP_ID_LEN: usize = squidvm_limits::MAX_APP_ID_BYTES;
pub const MAX_PATH_LEN: usize = 128;
pub const CONTENT_LIBRARY_PREFIX: &str = "books/";
pub const MAX_CONTENT_NAME_BYTES: usize = MAX_PATH_LEN - CONTENT_LIBRARY_PREFIX.len() - 1;
pub const MAX_APP_BYTES: usize = 65_536;
pub const MAX_RESOURCE_BYTES: usize = 1_048_576;
pub const DEFAULT_SERIAL_MAX_FRAME_BYTES: usize = 8192;
pub const DEFAULT_TRANSFER_ACK_WINDOW_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Request = 1,
    Response = 2,
    Event = 3,
}

impl TryFrom<u8> for FrameKind {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, <Self as TryFrom<u8>>::Error> {
        match value {
            1 => Ok(Self::Request),
            2 => Ok(Self::Response),
            3 => Ok(Self::Event),
            _ => Err(DecodeError::UnknownFrameKind(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Hello = 1,
    AppInstallBegin = 16,
    AppInstallChunk = 17,
    AppInstallCommit = 18,
    ResourceInstallBegin = 19,
    ResourceInstallChunk = 20,
    ResourceInstallCommit = 21,
    TempRunBegin = 24,
    TempRunChunk = 25,
    TempRunCommit = 26,
    AppLaunch = 32,
    AppList = 33,
    Key = 48,
    EventDispatch = 49,
    OutputGet = 64,
    StateGet = 65,
    DrawlogGet = 66,
    TraceGet = 67,
    ErrorsGet = 68,
    ResourcesGet = 69,
    LifecycleGet = 70,
    StateImport = 72,
    WifiProfileSet = 76,
    Reset = 80,
    StorageFormat = 81,
    RuntimeCapGet = 82,
    RuntimeCapSet = 83,
    RuntimeCapClear = 84,
    DisplayWindowProbe = 85,
    ContentInstallBegin = 88,
    ContentInstallChunk = 89,
    ContentInstallCommit = 90,
    ContentCheck = 91,
    DebugLogGet = 92,
    ContentDelete = 93,
    FirmwareInfo = 96,
    FirmwareUpdateBegin = 97,
    FirmwareUpdateChunk = 98,
    FirmwareUpdateCommit = 99,
    FirmwareUpdateStatus = 100,
    FirmwareUpdateAbort = 101,
}

impl Opcode {
    #[cfg(feature = "alloc")]
    pub fn parse(name: &str) -> Result<Self, String> {
        match normalize_name(name).as_str() {
            "hello" => Ok(Self::Hello),
            "appinstallbegin" => Ok(Self::AppInstallBegin),
            "appinstallchunk" => Ok(Self::AppInstallChunk),
            "appinstallcommit" => Ok(Self::AppInstallCommit),
            "resourceinstallbegin" => Ok(Self::ResourceInstallBegin),
            "resourceinstallchunk" => Ok(Self::ResourceInstallChunk),
            "resourceinstallcommit" => Ok(Self::ResourceInstallCommit),
            "temprunbegin" => Ok(Self::TempRunBegin),
            "temprunchunk" => Ok(Self::TempRunChunk),
            "tempruncommit" => Ok(Self::TempRunCommit),
            "applaunch" => Ok(Self::AppLaunch),
            "applist" => Ok(Self::AppList),
            "key" => Ok(Self::Key),
            "eventdispatch" => Ok(Self::EventDispatch),
            "outputget" => Ok(Self::OutputGet),
            "stateget" => Ok(Self::StateGet),
            "drawlogget" => Ok(Self::DrawlogGet),
            "traceget" => Ok(Self::TraceGet),
            "errorsget" => Ok(Self::ErrorsGet),
            "resourcesget" => Ok(Self::ResourcesGet),
            "lifecycleget" => Ok(Self::LifecycleGet),
            "stateimport" => Ok(Self::StateImport),
            "wifiprofileset" => Ok(Self::WifiProfileSet),
            "reset" => Ok(Self::Reset),
            "storageformat" => Ok(Self::StorageFormat),
            "runtimecapget" => Ok(Self::RuntimeCapGet),
            "runtimecapset" => Ok(Self::RuntimeCapSet),
            "runtimecapclear" => Ok(Self::RuntimeCapClear),
            "displaywindowprobe" => Ok(Self::DisplayWindowProbe),
            "contentinstallbegin" => Ok(Self::ContentInstallBegin),
            "contentinstallchunk" => Ok(Self::ContentInstallChunk),
            "contentinstallcommit" => Ok(Self::ContentInstallCommit),
            "contentcheck" => Ok(Self::ContentCheck),
            "debuglogget" => Ok(Self::DebugLogGet),
            "contentdelete" => Ok(Self::ContentDelete),
            "firmwareinfo" => Ok(Self::FirmwareInfo),
            "firmwareupdatebegin" => Ok(Self::FirmwareUpdateBegin),
            "firmwareupdatechunk" => Ok(Self::FirmwareUpdateChunk),
            "firmwareupdatecommit" => Ok(Self::FirmwareUpdateCommit),
            "firmwareupdatestatus" => Ok(Self::FirmwareUpdateStatus),
            "firmwareupdateabort" => Ok(Self::FirmwareUpdateAbort),
            _ => Err(format!("unknown protocol opcode: {name}")),
        }
    }
}

impl TryFrom<u8> for Opcode {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, <Self as TryFrom<u8>>::Error> {
        match value {
            1 => Ok(Self::Hello),
            16 => Ok(Self::AppInstallBegin),
            17 => Ok(Self::AppInstallChunk),
            18 => Ok(Self::AppInstallCommit),
            19 => Ok(Self::ResourceInstallBegin),
            20 => Ok(Self::ResourceInstallChunk),
            21 => Ok(Self::ResourceInstallCommit),
            24 => Ok(Self::TempRunBegin),
            25 => Ok(Self::TempRunChunk),
            26 => Ok(Self::TempRunCommit),
            32 => Ok(Self::AppLaunch),
            33 => Ok(Self::AppList),
            48 => Ok(Self::Key),
            49 => Ok(Self::EventDispatch),
            64 => Ok(Self::OutputGet),
            65 => Ok(Self::StateGet),
            66 => Ok(Self::DrawlogGet),
            67 => Ok(Self::TraceGet),
            68 => Ok(Self::ErrorsGet),
            69 => Ok(Self::ResourcesGet),
            70 => Ok(Self::LifecycleGet),
            72 => Ok(Self::StateImport),
            76 => Ok(Self::WifiProfileSet),
            80 => Ok(Self::Reset),
            81 => Ok(Self::StorageFormat),
            82 => Ok(Self::RuntimeCapGet),
            83 => Ok(Self::RuntimeCapSet),
            84 => Ok(Self::RuntimeCapClear),
            85 => Ok(Self::DisplayWindowProbe),
            88 => Ok(Self::ContentInstallBegin),
            89 => Ok(Self::ContentInstallChunk),
            90 => Ok(Self::ContentInstallCommit),
            91 => Ok(Self::ContentCheck),
            92 => Ok(Self::DebugLogGet),
            93 => Ok(Self::ContentDelete),
            96 => Ok(Self::FirmwareInfo),
            97 => Ok(Self::FirmwareUpdateBegin),
            98 => Ok(Self::FirmwareUpdateChunk),
            99 => Ok(Self::FirmwareUpdateCommit),
            100 => Ok(Self::FirmwareUpdateStatus),
            101 => Ok(Self::FirmwareUpdateAbort),
            _ => Err(DecodeError::UnknownOpcode(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    Ok = 0,
    Error = 1,
    Pending = 2,
}

impl TryFrom<u8> for Status {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, <Status as TryFrom<u8>>::Error> {
        match value {
            0 => Ok(Self::Ok),
            1 => Ok(Self::Error),
            2 => Ok(Self::Pending),
            _ => Err(DecodeError::UnknownStatus(value)),
        }
    }
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub opcode: Opcode,
    pub status: Status,
    pub sequence: u32,
    pub fields: Vec<Field>,
}

#[cfg(feature = "alloc")]
impl Frame {
    pub fn request(opcode: Opcode, sequence: u32, fields: Vec<Field>) -> Self {
        Self {
            kind: FrameKind::Request,
            opcode,
            status: Status::Ok,
            sequence,
            fields,
        }
    }

    pub fn response(opcode: Opcode, status: Status, sequence: u32, fields: Vec<Field>) -> Self {
        Self {
            kind: FrameKind::Response,
            opcode,
            status,
            sequence,
            fields,
        }
    }
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub tag: u8,
    pub value: FieldValue,
}

#[cfg(feature = "alloc")]
impl Field {
    pub fn bytes(tag: u8, value: impl Into<Vec<u8>>) -> Self {
        Self {
            tag,
            value: FieldValue::Bytes(value.into()),
        }
    }

    pub fn string(tag: u8, value: impl Into<String>) -> Self {
        Self {
            tag,
            value: FieldValue::String(value.into()),
        }
    }

    pub fn bool(tag: u8, value: bool) -> Self {
        Self {
            tag,
            value: FieldValue::Bool(value),
        }
    }

    pub fn i64(tag: u8, value: i64) -> Self {
        Self {
            tag,
            value: FieldValue::I64(value),
        }
    }

    pub fn u32(tag: u8, value: u32) -> Self {
        Self {
            tag,
            value: FieldValue::U32(value),
        }
    }

    pub fn u64(tag: u8, value: u64) -> Self {
        Self {
            tag,
            value: FieldValue::U64(value),
        }
    }

    pub fn record(tag: u8, fields: Vec<Field>) -> Self {
        Self {
            tag,
            value: FieldValue::Record(fields),
        }
    }
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Bytes(Vec<u8>),
    String(String),
    Bool(bool),
    I64(i64),
    U32(u32),
    U64(u64),
    Record(Vec<Field>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    TruncatedHeader,
    BadMagic,
    UnknownFrameKind(u8),
    UnknownOpcode(u8),
    UnknownStatus(u8),
    LengthMismatch { expected: usize, actual: usize },
    PayloadCrc,
    TruncatedField,
    UnknownFieldType(u8),
    InvalidBoolLength(usize),
    InvalidIntegerLength(usize),
    InvalidUtf8,
    OutputTooSmall { needed: usize, capacity: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    Decode(DecodeError),
    InvalidRequest,
    MissingField,
    InvalidUtf8,
    AppIdTooLong,
    PathTooLong,
    TooLarge,
    Inactive,
    Offset,
    Bounds,
    Crc,
}

impl From<DecodeError> for SessionError {
    fn from(value: DecodeError) -> Self {
        Self::Decode(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRequest<'a> {
    pub opcode: Opcode,
    pub sequence: u32,
    payload: &'a [u8],
}

impl<'a> DeviceRequest<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if bytes.len() < HEADER_LEN {
            return Err(DecodeError::TruncatedHeader);
        }
        if bytes[..4] != MAGIC {
            return Err(DecodeError::BadMagic);
        }

        let kind = FrameKind::try_from(bytes[4])?;
        if kind != FrameKind::Request {
            return Err(DecodeError::BadMagic);
        }
        let opcode = Opcode::try_from(bytes[5])?;
        let _status = Status::try_from(bytes[6])?;
        let sequence = u32::from_le_bytes(bytes[8..12].try_into().expect("slice length checked"));
        let payload_len =
            u32::from_le_bytes(bytes[12..16].try_into().expect("slice length checked")) as usize;
        let payload_crc =
            u32::from_le_bytes(bytes[16..20].try_into().expect("slice length checked"));
        let expected = HEADER_LEN + payload_len;
        if bytes.len() != expected {
            return Err(DecodeError::LengthMismatch {
                expected,
                actual: bytes.len(),
            });
        }
        let payload = &bytes[HEADER_LEN..];
        if crc32fast::hash(payload) != payload_crc {
            return Err(DecodeError::PayloadCrc);
        }
        Ok(Self {
            opcode,
            sequence,
            payload,
        })
    }

    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

pub fn key_event_from_request_into(request: &[u8], out: &mut [u8]) -> Result<usize, DecodeError> {
    let request = DeviceRequest::decode(request)?;
    if request.opcode != Opcode::Key {
        return Err(DecodeError::UnknownOpcode(request.opcode as u8));
    }
    let key = payload_field_bytes(request.payload(), 1, 1)?
        .filter(|bytes| !bytes.is_empty())
        .ok_or(DecodeError::TruncatedField)?;
    if str::from_utf8(key).is_err() {
        return Err(DecodeError::InvalidUtf8);
    }
    let needed = 4usize
        .checked_add(key.len())
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    if out.len() < needed {
        return Err(DecodeError::OutputTooSmall {
            needed,
            capacity: out.len(),
        });
    }
    out[..4].copy_from_slice(b"key.");
    out[4..needed].copy_from_slice(key);
    Ok(needed)
}

pub fn request_string_field<'a>(
    request: &'a DeviceRequest<'a>,
    tag: u8,
) -> Result<Option<&'a str>, DecodeError> {
    let Some(bytes) = payload_field_bytes(request.payload(), tag, 1)? else {
        return Ok(None);
    };
    str::from_utf8(bytes)
        .map(Some)
        .map_err(|_| DecodeError::InvalidUtf8)
}

pub fn request_bytes_field<'a>(
    request: &'a DeviceRequest<'a>,
    tag: u8,
) -> Result<Option<&'a [u8]>, DecodeError> {
    payload_field_bytes(request.payload(), tag, 0)
}

pub fn request_u64_field(request: &DeviceRequest<'_>, tag: u8) -> Result<Option<u64>, DecodeError> {
    let Some(bytes) = payload_field_bytes(request.payload(), tag, 5)? else {
        return Ok(None);
    };
    if bytes.len() != 8 {
        return Err(DecodeError::InvalidIntegerLength(bytes.len()));
    }
    Ok(Some(u64::from_le_bytes(
        bytes.try_into().expect("length checked"),
    )))
}

fn payload_field_bytes<'a>(
    payload: &'a [u8],
    expected_tag: u8,
    expected_type: u8,
) -> Result<Option<&'a [u8]>, DecodeError> {
    let mut offset = 0usize;
    while offset < payload.len() {
        if payload.len() - offset < 4 {
            return Err(DecodeError::TruncatedField);
        }
        let tag = payload[offset];
        let field_type = payload[offset + 1];
        let len = u16::from_le_bytes([payload[offset + 2], payload[offset + 3]]) as usize;
        offset += 4;
        if payload.len() - offset < len {
            return Err(DecodeError::TruncatedField);
        }
        let value = &payload[offset..offset + len];
        if tag == expected_tag && field_type == expected_type {
            return Ok(Some(value));
        }
        offset += len;
    }
    Ok(None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAction<'a> {
    BeginInstall {
        app_id: &'a str,
        total_len: usize,
    },
    WriteInstallChunk {
        staging_path: &'a str,
        offset: usize,
        bytes: &'a [u8],
    },
    CommitInstall {
        app_id: &'a str,
        staging_path: &'a str,
    },
    BeginTempRun {
        app_id: &'a str,
        total_len: usize,
    },
    WriteTempRunChunk {
        staging_path: &'a str,
        offset: usize,
        bytes: &'a [u8],
    },
    CommitTempRun {
        app_id: &'a str,
        staging_path: &'a str,
        total_len: usize,
    },
    BeginResourceInstall {
        app_id: &'a str,
        resource_path: &'a str,
        total_len: usize,
    },
    WriteResourceChunk {
        staging_path: &'a str,
        offset: usize,
        bytes: &'a [u8],
    },
    CommitResourceInstall {
        app_id: &'a str,
        resource_path: &'a str,
        staging_path: &'a str,
    },
    BeginContentInstall {
        name: &'a str,
        total_len: usize,
    },
    WriteContentChunk {
        path: &'a str,
        offset: usize,
        bytes: &'a [u8],
    },
    CommitContentInstall {
        name: &'a str,
        path: &'a str,
    },
}

#[derive(Clone, Copy)]
struct FixedStr<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Default for FixedStr<N> {
    fn default() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
}

impl<const N: usize> FixedStr<N> {
    fn set(&mut self, value: &str) -> Result<(), SessionError> {
        if value.is_empty() || value.len() >= N {
            return Err(if N == MAX_APP_ID_LEN {
                SessionError::AppIdTooLong
            } else {
                SessionError::PathTooLong
            });
        }
        self.bytes = [0; N];
        self.bytes[..value.len()].copy_from_slice(value.as_bytes());
        self.len = value.len();
        Ok(())
    }

    fn as_str(&self) -> &str {
        str::from_utf8(&self.bytes[..self.len]).expect("FixedStr only stores utf8")
    }

    fn clear(&mut self) {
        self.bytes = [0; N];
        self.len = 0;
    }
}

#[derive(Clone, Copy)]
struct TransferSession<const ID_LEN: usize> {
    active: bool,
    app_id: FixedStr<ID_LEN>,
    total_len: usize,
    received: usize,
    expected_crc: u32,
    running_crc: u32,
    staging_path: FixedStr<MAX_PATH_LEN>,
}

impl<const ID_LEN: usize> Default for TransferSession<ID_LEN> {
    fn default() -> Self {
        Self {
            active: false,
            app_id: FixedStr::default(),
            total_len: 0,
            received: 0,
            expected_crc: 0,
            running_crc: 0xffff_ffff,
            staging_path: FixedStr::default(),
        }
    }
}

impl<const ID_LEN: usize> TransferSession<ID_LEN> {
    fn begin(
        &mut self,
        app_id: &str,
        total_len: usize,
        expected_crc: u32,
    ) -> Result<(), SessionError> {
        self.begin_with_limit(app_id, total_len, expected_crc, MAX_APP_BYTES)
    }

    fn begin_with_limit(
        &mut self,
        app_id: &str,
        total_len: usize,
        expected_crc: u32,
        max_bytes: usize,
    ) -> Result<(), SessionError> {
        validate_transfer_len(total_len, max_bytes)?;
        self.clear();
        self.app_id.set(app_id)?;
        self.total_len = total_len;
        self.expected_crc = expected_crc;
        self.running_crc = 0xffff_ffff;
        self.active = true;
        Ok(())
    }

    fn complete_begin(&mut self, staging_path: &str) -> Result<(), SessionError> {
        if !self.active {
            return Err(SessionError::Inactive);
        }
        self.staging_path.set(staging_path)
    }

    fn validate_chunk(&self, offset: usize, bytes: &[u8]) -> Result<(), SessionError> {
        if !self.active {
            return Err(SessionError::Inactive);
        }
        if offset != self.received {
            return Err(SessionError::Offset);
        }
        if bytes.len() > self.total_len.saturating_sub(self.received) {
            return Err(SessionError::Bounds);
        }
        Ok(())
    }

    fn complete_chunk(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        self.validate_chunk(self.received, bytes)?;
        self.running_crc = crc32_update(self.running_crc, bytes);
        self.received += bytes.len();
        Ok(())
    }

    fn validate_commit(&self) -> Result<(), SessionError> {
        if !self.active {
            return Err(SessionError::Inactive);
        }
        if self.received != self.total_len {
            return Err(SessionError::Bounds);
        }
        if !self.crc_matches() {
            return Err(SessionError::Crc);
        }
        Ok(())
    }

    fn crc_matches(&self) -> bool {
        !self.running_crc == self.expected_crc
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Default)]
struct ResourceSession {
    transfer: TransferSession<MAX_APP_ID_LEN>,
    resource_path: FixedStr<MAX_PATH_LEN>,
}

impl ResourceSession {
    fn begin(
        &mut self,
        app_id: &str,
        resource_path: &str,
        total_len: usize,
        expected_crc: u32,
    ) -> Result<(), SessionError> {
        self.clear();
        self.transfer
            .begin_with_limit(app_id, total_len, expected_crc, MAX_RESOURCE_BYTES)?;
        self.resource_path.set(resource_path)?;
        Ok(())
    }

    fn clear(&mut self) {
        self.transfer.clear();
        self.resource_path.clear();
    }
}

#[derive(Default)]
pub struct ProtocolSessions {
    install: TransferSession<MAX_APP_ID_LEN>,
    temp_run: TransferSession<MAX_APP_ID_LEN>,
    resource: ResourceSession,
    content: TransferSession<{ MAX_CONTENT_NAME_BYTES + 1 }>,
}

impl ProtocolSessions {
    pub fn next_action<'a>(
        &'a mut self,
        request: &'a DeviceRequest<'a>,
    ) -> Result<HostAction<'a>, SessionError> {
        match request.opcode {
            Opcode::AppInstallBegin => self.begin_install(request, false),
            Opcode::TempRunBegin => self.begin_install(request, true),
            Opcode::AppInstallChunk => self.install_chunk(request, false),
            Opcode::TempRunChunk => self.install_chunk(request, true),
            Opcode::AppInstallCommit => self.commit_install(),
            Opcode::TempRunCommit => self.commit_temp_run(),
            Opcode::ResourceInstallBegin => self.begin_resource(request),
            Opcode::ResourceInstallChunk => self.resource_chunk(request),
            Opcode::ResourceInstallCommit => self.commit_resource(),
            Opcode::ContentInstallBegin => self.begin_content(request),
            Opcode::ContentInstallChunk => self.content_chunk(request),
            Opcode::ContentInstallCommit => self.commit_content(),
            _ => Err(SessionError::InvalidRequest),
        }
    }

    pub fn complete_begin_install(&mut self, staging_path: &str) -> Result<(), SessionError> {
        self.install.complete_begin(staging_path)
    }

    pub fn complete_install_chunk(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        self.install.complete_chunk(bytes)
    }

    pub fn complete_install_commit(&mut self) {
        self.install.clear();
    }

    pub fn complete_begin_temp_run(&mut self, staging_path: &str) -> Result<(), SessionError> {
        self.temp_run.complete_begin(staging_path)
    }

    pub fn complete_temp_run_chunk(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        self.temp_run.complete_chunk(bytes)
    }

    pub fn complete_temp_run_commit(&mut self) {
        self.temp_run.clear();
    }

    pub fn complete_begin_resource_install(
        &mut self,
        staging_path: &str,
    ) -> Result<(), SessionError> {
        self.resource.transfer.complete_begin(staging_path)
    }

    pub fn complete_resource_chunk(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        self.resource.transfer.complete_chunk(bytes)
    }

    pub fn complete_resource_commit(&mut self) {
        self.resource.clear();
    }

    pub fn complete_begin_content_install(&mut self, path: &str) -> Result<(), SessionError> {
        self.content.complete_begin(path)
    }

    pub fn complete_content_chunk(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        self.content.complete_chunk(bytes)
    }

    pub fn complete_content_commit(&mut self) {
        self.content.clear();
    }

    fn begin_install<'a>(
        &'a mut self,
        request: &'a DeviceRequest<'a>,
        temp_run: bool,
    ) -> Result<HostAction<'a>, SessionError> {
        let app_id = string_field(request.payload, 1)?.ok_or(SessionError::MissingField)?;
        let total_len = u64_field(request.payload, 2)?.ok_or(SessionError::MissingField)?;
        let crc32 = u64_field(request.payload, 3)?.ok_or(SessionError::MissingField)?;
        if crc32 > u32::MAX as u64 {
            return Err(SessionError::InvalidRequest);
        }
        let total_len = usize::try_from(total_len).map_err(|_| SessionError::TooLarge)?;
        if temp_run {
            self.temp_run.begin(app_id, total_len, crc32 as u32)?;
            Ok(HostAction::BeginTempRun { app_id, total_len })
        } else {
            self.install.begin(app_id, total_len, crc32 as u32)?;
            Ok(HostAction::BeginInstall { app_id, total_len })
        }
    }

    fn install_chunk<'a>(
        &'a self,
        request: &'a DeviceRequest<'a>,
        temp_run: bool,
    ) -> Result<HostAction<'a>, SessionError> {
        let offset = chunk_offset(request.payload)?;
        let bytes = bytes_field(request.payload, 2)?.ok_or(SessionError::MissingField)?;
        let session = if temp_run {
            &self.temp_run
        } else {
            &self.install
        };
        session.validate_chunk(offset, bytes)?;
        if temp_run {
            Ok(HostAction::WriteTempRunChunk {
                staging_path: session.staging_path.as_str(),
                offset,
                bytes,
            })
        } else {
            Ok(HostAction::WriteInstallChunk {
                staging_path: session.staging_path.as_str(),
                offset,
                bytes,
            })
        }
    }

    fn commit_install(&self) -> Result<HostAction<'_>, SessionError> {
        self.install.validate_commit()?;
        Ok(HostAction::CommitInstall {
            app_id: self.install.app_id.as_str(),
            staging_path: self.install.staging_path.as_str(),
        })
    }

    fn commit_temp_run(&self) -> Result<HostAction<'_>, SessionError> {
        self.temp_run.validate_commit()?;
        Ok(HostAction::CommitTempRun {
            app_id: self.temp_run.app_id.as_str(),
            staging_path: self.temp_run.staging_path.as_str(),
            total_len: self.temp_run.total_len,
        })
    }

    fn begin_resource<'a>(
        &'a mut self,
        request: &'a DeviceRequest<'a>,
    ) -> Result<HostAction<'a>, SessionError> {
        let app_id = string_field(request.payload, 1)?.ok_or(SessionError::MissingField)?;
        let resource_path = string_field(request.payload, 2)?.ok_or(SessionError::MissingField)?;
        let total_len = u64_field(request.payload, 3)?.ok_or(SessionError::MissingField)?;
        let crc32 = u64_field(request.payload, 4)?.ok_or(SessionError::MissingField)?;
        if crc32 > u32::MAX as u64 {
            return Err(SessionError::InvalidRequest);
        }
        let total_len = usize::try_from(total_len).map_err(|_| SessionError::TooLarge)?;
        self.resource
            .begin(app_id, resource_path, total_len, crc32 as u32)?;
        Ok(HostAction::BeginResourceInstall {
            app_id,
            resource_path,
            total_len,
        })
    }

    fn resource_chunk<'a>(
        &'a self,
        request: &'a DeviceRequest<'a>,
    ) -> Result<HostAction<'a>, SessionError> {
        let offset = chunk_offset(request.payload)?;
        let bytes = bytes_field(request.payload, 2)?.ok_or(SessionError::MissingField)?;
        self.resource.transfer.validate_chunk(offset, bytes)?;
        Ok(HostAction::WriteResourceChunk {
            staging_path: self.resource.transfer.staging_path.as_str(),
            offset,
            bytes,
        })
    }

    fn commit_resource(&self) -> Result<HostAction<'_>, SessionError> {
        self.resource.transfer.validate_commit()?;
        Ok(HostAction::CommitResourceInstall {
            app_id: self.resource.transfer.app_id.as_str(),
            resource_path: self.resource.resource_path.as_str(),
            staging_path: self.resource.transfer.staging_path.as_str(),
        })
    }

    fn begin_content<'a>(
        &'a mut self,
        request: &'a DeviceRequest<'a>,
    ) -> Result<HostAction<'a>, SessionError> {
        let name = string_field(request.payload, 1)?.ok_or(SessionError::MissingField)?;
        let total_len = u64_field(request.payload, 2)?.ok_or(SessionError::MissingField)?;
        let crc32 = u64_field(request.payload, 3)?.ok_or(SessionError::MissingField)?;
        if crc32 > u32::MAX as u64 {
            return Err(SessionError::InvalidRequest);
        }
        let total_len = usize::try_from(total_len).map_err(|_| SessionError::TooLarge)?;
        self.content
            .begin_with_limit(name, total_len, crc32 as u32, MAX_RESOURCE_BYTES)?;
        Ok(HostAction::BeginContentInstall { name, total_len })
    }

    fn content_chunk<'a>(
        &'a self,
        request: &'a DeviceRequest<'a>,
    ) -> Result<HostAction<'a>, SessionError> {
        let offset = chunk_offset(request.payload)?;
        let bytes = bytes_field(request.payload, 2)?.ok_or(SessionError::MissingField)?;
        self.content.validate_chunk(offset, bytes)?;
        Ok(HostAction::WriteContentChunk {
            path: self.content.staging_path.as_str(),
            offset,
            bytes,
        })
    }

    fn commit_content(&self) -> Result<HostAction<'_>, SessionError> {
        self.content.validate_commit()?;
        Ok(HostAction::CommitContentInstall {
            name: self.content.app_id.as_str(),
            path: self.content.staging_path.as_str(),
        })
    }
}

fn validate_transfer_len(total_len: usize, max_bytes: usize) -> Result<(), SessionError> {
    if total_len == 0 {
        return Err(SessionError::InvalidRequest);
    }
    if total_len > max_bytes {
        return Err(SessionError::TooLarge);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct RawField<'a> {
    tag: u8,
    field_type: u8,
    value: &'a [u8],
}

struct RawFields<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for RawFields<'a> {
    type Item = Result<RawField<'a>, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.payload.len() {
            return None;
        }
        if self.payload.len().saturating_sub(self.offset) < 4 {
            return Some(Err(DecodeError::TruncatedField));
        }
        let tag = self.payload[self.offset];
        let field_type = self.payload[self.offset + 1];
        let len = u16::from_le_bytes([self.payload[self.offset + 2], self.payload[self.offset + 3]])
            as usize;
        let value_start = self.offset + 4;
        let value_end = value_start + len;
        if value_end > self.payload.len() {
            return Some(Err(DecodeError::TruncatedField));
        }
        self.offset = value_end;
        Some(Ok(RawField {
            tag,
            field_type,
            value: &self.payload[value_start..value_end],
        }))
    }
}

fn fields(payload: &[u8]) -> RawFields<'_> {
    RawFields { payload, offset: 0 }
}

fn string_field(payload: &[u8], tag: u8) -> Result<Option<&str>, SessionError> {
    for field in fields(payload) {
        let field = field?;
        if field.tag == tag && field.field_type == 1 {
            return str::from_utf8(field.value)
                .map(Some)
                .map_err(|_| SessionError::InvalidUtf8);
        }
    }
    Ok(None)
}

fn bytes_field(payload: &[u8], tag: u8) -> Result<Option<&[u8]>, SessionError> {
    for field in fields(payload) {
        let field = field?;
        if field.tag == tag && field.field_type == 0 {
            return Ok(Some(field.value));
        }
    }
    Ok(None)
}

fn u64_field(payload: &[u8], tag: u8) -> Result<Option<u64>, SessionError> {
    for field in fields(payload) {
        let field = field?;
        if field.tag == tag && field.field_type == 5 {
            if field.value.len() != 8 {
                return Err(SessionError::InvalidRequest);
            }
            return Ok(Some(u64::from_le_bytes(
                field.value.try_into().expect("length checked"),
            )));
        }
    }
    Ok(None)
}

fn chunk_offset(payload: &[u8]) -> Result<usize, SessionError> {
    let offset = u64_field(payload, 1)?.ok_or(SessionError::MissingField)?;
    usize::try_from(offset).map_err(|_| SessionError::Bounds)
}

fn crc32_update(crc: u32, bytes: &[u8]) -> u32 {
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

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloIdentity {
    pub target: String,
    pub firmware: String,
    pub diagnostic: bool,
    pub transfer_capabilities: TransferCapabilities,
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub app_id: String,
    pub sqbc_len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppListEntry<'a> {
    pub app_id: &'a str,
    pub sqbc_len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceMetric<'a> {
    pub key: &'a str,
    pub value: u64,
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentCheckResult {
    pub name: String,
    pub size: u64,
    pub crc32: u64,
}

pub const FIRMWARE_SHA256_BYTES: usize = 32;

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareInfo {
    pub active_slot: String,
    pub active_slot_size: u64,
    pub inactive_slot: String,
    pub inactive_slot_size: u64,
    pub build_id: String,
    pub boot_state: String,
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareUpdateStatus {
    pub state: String,
    pub candidate_slot: String,
    pub expected_len: u64,
    pub durable_offset: u64,
    pub build_id: String,
    pub expected_sha256: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareInfoRef<'a> {
    pub active_slot: &'a str,
    pub active_slot_size: u64,
    pub inactive_slot: &'a str,
    pub inactive_slot_size: u64,
    pub build_id: &'a str,
    pub boot_state: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareUpdateStatusRef<'a> {
    pub state: &'a str,
    pub candidate_slot: &'a str,
    pub expected_len: u64,
    pub durable_offset: u64,
    pub build_id: &'a str,
    pub expected_sha256: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferCapabilities {
    pub max_frame_bytes: usize,
    pub max_payload_bytes: usize,
    pub ack_window_bytes: usize,
}

impl TransferCapabilities {
    pub fn default_serial() -> Self {
        Self::from_serial_frame_budget(DEFAULT_SERIAL_MAX_FRAME_BYTES)
    }

    pub fn from_serial_frame_budget(max_frame_bytes: usize) -> Self {
        let max_payload_bytes = transfer_chunk_payload_for_frame_budget(max_frame_bytes);
        Self {
            max_frame_bytes,
            max_payload_bytes,
            ack_window_bytes: max_payload_bytes,
        }
    }
}

const RESOURCE_METRIC_NAMES: &[(u32, &str)] = &[
    (1, "ram_total_bytes"),
    (2, "runtime_static_bytes"),
    (3, "vm_sqbc_chunk_bytes"),
    (4, "heap_count"),
    (5, "heap_free_bytes"),
    (6, "heap_alloc_bytes"),
    (7, "heap_max_alloc_bytes"),
    (8, "heap_largest_free_supported"),
    (9, "heap_largest_free_bytes"),
    (10, "last_dispatch_us"),
    (11, "last_dispatch_seq"),
    (12, "last_sqbc_reads"),
    (13, "last_sqbc_bytes"),
    (14, "runtime_status"),
    (15, "runtime_dispatch_started"),
    (16, "runtime_dispatch_age_us"),
    (17, "runtime_work_submitted"),
    (18, "runtime_current_app_present"),
    (19, "runtime_lifecycle_phase"),
    (20, "runtime_arm_phase"),
    (21, "cap.static.timer"),
    (22, "cap.static.armed_timer"),
    (23, "cap.static.input_button"),
    (24, "cap.static.binding"),
    (25, "cap.static.output"),
    (26, "cap.static.drawlog"),
    (27, "cap.static.device_error"),
    (28, "cap.active.timer"),
    (29, "cap.active.armed_timer"),
    (30, "cap.active.input_button"),
    (31, "cap.active.binding"),
    (32, "cap.active.output"),
    (33, "cap.active.drawlog"),
    (34, "proto_stack_size_bytes"),
    (35, "proto_stack_pre_unused_bytes"),
    (36, "proto_stack_pre_used_bytes"),
    (37, "proto_stack_unused_bytes"),
    (38, "proto_stack_used_bytes"),
    (39, "vm_stack_size_bytes"),
    (40, "vm_stack_unused_bytes"),
    (41, "vm_stack_used_bytes"),
    (42, "app_count"),
    (43, "input_button_state"),
    (44, "x4.input.adc_gpio1_raw"),
    (45, "x4.input.adc_gpio1_logical"),
    (46, "x4.input.adc_gpio1_error"),
    (47, "x4.input.adc_gpio2_raw"),
    (48, "x4.input.adc_gpio2_logical"),
    (49, "x4.input.adc_gpio2_error"),
    (50, "x4.input.power_raw"),
    (51, "x4.input.power_pressed"),
    (52, "x4.input.power_error"),
    (53, "display_stack_size_bytes"),
    (54, "display_stack_unused_bytes"),
    (55, "display_stack_used_bytes"),
    (56, "last_sqbc_read_us"),
    (57, "last_display_flush_us"),
    (58, "last_state_save_us"),
    (59, "last_binbook_open_us"),
    (60, "last_binbook_read_page_us"),
    (61, "radio_active_leases"),
    (62, "radio_wifi_active"),
    (63, "radio_ble_active"),
    (64, "serial_buffer_bytes"),
    (65, "known_static_bytes"),
    (66, "heap_pool_bytes"),
    (67, "known_used_bytes"),
    (68, "nonheap_remainder_bytes"),
    (69, "display_pending_refreshes"),
    (70, "display_recorded_draws"),
    (71, "display_dropped_draws"),
    (72, "demand_wifi"),
    (73, "demand_ble"),
    (74, "demand_http"),
    (75, "demand_display"),
    (76, "demand_storage"),
    (77, "demand_binbook"),
    (78, "upload_profile_active"),
    (79, "upload_profile_id_len"),
    (80, "upload_profile_start_events"),
    (81, "upload_profile_stop_events"),
    (82, "upload_transport_http_active"),
    (83, "upload_transport_ble_active"),
];

fn resource_metric_id_for_name(name: &str) -> Option<u32> {
    RESOURCE_METRIC_NAMES
        .iter()
        .find_map(|(id, metric_name)| (*metric_name == name).then_some(*id))
}

#[cfg(feature = "alloc")]
fn resource_metric_name_for_id(id: u32) -> String {
    RESOURCE_METRIC_NAMES
        .iter()
        .find_map(|(metric_id, metric_name)| (*metric_id == id).then_some((*metric_name).into()))
        .unwrap_or_else(|| format!("resource.{id}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleTimer<'a> {
    pub app_id: &'a str,
    pub event: &'a str,
}

#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: i64,
    pub message: String,
}

#[cfg(feature = "alloc")]
pub fn hello_request(sequence: u32) -> Frame {
    Frame::request(Opcode::Hello, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn app_list_request(sequence: u32) -> Frame {
    Frame::request(Opcode::AppList, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn output_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::OutputGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn trace_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::TraceGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn state_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::StateGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn drawlog_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::DrawlogGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn debug_log_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::DebugLogGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn errors_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ErrorsGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn resources_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ResourcesGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn resources_get_request_with_heap_reset(sequence: u32) -> Frame {
    Frame::request(Opcode::ResourcesGet, sequence, vec![Field::bool(1, true)])
}

#[cfg(feature = "alloc")]
pub fn lifecycle_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::LifecycleGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn reset_request(sequence: u32) -> Frame {
    Frame::request(Opcode::Reset, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn storage_format_request(sequence: u32) -> Frame {
    Frame::request(Opcode::StorageFormat, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn runtime_cap_get_request(sequence: u32, key: Option<&str>) -> Frame {
    let fields = key
        .map(|key| vec![Field::string(1, key)])
        .unwrap_or_default();
    Frame::request(Opcode::RuntimeCapGet, sequence, fields)
}

#[cfg(feature = "alloc")]
pub fn runtime_cap_set_request(sequence: u32, key: impl Into<String>, value: u16) -> Frame {
    Frame::request(
        Opcode::RuntimeCapSet,
        sequence,
        vec![Field::string(1, key), Field::u32(2, u32::from(value))],
    )
}

#[cfg(feature = "alloc")]
pub fn runtime_cap_clear_request(sequence: u32, key: Option<&str>) -> Frame {
    let fields = key
        .map(|key| vec![Field::string(1, key)])
        .unwrap_or_default();
    Frame::request(Opcode::RuntimeCapClear, sequence, fields)
}

#[cfg(feature = "alloc")]
pub fn display_window_probe_request(sequence: u32, pattern: impl Into<String>) -> Frame {
    Frame::request(
        Opcode::DisplayWindowProbe,
        sequence,
        vec![Field::string(1, pattern)],
    )
}

#[cfg(feature = "alloc")]
pub fn key_request(sequence: u32, key: impl Into<String>) -> Frame {
    Frame::request(Opcode::Key, sequence, vec![Field::string(1, key)])
}

#[cfg(feature = "alloc")]
pub fn event_dispatch_request(
    sequence: u32,
    app_id: impl Into<String>,
    event: impl Into<String>,
) -> Frame {
    Frame::request(
        Opcode::EventDispatch,
        sequence,
        vec![Field::string(1, app_id), Field::string(2, event)],
    )
}

#[cfg(feature = "alloc")]
pub fn state_import_request(sequence: u32, bytes: Vec<u8>) -> Frame {
    Frame::request(Opcode::StateImport, sequence, vec![Field::bytes(1, bytes)])
}

#[cfg(feature = "alloc")]
pub fn wifi_profile_set_request(
    sequence: u32,
    profile: impl Into<String>,
    ssid: impl Into<String>,
    password: impl Into<String>,
) -> Frame {
    Frame::request(
        Opcode::WifiProfileSet,
        sequence,
        vec![
            Field::string(1, profile),
            Field::string(2, ssid),
            Field::string(3, password),
        ],
    )
}

#[cfg(feature = "alloc")]
pub fn app_install_begin_request(
    sequence: u32,
    app_id: impl Into<String>,
    total_len: u64,
    crc32: u64,
) -> Frame {
    Frame::request(
        Opcode::AppInstallBegin,
        sequence,
        vec![
            Field::string(1, app_id),
            Field::u64(2, total_len),
            Field::u64(3, crc32),
        ],
    )
}

#[cfg(feature = "alloc")]
pub fn app_install_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    app_install_chunk_request_with_ack(sequence, offset, bytes, true)
}

#[cfg(feature = "alloc")]
pub fn app_install_chunk_request_with_ack(
    sequence: u32,
    offset: u64,
    bytes: Vec<u8>,
    ack_requested: bool,
) -> Frame {
    Frame::request(
        Opcode::AppInstallChunk,
        sequence,
        transfer_chunk_fields(offset, bytes, ack_requested),
    )
}

#[cfg(feature = "alloc")]
pub fn app_install_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::AppInstallCommit, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn resource_install_begin_request(
    sequence: u32,
    app_id: impl Into<String>,
    path: impl Into<String>,
    total_len: u64,
    crc32: u64,
) -> Frame {
    Frame::request(
        Opcode::ResourceInstallBegin,
        sequence,
        vec![
            Field::string(1, app_id),
            Field::string(2, path),
            Field::u64(3, total_len),
            Field::u64(4, crc32),
        ],
    )
}

#[cfg(feature = "alloc")]
pub fn resource_install_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    resource_install_chunk_request_with_ack(sequence, offset, bytes, true)
}

#[cfg(feature = "alloc")]
pub fn resource_install_chunk_request_with_ack(
    sequence: u32,
    offset: u64,
    bytes: Vec<u8>,
    ack_requested: bool,
) -> Frame {
    Frame::request(
        Opcode::ResourceInstallChunk,
        sequence,
        transfer_chunk_fields(offset, bytes, ack_requested),
    )
}

#[cfg(feature = "alloc")]
pub fn resource_install_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ResourceInstallCommit, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn content_install_begin_request(
    sequence: u32,
    name: impl Into<String>,
    total_len: u64,
    crc32: u64,
) -> Frame {
    Frame::request(
        Opcode::ContentInstallBegin,
        sequence,
        vec![
            Field::string(1, name),
            Field::u64(2, total_len),
            Field::u64(3, crc32),
        ],
    )
}

#[cfg(feature = "alloc")]
pub fn content_install_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    content_install_chunk_request_with_ack(sequence, offset, bytes, true)
}

#[cfg(feature = "alloc")]
pub fn content_install_chunk_request_with_ack(
    sequence: u32,
    offset: u64,
    bytes: Vec<u8>,
    ack_requested: bool,
) -> Frame {
    Frame::request(
        Opcode::ContentInstallChunk,
        sequence,
        transfer_chunk_fields(offset, bytes, ack_requested),
    )
}

#[cfg(feature = "alloc")]
pub fn content_install_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ContentInstallCommit, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn content_check_request(sequence: u32, name: impl Into<String>) -> Frame {
    Frame::request(Opcode::ContentCheck, sequence, vec![Field::string(1, name)])
}

#[cfg(feature = "alloc")]
pub fn app_launch_request(sequence: u32, app_id: impl Into<String>) -> Frame {
    Frame::request(Opcode::AppLaunch, sequence, vec![Field::string(1, app_id)])
}

#[cfg(feature = "alloc")]
pub fn temp_run_begin_request(
    sequence: u32,
    app_id: impl Into<String>,
    total_len: u64,
    crc32: u64,
) -> Frame {
    Frame::request(
        Opcode::TempRunBegin,
        sequence,
        vec![
            Field::string(1, app_id),
            Field::u64(2, total_len),
            Field::u64(3, crc32),
        ],
    )
}

#[cfg(feature = "alloc")]
pub fn temp_run_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    temp_run_chunk_request_with_ack(sequence, offset, bytes, true)
}

#[cfg(feature = "alloc")]
pub fn temp_run_chunk_request_with_ack(
    sequence: u32,
    offset: u64,
    bytes: Vec<u8>,
    ack_requested: bool,
) -> Frame {
    Frame::request(
        Opcode::TempRunChunk,
        sequence,
        transfer_chunk_fields(offset, bytes, ack_requested),
    )
}

#[cfg(feature = "alloc")]
pub fn temp_run_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::TempRunCommit, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
fn transfer_chunk_fields(offset: u64, bytes: Vec<u8>, ack_requested: bool) -> Vec<Field> {
    vec![
        Field::u64(1, offset),
        Field::bytes(2, bytes),
        Field::bool(3, ack_requested),
    ]
}

#[cfg(feature = "alloc")]
pub fn hello_identity(frame: &Frame) -> Option<HelloIdentity> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::Hello
        || frame.status != Status::Ok
    {
        return None;
    }

    let mut target = None;
    let mut firmware = None;
    let mut diagnostic = None;
    let mut serial_max_frame_bytes = None;
    for field in &frame.fields {
        match (field.tag, &field.value) {
            (1, FieldValue::String(value)) => target = Some(value.clone()),
            (2, FieldValue::String(value)) => firmware = Some(value.clone()),
            (3, FieldValue::Bool(value)) => diagnostic = Some(*value),
            (4, FieldValue::U64(value)) => serial_max_frame_bytes = usize::try_from(*value).ok(),
            _ => {}
        }
    }

    let transfer_capabilities = serial_max_frame_bytes
        .map(TransferCapabilities::from_serial_frame_budget)
        .unwrap_or_else(TransferCapabilities::default_serial);
    Some(HelloIdentity {
        target: target?,
        firmware: firmware?,
        diagnostic: diagnostic.unwrap_or(false),
        transfer_capabilities,
    })
}

#[cfg(feature = "alloc")]
pub fn app_list_entries(frame: &Frame) -> Option<Vec<AppEntry>> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::AppList
        || frame.status != Status::Ok
    {
        return None;
    }

    let mut entries = Vec::new();
    for field in &frame.fields {
        let FieldValue::Record(fields) = &field.value else {
            continue;
        };
        if field.tag != 1 {
            continue;
        }

        let mut app_id = None;
        let mut sqbc_len = None;
        for field in fields {
            match (field.tag, &field.value) {
                (1, FieldValue::String(value)) => app_id = Some(value.clone()),
                (2, FieldValue::U64(value)) => sqbc_len = Some(*value),
                _ => {}
            }
        }
        entries.push(AppEntry {
            app_id: app_id?,
            sqbc_len: sqbc_len?,
        });
    }

    Some(entries)
}

#[cfg(feature = "alloc")]
pub fn output_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::OutputGet, 1)
}

#[cfg(feature = "alloc")]
pub fn trace_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::TraceGet, 1)
}

#[cfg(feature = "alloc")]
pub fn lifecycle_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::LifecycleGet, 1)
}

#[cfg(feature = "alloc")]
pub fn runtime_cap_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::RuntimeCapGet, 1)
}

#[cfg(feature = "alloc")]
pub fn drawlog_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::DrawlogGet, 1)
}

#[cfg(feature = "alloc")]
pub fn debug_log_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::DebugLogGet, 1)
}

#[cfg(feature = "alloc")]
pub fn error_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::ErrorsGet, 1)
}

#[cfg(feature = "alloc")]
pub fn state_bytes(frame: &Frame) -> Option<Vec<u8>> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::StateGet
        || frame.status != Status::Ok
    {
        return None;
    }
    for field in &frame.fields {
        if let (1, FieldValue::Bytes(value)) = (field.tag, &field.value) {
            return Some(value.clone());
        }
    }
    Some(Vec::new())
}

#[cfg(feature = "alloc")]
pub fn resource_values(frame: &Frame) -> Option<Vec<(String, u64)>> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::ResourcesGet
        || frame.status != Status::Ok
    {
        return None;
    }
    let mut values = Vec::new();
    for field in &frame.fields {
        let FieldValue::Record(fields) = &field.value else {
            continue;
        };
        if field.tag != 1 {
            continue;
        }
        let mut key = None;
        let mut value = None;
        for field in fields {
            match (field.tag, &field.value) {
                (1, FieldValue::String(text)) => key = Some(text.clone()),
                (1, FieldValue::U32(id)) => key = Some(resource_metric_name_for_id(*id)),
                (2, FieldValue::U32(number)) => value = Some(u64::from(*number)),
                (2, FieldValue::U64(number)) => value = Some(*number),
                _ => {}
            }
        }
        values.push((key?, value?));
    }
    Some(values)
}

#[cfg(feature = "alloc")]
pub fn content_check_result(frame: &Frame) -> Option<ContentCheckResult> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::ContentCheck
        || frame.status != Status::Ok
    {
        return None;
    }

    let mut name = None;
    let mut size = None;
    let mut crc32 = None;
    for field in &frame.fields {
        match (field.tag, &field.value) {
            (1, FieldValue::String(value)) => name = Some(value.clone()),
            (2, FieldValue::U64(value)) => size = Some(*value),
            (3, FieldValue::U64(value)) => crc32 = Some(*value),
            _ => {}
        }
    }

    Some(ContentCheckResult {
        name: name?,
        size: size?,
        crc32: crc32?,
    })
}

#[cfg(feature = "alloc")]
pub fn content_delete_request(sequence: u32, name: impl Into<String>) -> Frame {
    Frame::request(
        Opcode::ContentDelete,
        sequence,
        vec![Field::string(1, name)],
    )
}

#[cfg(feature = "alloc")]
pub fn content_delete_result(frame: &Frame) -> Option<String> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::ContentDelete
        || frame.status != Status::Ok
    {
        return None;
    }

    for field in &frame.fields {
        if let (1, FieldValue::String(value)) = (field.tag, &field.value) {
            return Some(value.clone());
        }
    }
    None
}

#[cfg(feature = "alloc")]
pub fn firmware_info_request(sequence: u32) -> Frame {
    Frame::request(Opcode::FirmwareInfo, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn firmware_update_begin_request(
    sequence: u32,
    total_len: u64,
    sha256: Vec<u8>,
    build_id: impl Into<String>,
) -> Frame {
    Frame::request(
        Opcode::FirmwareUpdateBegin,
        sequence,
        vec![
            Field::u64(1, total_len),
            Field::bytes(2, sha256),
            Field::string(3, build_id),
        ],
    )
}

#[cfg(feature = "alloc")]
pub fn firmware_update_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    Frame::request(
        Opcode::FirmwareUpdateChunk,
        sequence,
        vec![Field::u64(1, offset), Field::bytes(2, bytes)],
    )
}

#[cfg(feature = "alloc")]
pub fn firmware_update_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::FirmwareUpdateCommit, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn firmware_update_status_request(sequence: u32) -> Frame {
    Frame::request(Opcode::FirmwareUpdateStatus, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn firmware_update_abort_request(sequence: u32) -> Frame {
    Frame::request(Opcode::FirmwareUpdateAbort, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn firmware_info(frame: &Frame) -> Option<FirmwareInfo> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::FirmwareInfo
        || frame.status != Status::Ok
    {
        return None;
    }
    Some(FirmwareInfo {
        active_slot: frame_string(frame, 1)?,
        active_slot_size: frame_u64(frame, 2)?,
        inactive_slot: frame_string(frame, 3)?,
        inactive_slot_size: frame_u64(frame, 4)?,
        build_id: frame_string(frame, 5)?,
        boot_state: frame_string(frame, 6)?,
    })
}

#[cfg(feature = "alloc")]
pub fn firmware_update_status(frame: &Frame) -> Option<FirmwareUpdateStatus> {
    if frame.kind != FrameKind::Response
        || frame.opcode != Opcode::FirmwareUpdateStatus
        || frame.status == Status::Error
    {
        return None;
    }
    Some(FirmwareUpdateStatus {
        state: frame_string(frame, 1)?,
        candidate_slot: frame_string(frame, 2)?,
        expected_len: frame_u64(frame, 3)?,
        durable_offset: frame_u64(frame, 4)?,
        build_id: frame_string(frame, 5)?,
        expected_sha256: frame_bytes(frame, 6)?,
    })
}

#[cfg(feature = "alloc")]
fn frame_string(frame: &Frame, tag: u8) -> Option<String> {
    frame
        .fields
        .iter()
        .find_map(|field| match (field.tag, &field.value) {
            (actual, FieldValue::String(value)) if actual == tag => Some(value.clone()),
            _ => None,
        })
}

#[cfg(feature = "alloc")]
fn frame_u64(frame: &Frame, tag: u8) -> Option<u64> {
    frame
        .fields
        .iter()
        .find_map(|field| match (field.tag, &field.value) {
            (actual, FieldValue::U64(value)) if actual == tag => Some(*value),
            _ => None,
        })
}

#[cfg(feature = "alloc")]
fn frame_bytes(frame: &Frame, tag: u8) -> Option<Vec<u8>> {
    frame
        .fields
        .iter()
        .find_map(|field| match (field.tag, &field.value) {
            (actual, FieldValue::Bytes(value)) if actual == tag => Some(value.clone()),
            _ => None,
        })
}

#[cfg(feature = "alloc")]
pub fn protocol_error(frame: &Frame) -> Option<ProtocolError> {
    if frame.kind != FrameKind::Response || frame.status != Status::Error {
        return None;
    }
    let mut code = None;
    let mut message = None;
    for field in &frame.fields {
        match (field.tag, &field.value) {
            (250, FieldValue::I64(value)) => code = Some(*value),
            (251, FieldValue::String(value)) => message = Some(value.clone()),
            _ => {}
        }
    }
    Some(ProtocolError {
        code: code.unwrap_or(-1),
        message: message.unwrap_or_else(|| "protocol error".into()),
    })
}

#[cfg(feature = "alloc")]
fn repeated_string_fields(frame: &Frame, opcode: Opcode, tag: u8) -> Option<Vec<String>> {
    if frame.kind != FrameKind::Response || frame.opcode != opcode || frame.status != Status::Ok {
        return None;
    }

    let mut lines = Vec::new();
    for field in &frame.fields {
        if field.tag == tag {
            if let FieldValue::String(value) = &field.value {
                lines.push(value.clone());
            }
        }
    }
    Some(lines)
}

#[cfg(feature = "alloc")]
pub fn encode_frame(frame: &Frame) -> Vec<u8> {
    let needed = encoded_frame_len(frame).expect("encoded frame length exceeds usize");
    let mut out = vec![0; needed];
    encode_frame_into(frame, &mut out).expect("pre-sized output buffer must fit");
    out
}

#[cfg(feature = "alloc")]
pub fn encode_frame_into(frame: &Frame, out: &mut [u8]) -> Result<usize, DecodeError> {
    let payload_len = encoded_fields_len(&frame.fields)?;
    let needed = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    if out.len() < needed {
        return Err(DecodeError::OutputTooSmall {
            needed,
            capacity: out.len(),
        });
    }

    out[..4].copy_from_slice(&MAGIC);
    out[4] = frame.kind as u8;
    out[5] = frame.opcode as u8;
    out[6] = frame.status as u8;
    out[7] = 0;
    out[8..12].copy_from_slice(&frame.sequence.to_le_bytes());
    out[12..16].copy_from_slice(&(payload_len as u32).to_le_bytes());
    encode_fields_into(&frame.fields, &mut out[HEADER_LEN..needed])?;
    let crc = crc32fast::hash(&out[HEADER_LEN..needed]);
    out[16..20].copy_from_slice(&crc.to_le_bytes());
    Ok(needed)
}

pub fn encode_empty_response_into(
    opcode: Opcode,
    status: Status,
    sequence: u32,
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    encode_response_payload_into(opcode, status, sequence, 0, out, |_| Ok(()))
}

pub fn encode_hello_response_into(
    opcode: Opcode,
    sequence: u32,
    target: &str,
    firmware: &str,
    diagnostic: bool,
    serial_max_frame_bytes: u64,
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    let payload_len = tlv_string_len(target)?
        .checked_add(tlv_string_len(firmware)?)
        .and_then(|len| len.checked_add(tlv_bool_len()))
        .and_then(|len| len.checked_add(tlv_u64_len()))
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    encode_response_payload_into(opcode, Status::Ok, sequence, payload_len, out, |payload| {
        let rest = write_string_tlv(payload, 1, target)?;
        let rest = write_string_tlv(rest, 2, firmware)?;
        let rest = write_bool_tlv(rest, 3, diagnostic)?;
        write_u64_tlv(rest, 4, serial_max_frame_bytes)?;
        Ok(())
    })
}

pub fn encode_app_list_response_into<'a, I>(
    sequence: u32,
    entries: I,
    out: &mut [u8],
) -> Result<usize, DecodeError>
where
    I: Clone + Iterator<Item = AppListEntry<'a>>,
{
    let mut payload_len = 0usize;
    for entry in entries.clone() {
        let record_len = tlv_string_len(entry.app_id)?
            .checked_add(tlv_u64_len())
            .ok_or(DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: out.len(),
            })?;
        payload_len = payload_len.checked_add(tlv_record_len(record_len)?).ok_or(
            DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: out.len(),
            },
        )?;
    }

    encode_response_payload_into(
        Opcode::AppList,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |mut payload| {
            for entry in entries {
                let record_len = tlv_string_len(entry.app_id)?
                    .checked_add(tlv_u64_len())
                    .ok_or(DecodeError::OutputTooSmall {
                        needed: usize::MAX,
                        capacity: payload.len(),
                    })?;
                write_tlv_header(payload, 1, 32, record_len)?;
                let (record, rest) = payload[4..].split_at_mut(record_len);
                let rest_record = write_string_tlv(record, 1, entry.app_id)?;
                write_u64_tlv(rest_record, 2, entry.sqbc_len)?;
                payload = rest;
            }
            Ok(())
        },
    )
}

pub fn encode_line_response_into<'a, I>(
    opcode: Opcode,
    sequence: u32,
    lines: I,
    out: &mut [u8],
) -> Result<usize, DecodeError>
where
    I: Clone + Iterator<Item = &'a str>,
{
    let mut payload_len = 0usize;
    for line in lines.clone() {
        payload_len =
            payload_len
                .checked_add(tlv_string_len(line)?)
                .ok_or(DecodeError::OutputTooSmall {
                    needed: usize::MAX,
                    capacity: out.len(),
                })?;
    }

    encode_response_payload_into(
        opcode,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |mut payload| {
            for line in lines {
                payload = write_string_tlv(payload, 1, line)?;
            }
            Ok(())
        },
    )
}

pub fn encode_resources_response_into<'a, I>(
    sequence: u32,
    metrics: I,
    out: &mut [u8],
) -> Result<usize, DecodeError>
where
    I: Clone + Iterator<Item = ResourceMetric<'a>>,
{
    let mut payload_len = 0usize;
    for _metric in metrics.clone() {
        let record_len =
            tlv_u32_len()
                .checked_add(tlv_u32_len())
                .ok_or(DecodeError::OutputTooSmall {
                    needed: usize::MAX,
                    capacity: out.len(),
                })?;
        payload_len = payload_len.checked_add(tlv_record_len(record_len)?).ok_or(
            DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: out.len(),
            },
        )?;
    }

    encode_response_payload_into(
        Opcode::ResourcesGet,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |mut payload| {
            for metric in metrics {
                let value =
                    u32::try_from(metric.value).map_err(|_| DecodeError::OutputTooSmall {
                        needed: metric.value as usize,
                        capacity: u32::MAX as usize,
                    })?;
                let key =
                    resource_metric_id_for_name(metric.key).ok_or(DecodeError::OutputTooSmall {
                        needed: metric.key.len(),
                        capacity: 0,
                    })?;
                let record_len = tlv_u32_len().checked_add(tlv_u32_len()).ok_or(
                    DecodeError::OutputTooSmall {
                        needed: usize::MAX,
                        capacity: payload.len(),
                    },
                )?;
                write_tlv_header(payload, 1, 32, record_len)?;
                let (record, rest) = payload[4..].split_at_mut(record_len);
                let record = write_u32_tlv(record, 1, key)?;
                write_u32_tlv(record, 2, value)?;
                payload = rest;
            }
            Ok(())
        },
    )
}

pub fn encode_state_response_into(
    sequence: u32,
    bytes: &[u8],
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    let payload_len = tlv_bytes_len(bytes)?;
    encode_response_payload_into(
        Opcode::StateGet,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |payload| {
            write_bytes_tlv(payload, 1, bytes)?;
            Ok(())
        },
    )
}

pub fn encode_lifecycle_response_into<'a, P, A>(
    sequence: u32,
    active_app: Option<&str>,
    process_stack: P,
    armed_timers: A,
    out: &mut [u8],
) -> Result<usize, DecodeError>
where
    P: Clone + Iterator<Item = &'a str>,
    A: Clone + Iterator<Item = LifecycleTimer<'a>>,
{
    encode_lifecycle_response_with_details_into(
        sequence,
        active_app,
        process_stack,
        armed_timers,
        core::iter::empty(),
        out,
    )
}

pub fn encode_lifecycle_response_with_details_into<'a, P, A, D>(
    sequence: u32,
    active_app: Option<&str>,
    process_stack: P,
    armed_timers: A,
    details: D,
    out: &mut [u8],
) -> Result<usize, DecodeError>
where
    P: Clone + Iterator<Item = &'a str>,
    A: Clone + Iterator<Item = LifecycleTimer<'a>>,
    D: Clone + Iterator<Item = &'a str>,
{
    let active = active_app.unwrap_or("");
    let mut payload_len = tlv_string_len_for_len("active=".len() + active.len())?;
    for (index, app_id) in process_stack.clone().enumerate() {
        payload_len = payload_len
            .checked_add(tlv_string_len_for_len(
                "process_stack[".len() + decimal_len(index) + "]=".len() + app_id.len(),
            )?)
            .ok_or(DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: out.len(),
            })?;
    }
    payload_len = payload_len
        .checked_add(tlv_string_len("armed_stack=")?)
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    for (index, timer) in armed_timers.clone().enumerate() {
        payload_len = payload_len
            .checked_add(tlv_string_len_for_len(
                "armed_stack[".len()
                    + decimal_len(index)
                    + "]=".len()
                    + timer.app_id.len()
                    + 1
                    + timer.event.len(),
            )?)
            .ok_or(DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: out.len(),
            })?;
    }
    for detail in details.clone() {
        payload_len = payload_len.checked_add(tlv_string_len(detail)?).ok_or(
            DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: out.len(),
            },
        )?;
    }

    encode_response_payload_into(
        Opcode::LifecycleGet,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |mut payload| {
            payload = write_active_line_tlv(payload, active)?;
            for (index, app_id) in process_stack.enumerate() {
                payload = write_process_line_tlv(payload, index, app_id)?;
            }
            payload = write_string_tlv(payload, 1, "armed_stack=")?;
            for (index, timer) in armed_timers.enumerate() {
                payload = write_armed_line_tlv(payload, index, timer.app_id, timer.event)?;
            }
            for detail in details {
                payload = write_string_tlv(payload, 1, detail)?;
            }
            Ok(())
        },
    )
}

pub fn encode_content_check_response_into(
    sequence: u32,
    name: &str,
    size: u64,
    crc32: u64,
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    let payload_len = tlv_string_len(name)?
        .checked_add(tlv_u64_len())
        .and_then(|len| len.checked_add(tlv_u64_len()))
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    encode_response_payload_into(
        Opcode::ContentCheck,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |payload| {
            let rest = write_string_tlv(payload, 1, name)?;
            let rest = write_u64_tlv(rest, 2, size)?;
            write_u64_tlv(rest, 3, crc32)?;
            Ok(())
        },
    )
}

pub fn encode_content_delete_response_into(
    sequence: u32,
    name: &str,
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    let payload_len = tlv_string_len(name)?;
    encode_response_payload_into(
        Opcode::ContentDelete,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |payload| {
            write_string_tlv(payload, 1, name)?;
            Ok(())
        },
    )
}

pub fn encode_firmware_info_response_into(
    sequence: u32,
    info: FirmwareInfoRef<'_>,
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    let payload_len = tlv_string_len(info.active_slot)?
        .checked_add(tlv_u64_len())
        .and_then(|len| len.checked_add(tlv_string_len(info.inactive_slot).ok()?))
        .and_then(|len| len.checked_add(tlv_u64_len()))
        .and_then(|len| len.checked_add(tlv_string_len(info.build_id).ok()?))
        .and_then(|len| len.checked_add(tlv_string_len(info.boot_state).ok()?))
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    encode_response_payload_into(
        Opcode::FirmwareInfo,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |payload| {
            let rest = write_string_tlv(payload, 1, info.active_slot)?;
            let rest = write_u64_tlv(rest, 2, info.active_slot_size)?;
            let rest = write_string_tlv(rest, 3, info.inactive_slot)?;
            let rest = write_u64_tlv(rest, 4, info.inactive_slot_size)?;
            let rest = write_string_tlv(rest, 5, info.build_id)?;
            write_string_tlv(rest, 6, info.boot_state)?;
            Ok(())
        },
    )
}

pub fn encode_firmware_update_status_response_into(
    sequence: u32,
    status: Status,
    update: FirmwareUpdateStatusRef<'_>,
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    if status == Status::Error {
        return Err(DecodeError::UnknownStatus(status as u8));
    }
    if update.expected_sha256.len() != FIRMWARE_SHA256_BYTES {
        return Err(DecodeError::InvalidIntegerLength(
            update.expected_sha256.len(),
        ));
    }
    let payload_len = tlv_string_len(update.state)?
        .checked_add(tlv_string_len(update.candidate_slot)?)
        .and_then(|len| len.checked_add(tlv_u64_len()))
        .and_then(|len| len.checked_add(tlv_u64_len()))
        .and_then(|len| len.checked_add(tlv_string_len(update.build_id).ok()?))
        .and_then(|len| len.checked_add(tlv_bytes_len(update.expected_sha256).ok()?))
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    encode_response_payload_into(
        Opcode::FirmwareUpdateStatus,
        status,
        sequence,
        payload_len,
        out,
        |payload| {
            let rest = write_string_tlv(payload, 1, update.state)?;
            let rest = write_string_tlv(rest, 2, update.candidate_slot)?;
            let rest = write_u64_tlv(rest, 3, update.expected_len)?;
            let rest = write_u64_tlv(rest, 4, update.durable_offset)?;
            let rest = write_string_tlv(rest, 5, update.build_id)?;
            write_bytes_tlv(rest, 6, update.expected_sha256)?;
            Ok(())
        },
    )
}

pub fn encode_error_response_into(
    opcode: Opcode,
    sequence: u32,
    code: i64,
    message: &str,
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    let payload_len =
        tlv_i64_len()
            .checked_add(tlv_string_len(message)?)
            .ok_or(DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: out.len(),
            })?;
    encode_response_payload_into(
        opcode,
        Status::Error,
        sequence,
        payload_len,
        out,
        |payload| {
            let rest = write_i64_tlv(payload, 250, code)?;
            write_string_tlv(rest, 251, message)?;
            Ok(())
        },
    )
}

fn encode_response_payload_into(
    opcode: Opcode,
    status: Status,
    sequence: u32,
    payload_len: usize,
    out: &mut [u8],
    write_payload: impl FnOnce(&mut [u8]) -> Result<(), DecodeError>,
) -> Result<usize, DecodeError> {
    if payload_len > u32::MAX as usize {
        return Err(DecodeError::OutputTooSmall {
            needed: payload_len,
            capacity: u32::MAX as usize,
        });
    }
    let needed = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    if out.len() < needed {
        return Err(DecodeError::OutputTooSmall {
            needed,
            capacity: out.len(),
        });
    }

    out[..4].copy_from_slice(&MAGIC);
    out[4] = FrameKind::Response as u8;
    out[5] = opcode as u8;
    out[6] = status as u8;
    out[7] = 0;
    out[8..12].copy_from_slice(&sequence.to_le_bytes());
    out[12..16].copy_from_slice(&(payload_len as u32).to_le_bytes());
    write_payload(&mut out[HEADER_LEN..needed])?;
    let crc = crc32fast::hash(&out[HEADER_LEN..needed]);
    out[16..20].copy_from_slice(&crc.to_le_bytes());
    Ok(needed)
}

#[cfg(feature = "alloc")]
pub fn decode_frame(bytes: &[u8]) -> Result<Frame, DecodeError> {
    if bytes.len() < HEADER_LEN {
        return Err(DecodeError::TruncatedHeader);
    }
    if bytes[..4] != MAGIC {
        return Err(DecodeError::BadMagic);
    }

    let kind = FrameKind::try_from(bytes[4])?;
    let opcode = Opcode::try_from(bytes[5])?;
    let status = Status::try_from(bytes[6])?;
    let sequence = u32::from_le_bytes(bytes[8..12].try_into().expect("slice length checked"));
    let payload_len = u32::from_le_bytes(bytes[12..16].try_into().expect("slice length checked"));
    let payload_crc = u32::from_le_bytes(bytes[16..20].try_into().expect("slice length checked"));
    let expected = HEADER_LEN + payload_len as usize;
    if bytes.len() != expected {
        return Err(DecodeError::LengthMismatch {
            expected,
            actual: bytes.len(),
        });
    }

    let payload = &bytes[HEADER_LEN..];
    if crc32fast::hash(payload) != payload_crc {
        return Err(DecodeError::PayloadCrc);
    }

    let fields = decode_fields(payload)?;
    Ok(Frame {
        kind,
        opcode,
        status,
        sequence,
        fields,
    })
}

#[cfg(feature = "alloc")]
pub fn decode_frame_from_stream(bytes: &[u8]) -> Result<Frame, DecodeError> {
    let Some(start) = bytes
        .windows(MAGIC.len())
        .position(|window| window == MAGIC)
    else {
        return Err(DecodeError::BadMagic);
    };
    if bytes.len() - start < HEADER_LEN {
        return Err(DecodeError::TruncatedHeader);
    }
    let payload_len = u32::from_le_bytes(
        bytes[start + 12..start + 16]
            .try_into()
            .expect("slice length checked"),
    ) as usize;
    let end = start + HEADER_LEN + payload_len;
    if bytes.len() < end {
        return Err(DecodeError::LengthMismatch {
            expected: HEADER_LEN + payload_len,
            actual: bytes.len() - start,
        });
    }
    decode_frame(&bytes[start..end])
}

fn tlv_string_len(value: &str) -> Result<usize, DecodeError> {
    tlv_string_len_for_len(value.len())
}

fn tlv_string_len_for_len(value_len: usize) -> Result<usize, DecodeError> {
    if value_len > u16::MAX as usize {
        return Err(DecodeError::OutputTooSmall {
            needed: value_len,
            capacity: u16::MAX as usize,
        });
    }
    Ok(4 + value_len)
}

fn tlv_bytes_len(value: &[u8]) -> Result<usize, DecodeError> {
    if value.len() > u16::MAX as usize {
        return Err(DecodeError::OutputTooSmall {
            needed: value.len(),
            capacity: u16::MAX as usize,
        });
    }
    Ok(4 + value.len())
}

fn tlv_bool_len() -> usize {
    5
}

fn tlv_i64_len() -> usize {
    12
}

fn tlv_u64_len() -> usize {
    12
}

fn tlv_u32_len() -> usize {
    8
}

fn tlv_record_len(value_len: usize) -> Result<usize, DecodeError> {
    if value_len > u16::MAX as usize {
        return Err(DecodeError::OutputTooSmall {
            needed: value_len,
            capacity: u16::MAX as usize,
        });
    }
    Ok(4 + value_len)
}

fn write_string_tlv<'a>(
    out: &'a mut [u8],
    tag: u8,
    value: &str,
) -> Result<&'a mut [u8], DecodeError> {
    write_tlv_header(out, tag, 1, value.len())?;
    out[4..4 + value.len()].copy_from_slice(value.as_bytes());
    Ok(&mut out[4 + value.len()..])
}

fn write_bytes_tlv<'a>(
    out: &'a mut [u8],
    tag: u8,
    value: &[u8],
) -> Result<&'a mut [u8], DecodeError> {
    write_tlv_header(out, tag, 0, value.len())?;
    out[4..4 + value.len()].copy_from_slice(value);
    Ok(&mut out[4 + value.len()..])
}

fn write_bool_tlv(out: &mut [u8], tag: u8, value: bool) -> Result<&mut [u8], DecodeError> {
    write_tlv_header(out, tag, 3, 1)?;
    out[4] = u8::from(value);
    Ok(&mut out[5..])
}

fn write_i64_tlv(out: &mut [u8], tag: u8, value: i64) -> Result<&mut [u8], DecodeError> {
    write_tlv_header(out, tag, 4, 8)?;
    out[4..12].copy_from_slice(&value.to_le_bytes());
    Ok(&mut out[12..])
}

fn write_u64_tlv(out: &mut [u8], tag: u8, value: u64) -> Result<&mut [u8], DecodeError> {
    write_tlv_header(out, tag, 5, 8)?;
    out[4..12].copy_from_slice(&value.to_le_bytes());
    Ok(&mut out[12..])
}

fn write_u32_tlv(out: &mut [u8], tag: u8, value: u32) -> Result<&mut [u8], DecodeError> {
    write_tlv_header(out, tag, 6, 4)?;
    out[4..8].copy_from_slice(&value.to_le_bytes());
    Ok(&mut out[8..])
}

fn write_active_line_tlv<'a>(out: &'a mut [u8], active: &str) -> Result<&'a mut [u8], DecodeError> {
    let len = "active=".len() + active.len();
    write_tlv_header(out, 1, 1, len)?;
    let mut offset = 4;
    offset = write_bytes(out, offset, b"active=");
    offset = write_bytes(out, offset, active.as_bytes());
    Ok(&mut out[offset..])
}

fn write_process_line_tlv<'a>(
    out: &'a mut [u8],
    index: usize,
    app_id: &str,
) -> Result<&'a mut [u8], DecodeError> {
    let len = "process_stack[".len() + decimal_len(index) + "]=".len() + app_id.len();
    write_tlv_header(out, 1, 1, len)?;
    let mut offset = 4;
    offset = write_bytes(out, offset, b"process_stack[");
    offset = write_decimal(out, offset, index);
    offset = write_bytes(out, offset, b"]=");
    offset = write_bytes(out, offset, app_id.as_bytes());
    Ok(&mut out[offset..])
}

fn write_armed_line_tlv<'a>(
    out: &'a mut [u8],
    index: usize,
    app_id: &str,
    event: &str,
) -> Result<&'a mut [u8], DecodeError> {
    let len =
        "armed_stack[".len() + decimal_len(index) + "]=".len() + app_id.len() + 1 + event.len();
    write_tlv_header(out, 1, 1, len)?;
    let mut offset = 4;
    offset = write_bytes(out, offset, b"armed_stack[");
    offset = write_decimal(out, offset, index);
    offset = write_bytes(out, offset, b"]=");
    offset = write_bytes(out, offset, app_id.as_bytes());
    offset = write_bytes(out, offset, b" ");
    offset = write_bytes(out, offset, event.as_bytes());
    Ok(&mut out[offset..])
}

fn write_bytes(out: &mut [u8], offset: usize, bytes: &[u8]) -> usize {
    out[offset..offset + bytes.len()].copy_from_slice(bytes);
    offset + bytes.len()
}

fn write_decimal(out: &mut [u8], offset: usize, mut value: usize) -> usize {
    let len = decimal_len(value);
    for index in (0..len).rev() {
        out[offset + index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    offset + len
}

fn decimal_len(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}

fn write_tlv_header(
    out: &mut [u8],
    tag: u8,
    field_type: u8,
    value_len: usize,
) -> Result<(), DecodeError> {
    if value_len > u16::MAX as usize || out.len() < 4 + value_len {
        return Err(DecodeError::OutputTooSmall {
            needed: 4 + value_len,
            capacity: out.len(),
        });
    }
    out[0] = tag;
    out[1] = field_type;
    out[2..4].copy_from_slice(&(value_len as u16).to_le_bytes());
    Ok(())
}

#[cfg(feature = "alloc")]
pub fn parse_field_arg(kind: &str, value: &str) -> Result<Field, String> {
    let (tag, raw_value) = value
        .split_once('=')
        .ok_or_else(|| format!("protocol field must be TAG=VALUE, got {value:?}"))?;
    let tag = parse_tag(tag)?;
    match kind {
        "string" => Ok(Field::string(tag, raw_value)),
        "bytes" => Ok(Field::bytes(tag, parse_hex_bytes(raw_value)?)),
        "bool" => match raw_value {
            "true" => Ok(Field::bool(tag, true)),
            "false" => Ok(Field::bool(tag, false)),
            _ => Err(format!("bool field {tag} must be true or false")),
        },
        "u64" => Ok(Field::u64(
            tag,
            raw_value
                .parse()
                .map_err(|error| format!("invalid u64 field {tag}: {error}"))?,
        )),
        "u32" => Ok(Field::u32(
            tag,
            raw_value
                .parse()
                .map_err(|error| format!("invalid u32 field {tag}: {error}"))?,
        )),
        "i64" => Ok(Field::i64(
            tag,
            raw_value
                .parse()
                .map_err(|error| format!("invalid i64 field {tag}: {error}"))?,
        )),
        _ => Err(format!("unsupported field kind: {kind}")),
    }
}

#[cfg(feature = "alloc")]
pub fn encoded_frame_len(frame: &Frame) -> Result<usize, DecodeError> {
    HEADER_LEN
        .checked_add(encoded_fields_len(&frame.fields)?)
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: 0,
        })
}

pub fn transfer_chunk_payload_for_frame_budget(frame_budget: usize) -> usize {
    const TRANSFER_CHUNK_FIXED_BYTES: usize = HEADER_LEN + 4 + 8 + 4 + 4 + 4 + 1;
    frame_budget.saturating_sub(TRANSFER_CHUNK_FIXED_BYTES)
}

#[cfg(feature = "alloc")]
fn encoded_fields_len(fields: &[Field]) -> Result<usize, DecodeError> {
    let mut len = 0usize;
    for field in fields {
        let value_len = encoded_field_value_len(&field.value)?;
        if value_len > u16::MAX as usize {
            return Err(DecodeError::OutputTooSmall {
                needed: value_len,
                capacity: u16::MAX as usize,
            });
        }
        len = len
            .checked_add(4)
            .and_then(|value| value.checked_add(value_len))
            .ok_or(DecodeError::OutputTooSmall {
                needed: usize::MAX,
                capacity: 0,
            })?;
    }
    Ok(len)
}

#[cfg(feature = "alloc")]
fn encoded_field_value_len(value: &FieldValue) -> Result<usize, DecodeError> {
    match value {
        FieldValue::Bytes(value) => Ok(value.len()),
        FieldValue::String(value) => Ok(value.len()),
        FieldValue::Bool(_) => Ok(1),
        FieldValue::I64(_) | FieldValue::U64(_) => Ok(8),
        FieldValue::U32(_) => Ok(4),
        FieldValue::Record(fields) => encoded_fields_len(fields),
    }
}

#[cfg(feature = "alloc")]
fn encode_fields_into(fields: &[Field], mut out: &mut [u8]) -> Result<(), DecodeError> {
    for field in fields {
        let field_type = match &field.value {
            FieldValue::Bytes(_) => 0,
            FieldValue::String(_) => 1,
            FieldValue::Bool(_) => 3,
            FieldValue::I64(_) => 4,
            FieldValue::U64(_) => 5,
            FieldValue::U32(_) => 6,
            FieldValue::Record(_) => 32,
        };
        let value_len = encoded_field_value_len(&field.value)?;
        if out.len() < 4 + value_len {
            return Err(DecodeError::OutputTooSmall {
                needed: 4 + value_len,
                capacity: out.len(),
            });
        }
        out[0] = field.tag;
        out[1] = field_type;
        out[2..4].copy_from_slice(&(value_len as u16).to_le_bytes());
        encode_field_value_into(&field.value, &mut out[4..4 + value_len])?;
        out = &mut out[4 + value_len..];
    }
    Ok(())
}

#[cfg(feature = "alloc")]
fn encode_field_value_into(value: &FieldValue, out: &mut [u8]) -> Result<(), DecodeError> {
    match value {
        FieldValue::Bytes(value) => out.copy_from_slice(value),
        FieldValue::String(value) => out.copy_from_slice(value.as_bytes()),
        FieldValue::Bool(value) => out[0] = u8::from(*value),
        FieldValue::I64(value) => out.copy_from_slice(&value.to_le_bytes()),
        FieldValue::U64(value) => out.copy_from_slice(&value.to_le_bytes()),
        FieldValue::U32(value) => out.copy_from_slice(&value.to_le_bytes()),
        FieldValue::Record(fields) => encode_fields_into(fields, out)?,
    }
    Ok(())
}

#[cfg(feature = "alloc")]
fn decode_fields(mut payload: &[u8]) -> Result<Vec<Field>, DecodeError> {
    let mut fields = Vec::new();
    while !payload.is_empty() {
        if payload.len() < 4 {
            return Err(DecodeError::TruncatedField);
        }
        let tag = payload[0];
        let field_type = payload[1];
        let len = u16::from_le_bytes([payload[2], payload[3]]) as usize;
        payload = &payload[4..];
        if payload.len() < len {
            return Err(DecodeError::TruncatedField);
        }
        let value = &payload[..len];
        fields.push(Field {
            tag,
            value: decode_field_value(field_type, value)?,
        });
        payload = &payload[len..];
    }
    Ok(fields)
}

#[cfg(feature = "alloc")]
fn decode_field_value(field_type: u8, value: &[u8]) -> Result<FieldValue, DecodeError> {
    match field_type {
        0 => Ok(FieldValue::Bytes(value.to_vec())),
        1 => String::from_utf8(value.to_vec())
            .map(FieldValue::String)
            .map_err(|_| DecodeError::InvalidUtf8),
        3 => match value {
            [0] => Ok(FieldValue::Bool(false)),
            [1] => Ok(FieldValue::Bool(true)),
            _ => Err(DecodeError::InvalidBoolLength(value.len())),
        },
        4 => {
            let bytes: [u8; 8] = value
                .try_into()
                .map_err(|_| DecodeError::InvalidIntegerLength(value.len()))?;
            Ok(FieldValue::I64(i64::from_le_bytes(bytes)))
        }
        5 => {
            let bytes: [u8; 8] = value
                .try_into()
                .map_err(|_| DecodeError::InvalidIntegerLength(value.len()))?;
            Ok(FieldValue::U64(u64::from_le_bytes(bytes)))
        }
        6 => {
            let bytes: [u8; 4] = value
                .try_into()
                .map_err(|_| DecodeError::InvalidIntegerLength(value.len()))?;
            Ok(FieldValue::U32(u32::from_le_bytes(bytes)))
        }
        32 => Ok(FieldValue::Record(decode_fields(value)?)),
        _ => Err(DecodeError::UnknownFieldType(field_type)),
    }
}

#[cfg(feature = "alloc")]
fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|ch| *ch != '-' && *ch != '_' && *ch != '.')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(feature = "alloc")]
fn parse_tag(value: &str) -> Result<u8, String> {
    value
        .parse()
        .map_err(|error| format!("invalid field tag {value:?}: {error}"))
}

#[cfg(feature = "alloc")]
fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() % 2 != 0 {
        return Err("hex byte fields must contain an even number of digits".into());
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|chunk| {
            let text = str::from_utf8(chunk).expect("hex chunks are ascii");
            u8::from_str_radix(text, 16)
                .map_err(|error| format!("invalid hex byte {text}: {error}"))
        })
        .collect()
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::{
        app_install_chunk_request_with_ack, content_check_request, content_check_result,
        content_delete_request, content_delete_result, display_window_probe_request,
        encode_content_check_response_into, encode_content_delete_response_into, hello_identity,
        Field, FieldValue, Frame, Opcode, Status, TransferCapabilities, CONTENT_LIBRARY_PREFIX,
        MAX_CONTENT_NAME_BYTES, MAX_PATH_LEN,
    };
    use crate::decode_frame;

    #[test]
    fn transfer_chunk_requests_carry_ack_intent() {
        let frame = app_install_chunk_request_with_ack(42, 1024, vec![1, 2, 3], false);

        assert_eq!(frame.opcode, Opcode::AppInstallChunk);
        assert_eq!(frame.status, Status::Ok);
        assert_eq!(frame.fields.len(), 3);
        assert_eq!(frame.fields[2].tag, 3);
        assert_eq!(frame.fields[2].value, FieldValue::Bool(false));
    }

    #[test]
    fn hello_identity_carries_serial_transfer_budget() {
        let response = Frame::response(
            Opcode::Hello,
            Status::Ok,
            1,
            vec![
                Field::string(1, "xteink-x4"),
                Field::string(2, "squidscript-native"),
                Field::bool(3, true),
                Field::u64(4, 4096),
            ],
        );

        let identity = hello_identity(&response).expect("hello identity decodes");
        assert_eq!(identity.target, "xteink-x4");
        assert_eq!(identity.transfer_capabilities.max_frame_bytes, 4096);
        assert!(identity.transfer_capabilities.max_payload_bytes > 3900);
        assert_eq!(
            identity.transfer_capabilities.ack_window_bytes,
            identity.transfer_capabilities.max_payload_bytes
        );
    }

    #[test]
    fn transfer_capabilities_default_to_serial_window_limits() {
        let caps = TransferCapabilities::default_serial();

        assert_eq!(caps.max_frame_bytes, 8192);
        assert!(caps.max_payload_bytes > 900);
        assert_eq!(caps.ack_window_bytes, caps.max_payload_bytes);
    }

    #[test]
    fn content_check_request_and_response_are_generic_file_verification() {
        let request = content_check_request(91, "transfer-smoke.dat");

        assert_eq!(request.opcode, Opcode::ContentCheck);
        assert_eq!(request.fields, vec![Field::string(1, "transfer-smoke.dat")]);

        let response = Frame::response(
            Opcode::ContentCheck,
            Status::Ok,
            91,
            vec![
                Field::string(1, "transfer-smoke.dat"),
                Field::u64(2, 8192),
                Field::u64(3, 0x1234_abcd),
            ],
        );

        let checked = content_check_result(&response).expect("content check response decodes");
        assert_eq!(checked.name, "transfer-smoke.dat");
        assert_eq!(checked.size, 8192);
        assert_eq!(checked.crc32, 0x1234_abcd);

        let wrong_opcode =
            Frame::response(Opcode::ContentInstallCommit, Status::Ok, 91, Vec::new());
        assert!(content_check_result(&wrong_opcode).is_none());
    }

    #[test]
    fn content_name_budget_leaves_room_for_library_prefix_and_terminator() {
        assert_eq!(MAX_CONTENT_NAME_BYTES, 121);
        assert_eq!(
            CONTENT_LIBRARY_PREFIX.len() + MAX_CONTENT_NAME_BYTES,
            MAX_PATH_LEN - 1
        );
    }

    #[test]
    fn content_check_response_encoder_round_trips() {
        let mut bytes = [0u8; 128];
        let len = encode_content_check_response_into(
            91,
            "transfer-smoke.dat",
            8192,
            0x1234_abcd,
            &mut bytes,
        )
        .unwrap();
        let frame = decode_frame(&bytes[..len]).unwrap();
        let checked = content_check_result(&frame).expect("content check response decodes");

        assert_eq!(checked.name, "transfer-smoke.dat");
        assert_eq!(checked.size, 8192);
        assert_eq!(checked.crc32, 0x1234_abcd);
    }

    #[test]
    fn display_window_probe_request_carries_pattern_name() {
        let request = display_window_probe_request(85, "corners");

        assert_eq!(request.opcode, Opcode::DisplayWindowProbe);
        assert_eq!(request.fields, vec![Field::string(1, "corners")]);
    }

    #[test]
    fn content_delete_request_and_response() {
        let request = content_delete_request(93, "old-book.binbook");

        assert_eq!(request.opcode, Opcode::ContentDelete);
        assert_eq!(request.fields, vec![Field::string(1, "old-book.binbook")]);

        let response = Frame::response(
            Opcode::ContentDelete,
            Status::Ok,
            93,
            vec![Field::string(1, "old-book.binbook")],
        );

        let deleted = content_delete_result(&response).expect("content delete response decodes");
        assert_eq!(deleted, "old-book.binbook");

        let wrong_opcode = Frame::response(Opcode::ContentCheck, Status::Ok, 93, Vec::new());
        assert!(content_delete_result(&wrong_opcode).is_none());
    }

    #[test]
    fn content_delete_response_encoder_round_trips() {
        let mut bytes = [0u8; 128];
        let len = encode_content_delete_response_into(93, "old-book.binbook", &mut bytes).unwrap();
        let frame = decode_frame(&bytes[..len]).unwrap();
        let deleted = content_delete_result(&frame).expect("content delete response decodes");

        assert_eq!(deleted, "old-book.binbook");
    }
}
