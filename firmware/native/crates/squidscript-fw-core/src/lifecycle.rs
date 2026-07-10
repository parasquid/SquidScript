use squidvm_core::limits::{MAX_APP_ID_BYTES, MAX_EVENT_NAME_BYTES};

pub const MAX_RETURN_STACK: usize = 2;
pub const MAX_ARMED_TIMERS: usize = 2;
pub const MAX_ARMED_INPUTS: usize = 8;
pub const MAX_PENDING_EVENTS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleError {
    InvalidText,
    ReturnStackFull,
    ArmedTimersFull,
    ArmedInputsFull,
    DuplicateInputOwner,
    EventQueueFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecyclePhase {
    Idle,
    Exiting,
    Starting,
    Dispatching,
}

impl LifecyclePhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Exiting => "exiting",
            Self::Starting => "starting",
            Self::Dispatching => "dispatching",
        }
    }

    pub const fn code(self) -> u64 {
        match self {
            Self::Idle => 0,
            Self::Exiting => 1,
            Self::Starting => 2,
            Self::Dispatching => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartReason {
    Boot,
    Launch,
    Return,
    Wake,
}

impl StartReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::Launch => "launch",
            Self::Return => "return",
            Self::Wake => "wake",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TriggerTimer<'a> {
    pub event: &'a str,
    pub interval_ms: u32,
    pub repeating: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArmedRoute<'a> {
    pub app_id: &'a str,
    pub event: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingEvent<'a> {
    pub owner: Option<&'a str>,
    pub event: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Text<N> {
    const fn empty() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    fn set(&mut self, value: &str) -> Result<(), LifecycleError> {
        if value.is_empty() || value.len() > N {
            return Err(LifecycleError::InvalidText);
        }
        self.bytes[..value.len()].copy_from_slice(value.as_bytes());
        self.len = value.len();
        Ok(())
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

type AppId = Text<MAX_APP_ID_BYTES>;
type EventName = Text<MAX_EVENT_NAME_BYTES>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArmedTimer {
    app_id: AppId,
    event: EventName,
    interval_ms: u32,
    remaining_ms: u32,
    repeating: bool,
    active: bool,
}

impl ArmedTimer {
    const fn empty() -> Self {
        Self {
            app_id: AppId::empty(),
            event: EventName::empty(),
            interval_ms: 0,
            remaining_ms: 0,
            repeating: false,
            active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArmedInput {
    app_id: AppId,
    event: EventName,
    active: bool,
}

impl ArmedInput {
    const fn empty() -> Self {
        Self {
            app_id: AppId::empty(),
            event: EventName::empty(),
            active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct QueuedEvent {
    event: EventName,
    owner_kind: u8,
    owner_index: u8,
}

impl QueuedEvent {
    const fn empty() -> Self {
        Self {
            event: EventName::empty(),
            owner_kind: 0,
            owner_index: 0,
        }
    }
}

pub struct ForegroundLifecycle {
    active: AppId,
    return_stack: [AppId; MAX_RETURN_STACK],
    return_len: usize,
    armed_timers: [ArmedTimer; MAX_ARMED_TIMERS],
    armed_inputs: [ArmedInput; MAX_ARMED_INPUTS],
    queue: [QueuedEvent; MAX_PENDING_EVENTS],
    queue_head: usize,
    queue_len: usize,
    phase: LifecyclePhase,
    start_reason: StartReason,
    queue_overflowed: bool,
}

impl ForegroundLifecycle {
    pub const fn new() -> Self {
        Self {
            active: AppId::empty(),
            return_stack: [AppId::empty(); MAX_RETURN_STACK],
            return_len: 0,
            armed_timers: [ArmedTimer::empty(); MAX_ARMED_TIMERS],
            armed_inputs: [ArmedInput::empty(); MAX_ARMED_INPUTS],
            queue: [QueuedEvent::empty(); MAX_PENDING_EVENTS],
            queue_head: 0,
            queue_len: 0,
            phase: LifecyclePhase::Idle,
            start_reason: StartReason::Boot,
            queue_overflowed: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn active(&self) -> Option<&str> {
        (!self.active.as_str().is_empty()).then(|| self.active.as_str())
    }

    pub const fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    pub fn set_phase(&mut self, phase: LifecyclePhase) {
        self.phase = phase;
    }

    pub const fn start_reason(&self) -> StartReason {
        self.start_reason
    }

    pub fn can_push_active(&self) -> bool {
        self.active().is_none() || self.return_len < self.return_stack.len()
    }

    pub fn begin_foreground(
        &mut self,
        app_id: &str,
        reason: StartReason,
        push_active: bool,
    ) -> Result<(), LifecycleError> {
        if push_active {
            if let Some(active) = self.active() {
                if self.return_len == self.return_stack.len() {
                    return Err(LifecycleError::ReturnStackFull);
                }
                let mut saved = AppId::empty();
                saved.set(active)?;
                self.return_stack[self.return_len] = saved;
                self.return_len += 1;
            }
        }
        self.active.set(app_id)?;
        self.start_reason = reason;
        self.phase = LifecyclePhase::Starting;
        Ok(())
    }

    pub fn return_target(&mut self) -> Result<AppIdResult<'_>, LifecycleError> {
        self.phase = LifecyclePhase::Starting;
        self.start_reason = StartReason::Return;
        if self.return_len == 0 {
            return Ok(AppIdResult { app_id: "main" });
        }
        self.return_len -= 1;
        Ok(AppIdResult {
            app_id: self.return_stack[self.return_len].as_str(),
        })
    }

    pub fn return_stack_len(&self) -> usize {
        self.return_len
    }

    pub fn return_stack_at(&self, index: usize) -> Option<&str> {
        (index < self.return_len).then(|| self.return_stack[index].as_str())
    }

    pub fn restore_return_stack(&mut self, apps: &[&str]) -> Result<(), LifecycleError> {
        if apps.len() > self.return_stack.len() {
            return Err(LifecycleError::ReturnStackFull);
        }
        self.return_stack = [AppId::empty(); MAX_RETURN_STACK];
        self.return_len = 0;
        for app_id in apps {
            self.return_stack[self.return_len].set(app_id)?;
            self.return_len += 1;
        }
        Ok(())
    }

    pub fn disarm(&mut self, app_id: &str) {
        for timer in &mut self.armed_timers {
            if timer.active && timer.app_id.as_str() == app_id {
                *timer = ArmedTimer::empty();
            }
        }
        for input in &mut self.armed_inputs {
            if input.active && input.app_id.as_str() == app_id {
                *input = ArmedInput::empty();
            }
        }
    }

    pub fn arm(
        &mut self,
        app_id: &str,
        timers: &[TriggerTimer<'_>],
        inputs: &[&str],
    ) -> Result<(), LifecycleError> {
        let timer_count = self
            .armed_timers
            .iter()
            .filter(|timer| timer.active && timer.app_id.as_str() != app_id)
            .count();
        let input_count = self
            .armed_inputs
            .iter()
            .filter(|input| input.active && input.app_id.as_str() != app_id)
            .count();
        if timer_count + timers.len() > MAX_ARMED_TIMERS {
            return Err(LifecycleError::ArmedTimersFull);
        }
        if input_count + inputs.len() > MAX_ARMED_INPUTS {
            return Err(LifecycleError::ArmedInputsFull);
        }
        for event in inputs {
            if self.armed_inputs.iter().any(|input| {
                input.active && input.app_id.as_str() != app_id && input.event.as_str() == *event
            }) {
                return Err(LifecycleError::DuplicateInputOwner);
            }
        }

        self.disarm(app_id);
        for trigger in timers {
            let slot = self
                .armed_timers
                .iter_mut()
                .find(|timer| !timer.active)
                .ok_or(LifecycleError::ArmedTimersFull)?;
            slot.app_id.set(app_id)?;
            slot.event.set(trigger.event)?;
            slot.interval_ms = trigger.interval_ms;
            slot.remaining_ms = trigger.interval_ms;
            slot.repeating = trigger.repeating;
            slot.active = true;
        }
        for event in inputs {
            let slot = self
                .armed_inputs
                .iter_mut()
                .find(|input| !input.active)
                .ok_or(LifecycleError::ArmedInputsFull)?;
            slot.app_id.set(app_id)?;
            slot.event.set(event)?;
            slot.active = true;
        }
        Ok(())
    }

    pub fn armed_len(&self) -> usize {
        self.armed_timers.iter().filter(|item| item.active).count()
            + self.armed_inputs.iter().filter(|item| item.active).count()
    }

    pub fn armed_at(&self, index: usize) -> Option<ArmedRoute<'_>> {
        self.armed_timers
            .iter()
            .filter(|item| item.active)
            .map(|item| ArmedRoute {
                app_id: item.app_id.as_str(),
                event: item.event.as_str(),
            })
            .chain(
                self.armed_inputs
                    .iter()
                    .filter(|item| item.active)
                    .map(|item| ArmedRoute {
                        app_id: item.app_id.as_str(),
                        event: item.event.as_str(),
                    }),
            )
            .nth(index)
    }

    pub fn enqueue_input(&mut self, event: &str) -> Result<(), LifecycleError> {
        let owner_index = self
            .armed_inputs
            .iter()
            .position(|route| route.active && route.event.as_str() == event);
        self.enqueue(2, owner_index, event)
    }

    pub fn enqueue_foreground(&mut self, event: &str) -> Result<(), LifecycleError> {
        self.enqueue(0, None, event)
    }

    pub fn tick(&mut self, elapsed_ms: u32) -> Result<(), LifecycleError> {
        let mut due = [QueuedEvent::empty(); MAX_ARMED_TIMERS];
        let mut due_len = 0;
        for (timer_index, timer) in self.armed_timers.iter_mut().enumerate() {
            if !timer.active {
                continue;
            }
            if elapsed_ms < timer.remaining_ms {
                timer.remaining_ms -= elapsed_ms;
                continue;
            }
            due[due_len].event = timer.event;
            due[due_len].owner_kind = 1;
            due[due_len].owner_index = timer_index as u8;
            due_len += 1;
            if timer.repeating {
                timer.remaining_ms = timer.interval_ms;
            } else {
                timer.active = false;
            }
        }
        for event in &due[..due_len] {
            self.enqueue(1, Some(event.owner_index as usize), event.event.as_str())?;
        }
        Ok(())
    }

    fn enqueue(
        &mut self,
        owner_kind: u8,
        owner_index: Option<usize>,
        event: &str,
    ) -> Result<(), LifecycleError> {
        if self.queue_len == self.queue.len() {
            self.queue_overflowed = true;
            return Err(LifecycleError::EventQueueFull);
        }
        let index = (self.queue_head + self.queue_len) % self.queue.len();
        self.queue[index].owner_kind = owner_index.map_or(0, |_| owner_kind);
        self.queue[index].owner_index = owner_index.unwrap_or(0) as u8;
        self.queue[index].event.set(event)?;
        self.queue_len += 1;
        Ok(())
    }

    pub fn pop_event(&mut self) -> Option<PendingEvent<'_>> {
        if self.queue_len == 0 {
            return None;
        }
        let index = self.queue_head;
        self.queue_head = (self.queue_head + 1) % self.queue.len();
        self.queue_len -= 1;
        let owner = match self.queue[index].owner_kind {
            1 => self
                .armed_timers
                .get(self.queue[index].owner_index as usize)
                .map(|route| route.app_id.as_str()),
            2 => self
                .armed_inputs
                .get(self.queue[index].owner_index as usize)
                .map(|route| route.app_id.as_str()),
            _ => None,
        };
        Some(PendingEvent {
            owner,
            event: self.queue[index].event.as_str(),
        })
    }

    pub const fn queue_len(&self) -> usize {
        self.queue_len
    }

    pub const fn queue_overflowed(&self) -> bool {
        self.queue_overflowed
    }
}

impl Default for ForegroundLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AppIdResult<'a> {
    pub app_id: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_return_stack_and_returns_in_lifo_order() {
        let mut lifecycle = ForegroundLifecycle::new();
        lifecycle
            .begin_foreground("main", StartReason::Boot, false)
            .unwrap();
        lifecycle
            .begin_foreground("reader", StartReason::Launch, true)
            .unwrap();
        lifecycle
            .begin_foreground("menu", StartReason::Launch, true)
            .unwrap();
        assert_eq!(
            lifecycle.begin_foreground("overflow", StartReason::Launch, true),
            Err(LifecycleError::ReturnStackFull)
        );
        assert_eq!(lifecycle.return_target().unwrap().app_id, "reader");
        assert_eq!(lifecycle.return_target().unwrap().app_id, "main");
        assert_eq!(lifecycle.return_target().unwrap().app_id, "main");
    }

    #[test]
    fn duplicate_input_owner_fails_without_replacing_old_owner() {
        let mut lifecycle = ForegroundLifecycle::new();
        lifecycle.arm("first", &[], &["key.POWER"]).unwrap();
        assert_eq!(
            lifecycle.arm("second", &[], &["key.POWER"]),
            Err(LifecycleError::DuplicateInputOwner)
        );
        lifecycle.enqueue_input("key.POWER").unwrap();
        assert_eq!(
            lifecycle.pop_event(),
            Some(PendingEvent {
                owner: Some("first"),
                event: "key.POWER"
            })
        );
    }

    #[test]
    fn timer_and_input_events_preserve_order_and_drop_newest_on_overflow() {
        let mut lifecycle = ForegroundLifecycle::new();
        lifecycle
            .arm(
                "timer",
                &[TriggerTimer {
                    event: "timer.due",
                    interval_ms: 10,
                    repeating: false,
                }],
                &[],
            )
            .unwrap();
        lifecycle.tick(10).unwrap();
        lifecycle.enqueue_input("key.UP").unwrap();
        assert_eq!(lifecycle.pop_event().unwrap().event, "timer.due");
        assert_eq!(lifecycle.pop_event().unwrap().event, "key.UP");
        for _ in 0..MAX_PENDING_EVENTS {
            lifecycle.enqueue_input("key.DOWN").unwrap();
        }
        assert_eq!(
            lifecycle.enqueue_input("key.NEW"),
            Err(LifecycleError::EventQueueFull)
        );
        assert!(lifecycle.queue_overflowed());
        assert_eq!(lifecycle.queue_len(), MAX_PENDING_EVENTS);
    }
}
