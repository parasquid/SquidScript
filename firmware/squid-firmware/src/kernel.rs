use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceError {
    QueueFull,
    InvalidProfile,
}

pub struct BoundedQueue<T: Copy, const CAP: usize> {
    items: [Option<T>; CAP],
    head: usize,
    len: usize,
}

impl<T: Copy, const CAP: usize> BoundedQueue<T, CAP> {
    pub const fn new() -> Self {
        Self {
            items: [None; CAP],
            head: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, item: T) -> Result<(), ServiceError> {
        if self.len == CAP {
            return Err(ServiceError::QueueFull);
        }
        let index = (self.head + self.len) % CAP;
        self.items[index] = Some(item);
        self.len += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let item = self.items[self.head].take();
        self.head = (self.head + 1) % CAP;
        self.len -= 1;
        item
    }

    pub fn clear(&mut self) {
        self.items = [None; CAP];
        self.head = 0;
        self.len = 0;
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub fn get(&self, offset: usize) -> Option<T> {
        if offset >= self.len {
            return None;
        }
        self.items[(self.head + offset) % CAP]
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleCommand<App, Event> {
    LaunchApp(App),
    ArmApp(App),
    DisarmApp(App),
    ExitApp,
    RootRestart,
    DispatchAppEvent { app: App, event: Event },
}

pub struct LifecycleService<App: Copy, Event: Copy, const CAP: usize> {
    commands: BoundedQueue<LifecycleCommand<App, Event>, CAP>,
}

impl<App: Copy, Event: Copy, const CAP: usize> LifecycleService<App, Event, CAP> {
    pub const fn new() -> Self {
        Self {
            commands: BoundedQueue::new(),
        }
    }

    pub fn launch_app(&mut self, app: App) -> Result<(), ServiceError> {
        self.commands.push(LifecycleCommand::LaunchApp(app))
    }

    pub fn arm_app(&mut self, app: App) -> Result<(), ServiceError> {
        self.commands.push(LifecycleCommand::ArmApp(app))
    }

    pub fn disarm_app(&mut self, app: App) -> Result<(), ServiceError> {
        self.commands.push(LifecycleCommand::DisarmApp(app))
    }

    pub fn exit_app(&mut self) -> Result<(), ServiceError> {
        self.commands.push(LifecycleCommand::ExitApp)
    }

    pub fn root_restart(&mut self) -> Result<(), ServiceError> {
        self.commands.push(LifecycleCommand::RootRestart)
    }

    pub fn dispatch_app_event(&mut self, app: App, event: Event) -> Result<(), ServiceError> {
        self.commands
            .push(LifecycleCommand::DispatchAppEvent { app, event })
    }

    pub fn pop_command(&mut self) -> Option<LifecycleCommand<App, Event>> {
        self.commands.pop()
    }

    pub fn take_root_restart(&mut self) -> bool {
        let command_count = self.commands.len();
        let mut found = false;
        for _ in 0..command_count {
            let Some(command) = self.commands.pop() else {
                break;
            };
            if matches!(command, LifecycleCommand::RootRestart) && !found {
                found = true;
                continue;
            }
            self.commands.push(command).ok();
        }
        found
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageCommand<App, Resource> {
    EnsureReady,
    Format,
    ReadInstalledApp { app: App },
    ReadInstalledAppRange { app: App },
    WriteInstalledApp { app: App },
    WriteAppResource { app: App, resource: Resource },
    WriteState { app: App },
    ReadState { app: App },
    DeleteState { app: App },
}

pub struct StorageService<App: Copy, Resource: Copy, const CAP: usize> {
    commands: BoundedQueue<StorageCommand<App, Resource>, CAP>,
}

impl<App: Copy, Resource: Copy, const CAP: usize> StorageService<App, Resource, CAP> {
    pub const fn new() -> Self {
        Self {
            commands: BoundedQueue::new(),
        }
    }

    pub fn enqueue(&mut self, command: StorageCommand<App, Resource>) -> Result<(), ServiceError> {
        self.commands.push(command)
    }

    pub fn pop_command(&mut self) -> Option<StorageCommand<App, Resource>> {
        self.commands.pop()
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WifiState {
    Unavailable,
    Idle,
    Configuring,
    Starting,
    Started,
    Stopping,
    Stopped,
    Error,
}

impl WifiState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Idle => "idle",
            Self::Configuring => "configuring",
            Self::Starting => "starting",
            Self::Started => "started",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WifiCommand {
    StartAp { ssid: usize },
    ConnectProfile { profile: usize },
    StopAp,
    DisconnectStation,
    Poll,
    Teardown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiCachedStatus {
    pub state: WifiState,
    pub active: bool,
    pub configured: bool,
    pub driver_started: bool,
    pub clients: i32,
    pub channel: i32,
    pub station_connected: bool,
    pub scan_matches: i32,
}

impl WifiCachedStatus {
    pub const fn new(state: WifiState) -> Self {
        Self {
            state,
            active: false,
            configured: false,
            driver_started: false,
            clients: 0,
            channel: 0,
            station_connected: false,
            scan_matches: 0,
        }
    }
}

pub struct WifiService<const CAP: usize> {
    commands: BoundedQueue<WifiCommand, CAP>,
    cached_status: WifiCachedStatus,
}

impl<const CAP: usize> WifiService<CAP> {
    pub const fn new(state: WifiState) -> Self {
        Self {
            commands: BoundedQueue::new(),
            cached_status: WifiCachedStatus::new(state),
        }
    }

    pub fn enqueue(&mut self, command: WifiCommand) -> Result<(), ServiceError> {
        self.commands.push(command)
    }

    pub fn pop_command(&mut self) -> Option<WifiCommand> {
        self.commands.pop()
    }

    pub fn set_cached_status(&mut self, status: WifiCachedStatus) {
        self.cached_status = status;
    }

    pub const fn cached_status(&self) -> WifiCachedStatus {
        self.cached_status
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiStationCredentials<'a> {
    pub profile: &'a str,
    pub ssid: &'a [u8],
    pub password: &'a [u8],
}

pub struct WifiStationProfileStore<
    const PROFILE_CAP: usize,
    const SSID_CAP: usize,
    const PASSWORD_CAP: usize,
> {
    profile: [u8; PROFILE_CAP],
    profile_len: usize,
    ssid: [u8; SSID_CAP],
    ssid_len: usize,
    password: [u8; PASSWORD_CAP],
    password_len: usize,
}

impl<const PROFILE_CAP: usize, const SSID_CAP: usize, const PASSWORD_CAP: usize>
    WifiStationProfileStore<PROFILE_CAP, SSID_CAP, PASSWORD_CAP>
{
    pub const fn new() -> Self {
        Self {
            profile: [0; PROFILE_CAP],
            profile_len: 0,
            ssid: [0; SSID_CAP],
            ssid_len: 0,
            password: [0; PASSWORD_CAP],
            password_len: 0,
        }
    }

    pub fn set(&mut self, profile: &str, ssid: &[u8], password: &[u8]) -> Result<(), ServiceError> {
        let profile_bytes = profile.as_bytes();
        if !valid_wifi_profile_name::<PROFILE_CAP>(profile)
            || ssid.is_empty()
            || ssid.len() > SSID_CAP
            || password.len() > PASSWORD_CAP
        {
            return Err(ServiceError::InvalidProfile);
        }

        self.profile[..profile_bytes.len()].copy_from_slice(profile_bytes);
        self.profile_len = profile_bytes.len();
        self.ssid[..ssid.len()].copy_from_slice(ssid);
        self.ssid_len = ssid.len();
        self.password[..password.len()].copy_from_slice(password);
        self.password_len = password.len();
        Ok(())
    }

    pub fn credentials_for(&self, profile: &str) -> Option<WifiStationCredentials<'_>> {
        if self.profile_len != profile.len()
            || self.profile[..self.profile_len] != *profile.as_bytes()
        {
            return None;
        }

        let profile = core::str::from_utf8(&self.profile[..self.profile_len]).ok()?;
        Some(WifiStationCredentials {
            profile,
            ssid: &self.ssid[..self.ssid_len],
            password: &self.password[..self.password_len],
        })
    }

    pub fn write_diagnostics(&self, out: &mut dyn fmt::Write) -> fmt::Result {
        let profile = core::str::from_utf8(&self.profile[..self.profile_len]).unwrap_or("");
        writeln!(out, "profile={profile}")?;
        writeln!(out, "ssid_len={}", self.ssid_len)?;
        writeln!(out, "password_len={}", self.password_len)
    }
}

pub const fn valid_wifi_profile_name<const CAP: usize>(profile: &str) -> bool {
    let bytes = profile.as_bytes();
    if bytes.is_empty() || bytes.len() > CAP {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
            return false;
        }
        index += 1;
    }
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayCommand<Draw> {
    Draw(Draw),
    Flush,
    Clear,
}

pub struct DisplayService<Draw: Copy, const CAP: usize> {
    commands: BoundedQueue<DisplayCommand<Draw>, CAP>,
}

impl<Draw: Copy, const CAP: usize> DisplayService<Draw, CAP> {
    pub const fn new() -> Self {
        Self {
            commands: BoundedQueue::new(),
        }
    }

    pub fn enqueue(&mut self, command: DisplayCommand<Draw>) -> Result<(), ServiceError> {
        self.commands.push(command)
    }

    pub fn pop_command(&mut self) -> Option<DisplayCommand<Draw>> {
        self.commands.pop()
    }

    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    pub fn command_at(&self, index: usize) -> Option<DisplayCommand<Draw>> {
        self.commands.get(index)
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RamDiagnostics {
    pub ram_total_bytes: usize,
    pub heap_total_bytes: usize,
    pub heap_used_bytes: usize,
    pub heap_peak_used_bytes: usize,
    pub heap_total_allocated_bytes: usize,
    pub heap_total_freed_bytes: usize,
}

impl RamDiagnostics {
    pub const fn heap_available_bytes(self) -> usize {
        self.heap_total_bytes.saturating_sub(self.heap_used_bytes)
    }
}

pub fn write_ram_diagnostics_text(
    out: &mut dyn fmt::Write,
    diagnostics: RamDiagnostics,
) -> Result<(), fmt::Error> {
    write!(out, "RAM ")?;
    write_bytes(out, diagnostics.ram_total_bytes)?;
    write!(out, " heap ")?;
    write_bytes(out, diagnostics.heap_used_bytes)?;
    write!(out, " used ")?;
    write_bytes(out, diagnostics.heap_available_bytes())?;
    write!(out, " free")
}

fn write_bytes(out: &mut dyn fmt::Write, bytes: usize) -> Result<(), fmt::Error> {
    if bytes >= 1024 * 1024 {
        write!(out, "{} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        write!(out, "{} KiB", bytes / 1024)
    } else {
        write!(out, "{bytes} B")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndicatorAction {
    SetBrightness(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerRegistration<App, Event> {
    pub app: App,
    pub event: Event,
    pub armed: bool,
    pub repeating: bool,
    pub interval_ms: u64,
    pub next_due_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerCommand<App, Event> {
    Register(TimerRegistration<App, Event>),
    RemoveApp(App),
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerDue<App, Event> {
    pub app: App,
    pub event: Event,
    pub armed: bool,
}

pub struct TimerService<
    App: Copy + Eq,
    Event: Copy + Eq,
    const TIMER_CAP: usize,
    const COMMAND_CAP: usize,
    const DUE_CAP: usize,
> {
    timers: [Option<TimerRegistration<App, Event>>; TIMER_CAP],
    commands: [Option<TimerCommand<App, Event>>; COMMAND_CAP],
    command_head: usize,
    command_len: usize,
    due: [Option<TimerDue<App, Event>>; DUE_CAP],
    due_head: usize,
    due_len: usize,
}

impl<
        App: Copy + Eq,
        Event: Copy + Eq,
        const TIMER_CAP: usize,
        const COMMAND_CAP: usize,
        const DUE_CAP: usize,
    > TimerService<App, Event, TIMER_CAP, COMMAND_CAP, DUE_CAP>
{
    pub const fn new() -> Self {
        Self {
            timers: [None; TIMER_CAP],
            commands: [None; COMMAND_CAP],
            command_head: 0,
            command_len: 0,
            due: [None; DUE_CAP],
            due_head: 0,
            due_len: 0,
        }
    }

    pub fn enqueue(&mut self, command: TimerCommand<App, Event>) -> Result<(), ServiceError> {
        if self.command_len == COMMAND_CAP {
            return Err(ServiceError::QueueFull);
        }
        let index = (self.command_head + self.command_len) % COMMAND_CAP;
        self.commands[index] = Some(command);
        self.command_len += 1;
        Ok(())
    }

    pub fn step(&mut self, now_ms: u64, active_app: Option<App>) -> Result<(), ServiceError> {
        while let Some(command) = self.pop_command() {
            self.apply_command(command)?;
        }

        for index in 0..self.timers.len() {
            let Some(mut timer) = self.timers[index] else {
                continue;
            };
            if now_ms < timer.next_due_ms {
                continue;
            }

            let is_active = active_app == Some(timer.app);
            if !timer.armed && !is_active {
                continue;
            }
            if timer.armed && is_active {
                continue;
            }

            if timer.repeating {
                timer.next_due_ms = now_ms.saturating_add(timer.interval_ms);
                self.timers[index] = Some(timer);
            } else {
                self.timers[index] = None;
            }

            self.push_due(TimerDue {
                app: timer.app,
                event: timer.event,
                armed: timer.armed,
            })?;
        }

        Ok(())
    }

    pub fn pop_due(&mut self) -> Option<TimerDue<App, Event>> {
        if self.due_len == 0 {
            return None;
        }
        let due = self.due[self.due_head].take();
        self.due_head = (self.due_head + 1) % DUE_CAP;
        self.due_len -= 1;
        due
    }

    pub fn register_now(
        &mut self,
        registration: TimerRegistration<App, Event>,
    ) -> Result<(), ServiceError> {
        self.register(registration)
    }

    pub fn remove_app_now(&mut self, app: App) {
        self.remove_app(app);
    }

    pub fn clear_now(&mut self) {
        self.timers = [None; TIMER_CAP];
        self.commands = [None; COMMAND_CAP];
        self.command_head = 0;
        self.command_len = 0;
        self.due = [None; DUE_CAP];
        self.due_head = 0;
        self.due_len = 0;
    }

    #[cfg(test)]
    fn next_due_ms(&self, app: App, event: Event) -> Option<u64> {
        self.timers.iter().find_map(|timer| {
            let timer = (*timer)?;
            (timer.app == app && timer.event == event).then_some(timer.next_due_ms)
        })
    }

    fn pop_command(&mut self) -> Option<TimerCommand<App, Event>> {
        if self.command_len == 0 {
            return None;
        }
        let command = self.commands[self.command_head].take();
        self.command_head = (self.command_head + 1) % COMMAND_CAP;
        self.command_len -= 1;
        command
    }

    fn apply_command(&mut self, command: TimerCommand<App, Event>) -> Result<(), ServiceError> {
        match command {
            TimerCommand::Register(registration) => self.register(registration),
            TimerCommand::RemoveApp(app) => {
                self.remove_app(app);
                Ok(())
            }
            TimerCommand::Clear => {
                self.clear_now();
                Ok(())
            }
        }
    }

    fn register(
        &mut self,
        registration: TimerRegistration<App, Event>,
    ) -> Result<(), ServiceError> {
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

        Err(ServiceError::QueueFull)
    }

    fn remove_app(&mut self, app: App) {
        for timer in &mut self.timers {
            if timer.map(|timer| timer.app) == Some(app) {
                *timer = None;
            }
        }

        let mut kept = [None; DUE_CAP];
        let mut kept_len = 0usize;
        while let Some(due) = self.pop_due() {
            if due.app != app {
                kept[kept_len] = Some(due);
                kept_len += 1;
            }
        }
        self.due = kept;
        self.due_head = 0;
        self.due_len = kept_len;
    }

    fn push_due(&mut self, due: TimerDue<App, Event>) -> Result<(), ServiceError> {
        if self.due_len == DUE_CAP {
            return Err(ServiceError::QueueFull);
        }
        let index = (self.due_head + self.due_len) % DUE_CAP;
        self.due[index] = Some(due);
        self.due_len += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndicatorMode {
    Steady,
    Breathing,
}

const BREATH_DUTIES: [u8; 65] = [
    0, 0, 1, 2, 4, 6, 8, 11, 15, 18, 22, 26, 31, 35, 40, 45, 50, 55, 60, 65, 69, 74, 78, 82, 85,
    89, 92, 94, 96, 98, 99, 100, 100, 100, 99, 98, 96, 94, 92, 89, 85, 82, 78, 74, 69, 65, 60, 55,
    50, 45, 40, 35, 31, 26, 22, 18, 15, 11, 8, 6, 4, 2, 1, 0, 0,
];

pub const INDICATOR_BREATH_SEGMENT_MS: u64 = 31;

pub struct IndicatorService<const CAP: usize> {
    actions: [Option<IndicatorAction>; CAP],
    head: usize,
    len: usize,
    brightness: u8,
    mode: IndicatorMode,
    breath_step: usize,
}

impl<const CAP: usize> IndicatorService<CAP> {
    pub const fn new_breathing() -> Self {
        Self {
            actions: [None; CAP],
            head: 0,
            len: 0,
            brightness: 0,
            mode: IndicatorMode::Breathing,
            breath_step: 0,
        }
    }

    pub fn write(&mut self, value: bool) -> Result<(), ServiceError> {
        self.ensure_action_capacity()?;
        self.mode = IndicatorMode::Steady;
        self.brightness = if value { 100 } else { 0 };
        self.push_action(IndicatorAction::SetBrightness(self.brightness))
    }

    pub fn toggle(&mut self) -> Result<(), ServiceError> {
        self.write(!self.read())
    }

    pub fn breathe(&mut self) -> Result<(), ServiceError> {
        self.ensure_action_capacity()?;
        self.mode = IndicatorMode::Breathing;
        self.breath_step = 0;
        self.push_action(IndicatorAction::SetBrightness(self.brightness))
    }

    pub fn read(&self) -> bool {
        self.brightness > 0
    }

    pub fn next_breath_action(&mut self) -> Option<IndicatorAction> {
        if self.mode != IndicatorMode::Breathing {
            return None;
        }

        let brightness = BREATH_DUTIES[self.breath_step];
        self.brightness = brightness;
        self.breath_step = (self.breath_step + 1) % BREATH_DUTIES.len();
        Some(IndicatorAction::SetBrightness(brightness))
    }

    pub fn pop_action(&mut self) -> Option<IndicatorAction> {
        if self.len == 0 {
            return None;
        }

        let action = self.actions[self.head].take();
        self.head = (self.head + 1) % CAP;
        self.len -= 1;
        action
    }

    fn push_action(&mut self, action: IndicatorAction) -> Result<(), ServiceError> {
        self.ensure_action_capacity()?;
        let index = (self.head + self.len) % CAP;
        self.actions[index] = Some(action);
        self.len += 1;
        Ok(())
    }

    fn ensure_action_capacity(&self) -> Result<(), ServiceError> {
        if self.len == CAP {
            return Err(ServiceError::QueueFull);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::String;

    #[test]
    fn indicator_write_updates_cached_state_and_enqueues_pwm_action() {
        let mut indicator = IndicatorService::<2>::new_breathing();

        indicator.write(true).unwrap();

        assert!(indicator.read());
        assert_eq!(
            indicator.pop_action(),
            Some(IndicatorAction::SetBrightness(100))
        );
        assert_eq!(indicator.pop_action(), None);
    }

    #[test]
    fn indicator_queue_is_bounded() {
        let mut indicator = IndicatorService::<1>::new_breathing();

        indicator.write(true).unwrap();

        assert_eq!(indicator.write(false), Err(ServiceError::QueueFull));
        assert!(indicator.read());
    }

    #[test]
    fn indicator_breathing_steps_are_service_owned_actions() {
        let mut indicator = IndicatorService::<4>::new_breathing();

        assert_eq!(
            indicator.next_breath_action(),
            Some(IndicatorAction::SetBrightness(0))
        );
        assert_eq!(
            indicator.next_breath_action(),
            Some(IndicatorAction::SetBrightness(0))
        );

        indicator.write(true).unwrap();
        indicator.pop_action();

        assert_eq!(indicator.next_breath_action(), None);
        indicator.breathe().unwrap();
        indicator.pop_action();

        assert_eq!(
            indicator.next_breath_action(),
            Some(IndicatorAction::SetBrightness(0))
        );
    }

    #[test]
    fn ram_diagnostics_reports_heap_available_with_saturating_math() {
        let diagnostics = RamDiagnostics {
            ram_total_bytes: 400 * 1024,
            heap_total_bytes: 96 * 1024,
            heap_used_bytes: 100 * 1024,
            heap_peak_used_bytes: 100 * 1024,
            heap_total_allocated_bytes: 120 * 1024,
            heap_total_freed_bytes: 20 * 1024,
        };

        assert_eq!(diagnostics.heap_available_bytes(), 0);
    }

    #[test]
    fn ram_diagnostics_text_shows_board_ram_and_live_heap() {
        let mut output = String::new();
        write_ram_diagnostics_text(
            &mut output,
            RamDiagnostics {
                ram_total_bytes: 400 * 1024,
                heap_total_bytes: 96 * 1024,
                heap_used_bytes: 12 * 1024,
                heap_peak_used_bytes: 20 * 1024,
                heap_total_allocated_bytes: 32 * 1024,
                heap_total_freed_bytes: 20 * 1024,
            },
        )
        .unwrap();

        assert_eq!(output.as_str(), "RAM 400 KiB heap 12 KiB used 84 KiB free");
    }

    #[test]
    fn timer_actor_command_queue_is_bounded() {
        let mut timers = TimerService::<u8, u8, 4, 1, 1>::new();

        timers
            .enqueue(TimerCommand::Register(TimerRegistration {
                app: 1,
                event: 1,
                armed: false,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            }))
            .unwrap();

        assert_eq!(
            timers.enqueue(TimerCommand::Clear),
            Err(ServiceError::QueueFull)
        );
    }

    #[test]
    fn bounded_queue_preserves_fifo_order_and_capacity() {
        let mut queue = BoundedQueue::<u8, 2>::new();

        queue.push(1).unwrap();
        queue.push(2).unwrap();

        assert_eq!(queue.push(3), Err(ServiceError::QueueFull));
        assert_eq!(queue.pop(), Some(1));
        queue.push(3).unwrap();
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn lifecycle_service_queues_explicit_app_intents() {
        let mut lifecycle = LifecycleService::<u8, u8, 3>::new();

        lifecycle.launch_app(1).unwrap();
        lifecycle.arm_app(2).unwrap();
        lifecycle.dispatch_app_event(1, 9).unwrap();

        assert_eq!(
            lifecycle.pop_command(),
            Some(LifecycleCommand::LaunchApp(1))
        );
        assert_eq!(lifecycle.pop_command(), Some(LifecycleCommand::ArmApp(2)));
        assert_eq!(
            lifecycle.pop_command(),
            Some(LifecycleCommand::DispatchAppEvent { app: 1, event: 9 })
        );
        assert_eq!(lifecycle.pop_command(), None);
    }

    #[test]
    fn lifecycle_service_queues_exit_restart_disarm_and_event_intents() {
        let mut lifecycle = LifecycleService::<u8, u8, 4>::new();

        lifecycle.disarm_app(2).unwrap();
        lifecycle.exit_app().unwrap();
        lifecycle.root_restart().unwrap();
        lifecycle.dispatch_app_event(3, 4).unwrap();

        assert_eq!(lifecycle.pop_command(), Some(LifecycleCommand::DisarmApp(2)));
        assert_eq!(lifecycle.pop_command(), Some(LifecycleCommand::ExitApp));
        assert!(lifecycle.take_root_restart());
        assert_eq!(
            lifecycle.pop_command(),
            Some(LifecycleCommand::DispatchAppEvent { app: 3, event: 4 })
        );
        assert_eq!(lifecycle.pop_command(), None);
    }

    #[test]
    fn storage_service_boundary_names_storage_operations_without_backend_change() {
        let mut storage = StorageService::<u8, u8, 3>::new();

        storage
            .enqueue(StorageCommand::ReadInstalledApp { app: 1 })
            .unwrap();
        storage
            .enqueue(StorageCommand::WriteAppResource {
                app: 1,
                resource: 7,
            })
            .unwrap();
        storage
            .enqueue(StorageCommand::WriteState { app: 1 })
            .unwrap();

        assert_eq!(
            storage.pop_command(),
            Some(StorageCommand::ReadInstalledApp { app: 1 })
        );
        assert_eq!(
            storage.pop_command(),
            Some(StorageCommand::WriteAppResource {
                app: 1,
                resource: 7,
            })
        );
        assert_eq!(
            storage.pop_command(),
            Some(StorageCommand::WriteState { app: 1 })
        );
    }

    #[test]
    fn wifi_service_processes_bounded_commands_and_caches_state() {
        let mut wifi = WifiService::<2>::new(WifiState::Idle);

        wifi.enqueue(WifiCommand::StartAp { ssid: 3 }).unwrap();
        wifi.enqueue(WifiCommand::Poll).unwrap();

        assert_eq!(
            wifi.enqueue(WifiCommand::StopAp),
            Err(ServiceError::QueueFull)
        );
        assert_eq!(wifi.pop_command(), Some(WifiCommand::StartAp { ssid: 3 }));
        wifi.set_cached_status(WifiCachedStatus {
            state: WifiState::Started,
            active: true,
            configured: true,
            driver_started: true,
            clients: 0,
            channel: 1,
            station_connected: false,
            scan_matches: 0,
        });

        assert_eq!(wifi.cached_status().state, WifiState::Started);
        assert!(wifi.cached_status().active);
        assert_eq!(wifi.pop_command(), Some(WifiCommand::Poll));
        assert_eq!(wifi.pop_command(), None);
    }

    #[test]
    fn wifi_service_queues_station_commands() {
        let mut wifi = WifiService::<2>::new(WifiState::Idle);

        wifi.enqueue(WifiCommand::ConnectProfile { profile: 3 })
            .unwrap();
        wifi.enqueue(WifiCommand::DisconnectStation).unwrap();

        assert_eq!(
            wifi.enqueue(WifiCommand::Poll),
            Err(ServiceError::QueueFull)
        );
        assert_eq!(
            wifi.pop_command(),
            Some(WifiCommand::ConnectProfile { profile: 3 })
        );
        wifi.set_cached_status(WifiCachedStatus {
            state: WifiState::Started,
            active: true,
            configured: true,
            driver_started: true,
            clients: 0,
            channel: 11,
            station_connected: true,
            scan_matches: 1,
        });
        assert!(wifi.cached_status().station_connected);
        assert_eq!(wifi.cached_status().scan_matches, 1);
        assert_eq!(wifi.pop_command(), Some(WifiCommand::DisconnectStation));
    }

    #[test]
    fn wifi_station_profile_store_routes_matching_credentials_without_leaking_secret() {
        let mut profiles = WifiStationProfileStore::<16, 32, 64>::new();

        profiles
            .set("dev", b"LabNetwork", b"correct horse battery staple")
            .unwrap();

        let credentials = profiles.credentials_for("dev").unwrap();
        assert_eq!(credentials.profile, "dev");
        assert_eq!(credentials.ssid, b"LabNetwork");
        assert_eq!(credentials.password, b"correct horse battery staple");
        assert!(profiles.credentials_for("prod").is_none());

        let mut diagnostics = std::string::String::new();
        profiles.write_diagnostics(&mut diagnostics).unwrap();
        assert!(!diagnostics.contains("correct horse battery staple"));
        assert!(!diagnostics.contains("LabNetwork"));
        assert!(diagnostics.contains("profile=dev"));
        assert!(diagnostics.contains("ssid_len=10"));
        assert!(diagnostics.contains("password_len=28"));
    }

    #[test]
    fn display_service_queue_is_bounded() {
        let mut display = DisplayService::<u8, 1>::new();

        display.enqueue(DisplayCommand::Draw(7)).unwrap();

        assert_eq!(
            display.enqueue(DisplayCommand::Draw(8)),
            Err(ServiceError::QueueFull)
        );
        assert_eq!(display.pop_command(), Some(DisplayCommand::Draw(7)));
        assert_eq!(display.pop_command(), None);
    }

    #[test]
    fn timer_actor_due_queue_is_bounded() {
        let mut timers = TimerService::<u8, u8, 4, 4, 1>::new();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 1,
                armed: false,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            })
            .unwrap();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 2,
                armed: false,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            })
            .unwrap();

        assert_eq!(timers.step(100, Some(1)), Err(ServiceError::QueueFull));
        assert_eq!(
            timers.pop_due(),
            Some(TimerDue {
                app: 1,
                event: 1,
                armed: false,
            })
        );
    }

    #[test]
    fn foreground_timer_fires_only_for_active_app() {
        let mut timers = TimerService::<u8, u8, 4, 4, 4>::new();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 7,
                armed: false,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            })
            .unwrap();

        timers.step(100, None).unwrap();
        assert_eq!(timers.pop_due(), None);

        timers.step(100, Some(2)).unwrap();
        assert_eq!(timers.pop_due(), None);

        timers.step(100, Some(1)).unwrap();
        assert_eq!(
            timers.pop_due(),
            Some(TimerDue {
                app: 1,
                event: 7,
                armed: false,
            })
        );
    }

    #[test]
    fn armed_timer_fires_only_when_app_is_inactive() {
        let mut timers = TimerService::<u8, u8, 4, 4, 4>::new();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 8,
                armed: true,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            })
            .unwrap();

        timers.step(100, Some(1)).unwrap();
        assert_eq!(timers.pop_due(), None);

        timers.step(100, Some(2)).unwrap();
        assert_eq!(
            timers.pop_due(),
            Some(TimerDue {
                app: 1,
                event: 8,
                armed: true,
            })
        );
    }

    #[test]
    fn repeating_timer_reschedules_before_dispatch() {
        let mut timers = TimerService::<u8, u8, 4, 4, 4>::new();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 9,
                armed: false,
                repeating: true,
                interval_ms: 50,
                next_due_ms: 100,
            })
            .unwrap();

        timers.step(100, Some(1)).unwrap();
        assert_eq!(timers.next_due_ms(1, 9), Some(150));
        assert_eq!(
            timers.pop_due(),
            Some(TimerDue {
                app: 1,
                event: 9,
                armed: false,
            })
        );
    }

    #[test]
    fn one_shot_timer_clears_before_dispatch() {
        let mut timers = TimerService::<u8, u8, 4, 4, 4>::new();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 9,
                armed: false,
                repeating: false,
                interval_ms: 50,
                next_due_ms: 100,
            })
            .unwrap();

        timers.step(100, Some(1)).unwrap();
        assert_eq!(timers.next_due_ms(1, 9), None);
        assert_eq!(
            timers.pop_due(),
            Some(TimerDue {
                app: 1,
                event: 9,
                armed: false,
            })
        );
    }

    #[test]
    fn timer_remove_for_app_clears_all_app_timers() {
        let mut timers = TimerService::<u8, u8, 4, 4, 4>::new();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 1,
                armed: false,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            })
            .unwrap();
        timers
            .register_now(TimerRegistration {
                app: 2,
                event: 1,
                armed: false,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            })
            .unwrap();

        timers.enqueue(TimerCommand::RemoveApp(1)).unwrap();
        timers.step(100, Some(1)).unwrap();
        timers.step(100, Some(2)).unwrap();

        assert_eq!(
            timers.pop_due(),
            Some(TimerDue {
                app: 2,
                event: 1,
                armed: false,
            })
        );
        assert_eq!(timers.pop_due(), None);
    }

    #[test]
    fn timer_remove_for_app_clears_already_queued_due_events() {
        let mut timers = TimerService::<u8, u8, 4, 4, 4>::new();
        timers
            .register_now(TimerRegistration {
                app: 1,
                event: 1,
                armed: false,
                repeating: true,
                interval_ms: 100,
                next_due_ms: 100,
            })
            .unwrap();

        timers.step(100, Some(1)).unwrap();
        timers.enqueue(TimerCommand::RemoveApp(1)).unwrap();
        timers.step(100, Some(1)).unwrap();

        assert_eq!(timers.pop_due(), None);
    }
}
