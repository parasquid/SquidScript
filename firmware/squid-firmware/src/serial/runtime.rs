use core::fmt::{self, Write};

use esp_hal::{
    gpio::{Level, Output},
    ledc::{channel::Channel, channel::ChannelIFace, LowSpeed},
    time::{Duration, Instant},
    usb_serial_jtag::UsbSerialJtag,
};
use squidvm_core::{
    error::VmError,
    host::{
        DisplayLineOptions, DisplayRectOptions, DisplayTextOptions, TraceSink, WifiActionResult,
        WifiApIp, WifiBackend, WifiScanResult, WifiStatus,
    },
    limits::MAX_APP_BYTES,
    strings::StringResolver,
    value::Value,
};

use crate::dev_harness::{
    AppName, AppRegistry, AppSlot, AppStorage, AppStorageError, DevTimerEvent as TimerEvent,
};
use crate::kernel::{
    write_ram_diagnostics_text, DisplayCommand, DisplayService, IndicatorAction, IndicatorService,
    LifecycleService, ServiceError, TimerCommand, TimerRegistration, TimerService, WifiCachedStatus,
    WifiCommand, WifiService, WifiState, WifiStationProfileStore, INDICATOR_BREATH_SEGMENT_MS,
};
use crate::storage::SQUIDFS_LEN;

use super::{
    lifecycle::{app_ref_available, run_app_event, set_runtime_error},
    log::{stable_trace, write_human_bytes, write_value, LogLine},
    ram::live_ram_diagnostics,
    vm::{AppRef, RuntimeError},
    ActiveVm, FirmwareWifiBackend, TempApp,
};

const INDICATOR_QUEUE_CAP: usize = 8;
const TRACE_CAP: usize = 24;
const OUTPUT_CAP: usize = 16;
const DRAW_CAP: usize = 32;
const STACK_CAP: usize = 4;
const LIFECYCLE_COMMAND_CAP: usize = 8;
const TIMER_CAP: usize = 4;
const TIMER_COMMAND_CAP: usize = 8;
const TIMER_DUE_CAP: usize = 4;
const WIFI_COMMAND_CAP: usize = 8;
const WIFI_PROFILE_NAME_CAP: usize = 16;
const WIFI_PROFILE_SSID_CAP: usize = 32;
const WIFI_PROFILE_PASSWORD_CAP: usize = 64;

pub struct RuntimeSink<'d> {
    onboard_indicator: OnboardIndicator<'d>,
    pub(super) external_indicator: Output<'d>,
    pub(super) use_external_indicator: bool,
    indicator_service: IndicatorService<INDICATOR_QUEUE_CAP>,
    next_indicator_breath_step: Option<Instant>,
    pub(super) current_app: Option<AppRef>,
    pub(super) lifecycle_service: LifecycleService<AppName, TimerEvent, LIFECYCLE_COMMAND_CAP>,
    pub(super) timer_service:
        TimerService<AppRef, TimerEvent, TIMER_CAP, TIMER_COMMAND_CAP, TIMER_DUE_CAP>,
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
    pub(super) display_service: DisplayService<LogLine, DRAW_CAP>,
    pub(super) app_storage_used_bytes: usize,
    pub(super) wifi_service: WifiService<WIFI_COMMAND_CAP>,
    pub(super) wifi: FirmwareWifiBackend<'d>,
    wifi_profiles:
        WifiStationProfileStore<WIFI_PROFILE_NAME_CAP, WIFI_PROFILE_SSID_CAP, WIFI_PROFILE_PASSWORD_CAP>,
}

impl<'d> RuntimeSink<'d> {
    pub fn new(
        onboard_indicator: OnboardIndicator<'d>,
        external_indicator: Output<'d>,
        wifi: FirmwareWifiBackend<'d>,
    ) -> Self {
        Self {
            onboard_indicator,
            external_indicator,
            use_external_indicator: false,
            indicator_service: IndicatorService::new_breathing(),
            next_indicator_breath_step: None,
            current_app: None,
            lifecycle_service: LifecycleService::new(),
            timer_service: TimerService::new(),
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
            display_service: DisplayService::new(),
            app_storage_used_bytes: 0,
            wifi_service: WifiService::new(WifiState::Idle),
            wifi,
            wifi_profiles: WifiStationProfileStore::new(),
        }
    }

    pub fn set_app_storage_used(&mut self, used: usize) {
        self.app_storage_used_bytes = used;
    }

