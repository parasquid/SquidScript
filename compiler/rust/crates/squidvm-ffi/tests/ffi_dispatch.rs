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
    drawlog: Vec<String>,
    indicator: bool,
    breathe_count: usize,
    gpio: Vec<(String, bool)>,
    lifecycle: Vec<String>,
    timer_every: Vec<(String, i32)>,
    timer_after: Vec<(String, i32)>,
    wifi_status_count: usize,
    wifi_scan_count: usize,
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

unsafe extern "C" fn display_clear(user_data: *mut c_void, color: *const u8, color_len: usize) {
    let host = &mut *(user_data as *mut Host);
    let color = std::str::from_utf8(std::slice::from_raw_parts(color, color_len)).unwrap();
    host.drawlog.push(format!("draw=clear color={color}"));
}

unsafe extern "C" fn display_text(
    user_data: *mut c_void,
    text: *const u8,
    text_len: usize,
    options: *const squidvm_ffi::SqvmDisplayTextOptions,
) {
    let host = &mut *(user_data as *mut Host);
    let text = std::str::from_utf8(std::slice::from_raw_parts(text, text_len)).unwrap();
    let options = *options;
    host.drawlog.push(format!(
        "draw=text text=\"{text}\" x={} y={}",
        options.x, options.y
    ));
}

unsafe extern "C" fn display_rect(
    user_data: *mut c_void,
    options: *const squidvm_ffi::SqvmDisplayRectOptions,
) {
    let host = &mut *(user_data as *mut Host);
    let options = *options;
    host.drawlog.push(format!(
        "draw=rect x={} y={} w={} h={}",
        options.x, options.y, options.w, options.h
    ));
}

unsafe extern "C" fn display_line(
    user_data: *mut c_void,
    options: *const squidvm_ffi::SqvmDisplayLineOptions,
) {
    let host = &mut *(user_data as *mut Host);
    let options = *options;
    host.drawlog.push(format!(
        "draw=line x1={} y1={} x2={} y2={}",
        options.x1, options.y1, options.x2, options.y2
    ));
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

unsafe extern "C" fn indicator_breathe(user_data: *mut c_void) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.breathe_count += 1;
    0
}

fn gpio_slot<'a>(host: &'a mut Host, name: &str) -> &'a mut bool {
    if let Some(index) = host.gpio.iter().position(|(stored, _)| stored == name) {
        return &mut host.gpio[index].1;
    }
    host.gpio.push((name.to_string(), false));
    &mut host.gpio.last_mut().unwrap().1
}

unsafe extern "C" fn hardware_gpio_write(
    user_data: *mut c_void,
    name: *const u8,
    name_len: usize,
    value: bool,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let name = std::str::from_utf8(std::slice::from_raw_parts(name, name_len)).unwrap();
    *gpio_slot(host, name) = value;
    0
}

unsafe extern "C" fn hardware_gpio_toggle(
    user_data: *mut c_void,
    name: *const u8,
    name_len: usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let name = std::str::from_utf8(std::slice::from_raw_parts(name, name_len)).unwrap();
    let value = gpio_slot(host, name);
    *value = !*value;
    0
}

unsafe extern "C" fn hardware_gpio_read(
    user_data: *mut c_void,
    name: *const u8,
    name_len: usize,
    out: *mut bool,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let name = std::str::from_utf8(std::slice::from_raw_parts(name, name_len)).unwrap();
    *out = *gpio_slot(host, name);
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

unsafe extern "C" fn timer_after(
    user_data: *mut c_void,
    event: *const u8,
    event_len: usize,
    delay_ms: i32,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let event = std::str::from_utf8(std::slice::from_raw_parts(event, event_len)).unwrap();
    host.timer_after.push((event.to_string(), delay_ms));
    0
}

unsafe extern "C" fn app_launch(user_data: *mut c_void, app: *const u8, app_len: usize) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let app = std::str::from_utf8(std::slice::from_raw_parts(app, app_len)).unwrap();
    host.lifecycle.push(format!("launch {app}"));
    0
}

