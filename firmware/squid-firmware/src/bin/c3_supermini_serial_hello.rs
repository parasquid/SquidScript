#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use core::fmt::{self, Write};

use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    time::{Duration, Instant},
    usb_serial_jtag::UsbSerialJtag,
};
use esp_storage::FlashStorage;
use squid_firmware::{
    dev_harness::{
        AppName, AppRegistry, AppRegistryError, AppSlot, AppStorage, AppStorageError,
        DevTimerEvent as TimerEvent, APP_REGISTRY_CAP,
    },
    protocol::{fnv1a, parse_install},
    storage::{LittleFsAppStorage, SquidFlashRegion, SQUIDFS_LEN},
    vm::{Program, StringResolver, TraceSink, Value, Vm, VmError, MAX_APP_BYTES},
};

const BUILD_ID: &str = match option_env!("SQUID_FIRMWARE_BUILD_ID") {
    Some(value) => value,
    None => "dev-build",
};

const BREATH_STEPS: [u32; 9] = [0, 2, 7, 16, 35, 65, 84, 96, 100];
const PWM_PERIOD_US: u32 = 2_000;
const LINE_CAP: usize = 128;
const TRACE_CAP: usize = 24;
const OUTPUT_CAP: usize = 16;
const DRAW_CAP: usize = 32;
const LOG_LINE_CAP: usize = 80;
const STATE_IMPORT_CAP: usize = 512;
const INSTALL_TIMEOUT_MS: u32 = 2_000;
const STACK_CAP: usize = 4;
const TIMER_CAP: usize = 4;
const MEMORY_AVAILABLE_BYTES: usize = 311_416;
static mut APP_LOAD_BYTES: [u8; MAX_APP_BYTES] = [0; MAX_APP_BYTES];
static mut STATE_IMPORT_BYTES: [u8; STATE_IMPORT_CAP] = [0; STATE_IMPORT_CAP];

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let delay = Delay::new();
    let led = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());
    let flash = FlashStorage::new(peripherals.FLASH);
    let mut app_storage = LittleFsAppStorage::new(SquidFlashRegion::new(flash));
    let mut serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
    let mut line = LineBuffer::new();
    let mut registry = AppRegistry::new();
    let mut runtime = RuntimeSink::new(led);
    let mut vm: Option<Vm<'static>> = None;
    let mut vm_slot: Option<AppSlot> = None;
    let mut temp_app = TempApp::empty();
    let mut last_error: Option<VmError> = None;
    let mut storage_error: Option<AppStorageError> = None;

    match registry.load_from_storage(&mut app_storage, unsafe { &mut APP_LOAD_BYTES }) {
        Ok(_) => {}
        Err(error) => {
            storage_error = Some(storage_error_from_persistent(error));
        }
    }

    writeln!(serial, "SquidScript reference firmware").ok();
    writeln!(serial, "target=esp32c3-super-mini build={BUILD_ID}").ok();
    writeln!(serial, "type help").ok();

    loop {
        runtime.breathe_once(&delay);
        runtime.advance_time(
            Instant::now(),
            &registry,
            &mut app_storage,
            unsafe { &mut APP_LOAD_BYTES },
            &mut temp_app,
            &mut vm,
            &mut vm_slot,
            &mut last_error,
            &mut storage_error,
        );
        match serial.read_byte() {
            Ok(byte) => {
                if let Some(command) = line.push(byte) {
                    let command = trim_ascii(command);
                    if !command.is_empty() {
                        handle_command(
                            command,
                            &mut serial,
                            &delay,
                            &mut registry,
                            &mut app_storage,
                            unsafe { &mut APP_LOAD_BYTES },
                            &mut temp_app,
                            &mut runtime,
                            &mut vm,
                            &mut vm_slot,
                            &mut last_error,
                            &mut storage_error,
                        );
                    }
                }
            }
            Err(_) => {}
        }
    }
}

