pub const BLE_PIPELINE_CHUNK_BYTES: usize = 192;
pub const BLE_PIPELINE_DEPTH: usize = 4;
pub const BLE_PIPELINE_BUFFER_BUDGET_BYTES: usize = 2_048;
pub const BLE_GATT_ATTRIBUTE_CAPACITY: usize = 17;
pub const DEFAULT_BLE_CONNECTION_WATCHDOG_MS: u64 = 30_000;

pub const fn ble_connection_watchdog_ms(raw: Option<&str>) -> u64 {
    let Some(raw) = raw else {
        return DEFAULT_BLE_CONNECTION_WATCHDOG_MS;
    };
    let bytes = raw.as_bytes();
    if bytes.is_empty() {
        return DEFAULT_BLE_CONNECTION_WATCHDOG_MS;
    }
    let mut value = 0u64;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte < b'0' || byte > b'9' {
            return DEFAULT_BLE_CONNECTION_WATCHDOG_MS;
        }
        let Some(base) = value.checked_mul(10) else {
            return DEFAULT_BLE_CONNECTION_WATCHDOG_MS;
        };
        let Some(next) = base.checked_add((byte - b'0') as u64) else {
            return DEFAULT_BLE_CONNECTION_WATCHDOG_MS;
        };
        value = next;
        index += 1;
    }
    if value == 0 {
        DEFAULT_BLE_CONNECTION_WATCHDOG_MS
    } else {
        value
    }
}

pub const fn required_gatt_attribute_count() -> usize {
    let gap_service = 1 + 2 + 2;
    let gatt_service = 1 + 3;
    let transfer_service = 1 + 2 + 2 + 3;
    gap_service + gatt_service + transfer_service
}

pub const fn should_report_ble_stage(reported: usize, observed: usize) -> bool {
    observed > reported
}

const _: () = assert!(BLE_GATT_ATTRIBUTE_CAPACITY >= required_gatt_attribute_count());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferSessionId(u32);

impl TransferSessionId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BleUploadRoute {
    pub name: heapless::String<64>,
    pub profile_id: heapless::String<32>,
    pub complete_event: heapless::String<64>,
    pub total_len: usize,
}

