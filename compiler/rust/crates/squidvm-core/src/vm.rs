use core::{fmt::Write, ptr, slice, str};

use crate::{
    bytecode::{
        BUILTIN_APP_ARM, BUILTIN_APP_ARMED_STACK, BUILTIN_APP_ARMED_STACK_GET, BUILTIN_APP_DISARM,
        BUILTIN_APP_EXIT, BUILTIN_APP_INSTALL, BUILTIN_APP_INSTALL_METADATA, BUILTIN_APP_LAUNCH,
        BUILTIN_APP_PROCESS_STACK, BUILTIN_APP_REGISTRY_GET, BUILTIN_APP_REGISTRY_LIST,
        BUILTIN_BINBOOK_INFO, BUILTIN_BINBOOK_OPEN, BUILTIN_BINBOOK_READ_PAGE,
        BUILTIN_CONTENT_BINBOOK_LIST, BUILTIN_DEBUG_PRINT, BUILTIN_DEVICE_CONFIG_LOAD,
        BUILTIN_DEVICE_CONFIG_REBIND, BUILTIN_DEVICE_CONFIG_SAVE, BUILTIN_DEVICE_CONFIG_SET,
        BUILTIN_DISPLAY_CLEAR, BUILTIN_DISPLAY_DRAW, BUILTIN_DISPLAY_IMAGE, BUILTIN_DISPLAY_INFO,
        BUILTIN_DISPLAY_LINE, BUILTIN_DISPLAY_RECT, BUILTIN_DISPLAY_SELECT, BUILTIN_DISPLAY_TEXT,
        BUILTIN_FILE_COPY, BUILTIN_HARDWARE_GPIO_READ, BUILTIN_HARDWARE_GPIO_TOGGLE,
        BUILTIN_HARDWARE_GPIO_WRITE, BUILTIN_SCREEN_OPEN, BUILTIN_SCREEN_REFRESH,
        BUILTIN_SERVICE_BLE_START, BUILTIN_SERVICE_BLE_STOP, BUILTIN_SERVICE_HTTP_START,
        BUILTIN_SERVICE_HTTP_STOP, BUILTIN_SERVICE_INDICATOR_BLINK,
        BUILTIN_SERVICE_INDICATOR_BREATHE, BUILTIN_SERVICE_INDICATOR_READ,
        BUILTIN_SERVICE_INDICATOR_TOGGLE, BUILTIN_SERVICE_INDICATOR_WRITE,
        BUILTIN_SERVICE_POWER_SLEEP, BUILTIN_SERVICE_TIMER_AFTER, BUILTIN_SERVICE_TIMER_EVERY,
        BUILTIN_SERVICE_WIFI_CANCEL, BUILTIN_SERVICE_WIFI_CONNECT, BUILTIN_SERVICE_WIFI_DISCONNECT,
        BUILTIN_SERVICE_WIFI_GET_AP_IP, BUILTIN_SERVICE_WIFI_OPERATION,
        BUILTIN_SERVICE_WIFI_RESULT, BUILTIN_SERVICE_WIFI_SCAN, BUILTIN_SERVICE_WIFI_SCAN_NETWORK,
        BUILTIN_SERVICE_WIFI_START_AP, BUILTIN_SERVICE_WIFI_STATUS, BUILTIN_SERVICE_WIFI_STOP_AP,
        BUILTIN_STATE_LOAD, BUILTIN_STATE_RESET, BUILTIN_STATE_SAVE, BUILTIN_SYSTEM_MEMORY,
        BUILTIN_SYSTEM_START_REASON, BUILTIN_SYSTEM_STORAGE, OP_ADD, OP_CALL_BUILTIN,
        OP_CALL_FUNCTION, OP_EQ, OP_GET_FIELD, OP_GET_LOCAL, OP_GET_STATE, OP_GT, OP_GTE, OP_HALT,
        OP_JUMP, OP_JUMP_IF_FALSE, OP_LIST_GET, OP_LIST_LEN, OP_LT, OP_LTE, OP_NE, OP_POP,
        OP_PUSH_BOOL, OP_PUSH_INT, OP_PUSH_NULL, OP_PUSH_STRING, OP_RETURN, OP_SET_LOCAL,
        OP_SET_STATE, OP_SUB,
    },
    chunk::{ChunkCache, ChunkKind, ChunkRef},
    error::VmError,
    host::{
        AppArmedStack, AppArmedStackEntry, AppInstallResult, AppProcessStack, AppRegistryEntry,
        AppRegistryList, BinBookInfoResult, BinBookOpenResult, BinBookReadPageResult,
        ContentBinBookEntry, ContentBinBookListResult, DeviceConfigResult, DisplayInfo,
        DisplayLineOptions, DisplayRectOptions, DisplayResourceOptions, DisplayTextOptions,
        FileCopyResult, FilePickFileResult, FileReadLinesResult, FileReadTextResult,
        StorageCompletion, StorageRequest, TraceSink, VmDispatch, WifiAccessPoint, WifiApIp,
        WifiOperation, WifiOperationResult, WifiScanNetwork, WifiStatus,
    },
    limits::{
        MAX_CALL_DEPTH, MAX_CODE_CHUNK_BYTES, MAX_FUNCTIONS, MAX_HANDLERS,
        MAX_INSTRUCTIONS_PER_EVENT, MAX_LOCALS, MAX_PROGRAM_STRING_BYTES, MAX_RUNTIME_LISTS,
        MAX_RUNTIME_LIST_ITEMS, MAX_RUNTIME_RECORDS, MAX_RUNTIME_RECORD_FIELDS,
        MAX_RUNTIME_STRING_BYTES, MAX_SAVED_STATE_BYTES, MAX_SCREENS, MAX_STACK, MAX_STATE,
        MAX_STRINGS, MAX_TRIGGERS,
    },
    program::{Program, ProgramIndex},
    reader::{ChunkedVmHost, SqbcReader},
    state::{
        apply_state_record, concat_value_strings, encode_state_record, state_value_matches,
        values_equal,
    },
    strings::{StringInterner, StringResolver, StringTable},
    value::{Handle, StringRef, Value},
};

pub struct Vm<'a> {
    program: Program<'a>,
    inner: ChunkedVm,
}

pub struct ChunkedVm {
    index: ProgramIndex,
    strings: StringInterner,
    runtime_records: RuntimeRecords,
    runtime_lists: RuntimeLists,
    state: [Value; MAX_STATE],
    stack: [Value; MAX_STACK],
    stack_len: usize,
    current_screen: Option<u16>,
    exited: bool,
    instructions: usize,
    chunk_cache: ChunkCache<4>,
    code: [u8; MAX_CODE_CHUNK_BYTES],
    storage_bytes: [u8; MAX_SAVED_STATE_BYTES],
    code_start: usize,
    code_len: usize,
    frames: [ChunkedResume; MAX_CALL_DEPTH + 1],
    frame_count: usize,
}

#[derive(Clone, Copy)]
pub struct EventPayloadField<'a> {
    pub name: &'static str,
    pub value: &'a str,
}

pub struct EventPayload<'a> {
    pub fields: &'a [EventPayloadField<'a>],
}

#[derive(Clone, Copy)]
struct ChunkedResume {
    kind: ChunkedFrameKind,
    start: usize,
    end: usize,
    ip: usize,
    locals: [Value; MAX_LOCALS],
    depth: usize,
    pending: PendingStorageResume,
}

impl ChunkedResume {
    const fn empty() -> Self {
        Self {
            kind: ChunkedFrameKind::Handler(0),
            start: 0,
            end: 0,
            ip: 0,
            locals: [Value::Null; MAX_LOCALS],
            depth: 0,
            pending: PendingStorageResume::None,
        }
    }
}

#[derive(Clone, Copy)]
enum ChunkedFrameKind {
    Handler(u16),
    Function(u16),
    Screen(u16),
}

