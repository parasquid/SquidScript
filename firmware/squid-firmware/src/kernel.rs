use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceError {
    QueueFull,
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
