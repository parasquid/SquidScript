use core::fmt::{self, Write};

use esp_hal::{delay::Delay, gpio::{Level, Output}, time::{Duration, Instant}, usb_serial_jtag::UsbSerialJtag};
use squidvm_core::{
    error::VmError,
    host::{DisplayLineOptions, DisplayRectOptions, DisplayTextOptions, TraceSink},
    limits::MAX_APP_BYTES,
    strings::StringResolver,
    value::Value,
};

use crate::dev_harness::{
    AppName, AppRegistry, AppSlot, AppStorage, AppStorageError, DevTimerEvent as TimerEvent,
};
use crate::storage::SQUIDFS_LEN;

use super::{
    lifecycle::{app_ref_available, run_app_event, set_runtime_error},
    log::{stable_trace, write_human_bytes, write_value, LogLine},
    vm::AppRef,
    ActiveVm, TempApp, MEMORY_AVAILABLE_BYTES,
};

const BREATH_STEPS: [u32; 9] = [0, 2, 7, 16, 35, 65, 84, 96, 100];
const PWM_PERIOD_US: u32 = 2_000;
const TRACE_CAP: usize = 24;
const OUTPUT_CAP: usize = 16;
const DRAW_CAP: usize = 32;
const STACK_CAP: usize = 4;
const TIMER_CAP: usize = 4;

pub struct RuntimeSink<'d> {
    pub(super) onboard_indicator: Output<'d>,
    pub(super) external_indicator: Output<'d>,
    pub(super) use_external_indicator: bool,
    pub(super) breathing_enabled: bool,
    pub(super) current_app: Option<AppRef>,
    pub(super) pending_launch: Option<AppName>,
    pub(super) pending_arm: Option<AppName>,
    pub(super) pending_disarm: Option<AppName>,
    pub(super) timers: [Option<TimerRegistration>; TIMER_CAP],
    pub(super) root_restart_pending: bool,
    pub(super) registration_mode: bool,
    pub(super) active_app: Option<AppRef>,
    pub(super) stack: [AppSlot; STACK_CAP],
    pub(super) stack_len: usize,
    pub(super) exited: bool,
    pub(super) in_exit_hook: bool,
    pub(super) entries: [&'static str; TRACE_CAP],
    pub(super) len: usize,
    pub(super) output: [LogLine; OUTPUT_CAP],
    pub(super) output_len: usize,
    pub(super) draw: [LogLine; DRAW_CAP],
    pub(super) draw_len: usize,
    pub(super) app_storage_used_bytes: usize,
}