impl ChunkedFrameKind {
    const fn chunk_ref(self) -> ChunkRef {
        match self {
            Self::Handler(index) => ChunkRef {
                app: 0,
                kind: ChunkKind::Handler,
                index,
            },
            Self::Function(index) => ChunkRef {
                app: 0,
                kind: ChunkKind::Function,
                index,
            },
            Self::Screen(index) => ChunkRef {
                app: 0,
                kind: ChunkKind::Screen,
                index,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum PendingStorageResume {
    None,
    SqbcRead { offset: usize, len: usize },
    StateLoad,
    StateSave,
    StateReset,
}

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
enum RuntimeFieldName {
    Empty = 0,
    Active,
    ApStartEvents,
    ApStopEvents,
    AppId,
    Auth,
    Available,
    Backend,
    Binding,
    Book,
    Bssid,
    Build,
    BytesReceived,
    BytesWritten,
    Cancelled,
    Channel,
    Clients,
    ColorModel,
    Configured,
    Connected,
    Count,
    DefaultFontHeight,
    Description,
    DisconnectReason,
    DisconnectReasonCode,
    Done,
    Driver,
    DriverMode,
    DriverStarted,
    Drawable,
    Error,
    Event,
    Gw,
    Height,
    HasMore,
    Hidden,
    Id,
    Ip,
    IpAddress,
    Items,
    Kind,
    LastBackendCode,
    Lines,
    Library,
    LogicalGrayLevels,
    Mode,
    Name,
    NativeBpp,
    NativePixelFormat,
    Netmask,
    ObjectName,
    Ok,
    PageCount,
    Path,
    PhysicalHeight,
    PhysicalWidth,
    ProbeEvents,
    Profile,
    Ready,
    Ref,
    Rotation,
    Rssi,
    ScanMatches,
    Size,
    Ssid,
    SsidLength,
    StaConnectedEvents,
    StaDisconnectedEvents,
    State,
    Status,
    SupportsFastRefresh,
    SupportsPartialRefresh,
    Text,
    Title,
    TotalBytes,
    Transport,
    Upload,
    Warning,
    Width,
}

impl RuntimeFieldName {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "active" => Self::Active,
            "apStartEvents" => Self::ApStartEvents,
            "apStopEvents" => Self::ApStopEvents,
            "appId" => Self::AppId,
            "auth" => Self::Auth,
            "available" => Self::Available,
            "backend" => Self::Backend,
            "binding" => Self::Binding,
            "book" => Self::Book,
            "bssid" => Self::Bssid,
            "build" => Self::Build,
            "bytesReceived" => Self::BytesReceived,
            "bytesWritten" => Self::BytesWritten,
            "cancelled" => Self::Cancelled,
            "channel" => Self::Channel,
            "clients" => Self::Clients,
            "colorModel" => Self::ColorModel,
            "configured" => Self::Configured,
            "connected" => Self::Connected,
            "count" => Self::Count,
            "defaultFontHeight" => Self::DefaultFontHeight,
            "description" => Self::Description,
            "disconnectReason" => Self::DisconnectReason,
            "disconnectReasonCode" => Self::DisconnectReasonCode,
            "done" => Self::Done,
            "driver" => Self::Driver,
            "driverMode" => Self::DriverMode,
            "driverStarted" => Self::DriverStarted,
            "drawable" => Self::Drawable,
            "error" => Self::Error,
            "event" => Self::Event,
            "gw" => Self::Gw,
            "height" => Self::Height,
            "hasMore" => Self::HasMore,
            "hidden" => Self::Hidden,
            "id" => Self::Id,
            "ip" => Self::Ip,
            "ipAddress" => Self::IpAddress,
            "items" => Self::Items,
            "kind" => Self::Kind,
            "lastBackendCode" => Self::LastBackendCode,
            "lines" => Self::Lines,
            "library" => Self::Library,
            "logicalGrayLevels" => Self::LogicalGrayLevels,
            "mode" => Self::Mode,
            "name" => Self::Name,
            "nativeBpp" => Self::NativeBpp,
            "nativePixelFormat" => Self::NativePixelFormat,
            "netmask" => Self::Netmask,
            "objectName" => Self::ObjectName,
            "ok" => Self::Ok,
            "pageCount" => Self::PageCount,
            "path" => Self::Path,
            "physicalHeight" => Self::PhysicalHeight,
            "physicalWidth" => Self::PhysicalWidth,
            "probeEvents" => Self::ProbeEvents,
            "profile" => Self::Profile,
            "ready" => Self::Ready,
            "ref" => Self::Ref,
            "rotation" => Self::Rotation,
            "rssi" => Self::Rssi,
            "scanMatches" => Self::ScanMatches,
            "size" => Self::Size,
            "ssid" => Self::Ssid,
            "ssidLength" => Self::SsidLength,
            "staConnectedEvents" => Self::StaConnectedEvents,
            "staDisconnectedEvents" => Self::StaDisconnectedEvents,
            "state" => Self::State,
            "status" => Self::Status,
            "supportsFastRefresh" => Self::SupportsFastRefresh,
            "supportsPartialRefresh" => Self::SupportsPartialRefresh,
            "text" => Self::Text,
            "title" => Self::Title,
            "totalBytes" => Self::TotalBytes,
            "transport" => Self::Transport,
            "upload" => Self::Upload,
            "warning" => Self::Warning,
            "width" => Self::Width,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy)]
struct RuntimeRecordField {
    name: RuntimeFieldName,
    value: Value,
}

impl RuntimeRecordField {
    const fn empty() -> Self {
        Self {
            name: RuntimeFieldName::Empty,
            value: Value::Null,
        }
    }

    const fn new(name: RuntimeFieldName, value: Value) -> Self {
        Self { name, value }
    }
}

#[derive(Clone, Copy)]
struct RuntimeRecord {
    fields: [RuntimeRecordField; MAX_RUNTIME_RECORD_FIELDS],
    field_count: usize,
}

impl RuntimeRecord {
    const fn empty() -> Self {
        Self {
            fields: [RuntimeRecordField::empty(); MAX_RUNTIME_RECORD_FIELDS],
            field_count: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct RuntimeList {
    items: [Value; MAX_RUNTIME_LIST_ITEMS],
    item_count: usize,
}

impl RuntimeList {
    const fn empty() -> Self {
        Self {
            items: [Value::Null; MAX_RUNTIME_LIST_ITEMS],
            item_count: 0,
        }
    }
}

struct RuntimeRecords {
    records: [RuntimeRecord; MAX_RUNTIME_RECORDS],
    next: usize,
}

#[cfg(test)]
mod runtime_record_layout_tests {
    use core::mem::size_of;

    use super::{RuntimeRecordField, Value};

    #[test]
    fn runtime_record_fields_store_compact_field_symbols() {
        assert!(
            size_of::<RuntimeRecordField>() <= size_of::<Value>() + 4,
            "RuntimeRecordField stores too much per field: field={} value={}",
            size_of::<RuntimeRecordField>(),
            size_of::<Value>()
        );
    }
}

impl RuntimeRecords {
    const fn new() -> Self {
        Self {
            records: [RuntimeRecord::empty(); MAX_RUNTIME_RECORDS],
            next: 0,
        }
    }

    fn alloc(&mut self, fields: &[RuntimeRecordField]) -> Result<Value, VmError> {
        if fields.len() > MAX_RUNTIME_RECORD_FIELDS {
            return Err(VmError::InvalidOperand);
        }
        let id = self.next;
        self.next = (self.next + 1) % MAX_RUNTIME_RECORDS;
        let mut record = RuntimeRecord::empty();
        record.field_count = fields.len();
        for (index, field) in fields.iter().enumerate() {
            record.fields[index] = *field;
        }
        self.records[id] = record;
        Ok(Value::Record(id as u8))
    }

    fn field(&self, record_id: u8, field_name: &str) -> Result<Value, VmError> {
        let field_name = RuntimeFieldName::parse(field_name).ok_or(VmError::InvalidOperand)?;
        let record = self
            .records
            .get(record_id as usize)
            .ok_or(VmError::InvalidOperand)?;
        for field in record.fields.iter().take(record.field_count) {
            if field.name == field_name {
                return Ok(field.value);
            }
        }
        Err(VmError::InvalidOperand)
    }

    fn reset(&mut self) {
        self.next = 0;
    }
}

struct RuntimeLists {
    lists: [RuntimeList; MAX_RUNTIME_LISTS],
    next: usize,
}

struct FixedString {
    bytes: [u8; MAX_RUNTIME_STRING_BYTES],
    len: usize,
}

impl FixedString {
    const fn new() -> Self {
        Self {
            bytes: [0; MAX_RUNTIME_STRING_BYTES],
            len: 0,
        }
    }

    fn as_str(&self) -> Result<&str, VmError> {
        str::from_utf8(&self.bytes[..self.len]).map_err(|_| VmError::InvalidUtf8)
    }
}

impl Write for FixedString {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(core::fmt::Error)?;
        if end > self.bytes.len() {
            return Err(core::fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl RuntimeLists {
    const fn new() -> Self {
        Self {
            lists: [RuntimeList::empty(); MAX_RUNTIME_LISTS],
            next: 0,
        }
    }

    fn alloc(&mut self, items: &[Value]) -> Result<Value, VmError> {
        if items.len() > MAX_RUNTIME_LIST_ITEMS {
            return Err(VmError::InvalidOperand);
        }
        let id = self.next;
        self.next = (self.next + 1) % MAX_RUNTIME_LISTS;
        let mut list = RuntimeList::empty();
        list.item_count = items.len();
        list.items[..items.len()].copy_from_slice(items);
        self.lists[id] = list;
        Ok(Value::List(id as u8))
    }

    fn len(&self, list_id: u8) -> Result<i32, VmError> {
        let list = self
            .lists
            .get(list_id as usize)
            .ok_or(VmError::InvalidOperand)?;
        Ok(list.item_count.min(i32::MAX as usize) as i32)
    }

    fn get(&self, list_id: u8, index: i32) -> Result<Value, VmError> {
        let list = self
            .lists
            .get(list_id as usize)
            .ok_or(VmError::InvalidOperand)?;
        if index < 0 {
            return Err(VmError::InvalidOperand);
        }
        let index = index as usize;
        if index >= list.item_count {
            return Err(VmError::InvalidOperand);
        }
        Ok(list.items[index])
    }

    fn reset(&mut self) {
        self.next = 0;
    }
}

impl ChunkedVm {
    pub fn new(index: ProgramIndex) -> Self {
        let mut state = [Value::Null; MAX_STATE];
        for (slot_index, slot) in index.state_slots.iter().take(index.state_count).enumerate() {
            state[slot_index] = slot.default;
        }
        Self {
            index,
            strings: StringInterner::new(),
            runtime_records: RuntimeRecords::new(),
            runtime_lists: RuntimeLists::new(),
            state,
            stack: [Value::Null; MAX_STACK],
            stack_len: 0,
            current_screen: None,
            exited: false,
            instructions: 0,
            chunk_cache: ChunkCache::new(),
            code: [0; MAX_CODE_CHUNK_BYTES],
            storage_bytes: [0; MAX_SAVED_STATE_BYTES],
            code_start: usize::MAX,
            code_len: 0,
            frames: [ChunkedResume::empty(); MAX_CALL_DEPTH + 1],
            frame_count: 0,
        }
    }

    /// Writes a freshly initialized VM directly into caller-owned storage.
    ///
    /// This avoids constructing the full VM as a transient stack temporary in
    /// C FFI callers on small firmware stacks.
    pub unsafe fn init_in_place(out: *mut Self, index: &ProgramIndex) {
        init_program_index_in_place(ptr::addr_of_mut!((*out).index), index);
        Self::init_after_index_in_place(out);
    }

    pub unsafe fn init_in_place_from_reader(
        out: *mut Self,
        reader: &mut impl SqbcReader,
        scratch: &mut [u8],
    ) -> Result<(), VmError> {
        ProgramIndex::parse_from_reader_in_place(ptr::addr_of_mut!((*out).index), reader, scratch)?;
        Self::init_after_index_in_place(out);
        Ok(())
    }

    unsafe fn init_after_index_in_place(out: *mut Self) {
        ptr::addr_of_mut!((*out).strings).write(StringInterner::new());
        init_runtime_records_in_place(ptr::addr_of_mut!((*out).runtime_records));
        init_runtime_lists_in_place(ptr::addr_of_mut!((*out).runtime_lists));

        let state = ptr::addr_of_mut!((*out).state).cast::<Value>();
        for slot_index in 0..MAX_STATE {
            let value = (*out)
                .index
                .state_slots
                .get(slot_index)
                .filter(|_| slot_index < (*out).index.state_count)
                .map_or(Value::Null, |slot| slot.default);
            state.add(slot_index).write(value);
        }
        let stack = ptr::addr_of_mut!((*out).stack).cast::<Value>();
        for stack_index in 0..MAX_STACK {
            stack.add(stack_index).write(Value::Null);
        }
        ptr::addr_of_mut!((*out).stack_len).write(0);
        ptr::addr_of_mut!((*out).current_screen).write(None);
        ptr::addr_of_mut!((*out).exited).write(false);
        ptr::addr_of_mut!((*out).instructions).write(0);
        ptr::addr_of_mut!((*out).chunk_cache).write(ChunkCache::new());
        ptr::write_bytes(
            ptr::addr_of_mut!((*out).code).cast::<u8>(),
            0,
            MAX_CODE_CHUNK_BYTES,
        );
        ptr::write_bytes(
            ptr::addr_of_mut!((*out).storage_bytes).cast::<u8>(),
            0,
            MAX_SAVED_STATE_BYTES,
        );
        ptr::addr_of_mut!((*out).code_start).write(usize::MAX);
        ptr::addr_of_mut!((*out).code_len).write(0);
        let frames = ptr::addr_of_mut!((*out).frames).cast::<ChunkedResume>();
        for frame_index in 0..(MAX_CALL_DEPTH + 1) {
            frames.add(frame_index).write(ChunkedResume::empty());
        }
        ptr::addr_of_mut!((*out).frame_count).write(0);
    }

    pub fn dispatch(&mut self, host: &mut impl ChunkedVmHost, event: &str) -> Result<(), VmError> {
        let result = (|| {
            let mut dispatch = self.dispatch_resumable(host, event)?;
            loop {
                match dispatch {
                    VmDispatch::Complete => return Ok(()),
                    VmDispatch::PendingStorage(request) => {
                        dispatch = self.resume_immediate_storage(host, request)?;
                    }
                }
            }
        })();
        if result.is_err() {
            host.service_wifi_teardown()?;
        }
        result
    }

    pub fn dispatch_resumable(
        &mut self,
        host: &mut impl ChunkedVmHost,
        event: &str,
    ) -> Result<VmDispatch, VmError> {
        self.dispatch_resumable_with_payload(host, event, None)
    }

    pub fn dispatch_resumable_with_payload(
        &mut self,
        host: &mut impl ChunkedVmHost,
        event: &str,
        payload: Option<EventPayload<'_>>,
    ) -> Result<VmDispatch, VmError> {
        if self.exited {
            return Ok(VmDispatch::Complete);
        }
        let (index, handler) = self.index.handler(event)?;
        if handler.local_count as usize > MAX_LOCALS || handler.param_count > handler.local_count {
            return Err(VmError::LocalOutOfBounds);
        }
        self.strings
            .retain_state_values(&self.index, &mut self.state[..self.index.state_count])?;
        self.runtime_records.reset();
        self.runtime_lists.reset();
        let key = ChunkRef {
            app: 0,
            kind: ChunkKind::Handler,
            index: index as u16,
        };
        self.chunk_cache.insert(key, handler.preload).ok();
        self.chunk_cache.begin_execute(key).ok();
        host.trace(event);
        self.instructions = 0;
        self.frame_count = 0;
        let frame_index = self.push_resume_frame(
            ChunkedFrameKind::Handler(index as u16),
            handler.start,
            handler.len,
            0,
        )?;
        if handler.param_count > 1 {
            return Err(VmError::LocalOutOfBounds);
        }
        if handler.param_count == 1 {
            let payload = payload
                .map(|payload| self.event_payload_record(payload))
                .transpose()?
                .unwrap_or(Value::Null);
            self.frames[frame_index].locals[0] = payload;
        }
        self.execute_resume_frames(host)
    }

    fn event_payload_record(&mut self, payload: EventPayload<'_>) -> Result<Value, VmError> {
        if payload.fields.len() > MAX_RUNTIME_RECORD_FIELDS {
            return Err(VmError::InvalidOperand);
        }
        let mut fields = [RuntimeRecordField::empty(); MAX_RUNTIME_RECORD_FIELDS];
        for (index, field) in payload.fields.iter().enumerate() {
            let value = self.strings.intern_runtime(&self.index, field.value)?;
            let name = RuntimeFieldName::parse(field.name).ok_or(VmError::InvalidOperand)?;
            fields[index] = RuntimeRecordField::new(name, value);
        }
        self.runtime_records.alloc(&fields[..payload.fields.len()])
    }

    pub fn resume_storage(
        &mut self,
        host: &mut impl ChunkedVmHost,
        completion: StorageCompletion<'_>,
    ) -> Result<VmDispatch, VmError> {
        if self.frame_count == 0 {
            return Ok(VmDispatch::Complete);
        };
        let frame_index = self.frame_count - 1;
        let pending = self.frames[frame_index].pending;
        match pending {
            PendingStorageResume::SqbcRead { offset, len } => {
                if offset != self.index.code_offset + self.frames[frame_index].start {
                    return Err(VmError::ReadFailed);
                }
                let relative_len = len.min(self.code.len());
                self.code[..relative_len].copy_from_slice(&completion.bytes[..relative_len]);
                self.code_start = self.frames[frame_index].start;
                self.code_len = relative_len;
            }
            PendingStorageResume::StateLoad => {
                if let Some(len) = completion.len {
                    apply_state_record(
                        &completion.bytes[..len],
                        &self.index,
                        &self.index.state_slots[..self.index.state_count],
                        &mut self.strings,
                        &mut self.state[..self.index.state_count],
                    )?;
                }
                host.trace("state.load");
            }
            PendingStorageResume::StateSave => {
                host.trace("state.save");
            }
            PendingStorageResume::StateReset => {
                host.trace("state.reset");
            }
            PendingStorageResume::None => {}
        }
        self.frames[frame_index].pending = PendingStorageResume::None;
        self.execute_resume_frames(host)
    }

    fn resume_immediate_storage(
        &mut self,
        host: &mut impl ChunkedVmHost,
        request: StorageRequest,
    ) -> Result<VmDispatch, VmError> {
        if self.frame_count == 0 {
            return Ok(VmDispatch::Complete);
        }
        let frame_index = self.frame_count - 1;
        match request {
            StorageRequest::SqbcRead { offset, len } => {
                if offset != self.index.code_offset + self.frames[frame_index].start
                    || len > self.code.len()
                {
                    return Err(VmError::ReadFailed);
                }
                host.read_exact_at(offset, &mut self.code[..len])?;
                self.code_start = self.frames[frame_index].start;
                self.code_len = len;
            }
            StorageRequest::StateLoad => {
                if let Some(len) = host.state_load(&mut self.storage_bytes)? {
                    apply_state_record(
                        &self.storage_bytes[..len],
                        &self.index,
                        &self.index.state_slots[..self.index.state_count],
                        &mut self.strings,
                        &mut self.state[..self.index.state_count],
                    )?;
                }
                host.trace("state.load");
            }
            StorageRequest::StateSave { len, bytes } => {
                let bytes = unsafe { slice::from_raw_parts(bytes, len) };
                host.state_save(bytes)?;
                host.trace("state.save");
            }
            StorageRequest::StateReset => {
                self.reset_state();
                host.state_reset_persistent()?;
                host.trace("state.reset");
            }
        }
        self.frames[frame_index].pending = PendingStorageResume::None;
        self.execute_resume_frames(host)
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn current_screen(&self) -> Result<Option<&str>, VmError> {
        self.current_screen
            .map(|id| self.index.string(id))
            .transpose()
    }

    pub fn state_value(&self, name: &str) -> Result<Value, VmError> {
        for (index, slot) in self
            .index
            .state_slots
            .iter()
            .take(self.index.state_count)
            .enumerate()
        {
            if self.index.string(slot.name_id)? == name {
                return Ok(self.state[index]);
            }
        }
        Err(VmError::StateOutOfBounds)
    }

    pub fn state_count(&self) -> usize {
        self.index.state_count
    }

    pub fn state_name(&self, index: usize) -> Result<&str, VmError> {
        if index >= self.index.state_count {
            return Err(VmError::StateOutOfBounds);
        }
        self.index.string(self.index.state_slots[index].name_id)
    }

    pub fn state_at(&self, index: usize) -> Result<Value, VmError> {
        if index >= self.index.state_count {
            return Err(VmError::StateOutOfBounds);
        }
        Ok(self.state[index])
    }

    pub fn set_state_value(&mut self, name: &str, value: Value) -> Result<(), VmError> {
        let value = self.materialize_state_value(value)?;
        for (index, slot) in self
            .index
            .state_slots
            .iter()
            .take(self.index.state_count)
            .enumerate()
        {
            if self.index.string(slot.name_id)? == name {
                if !state_value_matches(slot.value_type.tag, slot.value_type.nullable, value) {
                    return Err(VmError::InvalidOperand);
                }
                self.state[index] = value;
                return Ok(());
            }
        }
        Err(VmError::StateOutOfBounds)
    }

    pub fn string_resolver(&self) -> StringResolver<'_> {
        self.resolver()
    }

    pub fn string_table(&self) -> &dyn StringTable {
        &self.index
    }

    fn resolver(&self) -> StringResolver<'_> {
        StringResolver::new(&self.index, &self.strings)
    }

    fn materialize_state_value(&mut self, value: Value) -> Result<Value, VmError> {
        self.strings.retain_value(value)
    }

    pub const fn installed_code_cache_bytes(&self) -> usize {
        MAX_CODE_CHUNK_BYTES
    }

    fn reset_state(&mut self) {
        for (slot_index, slot) in self
            .index
            .state_slots
            .iter()
            .take(self.index.state_count)
            .enumerate()
        {
            self.state[slot_index] = slot.default;
        }
    }

    fn load_chunk_resumable(
        &mut self,
        reader: &mut impl SqbcReader,
        start: usize,
        len: usize,
    ) -> Result<Option<StorageRequest>, VmError> {
        let end = start.checked_add(len).ok_or(VmError::InvalidJump)?;
        if end > self.index.code_len {
            return Err(VmError::InvalidJump);
        }
        if len > self.code.len() {
            return Err(VmError::ChunkTooLarge);
        }
        if self.code_start == start && self.code_len == len {
            return Ok(None);
        }
        let offset = self.index.code_offset + start;
        if reader.should_defer_read(offset, len)? {
            return Ok(Some(StorageRequest::sqbc_read(offset, len)));
        }
        reader.read_exact_at(offset, &mut self.code[..len])?;
        self.code_start = start;
        self.code_len = len;
        Ok(None)
    }

    fn pop_optional_string(&mut self) -> Result<Option<u16>, VmError> {
        match self.pop()? {
            Value::Null => Ok(None),
            Value::String(StringRef::Sqbc(id)) => Ok(Some(id)),
            _ => Err(VmError::InvalidOperand),
        }
    }

    fn pop_sqbc_string_id(&mut self) -> Result<u16, VmError> {
        match self.pop()? {
            Value::String(StringRef::Sqbc(id)) => Ok(id),
            _ => Err(VmError::InvalidOperand),
        }
    }

    fn pop_handle(&mut self) -> Result<Handle, VmError> {
        match self.pop()? {
            Value::Handle(handle) => Ok(handle),
            _ => Err(VmError::InvalidOperand),
        }
    }

    fn code_byte(&self, offset: usize) -> Result<u8, VmError> {
        if offset < self.code_start {
            return Err(VmError::InvalidJump);
        }
        let relative = offset - self.code_start;
        if relative >= self.code_len {
            return Err(VmError::InvalidJump);
        }
        Ok(self.code[relative])
    }

    fn read_u16_code(&self, offset: usize) -> Result<u16, VmError> {
        Ok(u16::from_le_bytes([
            self.code_byte(offset)?,
            self.code_byte(offset + 1)?,
        ]))
    }

    fn read_u32_code(&self, offset: usize) -> Result<u32, VmError> {
        Ok(u32::from_le_bytes([
            self.code_byte(offset)?,
            self.code_byte(offset + 1)?,
            self.code_byte(offset + 2)?,
            self.code_byte(offset + 3)?,
        ]))
    }

    fn read_i32_code(&self, offset: usize) -> Result<i32, VmError> {
        Ok(i32::from_le_bytes([
            self.code_byte(offset)?,
            self.code_byte(offset + 1)?,
            self.code_byte(offset + 2)?,
            self.code_byte(offset + 3)?,
        ]))
    }

    fn push_resume_frame(
        &mut self,
        kind: ChunkedFrameKind,
        start: usize,
        len: usize,
        depth: usize,
    ) -> Result<usize, VmError> {
        if depth > MAX_CALL_DEPTH {
            return Err(VmError::CallDepthExceeded);
        }
        if self.frame_count >= self.frames.len() {
            return Err(VmError::CallDepthExceeded);
        }
        let end = start.checked_add(len).ok_or(VmError::InvalidJump)?;
        if end > self.index.code_len {
            return Err(VmError::InvalidJump);
        }
        let frame_index = self.frame_count;
        self.frames[frame_index] = ChunkedResume {
            kind,
            start,
            end,
            ip: start,
            locals: [Value::Null; MAX_LOCALS],
            depth,
            pending: PendingStorageResume::None,
        };
        self.frame_count += 1;
        Ok(frame_index)
    }

    fn push_screen_resume_frame(&mut self, screen_id: u16, depth: usize) -> Result<(), VmError> {
        let (screen_index, screen) = self.index.screen(self.index.string(screen_id)?)?;
        let key = ChunkRef {
            app: 0,
            kind: ChunkKind::Screen,
            index: screen_index as u16,
        };
        self.chunk_cache.insert(key, false).ok();
        self.chunk_cache.begin_execute(key).ok();
        self.push_resume_frame(
            ChunkedFrameKind::Screen(screen_index as u16),
            screen.start,
            screen.len,
            depth,
        )?;
        Ok(())
    }

    fn complete_resume_frame(&mut self, return_value: Option<Value>) -> Result<(), VmError> {
        if self.frame_count == 0 {
            return Ok(());
        }
        let frame_index = self.frame_count - 1;
        let kind = self.frames[frame_index].kind;
        self.chunk_cache.end_execute(kind.chunk_ref()).ok();
        self.frames[frame_index] = ChunkedResume::empty();
        self.frame_count -= 1;
        if self.frame_count > 0 && matches!(kind, ChunkedFrameKind::Function(_)) {
            self.push(return_value.unwrap_or(Value::Null))?;
        }
        Ok(())
    }

    fn execute_resume_frames(
        &mut self,
        host: &mut impl ChunkedVmHost,
    ) -> Result<VmDispatch, VmError> {
        loop {
            if self.frame_count == 0 {
                return Ok(VmDispatch::Complete);
            }
            let frame_index = self.frame_count - 1;
            let frame_start = self.frames[frame_index].start;
            let frame_end = self.frames[frame_index].end;
            if self.frames[frame_index].ip >= frame_end {
                self.complete_resume_frame(None)?;
                if self.frame_count > 0 {
                    continue;
                }
                return Ok(VmDispatch::Complete);
            }
            if let Some(request) =
                self.load_chunk_resumable(host, frame_start, frame_end - frame_start)?
            {
                let StorageRequest::SqbcRead { offset, len } = request else {
                    return Err(VmError::ReadFailed);
                };
                self.frames[frame_index].pending = PendingStorageResume::SqbcRead { offset, len };
                return Ok(VmDispatch::PendingStorage(request));
            }
            self.instructions += 1;
            if self.instructions > MAX_INSTRUCTIONS_PER_EVENT {
                return Err(VmError::InstructionBudgetExceeded);
            }
            let mut ip = self.frames[frame_index].ip;
            let op = self.code_byte(ip)?;
            ip += 1;
            self.frames[frame_index].ip = ip;
            match op {
                OP_PUSH_INT => {
                    let value = self.read_i32_code(self.frames[frame_index].ip)?;
                    self.frames[frame_index].ip += 4;
                    self.push(Value::I32(value))?;
                }
                OP_PUSH_BOOL => {
                    let value = self.code_byte(self.frames[frame_index].ip)? != 0;
                    self.frames[frame_index].ip += 1;
                    self.push(Value::Bool(value))?;
                }
                OP_PUSH_STRING => {
                    let value = self.read_u16_code(self.frames[frame_index].ip)?;
                    self.frames[frame_index].ip += 2;
                    self.push(Value::String(StringRef::Sqbc(value)))?;
                }
                OP_PUSH_NULL => self.push(Value::Null)?,
                OP_GET_STATE => {
                    let state = self.read_u16_code(self.frames[frame_index].ip)? as usize;
                    self.frames[frame_index].ip += 2;
                    self.push(*self.state.get(state).ok_or(VmError::StateOutOfBounds)?)?;
                }
                OP_SET_STATE => {
                    let state = self.read_u16_code(self.frames[frame_index].ip)? as usize;
                    self.frames[frame_index].ip += 2;
                    let value = self.pop()?;
                    let value = self.materialize_state_value(value)?;
                    let state_slot = self
                        .index
                        .state_slots
                        .get(state)
                        .ok_or(VmError::StateOutOfBounds)?;
                    if state >= self.index.state_count
                        || !state_value_matches(
                            state_slot.value_type.tag,
                            state_slot.value_type.nullable,
                            value,
                        )
                    {
                        return Err(VmError::InvalidOperand);
                    }
                    let slot = self.state.get_mut(state).ok_or(VmError::StateOutOfBounds)?;
                    *slot = value;
                }
                OP_GET_LOCAL => {
                    let local = self.read_u16_code(self.frames[frame_index].ip)? as usize;
                    self.frames[frame_index].ip += 2;
                    let value = *self.frames[frame_index]
                        .locals
                        .get(local)
                        .ok_or(VmError::LocalOutOfBounds)?;
                    self.push(value)?;
                }
                OP_SET_LOCAL => {
                    let local = self.read_u16_code(self.frames[frame_index].ip)? as usize;
                    self.frames[frame_index].ip += 2;
                    let value = self.pop()?;
                    let slot = self.frames[frame_index]
                        .locals
                        .get_mut(local)
                        .ok_or(VmError::LocalOutOfBounds)?;
                    *slot = value;
                }
                OP_GET_FIELD => {
                    let field_id = self.read_u16_code(self.frames[frame_index].ip)?;
                    self.frames[frame_index].ip += 2;
                    let target = self.pop()?;
                    let field = self.index.string(field_id)?;
                    let value = match target {
                        Value::Record(record_id) => self.runtime_records.field(record_id, field)?,
                        _ => return Err(VmError::InvalidOperand),
                    };
                    self.push(value)?;
                }
                OP_ADD | OP_SUB | OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE => {
                    self.binary(op)?
                }
                OP_LIST_LEN => {
                    let Value::List(list_id) = self.pop()? else {
                        return Err(VmError::InvalidOperand);
                    };
                    self.push(Value::I32(self.runtime_lists.len(list_id)?))?;
                }
                OP_LIST_GET => {
                    let index = self.pop()?.expect_i32()?;
                    let Value::List(list_id) = self.pop()? else {
                        return Err(VmError::InvalidOperand);
                    };
                    self.push(self.runtime_lists.get(list_id, index)?)?;
                }
                OP_JUMP => {
                    let target = self.read_u32_code(self.frames[frame_index].ip)? as usize;
                    if target > self.frames[frame_index].end {
                        return Err(VmError::InvalidJump);
                    }
                    self.frames[frame_index].ip = target;
                }
                OP_JUMP_IF_FALSE => {
                    let target = self.read_u32_code(self.frames[frame_index].ip)? as usize;
                    self.frames[frame_index].ip += 4;
                    if !self.pop()?.truthy() {
                        if target > self.frames[frame_index].end {
                            return Err(VmError::InvalidJump);
                        }
                        self.frames[frame_index].ip = target;
                    }
                }
                OP_CALL_BUILTIN => {
                    let builtin = self.code_byte(self.frames[frame_index].ip)?;
                    self.frames[frame_index].ip += 1;
                    let arg_count = if builtin == BUILTIN_DEBUG_PRINT {
                        let count = self.code_byte(self.frames[frame_index].ip)?;
                        self.frames[frame_index].ip += 1;
                        count
                    } else {
                        0
                    };
                    if builtin == BUILTIN_SCREEN_OPEN {
                        let name_id = self.pop_sqbc_string_id()?;
                        self.current_screen = Some(name_id);
                        let depth = self.frames[frame_index].depth;
                        self.push_screen_resume_frame(name_id, depth + 1)?;
                        continue;
                    }
                    if builtin == BUILTIN_SCREEN_REFRESH {
                        let screen_id = self.current_screen.ok_or(VmError::InvalidOperand)?;
                        let depth = self.frames[frame_index].depth;
                        self.push_screen_resume_frame(screen_id, depth + 1)?;
                        continue;
                    }
                    let depth = self.frames[frame_index].depth;
                    if let Some(request) =
                        self.call_builtin_resumable(host, builtin, arg_count, depth)?
                    {
                        self.frames[frame_index].pending = match request {
                            StorageRequest::StateLoad => PendingStorageResume::StateLoad,
                            StorageRequest::StateSave { .. } => PendingStorageResume::StateSave,
                            StorageRequest::StateReset => PendingStorageResume::StateReset,
                            StorageRequest::SqbcRead { .. } => PendingStorageResume::None,
                        };
                        return Ok(VmDispatch::PendingStorage(request));
                    }
                }
                OP_CALL_FUNCTION => {
                    let function_id = self.read_u16_code(self.frames[frame_index].ip)? as usize;
                    self.frames[frame_index].ip += 2;
                    let arg_count = self.read_u16_code(self.frames[frame_index].ip)? as usize;
                    self.frames[frame_index].ip += 2;
                    let function = *self
                        .index
                        .functions
                        .get(function_id)
                        .ok_or(VmError::FunctionOutOfBounds)?;
                    if function_id >= self.index.function_count
                        || arg_count != function.param_count as usize
                    {
                        return Err(VmError::FunctionOutOfBounds);
                    }
                    if function.local_count as usize > MAX_LOCALS {
                        return Err(VmError::LocalOutOfBounds);
                    }
                    if arg_count > self.stack_len {
                        return Err(VmError::StackUnderflow);
                    }
                    let key = ChunkRef {
                        app: 0,
                        kind: ChunkKind::Function,
                        index: function_id as u16,
                    };
                    self.chunk_cache.insert(key, false).ok();
                    self.chunk_cache.begin_execute(key).ok();
                    let child_frame_index = self.push_resume_frame(
                        ChunkedFrameKind::Function(function_id as u16),
                        function.start,
                        function.len,
                        self.frames[frame_index].depth + 1,
                    )?;
                    for index in (0..arg_count).rev() {
                        let value = self.pop()?;
                        self.frames[child_frame_index].locals[index] = value;
                    }
                }
                OP_RETURN => {
                    let value = self.pop()?;
                    self.complete_resume_frame(Some(value))?;
                    if self.frame_count > 0 {
                        continue;
                    }
                    return Ok(VmDispatch::Complete);
                }
                OP_HALT => {
                    self.complete_resume_frame(None)?;
                    if self.frame_count > 0 {
                        continue;
                    }
                    return Ok(VmDispatch::Complete);
                }
                OP_POP => {
                    let _ = self.pop()?;
                }
                _ => return Err(VmError::UnknownOpcode),
            }
        }
    }

    fn call_builtin_resumable(
        &mut self,
        host: &mut impl ChunkedVmHost,
        builtin: u8,
        arg_count: u8,
        depth: usize,
    ) -> Result<Option<StorageRequest>, VmError> {
        match builtin {
            BUILTIN_STATE_LOAD => Ok(Some(StorageRequest::state_load())),
            BUILTIN_STATE_SAVE => {
                let len = encode_state_record(
                    &self.index,
                    &self.strings,
                    &self.index.state_slots[..self.index.state_count],
                    &self.state[..self.index.state_count],
                    &mut self.storage_bytes,
                )?;
                Ok(Some(StorageRequest::state_save(
                    &self.storage_bytes[..len],
                )?))
            }
            BUILTIN_STATE_RESET => {
                self.reset_state();
                Ok(Some(StorageRequest::state_reset()))
            }
            _ => {
                self.call_builtin(host, builtin, arg_count, depth)?;
                Ok(None)
            }
        }
    }

    fn call_builtin(
        &mut self,
        host: &mut impl ChunkedVmHost,
        builtin: u8,
        arg_count: u8,
        _depth: usize,
    ) -> Result<(), VmError> {
        match builtin {
            BUILTIN_STATE_LOAD
            | BUILTIN_STATE_SAVE
            | BUILTIN_STATE_RESET
            | BUILTIN_SCREEN_OPEN
            | BUILTIN_SCREEN_REFRESH => return Err(VmError::InvalidOperand),
            BUILTIN_APP_EXIT => {
                host.service_wifi_teardown()?;
                self.exited = true;
                host.trace("app.exit");
            }
            BUILTIN_DEBUG_PRINT => {
                let count = arg_count as usize;
                if count > self.stack_len {
                    return Err(VmError::StackUnderflow);
                }
                let start = self.stack_len - count;
                let strings = self.resolver();
                host.debug_print(&strings, &self.stack[start..self.stack_len]);
                self.stack_len = start;
            }
            BUILTIN_DISPLAY_CLEAR => {
                let color_id = self.pop_sqbc_string_id()?;
                host.draw_clear(self.index.string(color_id)?);
            }
            BUILTIN_DISPLAY_TEXT => {
                let valign_id = self.pop_optional_string()?;
                let align_id = self.pop_optional_string()?;
                let background_color_id = self.pop_optional_string()?;
                let text_color_id = self.pop_optional_string()?;
                let font_height = self.pop()?.expect_i32()?;
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                let text = self.pop()?;
                let strings = self.resolver();
                host.draw_text(
                    &strings,
                    text,
                    DisplayTextOptions {
                        x,
                        y,
                        w,
                        h,
                        font_height,
                        text_color: text_color_id.map(|id| self.index.string(id)).transpose()?,
                        background_color: background_color_id
                            .map(|id| self.index.string(id))
                            .transpose()?,
                        align: align_id.map(|id| self.index.string(id)).transpose()?,
                        valign: valign_id.map(|id| self.index.string(id)).transpose()?,
                    },
                );
            }
            BUILTIN_DISPLAY_RECT => {
                let stroke_color_id = self.pop_optional_string()?;
                let fill_color_id = self.pop_optional_string()?;
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                host.draw_rect(DisplayRectOptions {
                    x,
                    y,
                    w,
                    h,
                    fill_color: fill_color_id.map(|id| self.index.string(id)).transpose()?,
                    stroke_color: stroke_color_id
                        .map(|id| self.index.string(id))
                        .transpose()?,
                });
            }
            BUILTIN_DISPLAY_LINE => {
                let color_id = self.pop_optional_string()?;
                let y2 = self.pop()?.expect_i32()?;
                let x2 = self.pop()?.expect_i32()?;
                let y1 = self.pop()?.expect_i32()?;
                let x1 = self.pop()?.expect_i32()?;
                host.draw_line(DisplayLineOptions {
                    x1,
                    y1,
                    x2,
                    y2,
                    color: color_id.map(|id| self.index.string(id)).transpose()?,
                });
            }
            BUILTIN_DISPLAY_SELECT => {
                let name_id = self.pop_sqbc_string_id()?;
                host.draw_select(self.index.string(name_id)?)?;
            }
            BUILTIN_DISPLAY_IMAGE => {
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                let path_id = self.pop_sqbc_string_id()?;
                host.draw_image(
                    self.index.string(path_id)?,
                    DisplayResourceOptions { x, y, w, h },
                );
            }
            BUILTIN_DISPLAY_DRAW => {
                let h = self.pop()?.expect_i32()?;
                let w = self.pop()?.expect_i32()?;
                let y = self.pop()?.expect_i32()?;
                let x = self.pop()?.expect_i32()?;
                let drawable = self.pop()?;
                let strings = self.resolver();
                host.draw_resource(&strings, drawable, DisplayResourceOptions { x, y, w, h });
            }
            BUILTIN_DISPLAY_INFO => {
                let result = host.display_info()?;
                let record = self.display_info_record(result)?;
                self.push(record)?;
            }
            BUILTIN_HARDWARE_GPIO_WRITE => {
                let name_id = self.pop_sqbc_string_id()?;
                let Value::Bool(value) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.hardware_gpio_write(self.index.string(name_id)?, value)?;
            }
            BUILTIN_HARDWARE_GPIO_TOGGLE => {
                let name_id = self.pop_sqbc_string_id()?;
                host.hardware_gpio_toggle(self.index.string(name_id)?)?;
            }
            BUILTIN_HARDWARE_GPIO_READ => {
                let name_id = self.pop_sqbc_string_id()?;
                let value = host.hardware_gpio_read(self.index.string(name_id)?)?;
                self.push(Value::Bool(value))?;
            }
            BUILTIN_SERVICE_INDICATOR_WRITE => {
                let Value::Bool(value) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.service_indicator_write(value)?;
            }
            BUILTIN_SERVICE_INDICATOR_TOGGLE => {
                host.service_indicator_toggle()?;
            }
            BUILTIN_SERVICE_INDICATOR_BREATHE => {
                host.service_indicator_breathe()?;
            }
            BUILTIN_SERVICE_INDICATOR_BLINK => {
                let Value::I32(off_ms) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let Value::I32(on_ms) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                host.service_indicator_blink(on_ms, off_ms)?;
            }
            BUILTIN_SERVICE_INDICATOR_READ => {
                let value = host.service_indicator_read()?;
                self.push(Value::Bool(value))?;
            }
            BUILTIN_APP_LAUNCH => {
                let app = self.pop()?;
                let resolver = self.resolver();
                host.app_launch(resolver.value_str(app)?)?;
            }
            BUILTIN_APP_ARM => {
                let app = self.pop()?;
                let resolver = self.resolver();
                host.app_arm(resolver.value_str(app)?)?;
            }
            BUILTIN_APP_DISARM => {
                let app = self.pop()?;
                let resolver = self.resolver();
                host.app_disarm(resolver.value_str(app)?)?;
            }
            BUILTIN_APP_INSTALL => {
                let file_ref = self.pop()?;
                let app_id = self.pop()?;
                let result = {
                    let resolver = self.resolver();
                    host.app_install(
                        resolver.value_str(file_ref)?,
                        Some(resolver.value_str(app_id)?),
                    )?
                };
                let value = self.app_install_record(result)?;
                self.push(value)?;
            }
            BUILTIN_APP_INSTALL_METADATA => {
                let file_ref = self.pop()?;
                let result = {
                    let resolver = self.resolver();
                    host.app_install(resolver.value_str(file_ref)?, None)?
                };
                let value = self.app_install_record(result)?;
                self.push(value)?;
            }
            BUILTIN_APP_REGISTRY_LIST => {
                let registry = host.app_registry_list()?;
                let mut items = [Value::Null; MAX_RUNTIME_LIST_ITEMS];
                let count = registry.apps.len().min(MAX_RUNTIME_LIST_ITEMS);
                for (index, app) in registry.apps.iter().take(count).enumerate() {
                    items[index] = self.runtime_string_value(Some(app.id))?;
                }
                let value = self.runtime_lists.alloc(&items[..count])?;
                self.push(value)?;
            }
            BUILTIN_APP_REGISTRY_GET => {
                let index = self.pop()?.expect_i32()?;
                let Value::List(list_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                let app_id = self.runtime_lists.get(list_id, index)?;
                let strings = self.resolver();
                let app_id = strings.value_str(app_id)?;
                let entry = host.app_registry_get(app_id)?;
                let value = self.app_registry_record(entry)?;
                self.push(value)?;
            }
            BUILTIN_APP_PROCESS_STACK => {
                let process = host.app_process_stack()?;
                let mut items = [Value::Null; MAX_RUNTIME_LIST_ITEMS];
                let count = process.apps.len().min(MAX_RUNTIME_LIST_ITEMS);
                for (index, app_id) in process.apps.iter().take(count).enumerate() {
                    items[index] = self.runtime_string_value(Some(app_id))?;
                }
                let value = self.runtime_lists.alloc(&items[..count])?;
                self.push(value)?;
            }
            BUILTIN_APP_ARMED_STACK => {
                let armed = host.app_armed_stack()?;
                let mut items = [Value::Null; MAX_RUNTIME_LIST_ITEMS];
                let count = armed.entries.len().min(MAX_RUNTIME_LIST_ITEMS);
                for (index, entry) in armed.entries.iter().take(count).enumerate() {
                    items[index] = self.app_armed_stack_record(*entry)?;
                }
                let value = self.runtime_lists.alloc(&items[..count])?;
                self.push(value)?;
            }
            BUILTIN_APP_ARMED_STACK_GET => {
                let index = self.pop()?.expect_i32()?;
                let Value::List(list_id) = self.pop()? else {
                    return Err(VmError::InvalidOperand);
                };
                self.push(self.runtime_lists.get(list_id, index)?)?;
            }
            BUILTIN_SERVICE_TIMER_EVERY => {
                let interval_ms = self.pop()?.expect_i32()?;
                let event_id = self.pop_sqbc_string_id()?;
                host.service_timer_every(self.index.string(event_id)?, interval_ms)?;
            }
            BUILTIN_SERVICE_TIMER_AFTER => {
                let delay_ms = self.pop()?.expect_i32()?;
                let event_id = self.pop_sqbc_string_id()?;
                host.service_timer_after(self.index.string(event_id)?, delay_ms)?;
            }
            BUILTIN_SERVICE_BLE_START => {
                let id = self.pop_sqbc_string_id()?;
                host.service_ble_start(self.index.string(id)?)?;
            }
            BUILTIN_SERVICE_BLE_STOP => {
                host.service_ble_stop()?;
            }
            BUILTIN_SERVICE_HTTP_START => {
                let id = self.pop_sqbc_string_id()?;
                host.service_http_start(self.index.string(id)?)?;
            }
            BUILTIN_SERVICE_HTTP_STOP => {
                host.service_http_stop()?;
            }
            BUILTIN_SERVICE_POWER_SLEEP => {
                let wake_after_ms = self.pop()?.expect_i32()?;
                host.service_power_sleep(wake_after_ms)?;
            }
            BUILTIN_SERVICE_WIFI_START_AP => {
                let ssid_id = self.pop_sqbc_string_id()?;
                let result = host.service_wifi_start_ap(self.index.string(ssid_id)?)?;
                let value = self.wifi_operation_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_STOP_AP => {
                let result = host.service_wifi_stop_ap()?;
                let value = self.wifi_operation_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_CONNECT => {
                let profile_id = self.pop_sqbc_string_id()?;
                let result = host.service_wifi_connect(self.index.string(profile_id)?)?;
                let value = self.wifi_operation_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_DISCONNECT => {
                let result = host.service_wifi_disconnect()?;
                let value = self.wifi_operation_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_STATUS => {
                let result = host.service_wifi_status()?;
                let value = self.wifi_status_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_GET_AP_IP => {
                let result = host.service_wifi_get_ap_ip()?;
                let value = self.wifi_ap_ip_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_SCAN => {
                let result = host.service_wifi_scan()?;
                let value = self.wifi_operation_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_OPERATION => {
                let result = host.service_wifi_operation()?;
                let value = self.wifi_operation_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_RESULT => {
                let result = host.service_wifi_result()?;
                let value = self.wifi_operation_result_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_CANCEL => {
                let result = host.service_wifi_cancel()?;
                let value = self.wifi_operation_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SERVICE_WIFI_SCAN_NETWORK => {
                let index = self.pop()?.expect_i32()?;
                let result = host.service_wifi_scan_network(index)?;
                let value = self.wifi_scan_network_record(result)?;
                self.push(value)?;
            }
            BUILTIN_DEVICE_CONFIG_LOAD => {
                let source_id = self.pop_sqbc_string_id()?;
                let result = host.device_config_load(self.index.string(source_id)?)?;
                let value = self.device_config_result_record(result)?;
                self.push(value)?;
            }
            BUILTIN_DEVICE_CONFIG_SET => {
                let value = self.pop()?;
                let key_id = self.pop_sqbc_string_id()?;
                let strings = self.resolver();
                let result = host.device_config_set(self.index.string(key_id)?, value, &strings)?;
                let value = self.device_config_result_record(result)?;
                self.push(value)?;
            }
            BUILTIN_DEVICE_CONFIG_REBIND => {
                let binding_id = self.pop_sqbc_string_id()?;
                let result = host.device_config_rebind(self.index.string(binding_id)?)?;
                let value = self.device_config_result_record(result)?;
                self.push(value)?;
            }
            BUILTIN_DEVICE_CONFIG_SAVE => {
                let destination_id = self.pop_sqbc_string_id()?;
                let result = host.device_config_save(self.index.string(destination_id)?)?;
                let value = self.device_config_result_record(result)?;
                self.push(value)?;
            }
            BUILTIN_BINBOOK_OPEN => {
                let path = self.pop()?;
                let result = {
                    let resolver = self.resolver();
                    host.binbook_open(resolver.value_str(path)?)?
                };
                let value = self.binbook_open_record(result)?;
                self.push(value)?;
            }
            BUILTIN_BINBOOK_INFO => {
                let book = self.pop_handle()?;
                let result = host.binbook_info(book)?;
                let value = self.binbook_info_record(result)?;
                self.push(value)?;
            }
            BUILTIN_BINBOOK_READ_PAGE => {
                let page_index = self.pop()?.expect_i32()?;
                let book = self.pop_handle()?;
                let result = host.binbook_read_page(book, page_index)?;
                let value = self.binbook_read_page_record(result)?;
                self.push(value)?;
            }
            BUILTIN_CONTENT_BINBOOK_LIST => {
                let limit = self.pop()?.expect_i32()?;
                let offset = self.pop()?.expect_i32()?;
                let library_id = self.pop_sqbc_string_id()?;
                let result =
                    host.content_binbook_list(self.index.string(library_id)?, offset, limit)?;
                let value = self.content_binbook_list_record(result)?;
                self.push(value)?;
            }
            crate::bytecode::BUILTIN_FILE_PICK_FILE => {
                let extension_id = self.pop_sqbc_string_id()?;
                let result = host.file_pick_file(self.index.string(extension_id)?)?;
                let value = self.file_pick_file_result_record(result)?;
                self.push(value)?;
            }
            crate::bytecode::BUILTIN_FILE_READ_TEXT => {
                let path_id = self.pop_sqbc_string_id()?;
                let result = host.file_read_text(self.index.string(path_id)?)?;
                let value = self.file_read_text_result_record(result)?;
                self.push(value)?;
            }
            crate::bytecode::BUILTIN_FILE_READ_LINES => {
                let max_lines = self.pop()?.expect_i32()?;
                let path_id = self.pop_sqbc_string_id()?;
                let result = host.file_read_lines(self.index.string(path_id)?, max_lines)?;
                let value = self.file_read_lines_result_record(result)?;
                self.push(value)?;
            }
            BUILTIN_FILE_COPY => {
                let name = self.pop()?;
                let library = self.pop()?;
                let source = self.pop()?;
                let result = {
                    let resolver = self.resolver();
                    host.file_copy(
                        resolver.value_str(source)?,
                        resolver.value_str(library)?,
                        resolver.value_str(name)?,
                    )?
                };
                let value = self.file_copy_result_record(result)?;
                self.push(value)?;
            }
            BUILTIN_SYSTEM_MEMORY => {
                let mut text = FixedString::new();
                host.system_memory_text(&mut text)?;
                let value = self.runtime_string_value(Some(text.as_str()?))?;
                self.push(value)?;
            }
            BUILTIN_SYSTEM_STORAGE => {
                let name_id = self.pop_sqbc_string_id()?;
                let name = self.index.string(name_id)?;
                let mut text = FixedString::new();
                host.system_storage_text(name, &mut text)?;
                let value = self.runtime_string_value(Some(text.as_str()?))?;
                self.push(value)?;
            }
            BUILTIN_SYSTEM_START_REASON => {
                let mut text = FixedString::new();
                host.system_start_reason_text(&mut text)?;
                let value = self.runtime_string_value(Some(text.as_str()?))?;
                self.push(value)?;
            }
            _ => return Err(VmError::InvalidOperand),
        }
        Ok(())
    }

    fn runtime_string_value(&mut self, value: Option<&str>) -> Result<Value, VmError> {
        let Some(value) = value else {
            return Ok(Value::Null);
        };
        self.strings.intern_event(&self.index, value)
    }

    fn wifi_operation_record(&mut self, result: WifiOperation<'_>) -> Result<Value, VmError> {
        let kind = self.runtime_string_value(result.kind)?;
        let state = self.runtime_string_value(Some(result.state))?;
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Active, Value::Bool(result.active)),
            RuntimeRecordField::new(RuntimeFieldName::Kind, kind),
            RuntimeRecordField::new(RuntimeFieldName::State, state),
            RuntimeRecordField::new(RuntimeFieldName::Done, Value::Bool(result.done)),
            RuntimeRecordField::new(RuntimeFieldName::Cancelled, Value::Bool(result.cancelled)),
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
        ])
    }

    fn wifi_operation_result_record(
        &mut self,
        result: WifiOperationResult<'_>,
    ) -> Result<Value, VmError> {
        let kind = self.runtime_string_value(result.kind)?;
        let state = self.runtime_string_value(Some(result.state))?;
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ready, Value::Bool(result.ready)),
            RuntimeRecordField::new(RuntimeFieldName::Kind, kind),
            RuntimeRecordField::new(RuntimeFieldName::State, state),
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Cancelled, Value::Bool(result.cancelled)),
            RuntimeRecordField::new(RuntimeFieldName::Count, Value::I32(result.count)),
        ])
    }

    fn device_config_result_record(
        &mut self,
        result: DeviceConfigResult<'_>,
    ) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let warning = self.runtime_string_value(result.warning)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Warning, warning),
        ])
    }

    fn file_pick_file_result_record(
        &mut self,
        result: FilePickFileResult<'_>,
    ) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let path = self.runtime_string_value(result.path)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Path, path),
        ])
    }

    fn file_read_text_result_record(
        &mut self,
        result: FileReadTextResult<'_>,
    ) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let text = self.runtime_string_value(result.text)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Text, text),
        ])
    }

