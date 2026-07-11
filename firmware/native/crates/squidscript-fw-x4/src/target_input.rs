#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedInputButton {
    pub logical: &'static str,
    pub kind: &'static str,
    pub gpio: Option<u8>,
    pub adc: Option<u8>,
    pub min_exclusive: Option<i32>,
    pub max_inclusive: Option<i32>,
    pub active_low: bool,
    pub long_tap: bool,
    pub double_tap: bool,
}

include!(concat!(env!("OUT_DIR"), "/target_input.rs"));

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DebouncedButton {
    candidate_pressed: bool,
    stable_pressed: bool,
    candidate_ms: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PowerGesture {
    held_ms: u32,
    long_emitted: bool,
    pending_short_ms: Option<u32>,
    second_press: bool,
}

pub struct InputClassifier {
    buttons: [DebouncedButton; INPUT_BUTTONS.len()],
    power: PowerGesture,
}

impl InputClassifier {
    pub const fn new() -> Self {
        Self {
            buttons: [DebouncedButton {
                candidate_pressed: false,
                stable_pressed: false,
                candidate_ms: 0,
            }; INPUT_BUTTONS.len()],
            power: PowerGesture {
                held_ms: 0,
                long_emitted: false,
                pending_short_ms: None,
                second_press: false,
            },
        }
    }

    pub fn sample(
        &mut self,
        adc1: u16,
        adc2: u16,
        power_high: bool,
        elapsed_ms: u32,
        mut emit: impl FnMut(&'static str),
    ) {
        self.advance_power(elapsed_ms, &mut emit);
        for (index, button) in INPUT_BUTTONS.iter().enumerate() {
            let raw_pressed = raw_button_pressed(button, adc1, adc2, power_high);
            let stable_pressed = {
                let state = &mut self.buttons[index];
                if raw_pressed != state.candidate_pressed {
                    state.candidate_pressed = raw_pressed;
                    state.candidate_ms = elapsed_ms;
                } else {
                    state.candidate_ms = state.candidate_ms.saturating_add(elapsed_ms);
                }
                if state.candidate_pressed == state.stable_pressed
                    || state.candidate_ms < INPUT_DEBOUNCE_MS
                {
                    continue;
                }
                state.stable_pressed = state.candidate_pressed;
                state.stable_pressed
            };
            if button.logical == "POWER" {
                self.power_transition(stable_pressed, &mut emit);
            } else if !stable_pressed {
                if let Some(event) = base_event(button.logical) {
                    emit(event);
                }
            }
        }
    }

    pub fn stable_mask(&self) -> u8 {
        self.buttons
            .iter()
            .enumerate()
            .fold(0u8, |mask, (index, state)| {
                mask | (u8::from(state.stable_pressed) << index)
            })
    }

    fn advance_power(&mut self, elapsed_ms: u32, emit: &mut impl FnMut(&'static str)) {
        let power_pressed = INPUT_BUTTONS
            .iter()
            .position(|button| button.logical == "POWER")
            .map(|index| self.buttons[index].stable_pressed)
            .unwrap_or(false);
        if power_pressed {
            self.power.held_ms = self.power.held_ms.saturating_add(elapsed_ms);
            if !self.power.long_emitted && self.power.held_ms >= INPUT_LONG_TAP_MS {
                self.power.long_emitted = true;
                self.power.pending_short_ms = None;
                self.power.second_press = false;
                emit("key.POWER.longTap");
            }
        } else if let Some(remaining) = self.power.pending_short_ms {
            if elapsed_ms >= remaining {
                self.power.pending_short_ms = None;
                emit("key.POWER");
            } else {
                self.power.pending_short_ms = Some(remaining - elapsed_ms);
            }
        }
    }

    fn power_transition(&mut self, pressed: bool, emit: &mut impl FnMut(&'static str)) {
        if pressed {
            self.power.held_ms = 0;
            self.power.long_emitted = false;
            if self.power.pending_short_ms.take().is_some() {
                self.power.second_press = true;
            }
            return;
        }
        if self.power.long_emitted {
            self.power.held_ms = 0;
            self.power.long_emitted = false;
            self.power.second_press = false;
        } else if self.power.second_press {
            self.power.held_ms = 0;
            self.power.second_press = false;
            emit("key.POWER.doubleTap");
        } else {
            self.power.pending_short_ms = Some(INPUT_DOUBLE_TAP_WINDOW_MS);
        }
    }
}

pub fn adc_bucket(adc: u8, value: u16) -> &'static str {
    INPUT_BUTTONS
        .iter()
        .find(|button| button.adc == Some(adc) && raw_button_pressed(button, value, value, true))
        .map(|button| button.logical)
        .unwrap_or("none")
}

pub const fn power_sleep_may_begin(requested: bool, power_high: bool) -> bool {
    requested && power_high
}

impl Default for InputClassifier {
    fn default() -> Self {
        Self::new()
    }
}

fn raw_button_pressed(
    button: &GeneratedInputButton,
    adc1: u16,
    adc2: u16,
    power_high: bool,
) -> bool {
    if button.kind == "gpio-button" {
        return if button.active_low {
            !power_high
        } else {
            power_high
        };
    }
    let value = match button.adc {
        Some(1) => i32::from(adc1),
        Some(2) => i32::from(adc2),
        _ => return false,
    };
    button.min_exclusive.is_none_or(|min| value > min)
        && button.max_inclusive.is_none_or(|max| value <= max)
}

fn base_event(logical: &str) -> Option<&'static str> {
    match logical {
        "UP" => Some("key.UP"),
        "DOWN" => Some("key.DOWN"),
        "LEFT" => Some("key.LEFT"),
        "RIGHT" => Some("key.RIGHT"),
        "SELECT" => Some("key.SELECT"),
        "BACK" => Some("key.BACK"),
        "POWER" => Some("key.POWER"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_x4_input_policy_matches_target_definition() {
        assert_eq!(INPUT_DEBOUNCE_MS, 5);
        assert_eq!(INPUT_LONG_TAP_MS, 350);
        assert_eq!(INPUT_DOUBLE_TAP_WINDOW_MS, 350);
        assert_eq!(INPUT_BUTTONS.len(), 7);
        let power = INPUT_BUTTONS
            .iter()
            .find(|button| button.logical == "POWER")
            .unwrap();
        assert_eq!(power.gpio, Some(3));
        assert!(power.active_low && power.long_tap && power.double_tap);
        assert!(INPUT_BUTTONS
            .iter()
            .filter(|button| button.logical != "POWER")
            .all(|button| !button.long_tap && !button.double_tap));
    }

    #[test]
    fn power_sleep_waits_for_the_triggering_press_to_be_released() {
        assert!(!power_sleep_may_begin(true, false));
        assert!(power_sleep_may_begin(true, true));
        assert!(!power_sleep_may_begin(false, true));
    }

    fn sample(
        classifier: &mut InputClassifier,
        adc1: u16,
        adc2: u16,
        power_high: bool,
        elapsed_ms: u32,
    ) -> std::vec::Vec<&'static str> {
        let mut events = std::vec::Vec::new();
        classifier.sample(adc1, adc2, power_high, elapsed_ms, |event| {
            events.push(event)
        });
        events
    }

    #[test]
    fn adc_ranges_use_exclusive_minimum_and_inclusive_maximum() {
        for button in INPUT_BUTTONS.iter().filter(|button| button.adc.is_some()) {
            if let Some(min) = button.min_exclusive {
                let min = u16::try_from(min).unwrap();
                assert!(!raw_button_pressed(button, min, min, true));
                assert!(raw_button_pressed(button, min + 1, min + 1, true));
            }
            let max = u16::try_from(button.max_inclusive.unwrap()).unwrap();
            assert!(raw_button_pressed(button, max, max, true));
            assert!(!raw_button_pressed(button, max + 1, max + 1, true));
        }
    }

    #[test]
    fn ordinary_buttons_emit_once_on_debounced_release() {
        let mut classifier = InputClassifier::new();
        assert!(sample(&mut classifier, 1000, 4095, true, 4).is_empty());
        assert!(sample(&mut classifier, 1000, 4095, true, 1).is_empty());
        assert!(sample(&mut classifier, 4095, 4095, true, 4).is_empty());
        assert_eq!(sample(&mut classifier, 4095, 4095, true, 1), ["key.LEFT"]);
    }

    #[test]
    fn no_button_regions_do_not_emit() {
        let mut classifier = InputClassifier::new();
        assert!(sample(&mut classifier, 3000, 3000, true, 20).is_empty());
    }

    #[test]
    fn active_low_power_single_tap_waits_out_double_window() {
        let mut classifier = InputClassifier::new();
        assert!(sample(&mut classifier, 4095, 4095, false, 5).is_empty());
        assert!(sample(&mut classifier, 4095, 4095, true, 5).is_empty());
        assert!(sample(
            &mut classifier,
            4095,
            4095,
            true,
            INPUT_DOUBLE_TAP_WINDOW_MS - 1,
        )
        .is_empty());
        assert_eq!(sample(&mut classifier, 4095, 4095, true, 1), ["key.POWER"]);
    }

    #[test]
    fn power_long_tap_emits_at_threshold_and_suppresses_short() {
        let mut classifier = InputClassifier::new();
        assert!(sample(&mut classifier, 4095, 4095, false, 5).is_empty());
        assert!(sample(&mut classifier, 4095, 4095, false, INPUT_LONG_TAP_MS - 1,).is_empty());
        assert_eq!(
            sample(&mut classifier, 4095, 4095, false, 1),
            ["key.POWER.longTap"]
        );
        assert!(sample(&mut classifier, 4095, 4095, false, 20).is_empty());
        assert!(sample(&mut classifier, 4095, 4095, true, 5).is_empty());
        assert!(sample(
            &mut classifier,
            4095,
            4095,
            true,
            INPUT_DOUBLE_TAP_WINDOW_MS,
        )
        .is_empty());
    }

    #[test]
    fn second_power_press_within_window_emits_double_on_release() {
        let mut classifier = InputClassifier::new();
        assert!(sample(&mut classifier, 4095, 4095, false, 5).is_empty());
        assert!(sample(&mut classifier, 4095, 4095, true, 5).is_empty());
        assert!(sample(&mut classifier, 4095, 4095, true, 100).is_empty());
        assert!(sample(&mut classifier, 4095, 4095, false, 5).is_empty());
        assert_eq!(
            sample(&mut classifier, 4095, 4095, true, 5),
            ["key.POWER.doubleTap"]
        );
    }
}
