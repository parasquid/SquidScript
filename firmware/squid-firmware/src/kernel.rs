#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceError {
    QueueFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndicatorAction {
    SetBrightness(u8),
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
}