    fn file_read_lines_result_record(
        &mut self,
        result: FileReadLinesResult<'_>,
    ) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let mut items = [Value::Null; MAX_RUNTIME_LIST_ITEMS];
        let count = result.lines.len().min(MAX_RUNTIME_LIST_ITEMS);
        for (index, line) in result.lines.iter().take(count).enumerate() {
            items[index] = self.runtime_string_value(Some(line))?;
        }
        let lines = self.runtime_lists.alloc(&items[..count])?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Lines, lines),
        ])
    }

    fn file_copy_result_record(&mut self, result: FileCopyResult<'_>) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let reference = self.runtime_string_value(result.reference)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Ref, reference),
            RuntimeRecordField::new(
                RuntimeFieldName::BytesWritten,
                Value::I32(result.bytes_written),
            ),
        ])
    }

    fn binbook_open_record(&mut self, result: BinBookOpenResult<'_>) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let book = result.book.map_or(Value::Null, Value::Handle);
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Book, book),
        ])
    }

    fn binbook_info_record(&mut self, result: BinBookInfoResult<'_>) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let title = self.runtime_string_value(result.title)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Title, title),
            RuntimeRecordField::new(RuntimeFieldName::PageCount, Value::I32(result.page_count)),
        ])
    }

    fn binbook_read_page_record(
        &mut self,
        result: BinBookReadPageResult<'_>,
    ) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let drawable = result.drawable.map_or(Value::Null, Value::Handle);
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Drawable, drawable),
        ])
    }

    fn content_binbook_entry_record(
        &mut self,
        entry: ContentBinBookEntry<'_>,
    ) -> Result<Value, VmError> {
        let name = self.runtime_string_value(Some(entry.name))?;
        let reference = self.runtime_string_value(Some(entry.reference))?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Name, name),
            RuntimeRecordField::new(RuntimeFieldName::Ref, reference),
            RuntimeRecordField::new(RuntimeFieldName::Size, Value::I32(entry.size)),
        ])
    }

    fn content_binbook_list_record(
        &mut self,
        result: ContentBinBookListResult<'_>,
    ) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let warning = self.runtime_string_value(result.warning)?;
        let mut items = [Value::Null; MAX_RUNTIME_LIST_ITEMS];
        let count = result.items.len().min(MAX_RUNTIME_LIST_ITEMS);
        for (index, entry) in result.items.iter().take(count).enumerate() {
            items[index] = self.content_binbook_entry_record(*entry)?;
        }
        let list = self.runtime_lists.alloc(&items[..count])?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Warning, warning),
            RuntimeRecordField::new(RuntimeFieldName::Items, list),
            RuntimeRecordField::new(RuntimeFieldName::Count, Value::I32(result.count)),
            RuntimeRecordField::new(RuntimeFieldName::HasMore, Value::Bool(result.has_more)),
        ])
    }

    fn display_info_record(&mut self, result: DisplayInfo<'_>) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let warning = self.runtime_string_value(result.warning)?;
        let status = self.runtime_string_value(Some(result.status))?;
        let binding = self.runtime_string_value(Some(result.binding))?;
        let driver = self.runtime_string_value(Some(result.driver))?;
        let transport = self.runtime_string_value(Some(result.transport))?;
        let color_model = self.runtime_string_value(Some(result.color_model))?;
        let native_pixel_format = self.runtime_string_value(Some(result.native_pixel_format))?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Warning, warning),
            RuntimeRecordField::new(RuntimeFieldName::Available, Value::Bool(result.available)),
            RuntimeRecordField::new(RuntimeFieldName::Status, status),
            RuntimeRecordField::new(RuntimeFieldName::Binding, binding),
            RuntimeRecordField::new(RuntimeFieldName::Driver, driver),
            RuntimeRecordField::new(RuntimeFieldName::Transport, transport),
            RuntimeRecordField::new(RuntimeFieldName::Width, Value::I32(result.width)),
            RuntimeRecordField::new(RuntimeFieldName::Height, Value::I32(result.height)),
            RuntimeRecordField::new(
                RuntimeFieldName::PhysicalWidth,
                Value::I32(result.physical_width),
            ),
            RuntimeRecordField::new(
                RuntimeFieldName::PhysicalHeight,
                Value::I32(result.physical_height),
            ),
            RuntimeRecordField::new(RuntimeFieldName::Rotation, Value::I32(result.rotation)),
            RuntimeRecordField::new(RuntimeFieldName::ColorModel, color_model),
            RuntimeRecordField::new(
                RuntimeFieldName::LogicalGrayLevels,
                Value::I32(result.logical_gray_levels),
            ),
            RuntimeRecordField::new(RuntimeFieldName::NativeBpp, Value::I32(result.native_bpp)),
            RuntimeRecordField::new(RuntimeFieldName::NativePixelFormat, native_pixel_format),
            RuntimeRecordField::new(
                RuntimeFieldName::DefaultFontHeight,
                Value::I32(result.default_font_height),
            ),
            RuntimeRecordField::new(
                RuntimeFieldName::SupportsPartialRefresh,
                Value::Bool(result.supports_partial_refresh),
            ),
            RuntimeRecordField::new(
                RuntimeFieldName::SupportsFastRefresh,
                Value::Bool(result.supports_fast_refresh),
            ),
        ])
    }

    fn app_registry_record(&mut self, entry: AppRegistryEntry<'_>) -> Result<Value, VmError> {
        let id = self.runtime_string_value(Some(entry.id))?;
        let name = self.runtime_string_value(Some(entry.name))?;
        let build = self.runtime_string_value(Some(entry.build))?;
        let description = self.runtime_string_value(Some(entry.description))?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Id, id),
            RuntimeRecordField::new(RuntimeFieldName::Name, name),
            RuntimeRecordField::new(RuntimeFieldName::Build, build),
            RuntimeRecordField::new(RuntimeFieldName::Description, description),
        ])
    }

    fn app_install_record(&mut self, result: AppInstallResult<'_>) -> Result<Value, VmError> {
        let id = self.runtime_string_value(Some(result.id))?;
        self.runtime_records
            .alloc(&[RuntimeRecordField::new(RuntimeFieldName::Id, id)])
    }

    fn app_armed_stack_record(&mut self, entry: AppArmedStackEntry<'_>) -> Result<Value, VmError> {
        let app_id = self.runtime_string_value(Some(entry.app_id))?;
        let event = self.runtime_string_value(Some(entry.event))?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::AppId, app_id),
            RuntimeRecordField::new(RuntimeFieldName::Event, event),
        ])
    }

    fn wifi_status_record(&mut self, result: WifiStatus<'_>) -> Result<Value, VmError> {
        let mode = self.runtime_string_value(result.mode)?;
        let ip_address = self.runtime_string_value(result.ip_address)?;
        let ssid = self.runtime_string_value(result.ssid)?;
        let error = self.runtime_string_value(result.error)?;
        let state = self.runtime_string_value(Some(result.state))?;
        let backend = self.runtime_string_value(Some(result.backend))?;
        let driver_mode = self.runtime_string_value(result.driver_mode)?;
        let last_backend_code = self.runtime_string_value(result.last_backend_code)?;
        let profile = self.runtime_string_value(result.profile)?;
        let auth = self.runtime_string_value(result.auth)?;
        let bssid = self.runtime_string_value(result.bssid)?;
        let disconnect_reason = self.runtime_string_value(result.disconnect_reason)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Active, Value::Bool(result.active)),
            RuntimeRecordField::new(RuntimeFieldName::Mode, mode),
            RuntimeRecordField::new(RuntimeFieldName::IpAddress, ip_address),
            RuntimeRecordField::new(RuntimeFieldName::Ssid, ssid),
            RuntimeRecordField::new(RuntimeFieldName::Clients, Value::I32(result.clients)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::State, state),
            RuntimeRecordField::new(RuntimeFieldName::Backend, backend),
            RuntimeRecordField::new(
                RuntimeFieldName::DriverStarted,
                Value::Bool(result.driver_started),
            ),
            RuntimeRecordField::new(RuntimeFieldName::Configured, Value::Bool(result.configured)),
            RuntimeRecordField::new(RuntimeFieldName::DriverMode, driver_mode),
            RuntimeRecordField::new(RuntimeFieldName::Channel, Value::I32(result.channel)),
            RuntimeRecordField::new(
                RuntimeFieldName::ApStartEvents,
                Value::I32(result.ap_start_events),
            ),
            RuntimeRecordField::new(
                RuntimeFieldName::ApStopEvents,
                Value::I32(result.ap_stop_events),
            ),
            RuntimeRecordField::new(
                RuntimeFieldName::ProbeEvents,
                Value::I32(result.probe_events),
            ),
            RuntimeRecordField::new(
                RuntimeFieldName::StaConnectedEvents,
                Value::I32(result.sta_connected_events),
            ),
            RuntimeRecordField::new(
                RuntimeFieldName::StaDisconnectedEvents,
                Value::I32(result.sta_disconnected_events),
            ),
            RuntimeRecordField::new(RuntimeFieldName::LastBackendCode, last_backend_code),
            RuntimeRecordField::new(RuntimeFieldName::Profile, profile),
            RuntimeRecordField::new(RuntimeFieldName::Connected, Value::Bool(result.connected)),
            RuntimeRecordField::new(
                RuntimeFieldName::ScanMatches,
                Value::I32(result.scan_matches),
            ),
            RuntimeRecordField::new(RuntimeFieldName::Rssi, Value::I32(result.rssi)),
            RuntimeRecordField::new(RuntimeFieldName::Auth, auth),
            RuntimeRecordField::new(RuntimeFieldName::Bssid, bssid),
            RuntimeRecordField::new(RuntimeFieldName::DisconnectReason, disconnect_reason),
            RuntimeRecordField::new(
                RuntimeFieldName::DisconnectReasonCode,
                Value::I32(result.disconnect_reason_code),
            ),
        ])
    }

    fn wifi_ap_ip_record(&mut self, result: WifiApIp<'_>) -> Result<Value, VmError> {
        let ip = self.runtime_string_value(result.ip)?;
        let gw = self.runtime_string_value(result.gw)?;
        let netmask = self.runtime_string_value(result.netmask)?;
        let error = self.runtime_string_value(result.error)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ip, ip),
            RuntimeRecordField::new(RuntimeFieldName::Gw, gw),
            RuntimeRecordField::new(RuntimeFieldName::Netmask, netmask),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
        ])
    }

    fn wifi_scan_network_record(&mut self, result: WifiScanNetwork<'_>) -> Result<Value, VmError> {
        let error = self.runtime_string_value(result.error)?;
        let network = result.network.unwrap_or_else(WifiAccessPoint::empty);
        let ssid = self.runtime_string_value(Some(network.ssid()?))?;
        let auth = self.runtime_string_value(network.auth)?;
        self.runtime_records.alloc(&[
            RuntimeRecordField::new(RuntimeFieldName::Ok, Value::Bool(result.ok)),
            RuntimeRecordField::new(RuntimeFieldName::Error, error),
            RuntimeRecordField::new(RuntimeFieldName::Ssid, ssid),
            RuntimeRecordField::new(
                RuntimeFieldName::SsidLength,
                Value::I32(network.ssid_length),
            ),
            RuntimeRecordField::new(RuntimeFieldName::Channel, Value::I32(network.channel)),
            RuntimeRecordField::new(RuntimeFieldName::Rssi, Value::I32(network.rssi)),
            RuntimeRecordField::new(RuntimeFieldName::Auth, auth),
            RuntimeRecordField::new(RuntimeFieldName::Hidden, Value::Bool(network.hidden)),
        ])
    }

    fn binary(&mut self, op: u8) -> Result<(), VmError> {
        let right = self.pop()?;
        let left = self.pop()?;
        let value = match op {
            OP_ADD => self.add_values(left, right)?,
            OP_SUB => Value::I32(left.expect_i32()? - right.expect_i32()?),
            OP_EQ => Value::Bool(values_equal(&self.index, &self.strings, left, right)?),
            OP_NE => Value::Bool(!values_equal(&self.index, &self.strings, left, right)?),
            OP_LT => Value::Bool(left.expect_i32()? < right.expect_i32()?),
            OP_LTE => Value::Bool(left.expect_i32()? <= right.expect_i32()?),
            OP_GT => Value::Bool(left.expect_i32()? > right.expect_i32()?),
            OP_GTE => Value::Bool(left.expect_i32()? >= right.expect_i32()?),
            _ => return Err(VmError::UnknownOpcode),
        };
        self.push(value)
    }

    fn add_values(&mut self, left: Value, right: Value) -> Result<Value, VmError> {
        if let (Value::I32(left), Value::I32(right)) = (left, right) {
            return Ok(Value::I32(left + right));
        }
        if left.is_string() && right.is_string() {
            let mut bytes = [0u8; MAX_RUNTIME_STRING_BYTES];
            let len = concat_value_strings(&self.index, &self.strings, left, right, &mut bytes)?;
            let text = str::from_utf8(&bytes[..len]).map_err(|_| VmError::InvalidUtf8)?;
            return self.runtime_string_value(Some(text));
        }
        Err(VmError::InvalidOperand)
    }

    fn push(&mut self, value: Value) -> Result<(), VmError> {
        if self.stack_len == MAX_STACK {
            return Err(VmError::StackOverflow);
        }
        self.stack[self.stack_len] = value;
        self.stack_len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, VmError> {
        if self.stack_len == 0 {
            return Err(VmError::StackUnderflow);
        }
        self.stack_len -= 1;
        Ok(self.stack[self.stack_len])
    }
}

