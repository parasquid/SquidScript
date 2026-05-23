#![cfg_attr(feature = "zephyr", no_std)]

use core::{
    ffi::c_void,
    mem::{align_of, size_of, MaybeUninit},
    ptr, slice, str,
};

#[cfg(feature = "zephyr")]
use core::panic::PanicInfo;

use squidvm_core::{
    error::VmError,
    host::{
        StorageCompletion as CoreStorageCompletion, StorageRequest, TraceSink, VmDispatch,
        MAX_STORAGE_TRANSFER_BYTES,
    },
    limits::MAX_CODE_CHUNK_BYTES,
    reader::SqbcReader,
    vm::ChunkedVm,
};

use squid_device_protocol::{
    encode_empty_response_into, encode_error_response_into, encode_hello_response_into,
    DecodeError, DeviceRequest, Opcode, Status as SqdpFrameStatus, MAX_APP_BYTES,
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

const SQDP_APP_ID_CAP: usize = 48;
const SQDP_PATH_CAP: usize = 128;

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
#[derive(Clone, Copy)]
pub struct SqdpTransferSession {
    pub active: bool,
    pub app_id: [u8; SQDP_APP_ID_CAP],
    pub total_len: usize,
    pub received: usize,
    pub expected_crc: u32,
    pub running_crc: u32,
    pub staging_path: [u8; SQDP_PATH_CAP],
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
            staging_path: [0; SQDP_PATH_CAP],
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
    pub staging_path: [u8; SQDP_PATH_CAP],
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
            staging_path: [0; SQDP_PATH_CAP],
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
    pub storage: SqvmStorageRequest,
}

impl Default for SqvmDispatchResult {
    fn default() -> Self {
        Self {
            status: SqvmStatus::Ok,
            outcome: SqvmDispatchOutcome::Complete,
            storage: SqvmStorageRequest::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SqvmCallbacks {
    pub user_data: *mut c_void,
    pub trace: Option<
        unsafe extern "C" fn(user_data: *mut c_void, message: *const u8, message_len: usize),
    >,
    pub read_exact_at: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            offset: usize,
            out: *mut u8,
            out_len: usize,
        ) -> i32,
    >,
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
    let mut host = FfiHost {
        callbacks,
        defer_sqbc_reads: false,
    };
    match ChunkedVm::init_in_place_from_reader(context.vm_ptr(), &mut host, scratch) {
        Ok(()) => {
            context.initialized = true;
            SqvmStatus::Ok
        }
        Err(_) => SqvmStatus::VmError,
    }
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
    let mut host = FfiHost {
        callbacks,
        defer_sqbc_reads: false,
    };
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
    let mut host = FfiHost {
        callbacks,
        defer_sqbc_reads: true,
    };
    write_dispatch_result(out_result, vm.dispatch_resumable(&mut host, event))
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
    let mut host = FfiHost {
        callbacks,
        defer_sqbc_reads: true,
    };
    write_dispatch_result(out_result, vm.resume_storage(&mut host, completion))
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
        || total_len > MAX_APP_BYTES as u64
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
        || total_len > MAX_APP_BYTES as u64
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

struct FfiHost {
    callbacks: SqvmCallbacks,
    defer_sqbc_reads: bool,
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
    result: Result<VmDispatch, VmError>,
) -> SqvmStatus {
    let out = &mut *out_result;
    match result {
        Ok(VmDispatch::Complete) => {
            *out = SqvmDispatchResult::default();
            SqvmStatus::Ok
        }
        Ok(VmDispatch::PendingStorage(request)) => {
            *out = SqvmDispatchResult {
                status: SqvmStatus::Ok,
                outcome: SqvmDispatchOutcome::PendingStorage,
                storage: storage_request_from_core(request),
            };
            SqvmStatus::Ok
        }
        Err(_) => {
            *out = SqvmDispatchResult {
                status: SqvmStatus::VmError,
                outcome: SqvmDispatchOutcome::Complete,
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
    let len = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    &bytes[..len]
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
