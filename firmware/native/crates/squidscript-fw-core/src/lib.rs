#![cfg_attr(not(test), no_std)]

pub mod app_store;
pub mod lifecycle;
pub mod native_runtime;

pub mod radio_lifecycle {
    use core::fmt;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum RadioKind {
        Wifi,
        Ble,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CycleSnapshot {
        pub radio: RadioKind,
        pub before_free_bytes: usize,
        pub active_free_bytes: usize,
        pub after_deinit_free_bytes: usize,
        pub before_largest_free_block: Option<usize>,
        pub after_largest_free_block: Option<usize>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReclaimGate {
        pub min_absolute_reclaim_bytes: usize,
        pub max_unreclaimed_ratio_per_mille: u16,
        pub warmup_cycle_count: usize,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReclaimSummary {
        pub radio: RadioKind,
        pub cycle_count: usize,
        pub passed: bool,
        pub worst_unreclaimed_bytes: usize,
        pub worst_unreclaimed_ratio_per_mille: u16,
        pub cold_retained_bytes: usize,
        pub largest_block_regressed: bool,
    }

    pub fn evaluate_reusable_reclaim(
        radio: RadioKind,
        cycles: &[CycleSnapshot],
        gate: ReclaimGate,
    ) -> ReclaimSummary {
        let mut worst_unreclaimed_bytes = 0;
        let mut worst_unreclaimed_ratio_per_mille = 0;
        let mut largest_block_regressed = false;
        let mut previous_after_largest = None;
        let mut passed = !cycles.is_empty() && cycles.len() > gate.warmup_cycle_count;

        let mut cold_retained_bytes = 0;

        for (index, cycle) in cycles.iter().enumerate() {
            if cycle.radio != radio {
                passed = false;
            }

            let service_delta = cycle
                .before_free_bytes
                .saturating_sub(cycle.active_free_bytes);
            let unreclaimed = cycle
                .before_free_bytes
                .saturating_sub(cycle.after_deinit_free_bytes);
            let ratio = ratio_per_mille(unreclaimed, service_delta);
            if index < gate.warmup_cycle_count {
                cold_retained_bytes = cold_retained_bytes.max(unreclaimed);
            } else {
                worst_unreclaimed_bytes = worst_unreclaimed_bytes.max(unreclaimed);
                worst_unreclaimed_ratio_per_mille = worst_unreclaimed_ratio_per_mille.max(ratio);

                if unreclaimed > gate.min_absolute_reclaim_bytes
                    && ratio > gate.max_unreclaimed_ratio_per_mille
                {
                    passed = false;
                }
            }

            if let (Some(previous), Some(after)) =
                (previous_after_largest, cycle.after_largest_free_block)
            {
                if after < previous {
                    largest_block_regressed = true;
                    passed = false;
                }
            }
            previous_after_largest = cycle.after_largest_free_block;
        }

        ReclaimSummary {
            radio,
            cycle_count: cycles.len(),
            passed,
            worst_unreclaimed_bytes,
            worst_unreclaimed_ratio_per_mille,
            cold_retained_bytes,
            largest_block_regressed,
        }
    }

    fn ratio_per_mille(numerator: usize, denominator: usize) -> u16 {
        if denominator == 0 {
            return if numerator == 0 { 0 } else { u16::MAX };
        }
        let ratio = numerator.saturating_mul(1000) / denominator;
        ratio.min(usize::from(u16::MAX)) as u16
    }

    pub fn format_reclaim_summary(
        summary: &ReclaimSummary,
        out: &mut dyn fmt::Write,
    ) -> fmt::Result {
        write!(
            out,
            "radio={} cycles={} passed={} cold_retained={} worst_unreclaimed={} worst_unreclaimed_per_mille={} largest_block_regressed={}",
            radio_name(summary.radio),
            summary.cycle_count,
            summary.passed,
            summary.cold_retained_bytes,
            summary.worst_unreclaimed_bytes,
            summary.worst_unreclaimed_ratio_per_mille,
            summary.largest_block_regressed
        )
    }

    pub fn format_cycle_snapshot(
        cycle_index: usize,
        snapshot: &CycleSnapshot,
        out: &mut dyn fmt::Write,
    ) -> fmt::Result {
        write!(
            out,
            "radio={} cycle={} before_free={} active_free={} after_deinit_free={} before_largest_free_block=",
            radio_name(snapshot.radio),
            cycle_index,
            snapshot.before_free_bytes,
            snapshot.active_free_bytes,
            snapshot.after_deinit_free_bytes
        )?;
        format_optional_usize(snapshot.before_largest_free_block, out)?;
        write!(out, " after_largest_free_block=")?;
        format_optional_usize(snapshot.after_largest_free_block, out)
    }

    fn format_optional_usize(value: Option<usize>, out: &mut dyn fmt::Write) -> fmt::Result {
        match value {
            Some(value) => write!(out, "{value}"),
            None => out.write_str("unknown"),
        }
    }

    pub const fn radio_name(radio: RadioKind) -> &'static str {
        match radio {
            RadioKind::Wifi => "wifi",
            RadioKind::Ble => "ble",
        }
    }
}

pub mod radio_service {
    use crate::radio_lifecycle::RadioKind;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum RadioLeaseState {
        Inactive,
        Active,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ServiceLeaseError {
        AlreadyActive,
        NotActive,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct RadioLeaseManager {
        wifi: RadioLeaseState,
        ble: RadioLeaseState,
    }

    impl RadioLeaseManager {
        pub const fn new() -> Self {
            Self {
                wifi: RadioLeaseState::Inactive,
                ble: RadioLeaseState::Inactive,
            }
        }

        pub fn acquire(&mut self, radio: RadioKind) -> Result<(), ServiceLeaseError> {
            let state = self.state_mut(radio);
            if *state == RadioLeaseState::Active {
                return Err(ServiceLeaseError::AlreadyActive);
            }
            *state = RadioLeaseState::Active;
            Ok(())
        }

        pub fn release(&mut self, radio: RadioKind) -> Result<(), ServiceLeaseError> {
            let state = self.state_mut(radio);
            if *state == RadioLeaseState::Inactive {
                return Err(ServiceLeaseError::NotActive);
            }
            *state = RadioLeaseState::Inactive;
            Ok(())
        }

        pub fn release_all(&mut self) {
            self.wifi = RadioLeaseState::Inactive;
            self.ble = RadioLeaseState::Inactive;
        }

        pub const fn state(&self, radio: RadioKind) -> RadioLeaseState {
            match radio {
                RadioKind::Wifi => self.wifi,
                RadioKind::Ble => self.ble,
            }
        }

        pub const fn active_count(&self) -> usize {
            let wifi = match self.wifi {
                RadioLeaseState::Inactive => 0,
                RadioLeaseState::Active => 1,
            };
            let ble = match self.ble {
                RadioLeaseState::Inactive => 0,
                RadioLeaseState::Active => 1,
            };
            wifi + ble
        }

        fn state_mut(&mut self, radio: RadioKind) -> &mut RadioLeaseState {
            match radio {
                RadioKind::Wifi => &mut self.wifi,
                RadioKind::Ble => &mut self.ble,
            }
        }
    }

    impl Default for RadioLeaseManager {
        fn default() -> Self {
            Self::new()
        }
    }
}
