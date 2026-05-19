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
    usb_serial_jtag::UsbSerialJtag,
};
use squid_firmware::{
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
const INSTALL_TIMEOUT_MS: u32 = 2_000;

static mut APP_BYTES: [u8; MAX_APP_BYTES] = [0; MAX_APP_BYTES];

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let delay = Delay::new();
    let mut led = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());
    let mut serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
    let mut line = LineBuffer::new();
    let mut trace = TraceLog::new();
    let mut app_len = 0usize;
    let mut vm: Option<Vm<'static>> = None;
    let mut last_error: Option<VmError> = None;

    writeln!(serial, "SquidScript reference firmware").ok();
    writeln!(serial, "target=esp32c3-super-mini build={BUILD_ID}").ok();
    writeln!(serial, "type help").ok();

    loop {
        breathe_once(&mut led, &delay);
        match serial.read_byte() {
            Ok(byte) => {
                if let Some(command) = line.push(byte) {
                    let command = trim_ascii(command);
                    if !command.is_empty() {
                        handle_command(
                            command,
                            &mut serial,
                            &delay,
                            &mut trace,
                            &mut app_len,
                            &mut vm,
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
    trace: &mut TraceLog,
    app_len: &mut usize,
    vm: &mut Option<Vm<'static>>,
    last_error: &mut Option<VmError>,
) {
    if command == "help" {
        writeln!(serial, "commands: help info install <len> <fnv32hex> run key SELECT key BACK state trace errors reset").ok();
    } else if command == "info" {
        writeln!(serial, "target=esp32c3-super-mini").ok();
        writeln!(serial, "build={BUILD_ID}").ok();
        writeln!(serial, "app_len={}", *app_len).ok();
        writeln!(serial, "vm_loaded={}", vm.is_some()).ok();
    } else if let Some(rest) = command.strip_prefix("install ") {
        match parse_install(rest) {
            Ok(request) if request.len <= MAX_APP_BYTES => {
                let len = request.len;
                writeln!(serial, "READY install len={len}").ok();
                let bytes = unsafe { &mut APP_BYTES[..len] };
                let read = read_exact_timeout(serial, bytes, delay, INSTALL_TIMEOUT_MS);
                if read != len {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(serial, "ERR install timeout read={read} expected={len}").ok();
                    return;
                }
                let actual_hash = fnv1a(bytes);
                if actual_hash == request.expected_hash {
                    *app_len = len;
                    *vm = None;
                    *last_error = None;
                    writeln!(serial, "OK install hash={actual_hash:08x}").ok();
                } else {
                    *last_error = Some(VmError::InvalidHeader);
                    writeln!(
                        serial,
                        "ERR install hash expected={:08x} actual={actual_hash:08x}",
                        request.expected_hash
                    )
                    .ok();
                }
            }
            _ => {
                *last_error = Some(VmError::TooLarge);
                writeln!(serial, "ERR install").ok();
            }
        }
    } else if command == "run" {
        if *app_len == 0 {
            writeln!(serial, "ERR no-app").ok();
            return;
        }
        let bytes: &'static [u8] = unsafe { &APP_BYTES[..*app_len] };
        match Program::parse(bytes) {
            Ok(program) => {
                let mut next_vm = Vm::new(program);
                trace.clear();
                match next_vm.dispatch("onStart", trace) {
                    Ok(()) => {
                        *vm = Some(next_vm);
                        *last_error = None;
                        writeln!(serial, "OK run").ok();
                    }
                    Err(error) => {
                        *last_error = Some(error);
                        writeln!(serial, "ERR run {:?}", error).ok();
                    }
                }
            }
            Err(error) => {
                *last_error = Some(error);
                writeln!(serial, "ERR load {:?}", error).ok();
            }
        }
    } else if let Some(key) = command.strip_prefix("key ") {
        let event = if key == "SELECT" {
            "onKey.SELECT"
        } else if key == "BACK" {
            "onKey.BACK"
        } else {
            writeln!(serial, "ERR key").ok();
            return;
        };
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
    } else if command == "trace" {
        trace.print(serial);
    } else if command == "errors" {
        match last_error {
            Some(error) => writeln!(serial, "last_error={:?}", error).ok(),
            None => writeln!(serial, "last_error=none").ok(),
        };
    } else if command == "reset" {
        trace.clear();
        *vm = None;
        *last_error = None;
        writeln!(serial, "OK reset").ok();
    } else {
        writeln!(serial, "ERR unknown-command").ok();
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

struct TraceLog {
    entries: [&'static str; TRACE_CAP],
    len: usize,
}

impl TraceLog {
    const fn new() -> Self {
        Self {
            entries: [""; TRACE_CAP],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn print(&self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        for entry in self.entries.iter().take(self.len) {
            writeln!(serial, "trace={entry}").ok();
        }
    }
}

impl TraceSink for TraceLog {
    fn trace(&mut self, message: &str) {
        if self.len < self.entries.len() {
            self.entries[self.len] = stable_trace(message);
            self.len += 1;
        }
    }
}

fn stable_trace(message: &str) -> &'static str {
    match message {
        "onStart" => "onStart",
        "onKey.SELECT" => "onKey.SELECT",
        "onKey.BACK" => "onKey.BACK",
        "state.load" => "state.load",
        "state.save" => "state.save",
        "app.exit" => "app.exit",
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

impl fmt::Debug for TraceLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().take(self.len))
            .finish()
    }
}