fn handle_command(
    command: &str,
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    delay: &Delay,
    registry: &mut AppRegistry,
    app_storage: &mut impl AppStorage,
    app_load_bytes: &'static mut [u8; MAX_APP_BYTES],
    temp_app: &mut TempApp,
    trace: &mut RuntimeSink<'_>,
    vm: &mut Option<Vm<'static>>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    trace.set_app_storage_used(registry_installed_bytes(registry));
    if command == "help" {
        writeln!(serial, "commands: HELLO INSTALL.APP <app-id> <len> <fnv32hex> RUN.TEMP <app-id> <len> <fnv32hex> RUN.APP <app-id> RUN.EVENT <app-id> <event> KEY SELECT APP.LIST RESOURCES.GET STATE.GET STATE.IMPORT <len> <fnv32hex> TRACE.GET OUTPUT.GET DRAWLOG.GET ERRORS.GET RESET STORAGE.FORMAT").ok();
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
            if storage_error.is_some() { "error" } else { "ok" }
        )
        .ok();
        writeln!(serial, "OK HELLO").ok();
    } else if command == "RESOURCES.GET" || command == "resources" {
        print_resources(serial, registry);
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
                let read =
                    read_exact_timeout(serial, &mut app_load_bytes[..len], delay, INSTALL_TIMEOUT_MS);
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
                let read =
                    read_exact_timeout(serial, &mut app_load_bytes[..len], delay, INSTALL_TIMEOUT_MS);
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
                trace.clear();
                trace.remove_timers_for(AppRef::Temp);
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
                match dispatch_loaded_vm(vm, AppRef::Temp, "app.start", trace) {
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
        trace.clear();
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
        match dispatch_loaded_vm(vm, AppRef::Persistent(slot), "app.start", trace) {
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
        match dispatch_loaded_vm(vm, AppRef::Persistent(slot), event, trace) {
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
            match dispatch_loaded_vm(vm, app, event, trace) {
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
        match vm.as_mut() {
            Some(active) => match active.dispatch(event, trace) {
                Ok(()) => {
                    *last_error = None;
                    writeln!(serial, "OK key {key}").ok();
                }
                Err(error) => {
                    *last_error = Some(error);
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
        trace.clear();
        trace.clear_timers();
        *temp_app = TempApp::empty();
        *vm = None;
        *last_error = None;
        writeln!(serial, "OK reset").ok();
    } else if command == "STORAGE.FORMAT" || command == "storage.format" {
        match app_storage.format() {
            Ok(()) => {
                registry.clear();
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

fn storage_error_from_persistent(
    error: squid_firmware::dev_harness::PersistentAppError,
) -> AppStorageError {
    match error {
        squid_firmware::dev_harness::PersistentAppError::Storage(error) => error,
        _ => AppStorageError::Io,
    }
}

fn registry_installed_bytes(registry: &AppRegistry) -> usize {
    registry.iter().map(|(_, entry)| entry.len()).sum()
}

fn app_storage_available_bytes(registry: &AppRegistry) -> usize {
    SQUIDFS_LEN.saturating_sub(registry_installed_bytes(registry))
}

fn print_resources(
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    registry: &AppRegistry,
) {
    let app_used = registry_installed_bytes(registry);
    writeln!(serial, "BEGIN RESOURCES").ok();
    writeln!(serial, "memory_available_bytes={MEMORY_AVAILABLE_BYTES}").ok();
    writeln!(serial, "app_storage_total_bytes={SQUIDFS_LEN}").ok();
    writeln!(serial, "app_storage_used_bytes={app_used}").ok();
    writeln!(
        serial,
        "app_storage_available_bytes={}",
        app_storage_available_bytes(registry)
    )
    .ok();
    writeln!(serial, "END RESOURCES").ok();
    writeln!(serial, "OK RESOURCES.GET").ok();
}

fn write_human_bytes(
    out: &mut dyn fmt::Write,
    label: &str,
    bytes: usize,
) -> Result<(), fmt::Error> {
    if bytes >= 1024 * 1024 {
        write!(out, "{label} {} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        write!(out, "{label} {} KiB", bytes / 1024)
    } else {
        write!(out, "{label} {bytes} B")
    }
}

fn import_state(vm: &mut Vm<'_>, bytes: &[u8]) {
    let Ok(text) = core::str::from_utf8(bytes) else {
        return;
    };
    for line in text.lines() {
        let Some((name, raw_value)) = line.split_once('=') else {
            continue;
        };
        if let Some(value) = parse_value(vm.program(), raw_value.trim()) {
            let _ = vm.set_state_value(name.trim(), value);
        }
    }
}

fn parse_value(program: &Program<'_>, input: &str) -> Option<Value> {
    if input == "null" {
        Some(Value::Null)
    } else if input == "true" {
        Some(Value::Bool(true))
    } else if input == "false" {
        Some(Value::Bool(false))
    } else if let Ok(value) = input.parse::<i32>() {
        Some(Value::I32(value))
    } else if let Some(text) = input.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        for id in 0..64u16 {
            if program.string(id).ok()? == text {
                return Some(Value::String(id));
            }
        }
        None
    } else {
        None
    }
}

fn print_state(serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>, vm: &Vm<'_>) {
    let strings = vm.string_resolver();
    for index in 0..vm.state_count() {
        let name = vm.state_name(index).unwrap_or("<bad-state>");
        let value = vm.state_at(index).unwrap_or(Value::Null);
        write!(serial, "{name}=").ok();
        print_value(serial, &strings, value);
        writeln!(serial).ok();
    }
    writeln!(serial, "exited={}", vm.exited()).ok();
}

fn print_value(
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    strings: &StringResolver<'_, '_>,
    value: Value,
) {
    match value {
        Value::Null => write!(serial, "null").ok(),
        Value::Bool(value) => write!(serial, "{value}").ok(),
        Value::I32(value) => write!(serial, "{value}").ok(),
        Value::String(_) | Value::RuntimeString(_) => write!(
            serial,
            "\"{}\"",
            strings.value_str(value).unwrap_or("<bad-string>")
        )
        .ok(),
    };
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

fn trim_ascii(input: &str) -> &str {
    input.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

struct LineBuffer {
    bytes: [u8; LINE_CAP],
    len: usize,
}

impl LineBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; LINE_CAP],
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) -> Option<&str> {
        if byte == b'\n' || byte == b'\r' {
            let line = core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("");
            self.len = 0;
            return Some(line);
        }
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppRef {
    Persistent(AppSlot),
    Temp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TempApp {
    name: AppName,
    len: usize,
    hash: u32,
    occupied: bool,
}

impl TempApp {
    const fn empty() -> Self {
        Self {
            name: AppName::empty(),
            len: 0,
            hash: 0,
            occupied: false,
        }
    }

    fn new(app_id: &str, len: usize, hash: u32) -> Result<Self, AppRegistryError> {
        Ok(Self {
            name: AppName::new(app_id)?,
            len,
            hash,
            occupied: true,
        })
    }

    fn is_available(&self) -> bool {
        self.occupied && self.len <= MAX_APP_BYTES
    }
}

struct RuntimeSink<'d> {
    status_led: Output<'d>,
    breathing_enabled: bool,
    current_app: Option<AppRef>,
    pending_launch: Option<AppName>,
    pending_arm: Option<AppName>,
    pending_disarm: Option<AppName>,
    timers: [Option<TimerRegistration>; TIMER_CAP],
    registration_mode: bool,
    active_app: Option<AppRef>,
    stack: [AppSlot; STACK_CAP],
    stack_len: usize,
    exited: bool,
    in_exit_hook: bool,
    entries: [&'static str; TRACE_CAP],
    len: usize,
    output: [LogLine; OUTPUT_CAP],
    output_len: usize,
    draw: [LogLine; DRAW_CAP],
    draw_len: usize,
    app_storage_used_bytes: usize,
}

impl<'d> RuntimeSink<'d> {
    fn new(status_led: Output<'d>) -> Self {
        Self {
            status_led,
            breathing_enabled: true,
            current_app: None,
            pending_launch: None,
            pending_arm: None,
            pending_disarm: None,
            timers: [None; TIMER_CAP],
            registration_mode: false,
            active_app: None,
            stack: [AppSlot(0); STACK_CAP],
            stack_len: 0,
            exited: false,
            in_exit_hook: false,
            entries: [""; TRACE_CAP],
            len: 0,
            output: [LogLine::new(); OUTPUT_CAP],
            output_len: 0,
            draw: [LogLine::new(); DRAW_CAP],
            draw_len: 0,
            app_storage_used_bytes: 0,
        }
    }

    fn set_app_storage_used(&mut self, used: usize) {
        self.app_storage_used_bytes = used;
    }

    fn breathe_once(&mut self, delay: &Delay) {
        if self.breathing_enabled {
            breathe_once(&mut self.status_led, delay);
        }
    }

    fn clear(&mut self) {
        self.len = 0;
        self.output_len = 0;
        self.draw_len = 0;
    }

    fn clear_timers(&mut self) {
        self.pending_launch = None;
        self.pending_arm = None;
        self.pending_disarm = None;
        self.timers = [None; TIMER_CAP];
        self.exited = false;
        self.registration_mode = false;
    }

    fn reset_stack(&mut self) {
        self.stack_len = 0;
        self.active_app = None;
    }

    fn push_return_target(&mut self, app: AppSlot) {
        if self.stack_len < self.stack.len() {
            self.stack[self.stack_len] = app;
            self.stack_len += 1;
        }
    }

    fn pop_return_target(&mut self) -> Option<AppSlot> {
        if self.stack_len == 0 {
            return None;
        }
        self.stack_len -= 1;
        Some(self.stack[self.stack_len])
    }

    fn remove_app_from_stack(&mut self, app: AppRef) {
        let AppRef::Persistent(app) = app else {
            return;
        };
        let mut write = 0usize;
        for read in 0..self.stack_len {
            if self.stack[read] != app {
                self.stack[write] = self.stack[read];
                write += 1;
            }
        }
        self.stack_len = write;
    }

    fn top_app(&self) -> Option<AppRef> {
        self.active_app
    }

    fn advance_time(
        &mut self,
        now: Instant,
        registry: &AppRegistry,
        app_storage: &mut impl AppStorage,
        app_load_bytes: &'static mut [u8; MAX_APP_BYTES],
        temp_app: &mut TempApp,
        vm: &mut Option<Vm<'static>>,
        vm_slot: &mut Option<AppSlot>,
        last_error: &mut Option<VmError>,
        storage_error: &mut Option<AppStorageError>,
    ) {
        for index in 0..self.timers.len() {
            let Some(mut timer) = self.timers[index] else {
                continue;
            };
            if now < timer.next_due {
                continue;
            }
            if !app_ref_available(timer.app, registry, temp_app) {
                continue;
            }
            let is_active = self.active_app == Some(timer.app);
            if !timer.armed && !is_active {
                continue;
            }
            if timer.armed && self.active_app == Some(timer.app) {
                continue;
            }
            let previous_active = self.active_app;
            if timer.armed {
                self.active_app = Some(timer.app);
            }
            *vm = None;
            *vm_slot = None;
            if timer.repeating {
                timer.next_due = now + timer.interval;
                self.timers[index] = Some(timer);
            } else {
                self.timers[index] = None;
            }
            match run_app_event(
                timer.app,
                timer.event.as_str(),
                registry,
                app_storage,
                app_load_bytes,
                temp_app,
                self,
            ) {
                Ok(()) => {
                    if self.exited {
                        self.active_app = previous_active;
                        self.exited = false;
                    }
                    *last_error = None;
                }
                Err(error) => {
                    set_runtime_error(error, last_error, storage_error);
                    if matches!(timer.app, AppRef::Temp) {
                        *temp_app = TempApp::empty();
                    }
                }
            }
            if storage_error.is_some() {
                return;
            }
        }
    }

    fn print(&self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        for entry in self.entries.iter().take(self.len) {
            writeln!(serial, "trace={entry}").ok();
        }
    }

    fn print_output(&self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        for entry in self.output.iter().take(self.output_len) {
            writeln!(serial, "output={}", entry.as_str()).ok();
        }
    }

    fn print_draw(&self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        for entry in self.draw.iter().take(self.draw_len) {
            writeln!(serial, "draw={}", entry.as_str()).ok();
        }
    }

    fn push_output(&mut self, line: LogLine) {
        if self.output_len < self.output.len() {
            self.output[self.output_len] = line;
            self.output_len += 1;
        }
    }

    fn push_draw(&mut self, line: LogLine) {
        if self.draw_len < self.draw.len() {
            self.draw[self.draw_len] = line;
            self.draw_len += 1;
        }
    }

    fn write_gpio(&mut self, name: &str, logical_value: bool) -> Result<(), VmError> {
        let raw_high = match name {
            "indicator.status_led" | "status_led" | "status" => !logical_value,
            "GPIO8" => logical_value,
            _ => return Err(VmError::InvalidOperand),
        };
        self.breathing_enabled = false;
        self.status_led
            .set_level(if raw_high { Level::High } else { Level::Low });
        Ok(())
    }

    fn read_gpio(&self, name: &str) -> Result<bool, VmError> {
        let raw_high = self.status_led.is_set_high();
        match name {
            "indicator.status_led" | "status_led" | "status" => Ok(!raw_high),
            "GPIO8" => Ok(raw_high),
            _ => Err(VmError::InvalidOperand),
        }
    }

    fn register_timer(&mut self, registration: TimerRegistration) -> Result<(), VmError> {
        for timer in &mut self.timers {
            if timer.map(|timer| (timer.app, timer.event))
                == Some((registration.app, registration.event))
            {
                *timer = Some(registration);
                return Ok(());
            }
        }
        for timer in &mut self.timers {
            if timer.is_none() {
                *timer = Some(registration);
                return Ok(());
            }
        }
        Err(VmError::TooLarge)
    }

    fn remove_timers_for(&mut self, app: AppRef) {
        for timer in &mut self.timers {
            if timer.map(|timer| timer.app) == Some(app) {
                *timer = None;
            }
        }
    }
}

impl TraceSink for RuntimeSink<'_> {
    fn trace(&mut self, message: &str) {
        if self.len < self.entries.len() {
            self.entries[self.len] = stable_trace(message);
            self.len += 1;
        }
    }

    fn debug_print(&mut self, strings: &StringResolver<'_, '_>, values: &[Value]) {
        let mut line = LogLine::new();
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                write!(line, " ").ok();
            }
            write_value(&mut line, strings, *value).ok();
        }
        self.push_output(line);
    }

    fn draw_clear(&mut self, color: &str) {
        let mut line = LogLine::new();
        write!(line, "clear color={color}").ok();
        self.push_draw(line);
    }

    fn draw_text(&mut self, strings: &StringResolver<'_, '_>, text: Value, x: i32, y: i32) {
        let mut line = LogLine::new();
        write!(line, "text text=").ok();
        write_value(&mut line, strings, text).ok();
        write!(line, " x={x} y={y}").ok();
        self.push_draw(line);
    }

    fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let mut line = LogLine::new();
        write!(line, "rect x={x} y={y} w={w} h={h}").ok();
        self.push_draw(line);
    }

    fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let mut line = LogLine::new();
        write!(line, "line x1={x1} y1={y1} x2={x2} y2={y2}").ok();
        self.push_draw(line);
    }

    fn hardware_gpio_write(&mut self, name: &str, value: bool) -> Result<(), VmError> {
        self.write_gpio(name, value)
    }

    fn hardware_gpio_toggle(&mut self, name: &str) -> Result<(), VmError> {
        let value = !self.read_gpio(name)?;
        self.write_gpio(name, value)
    }

    fn hardware_gpio_read(&mut self, name: &str) -> Result<bool, VmError> {
        self.read_gpio(name)
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        self.pending_launch = Some(AppName::new(app).map_err(|_| VmError::InvalidOperand)?);
        Ok(())
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        self.pending_arm = Some(AppName::new(app).map_err(|_| VmError::InvalidOperand)?);
        Ok(())
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        self.pending_disarm = Some(AppName::new(app).map_err(|_| VmError::InvalidOperand)?);
        Ok(())
    }

    fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
        if interval_ms <= 0 {
            return Err(VmError::InvalidOperand);
        }
        let Some(event) = TimerEvent::from_event(event) else {
            return Err(VmError::InvalidOperand);
        };
        self.register_timer(TimerRegistration {
            app: self.current_app.ok_or(VmError::InvalidOperand)?,
            event,
            armed: self.registration_mode,
            repeating: true,
            interval: Duration::from_micros((interval_ms as u64).saturating_mul(1000)),
            next_due: Instant::now()
                + Duration::from_micros((interval_ms as u64).saturating_mul(1000)),
        })?;
        Ok(())
    }

    fn service_timer_after(&mut self, event: &str, delay_ms: i32) -> Result<(), VmError> {
        if delay_ms <= 0 {
            return Err(VmError::InvalidOperand);
        }
        let Some(event) = TimerEvent::from_event(event) else {
            return Err(VmError::InvalidOperand);
        };
        self.register_timer(TimerRegistration {
            app: self.current_app.ok_or(VmError::InvalidOperand)?,
            event,
            armed: self.registration_mode,
            repeating: false,
            interval: Duration::from_micros((delay_ms as u64).saturating_mul(1000)),
            next_due: Instant::now()
                + Duration::from_micros((delay_ms as u64).saturating_mul(1000)),
        })?;
        Ok(())
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write_human_bytes(out, "RAM", MEMORY_AVAILABLE_BYTES).map_err(|_| VmError::InvalidOperand)
    }

    fn system_storage_text(
        &mut self,
        name: &str,
        out: &mut dyn fmt::Write,
    ) -> Result<(), VmError> {
        if name != "apps" {
            return Err(VmError::InvalidOperand);
        }
        write_human_bytes(
            out,
            "Apps",
            SQUIDFS_LEN.saturating_sub(self.app_storage_used_bytes),
        )
        .map_err(|_| VmError::InvalidOperand)
    }
}

#[derive(Clone, Copy)]
struct TimerRegistration {
    app: AppRef,
    event: TimerEvent,
    armed: bool,
    repeating: bool,
    interval: Duration,
    next_due: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeError {
    Vm(VmError),
    Storage(AppStorageError),
}

impl From<VmError> for RuntimeError {
    fn from(value: VmError) -> Self {
        Self::Vm(value)
    }
}

impl From<AppStorageError> for RuntimeError {
    fn from(value: AppStorageError) -> Self {
        Self::Storage(value)
    }
}

fn set_runtime_error(
    error: RuntimeError,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    match error {
        RuntimeError::Vm(error) => *last_error = Some(error),
        RuntimeError::Storage(error) => *storage_error = Some(error),
    }
}

fn app_ref_available(app: AppRef, registry: &AppRegistry, temp_app: &TempApp) -> bool {
    match app {
        AppRef::Persistent(slot) => registry.entry(slot).is_some(),
        AppRef::Temp => temp_app.is_available(),
    }
}

fn load_persistent_app<'a>(
    app: AppSlot,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &'a mut [u8; MAX_APP_BYTES],
) -> Result<&'a [u8], AppStorageError> {
    let Some(entry) = registry.entry(app) else {
        return Err(AppStorageError::NotFound);
    };
    let len = storage.read_app(entry.name(), app_load_bytes)?;
    Ok(&app_load_bytes[..len])
}

fn app_ref_bytes<'a>(
    app: AppRef,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &'a mut [u8; MAX_APP_BYTES],
    temp_app: &TempApp,
) -> Result<&'a [u8], AppStorageError> {
    match app {
        AppRef::Persistent(slot) => load_persistent_app(slot, registry, storage, app_load_bytes),
        AppRef::Temp => Ok(&app_load_bytes[..temp_app.len]),
    }
}

fn run_app_event(
    app: AppRef,
    event: &str,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &TempApp,
    trace: &mut RuntimeSink<'_>,
) -> Result<(), RuntimeError> {
    let program = Program::parse(app_ref_bytes(
        app,
        registry,
        storage,
        app_load_bytes,
        temp_app,
    )?)?;
    let previous = trace.current_app;
    trace.current_app = Some(app);
    let mut vm = Vm::new(program);
    let result = vm.dispatch(event, trace);
    if vm.exited() {
        trace.exited = true;
    }
    trace.current_app = previous;
    result.map_err(RuntimeError::Vm)
}

fn load_vm_for_app(
    app: AppRef,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &TempApp,
) -> Result<Vm<'static>, RuntimeError> {
    let bytes = app_ref_bytes(app, registry, storage, app_load_bytes, temp_app)?;
    let ptr = bytes.as_ptr();
    let len = bytes.len();
    // The firmware owns APP_LOAD_BYTES for the lifetime of the active VM and
    // explicitly drops the VM before reusing the buffer for another app.
    let stable = unsafe { core::slice::from_raw_parts(ptr, len) };
    Ok(Vm::new(Program::parse(stable)?))
}

fn dispatch_loaded_vm(
    vm: &mut Option<Vm<'static>>,
    app: AppRef,
    event: &str,
    trace: &mut RuntimeSink<'_>,
) -> Result<(), RuntimeError> {
    let Some(active) = vm.as_mut() else {
        return Err(RuntimeError::Vm(VmError::InvalidOperand));
    };
    let previous = trace.current_app;
    trace.current_app = Some(app);
    let result = active.dispatch(event, trace);
    if active.exited() {
        trace.exited = true;
    }
    trace.current_app = previous;
    result.map_err(RuntimeError::Vm)
}

fn dispatch_exit_hook(
    app: AppRef,
    trace: &mut RuntimeSink<'_>,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &TempApp,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    if trace.in_exit_hook {
        return;
    }
    trace.in_exit_hook = true;
    let result = run_app_event(
        app,
        "app.exit",
        registry,
        storage,
        app_load_bytes,
        temp_app,
        trace,
    );
    trace.in_exit_hook = false;
    trace.exited = false;
    match result {
        Ok(()) | Err(RuntimeError::Vm(VmError::HandlerNotFound)) => {}
        Err(error) => set_runtime_error(error, last_error, storage_error),
    }
}

fn finish_current_exit(
    trace: &mut RuntimeSink<'_>,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &mut TempApp,
    vm: &mut Option<Vm<'static>>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    *vm = None;
    *vm_slot = None;
    let Some(current) = trace.active_app else {
        trace.exited = false;
        return;
    };
    dispatch_exit_hook(
        current,
        trace,
        registry,
        storage,
        app_load_bytes,
        temp_app,
        last_error,
        storage_error,
    );
    trace.remove_timers_for(current);
    if matches!(current, AppRef::Temp) {
        *temp_app = TempApp::empty();
    }
    trace.exited = false;
    trace.active_app = trace
        .pop_return_target()
        .or_else(|| registry.find("main"))
        .map(AppRef::Persistent);
    if let Some(app) = trace.active_app {
        match load_vm_for_app(app, registry, storage, app_load_bytes, temp_app) {
            Ok(loaded) => {
                *vm = Some(loaded);
                *vm_slot = match app {
                    AppRef::Persistent(slot) => Some(slot),
                    AppRef::Temp => None,
                };
                if let Err(error) = dispatch_loaded_vm(vm, app, "app.start", trace) {
                    set_runtime_error(error, last_error, storage_error);
                }
            }
            Err(error) => set_runtime_error(error, last_error, storage_error),
        }
    }
}

fn process_pending_actions(
    trace: &mut RuntimeSink<'_>,
    registry: &AppRegistry,
    storage: &mut impl AppStorage,
    app_load_bytes: &mut [u8; MAX_APP_BYTES],
    temp_app: &mut TempApp,
    vm: &mut Option<Vm<'static>>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
    storage_error: &mut Option<AppStorageError>,
) {
    while let Some(app_name) = trace.pending_disarm.take() {
        let Some(app) = registry.find(app_name.as_str()) else {
            *last_error = Some(VmError::InvalidOperand);
            return;
        };
        trace.remove_timers_for(AppRef::Persistent(app));
    }
    while let Some(app_name) = trace.pending_arm.take() {
        let Some(app) = registry.find(app_name.as_str()) else {
            *last_error = Some(VmError::InvalidOperand);
            return;
        };
        trace.registration_mode = true;
        let result = run_app_event(
            AppRef::Persistent(app),
            "app.arm",
            registry,
            storage,
            app_load_bytes,
            temp_app,
            trace,
        );
        trace.registration_mode = false;
        if let Err(error) = result {
            set_runtime_error(error, last_error, storage_error);
            return;
        }
    }
    while let Some(app_name) = trace.pending_launch.take() {
        let Some(app) = registry.find(app_name.as_str()) else {
            *last_error = Some(VmError::InvalidOperand);
            return;
        };
        let app = AppRef::Persistent(app);
        if let Some(AppRef::Persistent(current)) = trace.active_app {
            trace.push_return_target(current);
        }
        let current = trace.active_app;
        if let Some(current) = current {
            *vm = None;
            *vm_slot = None;
            dispatch_exit_hook(
                current,
                trace,
                registry,
                storage,
                app_load_bytes,
                temp_app,
                last_error,
                storage_error,
            );
            trace.remove_timers_for(current);
        }
        trace.active_app = Some(app);
        *vm = None;
        *vm_slot = None;
        if let Err(error) = run_app_event(
            app,
            "app.start",
            registry,
            storage,
            app_load_bytes,
            temp_app,
            trace,
        ) {
            set_runtime_error(error, last_error, storage_error);
            return;
        }
        if trace.exited {
            finish_current_exit(
                trace,
                registry,
                storage,
                app_load_bytes,
                temp_app,
                vm,
                vm_slot,
                last_error,
                storage_error,
            );
        }
    }
}

#[derive(Clone, Copy)]
struct LogLine {
    bytes: [u8; LOG_LINE_CAP],
    len: usize,
}

impl LogLine {
    const fn new() -> Self {
        Self {
            bytes: [0; LOG_LINE_CAP],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("<bad-log>")
    }
}

impl Write for LogLine {
    fn write_str(&mut self, input: &str) -> fmt::Result {
        let remaining = self.bytes.len().saturating_sub(self.len);
        let bytes = input.as_bytes();
        let copy_len = remaining.min(bytes.len());
        self.bytes[self.len..self.len + copy_len].copy_from_slice(&bytes[..copy_len]);
        self.len += copy_len;
        Ok(())
    }
}

fn write_value(
    out: &mut impl Write,
    strings: &StringResolver<'_, '_>,
    value: Value,
) -> Result<(), fmt::Error> {
    match value {
        Value::Null => write!(out, "null"),
        Value::Bool(value) => write!(out, "{value}"),
        Value::I32(value) => write!(out, "{value}"),
        Value::String(_) | Value::RuntimeString(_) => {
            write!(out, "\"{}\"", strings.value_str(value).unwrap_or("<bad-string>"))
        }
    }
}

fn stable_trace(message: &str) -> &'static str {
    match message {
        "state.load" => "state.load",
        "state.save" => "state.save",
        "app.exit" => "app.exit",
        "app.start" => "app.start",
        "app.arm" => "app.arm",
        "key.SELECT" => "key.SELECT",
        "key.BACK" => "key.BACK",
        "timer.clock" => "timer.clock",
        "timer.break" => "timer.break",
        "timer.debug" => "timer.debug",
        "app.launch" => "app.launch",
        _ => "unknown",
    }
}

fn breathe_once(led: &mut Output<'_>, delay: &Delay) {
    for duty in BREATH_STEPS {
        pulse(led, delay, duty);
    }
    for duty in BREATH_STEPS.iter().rev().copied() {
        pulse(led, delay, duty);
    }
}

fn pulse(led: &mut Output<'_>, delay: &Delay, duty_percent: u32) {
    let on_us = PWM_PERIOD_US * duty_percent / 100;
    let off_us = PWM_PERIOD_US - on_us;
    if on_us > 0 {
        led.set_high();
        delay.delay_micros(on_us);
    }
    if off_us > 0 {
        led.set_low();
        delay.delay_micros(off_us);
    }
}

impl fmt::Debug for RuntimeSink<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().take(self.len))
            .finish()
    }
}