impl<'a> Vm<'a> {
    pub fn new(program: Program<'a>) -> Self {
        let index = ProgramIndex::from_program(&program)
            .expect("Program parsed for Vm must fit ProgramIndex limits");
        Self {
            program,
            inner: ChunkedVm::new(index),
        }
    }

    pub fn dispatch<T: TraceSink>(&mut self, event: &str, trace: &mut T) -> Result<(), VmError> {
        let mut host = InMemoryVmHost {
            code: self.program.code,
            trace,
        };
        self.inner.dispatch(&mut host, event)
    }

    pub fn exited(&self) -> bool {
        self.inner.exited()
    }

    pub fn current_screen(&self) -> Result<Option<&str>, VmError> {
        self.inner.current_screen()
    }

    pub fn state_value(&self, name: &str) -> Result<Value, VmError> {
        self.inner.state_value(name)
    }

    pub fn program(&self) -> &Program<'a> {
        &self.program
    }

    pub fn string_resolver(&self) -> StringResolver<'_> {
        self.inner.string_resolver()
    }

    pub fn state_count(&self) -> usize {
        self.inner.state_count()
    }

    pub fn state_name(&self, index: usize) -> Result<&str, VmError> {
        self.inner.state_name(index)
    }

    pub fn state_at(&self, index: usize) -> Result<Value, VmError> {
        self.inner.state_at(index)
    }

    pub fn set_state_value(&mut self, name: &str, value: Value) -> Result<(), VmError> {
        self.inner.set_state_value(name, value)
    }
}

