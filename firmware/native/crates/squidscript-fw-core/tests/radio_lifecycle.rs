use squidscript_fw_core::radio_lifecycle::{
    evaluate_reusable_reclaim, format_cycle_snapshot, format_reclaim_summary, CycleSnapshot,
    RadioKind, ReclaimGate,
};

const STRICT_GATE: ReclaimGate = ReclaimGate {
    min_absolute_reclaim_bytes: 4 * 1024,
    max_unreclaimed_ratio_per_mille: 100,
    warmup_cycle_count: 0,
};

const WARMED_GATE: ReclaimGate = ReclaimGate {
    min_absolute_reclaim_bytes: 4 * 1024,
    max_unreclaimed_ratio_per_mille: 100,
    warmup_cycle_count: 1,
};

#[test]
fn reusable_reclaim_passes_when_cycles_return_near_baseline() {
    let cycles = [
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 120_000,
            active_free_bytes: 82_000,
            after_deinit_free_bytes: 117_000,
            before_largest_free_block: Some(88_000),
            after_largest_free_block: Some(87_500),
        },
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 117_000,
            active_free_bytes: 82_000,
            after_deinit_free_bytes: 116_000,
            before_largest_free_block: Some(87_500),
            after_largest_free_block: Some(87_500),
        },
    ];

    let summary = evaluate_reusable_reclaim(RadioKind::Wifi, &cycles, STRICT_GATE);

    assert!(summary.passed, "{summary:?}");
    assert_eq!(summary.cycle_count, 2);
    assert_eq!(summary.worst_unreclaimed_bytes, 3_000);
}

#[test]
fn reusable_reclaim_fails_when_service_ram_remains_live() {
    let cycles = [CycleSnapshot {
        radio: RadioKind::Ble,
        before_free_bytes: 120_000,
        active_free_bytes: 70_000,
        after_deinit_free_bytes: 80_000,
        before_largest_free_block: Some(90_000),
        after_largest_free_block: Some(89_000),
    }];

    let summary = evaluate_reusable_reclaim(RadioKind::Ble, &cycles, STRICT_GATE);

    assert!(!summary.passed, "{summary:?}");
    assert_eq!(summary.worst_unreclaimed_bytes, 40_000);
}

#[test]
fn reusable_reclaim_passes_with_stable_warmed_baseline_after_first_cycle() {
    let cycles = [
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 102_400,
            active_free_bytes: 55_968,
            after_deinit_free_bytes: 93_544,
            before_largest_free_block: None,
            after_largest_free_block: None,
        },
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 93_544,
            active_free_bytes: 55_872,
            after_deinit_free_bytes: 93_544,
            before_largest_free_block: None,
            after_largest_free_block: None,
        },
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 93_544,
            active_free_bytes: 55_872,
            after_deinit_free_bytes: 93_544,
            before_largest_free_block: None,
            after_largest_free_block: None,
        },
    ];

    let summary = evaluate_reusable_reclaim(RadioKind::Wifi, &cycles, WARMED_GATE);

    assert!(summary.passed, "{summary:?}");
    assert_eq!(summary.cold_retained_bytes, 8_856);
    assert_eq!(summary.worst_unreclaimed_bytes, 0);
}

#[test]
fn reusable_reclaim_requires_post_warmup_cycle_evidence() {
    let cycles = [CycleSnapshot {
        radio: RadioKind::Wifi,
        before_free_bytes: 102_400,
        active_free_bytes: 55_968,
        after_deinit_free_bytes: 93_544,
        before_largest_free_block: None,
        after_largest_free_block: None,
    }];

    let summary = evaluate_reusable_reclaim(RadioKind::Wifi, &cycles, WARMED_GATE);

    assert!(!summary.passed, "{summary:?}");
    assert_eq!(summary.cold_retained_bytes, 8_856);
    assert_eq!(summary.worst_unreclaimed_bytes, 0);
}

#[test]
fn reusable_reclaim_fails_on_monotonic_largest_block_loss() {
    let cycles = [
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 120_000,
            active_free_bytes: 86_000,
            after_deinit_free_bytes: 118_000,
            before_largest_free_block: Some(90_000),
            after_largest_free_block: Some(88_000),
        },
        CycleSnapshot {
            radio: RadioKind::Wifi,
            before_free_bytes: 118_000,
            active_free_bytes: 86_000,
            after_deinit_free_bytes: 117_000,
            before_largest_free_block: Some(88_000),
            after_largest_free_block: Some(85_000),
        },
    ];

    let summary = evaluate_reusable_reclaim(RadioKind::Wifi, &cycles, STRICT_GATE);

    assert!(!summary.passed, "{summary:?}");
    assert!(summary.largest_block_regressed);
}

#[test]
fn reclaim_summary_formats_redacted_serial_line() {
    let cycles = [CycleSnapshot {
        radio: RadioKind::Wifi,
        before_free_bytes: 120_000,
        active_free_bytes: 82_000,
        after_deinit_free_bytes: 117_000,
        before_largest_free_block: Some(88_000),
        after_largest_free_block: Some(88_000),
    }];
    let summary = evaluate_reusable_reclaim(RadioKind::Wifi, &cycles, STRICT_GATE);
    let mut line = String::new();

    format_reclaim_summary(&summary, &mut line).unwrap();

    assert!(line.contains("radio=wifi"));
    assert!(line.contains("cycles=1"));
    assert!(line.contains("passed=true"));
    assert!(line.contains("worst_unreclaimed=3000"));
    assert!(!line.contains("ssid"));
    assert!(!line.contains("bssid"));
    assert!(!line.contains("mac"));
    assert!(!line.contains("ip="));
}

#[test]
fn cycle_snapshot_formats_redacted_serial_line() {
    let snapshot = CycleSnapshot {
        radio: RadioKind::Ble,
        before_free_bytes: 118_000,
        active_free_bytes: 74_000,
        after_deinit_free_bytes: 116_500,
        before_largest_free_block: None,
        after_largest_free_block: Some(82_000),
    };
    let mut line = String::new();

    format_cycle_snapshot(3, &snapshot, &mut line).unwrap();

    assert!(line.contains("radio=ble"));
    assert!(line.contains("cycle=3"));
    assert!(line.contains("before_free=118000"));
    assert!(line.contains("active_free=74000"));
    assert!(line.contains("after_deinit_free=116500"));
    assert!(line.contains("before_largest_free_block=unknown"));
    assert!(line.contains("after_largest_free_block=82000"));
    assert!(!line.contains("ssid"));
    assert!(!line.contains("bssid"));
    assert!(!line.contains("mac"));
    assert!(!line.contains("ip="));
}
