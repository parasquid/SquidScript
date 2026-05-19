//! Temporary development harness helpers for the ESP32-C3 Super Mini reference
//! firmware.
//!
//! This module intentionally models the current RAM-only, fixed-slot app store.
//! It is not the final persistent app registry.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevAppId {
    Legacy,
    Main,
    TimerBackground,
    ReaderClock,
    BreakReminder,
}

impl DevAppId {
    pub fn from_runtime_name(name: &str) -> Option<Self> {
        match name {
            "main" => Some(Self::Main),
            "timer-background" => Some(Self::TimerBackground),
            "reader-clock" => Some(Self::ReaderClock),
            "break-reminder" => Some(Self::BreakReminder),
            _ => None,
        }
    }

    pub const fn install_len(
        self,
        main_len: usize,
        timer_background_len: usize,
        reader_clock_len: usize,
        break_reminder_len: usize,
    ) -> usize {
        match self {
            Self::Main => main_len,
            Self::TimerBackground => timer_background_len,
            Self::ReaderClock => reader_clock_len,
            Self::BreakReminder => break_reminder_len,
            Self::Legacy => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevTimerEvent {
    Debug,
    Clock,
    Break,
}

impl DevTimerEvent {
    pub fn from_event(event: &str) -> Option<Self> {
        match event {
            "timer.clock" => Some(Self::Clock),
            "timer.break" => Some(Self::Break),
            "timer.debug" => Some(Self::Debug),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "timer.debug",
            Self::Clock => "timer.clock",
            Self::Break => "timer.break",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_temporary_fixed_app_slots() {
        assert_eq!(DevAppId::from_runtime_name("main"), Some(DevAppId::Main));
        assert_eq!(
            DevAppId::from_runtime_name("timer-background"),
            Some(DevAppId::TimerBackground)
        );
        assert_eq!(DevAppId::from_runtime_name("unknown"), None);
    }

    #[test]
    fn reports_installed_lengths_by_slot() {
        assert_eq!(DevAppId::Main.install_len(10, 20, 30, 40), 10);
        assert_eq!(DevAppId::TimerBackground.install_len(10, 20, 30, 40), 20);
        assert_eq!(DevAppId::ReaderClock.install_len(10, 20, 30, 40), 30);
        assert_eq!(DevAppId::BreakReminder.install_len(10, 20, 30, 40), 40);
        assert_eq!(DevAppId::Legacy.install_len(10, 20, 30, 40), 0);
    }

    #[test]
    fn maps_timer_event_names() {
        assert_eq!(
            DevTimerEvent::from_event("timer.clock"),
            Some(DevTimerEvent::Clock)
        );
        assert_eq!(DevTimerEvent::from_event("timer.unknown"), None);
        assert_eq!(DevTimerEvent::Debug.as_str(), "timer.debug");
        assert_eq!(DevTimerEvent::Break.as_str(), "timer.break");
    }
}
