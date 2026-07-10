use squidvm_core::limits::MAX_APP_ID_BYTES;

use crate::lifecycle::{MAX_ARMED_INPUTS, MAX_ARMED_TIMERS, MAX_RETURN_STACK};

const MAGIC: [u8; 4] = *b"SQPW";
pub const MAX_ARMED_APP_IDS: usize = MAX_ARMED_INPUTS + MAX_ARMED_TIMERS;
pub const POWER_CHECKPOINT_BYTES: usize = 640;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativePowerRequest {
    pub wake_after_ms: u32,
}

pub trait NativePowerBackend {
    fn request_sleep(&mut self, request: NativePowerRequest) -> Result<(), ()>;
    fn take_requested_sleep(&mut self) -> Option<NativePowerRequest>;
    fn prepare_sleep(&mut self, request: NativePowerRequest);
    fn take_prepared_sleep(&mut self) -> Option<NativePowerRequest>;
    fn abort_sleep(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PowerRequestState {
    Idle,
    Requested(NativePowerRequest),
    Prepared(NativePowerRequest),
}

pub struct DeferredNativePowerBackend {
    state: PowerRequestState,
}

impl DeferredNativePowerBackend {
    pub const fn new() -> Self {
        Self {
            state: PowerRequestState::Idle,
        }
    }
}

impl Default for DeferredNativePowerBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl NativePowerBackend for DeferredNativePowerBackend {
    fn request_sleep(&mut self, request: NativePowerRequest) -> Result<(), ()> {
        if self.state != PowerRequestState::Idle {
            return Err(());
        }
        self.state = PowerRequestState::Requested(request);
        Ok(())
    }

    fn take_requested_sleep(&mut self) -> Option<NativePowerRequest> {
        let PowerRequestState::Requested(request) = self.state else {
            return None;
        };
        self.state = PowerRequestState::Idle;
        Some(request)
    }

    fn prepare_sleep(&mut self, request: NativePowerRequest) {
        self.state = PowerRequestState::Prepared(request);
    }

    fn take_prepared_sleep(&mut self) -> Option<NativePowerRequest> {
        let PowerRequestState::Prepared(request) = self.state else {
            return None;
        };
        self.state = PowerRequestState::Idle;
        Some(request)
    }

    fn abort_sleep(&mut self) {
        self.state = PowerRequestState::Idle;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerCheckpointError {
    InvalidAppId,
    TooManyReturnApps,
    TooManyArmedApps,
    BufferTooSmall,
    Corrupt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointAppId {
    bytes: [u8; MAX_APP_ID_BYTES],
    len: u8,
}

impl CheckpointAppId {
    pub const fn empty() -> Self {
        Self {
            bytes: [0; MAX_APP_ID_BYTES],
            len: 0,
        }
    }

    pub fn new(value: &str) -> Result<Self, PowerCheckpointError> {
        if value.is_empty() || value.len() > MAX_APP_ID_BYTES || !value.is_ascii() {
            return Err(PowerCheckpointError::InvalidAppId);
        }
        let mut result = Self::empty();
        result.bytes[..value.len()].copy_from_slice(value.as_bytes());
        result.len = value.len() as u8;
        Ok(result)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PowerCheckpoint {
    pub active_app: CheckpointAppId,
    pub return_apps: [CheckpointAppId; MAX_RETURN_STACK],
    pub return_len: usize,
    pub armed_apps: [CheckpointAppId; MAX_ARMED_APP_IDS],
    pub armed_len: usize,
    pub wake_after_ms: u32,
}

impl PowerCheckpoint {
    pub fn new(active_app: &str, wake_after_ms: u32) -> Result<Self, PowerCheckpointError> {
        Ok(Self {
            active_app: CheckpointAppId::new(active_app)?,
            return_apps: [CheckpointAppId::empty(); MAX_RETURN_STACK],
            return_len: 0,
            armed_apps: [CheckpointAppId::empty(); MAX_ARMED_APP_IDS],
            armed_len: 0,
            wake_after_ms,
        })
    }

    pub fn push_return_app(&mut self, app_id: &str) -> Result<(), PowerCheckpointError> {
        let slot = self
            .return_apps
            .get_mut(self.return_len)
            .ok_or(PowerCheckpointError::TooManyReturnApps)?;
        *slot = CheckpointAppId::new(app_id)?;
        self.return_len += 1;
        Ok(())
    }

    pub fn push_armed_app(&mut self, app_id: &str) -> Result<(), PowerCheckpointError> {
        if self.armed_apps[..self.armed_len]
            .iter()
            .any(|existing| existing.as_str() == app_id)
        {
            return Ok(());
        }
        let slot = self
            .armed_apps
            .get_mut(self.armed_len)
            .ok_or(PowerCheckpointError::TooManyArmedApps)?;
        *slot = CheckpointAppId::new(app_id)?;
        self.armed_len += 1;
        Ok(())
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize, PowerCheckpointError> {
        if out.len() < POWER_CHECKPOINT_BYTES {
            return Err(PowerCheckpointError::BufferTooSmall);
        }
        out[..POWER_CHECKPOINT_BYTES].fill(0);
        out[..4].copy_from_slice(&MAGIC);
        let mut cursor = 6;
        write_app_id(out, &mut cursor, &self.active_app)?;
        write_count(out, &mut cursor, self.return_len)?;
        for app_id in &self.return_apps[..self.return_len] {
            write_app_id(out, &mut cursor, app_id)?;
        }
        write_count(out, &mut cursor, self.armed_len)?;
        for app_id in &self.armed_apps[..self.armed_len] {
            write_app_id(out, &mut cursor, app_id)?;
        }
        write_bytes(out, &mut cursor, &self.wake_after_ms.to_le_bytes())?;
        let payload_len = u16::try_from(cursor - 6).map_err(|_| PowerCheckpointError::Corrupt)?;
        out[4..6].copy_from_slice(&payload_len.to_le_bytes());
        let crc = crc32fast::hash(&out[..cursor]);
        write_bytes(out, &mut cursor, &crc.to_le_bytes())?;
        Ok(cursor)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PowerCheckpointError> {
        if bytes.len() < 10 || bytes[..4] != MAGIC {
            return Err(PowerCheckpointError::Corrupt);
        }
        let payload_len = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
        let crc_offset = 6usize
            .checked_add(payload_len)
            .ok_or(PowerCheckpointError::Corrupt)?;
        let end = crc_offset
            .checked_add(4)
            .ok_or(PowerCheckpointError::Corrupt)?;
        if end > bytes.len() {
            return Err(PowerCheckpointError::Corrupt);
        }
        let expected_crc = u32::from_le_bytes(
            bytes[crc_offset..end]
                .try_into()
                .map_err(|_| PowerCheckpointError::Corrupt)?,
        );
        if crc32fast::hash(&bytes[..crc_offset]) != expected_crc {
            return Err(PowerCheckpointError::Corrupt);
        }

        let mut cursor = 6;
        let active_app = read_app_id(bytes, &mut cursor, crc_offset)?;
        let return_len = read_count(bytes, &mut cursor, crc_offset)?;
        if return_len > MAX_RETURN_STACK {
            return Err(PowerCheckpointError::Corrupt);
        }
        let mut return_apps = [CheckpointAppId::empty(); MAX_RETURN_STACK];
        for slot in &mut return_apps[..return_len] {
            *slot = read_app_id(bytes, &mut cursor, crc_offset)?;
        }
        let armed_len = read_count(bytes, &mut cursor, crc_offset)?;
        if armed_len > MAX_ARMED_APP_IDS {
            return Err(PowerCheckpointError::Corrupt);
        }
        let mut armed_apps = [CheckpointAppId::empty(); MAX_ARMED_APP_IDS];
        for slot in &mut armed_apps[..armed_len] {
            *slot = read_app_id(bytes, &mut cursor, crc_offset)?;
        }
        let wake_after_ms = u32::from_le_bytes(
            read_bytes(bytes, &mut cursor, crc_offset, 4)?
                .try_into()
                .map_err(|_| PowerCheckpointError::Corrupt)?,
        );
        if cursor != crc_offset {
            return Err(PowerCheckpointError::Corrupt);
        }
        Ok(Self {
            active_app,
            return_apps,
            return_len,
            armed_apps,
            armed_len,
            wake_after_ms,
        })
    }
}

fn write_app_id(
    out: &mut [u8],
    cursor: &mut usize,
    app_id: &CheckpointAppId,
) -> Result<(), PowerCheckpointError> {
    write_bytes(out, cursor, &[app_id.len])?;
    write_bytes(out, cursor, app_id.as_str().as_bytes())
}

fn write_count(
    out: &mut [u8],
    cursor: &mut usize,
    count: usize,
) -> Result<(), PowerCheckpointError> {
    let count = u8::try_from(count).map_err(|_| PowerCheckpointError::Corrupt)?;
    write_bytes(out, cursor, &[count])
}

fn write_bytes(
    out: &mut [u8],
    cursor: &mut usize,
    bytes: &[u8],
) -> Result<(), PowerCheckpointError> {
    let end = cursor
        .checked_add(bytes.len())
        .ok_or(PowerCheckpointError::BufferTooSmall)?;
    out.get_mut(*cursor..end)
        .ok_or(PowerCheckpointError::BufferTooSmall)?
        .copy_from_slice(bytes);
    *cursor = end;
    Ok(())
}

fn read_app_id(
    bytes: &[u8],
    cursor: &mut usize,
    end: usize,
) -> Result<CheckpointAppId, PowerCheckpointError> {
    let len = usize::from(
        *read_bytes(bytes, cursor, end, 1)?
            .first()
            .ok_or(PowerCheckpointError::Corrupt)?,
    );
    let value = read_bytes(bytes, cursor, end, len)?;
    let value = core::str::from_utf8(value).map_err(|_| PowerCheckpointError::Corrupt)?;
    CheckpointAppId::new(value).map_err(|_| PowerCheckpointError::Corrupt)
}

fn read_count(bytes: &[u8], cursor: &mut usize, end: usize) -> Result<usize, PowerCheckpointError> {
    Ok(usize::from(
        *read_bytes(bytes, cursor, end, 1)?
            .first()
            .ok_or(PowerCheckpointError::Corrupt)?,
    ))
}

fn read_bytes<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    end: usize,
    len: usize,
) -> Result<&'a [u8], PowerCheckpointError> {
    let next = cursor
        .checked_add(len)
        .filter(|next| *next <= end)
        .ok_or(PowerCheckpointError::Corrupt)?;
    let value = bytes
        .get(*cursor..next)
        .ok_or(PowerCheckpointError::Corrupt)?;
    *cursor = next;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_round_trips_full_lifecycle_routing() {
        let mut checkpoint = PowerCheckpoint::new("reader", 3_000).unwrap();
        checkpoint.push_return_app("main").unwrap();
        checkpoint.push_return_app("library").unwrap();
        checkpoint.push_armed_app("clock").unwrap();
        checkpoint.push_armed_app("sleep-helper").unwrap();
        checkpoint.push_armed_app("clock").unwrap();
        let mut bytes = [0_u8; POWER_CHECKPOINT_BYTES];
        let len = checkpoint.encode(&mut bytes).unwrap();

        assert_eq!(PowerCheckpoint::decode(&bytes[..len]), Ok(checkpoint));
        assert_eq!(checkpoint.armed_len, 2);
    }

    #[test]
    fn checkpoint_rejects_crc_damage_and_truncation() {
        let checkpoint = PowerCheckpoint::new("reader", 0).unwrap();
        let mut bytes = [0_u8; POWER_CHECKPOINT_BYTES];
        let len = checkpoint.encode(&mut bytes).unwrap();
        bytes[7] ^= 0x40;
        assert_eq!(
            PowerCheckpoint::decode(&bytes[..len]),
            Err(PowerCheckpointError::Corrupt)
        );
        assert_eq!(
            PowerCheckpoint::decode(&bytes[..len - 1]),
            Err(PowerCheckpointError::Corrupt)
        );
    }

    #[test]
    fn checkpoint_enforces_app_id_and_collection_bounds() {
        assert_eq!(
            PowerCheckpoint::new("", 0),
            Err(PowerCheckpointError::InvalidAppId)
        );
        let mut checkpoint = PowerCheckpoint::new("reader", 0).unwrap();
        for index in 0..MAX_ARMED_APP_IDS {
            checkpoint
                .push_armed_app(&format!("armed-{index}"))
                .unwrap();
        }
        assert_eq!(checkpoint.armed_len, MAX_ARMED_APP_IDS);
        assert_eq!(
            checkpoint.push_armed_app("one-too-many"),
            Err(PowerCheckpointError::TooManyArmedApps)
        );
    }
}