    pub fn poll_wifi(&mut self) {
        self.wifi_service.enqueue(WifiCommand::Poll).ok();
        while let Some(command) = self.wifi_service.pop_command() {
            if let WifiCommand::Poll = command {
                self.wifi.poll();
            }
        }
    }

    pub(super) fn set_wifi_profile(
        &mut self,
        profile: &str,
        ssid: &[u8],
        password: &[u8],
    ) -> Result<(), VmError> {
        self.wifi_profiles
            .set(profile, ssid, password)
            .map_err(|_| VmError::InvalidOperand)
    }

    pub fn poll_indicator(&mut self) {
        while let Some(action) = self.indicator_service.pop_action() {
            self.apply_indicator_action(action);
        }

        let now = Instant::now();
        if self.next_indicator_breath_step.is_some_and(|due| now < due) {
            return;
        }

        if let Some(action) = self.indicator_service.next_breath_action() {
            self.apply_indicator_action(action);
            self.next_indicator_breath_step =
                Some(now + Duration::from_millis(INDICATOR_BREATH_SEGMENT_MS));
        }
    }

    pub(super) fn clear(&mut self) {
        self.len = 0;
        self.output_len = 0;
        self.display_service.clear();
    }

    pub(super) fn clear_timers(&mut self) {
        self.lifecycle_service.clear();
        self.timer_service.clear_now();
        self.exited = false;
        self.registration_mode = false;
    }

    pub(super) fn teardown_services(&mut self) -> Result<(), VmError> {
        self.wifi_service.enqueue(WifiCommand::Teardown).ok();
        self.wifi_service.set_cached_status(WifiCachedStatus::new(WifiState::Stopping));
        let _ = self.wifi_service.pop_command();
        if self.wifi.teardown()? {
            self.trace("wifi.stopAP");
        }
        self.wifi_service
            .set_cached_status(WifiCachedStatus::new(WifiState::Stopped));
        Ok(())
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
        self.lifecycle_service.root_restart().ok();
    }