struct InMemoryVmHost<'a, T: TraceSink> {
    code: &'a [u8],
    trace: &'a mut T,
}

impl<T: TraceSink> SqbcReader for InMemoryVmHost<'_, T> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let end = offset
            .checked_add(out.len())
            .ok_or(VmError::InvalidSection)?;
        let bytes = self.code.get(offset..end).ok_or(VmError::InvalidSection)?;
        out.copy_from_slice(bytes);
        Ok(())
    }
}

impl<T: TraceSink> TraceSink for InMemoryVmHost<'_, T> {
    fn trace(&mut self, message: &str) {
        self.trace.trace(message);
    }

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
        self.trace.debug_print(strings, values);
    }

    fn draw_clear(&mut self, color: &str) {
        self.trace.draw_clear(color);
    }

    fn draw_text(
        &mut self,
        strings: &StringResolver<'_>,
        text: Value,
        options: DisplayTextOptions<'_>,
    ) {
        self.trace.draw_text(strings, text, options);
    }

    fn draw_rect(&mut self, options: DisplayRectOptions<'_>) {
        self.trace.draw_rect(options);
    }

    fn draw_line(&mut self, options: DisplayLineOptions<'_>) {
        self.trace.draw_line(options);
    }

    fn draw_select(&mut self, name: &str) -> Result<(), VmError> {
        self.trace.draw_select(name)
    }

    fn draw_image(&mut self, path: &str, options: DisplayResourceOptions) {
        self.trace.draw_image(path, options);
    }

    fn draw_resource(
        &mut self,
        strings: &StringResolver<'_>,
        drawable: Value,
        options: DisplayResourceOptions,
    ) {
        self.trace.draw_resource(strings, drawable, options);
    }

    fn display_info<'b>(&'b mut self) -> Result<DisplayInfo<'b>, VmError> {
        self.trace.display_info()
    }

    fn hardware_gpio_write(&mut self, name: &str, value: bool) -> Result<(), VmError> {
        self.trace.hardware_gpio_write(name, value)
    }

    fn hardware_gpio_toggle(&mut self, name: &str) -> Result<(), VmError> {
        self.trace.hardware_gpio_toggle(name)
    }

    fn hardware_gpio_read(&mut self, name: &str) -> Result<bool, VmError> {
        self.trace.hardware_gpio_read(name)
    }

    fn service_indicator_write(&mut self, value: bool) -> Result<(), VmError> {
        self.trace.service_indicator_write(value)
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        self.trace.service_indicator_toggle()
    }

    fn service_indicator_breathe(&mut self) -> Result<(), VmError> {
        self.trace.service_indicator_breathe()
    }

    fn service_indicator_blink(&mut self, on_ms: i32, off_ms: i32) -> Result<(), VmError> {
        self.trace.service_indicator_blink(on_ms, off_ms)
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        self.trace.service_indicator_read()
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        self.trace.app_launch(app)
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        self.trace.app_arm(app)
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        self.trace.app_disarm(app)
    }

    fn app_install<'b>(
        &'b mut self,
        file_ref: &str,
        app_id: Option<&str>,
    ) -> Result<AppInstallResult<'b>, VmError> {
        self.trace.app_install(file_ref, app_id)
    }

    fn app_registry_list<'b>(&'b mut self) -> Result<AppRegistryList<'b>, VmError> {
        self.trace.app_registry_list()
    }

    fn app_registry_get<'b>(&'b mut self, app_id: &str) -> Result<AppRegistryEntry<'b>, VmError> {
        self.trace.app_registry_get(app_id)
    }

    fn app_process_stack<'b>(&'b mut self) -> Result<AppProcessStack<'b>, VmError> {
        self.trace.app_process_stack()
    }

    fn app_armed_stack<'b>(&'b mut self) -> Result<AppArmedStack<'b>, VmError> {
        self.trace.app_armed_stack()
    }

    fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
        self.trace.service_timer_every(event, interval_ms)
    }

    fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
        self.trace.service_timer_after(event, delay_ms)
    }

    fn service_ble_start(&mut self, id: &str) -> Result<(), VmError> {
        self.trace.service_ble_start(id)
    }

    fn service_ble_stop(&mut self) -> Result<(), VmError> {
        self.trace.service_ble_stop()
    }

    fn service_http_start(&mut self, id: &str) -> Result<(), VmError> {
        self.trace.service_http_start(id)
    }

    fn service_http_stop(&mut self) -> Result<(), VmError> {
        self.trace.service_http_stop()
    }

    fn service_wifi_start_ap<'b>(&'b mut self, ssid: &str) -> Result<WifiOperation<'b>, VmError> {
        self.trace.service_wifi_start_ap(ssid)
    }

    fn service_wifi_stop_ap<'b>(&'b mut self) -> Result<WifiOperation<'b>, VmError> {
        self.trace.service_wifi_stop_ap()
    }

    fn service_wifi_connect<'b>(&'b mut self, profile: &str) -> Result<WifiOperation<'b>, VmError> {
        self.trace.service_wifi_connect(profile)
    }

    fn service_wifi_disconnect<'b>(&'b mut self) -> Result<WifiOperation<'b>, VmError> {
        self.trace.service_wifi_disconnect()
    }

    fn service_wifi_status<'b>(&'b mut self) -> Result<WifiStatus<'b>, VmError> {
        self.trace.service_wifi_status()
    }

    fn service_wifi_get_ap_ip<'b>(&'b mut self) -> Result<WifiApIp<'b>, VmError> {
        self.trace.service_wifi_get_ap_ip()
    }

    fn service_wifi_scan<'b>(&'b mut self) -> Result<WifiOperation<'b>, VmError> {
        self.trace.service_wifi_scan()
    }

    fn service_wifi_operation<'b>(&'b mut self) -> Result<WifiOperation<'b>, VmError> {
        self.trace.service_wifi_operation()
    }

    fn service_wifi_result<'b>(&'b mut self) -> Result<WifiOperationResult<'b>, VmError> {
        self.trace.service_wifi_result()
    }

    fn service_wifi_cancel<'b>(&'b mut self) -> Result<WifiOperation<'b>, VmError> {
        self.trace.service_wifi_cancel()
    }

    fn service_wifi_scan_network<'b>(
        &'b mut self,
        index: i32,
    ) -> Result<WifiScanNetwork<'b>, VmError> {
        self.trace.service_wifi_scan_network(index)
    }

    fn service_wifi_teardown(&mut self) -> Result<(), VmError> {
        self.trace.service_wifi_teardown()
    }

    fn service_power_sleep(&mut self, wake_after_ms: i32) -> Result<(), VmError> {
        self.trace.service_power_sleep(wake_after_ms)
    }

    fn system_memory_text(&mut self, out: &mut dyn Write) -> Result<(), VmError> {
        self.trace.system_memory_text(out)
    }

    fn system_storage_text(&mut self, name: &str, out: &mut dyn Write) -> Result<(), VmError> {
        self.trace.system_storage_text(name, out)
    }

    fn system_start_reason_text(&mut self, out: &mut dyn Write) -> Result<(), VmError> {
        self.trace.system_start_reason_text(out)
    }

    fn device_config_load<'b>(
        &'b mut self,
        source: &str,
    ) -> Result<DeviceConfigResult<'b>, VmError> {
        self.trace.device_config_load(source)
    }

    fn device_config_set<'b>(
        &'b mut self,
        key: &str,
        value: Value,
        strings: &StringResolver<'_>,
    ) -> Result<DeviceConfigResult<'b>, VmError> {
        self.trace.device_config_set(key, value, strings)
    }

    fn device_config_rebind<'b>(
        &'b mut self,
        binding: &str,
    ) -> Result<DeviceConfigResult<'b>, VmError> {
        self.trace.device_config_rebind(binding)
    }

    fn device_config_save<'b>(
        &'b mut self,
        destination: &str,
    ) -> Result<DeviceConfigResult<'b>, VmError> {
        self.trace.device_config_save(destination)
    }

    fn file_pick_file<'b>(
        &'b mut self,
        extension: &str,
    ) -> Result<FilePickFileResult<'b>, VmError> {
        self.trace.file_pick_file(extension)
    }

    fn file_read_text<'b>(&'b mut self, path: &str) -> Result<FileReadTextResult<'b>, VmError> {
        self.trace.file_read_text(path)
    }

    fn file_read_lines<'b>(
        &'b mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<FileReadLinesResult<'b>, VmError> {
        self.trace.file_read_lines(path, max_lines)
    }

    fn file_copy<'b>(
        &'b mut self,
        source: &str,
        library: &str,
        name: &str,
    ) -> Result<FileCopyResult<'b>, VmError> {
        self.trace.file_copy(source, library, name)
    }

    fn binbook_open<'b>(&'b mut self, path: &str) -> Result<BinBookOpenResult<'b>, VmError> {
        self.trace.binbook_open(path)
    }

    fn binbook_info<'b>(&'b mut self, book: Handle) -> Result<BinBookInfoResult<'b>, VmError> {
        self.trace.binbook_info(book)
    }

    fn binbook_read_page<'b>(
        &'b mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'b>, VmError> {
        self.trace.binbook_read_page(book, page_index)
    }

    fn content_binbook_list<'b>(
        &'b mut self,
        library: &str,
        offset: i32,
        limit: i32,
    ) -> Result<ContentBinBookListResult<'b>, VmError> {
        self.trace.content_binbook_list(library, offset, limit)
    }

    fn state_load(&mut self, out: &mut [u8]) -> Result<Option<usize>, VmError> {
        self.trace.state_load(out)
    }

    fn state_save(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        self.trace.state_save(bytes)
    }

    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        self.trace.state_reset_persistent()
    }
}