unsafe extern "C" fn app_arm(user_data: *mut c_void, app: *const u8, app_len: usize) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let app = std::str::from_utf8(std::slice::from_raw_parts(app, app_len)).unwrap();
    host.lifecycle.push(format!("arm {app}"));
    0
}

unsafe extern "C" fn app_disarm(user_data: *mut c_void, app: *const u8, app_len: usize) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let app = std::str::from_utf8(std::slice::from_raw_parts(app, app_len)).unwrap();
    host.lifecycle.push(format!("disarm {app}"));
    0
}

unsafe extern "C" fn wifi_status(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiStatus,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_status_count += 1;
    *out = squidvm_ffi::SqvmWifiStatus {
        active: false,
        state: b"stopped".as_ptr(),
        state_len: b"stopped".len(),
        backend: b"zephyr".as_ptr(),
        backend_len: b"zephyr".len(),
        driver_started: true,
        configured: false,
        error: b"unsupported".as_ptr(),
        error_len: b"unsupported".len(),
        ..squidvm_ffi::SqvmWifiStatus::default()
    };
    0
}

unsafe extern "C" fn wifi_scan(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiScanResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_scan_count += 1;
    *out = squidvm_ffi::SqvmWifiScanResult {
        ok: false,
        error: b"unsupported".as_ptr(),
        error_len: b"unsupported".len(),
        networks: ptr::null(),
        network_count: 0,
    };
    0
}

fn callbacks(host: &mut Host) -> SqvmCallbacks {
    SqvmCallbacks {
        user_data: host as *mut Host as *mut c_void,
        trace: Some(trace),
        read_exact_at: Some(read_exact_at),
        debug_output: Some(debug_output),
        display_clear: Some(display_clear),
        display_text: Some(display_text),
        display_rect: Some(display_rect),
        display_line: Some(display_line),
        indicator_write: Some(indicator_write),
        indicator_toggle: Some(indicator_toggle),
        indicator_read: Some(indicator_read),
        indicator_breathe: Some(indicator_breathe),
        hardware_gpio_write: Some(hardware_gpio_write),
        hardware_gpio_toggle: Some(hardware_gpio_toggle),
        hardware_gpio_read: Some(hardware_gpio_read),
        app_launch: Some(app_launch),
        app_arm: Some(app_arm),
        app_disarm: Some(app_disarm),
        timer_every: Some(timer_every),
        timer_after: Some(timer_after),
        wifi_status: Some(wifi_status),
        wifi_scan: Some(wifi_scan),
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

fn compile_indicator_breathe_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-breathe"
event.on("app.start") {
  service.indicator.breathe()
  debug.print("breathe ready")
}
screen("main") {}
"#,
    )
}

fn compile_hardware_gpio_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-gpio"
event.on("app.start") {
  hardware.gpio.write("GPIO8", true)
  debug.print("gpio", hardware.gpio.read("GPIO8"))
  hardware.gpio.toggle("GPIO8")
  debug.print("gpio", hardware.gpio.read("GPIO8"))
}
screen("main") {}
"#,
    )
}

fn compile_lifecycle_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-lifecycle"
event.on("app.start") {
  app.arm("break-reminder")
  app.launch("reader")
  service.timer.after("timer.break", 250)
  debug.print("lifecycle start")
}
event.on("timer.break") {
  app.disarm("break-reminder")
  debug.print("lifecycle timer")
}
screen("main") {}
"#,
    )
}

fn compile_exit_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-exit"
event.on("app.start") {
  debug.print("before exit")
  app.exit()
}
screen("main") {}
"#,
    )
}

fn compile_display_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-display"
event.on("app.start") {
  screen.open("main")
}
screen("main") {
  service.display.clear("gray0")
  service.display.text("Hello", { x: 10, y: 20 })
  service.display.rect(1, 2, 3, 4, { fillColor: "gray4" })
  service.display.line(5, 6, 7, 8, { color: "gray15" })
}
"#,
    )
}