    pub fn take_root_restart(&mut self) -> bool {
        self.lifecycle_service.take_root_restart()
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
        let now_ms = now.duration_since_epoch().as_millis();
        if let Err(error) = self.timer_service.step(now_ms, self.active_app) {
            set_runtime_error(
                RuntimeError::Vm(service_error_to_vm_error(error)),
                last_error,
                storage_error,
            );
            return;
        }

        while let Some(timer) = self.timer_service.pop_due() {
            if !app_ref_available(timer.app, registry, temp_app) {
                self.timer_service.remove_app_now(timer.app);
                continue;
            }
            let previous_active = self.active_app;
            if timer.armed {
                self.active_app = Some(timer.app);
            }
            *vm = None;
            *vm_slot = None;
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
        for index in 0..self.display_service.command_count() {
            let Some(DisplayCommand::Draw(entry)) = self.display_service.command_at(index) else {
                continue;
            };
            writeln!(serial, "draw={}", entry.as_str()).ok();
        }
    }

    pub(super) fn print_wifi_status(&mut self, serial: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
        match self.wifi.status() {
            Ok(status) => {
                self.wifi_service
                    .set_cached_status(wifi_cached_status(&status));
                writeln!(serial, "state={}", status.state).ok();
                writeln!(serial, "backend={}", status.backend).ok();
                writeln!(serial, "active={}", status.active).ok();
                writeln!(serial, "mode={}", status.mode.unwrap_or("none")).ok();
                writeln!(serial, "ssid={}", status.ssid.unwrap_or("")).ok();
                writeln!(serial, "ip={}", status.ip_address.unwrap_or("")).ok();
                writeln!(serial, "clients={}", status.clients).ok();
                writeln!(serial, "error={}", status.error.unwrap_or("")).ok();
                writeln!(serial, "driver_started={}", status.driver_started).ok();
                writeln!(serial, "configured={}", status.configured).ok();
                writeln!(serial, "driver_mode={}", status.driver_mode.unwrap_or("none")).ok();
                writeln!(serial, "channel={}", status.channel).ok();
                writeln!(serial, "event_ap_start={}", status.ap_start_events).ok();
                writeln!(serial, "event_ap_stop={}", status.ap_stop_events).ok();
                writeln!(serial, "event_ap_probe={}", status.probe_events).ok();
                writeln!(
                    serial,
                    "event_ap_sta_connected={}",
                    status.sta_connected_events
                )
                .ok();
                writeln!(
                    serial,
                    "event_ap_sta_disconnected={}",
                    status.sta_disconnected_events
                )
                .ok();
                writeln!(
                    serial,
                    "last_backend_code={}",
                    status.last_backend_code.unwrap_or("")
                )
                .ok();
                writeln!(serial, "profile={}", status.profile.unwrap_or("")).ok();
                writeln!(serial, "connected={}", status.connected).ok();
                writeln!(serial, "scan_matches={}", status.scan_matches).ok();
                writeln!(serial, "rssi={}", status.rssi).ok();
                writeln!(serial, "auth={}", status.auth.unwrap_or("")).ok();
                writeln!(serial, "bssid={}", status.bssid.unwrap_or("")).ok();
                writeln!(
                    serial,
                    "disconnect_reason={}",
                    status.disconnect_reason.unwrap_or("")
                )
                .ok();
                writeln!(
                    serial,
                    "disconnect_reason_code={}",
                    status.disconnect_reason_code
                )
                .ok();
            }
            Err(error) => {
                writeln!(serial, "ERR WIFI.STATUS {:?}", error).ok();
                return;
            }
        }

        match self.wifi.ap_ip() {
            Ok(ip) => {
                writeln!(serial, "ap_ip={}", ip.ip.unwrap_or("")).ok();
                writeln!(serial, "ap_gw={}", ip.gw.unwrap_or("")).ok();
                writeln!(serial, "ap_netmask={}", ip.netmask.unwrap_or("")).ok();
                writeln!(serial, "ap_error={}", ip.error.unwrap_or("")).ok();
            }
            Err(error) => {
                writeln!(serial, "ERR WIFI.APIP {:?}", error).ok();
            }
        }

        self.wifi.write_driver_diagnostics(serial);
    }

    pub(super) fn push_output(&mut self, line: LogLine) {
        if self.output_len < self.output.len() {
            self.output[self.output_len] = line;
            self.output_len += 1;
        }
    }

    pub(super) fn push_draw(&mut self, line: LogLine) {
        self.display_service
            .enqueue(DisplayCommand::Draw(line))
            .ok();
    }

    fn write_indicator(&mut self, logical_value: bool) -> Result<(), VmError> {
        self.indicator_service
            .write(logical_value)
            .map_err(service_error_to_vm_error)
    }

    fn apply_indicator_action(&mut self, action: IndicatorAction) {
        match action {
            IndicatorAction::SetBrightness(brightness) => {
                self.write_indicator_brightness(brightness)
            }
        }
    }

    fn write_indicator_brightness(&mut self, brightness: u8) {
        if self.use_external_indicator {
            self.external_indicator.set_level(if brightness > 0 {
                Level::High
            } else {
                Level::Low
            });
        } else {
            self.onboard_indicator.write_brightness(brightness);
        }
    }

    fn read_indicator(&self) -> bool {
        self.indicator_service.read()
    }

    fn breathe_indicator(&mut self) -> Result<(), VmError> {
        self.indicator_service
            .breathe()
            .map_err(service_error_to_vm_error)
    }

    fn write_gpio(&mut self, name: &str, logical_value: bool) -> Result<(), VmError> {
        let raw_high = match name {
            "GPIO8" => logical_value,
            "GPIO10" => logical_value,
            _ => return Err(VmError::InvalidOperand),
        };
        match name {
            "GPIO8" => self.onboard_indicator.write_raw_high(raw_high),
            "GPIO10" => {
                self.external_indicator
                    .set_level(if raw_high { Level::High } else { Level::Low })
            }
            _ => return Err(VmError::InvalidOperand),
        }
        Ok(())
    }

    fn read_gpio(&self, name: &str) -> Result<bool, VmError> {
        match name {
            "GPIO8" => Ok(self.onboard_indicator.read_raw_high()),
            "GPIO10" => Ok(self.external_indicator.is_set_high()),
            _ => Err(VmError::InvalidOperand),
        }
    }

    pub(super) fn register_timer(
        &mut self,
        registration: TimerRegistration<AppRef, TimerEvent>,
    ) -> Result<(), VmError> {
        self.timer_service
            .enqueue(TimerCommand::Register(registration))
            .map_err(service_error_to_vm_error)
    }

    pub(super) fn remove_timers_for(&mut self, app: AppRef) {
        self.timer_service.remove_app_now(app);
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
        self.write_indicator(value)
    }

    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        self.indicator_service
            .toggle()
            .map_err(service_error_to_vm_error)
    }

