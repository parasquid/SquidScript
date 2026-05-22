const MAGIC: [u8; 4] = *b"SQDP";
const HEADER_LEN: usize = 20;

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
    StateImport = 72,
    WifiProfileSet = 76,
    Reset = 80,
    StorageFormat = 81,
}

impl Opcode {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub opcode: Opcode,
    pub status: Status,
    pub sequence: u32,
    pub fields: Vec<Field>,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub tag: u8,
    pub value: FieldValue,
}

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloIdentity {
    pub target: String,
    pub firmware: String,
    pub diagnostic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub app_id: String,
    pub sqbc_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: i64,
    pub message: String,
}

pub fn hello_request(sequence: u32) -> Frame {
    Frame::request(Opcode::Hello, sequence, Vec::new())
}

pub fn app_list_request(sequence: u32) -> Frame {
    Frame::request(Opcode::AppList, sequence, Vec::new())
}

pub fn output_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::OutputGet, sequence, Vec::new())
}

pub fn trace_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::TraceGet, sequence, Vec::new())
}

pub fn state_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::StateGet, sequence, Vec::new())
}

pub fn drawlog_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::DrawlogGet, sequence, Vec::new())
}

pub fn errors_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ErrorsGet, sequence, Vec::new())
}

pub fn resources_get_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ResourcesGet, sequence, Vec::new())
}

pub fn reset_request(sequence: u32) -> Frame {
    Frame::request(Opcode::Reset, sequence, Vec::new())
}

pub fn storage_format_request(sequence: u32) -> Frame {
    Frame::request(Opcode::StorageFormat, sequence, Vec::new())
}

pub fn key_request(sequence: u32, key: impl Into<String>) -> Frame {
    Frame::request(Opcode::Key, sequence, vec![Field::string(1, key)])
}

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

pub fn state_import_request(sequence: u32, bytes: Vec<u8>) -> Frame {
    Frame::request(Opcode::StateImport, sequence, vec![Field::bytes(1, bytes)])
}

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

pub fn app_install_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    Frame::request(
        Opcode::AppInstallChunk,
        sequence,
        vec![Field::u64(1, offset), Field::bytes(2, bytes)],
    )
}

pub fn app_install_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::AppInstallCommit, sequence, Vec::new())
}

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

pub fn resource_install_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    Frame::request(
        Opcode::ResourceInstallChunk,
        sequence,
        vec![Field::u64(1, offset), Field::bytes(2, bytes)],
    )
}

pub fn resource_install_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::ResourceInstallCommit, sequence, Vec::new())
}

pub fn app_launch_request(sequence: u32, app_id: impl Into<String>) -> Frame {
    Frame::request(Opcode::AppLaunch, sequence, vec![Field::string(1, app_id)])
}

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

pub fn temp_run_chunk_request(sequence: u32, offset: u64, bytes: Vec<u8>) -> Frame {
    Frame::request(
        Opcode::TempRunChunk,
        sequence,
        vec![Field::u64(1, offset), Field::bytes(2, bytes)],
    )
}

pub fn temp_run_commit_request(sequence: u32) -> Frame {
    Frame::request(Opcode::TempRunCommit, sequence, Vec::new())
}

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

pub fn output_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::OutputGet, 1)
}

pub fn trace_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::TraceGet, 1)
}

pub fn drawlog_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::DrawlogGet, 1)
}

pub fn error_lines(frame: &Frame) -> Option<Vec<String>> {
    repeated_string_fields(frame, Opcode::ErrorsGet, 1)
}

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
        message: message.unwrap_or_else(|| "protocol error".to_string()),
    })
}

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

pub fn encode_frame(frame: &Frame) -> Vec<u8> {
    let mut payload = Vec::new();
    encode_fields(&frame.fields, &mut payload);

    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.push(frame.kind as u8);
    out.push(frame.opcode as u8);
    out.push(frame.status as u8);
    out.push(0);
    out.extend_from_slice(&frame.sequence.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

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

fn encode_fields(fields: &[Field], out: &mut Vec<u8>) {
    for field in fields {
        let (field_type, value) = match &field.value {
            FieldValue::Bytes(value) => (0, value.clone()),
            FieldValue::String(value) => (1, value.as_bytes().to_vec()),
            FieldValue::Bool(value) => (3, vec![u8::from(*value)]),
            FieldValue::I64(value) => (4, value.to_le_bytes().to_vec()),
            FieldValue::U64(value) => (5, value.to_le_bytes().to_vec()),
            FieldValue::Record(fields) => {
                let mut value = Vec::new();
                encode_fields(fields, &mut value);
                (32, value)
            }
        };
        out.push(field.tag);
        out.push(field_type);
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
        out.extend_from_slice(&value);
    }
}

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

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|ch| *ch != '-' && *ch != '_' && *ch != '.')
        .flat_map(char::to_lowercase)
        .collect()
}

fn parse_tag(value: &str) -> Result<u8, String> {
    value
        .parse()
        .map_err(|error| format!("invalid field tag {value:?}: {error}"))
}

fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() % 2 != 0 {
        return Err("hex byte fields must contain an even number of digits".to_string());
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|chunk| {
            let text = std::str::from_utf8(chunk).expect("hex chunks are ascii");
            u8::from_str_radix(text, 16)
                .map_err(|error| format!("invalid hex byte {text}: {error}"))
        })
        .collect()
}