unsafe fn init_program_index_in_place(out: *mut ProgramIndex, index: &ProgramIndex) {
    ptr::copy_nonoverlapping(
        index.string_bytes.as_ptr(),
        ptr::addr_of_mut!((*out).string_bytes).cast::<u8>(),
        MAX_PROGRAM_STRING_BYTES,
    );
    ptr::copy_nonoverlapping(
        index.string_offsets.as_ptr(),
        ptr::addr_of_mut!((*out).string_offsets).cast::<u16>(),
        MAX_STRINGS,
    );
    ptr::copy_nonoverlapping(
        index.string_lens.as_ptr(),
        ptr::addr_of_mut!((*out).string_lens).cast::<u16>(),
        MAX_STRINGS,
    );
    ptr::addr_of_mut!((*out).string_count).write(index.string_count);
    ptr::copy_nonoverlapping(
        index.state_slots.as_ptr(),
        ptr::addr_of_mut!((*out).state_slots).cast(),
        MAX_STATE,
    );
    ptr::addr_of_mut!((*out).state_count).write(index.state_count);
    ptr::copy_nonoverlapping(
        index.functions.as_ptr(),
        ptr::addr_of_mut!((*out).functions).cast(),
        MAX_FUNCTIONS,
    );
    ptr::addr_of_mut!((*out).function_count).write(index.function_count);
    ptr::copy_nonoverlapping(
        index.handlers.as_ptr(),
        ptr::addr_of_mut!((*out).handlers).cast(),
        MAX_HANDLERS,
    );
    ptr::addr_of_mut!((*out).handler_count).write(index.handler_count);
    ptr::copy_nonoverlapping(
        index.trigger_timers.as_ptr(),
        ptr::addr_of_mut!((*out).trigger_timers).cast(),
        MAX_TRIGGERS,
    );
    ptr::addr_of_mut!((*out).trigger_timer_count).write(index.trigger_timer_count);
    ptr::copy_nonoverlapping(
        index.screens.as_ptr(),
        ptr::addr_of_mut!((*out).screens).cast(),
        MAX_SCREENS,
    );
    ptr::addr_of_mut!((*out).screen_count).write(index.screen_count);
    ptr::addr_of_mut!((*out).code_offset).write(index.code_offset);
    ptr::addr_of_mut!((*out).code_len).write(index.code_len);
}