fn compile_wifi_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-wifi"
event.on("app.start") {
  let status = service.wifi.status()
  let scan = service.wifi.scan()
  debug.print(status.state, status.backend, status.driverStarted, status.error)
  debug.print(scan.ok, scan.error, scan.count)
}
screen("main") {}
"#,
    )
}

#[test]
fn resumable_dispatch_reports_app_exit() {
    let mut host = Host {
        sqbc: compile_exit_sqbc(),
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
    while result.outcome == SqvmDispatchOutcome::PendingStorage {
        let mut completion = SqvmStorageCompletion::default();
        match result.storage.kind {
            SqvmStorageRequestKind::SqbcRead => {
                completion.has_len = true;
                completion.len = result.storage.len;
                let start = result.storage.offset;
                let end = start + result.storage.len;
                completion.bytes[..result.storage.len].copy_from_slice(&host.sqbc[start..end]);
            }
            SqvmStorageRequestKind::StateLoad => {}
            SqvmStorageRequestKind::StateSave | SqvmStorageRequestKind::StateReset => {}
            SqvmStorageRequestKind::None => panic!("pending storage without request"),
        }
        let status = unsafe {
            sqvm_dispatch_resume_storage(
                &mut context,
                callbacks(&mut host),
                &completion,
                &mut result,
            )
        };
        assert_eq!(status, SqvmStatus::Ok);
    }

    assert_eq!(result.outcome, SqvmDispatchOutcome::Complete);
    assert!(result.exited);
    assert_eq!(host.output, vec!["before exit"]);
    assert_eq!(host.traces, vec!["app.start", "app.exit"]);
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
fn dispatches_display_service_callbacks() {
    let mut host = Host {
        sqbc: compile_display_sqbc(),
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
    assert_eq!(
        host.drawlog,
        vec![
            "draw=clear color=gray0".to_string(),
            "draw=text text=\"Hello\" x=10 y=20".to_string(),
            "draw=rect x=1 y=2 w=3 h=4".to_string(),
            "draw=line x1=5 y1=6 x2=7 y2=8".to_string()
        ]
    );
}

#[test]
fn dispatches_wifi_status_and_scan_callbacks() {
    let mut host = Host {
        sqbc: compile_wifi_sqbc(),
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
    assert_eq!(host.wifi_status_count, 1);
    assert_eq!(host.wifi_scan_count, 1);
    assert_eq!(
        host.output,
        vec![
            "stopped zephyr true unsupported".to_string(),
            "false unsupported 0".to_string()
        ]
    );
}

#[test]
fn dispatches_indicator_breathe_service_callback() {
    let mut host = Host {
        sqbc: compile_indicator_breathe_sqbc(),
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
    assert_eq!(host.breathe_count, 1);
    assert_eq!(host.output, vec!["breathe ready"]);
}

#[test]
fn dispatches_hardware_gpio_service_callbacks() {
    let mut host = Host {
        sqbc: compile_hardware_gpio_sqbc(),
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
    assert_eq!(host.gpio, vec![("GPIO8".to_string(), false)]);
    assert_eq!(host.output, vec!["gpio true", "gpio false"]);
}

#[test]
fn dispatches_app_lifecycle_and_timer_after_callbacks() {
    let mut host = Host {
        sqbc: compile_lifecycle_sqbc(),
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
    assert_eq!(
        host.lifecycle,
        vec![
            "arm break-reminder".to_string(),
            "launch reader".to_string()
        ]
    );
    assert_eq!(host.timer_after, vec![("timer.break".to_string(), 250)]);
    assert_eq!(host.output, vec!["lifecycle start"]);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callbacks(&mut host),
            b"timer.break".as_ptr(),
            b"timer.break".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(
        host.lifecycle,
        vec![
            "arm break-reminder".to_string(),
            "launch reader".to_string(),
            "disarm break-reminder".to_string()
        ]
    );
    assert_eq!(
        host.output,
        vec!["lifecycle start".to_string(), "lifecycle timer".to_string()]
    );
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
