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
    output: Vec<String>,
    indicator: bool,
    timer_every: Vec<(String, i32)>,
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

unsafe extern "C" fn debug_output(user_data: *mut c_void, message: *const u8, message_len: usize) {
    let host = &mut *(user_data as *mut Host);
    let message = std::str::from_utf8(std::slice::from_raw_parts(message, message_len)).unwrap();
    host.output.push(message.to_string());
}

unsafe extern "C" fn indicator_write(user_data: *mut c_void, value: bool) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.indicator = value;
    0
}

unsafe extern "C" fn indicator_toggle(user_data: *mut c_void) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.indicator = !host.indicator;
    0
}

unsafe extern "C" fn indicator_read(user_data: *mut c_void, out: *mut bool) -> i32 {
    let host = &mut *(user_data as *mut Host);
    *out = host.indicator;
    0
}

unsafe extern "C" fn timer_every(
    user_data: *mut c_void,
    event: *const u8,
    event_len: usize,
    interval_ms: i32,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let event = std::str::from_utf8(std::slice::from_raw_parts(event, event_len)).unwrap();
    host.timer_every.push((event.to_string(), interval_ms));
    0
}

fn callbacks(host: &mut Host) -> SqvmCallbacks {
    SqvmCallbacks {
        user_data: host as *mut Host as *mut c_void,
        trace: Some(trace),
        read_exact_at: Some(read_exact_at),
        debug_output: Some(debug_output),
        indicator_write: Some(indicator_write),
        indicator_toggle: Some(indicator_toggle),
        indicator_read: Some(indicator_read),
        timer_every: Some(timer_every),
        timer_after: None,
    }
}

fn compile_sqbc(source: &str) -> Vec<u8> {
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    encode_sqbc(&compiled.ir.unwrap()).unwrap()
}

fn compile_counter_sqbc() -> Vec<u8> {
    compile_sqbc(include_str!(
        "../../../fixtures/conformance/headless_counter.squid"
    ))
}

fn compile_blinky_service_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-blinky"
state { led: bool = false }
event.on("app.start") {
  service.indicator.write(led)
  service.timer.every("timer.debug", 500)
  debug.print("blinky ready", led)
}
event.on("timer.debug") {
  service.indicator.toggle()
  led = service.indicator.read()
  debug.print("blink", led)
}
screen("main") {}
"#,
    )
}

#[test]
fn dispatches_sqbc_through_c_abi_callbacks() {
    let mut host = Host {
        sqbc: compile_counter_sqbc(),
        ..Host::default()
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
fn dispatches_debug_indicator_and_timer_service_callbacks() {
    let mut host = Host {
        sqbc: compile_blinky_service_sqbc(),
        ..Host::default()
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
    assert_eq!(host.indicator, false);
    assert_eq!(host.timer_every, vec![("timer.debug".to_string(), 500)]);
    assert_eq!(host.output, vec!["blinky ready false"]);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callbacks(&mut host),
            b"timer.debug".as_ptr(),
            b"timer.debug".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.indicator, true);
    assert_eq!(host.output, vec!["blinky ready false", "blink true"]);
}

#[test]
fn resumable_dispatch_reports_sqbc_and_state_storage_requests() {
    let mut host = Host {
        sqbc: compile_counter_sqbc(),
        ..Host::default()
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
        ..Host::default()
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
