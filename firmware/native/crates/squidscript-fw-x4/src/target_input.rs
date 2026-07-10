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
}
