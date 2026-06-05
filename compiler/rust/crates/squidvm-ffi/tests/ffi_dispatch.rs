use core::{ffi::c_void, ptr};

use squidc_core::{
    compile::{compile, CompileRequest},
    sqbc::encode_sqbc,
};
use squidvm_ffi::{
    sqvm_context_init, sqvm_context_init_in_place, sqvm_context_prepare, sqvm_context_size,
    sqvm_device_binding_count_from_reader, sqvm_device_binding_read_from_reader, sqvm_dispatch,
    sqvm_dispatch_resume_storage, sqvm_dispatch_start_resumable,
    sqvm_dispatch_start_resumable_with_payload, sqvm_trigger_ble_profile_count,
    sqvm_trigger_ble_profile_read, sqvm_trigger_timer_count, sqvm_trigger_timer_read,
    SqvmAppRegistryEntry, SqvmAppStackEntry, SqvmBleProfileTrigger, SqvmCallbacks,
    SqvmDeviceBinding, SqvmDeviceConfigResult, SqvmDeviceConfigValue, SqvmDeviceConfigValueKind,
    SqvmDispatchOutcome, SqvmDispatchResult, SqvmDisplayInfo, SqvmEventPayloadField,
    SqvmFilePickFileResult, SqvmFileReadLinesResult, SqvmFileReadTextResult, SqvmStatus,
    SqvmStorageCompletion, SqvmStorageRequestKind, SqvmTriggerTimer,
};

#[path = "support/generated_ffi_dispatch_cases.rs"]
mod generated_ffi_dispatch_cases;

#[derive(Default)]
struct Host {
    sqbc: Vec<u8>,
    traces: Vec<String>,
    output: Vec<String>,
    drawlog: Vec<String>,
    indicator: bool,
    breathe_count: usize,
    blink_requests: Vec<(i32, i32)>,
    gpio: Vec<(String, bool)>,
    lifecycle: Vec<String>,
    app_install_files: Vec<(String, String)>,
    timer_every: Vec<(String, i32)>,
    timer_after: Vec<(String, i32)>,
    wifi_actions: Vec<String>,
    wifi_status_count: usize,
    wifi_scan_count: usize,
    wifi_ap_ip_count: usize,
    device_config_actions: Vec<String>,
    file_pick_files: Vec<String>,
    file_read_texts: Vec<String>,
    file_read_lines: Vec<(String, i32)>,
    system_memory_count: usize,
    system_storage_names: Vec<String>,
    system_start_reason_count: usize,
    power_sleep_requests: Vec<i32>,
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

fn fixed_text(bytes: &[u8]) -> &str {
    let len = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..len]).unwrap()
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

unsafe extern "C" fn display_select(
    user_data: *mut c_void,
    name: *const u8,
    name_len: usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let name = std::str::from_utf8(std::slice::from_raw_parts(name, name_len)).unwrap();
    host.drawlog.push(format!("draw=select name={name}"));
    0
}

unsafe extern "C" fn display_image(
    user_data: *mut c_void,
    path: *const u8,
    path_len: usize,
    options: *const squidvm_ffi::SqvmDisplayResourceOptions,
) {
    let host = &mut *(user_data as *mut Host);
    let path = std::str::from_utf8(std::slice::from_raw_parts(path, path_len)).unwrap();
    let options = *options;
    host.drawlog.push(format!(
        "draw=image path=\"{path}\" x={} y={}",
        options.x, options.y
    ));
}

unsafe extern "C" fn display_draw(
    user_data: *mut c_void,
    drawable: *const u8,
    drawable_len: usize,
    options: *const squidvm_ffi::SqvmDisplayResourceOptions,
) {
    let host = &mut *(user_data as *mut Host);
    let drawable = std::str::from_utf8(std::slice::from_raw_parts(drawable, drawable_len)).unwrap();
    let options = *options;
    host.drawlog.push(format!(
        "draw=resource drawable=\"{drawable}\" x={} y={}",
        options.x, options.y
    ));
}