impl<'d> RuntimeSink<'d> {
    pub fn new(onboard_indicator: Output<'d>, external_indicator: Output<'d>) -> Self {
        Self {
            onboard_indicator,
            external_indicator,
            use_external_indicator: false,
            breathing_enabled: true,
            current_app: None,
            pending_launch: None,
            pending_arm: None,
            pending_disarm: None,
            timers: [None; TIMER_CAP],
            root_restart_pending: false,
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

    pub fn set_app_storage_used(&mut self, used: usize) {
        self.app_storage_used_bytes = used;
    }

    pub fn breathe_once(&mut self, delay: &Delay) {
        if self.breathing_enabled {
            breathe_once(&mut self.onboard_indicator, delay);
        }
    }

    pub(super) fn clear(&mut self) {
        self.len = 0;
        self.output_len = 0;
        self.draw_len = 0;
    }

    pub(super) fn clear_timers(&mut self) {
        self.pending_launch = None;
        self.pending_arm = None;
        self.pending_disarm = None;
        self.timers = [None; TIMER_CAP];
        self.exited = false;
        self.registration_mode = false;
        self.root_restart_pending = false;
    }

    pub(super) fn reset_stack(&mut self) {
        self.stack_len = 0;
        self.active_app = None;
    }

    pub(super) fn push_return_target(&mut self, app: AppSlot) {
        if self.stack_len < self.stack.len() {
            self.stack[self.stack_len] = app;
            self.stack_len += 1;
        }
    }

    pub(super) fn pop_return_target(&mut self) -> Option<AppSlot> {
        if self.stack_len == 0 {
            return None;
        }
        self.stack_len -= 1;
        Some(self.stack[self.stack_len])
    }

    pub(super) fn remove_app_from_stack(&mut self, app: AppRef) {
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

    pub(super) fn top_app(&self) -> Option<AppRef> {
        self.active_app
    }

    pub(super) fn request_root_restart(&mut self) {
        self.root_restart_pending = true;
    }

    pub fn take_root_restart(&mut self) -> bool {
        let pending = self.root_restart_pending;
        self.root_restart_pending = false;
        pending
    }

    pub fn advance_time(
        &mut self,
        now: Instant,
        registry: &AppRegistry,
        app_storage: &mut impl AppStorage,
        app_load_bytes: &'static mut [u8; MAX_APP_BYTES],
        temp_app: &mut TempApp,
        vm: &mut Option<ActiveVm>,
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

    pub(super) fn print(&self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        for entry in self.entries.iter().take(self.len) {
            writeln!(serial, "trace={entry}").ok();
        }
    }

    pub(super) fn print_output(&self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        for entry in self.output.iter().take(self.output_len) {
            writeln!(serial, "output={}", entry.as_str()).ok();
        }
    }

    pub(super) fn print_draw(&self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        for entry in self.draw.iter().take(self.draw_len) {
            writeln!(serial, "draw={}", entry.as_str()).ok();
        }
    }

    pub(super) fn push_output(&mut self, line: LogLine) {
        if self.output_len < self.output.len() {
            self.output[self.output_len] = line;
            self.output_len += 1;
        }
    }

    pub(super) fn push_draw(&mut self, line: LogLine) {
        if self.draw_len < self.draw.len() {
            self.draw[self.draw_len] = line;
            self.draw_len += 1;
        }
    }

    fn write_indicator(&mut self, logical_value: bool) {
        self.breathing_enabled = false;
        if self.use_external_indicator {
            self.external_indicator
                .set_level(if logical_value { Level::High } else { Level::Low });
        } else {
            self.onboard_indicator
                .set_level(if logical_value { Level::Low } else { Level::High });
        }
    }

    fn read_indicator(&self) -> bool {
        if self.use_external_indicator {
            self.external_indicator.is_set_high()
        } else {
            !self.onboard_indicator.is_set_high()
        }
    }

    fn write_gpio(&mut self, name: &str, logical_value: bool) -> Result<(), VmError> {
        let raw_high = match name {
            "GPIO8" => logical_value,
            "GPIO10" => logical_value,
            _ => return Err(VmError::InvalidOperand),
        };
        self.breathing_enabled = false;
        match name {
            "GPIO8" => self
                .onboard_indicator
                .set_level(if raw_high { Level::High } else { Level::Low }),
            "GPIO10" => self
                .external_indicator
                .set_level(if raw_high { Level::High } else { Level::Low }),
            _ => return Err(VmError::InvalidOperand),
        }
        Ok(())
    }

    fn read_gpio(&self, name: &str) -> Result<bool, VmError> {
        match name {
            "GPIO8" => Ok(self.onboard_indicator.is_set_high()),
            "GPIO10" => Ok(self.external_indicator.is_set_high()),
            _ => Err(VmError::InvalidOperand),
        }
    }

    pub(super) fn register_timer(&mut self, registration: TimerRegistration) -> Result<(), VmError> {
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

    pub(super) fn remove_timers_for(&mut self, app: AppRef) {
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

    fn debug_print(&mut self, strings: &StringResolver<'_>, values: &[Value]) {
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

    fn draw_text(
        &mut self,
        strings: &StringResolver<'_>,
        text: Value,
        options: DisplayTextOptions<'_>,
    ) {
        let mut line = LogLine::new();
        write!(line, "text text=").ok();
        write_value(&mut line, strings, text).ok();
        write!(line, " x={} y={}", options.x, options.y).ok();
        self.push_draw(line);
    }

    fn draw_rect(&mut self, options: DisplayRectOptions<'_>) {
        let mut line = LogLine::new();
        write!(
            line,
            "rect x={} y={} w={} h={}",
            options.x, options.y, options.w, options.h
        )
        .ok();
        self.push_draw(line);
    }

    fn draw_line(&mut self, options: DisplayLineOptions<'_>) {
        let mut line = LogLine::new();
        write!(
            line,
            "line x1={} y1={} x2={} y2={}",
            options.x1, options.y1, options.x2, options.y2
        )
        .ok();
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

    fn service_indicator_write(&mut self, value: bool) -> Result<(), VmError> {
        self.write_indicator(value);
        Ok(())
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        let value = !self.read_indicator();
        self.write_indicator(value);
        Ok(())
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        Ok(self.read_indicator())
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

    fn system_storage_text(&mut self, name: &str, out: &mut dyn fmt::Write) -> Result<(), VmError> {
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
pub(super) struct TimerRegistration {
    pub(super) app: AppRef,
    pub(super) event: TimerEvent,
    pub(super) armed: bool,
    pub(super) repeating: bool,
    pub(super) interval: Duration,
    pub(super) next_due: Instant,
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
