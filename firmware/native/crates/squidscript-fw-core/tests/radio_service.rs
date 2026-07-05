use squidscript_fw_core::radio_lifecycle::RadioKind;
use squidscript_fw_core::radio_service::{RadioLeaseManager, RadioLeaseState, ServiceLeaseError};

#[test]
fn wifi_and_ble_can_be_active_together() {
    let mut manager = RadioLeaseManager::new();

    assert_eq!(manager.acquire(RadioKind::Wifi), Ok(()));
    assert_eq!(manager.acquire(RadioKind::Ble), Ok(()));

    assert_eq!(manager.state(RadioKind::Wifi), RadioLeaseState::Active);
    assert_eq!(manager.state(RadioKind::Ble), RadioLeaseState::Active);
    assert_eq!(manager.active_count(), 2);
}

#[test]
fn duplicate_acquire_reports_existing_active_lease() {
    let mut manager = RadioLeaseManager::new();

    assert_eq!(manager.acquire(RadioKind::Wifi), Ok(()));

    assert_eq!(
        manager.acquire(RadioKind::Wifi),
        Err(ServiceLeaseError::AlreadyActive)
    );
    assert_eq!(manager.active_count(), 1);
}

#[test]
fn release_only_changes_the_requested_radio() {
    let mut manager = RadioLeaseManager::new();

    manager.acquire(RadioKind::Wifi).unwrap();
    manager.acquire(RadioKind::Ble).unwrap();

    assert_eq!(manager.release(RadioKind::Wifi), Ok(()));

    assert_eq!(manager.state(RadioKind::Wifi), RadioLeaseState::Inactive);
    assert_eq!(manager.state(RadioKind::Ble), RadioLeaseState::Active);
    assert_eq!(manager.active_count(), 1);
}

#[test]
fn release_requires_an_active_lease() {
    let mut manager = RadioLeaseManager::new();

    assert_eq!(
        manager.release(RadioKind::Ble),
        Err(ServiceLeaseError::NotActive)
    );
}

#[test]
fn reset_releases_all_radio_leases() {
    let mut manager = RadioLeaseManager::new();

    manager.acquire(RadioKind::Wifi).unwrap();
    manager.acquire(RadioKind::Ble).unwrap();

    manager.release_all();

    assert_eq!(manager.state(RadioKind::Wifi), RadioLeaseState::Inactive);
    assert_eq!(manager.state(RadioKind::Ble), RadioLeaseState::Inactive);
    assert_eq!(manager.active_count(), 0);
}