unsafe extern "C" fn display_info(_user_data: *mut c_void, out: *mut SqvmDisplayInfo) -> i32 {
    if out.is_null() {
        return -1;
    }
    *out = SqvmDisplayInfo {
        ok: true,
        error: ptr::null(),
        error_len: 0,
        warning: ptr::null(),
        warning_len: 0,
        available: true,
        status: b"ready".as_ptr(),
        status_len: b"ready".len(),
        binding: b"display.default".as_ptr(),
        binding_len: b"display.default".len(),
        driver: b"ssd1306".as_ptr(),
        driver_len: b"ssd1306".len(),
        transport: b"i2c".as_ptr(),
        transport_len: b"i2c".len(),
        width: 78,
        height: 40,
        physical_width: 78,
        physical_height: 40,
        rotation: 0,
        color_model: b"mono".as_ptr(),
        color_model_len: b"mono".len(),
        logical_gray_levels: 2,
        native_bpp: 1,
        native_pixel_format: b"MONO1_PACKED".as_ptr(),
        native_pixel_format_len: b"MONO1_PACKED".len(),
        default_font_height: 8,
        supports_partial_refresh: false,
        supports_fast_refresh: true,
    };
    0
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

unsafe extern "C" fn indicator_blink(user_data: *mut c_void, on_ms: i32, off_ms: i32) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.blink_requests.push((on_ms, off_ms));
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

unsafe extern "C" fn app_install_file(
    user_data: *mut c_void,
    file_ref: *const u8,
    file_ref_len: usize,
    app_id: *const u8,
    app_id_len: usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let file_ref = std::str::from_utf8(std::slice::from_raw_parts(file_ref, file_ref_len)).unwrap();
    let app_id = std::str::from_utf8(std::slice::from_raw_parts(app_id, app_id_len)).unwrap();
    host.app_install_files
        .push((file_ref.to_string(), app_id.to_string()));
    0
}

unsafe extern "C" fn failing_app_install_file(
    _user_data: *mut c_void,
    _file_ref: *const u8,
    _file_ref_len: usize,
    _app_id: *const u8,
    _app_id_len: usize,
) -> i32 {
    -22
}

unsafe extern "C" fn wifi_start_ap(
    user_data: *mut c_void,
    ssid: *const u8,
    ssid_len: usize,
    out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let ssid = std::str::from_utf8(std::slice::from_raw_parts(ssid, ssid_len)).unwrap();
    host.wifi_actions.push(format!("startAP {ssid}"));
    *out = squidvm_ffi::SqvmWifiOperation {
        active: true,
        kind: b"startAP".as_ptr(),
        kind_len: b"startAP".len(),
        state: b"done".as_ptr(),
        state_len: b"done".len(),
        done: true,
        cancelled: false,
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn wifi_stop_ap(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_actions.push("stopAP".to_string());
    *out = squidvm_ffi::SqvmWifiOperation {
        active: true,
        kind: b"stopAP".as_ptr(),
        kind_len: b"stopAP".len(),
        state: b"done".as_ptr(),
        state_len: b"done".len(),
        done: true,
        cancelled: false,
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
    out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let profile = std::str::from_utf8(std::slice::from_raw_parts(profile, profile_len)).unwrap();
    host.wifi_actions.push(format!("connect {profile}"));
    *out = squidvm_ffi::SqvmWifiOperation {
        active: true,
        kind: b"connect".as_ptr(),
        kind_len: b"connect".len(),
        state: b"done".as_ptr(),
        state_len: b"done".len(),
        done: true,
        cancelled: false,
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn wifi_disconnect(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_actions.push("disconnect".to_string());
    *out = squidvm_ffi::SqvmWifiOperation {
        active: true,
        kind: b"disconnect".as_ptr(),
        kind_len: b"disconnect".len(),
        state: b"done".as_ptr(),
        state_len: b"done".len(),
        done: true,
        cancelled: false,
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
    out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_scan_count += 1;
    *out = squidvm_ffi::SqvmWifiOperation {
        active: true,
        kind: b"scan".as_ptr(),
        kind_len: b"scan".len(),
        state: b"started".as_ptr(),
        state_len: b"started".len(),
        done: false,
        cancelled: false,
        ok: false,
        error: b"unsupported".as_ptr(),
        error_len: b"unsupported".len(),
    };
    0
}

unsafe extern "C" fn wifi_operation(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_actions.push("operation".to_string());
    *out = squidvm_ffi::SqvmWifiOperation {
        active: true,
        kind: b"scan".as_ptr(),
        kind_len: b"scan".len(),
        state: b"done".as_ptr(),
        state_len: b"done".len(),
        done: true,
        cancelled: false,
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn wifi_result(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiOperationResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_actions.push("result".to_string());
    *out = squidvm_ffi::SqvmWifiOperationResult {
        ready: true,
        kind: b"scan".as_ptr(),
        kind_len: b"scan".len(),
        state: b"done".as_ptr(),
        state_len: b"done".len(),
        ok: true,
        error: ptr::null(),
        error_len: 0,
        cancelled: false,
        count: 3,
    };
    0
}

unsafe extern "C" fn wifi_cancel(
    user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.wifi_actions.push("cancel".to_string());
    *out = squidvm_ffi::SqvmWifiOperation {
        active: true,
        kind: b"scan".as_ptr(),
        kind_len: b"scan".len(),
        state: b"cancelled".as_ptr(),
        state_len: b"cancelled".len(),
        done: true,
        cancelled: true,
        ok: true,
        error: ptr::null(),
        error_len: 0,
    };
    0
}

unsafe extern "C" fn zephyr_wifi_scan_network(
    user_data: *mut c_void,
    index: i32,
    out: *mut squidvm_ffi::SqvmWifiScanNetworkResult,
) -> i32 {
    static VISIBLE_SSID: &[u8] = b"truncated-visible-ssid";
    static WPA2_AUTH: &[u8] = b"WPA2-PSK";
    static OPEN_AUTH: &[u8] = b"OPEN";
    static OTHER_SSID: &[u8] = b"other-auth";
    static WAPI_AUTH: &[u8] = b"WAPI";
    static mut NETWORKS: [squidvm_ffi::SqvmWifiAccessPoint; 3] = [
        squidvm_ffi::SqvmWifiAccessPoint {
            ssid: VISIBLE_SSID.as_ptr(),
            ssid_len: VISIBLE_SSID.len(),
            bssid: ptr::null(),
            bssid_len: 0,
            ssid_length: 40,
            channel: 6,
            rssi: -41,
            auth: WPA2_AUTH.as_ptr(),
            auth_len: WPA2_AUTH.len(),
            hidden: false,
        },
        squidvm_ffi::SqvmWifiAccessPoint {
            ssid: ptr::null(),
            ssid_len: 0,
            bssid: ptr::null(),
            bssid_len: 0,
            ssid_length: 0,
            channel: 11,
            rssi: -72,
            auth: OPEN_AUTH.as_ptr(),
            auth_len: OPEN_AUTH.len(),
            hidden: true,
        },
        squidvm_ffi::SqvmWifiAccessPoint {
            ssid: OTHER_SSID.as_ptr(),
            ssid_len: OTHER_SSID.len(),
            bssid: ptr::null(),
            bssid_len: 0,
            ssid_length: 10,
            channel: 1,
            rssi: -88,
            auth: WAPI_AUTH.as_ptr(),
            auth_len: WAPI_AUTH.len(),
            hidden: false,
        },
    ];
    let host = &mut *(user_data as *mut Host);
    let Ok(index) = usize::try_from(index) else {
        *out = squidvm_ffi::SqvmWifiScanNetworkResult::default();
        return 0;
    };
    let networks = &raw const NETWORKS;
    let Some(network) = (*networks).get(index).copied() else {
        *out = squidvm_ffi::SqvmWifiScanNetworkResult::default();
        return 0;
    };
    host.wifi_actions.push(format!("scanNetwork {index}"));
    *out = squidvm_ffi::SqvmWifiScanNetworkResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
        network,
    };
    0
}

unsafe extern "C" fn device_config_load(
    user_data: *mut c_void,
    source: *const u8,
    source_len: usize,
    out: *mut SqvmDeviceConfigResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let source = std::str::from_utf8(std::slice::from_raw_parts(source, source_len)).unwrap();
    host.device_config_actions.push(format!("load {source}"));
    *out = SqvmDeviceConfigResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
        warning: b"loaded".as_ptr(),
        warning_len: b"loaded".len(),
    };
    0
}

unsafe extern "C" fn device_config_set(
    user_data: *mut c_void,
    key: *const u8,
    key_len: usize,
    value: SqvmDeviceConfigValue,
    out: *mut SqvmDeviceConfigResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let key = std::str::from_utf8(std::slice::from_raw_parts(key, key_len)).unwrap();
    let value = match value.kind {
        SqvmDeviceConfigValueKind::Null => "null".to_string(),
        SqvmDeviceConfigValueKind::Bool => format!("bool:{}", value.bool_value),
        SqvmDeviceConfigValueKind::I32 => format!("i32:{}", value.i32_value),
        SqvmDeviceConfigValueKind::String => {
            let text =
                std::str::from_utf8(std::slice::from_raw_parts(value.string, value.string_len))
                    .unwrap();
            format!("string:{text}")
        }
    };
    host.device_config_actions
        .push(format!("set {key} {value}"));
    *out = SqvmDeviceConfigResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
        warning: ptr::null(),
        warning_len: 0,
    };
    0
}

unsafe extern "C" fn device_config_rebind(
    user_data: *mut c_void,
    alias: *const u8,
    alias_len: usize,
    out: *mut SqvmDeviceConfigResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let alias = std::str::from_utf8(std::slice::from_raw_parts(alias, alias_len)).unwrap();
    host.device_config_actions.push(format!("rebind {alias}"));
    *out = SqvmDeviceConfigResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
        warning: b"rebound".as_ptr(),
        warning_len: b"rebound".len(),
    };
    0
}

unsafe extern "C" fn device_config_save(
    user_data: *mut c_void,
    destination: *const u8,
    destination_len: usize,
    out: *mut SqvmDeviceConfigResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let destination =
        std::str::from_utf8(std::slice::from_raw_parts(destination, destination_len)).unwrap();
    host.device_config_actions
        .push(format!("save {destination}"));
    *out = SqvmDeviceConfigResult {
        ok: true,
        error: ptr::null(),
        error_len: 0,
        warning: ptr::null(),
        warning_len: 0,
    };
    0
}

unsafe extern "C" fn file_pick_file(
    user_data: *mut c_void,
    extension: *const u8,
    extension_len: usize,
    out: *mut SqvmFilePickFileResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let extension =
        std::str::from_utf8(std::slice::from_raw_parts(extension, extension_len)).unwrap();
    host.file_pick_files.push(extension.to_string());
    if out.is_null() {
        return -1;
    }
    (*out).ok = false;
    (*out).error = b"unsupported".as_ptr();
    (*out).error_len = b"unsupported".len();
    (*out).path = ptr::null();
    (*out).path_len = 0;
    0
}

unsafe extern "C" fn file_read_text(
    user_data: *mut c_void,
    path: *const u8,
    path_len: usize,
    out: *mut SqvmFileReadTextResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let path = std::str::from_utf8(std::slice::from_raw_parts(path, path_len)).unwrap();
    host.file_read_texts.push(path.to_string());
    if out.is_null() {
        return -1;
    }
    (*out).ok = false;
    (*out).error = b"unsupported".as_ptr();
    (*out).error_len = b"unsupported".len();
    (*out).text = ptr::null();
    (*out).text_len = 0;
    0
}

unsafe extern "C" fn file_read_lines(
    user_data: *mut c_void,
    path: *const u8,
    path_len: usize,
    max_lines: i32,
    out: *mut SqvmFileReadLinesResult,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    let path = std::str::from_utf8(std::slice::from_raw_parts(path, path_len)).unwrap();
    host.file_read_lines.push((path.to_string(), max_lines));
    if out.is_null() {
        return -1;
    }
    (*out).ok = false;
    (*out).error = b"unsupported".as_ptr();
    (*out).error_len = b"unsupported".len();
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

unsafe extern "C" fn system_start_reason_text(
    user_data: *mut c_void,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.system_start_reason_count += 1;
    let bytes = b"wake";
    if out.is_null() || out_len.is_null() || out_cap < bytes.len() {
        return -1;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    *out_len = bytes.len();
    0
}

unsafe extern "C" fn power_sleep(user_data: *mut c_void, wake_after_ms: i32) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.power_sleep_requests.push(wake_after_ms);
    0
}

unsafe extern "C" fn failing_indicator_write(_user_data: *mut c_void, _value: bool) -> i32 {
    -22
}

unsafe extern "C" fn failing_indicator_toggle(_user_data: *mut c_void) -> i32 {
    -22
}

unsafe extern "C" fn failing_indicator_read(_user_data: *mut c_void, _out: *mut bool) -> i32 {
    -22
}

unsafe extern "C" fn failing_indicator_breathe(_user_data: *mut c_void) -> i32 {
    -22
}

unsafe extern "C" fn failing_indicator_blink(
    _user_data: *mut c_void,
    _on_ms: i32,
    _off_ms: i32,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_hardware_gpio_write(
    _user_data: *mut c_void,
    _name: *const u8,
    _name_len: usize,
    _value: bool,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_hardware_gpio_toggle(
    _user_data: *mut c_void,
    _name: *const u8,
    _name_len: usize,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_hardware_gpio_read(
    _user_data: *mut c_void,
    _name: *const u8,
    _name_len: usize,
    _out: *mut bool,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_display_select(
    _user_data: *mut c_void,
    _name: *const u8,
    _name_len: usize,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_system_memory_text(
    _user_data: *mut c_void,
    _out: *mut u8,
    _out_cap: usize,
    _out_len: *mut usize,
) -> i32 {
    -28
}

unsafe extern "C" fn failing_app_arm(
    _user_data: *mut c_void,
    _app: *const u8,
    _app_len: usize,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_app_disarm(
    _user_data: *mut c_void,
    _app: *const u8,
    _app_len: usize,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_app_launch(
    _user_data: *mut c_void,
    _app: *const u8,
    _app_len: usize,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_timer_after(
    _user_data: *mut c_void,
    _event: *const u8,
    _event_len: usize,
    _delay_ms: i32,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_timer_every(
    _user_data: *mut c_void,
    _event: *const u8,
    _event_len: usize,
    _interval_ms: i32,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_wifi_start_ap(
    _user_data: *mut c_void,
    _ssid: *const u8,
    _ssid_len: usize,
    _out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_wifi_stop_ap(
    _user_data: *mut c_void,
    _out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_wifi_connect(
    _user_data: *mut c_void,
    _profile: *const u8,
    _profile_len: usize,
    _out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_wifi_disconnect(
    _user_data: *mut c_void,
    _out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_wifi_get_ap_ip(
    _user_data: *mut c_void,
    _out: *mut squidvm_ffi::SqvmWifiApIp,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_wifi_status(
    _user_data: *mut c_void,
    _out: *mut squidvm_ffi::SqvmWifiStatus,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_wifi_scan(
    _user_data: *mut c_void,
    _out: *mut squidvm_ffi::SqvmWifiOperation,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_device_config_load(
    _user_data: *mut c_void,
    _source: *const u8,
    _source_len: usize,
    _out: *mut SqvmDeviceConfigResult,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_device_config_set(
    _user_data: *mut c_void,
    _key: *const u8,
    _key_len: usize,
    _value: SqvmDeviceConfigValue,
    _out: *mut SqvmDeviceConfigResult,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_device_config_rebind(
    _user_data: *mut c_void,
    _alias: *const u8,
    _alias_len: usize,
    _out: *mut SqvmDeviceConfigResult,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_device_config_save(
    _user_data: *mut c_void,
    _destination: *const u8,
    _destination_len: usize,
    _out: *mut SqvmDeviceConfigResult,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_file_pick_file(
    _user_data: *mut c_void,
    _extension: *const u8,
    _extension_len: usize,
    _out: *mut SqvmFilePickFileResult,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_file_read_text(
    _user_data: *mut c_void,
    _path: *const u8,
    _path_len: usize,
    _out: *mut SqvmFileReadTextResult,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_file_read_lines(
    _user_data: *mut c_void,
    _path: *const u8,
    _path_len: usize,
    _max_lines: i32,
    _out: *mut SqvmFileReadLinesResult,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_app_registry_list(
    _user_data: *mut c_void,
    _out: *mut SqvmAppRegistryEntry,
    _out_cap: usize,
    _out_count: *mut usize,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_app_registry_get(
    _user_data: *mut c_void,
    _app: *const u8,
    _app_len: usize,
    _out: *mut SqvmAppRegistryEntry,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_app_process_stack(
    _user_data: *mut c_void,
    _out: *mut SqvmAppStackEntry,
    _out_cap: usize,
    _out_count: *mut usize,
) -> i32 {
    -22
}

unsafe extern "C" fn failing_app_armed_stack(
    _user_data: *mut c_void,
    _out: *mut SqvmAppStackEntry,
    _out_cap: usize,
    _out_count: *mut usize,
) -> i32 {
    -22
}

unsafe extern "C" fn malformed_wifi_status(
    _user_data: *mut c_void,
    out: *mut squidvm_ffi::SqvmWifiStatus,
) -> i32 {
    *out = squidvm_ffi::SqvmWifiStatus {
        active: false,
        state: ptr::null(),
        state_len: 7,
        backend: b"zephyr".as_ptr(),
        backend_len: b"zephyr".len(),
        ..squidvm_ffi::SqvmWifiStatus::default()
    };
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

unsafe extern "C" fn app_process_stack(
    user_data: *mut c_void,
    out: *mut SqvmAppStackEntry,
    out_cap: usize,
    out_count: *mut usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.traces.push("process.stack".to_string());
    if out.is_null() || out_count.is_null() || out_cap < 2 {
        return -1;
    }
    *out.add(0) = SqvmAppStackEntry {
        app_id: b"main".as_ptr(),
        app_id_len: b"main".len(),
        event: ptr::null(),
        event_len: 0,
    };
    *out.add(1) = SqvmAppStackEntry {
        app_id: b"reader".as_ptr(),
        app_id_len: b"reader".len(),
        event: ptr::null(),
        event_len: 0,
    };
    *out_count = 2;
    0
}

unsafe extern "C" fn app_armed_stack(
    user_data: *mut c_void,
    out: *mut SqvmAppStackEntry,
    out_cap: usize,
    out_count: *mut usize,
) -> i32 {
    let host = &mut *(user_data as *mut Host);
    host.traces.push("armed.stack".to_string());
    if out.is_null() || out_count.is_null() || out_cap < 2 {
        return -1;
    }
    *out.add(0) = SqvmAppStackEntry {
        app_id: b"break-reminder".as_ptr(),
        app_id_len: b"break-reminder".len(),
        event: b"timer.break".as_ptr(),
        event_len: b"timer.break".len(),
    };
    *out.add(1) = SqvmAppStackEntry {
        app_id: b"weather-sync".as_ptr(),
        app_id_len: b"weather-sync".len(),
        event: b"timer.sync".as_ptr(),
        event_len: b"timer.sync".len(),
    };
    *out_count = 2;
    0
}

fn callback_user_data(host: &mut Host) -> *mut c_void {
    host as *mut Host as *mut c_void
}

fn callbacks(_host: &mut Host) -> SqvmCallbacks {
    SqvmCallbacks {
        trace: Some(trace),
        read_exact_at: Some(read_exact_at),
        debug_output: Some(debug_output),
        display_clear: Some(display_clear),
        display_text: Some(display_text),
        display_rect: Some(display_rect),
        display_line: Some(display_line),
        display_select: Some(display_select),
        display_image: Some(display_image),
        display_draw: Some(display_draw),
        display_info: Some(display_info),
        indicator_write: Some(indicator_write),
        indicator_toggle: Some(indicator_toggle),
        indicator_read: Some(indicator_read),
        indicator_breathe: Some(indicator_breathe),
        indicator_blink: Some(indicator_blink),
        hardware_gpio_write: Some(hardware_gpio_write),
        hardware_gpio_toggle: Some(hardware_gpio_toggle),
        hardware_gpio_read: Some(hardware_gpio_read),
        app_launch: Some(app_launch),
        app_arm: Some(app_arm),
        app_disarm: Some(app_disarm),
        app_install_file: Some(app_install_file),
        app_registry_list: Some(app_registry_list),
        app_registry_get: Some(app_registry_get),
        app_process_stack: Some(app_process_stack),
        app_armed_stack: Some(app_armed_stack),
        timer_every: Some(timer_every),
        timer_after: Some(timer_after),
        wifi_start_ap: Some(wifi_start_ap),
        wifi_stop_ap: Some(wifi_stop_ap),
        wifi_connect: Some(wifi_connect),
        wifi_disconnect: Some(wifi_disconnect),
        wifi_get_ap_ip: Some(wifi_get_ap_ip),
        wifi_status: Some(wifi_status),
        wifi_scan: Some(wifi_scan),
        wifi_operation: Some(wifi_operation),
        wifi_result: Some(wifi_result),
        wifi_cancel: Some(wifi_cancel),
        wifi_scan_network: Some(zephyr_wifi_scan_network),
        device_config_load: Some(device_config_load),
        device_config_set: Some(device_config_set),
        device_config_rebind: Some(device_config_rebind),
        device_config_save: Some(device_config_save),
        file_pick_file: Some(file_pick_file),
        file_read_text: Some(file_read_text),
        file_read_lines: Some(file_read_lines),
        system_memory_text: Some(system_memory_text),
        system_storage_text: Some(system_storage_text),
        system_start_reason_text: Some(system_start_reason_text),
        power_sleep: Some(power_sleep),
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
  service.indicator.write(state.led)
  service.timer.every("timer.debug", 500)
  debug.print("blinky ready", state.led)
}
event.on("timer.debug") {
  service.indicator.toggle()
  state.led = service.indicator.read()
  debug.print("blink", state.led)
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

fn compile_indicator_blink_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-blink"
event.on("app.start") {
  service.indicator.blink()
  service.indicator.blink(120, 80)
  debug.print("blink ready")
}
screen("main") {}
"#,
    )
}

fn compile_indicator_toggle_read_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-indicator-toggle-read"
event.on("app.start") {
  service.indicator.toggle()
  debug.print("indicator", service.indicator.read())
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

fn compile_app_install_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-install"
event.on("app.start") {
  app.install("/sq/tmp/ble-object-test.sqbc", "installed-app")
}
screen("main") {}
"#,
    )
}

fn compile_app_disarm_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-disarm"
event.on("app.start") {
  app.disarm("break-reminder")
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

fn compile_ble_object_transfer_trigger_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-ble"
app.triggers {
  service.ble.profile("object-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: {
      complete: "ble.object.complete",
      error: "ble.object.error"
    }
  })
}
event.on("ble.object.complete", ev) {
  debug.print(ev.id)
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
  let info = display.info()
  debug.print(info.width, info.height, info.colorModel, info.nativePixelFormat)
  screen.open("main")
}
screen("main") {
  service.display.clear("gray0")
  service.display.text("Hello", { x: 10, y: 20 })
  service.display.rect(1, 2, 3, 4, { fillColor: "gray4" })
  service.display.line(5, 6, 7, 8, { color: "gray15" })
  service.display.select("status")
  service.display.image("data/icon.bmp", { x: 20, y: 24 })
  service.display.draw("drawable/page", { x: 0, y: 0 })
}
"#,
    )
}

fn compile_wifi_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-wifi"
event.on("app.start") {
  let status = service.wifi.status()
  service.wifi.scan()
  let result = service.wifi.result()
  debug.print(status.state, status.backend, status.driverStarted, status.error)
  debug.print(result.ok, result.error, result.count)
}
screen("main") {}
"#,
    )
}

fn compile_wifi_scan_networks_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-wifi-networks"
event.on("app.start") {
  service.wifi.scan()
  let result = service.wifi.result()
  let first = service.wifi.scanNetwork(0)
  let second = service.wifi.scanNetwork(1)
  let third = service.wifi.scanNetwork(2)
  debug.print(result.count)
  debug.print(first.ssidLength, first.channel, first.rssi, first.auth, first.hidden)
  debug.print(second.ssidLength, second.channel, second.rssi, second.auth, second.hidden)
  debug.print(third.ssidLength, third.channel, third.rssi, third.auth, third.hidden)
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

fn compile_power_lifecycle_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-power"
event.on("app.start") {
  debug.print(system.startReason())
  service.power.sleep({ wakeAfterMs: 30000 })
}
screen("main") {}
"#,
    )
}

fn compile_device_config_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-device-config"
event.on("app.start") {
  let loaded = device.config.load("package:device/indicator.sqdevice")
  let set = device.config.set("mode", "gpio")
  let rebound = device.config.rebind("indicator.default")
  let saved = device.config.save("flash")
  debug.print(loaded.ok, loaded.error, loaded.warning)
  debug.print(set.ok, set.error, rebound.ok, rebound.warning, saved.ok)
}
screen("main") {}
"#,
    )
}

fn compile_file_pick_file_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-content"
event.on("app.start") {
  let picked = file.pickFile(".binbook")
  debug.print(picked.ok, picked.error, picked.path)
}
screen("main") {}
"#,
    )
}

fn compile_file_read_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-content-read"
event.on("app.start") {
  let text = file.readText("notes.txt")
  let lines = file.readLines("notes.txt", 4)
  debug.print(text.ok, text.error, text.text)
  debug.print(lines.ok, lines.error, lines.lines)
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

fn compile_app_lifecycle_inspection_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-lifecycle-inspect"
event.on("app.start") {
  let process = app.processStack()
  for appId in process max 2 {
    debug.print(appId)
  }
  let armed = app.armedStack()
  for armedApp in armed max 2 {
    debug.print(armedApp.appId)
  }
  let selected = app.armedStack.get(armed, 1)
  debug.print(selected.appId, selected.event)
}
screen("main") {}
"#,
    )
}

fn compile_repeated_armed_stack_sqbc() -> Vec<u8> {
    compile_sqbc(
        r#"app "ffi-repeated-armed-stack"
event.on("timer.clock") {
  let armed = app.armedStack()
  for armedApp in armed max 2 {
    debug.print("armed", armedApp.appId, armedApp.event)
  }
  let selected = app.armedStack.get(armed, 0)
  debug.print("selected", selected.appId, selected.event)
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
            callback_user_data(host),
            &callbacks(host),
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
            sqvm_dispatch_resume_storage(
                context,
                callback_user_data(host),
                &callbacks(host),
                &completion,
                &mut result,
            )
        };
        assert_eq!(status, SqvmStatus::Ok);
    }
    result
}

fn dispatch_resumable_with_payload_to_completion(
    context: &mut squidvm_ffi::SqvmContext,
    host: &mut Host,
    event: &[u8],
    payload: &[SqvmEventPayloadField],
) -> SqvmDispatchResult {
    let mut result = SqvmDispatchResult::default();
    let status = unsafe {
        sqvm_dispatch_start_resumable_with_payload(
            context,
            callback_user_data(host),
            &callbacks(host),
            event.as_ptr(),
            event.len(),
            payload.as_ptr(),
            payload.len(),
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
                context,
                callback_user_data(host),
                &callbacks(host),
                &completion,
                &mut result,
            )
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let mut result = SqvmDispatchResult::default();
    let status = unsafe {
        sqvm_dispatch_start_resumable(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
                callback_user_data(&mut host),
                &callbacks(&mut host),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.output, vec!["78 40 mono MONO1_PACKED"]);
    assert_eq!(
        host.drawlog,
        vec![
            "draw=clear color=gray0".to_string(),
            "draw=text text=\"Hello\" x=10 y=20".to_string(),
            "draw=rect x=1 y=2 w=3 h=4".to_string(),
            "draw=line x1=5 y1=6 x2=7 y2=8".to_string(),
            "draw=select name=status".to_string(),
            "draw=image path=\"data/icon.bmp\" x=20 y=24".to_string(),
            "draw=resource drawable=\"drawable/page\" x=0 y=0".to_string()
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
            "true null 3".to_string()
        ]
    );
}

#[test]
fn dispatches_zephyr_wifi_scan_network_auth_and_original_ssid_length() {
    let mut host = Host {
        sqbc: compile_wifi_scan_networks_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();
    let callbacks = callbacks(&mut host);

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks,
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks,
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.wifi_scan_count, 1);
    assert_eq!(
        host.output,
        vec![
            "3".to_string(),
            "40 6 -41 wpa2 false".to_string(),
            "0 11 -72 open true".to_string(),
            "10 1 -88 unknown false".to_string(),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.breathe_count, 1);
    assert_eq!(host.output, vec!["breathe ready"]);
}

#[test]
fn dispatches_indicator_blink_service_callback() {
    let mut host = Host {
        sqbc: compile_indicator_blink_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.blink_requests, vec![(500, 500), (120, 80)]);
    assert_eq!(host.output, vec!["blink ready"]);
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
fn dispatches_device_config_callbacks() {
    let mut host = Host {
        sqbc: compile_device_config_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(
        host.device_config_actions,
        vec![
            "load package:device/indicator.sqdevice".to_string(),
            "set mode string:gpio".to_string(),
            "rebind indicator.default".to_string(),
            "save flash".to_string()
        ]
    );
    assert_eq!(
        host.output,
        vec![
            "true null loaded".to_string(),
            "true null true rebound true".to_string()
        ]
    );
}

#[test]
fn dispatches_file_pick_file_callback() {
    let mut host = Host {
        sqbc: compile_file_pick_file_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.file_pick_files, vec![".binbook".to_string()]);
    assert_eq!(host.output, vec!["false unsupported null".to_string()]);
}

#[test]
fn dispatches_file_read_callbacks() {
    let mut host = Host {
        sqbc: compile_file_read_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.file_read_texts, vec!["notes.txt".to_string()]);
    assert_eq!(host.file_read_lines, vec![("notes.txt".to_string(), 4)]);
    assert_eq!(
        host.output,
        vec![
            "false unsupported null".to_string(),
            "false unsupported <list>".to_string()
        ]
    );
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
fn dispatches_power_sleep_and_start_reason_callbacks() {
    let mut host = Host {
        sqbc: compile_power_lifecycle_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.system_start_reason_count, 1);
    assert_eq!(host.power_sleep_requests, vec![30000]);
    assert_eq!(host.output, vec!["wake".to_string()]);
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
fn dispatches_app_lifecycle_inspection_callbacks() {
    let mut host = Host {
        sqbc: compile_app_lifecycle_inspection_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(
        host.output,
        vec![
            "main".to_string(),
            "reader".to_string(),
            "break-reminder".to_string(),
            "weather-sync".to_string(),
            "weather-sync timer.sync".to_string(),
        ]
    );
}

#[test]
fn repeated_armed_stack_inspection_does_not_exhaust_ffi_dynamic_strings() {
    let mut host = Host {
        sqbc: compile_repeated_armed_stack_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    for tick in 0..10 {
        let status = unsafe {
            sqvm_dispatch(
                &mut context,
                callback_user_data(&mut host),
                &callbacks(&mut host),
                b"timer.clock".as_ptr(),
                b"timer.clock".len(),
            )
        };
        assert_eq!(status, SqvmStatus::Ok, "timer.clock tick {tick}");
    }

    assert_eq!(
        host.traces
            .iter()
            .filter(|trace| trace.as_str() == "armed.stack")
            .count(),
        10
    );
    assert!(host
        .output
        .iter()
        .any(|line| line == "selected break-reminder timer.break"));
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.gpio, vec![("GPIO8".to_string(), false)]);
    assert_eq!(host.output, vec!["gpio true", "gpio false"]);
}

#[test]
fn dispatches_app_install_file_callback() {
    let mut host = Host {
        sqbc: compile_app_install_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(
        host.app_install_files,
        vec![(
            "/sq/tmp/ble-object-test.sqbc".to_string(),
            "installed-app".to_string()
        )]
    );
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
fn callback_errors_surface_as_vm_error_status() {
    let cases = generated_ffi_dispatch_cases::callback_error_cases();

    for (name, sqbc, break_callback) in cases {
        let mut host = Host {
            sqbc: sqbc.clone(),
            ..Host::default()
        };
        let mut scratch = vec![0u8; 4096];
        let mut context = sqvm_context_init();
        let mut host_callbacks = callbacks(&mut host);
        let host_user_data = callback_user_data(&mut host);
        break_callback(&mut host_callbacks);

        let status = unsafe {
            sqvm_context_init_in_place(
                &mut context,
                host_user_data,
                &host_callbacks,
                scratch.as_mut_ptr(),
                scratch.len(),
            )
        };
        assert_eq!(status, SqvmStatus::Ok, "{name}");

        let status = unsafe {
            sqvm_dispatch(
                &mut context,
                host_user_data,
                &host_callbacks,
                b"app.start".as_ptr(),
                b"app.start".len(),
            )
        };

        assert_eq!(status, SqvmStatus::VmError, "{name}");
    }
}

#[test]
fn generated_callback_policy_cases_cover_manifest_inventory() {
    let cases = generated_ffi_dispatch_cases::callback_policy_cases();
    assert_eq!(cases.len(), 51);
    assert!(cases.contains(&("display_info", "unsupported_result")));
    assert!(cases.contains(&("wifi_operation", "idle_result")));
    assert!(cases.contains(&("wifi_scan_network", "unsupported_result")));
    assert!(cases.contains(&("system_start_reason_text", "required_vm_error")));
    assert!(cases.contains(&("power_sleep", "required_vm_error")));
}

#[test]
fn callback_result_records_reject_invalid_required_strings() {
    let mut host = Host {
        sqbc: compile_wifi_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();
    let mut host_callbacks = callbacks(&mut host);
    let host_user_data = callback_user_data(&mut host);
    host_callbacks.wifi_status = Some(malformed_wifi_status);

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            host_user_data,
            &host_callbacks,
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            host_user_data,
            &host_callbacks,
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::VmError);
}

#[test]
fn missing_optional_service_callbacks_return_unsupported_records() {
    let mut host = Host {
        sqbc: compile_wifi_actions_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();
    let mut host_callbacks = callbacks(&mut host);
    let host_user_data = callback_user_data(&mut host);
    host_callbacks.wifi_start_ap = None;
    host_callbacks.wifi_get_ap_ip = None;
    host_callbacks.wifi_stop_ap = None;
    host_callbacks.wifi_connect = None;
    host_callbacks.wifi_disconnect = None;

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            host_user_data,
            &host_callbacks,
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            host_user_data,
            &host_callbacks,
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(host.output, vec!["false null false false false"]);

    host = Host {
        sqbc: compile_wifi_sqbc(),
        ..Host::default()
    };
    scratch = vec![0u8; 4096];
    context = sqvm_context_init();
    host_callbacks = callbacks(&mut host);
    host_callbacks.wifi_scan = None;
    host_callbacks.wifi_result = None;

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            host_user_data,
            &host_callbacks,
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            host_user_data,
            &host_callbacks,
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(
        host.output,
        vec![
            "stopped zephyr true unsupported".to_string(),
            "false unsupported 0".to_string()
        ]
    );
}

#[test]
fn missing_device_config_callbacks_return_unsupported_records() {
    let mut host = Host {
        sqbc: compile_device_config_sqbc(),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut context = sqvm_context_init();
    let mut host_callbacks = callbacks(&mut host);
    let host_user_data = callback_user_data(&mut host);
    host_callbacks.device_config_load = None;
    host_callbacks.device_config_set = None;
    host_callbacks.device_config_rebind = None;
    host_callbacks.device_config_save = None;

    let status = unsafe {
        sqvm_context_init_in_place(
            &mut context,
            host_user_data,
            &host_callbacks,
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let status = unsafe {
        sqvm_dispatch(
            &mut context,
            host_user_data,
            &host_callbacks,
            b"app.start".as_ptr(),
            b"app.start".len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(
        host.output,
        vec![
            "false unsupported null".to_string(),
            "false unsupported false null false".to_string()
        ]
    );
}

#[test]
fn missing_file_callbacks_return_unsupported_records() {
    let mut pick_host = Host {
        sqbc: compile_file_pick_file_sqbc(),
        ..Host::default()
    };
    let mut read_host = Host {
        sqbc: compile_file_read_sqbc(),
        ..Host::default()
    };

    for host in [&mut pick_host, &mut read_host] {
        let mut scratch = vec![0u8; 4096];
        let mut context = sqvm_context_init();
        let mut host_callbacks = callbacks(host);
        let host_user_data = callback_user_data(host);
        host_callbacks.file_pick_file = None;
        host_callbacks.file_read_text = None;
        host_callbacks.file_read_lines = None;

        let status = unsafe {
            sqvm_context_init_in_place(
                &mut context,
                host_user_data,
                &host_callbacks,
                scratch.as_mut_ptr(),
                scratch.len(),
            )
        };
        assert_eq!(status, SqvmStatus::Ok);

        let status = unsafe {
            sqvm_dispatch(
                &mut context,
                host_user_data,
                &host_callbacks,
                b"app.start".as_ptr(),
                b"app.start".len(),
            )
        };

        assert_eq!(status, SqvmStatus::Ok);
    }

    assert_eq!(pick_host.output, vec!["false unsupported null".to_string()]);
    assert_eq!(
        read_host.output,
        vec![
            "false unsupported null".to_string(),
            "false unsupported <list>".to_string()
        ]
    );
}

#[test]
fn missing_noop_callbacks_remain_optional() {
    let cases = generated_ffi_dispatch_cases::missing_noop_cases();

    for (name, sqbc, remove_callback) in cases {
        let mut host = Host {
            sqbc: sqbc.clone(),
            ..Host::default()
        };
        let mut scratch = vec![0u8; 4096];
        let mut context = sqvm_context_init();
        let mut host_callbacks = callbacks(&mut host);
        let host_user_data = callback_user_data(&mut host);
        remove_callback(&mut host_callbacks);

        let status = unsafe {
            sqvm_context_init_in_place(
                &mut context,
                host_user_data,
                &host_callbacks,
                scratch.as_mut_ptr(),
                scratch.len(),
            )
        };
        assert_eq!(status, SqvmStatus::Ok, "{name}");

        let status = unsafe {
            sqvm_dispatch(
                &mut context,
                host_user_data,
                &host_callbacks,
                b"app.start".as_ptr(),
                b"app.start".len(),
            )
        };

        assert_eq!(status, SqvmStatus::Ok, "{name}");
    }
}

#[test]
fn missing_required_callbacks_surface_as_vm_error_status() {
    let cases = generated_ffi_dispatch_cases::missing_required_cases();

    for (name, sqbc, remove_callback) in cases {
        let mut host = Host {
            sqbc: sqbc.clone(),
            ..Host::default()
        };
        let mut scratch = vec![0u8; 4096];
        let mut context = sqvm_context_init();
        let mut host_callbacks = callbacks(&mut host);
        let host_user_data = callback_user_data(&mut host);
        remove_callback(&mut host_callbacks);

        let status = unsafe {
            sqvm_context_init_in_place(
                &mut context,
                host_user_data,
                &host_callbacks,
                scratch.as_mut_ptr(),
                scratch.len(),
            )
        };
        assert_eq!(status, SqvmStatus::Ok, "{name}");

        let status = unsafe {
            sqvm_dispatch(
                &mut context,
                host_user_data,
                &host_callbacks,
                b"app.start".as_ptr(),
                b"app.start".len(),
            )
        };

        assert_eq!(status, SqvmStatus::VmError, "{name}");
    }
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
                callback_user_data(&mut host),
                &callbacks(&mut host),
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
                callback_user_data(&mut host),
                &callbacks(&mut host),
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
fn reads_ble_object_transfer_trigger_metadata() {
    let sqbc = compile_ble_object_transfer_trigger_sqbc();
    let mut count = 0usize;
    let status = unsafe { sqvm_trigger_ble_profile_count(sqbc.as_ptr(), sqbc.len(), &mut count) };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(count, 1);

    let mut profile = SqvmBleProfileTrigger::default();
    let status = unsafe {
        sqvm_trigger_ble_profile_read(sqbc.as_ptr(), sqbc.len(), 0, &mut profile as *mut _)
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(fixed_text(&profile.profile), "object-transfer");
    assert_eq!(fixed_text(&profile.id), "sqbc-install");
    assert_eq!(fixed_text(&profile.role), "server");
    assert_eq!(profile.accept_count, 1);
    assert_eq!(fixed_text(&profile.accept[0]), ".sqbc");
    assert_eq!(profile.event_count, 2);
    assert_eq!(fixed_text(&profile.events[0].kind), "complete");
    assert_eq!(fixed_text(&profile.events[0].event), "ble.object.complete");
    assert_eq!(fixed_text(&profile.events[1].kind), "error");
    assert_eq!(fixed_text(&profile.events[1].event), "ble.object.error");
}

#[test]
fn dispatches_payload_handler_with_read_only_event_record() {
    let sqbc = compile_ble_object_transfer_trigger_sqbc();
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
                callback_user_data(&mut host),
                &callbacks(&mut host),
                scratch.as_mut_ptr(),
                scratch.len(),
            )
        },
        SqvmStatus::Ok
    );

    let name = b"id";
    let value = b"sqbc-install";
    let payload = [SqvmEventPayloadField {
        name: name.as_ptr(),
        name_len: name.len(),
        value: value.as_ptr(),
        value_len: value.len(),
    }];
    let result = dispatch_resumable_with_payload_to_completion(
        &mut context,
        &mut host,
        b"ble.object.complete",
        &payload,
    );
    assert_eq!(result.outcome, SqvmDispatchOutcome::Complete);
    assert_eq!(host.output, vec!["sqbc-install"]);
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };
    assert_eq!(status, SqvmStatus::Ok);

    let mut result = SqvmDispatchResult::default();
    let status = unsafe {
        sqvm_dispatch_start_resumable(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
        sqvm_dispatch_resume_storage(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
            &completion,
            &mut result,
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(result.outcome, SqvmDispatchOutcome::PendingStorage);
    assert_eq!(result.storage.kind, SqvmStorageRequestKind::StateLoad);

    let status = unsafe {
        sqvm_dispatch_resume_storage(
            &mut context,
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
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
fn reads_device_binding_metadata_without_dispatching_app() {
    let mut host = Host {
        sqbc: compile_sqbc(
            r#"app "ffi-device-binding"
device {
  indicator { use "device/indicator.sqdevice" }
}
event.on("app.start") {
  debug.print("started")
}
screen("main") {}
"#,
        ),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut count = 0usize;

    let status = unsafe {
        sqvm_device_binding_count_from_reader(
            &mut host as *mut Host as *mut c_void,
            Some(read_exact_at),
            scratch.as_mut_ptr(),
            scratch.len(),
            &mut count,
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(count, 1);

    let mut binding = SqvmDeviceBinding::default();
    let status = unsafe {
        sqvm_device_binding_read_from_reader(
            &mut host as *mut Host as *mut c_void,
            Some(read_exact_at),
            scratch.as_mut_ptr(),
            scratch.len(),
            0,
            &mut binding,
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(fixed_text(&binding.service), "indicator");
    assert_eq!(fixed_text(&binding.binding), "default");
    assert_eq!(fixed_text(&binding.resource), "device/indicator.sqdevice");
}

#[test]
fn reads_inline_gpio_device_binding_metadata_without_dispatching_app() {
    let mut host = Host {
        sqbc: compile_sqbc(
            r#"app "ffi-inline-device-binding"
device {
  indicator { use "gpio:GPIO8" }
}
event.on("app.start") {
  debug.print("started")
}
screen("main") {}
"#,
        ),
        ..Host::default()
    };
    let mut scratch = vec![0u8; 4096];
    let mut binding = SqvmDeviceBinding::default();

    let status = unsafe {
        sqvm_device_binding_read_from_reader(
            &mut host as *mut Host as *mut c_void,
            Some(read_exact_at),
            scratch.as_mut_ptr(),
            scratch.len(),
            0,
            &mut binding,
        )
    };
    assert_eq!(status, SqvmStatus::Ok);
    assert_eq!(fixed_text(&binding.service), "indicator");
    assert_eq!(fixed_text(&binding.binding), "default");
    assert_eq!(fixed_text(&binding.resource), "gpio:GPIO8");
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
            callback_user_data(&mut host),
            &callbacks(&mut host),
            scratch.as_mut_ptr(),
            scratch.len(),
        )
    };

    assert_eq!(status, SqvmStatus::Ok);
}