impl BleUploadRoute {
    pub fn new(
        name: &str,
        profile_id: &str,
        complete_event: &str,
        total_len: usize,
    ) -> Result<Self, BlePipelineError> {
        if total_len == 0 {
            return Err(BlePipelineError::InvalidLength);
        }
        let name = heapless::String::try_from(name).map_err(|_| BlePipelineError::TooLarge)?;
        let profile_id =
            heapless::String::try_from(profile_id).map_err(|_| BlePipelineError::TooLarge)?;
        let complete_event =
            heapless::String::try_from(complete_event).map_err(|_| BlePipelineError::TooLarge)?;
        Ok(Self {
            name,
            profile_id,
            complete_event,
            total_len,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BleStorageCommand {
    Begin {
        session_id: TransferSessionId,
        route: BleUploadRoute,
    },
    Chunk {
        session_id: TransferSessionId,
        offset: usize,
        len: usize,
        bytes: [u8; BLE_PIPELINE_CHUNK_BYTES],
    },
    Commit {
        session_id: TransferSessionId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlePipelineError {
    Busy,
    Idle,
    Incomplete,
    InvalidLength,
    InvalidOffset,
    StaleSession,
    TooLarge,
}

#[derive(Clone, Debug)]
pub struct BleStorageSession {
    id: Option<TransferSessionId>,
    route: Option<BleUploadRoute>,
    received: usize,
}

impl BleStorageSession {
    pub const fn new() -> Self {
        Self {
            id: None,
            route: None,
            received: 0,
        }
    }

    pub fn begin(
        &mut self,
        id: TransferSessionId,
        route: BleUploadRoute,
    ) -> Result<(), BlePipelineError> {
        if self.id.is_some() {
            return Err(BlePipelineError::Busy);
        }
        self.id = Some(id);
        self.route = Some(route);
        self.received = 0;
        Ok(())
    }

    pub fn accept_chunk(
        &mut self,
        id: TransferSessionId,
        offset: usize,
        len: usize,
    ) -> Result<bool, BlePipelineError> {
        self.require_session(id)?;
        let route = self.route.as_ref().ok_or(BlePipelineError::Idle)?;
        if len == 0 || offset != self.received || offset.saturating_add(len) > route.total_len {
            return Err(BlePipelineError::InvalidOffset);
        }
        self.received = self.received.saturating_add(len);
        Ok(self.received == route.total_len)
    }

    pub fn commit(&self, id: TransferSessionId) -> Result<(), BlePipelineError> {
        self.require_session(id)?;
        let route = self.route.as_ref().ok_or(BlePipelineError::Idle)?;
        if self.received != route.total_len {
            return Err(BlePipelineError::Incomplete);
        }
        Ok(())
    }

    pub fn cancel(&mut self, id: TransferSessionId) -> bool {
        if self.id != Some(id) {
            return false;
        }
        self.clear();
        true
    }

    pub fn clear(&mut self) {
        self.id = None;
        self.route = None;
        self.received = 0;
    }

    pub const fn received(&self) -> usize {
        self.received
    }

    pub fn route(&self) -> Option<&BleUploadRoute> {
        self.route.as_ref()
    }

    fn require_session(&self, id: TransferSessionId) -> Result<(), BlePipelineError> {
        match self.id {
            Some(active) if active == id => Ok(()),
            Some(_) => Err(BlePipelineError::StaleSession),
            None => Err(BlePipelineError::Idle),
        }
    }
}

impl Default for BleStorageSession {
    fn default() -> Self {
        Self::new()
    }
}

const _: () = assert!(
    core::mem::size_of::<BleStorageCommand>() * BLE_PIPELINE_DEPTH
        + BLE_PIPELINE_CHUNK_BYTES * 2
        + core::mem::size_of::<BleStorageSession>()
        <= BLE_PIPELINE_BUFFER_BUDGET_BYTES
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_buffer_budget_is_bounded() {
        assert_eq!(BLE_PIPELINE_CHUNK_BYTES, 192);
        assert_eq!(BLE_PIPELINE_DEPTH, 4);
        assert!(BLE_PIPELINE_BUFFER_BUDGET_BYTES <= 2_048);
        assert!(core::mem::size_of::<BleStorageCommand>() <= 272);
    }

    #[test]
    fn bounded_queue_refuses_a_fifth_chunk_until_one_is_consumed() {
        use embassy_sync_07::{blocking_mutex::raw::NoopRawMutex, channel::Channel};

        let queue = Channel::<NoopRawMutex, BleStorageCommand, BLE_PIPELINE_DEPTH>::new();
        for offset in 0..BLE_PIPELINE_DEPTH {
            assert!(queue
                .try_send(chunk_command(offset * BLE_PIPELINE_CHUNK_BYTES))
                .is_ok());
        }
        assert!(queue.try_send(chunk_command(768)).is_err());
        assert!(queue.try_receive().is_ok());
        assert!(queue.try_send(chunk_command(768)).is_ok());
    }

    fn chunk_command(offset: usize) -> BleStorageCommand {
        BleStorageCommand::Chunk {
            session_id: TransferSessionId::new(1),
            offset,
            len: BLE_PIPELINE_CHUNK_BYTES,
            bytes: [0; BLE_PIPELINE_CHUNK_BYTES],
        }
    }

    #[test]
    fn chunks_are_ordered_and_commit_only_at_exact_length() {
        let mut session = BleStorageSession::new();
        let route = BleUploadRoute::new("book.binbook", "reader", "ble.complete", 26).unwrap();
        assert_eq!(session.begin(TransferSessionId::new(7), route), Ok(()));
        assert_eq!(
            session.accept_chunk(TransferSessionId::new(7), 0, 13),
            Ok(false)
        );
        assert_eq!(
            session.accept_chunk(TransferSessionId::new(7), 13, 13),
            Ok(true)
        );
        assert_eq!(session.commit(TransferSessionId::new(7)), Ok(()));
    }

    #[test]
    fn gaps_stale_sessions_and_early_commit_are_rejected() {
        let mut session = BleStorageSession::new();
        let route = BleUploadRoute::new("book.binbook", "reader", "ble.complete", 26).unwrap();
        assert_eq!(session.begin(TransferSessionId::new(4), route), Ok(()));
        assert_eq!(
            session.accept_chunk(TransferSessionId::new(4), 13, 13),
            Err(BlePipelineError::InvalidOffset)
        );
        assert_eq!(
            session.accept_chunk(TransferSessionId::new(5), 0, 13),
            Err(BlePipelineError::StaleSession)
        );
        assert_eq!(
            session.commit(TransferSessionId::new(4)),
            Err(BlePipelineError::Incomplete)
        );
    }

    #[test]
    fn cancellation_invalidates_queued_work_and_allows_reuse() {
        let mut session = BleStorageSession::new();
        let route = BleUploadRoute::new("book.binbook", "reader", "ble.complete", 26).unwrap();
        session.begin(TransferSessionId::new(1), route).unwrap();
        session
            .accept_chunk(TransferSessionId::new(1), 0, 13)
            .unwrap();
        assert!(session.cancel(TransferSessionId::new(1)));
        assert_eq!(
            session.accept_chunk(TransferSessionId::new(1), 13, 13),
            Err(BlePipelineError::Idle)
        );
        let replacement =
            BleUploadRoute::new("book.binbook", "reader", "ble.complete", 26).unwrap();
        assert_eq!(
            session.begin(TransferSessionId::new(2), replacement),
            Ok(())
        );
    }

    #[test]
    fn gatt_table_budget_includes_gap_gatt_and_transfer_services() {
        assert_eq!(required_gatt_attribute_count(), 17);
        assert!(BLE_GATT_ATTRIBUTE_CAPACITY >= required_gatt_attribute_count());
    }

    #[test]
    fn reconnect_does_not_repeat_completed_stage_diagnostics() {
        assert!(should_report_ble_stage(3, 4));
        assert!(!should_report_ble_stage(5, 4));
        assert!(!should_report_ble_stage(5, 5));
    }

    #[test]
    fn connection_watchdog_defaults_and_accepts_target_override() {
        assert_eq!(ble_connection_watchdog_ms(None), 30_000);
        assert_eq!(ble_connection_watchdog_ms(Some("45000")), 45_000);
        assert_eq!(ble_connection_watchdog_ms(Some("invalid")), 30_000);
    }
}
