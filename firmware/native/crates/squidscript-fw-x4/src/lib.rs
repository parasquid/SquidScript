#![cfg_attr(not(test), no_std)]

pub mod radio_probe {
    use core::fmt;

    use squidscript_fw_core::radio_lifecycle::{
        evaluate_reusable_reclaim, format_reclaim_summary, CycleSnapshot, RadioKind, ReclaimGate,
        ReclaimSummary,
    };

    pub const REUSABLE_RECLAIM_GATE: ReclaimGate = ReclaimGate {
        min_absolute_reclaim_bytes: 4 * 1024,
        max_unreclaimed_ratio_per_mille: 100,
        warmup_cycle_count: 1,
    };
    pub const ESP_RADIO_VERSION: &str = "1.0.0-beta.0";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct RadioStackMetadata {
        pub stack: &'static str,
        pub version: &'static str,
        pub features: &'static [&'static str],
    }

    pub const fn radio_stack_metadata() -> RadioStackMetadata {
        RadioStackMetadata {
            stack: "esp-radio",
            version: ESP_RADIO_VERSION,
            features: &["esp32c3", "wifi", "ble", "unstable"],
        }
    }

    pub trait RadioCycleRunner {
        type Error;

        fn run_cycle(&mut self, radio: RadioKind) -> Result<CycleSnapshot, Self::Error>;
    }

    pub fn run_probe_cycles<R: RadioCycleRunner>(
        radio: RadioKind,
        runner: &mut R,
        snapshots: &mut [CycleSnapshot],
        serial_line: &mut dyn fmt::Write,
    ) -> Result<ReclaimSummary, R::Error> {
        for snapshot in snapshots.iter_mut() {
            *snapshot = runner.run_cycle(radio)?;
        }
        let summary = evaluate_reusable_reclaim(radio, snapshots, REUSABLE_RECLAIM_GATE);
        let _ = format_reclaim_summary(&summary, serial_line);
        Ok(summary)
    }
}