    fn service_indicator_breathe(&mut self) -> Result<(), VmError> {
        self.breathe_indicator()
    }

    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        Ok(self.read_indicator())
    }

    fn app_launch(&mut self, app: &str) -> Result<(), VmError> {
        self.lifecycle_service
            .launch_app(AppName::new(app).map_err(|_| VmError::InvalidOperand)?)
            .map_err(service_error_to_vm_error)
    }

    fn app_arm(&mut self, app: &str) -> Result<(), VmError> {
        self.lifecycle_service
            .arm_app(AppName::new(app).map_err(|_| VmError::InvalidOperand)?)
            .map_err(service_error_to_vm_error)
    }

    fn app_disarm(&mut self, app: &str) -> Result<(), VmError> {
        self.lifecycle_service
            .disarm_app(AppName::new(app).map_err(|_| VmError::InvalidOperand)?)
            .map_err(service_error_to_vm_error)
    }

    fn service_timer_every(&mut self, event: &str, interval_ms: i32) -> Result<(), VmError> {
        if interval_ms <= 0 {
            return Err(VmError::InvalidOperand);
        }
        let Some(event) = TimerEvent::from_event(event) else {
            return Err(VmError::InvalidOperand);
        };
        let interval_ms = interval_ms as u64;
        let now_ms = Instant::now().duration_since_epoch().as_millis();
        self.register_timer(TimerRegistration {
            app: self.current_app.ok_or(VmError::InvalidOperand)?,
            event,
            armed: self.registration_mode,
            repeating: true,
            interval_ms,
            next_due_ms: now_ms.saturating_add(interval_ms),
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
        let delay_ms = delay_ms as u64;
        let now_ms = Instant::now().duration_since_epoch().as_millis();
        self.register_timer(TimerRegistration {
            app: self.current_app.ok_or(VmError::InvalidOperand)?,
            event,
            armed: self.registration_mode,
            repeating: false,
            interval_ms: delay_ms,
            next_due_ms: now_ms.saturating_add(delay_ms),
        })?;
        Ok(())
    }

    fn service_wifi_start_ap<'a>(
        &'a mut self,
        ssid: &str,
    ) -> Result<WifiActionResult<'a>, VmError> {
        if self
            .wifi_service
            .enqueue(WifiCommand::StartAp { ssid: ssid.len() })
            .is_err()
        {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("wifi command queue full"),
            });
        }
        self.wifi_service
            .set_cached_status(WifiCachedStatus::new(WifiState::Configuring));
        let _ = self.wifi_service.pop_command();
        self.wifi_service
            .set_cached_status(WifiCachedStatus::new(WifiState::Starting));
        let result = self.wifi.start_ap(ssid)?;
        let ok = result.ok;
        let error = result.error;
        if ok {
            self.trace("wifi.startAP");
            if let Ok(status) = self.wifi.status() {
                self.wifi_service
                    .set_cached_status(wifi_cached_status(&status));
            }
        } else {
            self.wifi_service
                .set_cached_status(WifiCachedStatus::new(WifiState::Error));
        }
        Ok(WifiActionResult { ok, error })
    }

    fn service_wifi_stop_ap<'a>(&'a mut self) -> Result<WifiActionResult<'a>, VmError> {
        if self.wifi_service.enqueue(WifiCommand::StopAp).is_err() {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("wifi command queue full"),
            });
        }
        self.wifi_service
            .set_cached_status(WifiCachedStatus::new(WifiState::Stopping));
        let _ = self.wifi_service.pop_command();
        let result = self.wifi.stop_ap()?;
        let ok = result.ok;
        let error = result.error;
        self.trace("wifi.stopAP");
        self.wifi_service.set_cached_status(WifiCachedStatus::new(
            if ok { WifiState::Stopped } else { WifiState::Error },
        ));
        Ok(WifiActionResult { ok, error })
    }

    fn service_wifi_connect<'a>(
        &'a mut self,
        profile: &str,
    ) -> Result<WifiActionResult<'a>, VmError> {
        let mut ssid = [0; WIFI_PROFILE_SSID_CAP];
        let mut password = [0; WIFI_PROFILE_PASSWORD_CAP];
        let Some(credentials) = self.wifi_profiles.credentials_for(profile) else {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("wifi profile not found"),
            });
        };
        let ssid_len = credentials.ssid.len();
        let password_len = credentials.password.len();
        ssid[..ssid_len].copy_from_slice(credentials.ssid);
        password[..password_len].copy_from_slice(credentials.password);
        if self
            .wifi_service
            .enqueue(WifiCommand::ConnectProfile {
                profile: profile.len(),
            })
            .is_err()
        {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("wifi command queue full"),
            });
        }
        self.wifi_service
            .set_cached_status(WifiCachedStatus::new(WifiState::Starting));
        let _ = self.wifi_service.pop_command();
        let result = self
            .wifi
            .connect_profile(profile, &ssid[..ssid_len], &password[..password_len])?;
        let ok = result.ok;
        let error = result.error;
        if ok {
            self.trace("wifi.connect");
            if let Ok(status) = self.wifi.status() {
                self.wifi_service
                    .set_cached_status(wifi_cached_status(&status));
            }
        } else {
            self.wifi_service
                .set_cached_status(WifiCachedStatus::new(WifiState::Error));
        }
        Ok(WifiActionResult { ok, error })
    }

    fn service_wifi_disconnect<'a>(&'a mut self) -> Result<WifiActionResult<'a>, VmError> {
        if self
            .wifi_service
            .enqueue(WifiCommand::DisconnectStation)
            .is_err()
        {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("wifi command queue full"),
            });
        }
        self.wifi_service
            .set_cached_status(WifiCachedStatus::new(WifiState::Stopping));
        let _ = self.wifi_service.pop_command();
        let result = self.wifi.disconnect()?;
        let ok = result.ok;
        let error = result.error;
        self.trace("wifi.disconnect");
        self.wifi_service.set_cached_status(WifiCachedStatus::new(
            if ok { WifiState::Stopped } else { WifiState::Error },
        ));
        Ok(WifiActionResult { ok, error })
    }

    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        let status = self.wifi.status()?;
        self.wifi_service
            .set_cached_status(wifi_cached_status(&status));
        Ok(status)
    }

    fn service_wifi_get_ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        self.wifi.ap_ip()
    }

    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiScanResult<'a>, VmError> {
        self.wifi.scan()
    }

    fn service_wifi_teardown(&mut self) -> Result<(), VmError> {
        self.teardown_services()
    }

    fn system_memory_text(&mut self, out: &mut dyn fmt::Write) -> Result<(), VmError> {
        write_ram_diagnostics_text(out, live_ram_diagnostics()).map_err(|_| VmError::InvalidOperand)
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

fn wifi_cached_status(status: &WifiStatus<'_>) -> WifiCachedStatus {
    WifiCachedStatus {
        state: match status.state {
            "unavailable" => WifiState::Unavailable,
            "idle" => WifiState::Idle,
            "configuring" => WifiState::Configuring,
            "starting" => WifiState::Starting,
            "started" => WifiState::Started,
            "stopping" => WifiState::Stopping,
            "stopped" => WifiState::Stopped,
            _ => WifiState::Error,
        },
        active: status.active,
        configured: status.configured,
        driver_started: status.driver_started,
        clients: status.clients,
        channel: status.channel,
        station_connected: status.connected,
        scan_matches: status.scan_matches,
    }
}

pub struct OnboardIndicator<'d> {
    channel: Channel<'d, LowSpeed>,
    brightness: u8,
    raw_high: bool,
}

impl<'d> OnboardIndicator<'d> {
    pub fn new(channel: Channel<'d, LowSpeed>) -> Self {
        let indicator = Self {
            channel,
            brightness: 0,
            raw_high: true,
        };
        indicator
    }

    fn write_raw_high(&mut self, value: bool) {
        self.raw_high = value;
        self.brightness = if value { 0 } else { 100 };
        let _ = self.channel.set_duty(if value { 100 } else { 0 });
    }

    fn read_raw_high(&self) -> bool {
        self.raw_high
    }

    fn write_brightness(&mut self, brightness: u8) {
        self.brightness = brightness.min(100);
        self.raw_high = self.brightness == 0;
        let _ = self.channel.set_duty(100 - self.brightness);
    }
}

fn service_error_to_vm_error(error: ServiceError) -> VmError {
    match error {
        ServiceError::QueueFull => VmError::InvalidOperand,
        ServiceError::InvalidProfile => VmError::InvalidOperand,
    }
}

impl fmt::Debug for RuntimeSink<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().take(self.len))
            .finish()
    }
}
