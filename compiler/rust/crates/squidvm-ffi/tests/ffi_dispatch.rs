use core::{ffi::c_void, ptr};

use squidc_core::{
    compile::{compile, CompileRequest},
    sqbc::encode_sqbc,
};
use squidvm_ffi::{
    sqvm_context_init, sqvm_context_init_in_place, sqvm_context_prepare, sqvm_context_size,
    sqvm_dispatch, sqvm_dispatch_resume_storage, sqvm_dispatch_start_resumable, SqvmCallbacks,
    SqvmDispatchOutcome, SqvmDispatchResult, SqvmStatus, SqvmStorageCompletion,
    SqvmStorageRequestKind,
};

#[derive(Default)]
struct Host {
    sqbc: Vec<u8>,
    traces: Vec<String>,
}

unsafe extern "C" fn trace(user_data: *mut c_void, message: *const u8, message_len: usize) {
    let host = &mut *(user_data as *mut Host);
    let message = std::str::from_utf8(std::slice::from_raw_parts(message, message_len)).unwrap();
    host.traces.push(message.to_string());
}

unsafe extern "C" fn read_exact_at(
    user_data: *mut c_void,
    offset: usize,
    out: *mut u8,
    out_len: usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let Some(bytes) = host.sqbc.get(offset..offset + out_len) else {
        return -1;
    };
    ptr::copy_nonoverlapping(bytes.as_ptr(), out, out_len);
    0
}

fn callbacks(host: &mut Host) -> SqvmCallbacks {
    SqvmCallbacks {
        user_data: host as *mut Host as *mut c_void,
        trace: Some(trace),
        read_exact_at: Some(read_exact_at),
    }
}

fn compile_counter_sqbc() -> Vec<u8> {
    let source = include_str!("../../../fixtures/conformance/headless_counter.squid");
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    encode_sqbc(&compiled.ir.unwrap()).unwrap()
}

#[test]
fn dispatches_sqbc_through_c_abi_callbacks() {
    let mut host = Host {
        sqbc: compile_counter_sqbc(),
        traces: Vec::new(),
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.traces, vec!["app.start", "state.load", "state.save"]);
}

#[test]
fn resumable_dispatch_reports_sqbc_and_state_storage_requests() {
    let mut host = Host {
        sqbc: compile_counter_sqbc(),
        traces: Vec::new(),
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let mut result = SqvmDispatchResult::default();
    let status = unsafe {
        sqvm_dispatch_start_resumable(
            &mut context,
            callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
            &mut result,
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(result.status, SqvmStatus::Ok);
    assert_eq!(result.outcome, SqvmDispatchOutcome::PendingStorage);
    assert_eq!(result.storage.kind, SqvmStorageRequestKind::SqbcRead);
    assert!(result.storage.len > 0);
    assert_eq!(host.traces, vec!["app.start"]);

    let offset = result.storage.offset;
    let len = result.storage.len;
    let mut completion = SqvmStorageCompletion::default();
    completion.has_len = true;
    completion.len = len;
    completion.bytes[..len].copy_from_slice(&host.sqbc[offset..offset + len]);

    let status = unsafe {
        sqvm_dispatch_resume_storage(&mut context, callbacks(&mut host), &completion, &mut result)
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(result.outcome, SqvmDispatchOutcome::PendingStorage);
    assert_eq!(result.storage.kind, SqvmStorageRequestKind::StateLoad);

    let status = unsafe {
        sqvm_dispatch_resume_storage(
            &mut context,
            callbacks(&mut host),
            &SqvmStorageCompletion::default(),
            &mut result,
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(result.outcome, SqvmDispatchOutcome::PendingStorage);
    assert_eq!(result.storage.kind, SqvmStorageRequestKind::StateSave);
    assert!(result.storage.len > 0);
    assert_eq!(host.traces, vec!["app.start", "state.load"]);

    let status = unsafe {
        sqvm_dispatch_resume_storage(
            &mut context,
            callbacks(&mut host),
            &SqvmStorageCompletion::default(),
            &mut result,
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(result.outcome, SqvmDispatchOutcome::Complete);
    assert_eq!(result.storage.kind, SqvmStorageRequestKind::None);
    assert_eq!(host.traces, vec!["app.start", "state.load", "state.save"]);
}

#[test]
fn reports_context_size_for_zephyr_static_allocation() {
    assert!(sqvm_context_size() >= core::mem::size_of_val(&sqvm_context_init()));
}

#[test]
fn prepares_raw_context_storage_for_c_callers() {
    let mut host = Host {
        sqbc: compile_counter_sqbc(),
        traces: Vec::new(),
    };
    let mut raw_context = vec![0xff; sqvm_context_size()];
    let mut scratch = vec![0u8; 4096];

    let status = unsafe { sqvm_context_prepare(raw_context.as_mut_ptr(), raw_context.len()) };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_context_init_in_place(
            raw_context.as_mut_ptr().cast(),
            callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
}
