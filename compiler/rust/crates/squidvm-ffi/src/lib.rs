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

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqvmStatus {
    Ok = 0,
    InvalidArgument = 1,
    VmError = 2,
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

#[cfg(feature = "zephyr")]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
