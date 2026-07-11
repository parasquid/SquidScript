use heapless::String;
use squid_device_protocol::FIRMWARE_SHA256_BYTES;

pub const OTA_SLOT_BYTES: usize = 0x280000;
pub const OTA_BUILD_ID_BYTES: usize = 32;
const CHECKPOINT_MAGIC: [u8; 4] = *b"SQOT";
const CHECKPOINT_BYTES: usize =
    4 + 1 + 1 + 8 + 8 + FIRMWARE_SHA256_BYTES + 1 + OTA_BUILD_ID_BYTES + 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Slot {
    App0,
    App1,
}

impl Slot {
    pub const fn name(self) -> &'static str {
        match self {
            Self::App0 => "app0",
            Self::App1 => "app1",
        }
    }

    const fn encoded(self) -> u8 {
        match self {
            Self::App0 => 0,
            Self::App1 => 1,
        }
    }

    fn decode(value: u8) -> Result<Self, OtaError> {
        match value {
            0 => Ok(Self::App0),
            1 => Ok(Self::App1),
            _ => Err(OtaError::Checkpoint),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferPhase {
    Idle,
    Receiving,
    Ready,
    Committed,
    Aborted,
    Failed,
}

impl TransferPhase {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Receiving => "receiving",
            Self::Ready => "ready",
            Self::Committed => "committed",
            Self::Aborted => "aborted",
            Self::Failed => "failed",
        }
    }

    const fn encoded(self) -> u8 {
        self as u8
    }

    fn decode(value: u8) -> Result<Self, OtaError> {
        match value {
            0 => Ok(Self::Idle),
            1 => Ok(Self::Receiving),
            2 => Ok(Self::Ready),
            3 => Ok(Self::Committed),
            4 => Ok(Self::Aborted),
            5 => Ok(Self::Failed),
            _ => Err(OtaError::Checkpoint),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkDisposition {
    Write,
    AlreadyDurable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OtaError {
    ActiveSlot,
    Length,
    Hash,
    BuildId,
    Inactive,
    Offset,
    Bounds,
    Incomplete,
    Checkpoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferState {
    phase: TransferPhase,
    candidate: Slot,
    expected_len: usize,
    durable_offset: usize,
    expected_sha256: [u8; FIRMWARE_SHA256_BYTES],
    build_id: String<OTA_BUILD_ID_BYTES>,
}

impl Default for TransferState {
    fn default() -> Self {
        Self {
            phase: TransferPhase::Idle,
            candidate: Slot::App1,
            expected_len: 0,
            durable_offset: 0,
            expected_sha256: [0; FIRMWARE_SHA256_BYTES],
            build_id: String::new(),
        }
    }
}

impl TransferState {
    pub fn begin(
        &mut self,
        active: Slot,
        candidate: Slot,
        expected_len: usize,
        expected_sha256: &[u8],
        build_id: &str,
    ) -> Result<(), OtaError> {
        if active == candidate {
            return Err(OtaError::ActiveSlot);
        }
        if expected_len == 0 || expected_len > OTA_SLOT_BYTES {
            return Err(OtaError::Length);
        }
        let expected_sha256: [u8; FIRMWARE_SHA256_BYTES] =
            expected_sha256.try_into().map_err(|_| OtaError::Hash)?;
        if build_id.is_empty() || !build_id.is_ascii() || build_id.len() > OTA_BUILD_ID_BYTES {
            return Err(OtaError::BuildId);
        }
        let mut fixed_build_id = String::new();
        fixed_build_id
            .push_str(build_id)
            .map_err(|_| OtaError::BuildId)?;
        *self = Self {
            phase: TransferPhase::Receiving,
            candidate,
            expected_len,
            durable_offset: 0,
            expected_sha256,
            build_id: fixed_build_id,
        };
        Ok(())
    }

    pub fn classify_chunk(&self, offset: usize, len: usize) -> Result<ChunkDisposition, OtaError> {
        if self.phase != TransferPhase::Receiving {
            return Err(OtaError::Inactive);
        }
        let end = offset.checked_add(len).ok_or(OtaError::Bounds)?;
        if len == 0 || end > self.expected_len {
            return Err(OtaError::Bounds);
        }
        if offset == self.durable_offset {
            return Ok(ChunkDisposition::Write);
        }
        if offset < self.durable_offset && end <= self.durable_offset {
            return Ok(ChunkDisposition::AlreadyDurable);
        }
        Err(OtaError::Offset)
    }

    pub fn mark_chunk_durable(&mut self, offset: usize, len: usize) -> Result<(), OtaError> {
        if self.classify_chunk(offset, len)? != ChunkDisposition::Write {
            return Ok(());
        }
        self.durable_offset = offset + len;
        if self.durable_offset == self.expected_len {
            self.phase = TransferPhase::Ready;
        }
        Ok(())
    }

    pub fn mark_committed(&mut self) -> Result<(), OtaError> {
        if self.phase != TransferPhase::Ready || self.durable_offset != self.expected_len {
            return Err(OtaError::Incomplete);
        }
        self.phase = TransferPhase::Committed;
        Ok(())
    }

    pub fn abort(&mut self) {
        self.phase = TransferPhase::Aborted;
        self.expected_len = 0;
        self.durable_offset = 0;
        self.expected_sha256 = [0; FIRMWARE_SHA256_BYTES];
        self.build_id.clear();
    }

    pub fn fail(&mut self) {
        self.phase = TransferPhase::Failed;
    }

    pub const fn phase(&self) -> TransferPhase {
        self.phase
    }

    pub const fn candidate(&self) -> Slot {
        self.candidate
    }

    pub const fn expected_len(&self) -> usize {
        self.expected_len
    }

    pub const fn durable_offset(&self) -> usize {
        self.durable_offset
    }

    pub const fn expected_sha256(&self) -> &[u8; FIRMWARE_SHA256_BYTES] {
        &self.expected_sha256
    }

    pub fn build_id(&self) -> &str {
        self.build_id.as_str()
    }

    pub fn encode_checkpoint(&self, out: &mut [u8]) -> Result<usize, OtaError> {
        let out = out
            .get_mut(..CHECKPOINT_BYTES)
            .ok_or(OtaError::Checkpoint)?;
        out.fill(0);
        out[..4].copy_from_slice(&CHECKPOINT_MAGIC);
        out[4] = self.phase.encoded();
        out[5] = self.candidate.encoded();
        out[6..14].copy_from_slice(&(self.expected_len as u64).to_le_bytes());
        out[14..22].copy_from_slice(&(self.durable_offset as u64).to_le_bytes());
        out[22..54].copy_from_slice(&self.expected_sha256);
        out[54] = self.build_id.len() as u8;
        out[55..55 + self.build_id.len()].copy_from_slice(self.build_id.as_bytes());
        let crc = crc32fast::hash(&out[..CHECKPOINT_BYTES - 4]);
        out[CHECKPOINT_BYTES - 4..].copy_from_slice(&crc.to_le_bytes());
        Ok(CHECKPOINT_BYTES)
    }

    pub fn decode_checkpoint(bytes: &[u8]) -> Result<Self, OtaError> {
        let bytes = bytes.get(..CHECKPOINT_BYTES).ok_or(OtaError::Checkpoint)?;
        if bytes[..4] != CHECKPOINT_MAGIC {
            return Err(OtaError::Checkpoint);
        }
        let expected_crc = u32::from_le_bytes(bytes[CHECKPOINT_BYTES - 4..].try_into().unwrap());
        if crc32fast::hash(&bytes[..CHECKPOINT_BYTES - 4]) != expected_crc {
            return Err(OtaError::Checkpoint);
        }
        let build_len = bytes[54] as usize;
        if build_len == 0 || build_len > OTA_BUILD_ID_BYTES {
            return Err(OtaError::Checkpoint);
        }
        let build =
            core::str::from_utf8(&bytes[55..55 + build_len]).map_err(|_| OtaError::Checkpoint)?;
        let expected_len = usize::try_from(u64::from_le_bytes(bytes[6..14].try_into().unwrap()))
            .map_err(|_| OtaError::Checkpoint)?;
        let durable_offset = usize::try_from(u64::from_le_bytes(bytes[14..22].try_into().unwrap()))
            .map_err(|_| OtaError::Checkpoint)?;
        if expected_len > OTA_SLOT_BYTES || durable_offset > expected_len {
            return Err(OtaError::Checkpoint);
        }
        let mut build_id = String::new();
        build_id.push_str(build).map_err(|_| OtaError::Checkpoint)?;
        Ok(Self {
            phase: TransferPhase::decode(bytes[4])?,
            candidate: Slot::decode(bytes[5])?,
            expected_len,
            durable_offset,
            expected_sha256: bytes[22..54].try_into().unwrap(),
            build_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receiving() -> TransferState {
        let mut state = TransferState::default();
        state
            .begin(Slot::App0, Slot::App1, 12, &[7; 32], "0123456789abcdef")
            .unwrap();
        state
    }

    #[test]
    fn enforces_inactive_slot_identity_and_bounds() {
        let mut state = TransferState::default();
        assert_eq!(
            state.begin(Slot::App0, Slot::App0, 12, &[7; 32], "build"),
            Err(OtaError::ActiveSlot)
        );
        assert_eq!(
            state.begin(
                Slot::App0,
                Slot::App1,
                OTA_SLOT_BYTES + 1,
                &[7; 32],
                "build"
            ),
            Err(OtaError::Length)
        );
        assert_eq!(
            state.begin(Slot::App0, Slot::App1, 12, &[7; 31], "build"),
            Err(OtaError::Hash)
        );
    }

    #[test]
    fn orders_chunks_and_acknowledges_fully_durable_retries() {
        let mut state = receiving();
        assert_eq!(state.classify_chunk(0, 4), Ok(ChunkDisposition::Write));
        state.mark_chunk_durable(0, 4).unwrap();
        assert_eq!(state.durable_offset(), 4);
        assert_eq!(
            state.classify_chunk(0, 4),
            Ok(ChunkDisposition::AlreadyDurable)
        );
        assert_eq!(state.classify_chunk(2, 4), Err(OtaError::Offset));
        assert_eq!(state.classify_chunk(8, 4), Err(OtaError::Offset));
        state.mark_chunk_durable(4, 8).unwrap();
        assert_eq!(state.phase(), TransferPhase::Ready);
        state.mark_committed().unwrap();
        assert_eq!(state.phase(), TransferPhase::Committed);
    }

    #[test]
    fn checkpoint_round_trip_preserves_reconnect_status_and_rejects_corruption() {
        let mut state = receiving();
        state.mark_chunk_durable(0, 4).unwrap();
        let mut bytes = [0u8; CHECKPOINT_BYTES];
        let len = state.encode_checkpoint(&mut bytes).unwrap();
        assert_eq!(TransferState::decode_checkpoint(&bytes[..len]), Ok(state));
        bytes[20] ^= 1;
        assert_eq!(
            TransferState::decode_checkpoint(&bytes),
            Err(OtaError::Checkpoint)
        );
    }

    #[test]
    fn abort_is_terminal_and_clears_candidate_identity() {
        let mut state = receiving();
        state.abort();
        assert_eq!(state.phase(), TransferPhase::Aborted);
        assert_eq!(state.expected_len(), 0);
        assert_eq!(state.durable_offset(), 0);
        assert_eq!(state.build_id(), "");
        assert_eq!(state.classify_chunk(0, 1), Err(OtaError::Inactive));
    }
}
