use core::convert::Infallible;

use squidscript_fw_core::radio_lifecycle::{CycleSnapshot, RadioKind};
use squidscript_fw_x4::radio_probe::{
    radio_stack_metadata, run_probe_cycles, RadioCycleRunner, ESP_RADIO_VERSION,
};

struct FixedRunner<'a> {
    snapshots: &'a [CycleSnapshot],
    index: usize,
}

impl RadioCycleRunner for FixedRunner<'_> {
    type Error = Infallible;

    fn run_cycle(&mut self, radio: RadioKind) -> Result<CycleSnapshot, Self::Error> {
        let snapshot = self.snapshots[self.index];
        self.index += 1;
        assert_eq!(snapshot.radio, radio);
        Ok(snapshot)
    }
}

#[test]
fn probe_cycles_return_summary_and_redacted_serial_line() {
    let snapshots = [
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 120_000,
            active_free_bytes: 82_000,
            after_deinit_free_bytes: 117_000,
            before_largest_free_block: Some(88_000),
            after_largest_free_block: Some(88_000),
        },
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 117_000,
            active_free_bytes: 82_000,
            after_deinit_free_bytes: 116_000,
            before_largest_free_block: Some(88_000),
            after_largest_free_block: Some(88_000),
        },
    ];
    let mut runner = FixedRunner {
        snapshots: &snapshots,
        index: 0,
    };
    let mut scratch = [CycleSnapshot {
        radio: RadioKind::Wifi,
        before_free_bytes: 0,
        active_free_bytes: 0,
        after_deinit_free_bytes: 0,
        before_largest_free_block: None,
        after_largest_free_block: None,
    }; 2];
    let mut line = String::new();

    let summary = run_probe_cycles(RadioKind::Wifi, &mut runner, &mut scratch, &mut line).unwrap();

    assert!(summary.passed, "{summary:?}");
    assert_eq!(summary.cycle_count, 2);
    assert!(line.contains("radio=wifi"));
    assert!(line.contains("cycles=2"));
    assert!(!line.contains("ssid"));
    assert!(!line.contains("mac"));
    assert!(!line.contains("ip="));
}

#[test]
fn radio_stack_metadata_tracks_current_esp32c3_stack() {
    let metadata = radio_stack_metadata();

    assert_eq!(metadata.stack, "esp-radio");
    assert_eq!(metadata.version, ESP_RADIO_VERSION);
    assert_eq!(metadata.version, "1.0.0-beta.0");
    assert!(metadata.features.contains(&"esp32c3"));
    assert!(metadata.features.contains(&"wifi"));
    assert!(metadata.features.contains(&"ble"));
    assert!(metadata.features.contains(&"unstable"));
}