unsafe fn init_runtime_records_in_place(out: *mut RuntimeRecords) {
    let records = ptr::addr_of_mut!((*out).records).cast::<RuntimeRecord>();
    for record_index in 0..MAX_RUNTIME_RECORDS {
        let record = records.add(record_index);
        let fields = ptr::addr_of_mut!((*record).fields).cast::<RuntimeRecordField>();
        for field_index in 0..MAX_RUNTIME_RECORD_FIELDS {
            fields.add(field_index).write(RuntimeRecordField::empty());
        }
        ptr::addr_of_mut!((*record).field_count).write(0);
    }
    ptr::addr_of_mut!((*out).next).write(0);
}

unsafe fn init_runtime_lists_in_place(out: *mut RuntimeLists) {
    let lists = ptr::addr_of_mut!((*out).lists).cast::<RuntimeList>();
    for list_index in 0..MAX_RUNTIME_LISTS {
        let list = lists.add(list_index);
        let items = ptr::addr_of_mut!((*list).items).cast::<Value>();
        for item_index in 0..MAX_RUNTIME_LIST_ITEMS {
            items.add(item_index).write(Value::Null);
        }
        ptr::addr_of_mut!((*list).item_count).write(0);
    }
    ptr::addr_of_mut!((*out).next).write(0);
}
