#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::{format, string::String, vec, vec::Vec};
use core::str;

pub const MAGIC: [u8; 4] = *b"SQDP";
pub const HEADER_LEN: usize = 20;
pub const MAX_APP_ID_LEN: usize = 40;
pub const MAX_PATH_LEN: usize = 128;
pub const MAX_APP_BYTES: usize = 65_536;

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
struct TransferSession {
    active: bool,
    app_id: FixedStr<MAX_APP_ID_LEN>,
    total_len: usize,
    received: usize,
    expected_crc: u32,
    running_crc: u32,
    staging_path: FixedStr<MAX_PATH_LEN>,
}

impl Default for TransferSession {
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

impl TransferSession {
    fn begin(
        &mut self,
        app_id: &str,
        total_len: usize,
        expected_crc: u32,
    ) -> Result<(), SessionError> {
        validate_transfer_len(total_len)?;
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
    transfer: TransferSession,
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
        self.transfer.begin(app_id, total_len, expected_crc)?;
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
    install: TransferSession,
    temp_run: TransferSession,
    resource: ResourceSession,
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
}

fn validate_transfer_len(total_len: usize) -> Result<(), SessionError> {
    if total_len == 0 {
        return Err(SessionError::InvalidRequest);
    }
    if total_len > MAX_APP_BYTES {
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
pub fn errors_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ErrorsGet, sequence, Vec::new())
}

#[cfg(feature = "alloc")]
pub fn resources_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ResourcesGet, sequence, Vec::new())
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
    Frame::request(
        Opcode::AppInstallChunk,
        sequence,
        vec![Field::u64(1, offset), Field::bytes(2, bytes)],
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
    Frame::request(
        Opcode::ResourceInstallChunk,
        sequence,
        vec![Field::u64(1, offset), Field::bytes(2, bytes)],
    )
}

#[cfg(feature = "alloc")]
pub fn resource_install_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ResourceInstallCommit, sequence, Vec::new())
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
    Frame::request(
        Opcode::TempRunChunk,
        sequence,
        vec![Field::u64(1, offset), Field::bytes(2, bytes)],
    )
}

#[cfg(feature = "alloc")]
pub fn temp_run_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::TempRunCommit, sequence, Vec::new())
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
    for field in &frame.fields {
        match (field.tag, &field.value) {
            (1, FieldValue::String(value)) => target = Some(value.clone()),
            (2, FieldValue::String(value)) => firmware = Some(value.clone()),
            (3, FieldValue::Bool(value)) => diagnostic = Some(*value),
            _ => {}
        }
    }

    Some(HelloIdentity {
        target: target?,
        firmware: firmware?,
        diagnostic: diagnostic.unwrap_or(false),
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
pub fn drawlog_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::DrawlogGet, 1)
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
                (2, FieldValue::U64(number)) => value = Some(*number),
                _ => {}
            }
        }
        values.push((key?, value?));
    }
    Some(values)
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
    out: &mut [u8],
) -> Result<usize, DecodeError> {
    let payload_len = tlv_string_len(target)?
        .checked_add(tlv_string_len(firmware)?)
        .and_then(|len| len.checked_add(tlv_bool_len()))
        .ok_or(DecodeError::OutputTooSmall {
            needed: usize::MAX,
            capacity: out.len(),
        })?;
    encode_response_payload_into(opcode, Status::Ok, sequence, payload_len, out, |payload| {
        let rest = write_string_tlv(payload, 1, target)?;
        let rest = write_string_tlv(rest, 2, firmware)?;
        write_bool_tlv(rest, 3, diagnostic)?;
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
    for metric in metrics.clone() {
        let record_len = tlv_string_len(metric.key)?
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
        Opcode::ResourcesGet,
        Status::Ok,
        sequence,
        payload_len,
        out,
        |mut payload| {
            for metric in metrics {
                let record_len = tlv_string_len(metric.key)?
                    .checked_add(tlv_u64_len())
                    .ok_or(DecodeError::OutputTooSmall {
                        needed: usize::MAX,
                        capacity: payload.len(),
                    })?;
                write_tlv_header(payload, 1, 32, record_len)?;
                let (record, rest) = payload[4..].split_at_mut(record_len);
                let record = write_string_tlv(record, 1, metric.key)?;
                write_u64_tlv(record, 2, metric.value)?;
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
