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
use squid_firmware::{
    dev_harness::{AppName, AppRegistry, AppRegistryError, AppSlot, DevTimerEvent as TimerEvent, APP_REGISTRY_CAP},
    protocol::{fnv1a, parse_install},
    vm::{Program, TraceSink, Value, Vm, VmError, MAX_APP_BYTES},
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
static mut APP_BYTES: [[u8; MAX_APP_BYTES]; APP_REGISTRY_CAP] =
    [[0; MAX_APP_BYTES]; APP_REGISTRY_CAP];
static mut STATE_IMPORT_BYTES: [u8; STATE_IMPORT_CAP] = [0; STATE_IMPORT_CAP];

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let delay = Delay::new();
    let led = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());
    let mut serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
    let mut line = LineBuffer::new();
    let mut registry = AppRegistry::new();
    let mut runtime = RuntimeSink::new(led);
    let mut vm: Option<Vm<'static>> = None;
    let mut vm_slot: Option<AppSlot> = None;
    let mut last_error: Option<VmError> = None;

    writeln!(serial, "SquidScript reference firmware").ok();
    writeln!(serial, "target=esp32c3-super-mini build={BUILD_ID}").ok();
    writeln!(serial, "type help").ok();

    loop {
        runtime.breathe_once(&delay);
        runtime.advance_time(
            Instant::now(),
            &registry,
            unsafe { &APP_BYTES },
            &mut last_error,
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
                            unsafe { &mut APP_BYTES },
                            &mut runtime,
                            &mut vm,
                            &mut vm_slot,
                            &mut last_error,
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
    app_bytes: &'static mut [[u8; MAX_APP_BYTES]; APP_REGISTRY_CAP],
    trace: &mut RuntimeSink<'_>,
    vm: &mut Option<Vm<'static>>,
    vm_slot: &mut Option<AppSlot>,
    last_error: &mut Option<VmError>,
) {
    if command == "help" {
        writeln!(serial, "commands: HELLO INSTALL.APP <app-id> <len> <fnv32hex> RUN.APP <app-id> RUN.EVENT <app-id> <event> KEY SELECT APP.LIST STATE.GET STATE.IMPORT <len> <fnv32hex> TRACE.GET OUTPUT.GET DRAWLOG.GET ERRORS.GET RESET").ok();
    } else if command == "HELLO" || command == "hello" || command == "info" {
        writeln!(serial, "target=esp32c3-super-mini").ok();
        writeln!(serial, "build={BUILD_ID}").ok();
        writeln!(serial, "profile=dev").ok();
        writeln!(serial, "app_slots={APP_REGISTRY_CAP}").ok();
        writeln!(serial, "installed_apps={}", registry.iter().count()).ok();
        writeln!(serial, "vm_loaded={}", vm.is_some()).ok();
        writeln!(serial, "OK HELLO").ok();
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
                let Some(bytes) = app_bytes.get_mut(slot.0) else {
                    writeln!(serial, "ERR install.app").ok();
                    return;
                };
                writeln!(serial, "READY install.app app={app_id} len={len}").ok();
                let read = read_exact_timeout(serial, &mut bytes[..len], delay, INSTALL_TIMEOUT_MS);
                if read != len {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(serial, "ERR install.app timeout read={read} expected={len}").ok();
                    return;
                }
                let actual_hash = fnv1a(&bytes[..len]);
                if actual_hash == request.expected_hash {
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
                    trace.clear();
                    trace.clear_timers();
                    trace.reset_stack();
                    *last_error = None;
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
        trace.push_app(slot);
        match run_app_event(slot, "app.start", registry, app_bytes, trace) {
            Ok(()) => {
                process_pending_actions(trace, registry, app_bytes, last_error);
                writeln!(serial, "OK RUN.APP {app_id}").ok();
            }
            Err(error) => {
                *last_error = Some(error);
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
        let bytes = registry_app_bytes(slot, registry, app_bytes);
        if vm_slot != &Some(slot) {
            *vm = None;
        }
        if vm.is_none() {
            match Program::parse(bytes) {
                Ok(program) => {
                    *vm = Some(Vm::new(program));
                    *vm_slot = Some(slot);
                }
                Err(error) => {
                    *last_error = Some(error);
                    writeln!(serial, "ERR load {:?}", error).ok();
                    return;
                }
            }
        }
        let previous = trace.current_app;
        trace.current_app = Some(slot);
        let result = vm.as_mut().unwrap().dispatch(event, trace);
        trace.current_app = previous;
        match result {
            Ok(()) => {
                process_pending_actions(trace, registry, app_bytes, last_error);
                writeln!(serial, "OK RUN.EVENT {app_id} {event}").ok();
            }
            Err(error) => {
                *last_error = Some(error);
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
            match run_app_event(app, event, registry, app_bytes, trace) {
                Ok(()) => {
                    if trace.exited {
                        trace.pop_app();
                        trace.exited = false;
                    }
                    process_pending_actions(trace, registry, app_bytes, last_error);
                    *last_error = None;
                    writeln!(serial, "OK key {key}").ok();
                }
                Err(error) => {
                    *last_error = Some(error);
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
        if command == "ERRORS.GET" {
            writeln!(serial, "OK ERRORS.GET").ok();
        }
    } else if command == "reset" || command == "RESET" {
        trace.clear();
        trace.clear_timers();
        *vm = None;
        *last_error = None;
        writeln!(serial, "OK reset").ok();
    } else {
        writeln!(serial, "ERR unknown-command").ok();
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
    for index in 0..vm.state_count() {
        let name = vm.state_name(index).unwrap_or("<bad-state>");
        let value = vm.state_at(index).unwrap_or(Value::Null);
        write!(serial, "{name}=").ok();
        print_value(serial, vm.program(), value);
        writeln!(serial).ok();
    }
    writeln!(serial, "exited={}", vm.exited()).ok();
}

fn print_value(
    serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
    program: &Program<'_>,
    value: Value,
) {
    match value {
        Value::Null => write!(serial, "null").ok(),
        Value::Bool(value) => write!(serial, "{value}").ok(),
        Value::I32(value) => write!(serial, "{value}").ok(),
        Value::String(id) => write!(
            serial,
            "\"{}\"",
            program.string(id).unwrap_or("<bad-string>")
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

struct RuntimeSink<'d> {
    status_led: Output<'d>,
    breathing_enabled: bool,
    current_app: Option<AppSlot>,
    pending_launch: Option<AppName>,
    pending_arm: Option<AppName>,
    pending_disarm: Option<AppName>,
    timers: [Option<TimerRegistration>; TIMER_CAP],
    registration_mode: bool,
    stack: [AppSlot; STACK_CAP],
    stack_len: usize,
    exited: bool,
    entries: [&'static str; TRACE_CAP],
    len: usize,
    output: [LogLine; OUTPUT_CAP],
    output_len: usize,
    draw: [LogLine; DRAW_CAP],
    draw_len: usize,
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
            stack: [AppSlot(0); STACK_CAP],
            stack_len: 0,
            exited: false,
            entries: [""; TRACE_CAP],
            len: 0,
            output: [LogLine::new(); OUTPUT_CAP],
            output_len: 0,
            draw: [LogLine::new(); DRAW_CAP],
            draw_len: 0,
        }
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
    }

    fn push_app(&mut self, app: AppSlot) {
        if self.stack_len < self.stack.len() {
            self.stack[self.stack_len] = app;
            self.stack_len += 1;
        }
    }

    fn pop_app(&mut self) {
        if self.stack_len > 1 {
            let app = self.stack[self.stack_len - 1];
            self.stack_len -= 1;
            for timer in &mut self.timers {
                if let Some(mut registration) = *timer {
                    if registration.armed && registration.app == app {
                        registration.next_due = Instant::now() + registration.interval;
                        *timer = Some(registration);
                    }
                }
            }
        }
    }

    fn top_app(&self) -> Option<AppSlot> {
        if self.stack_len == 0 {
            None
        } else {
            Some(self.stack[self.stack_len - 1])
        }
    }

    fn advance_time(
        &mut self,
        now: Instant,
        registry: &AppRegistry,
        app_bytes: &'static [[u8; MAX_APP_BYTES]; APP_REGISTRY_CAP],
        last_error: &mut Option<VmError>,
    ) {
        for index in 0..self.timers.len() {
            let Some(mut timer) = self.timers[index] else {
                continue;
            };
            if now < timer.next_due {
                continue;
            }
            timer.next_due = now + timer.interval;
            self.timers[index] = Some(timer);
            if registry.entry(timer.app).is_none() {
                continue;
            }
            let is_top = self.top_app() == Some(timer.app);
            if !timer.armed && !is_top {
                continue;
            }
            if timer.armed && self.stack[..self.stack_len].contains(&timer.app) {
                continue;
            }
            if !is_top {
                self.push_app(timer.app);
            }
            match run_app_event(timer.app, timer.event.as_str(), registry, app_bytes, self) {
                Ok(()) => {
                    if self.exited {
                        self.pop_app();
                        self.exited = false;
                    }
                    *last_error = None;
                }
                Err(error) => *last_error = Some(error),
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

    fn remove_timers_for(&mut self, app: AppSlot) {
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

    fn debug_print(&mut self, program: &Program<'_>, values: &[Value]) {
        let mut line = LogLine::new();
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                write!(line, " ").ok();
            }
            write_value(&mut line, program, *value).ok();
        }
        self.push_output(line);
    }

    fn draw_clear(&mut self, color: &str) {
        let mut line = LogLine::new();
        write!(line, "clear color={color}").ok();
        self.push_draw(line);
    }

    fn draw_text(&mut self, program: &Program<'_>, text: Value, x: i32, y: i32) {
        let mut line = LogLine::new();
        write!(line, "text text=").ok();
        write_value(&mut line, program, text).ok();
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

    fn event_add_source(&mut self, event: &str, every_ms: Option<i32>) -> Result<(), VmError> {
        let Some(interval_ms) = every_ms else {
            return Err(VmError::InvalidOperand);
        };
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
            interval: Duration::from_micros((interval_ms as u64).saturating_mul(1000)),
            next_due: Instant::now()
                + Duration::from_micros((interval_ms as u64).saturating_mul(1000)),
        })?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct TimerRegistration {
    app: AppSlot,
    event: TimerEvent,
    armed: bool,
    interval: Duration,
    next_due: Instant,
}

fn registry_app_bytes(
    app: AppSlot,
    registry: &AppRegistry,
    bytes: &'static [[u8; MAX_APP_BYTES]; APP_REGISTRY_CAP],
) -> &'static [u8] {
    let len = registry.len_for_slot(app).unwrap_or(0);
    &bytes[app.0][..len]
}

fn run_app_event(
    app: AppSlot,
    event: &str,
    registry: &AppRegistry,
    app_bytes: &'static [[u8; MAX_APP_BYTES]; APP_REGISTRY_CAP],
    trace: &mut RuntimeSink<'_>,
) -> Result<(), VmError> {
    let program = Program::parse(registry_app_bytes(app, registry, app_bytes))?;
    let previous = trace.current_app;
    trace.current_app = Some(app);
    let mut vm = Vm::new(program);
    let result = vm.dispatch(event, trace);
    if vm.exited() {
        trace.exited = true;
    }
    trace.current_app = previous;
    result
}

fn process_pending_actions(
    trace: &mut RuntimeSink<'_>,
    registry: &AppRegistry,
    app_bytes: &'static [[u8; MAX_APP_BYTES]; APP_REGISTRY_CAP],
    last_error: &mut Option<VmError>,
) {
    while let Some(app_name) = trace.pending_disarm.take() {
        let Some(app) = registry.find(app_name.as_str()) else {
            *last_error = Some(VmError::InvalidOperand);
            return;
        };
        trace.remove_timers_for(app);
    }
    while let Some(app_name) = trace.pending_arm.take() {
        let Some(app) = registry.find(app_name.as_str()) else {
            *last_error = Some(VmError::InvalidOperand);
            return;
        };
        trace.registration_mode = true;
        let result = run_app_event(app, "app.arm", registry, app_bytes, trace);
        trace.registration_mode = false;
        if let Err(error) = result {
            *last_error = Some(error);
            return;
        }
    }
    while let Some(app_name) = trace.pending_launch.take() {
        let Some(app) = registry.find(app_name.as_str()) else {
            *last_error = Some(VmError::InvalidOperand);
            return;
        };
        trace.push_app(app);
        if let Err(error) = run_app_event(app, "app.start", registry, app_bytes, trace) {
            *last_error = Some(error);
            return;
        }
        if trace.exited {
            trace.pop_app();
            trace.exited = false;
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
    program: &Program<'_>,
    value: Value,
) -> Result<(), fmt::Error> {
    match value {
        Value::Null => write!(out, "null"),
        Value::Bool(value) => write!(out, "{value}"),
        Value::I32(value) => write!(out, "{value}"),
        Value::String(id) => write!(out, "\"{}\"", program.string(id).unwrap_or("<bad-string>")),
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
