use core::fmt::Write;

use esp_hal::{delay::Delay, usb_serial_jtag::UsbSerialJtag};
use squidvm_core::{error::VmError, limits::MAX_APP_BYTES, program::Program};

use crate::{
    dev_harness::{
        validate_package_path, AppName, AppRegistry, AppRegistryError, AppSlot, AppStorage,
        AppStorageError, APP_REGISTRY_CAP,
    },
    protocol::{fnv1a, parse_install},
};

use super::{
    lifecycle::{boot_main, finish_current_exit, process_pending_actions, set_runtime_error},
    state::{import_state, print_resources, print_state, registry_installed_bytes},
    vm::{dispatch_loaded_vm, load_vm_for_app, AppRef},
    ActiveVm, RuntimeSink, TempApp, BUILD_ID, INSTALL_TIMEOUT_MS,
};

const STATE_IMPORT_CAP: usize = 512;
static mut STATE_IMPORT_BYTES: [u8; STATE_IMPORT_CAP] = [0; STATE_IMPORT_CAP];

pub fn handle_command(
    command: &str,
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    delay: &Delay,
    registry: &mut AppRegistry,
    app_storage: &mut impl AppStorage,
    app_load_bytes: &'static mut [u8; MAX_APP_BYTES],
    temp_app: &mut TempApp,
    trace: &mut RuntimeSink<'_>,
    vm: &mut Option<ActiveVm>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    trace.set_app_storage_used(registry_installed_bytes(registry));
    if command == "help" {
        writeln!(serial, "commands: HELLO INSTALL.APP <app-id> <len> <fnv32hex> INSTALL.RESOURCE <app-id> <path> <len> <fnv32hex> RUN.TEMP <app-id> <len> <fnv32hex> RUN.APP <app-id> RUN.EVENT <app-id> <event> KEY SELECT APP.LIST RESOURCES.GET WIFI.STATUS STATE.GET STATE.IMPORT <len> <fnv32hex> TRACE.GET OUTPUT.GET DRAWLOG.GET ERRORS.GET RESET STORAGE.FORMAT").ok();
    } else if command == "HELLO" || command == "hello" || command == "info" {
        writeln!(serial, "target=esp32c3-super-mini").ok();
        writeln!(serial, "build={BUILD_ID}").ok();
        writeln!(serial, "profile=dev").ok();
        writeln!(serial, "app_slots={APP_REGISTRY_CAP}").ok();
        writeln!(serial, "installed_apps={}", registry.iter().count()).ok();
        writeln!(serial, "vm_loaded={}", vm.is_some()).ok();
        writeln!(
            serial,
            "storage={}",
            if storage_error.is_some() {
                "error"
            } else {
                "ok"
            }
        )
        .ok();
        writeln!(serial, "OK HELLO").ok();
    } else if command == "RESOURCES.GET" || command == "resources" {
        print_resources(serial, registry, temp_app, vm.as_ref());
    } else if command == "WIFI.STATUS" || command == "wifi.status" {
        writeln!(serial, "BEGIN WIFI.STATUS").ok();
        trace.print_wifi_status(serial);
        writeln!(serial, "END WIFI.STATUS").ok();
        writeln!(serial, "OK WIFI.STATUS").ok();
    } else if let Some(rest) = command
        .strip_prefix("INSTALL.APP ")
        .or_else(|| command.strip_prefix("install.app "))
    {
        let Some((app_id, request_text)) = rest.split_once(' ') else {
            writeln!(serial, "ERR install.app").ok();
            return;
        };
        match parse_install(request_text) {
            Ok(request) if request.len <= MAX_APP_BYTES => {
                let len = request.len;
                let slot = match registry.reserve_install(app_id, len, MAX_APP_BYTES) {
                    Ok(slot) => slot,
                    Err(AppRegistryError::Full) => {
                        writeln!(serial, "ERR install.app full").ok();
                        return;
                    }
                    Err(AppRegistryError::InvalidAppId) => {
                        writeln!(serial, "ERR install.app invalid-app-id").ok();
                        return;
                    }
                    Err(_) => {
                        writeln!(serial, "ERR install.app").ok();
                        return;
                    }
                };
                writeln!(serial, "READY install.app app={app_id} len={len}").ok();
                let read = read_exact_timeout(
                    serial,
                    &mut app_load_bytes[..len],
                    delay,
                    INSTALL_TIMEOUT_MS,
                );
                if read != len {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(serial, "ERR install.app timeout read={read} expected={len}").ok();
                    return;
                }
                let actual_hash = fnv1a(&app_load_bytes[..len]);
                if actual_hash == request.expected_hash {
                    if let Err(error) = Program::parse(&app_load_bytes[..len]).map(|_| ()) {
                        *last_error = Some(error);
                        writeln!(serial, "ERR install.app invalid-bytecode").ok();
                        return;
                    }
                    if let Err(error) = app_storage.write_app(app_id, &app_load_bytes[..len]) {
                        *storage_error = Some(error);
                        writeln!(serial, "ERR install.app storage").ok();
                        return;
                    }
                    if let Err(error) = registry.commit_install(slot, app_id, len, actual_hash) {
                        *last_error = Some(VmError::InvalidOperand);
                        match error {
                            AppRegistryError::Full => writeln!(serial, "ERR install.app full").ok(),
                            AppRegistryError::InvalidAppId => {
                                writeln!(serial, "ERR install.app invalid-app-id").ok()
                            }
                            _ => writeln!(serial, "ERR install.app").ok(),
                        };
                        return;
                    }
                    let _ = trace.teardown_services();
                    *vm = None;
                    *vm_slot = None;
                    *temp_app = TempApp::empty();
                    trace.clear();
                    trace.clear_timers();
                    trace.reset_stack();
                    *last_error = None;
                    *storage_error = None;
                    writeln!(serial, "OK install.app app={app_id} hash={actual_hash:08x}").ok();
                } else {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(
                        serial,
                        "ERR install.app hash expected={:08x} actual={actual_hash:08x}",
                        request.expected_hash
                    )
                    .ok();
                }
            }
            _ => {
                *last_error = Some(VmError::TooLarge);
                writeln!(serial, "ERR install.app").ok();
            }
        }
    } else if let Some(rest) = command
        .strip_prefix("INSTALL.RESOURCE ")
        .or_else(|| command.strip_prefix("install.resource "))
    {
        let Some((app_id, rest)) = rest.split_once(' ') else {
            writeln!(serial, "ERR install.resource").ok();
            return;
        };
        let Some((path, request_text)) = rest.split_once(' ') else {
            writeln!(serial, "ERR install.resource").ok();
            return;
        };
        if AppName::new(app_id).is_err() || validate_package_path(path).is_err() {
            writeln!(serial, "ERR install.resource invalid-name").ok();
            return;
        }
        match parse_install(request_text) {
            Ok(request) if request.len <= MAX_APP_BYTES => {
                let len = request.len;
                writeln!(
                    serial,
                    "READY install.resource app={app_id} path={path} len={len}"
                )
                .ok();
                let read = read_exact_timeout(
                    serial,
                    &mut app_load_bytes[..len],
                    delay,
                    INSTALL_TIMEOUT_MS,
                );
                if read != len {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(
                        serial,
                        "ERR install.resource timeout read={read} expected={len}"
                    )
                    .ok();
                    return;
                }
                let actual_hash = fnv1a(&app_load_bytes[..len]);
                if actual_hash != request.expected_hash {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(
                        serial,
                        "ERR install.resource hash expected={:08x} actual={actual_hash:08x}",
                        request.expected_hash
                    )
                    .ok();
                    return;
                }
                if let Err(error) =
                    app_storage.write_app_resource(app_id, path, &app_load_bytes[..len])
                {
                    *storage_error = Some(error);
                    writeln!(serial, "ERR install.resource storage").ok();
                    return;
                }
                *storage_error = None;
                writeln!(
                    serial,
                    "OK install.resource app={app_id} path={path} hash={actual_hash:08x}"
                )
                .ok();
            }
            _ => {
                *last_error = Some(VmError::TooLarge);
                writeln!(serial, "ERR install.resource").ok();
            }
        }
    } else if let Some(rest) = command
        .strip_prefix("RUN.TEMP ")
        .or_else(|| command.strip_prefix("run.temp "))
    {
        let Some((app_id, request_text)) = rest.split_once(' ') else {
            writeln!(serial, "ERR RUN.TEMP").ok();
            return;
        };
        match parse_install(request_text) {
            Ok(request) if request.len <= MAX_APP_BYTES => {
                if AppName::new(app_id).is_err() {
                    writeln!(serial, "ERR RUN.TEMP invalid-app-id").ok();
                    return;
                }
                let len = request.len;
                writeln!(serial, "READY RUN.TEMP app={app_id} len={len}").ok();
                let read = read_exact_timeout(
                    serial,
                    &mut app_load_bytes[..len],
                    delay,
                    INSTALL_TIMEOUT_MS,
                );
                if read != len {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(serial, "ERR RUN.TEMP timeout read={read} expected={len}").ok();
                    return;
                }
                let actual_hash = fnv1a(&app_load_bytes[..len]);
                if actual_hash != request.expected_hash {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(
                        serial,
                        "ERR RUN.TEMP hash expected={:08x} actual={actual_hash:08x}",
                        request.expected_hash
                    )
                    .ok();
                    return;
                }
                if let Err(error) = Program::parse(&app_load_bytes[..len]).map(|_| ()) {
                    *last_error = Some(error);
                    writeln!(serial, "ERR RUN.TEMP invalid-bytecode").ok();
                    return;
                }
                *temp_app = match TempApp::new(app_id, len, actual_hash) {
                    Ok(temp) => temp,
                    Err(_) => {
                        writeln!(serial, "ERR RUN.TEMP invalid-app-id").ok();
                        return;
                    }
                };
                let _ = trace.teardown_services();
                trace.clear();
                trace.clear_timers();
                trace.remove_app_from_stack(AppRef::Temp);
                *vm = None;
                *vm_slot = None;
                trace.active_app = Some(AppRef::Temp);
                match load_vm_for_app(
                    AppRef::Temp,
                    registry,
                    app_storage,
                    app_load_bytes,
                    temp_app,
                ) {
                    Ok(loaded) => {
                        *vm = Some(loaded);
                        *vm_slot = None;
                    }
                    Err(error) => {
                        set_runtime_error(error, last_error, storage_error);
                        writeln!(serial, "ERR RUN.TEMP {:?}", error).ok();
                        return;
                    }
                }
                match dispatch_loaded_vm(
                    vm,
                    AppRef::Temp,
                    "app.start",
                    registry,
                    app_storage,
                    trace,
                ) {
                    Ok(()) => {
                        process_pending_actions(
                            trace,
                            registry,
                            app_storage,
                            app_load_bytes,
                            temp_app,
                            vm,
                            vm_slot,
                            last_error,
                            storage_error,
                        );
                        if trace.exited {
                            finish_current_exit(
                                trace,
                                registry,
                                app_storage,
                                app_load_bytes,
                                temp_app,
                                vm,
                                vm_slot,
                                last_error,
                                storage_error,
                            );
                        }
                        *last_error = None;
                        writeln!(serial, "OK RUN.TEMP app={app_id} hash={actual_hash:08x}").ok();
                    }
                    Err(error) => {
                        set_runtime_error(error, last_error, storage_error);
                        writeln!(serial, "ERR RUN.TEMP {:?}", error).ok();
                    }
                }
            }
            _ => {
                *last_error = Some(VmError::TooLarge);
                writeln!(serial, "ERR RUN.TEMP").ok();
            }
        }
    } else if let Some(app_id) = command
        .strip_prefix("RUN.APP ")
        .or_else(|| command.strip_prefix("run.app "))
    {
        let Some(slot) = registry.find(app_id) else {
            writeln!(serial, "ERR no-app").ok();
            return;
        };
        let _ = trace.teardown_services();
        trace.clear();
        trace.clear_timers();
        trace.reset_stack();
        trace.active_app = Some(AppRef::Persistent(slot));
        *vm = None;
        *vm_slot = None;
        *temp_app = TempApp::empty();
        match load_vm_for_app(
            AppRef::Persistent(slot),
            registry,
            app_storage,
            app_load_bytes,
            temp_app,
        ) {
            Ok(loaded) => {
                *vm = Some(loaded);
                *vm_slot = Some(slot);
            }
            Err(error) => {
                set_runtime_error(error, last_error, storage_error);
                writeln!(serial, "ERR RUN.APP {:?}", error).ok();
                return;
            }
        }
        match dispatch_loaded_vm(
            vm,
            AppRef::Persistent(slot),
            "app.start",
            registry,
            app_storage,
            trace,
        ) {
            Ok(()) => {
                process_pending_actions(
                    trace,
                    registry,
                    app_storage,
                    app_load_bytes,
                    temp_app,
                    vm,
                    vm_slot,
                    last_error,
                    storage_error,
                );
                writeln!(serial, "OK RUN.APP {app_id}").ok();
            }
            Err(error) => {
                set_runtime_error(error, last_error, storage_error);
                writeln!(serial, "ERR RUN.APP {:?}", error).ok();
            }
        }
    } else if let Some(rest) = command
        .strip_prefix("RUN.EVENT ")
        .or_else(|| command.strip_prefix("run.event "))
    {
        let Some((app_id, event)) = rest.split_once(' ') else {
            writeln!(serial, "ERR RUN.EVENT").ok();
            return;
        };
        let Some(slot) = registry.find(app_id) else {
            writeln!(serial, "ERR no-app").ok();
            return;
        };
        *temp_app = TempApp::empty();
        *vm = None;
        *vm_slot = None;
        if vm_slot != &Some(slot) || vm.is_none() {
            match load_vm_for_app(
                AppRef::Persistent(slot),
                registry,
                app_storage,
                app_load_bytes,
                temp_app,
            ) {
                Ok(loaded) => {
                    *vm = Some(loaded);
                    *vm_slot = Some(slot);
                }
                Err(error) => {
                    set_runtime_error(error, last_error, storage_error);
                    writeln!(serial, "ERR RUN.EVENT {:?}", error).ok();
                    return;
                }
            }
        }
        match dispatch_loaded_vm(
            vm,
            AppRef::Persistent(slot),
            event,
            registry,
            app_storage,
            trace,
        ) {
            Ok(()) => {
                process_pending_actions(
                    trace,
                    registry,
                    app_storage,
                    app_load_bytes,
                    temp_app,
                    vm,
                    vm_slot,
                    last_error,
                    storage_error,
                );
                writeln!(serial, "OK RUN.EVENT {app_id} {event}").ok();
            }
            Err(error) => {
                set_runtime_error(error, last_error, storage_error);
                writeln!(serial, "ERR RUN.EVENT {:?}", error).ok();
            }
        }
    } else if command == "APP.LIST" || command == "app.list" {
        writeln!(serial, "BEGIN APPS").ok();
        for (_, entry) in registry.iter() {
            writeln!(
                serial,
                "app={} len={} hash={:08x}",
                entry.name(),
                entry.len(),
                entry.hash()
            )
            .ok();
        }
        writeln!(serial, "END APPS").ok();
        writeln!(serial, "OK APP.LIST").ok();
    } else if let Some(key) = command
        .strip_prefix("KEY ")
        .or_else(|| command.strip_prefix("key "))
    {
        let event = if key == "SELECT" {
            "key.SELECT"
        } else if key == "BACK" {
            "key.BACK"
        } else {
            writeln!(serial, "ERR key").ok();
            return;
        };
        if let Some(app) = trace.top_app() {
            if vm.is_none() {
                match load_vm_for_app(app, registry, app_storage, app_load_bytes, temp_app) {
                    Ok(loaded) => {
                        *vm = Some(loaded);
                        *vm_slot = match app {
                            AppRef::Persistent(slot) => Some(slot),
                            AppRef::Temp => None,
                        };
                    }
                    Err(error) => {
                        set_runtime_error(error, last_error, storage_error);
                        writeln!(serial, "ERR key {:?}", error).ok();
                        return;
                    }
                }
            }
            match dispatch_loaded_vm(vm, app, event, registry, app_storage, trace) {
                Ok(()) => {
                    if trace.exited {
                        finish_current_exit(
                            trace,
                            registry,
                            app_storage,
                            app_load_bytes,
                            temp_app,
                            vm,
                            vm_slot,
                            last_error,
                            storage_error,
                        );
                    }
                    process_pending_actions(
                        trace,
                        registry,
                        app_storage,
                        app_load_bytes,
                        temp_app,
                        vm,
                        vm_slot,
                        last_error,
                        storage_error,
                    );
                    *last_error = None;
                    writeln!(serial, "OK key {key}").ok();
                }
                Err(error) => {
                    set_runtime_error(error, last_error, storage_error);
                    writeln!(serial, "ERR key {:?}", error).ok();
                }
            }
            return;
        }
        let fallback_app = (*vm_slot).map(AppRef::Persistent).or_else(|| {
            if temp_app.is_available() {
                Some(AppRef::Temp)
            } else {
                None
            }
        });
        match fallback_app {
            Some(app) => match dispatch_loaded_vm(vm, app, event, registry, app_storage, trace) {
                Ok(()) => {
                    *last_error = None;
                    writeln!(serial, "OK key {key}").ok();
                }
                Err(error) => {
                    set_runtime_error(error, last_error, storage_error);
                    writeln!(serial, "ERR key {:?}", error).ok();
                }
            },
            None => {
                writeln!(serial, "ERR no-vm").ok();
            }
        }
    } else if command == "state" {
        match vm.as_ref() {
            Some(active) => print_state(serial, active),
            None => {
                writeln!(serial, "ERR no-vm").ok();
            }
        }
    } else if command == "STATE.GET" {
        match vm.as_ref() {
            Some(active) => {
                writeln!(serial, "BEGIN STATE").ok();
                print_state(serial, active);
                writeln!(serial, "END STATE").ok();
                writeln!(serial, "OK STATE.GET").ok();
            }
            None => {
                writeln!(serial, "ERR no-vm").ok();
            }
        }
    } else if let Some(rest) = command.strip_prefix("STATE.IMPORT ") {
        match parse_install(rest) {
            Ok(request) if request.len <= STATE_IMPORT_CAP => {
                writeln!(serial, "READY STATE.IMPORT len={}", request.len).ok();
                let bytes = unsafe { &mut STATE_IMPORT_BYTES[..request.len] };
                let read = read_exact_timeout(serial, bytes, delay, INSTALL_TIMEOUT_MS);
                if read != request.len {
                    writeln!(
                        serial,
                        "ERR STATE.IMPORT timeout read={read} expected={}",
                        request.len
                    )
                    .ok();
                    return;
                }
                let actual_hash = fnv1a(bytes);
                if actual_hash != request.expected_hash {
                    writeln!(
                        serial,
                        "ERR STATE.IMPORT hash expected={:08x} actual={actual_hash:08x}",
                        request.expected_hash
                    )
                    .ok();
                    return;
                }
                match vm.as_mut() {
                    Some(active) => {
                        import_state(active, bytes);
                        writeln!(serial, "OK STATE.IMPORT hash={actual_hash:08x}").ok();
                    }
                    None => {
                        writeln!(serial, "ERR no-vm").ok();
                    }
                }
            }
            _ => {
                writeln!(serial, "ERR STATE.IMPORT").ok();
            }
        }
    } else if command == "trace" || command == "TRACE.GET" {
        if command == "TRACE.GET" {
            writeln!(serial, "BEGIN TRACE").ok();
        }
        trace.print(serial);
        if command == "TRACE.GET" {
            writeln!(serial, "END TRACE").ok();
            writeln!(serial, "OK TRACE.GET").ok();
        }
    } else if command == "OUTPUT.GET" {
        writeln!(serial, "BEGIN OUTPUT").ok();
        trace.print_output(serial);
        writeln!(serial, "END OUTPUT").ok();
        writeln!(serial, "OK OUTPUT.GET").ok();
    } else if command == "DRAWLOG.GET" {
        writeln!(serial, "BEGIN DRAWLOG").ok();
        trace.print_draw(serial);
        writeln!(serial, "END DRAWLOG").ok();
        writeln!(serial, "OK DRAWLOG.GET").ok();
    } else if command == "errors" || command == "ERRORS.GET" {
        match last_error {
            Some(error) => writeln!(serial, "last_error={:?}", error).ok(),
            None => writeln!(serial, "last_error=none").ok(),
        };
        match storage_error {
            Some(error) => writeln!(serial, "storage_error={:?}", error).ok(),
            None => writeln!(serial, "storage_error=none").ok(),
        };
        if command == "ERRORS.GET" {
            writeln!(serial, "OK ERRORS.GET").ok();
        }
    } else if command == "reset" || command == "RESET" {
        let _ = trace.teardown_services();
        trace.clear();
        trace.clear_timers();
        trace.reset_stack();
        *temp_app = TempApp::empty();
        *vm = None;
        *vm_slot = None;
        *last_error = None;
        boot_main(
            serial,
            registry,
            app_storage,
            app_load_bytes,
            temp_app,
            trace,
            vm,
            vm_slot,
            last_error,
            storage_error,
        );
        writeln!(serial, "OK reset").ok();
    } else if command == "STORAGE.FORMAT" || command == "storage.format" {
        match app_storage.format() {
            Ok(()) => {
                registry.clear();
                let _ = trace.teardown_services();
                trace.clear();
                trace.clear_timers();
                trace.reset_stack();
                *temp_app = TempApp::empty();
                *vm = None;
                *vm_slot = None;
                *last_error = None;
                *storage_error = None;
                writeln!(serial, "OK STORAGE.FORMAT").ok();
            }
            Err(error) => {
                *storage_error = Some(error);
                writeln!(serial, "ERR STORAGE.FORMAT {:?}", error).ok();
            }
        }
    } else {
        writeln!(serial, "ERR unknown-command").ok();
    }
}

fn read_exact_timeout(
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    out: &mut [u8],
    delay: &Delay,
    timeout_ms: u32,
) -> usize {
    let mut index = 0usize;
    let mut idle_ms = 0u32;
    while index < out.len() && idle_ms < timeout_ms {
        if let Ok(byte) = serial.read_byte() {
            out[index] = byte;
            index += 1;
            idle_ms = 0;
        } else {
            delay.delay_millis(1);
            idle_ms += 1;
        }
    }
    index
}
