use core::{ffi::c_void, ptr};

use squidc_core::{
    compile::{compile, CompileRequest},
    sqbc::encode_sqbc,
};
use squidvm_ffi::{
    sqvm_context_init, sqvm_context_init_in_place, sqvm_context_prepare, sqvm_context_size,
    sqvm_dispatch, sqvm_dispatch_resume_storage, sqvm_dispatch_start_resumable,
    sqvm_trigger_timer_count, sqvm_trigger_timer_read, SqvmAppRegistryEntry, SqvmCallbacks,
    SqvmDispatchOutcome, SqvmDispatchResult, SqvmStatus, SqvmStorageCompletion,
    SqvmStorageRequestKind, SqvmTriggerTimer,
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
    wifi_actions: Vec<String>,
    wifi_status_count: usize,
    wifi_scan_count: usize,
    wifi_ap_ip_count: usize,
    system_memory_count: usize,
    system_storage_names: Vec<String>,
    registry_gets: Vec<String>,
}

fn trigger_event_text(timer: &SqvmTriggerTimer) -> &str {
    let len = timer
        .event
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(timer.event.len());
    core::str::from_utf8(&timer.event[..len]).unwrap()
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

unsafe extern "C" fn wifi_start_ap(
    user_data: *mut c_void,
    ssid: *const u8,
    ssid_len: usize,
    out: *mut squidvm_ffi::SqvmWifiActionResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let ssid = std::str::from_utf8(std::slice::from_raw_parts(ssid, ssid_len)).unwrap();
    host.wifi_actions.push(format!("startAP {ssid}"));
    *out = squidvm_ffi::SqvmWifiActionResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn wifi_stop_ap(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiActionResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_actions.push("stopAP".to_string());
    *out = squidvm_ffi::SqvmWifiActionResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn wifi_connect(
    user_data: *mut c_void,
    profile: *const u8,
    profile_len: usize,
    out: *mut squidvm_ffi::SqvmWifiActionResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let profile = std::str::from_utf8(std::slice::from_raw_parts(profile, profile_len)).unwrap();
    host.wifi_actions.push(format!("connect {profile}"));
    *out = squidvm_ffi::SqvmWifiActionResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn wifi_disconnect(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiActionResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_actions.push("disconnect".to_string());
    *out = squidvm_ffi::SqvmWifiActionResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn wifi_get_ap_ip(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiApIp,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_ap_ip_count += 1;
    *out = squidvm_ffi::SqvmWifiApIp {
        ip: b"192.168.4.1".as_ptr(),
        ip_len: b"192.168.4.1".len(),
        gw: b"192.168.4.1".as_ptr(),
        gw_len: b"192.168.4.1".len(),
        netmask: b"255.255.255.0".as_ptr(),
        netmask_len: b"255.255.255.0".len(),
        error: ptr::null(),
        error_len: 0,
    };
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

unsafe extern "C" fn system_memory_text(
    user_data: *mut c_void,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.system_memory_count += 1;
    let bytes = b"RAM 320 KiB heap 32 KiB used 48 KiB free";
    if out.is_null() || out_len.is_null() || out_cap < bytes.len() {
        return -1;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    *out_len = bytes.len();
    0
}

unsafe extern "C" fn system_storage_text(
    user_data: *mut c_void,
    name: *const u8,
    name_len: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let name = std::str::from_utf8(std::slice::from_raw_parts(name, name_len)).unwrap();
    host.system_storage_names.push(name.to_string());
    let bytes = b"Apps 128 KiB";
    if out.is_null() || out_len.is_null() || out_cap < bytes.len() {
        return -1;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    *out_len = bytes.len();
    0
}

unsafe extern "C" fn app_registry_list(
    user_data: *mut c_void,
    out: *mut SqvmAppRegistryEntry,
    out_cap: usize,
    out_count: *mut usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.traces.push("registry.list".to_string());
    if out.is_null() || out_count.is_null() || out_cap < 2 {
        return -1;
    }
    *out.add(0) = SqvmAppRegistryEntry {
        id: b"main".as_ptr(),
        id_len: b"main".len(),
        name: b"Main".as_ptr(),
        name_len: b"Main".len(),
        build: b"ffi-main".as_ptr(),
        build_len: b"ffi-main".len(),
        description: b"Root app".as_ptr(),
        description_len: b"Root app".len(),
    };
    *out.add(1) = SqvmAppRegistryEntry {
        id: b"reader".as_ptr(),
        id_len: b"reader".len(),
        name: b"Reader".as_ptr(),
        name_len: b"Reader".len(),
        build: b"ffi-reader".as_ptr(),
        build_len: b"ffi-reader".len(),
        description: b"Read documents".as_ptr(),
        description_len: b"Read documents".len(),
    };
    *out_count = 2;
    0
}

unsafe extern "C" fn app_registry_get(
    user_data: *mut c_void,
    app: *const u8,
    app_len: usize,
    out: *mut SqvmAppRegistryEntry,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let app = std::str::from_utf8(std::slice::from_raw_parts(app, app_len)).unwrap();
    host.registry_gets.push(app.to_string());
    if out.is_null() || app != "reader" {
        return -1;
    }
    *out = SqvmAppRegistryEntry {
        id: b"reader".as_ptr(),
        id_len: b"reader".len(),
        name: b"Reader".as_ptr(),
        name_len: b"Reader".len(),
        build: b"ffi-reader".as_ptr(),
        build_len: b"ffi-reader".len(),
        description: b"Read documents".as_ptr(),
        description_len: b"Read documents".len(),
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
        app_registry_list: Some(app_registry_list),
        app_registry_get: Some(app_registry_get),
        timer_every: Some(timer_every),
        timer_after: Some(timer_after),
        wifi_start_ap: Some(wifi_start_ap),
        wifi_stop_ap: Some(wifi_stop_ap),
        wifi_connect: Some(wifi_connect),
        wifi_disconnect: Some(wifi_disconnect),
        wifi_get_ap_ip: Some(wifi_get_ap_ip),
        wifi_status: Some(wifi_status),
        wifi_scan: Some(wifi_scan),
        system_memory_text: Some(system_memory_text),
        system_storage_text: Some(system_storage_text),
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

fn compile_trigger_registration_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-triggers"
app.triggers {
  service.timer.after("timer.break", 250)
  service.timer.every("timer.stretch", 60000)
}
event.on("timer.break") {
  debug.print("break")
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

fn compile_wifi_actions_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-wifi-actions"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidScript")
  let ip = service.wifi.getAPIP()
  let stop = service.wifi.stopAP()
  let connected = service.wifi.connect("dev")
  let disconnected = service.wifi.disconnect()
  debug.print(ap.ok, ip.ip, stop.ok, connected.ok, disconnected.ok)
}
screen("main") {}
"#,
    )
}

fn compile_system_resources_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-system"
event.on("app.start") {
  debug.print(system.memory())
  debug.print(system.storage("apps"))
}
screen("main") {}
"#,
    )
}

fn compile_app_registry_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-registry"
event.on("app.start") {
  let apps = app.registry()
  for appId in apps max 2 {
    debug.print(appId)
  }
  let selected = app.registry.get(apps, 1)
  debug.print(selected.id, selected.name, selected.build, selected.description)
}
screen("main") {}
"#,
    )
}

fn compile_helper_function_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-helper"
function report() {
  debug.print("from helper")
}
event.on("app.start") {
  report()
}
screen("main") {}
"#,
    )
}

fn dispatch_resumable_to_completion(
    context: &mut squidvm_ffi::SqvmContext,
    host: &mut Host,
    event: &[u8],
) -> SqvmDispatchResult {
    let mut result = SqvmDispatchResult::default();
    let status = unsafe {
        sqvm_dispatch_start_resumable(
            context,
            callbacks(host),
            event.as_ptr(),
            event.len(),
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
            sqvm_dispatch_resume_storage(context, callbacks(host), &completion, &mut result)
        };
        assert_eq!(status, SqvmStatus::Ok);
    }
    result
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
fn resumable_dispatch_supports_user_function_calls() {
    let mut host = Host {
        sqbc: compile_helper_function_sqbc(),
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

    let result = dispatch_resumable_to_completion(&mut context, &mut host, b"app.start");

    assert_eq!(result.outcome, SqvmDispatchOutcome::Complete);
    assert_eq!(host.output, vec!["from helper"]);
    assert_eq!(host.traces, vec!["app.start"]);
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
fn dispatches_wifi_action_service_callbacks() {
    let mut host = Host {
        sqbc: compile_wifi_actions_sqbc(),
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
        host.wifi_actions,
        vec![
            "startAP SquidScript".to_string(),
            "stopAP".to_string(),
            "connect dev".to_string(),
            "disconnect".to_string()
        ]
    );
    assert_eq!(host.wifi_ap_ip_count, 1);
    assert_eq!(host.output, vec!["true 192.168.4.1 true true true"]);
}

#[test]
fn dispatches_system_resource_text_callbacks() {
    let mut host = Host {
        sqbc: compile_system_resources_sqbc(),
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
    assert_eq!(host.system_memory_count, 1);
    assert_eq!(host.system_storage_names, vec!["apps".to_string()]);
    assert_eq!(
        host.output,
        vec![
            "RAM 320 KiB heap 32 KiB used 48 KiB free".to_string(),
            "Apps 128 KiB".to_string()
        ]
    );
}

#[test]
fn dispatches_app_registry_callbacks() {
    let mut host = Host {
        sqbc: compile_app_registry_sqbc(),
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
    assert_eq!(host.registry_gets, vec!["reader"]);
    assert_eq!(
        host.output,
        vec![
            "main".to_string(),
            "reader".to_string(),
            "reader Reader ffi-reader Read documents".to_string(),
        ]
    );
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
fn reads_trigger_timer_metadata_without_dispatching_app_arm() {
    let sqbc = compile_trigger_registration_sqbc();
    let mut count = 0usize;
    let status = unsafe { sqvm_trigger_timer_count(sqbc.as_ptr(), sqbc.len(), &mut count) };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(count, 2);

    let mut first = SqvmTriggerTimer::default();
    let status =
        unsafe { sqvm_trigger_timer_read(sqbc.as_ptr(), sqbc.len(), 0, &mut first as *mut _) };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(first.interval_ms, 250);
    assert!(!first.repeating);
    assert_eq!(trigger_event_text(&first), "timer.break");

    let mut second = SqvmTriggerTimer::default();
    let status =
        unsafe { sqvm_trigger_timer_read(sqbc.as_ptr(), sqbc.len(), 1, &mut second as *mut _) };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(second.interval_ms, 60000);
    assert!(second.repeating);
    assert_eq!(trigger_event_text(&second), "timer.stretch");

    let mut host = Host {
        sqbc,
        ..Host::default()
    };
    let mut context = sqvm_context_init();
    let mut scratch = vec![0u8; 4096];
    assert_eq!(
        unsafe {
            sqvm_context_init_in_place(
                &mut context,
                callbacks(&mut host),
                scratch.as_mut_ptr(),
                scratch.len(),
            )
        },
        SqvmStatus::Ok
    );
    assert_eq!(
        unsafe {
            sqvm_dispatch(
                &mut context,
                callbacks(&mut host),
                b"app.arm".as_ptr(),
                b"app.arm".len(),
            )
        },
        SqvmStatus::VmError
    );
    assert!(host.timer_after.is_empty());
    assert!(host.timer_every.is_empty());
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
