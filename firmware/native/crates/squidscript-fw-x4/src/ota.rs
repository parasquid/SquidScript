use heapless::String;
use sha2::{Digest, Sha256};
use squid_device_protocol::FIRMWARE_SHA256_BYTES;

pub const OTA_SLOT_BYTES: usize = 0x280000;
pub const OTA_BUILD_ID_BYTES: usize = 32;
const CHECKPOINT_MAGIC: [u8; 4] = *b"SQOT";
pub const OTA_CHECKPOINT_BYTES: usize =
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
    Flash,
    Verify,
}

pub trait OtaSlotStorage {
    fn erase(&mut self, slot: Slot, from: usize, to: usize) -> Result<(), OtaError>;
    fn write(&mut self, slot: Slot, offset: usize, bytes: &[u8]) -> Result<(), OtaError>;
    fn read(&mut self, slot: Slot, offset: usize, out: &mut [u8]) -> Result<(), OtaError>;
    fn activate(&mut self, slot: Slot) -> Result<(), OtaError>;
}

#[cfg(target_arch = "riscv32")]
pub struct EspOtaSlotStorage<'a, F> {
    flash: &'a mut F,
    partition_table: &'a mut [u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN],
}

#[cfg(target_arch = "riscv32")]
impl<'a, F> EspOtaSlotStorage<'a, F>
where
    F: embedded_storage::Storage + embedded_storage::nor_flash::NorFlash,
{
    pub fn new(
        flash: &'a mut F,
        partition_table: &'a mut [u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN],
    ) -> Self {
        Self {
            flash,
            partition_table,
        }
    }

    pub fn active_slot(&mut self) -> Result<Slot, OtaError> {
        use esp_bootloader_esp_idf::partitions::{read_partition_table, PartitionType};
        let table =
            read_partition_table(self.flash, self.partition_table).map_err(|_| OtaError::Flash)?;
        let booted = table
            .booted_partition()
            .map_err(|_| OtaError::Flash)?
            .ok_or(OtaError::Flash)?;
        match booted.partition_type() {
            PartitionType::App(subtype) => slot_from_subtype(subtype),
            _ => Err(OtaError::Flash),
        }
    }

    pub fn inactive_slot(&mut self) -> Result<Slot, OtaError> {
        match self.active_slot()? {
            Slot::App0 => Ok(Slot::App1),
            Slot::App1 => Ok(Slot::App0),
        }
    }

    pub fn boot_state(&mut self) -> Result<&'static str, OtaError> {
        use esp_bootloader_esp_idf::ota::OtaImageState;
        let mut ota = self.ota_data()?;
        Ok(
            match ota.current_ota_state().map_err(|_| OtaError::Flash)? {
                OtaImageState::New => "new",
                OtaImageState::PendingVerify => "pending-verify",
                OtaImageState::Valid => "valid",
                OtaImageState::Invalid => "invalid",
                OtaImageState::Aborted => "aborted",
                OtaImageState::Undefined => "undefined",
            },
        )
    }

    pub fn mark_running_valid(&mut self) -> Result<(), OtaError> {
        use esp_bootloader_esp_idf::ota::OtaImageState;
        let mut ota = self.ota_data()?;
        if ota.current_ota_state().map_err(|_| OtaError::Flash)? == OtaImageState::PendingVerify {
            ota.set_current_ota_state(OtaImageState::Valid)
                .map_err(|_| OtaError::Flash)?;
        }
        Ok(())
    }

    fn ota_data(&mut self) -> Result<esp_bootloader_esp_idf::ota::Ota<'_, F>, OtaError> {
        use esp_bootloader_esp_idf::partitions::{
            read_partition_table, DataPartitionSubType, PartitionType,
        };
        let table =
            read_partition_table(self.flash, self.partition_table).map_err(|_| OtaError::Flash)?;
        let entry = table
            .find_partition(PartitionType::Data(DataPartitionSubType::Ota))
            .map_err(|_| OtaError::Flash)?
            .ok_or(OtaError::Flash)?;
        esp_bootloader_esp_idf::ota::Ota::new(entry.as_embedded_storage(self.flash), 2)
            .map_err(|_| OtaError::Flash)
    }

    pub fn inactive_geometry(&mut self, expected: Slot) -> Result<(u32, usize), OtaError> {
        use esp_bootloader_esp_idf::partitions::{read_partition_table, PartitionType};

        if self.active_slot()? == expected {
            return Err(OtaError::ActiveSlot);
        }
        let subtype = match expected {
            Slot::App0 => esp_bootloader_esp_idf::partitions::AppPartitionSubType::Ota0,
            Slot::App1 => esp_bootloader_esp_idf::partitions::AppPartitionSubType::Ota1,
        };
        let table =
            read_partition_table(self.flash, self.partition_table).map_err(|_| OtaError::Flash)?;
        let entry = table
            .find_partition(PartitionType::App(subtype))
            .map_err(|_| OtaError::Flash)?
            .ok_or(OtaError::Flash)?;
        if entry.len() as usize != OTA_SLOT_BYTES {
            return Err(OtaError::Bounds);
        }
        Ok((entry.offset(), entry.len() as usize))
    }
}

#[cfg(target_arch = "riscv32")]
pub struct CachedEspOtaSlotStorage<'a, F> {
    flash: &'a mut F,
    partition_table: &'a mut [u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN],
    slot: Slot,
    base: u32,
    size: usize,
}

#[cfg(target_arch = "riscv32")]
impl<'a, F> CachedEspOtaSlotStorage<'a, F> {
    pub fn new(
        flash: &'a mut F,
        partition_table: &'a mut [u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN],
        slot: Slot,
        base: u32,
        size: usize,
    ) -> Self {
        Self {
            flash,
            partition_table,
            slot,
            base,
            size,
        }
    }

    fn address(&self, slot: Slot, offset: usize, len: usize) -> Result<u32, OtaError> {
        if slot != self.slot || len == 0 || offset.saturating_add(len) > self.size {
            return Err(OtaError::Bounds);
        }
        self.base.checked_add(offset as u32).ok_or(OtaError::Bounds)
    }
}

#[cfg(target_arch = "riscv32")]
impl<F> OtaSlotStorage for CachedEspOtaSlotStorage<'_, F>
where
    F: embedded_storage::Storage + embedded_storage::nor_flash::NorFlash,
{
    fn erase(&mut self, slot: Slot, from: usize, to: usize) -> Result<(), OtaError> {
        if from >= to {
            return Err(OtaError::Bounds);
        }
        let from = self.address(slot, from, to - from)?;
        let to = self.base.checked_add(to as u32).ok_or(OtaError::Bounds)?;
        embedded_storage::nor_flash::NorFlash::erase(self.flash, from, to)
            .map_err(|_| OtaError::Flash)
    }

    fn write(&mut self, slot: Slot, offset: usize, bytes: &[u8]) -> Result<(), OtaError> {
        let address = self.address(slot, offset, bytes.len())?;
        embedded_storage::nor_flash::NorFlash::write(self.flash, address, bytes)
            .map_err(|_| OtaError::Flash)
    }

    fn read(&mut self, slot: Slot, offset: usize, out: &mut [u8]) -> Result<(), OtaError> {
        let address = self.address(slot, offset, out.len())?;
        embedded_storage::nor_flash::ReadNorFlash::read(self.flash, address, out)
            .map_err(|_| OtaError::Flash)
    }

    fn activate(&mut self, slot: Slot) -> Result<(), OtaError> {
        EspOtaSlotStorage::new(self.flash, self.partition_table).activate(slot)
    }
}

#[cfg(target_arch = "riscv32")]
impl<F> OtaSlotStorage for EspOtaSlotStorage<'_, F>
where
    F: embedded_storage::Storage + embedded_storage::nor_flash::NorFlash,
{
    fn erase(&mut self, slot: Slot, from: usize, to: usize) -> Result<(), OtaError> {
        let (base, size) = self.inactive_geometry(slot)?;
        if from >= to || to > size {
            return Err(OtaError::Bounds);
        }
        let from = base.checked_add(from as u32).ok_or(OtaError::Bounds)?;
        let to = base.checked_add(to as u32).ok_or(OtaError::Bounds)?;
        embedded_storage::nor_flash::NorFlash::erase(self.flash, from, to)
            .map_err(|_| OtaError::Flash)
    }

    fn write(&mut self, slot: Slot, offset: usize, bytes: &[u8]) -> Result<(), OtaError> {
        let (base, size) = self.inactive_geometry(slot)?;
        if bytes.is_empty() || offset.saturating_add(bytes.len()) > size {
            return Err(OtaError::Bounds);
        }
        let offset = base.checked_add(offset as u32).ok_or(OtaError::Bounds)?;
        embedded_storage::nor_flash::NorFlash::write(self.flash, offset, bytes)
            .map_err(|_| OtaError::Flash)
    }

    fn read(&mut self, slot: Slot, offset: usize, out: &mut [u8]) -> Result<(), OtaError> {
        let (base, size) = self.inactive_geometry(slot)?;
        if out.is_empty() || offset.saturating_add(out.len()) > size {
            return Err(OtaError::Bounds);
        }
        let offset = base.checked_add(offset as u32).ok_or(OtaError::Bounds)?;
        embedded_storage::nor_flash::ReadNorFlash::read(self.flash, offset, out)
            .map_err(|_| OtaError::Flash)
    }

    fn activate(&mut self, slot: Slot) -> Result<(), OtaError> {
        use esp_bootloader_esp_idf::partitions::{
            read_partition_table, AppPartitionSubType, DataPartitionSubType, PartitionType,
        };
        if self.active_slot()? == slot {
            return Err(OtaError::ActiveSlot);
        }
        let subtype = match slot {
            Slot::App0 => AppPartitionSubType::Ota0,
            Slot::App1 => AppPartitionSubType::Ota1,
        };
        let table =
            read_partition_table(self.flash, self.partition_table).map_err(|_| OtaError::Flash)?;
        let ota_data = table
            .find_partition(PartitionType::Data(DataPartitionSubType::Ota))
            .map_err(|_| OtaError::Flash)?
            .ok_or(OtaError::Flash)?;
        let region = ota_data.as_embedded_storage(self.flash);
        let mut ota =
            esp_bootloader_esp_idf::ota::Ota::new(region, 2).map_err(|_| OtaError::Flash)?;
        ota.set_current_app_partition(subtype)
            .map_err(|_| OtaError::Flash)?;
        ota.set_current_ota_state(esp_bootloader_esp_idf::ota::OtaImageState::New)
            .map_err(|_| OtaError::Flash)
    }
}

#[cfg(target_arch = "riscv32")]
fn slot_from_subtype(
    subtype: esp_bootloader_esp_idf::partitions::AppPartitionSubType,
) -> Result<Slot, OtaError> {
    use esp_bootloader_esp_idf::partitions::AppPartitionSubType;
    match subtype {
        AppPartitionSubType::Ota0 => Ok(Slot::App0),
        AppPartitionSubType::Ota1 => Ok(Slot::App1),
        _ => Err(OtaError::Bounds),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CooperativeStatus {
    Idle,
    Erasing { erased: usize, total: usize },
    Receiving { durable: usize, total: usize },
    Verifying { verified: usize, total: usize },
    ReadyToActivate,
    Committed,
    Aborted,
    Failed,
}

pub struct OtaController {
    state: TransferState,
    erase_offset: usize,
    verify_offset: usize,
    verifier: Sha256,
    verified: bool,
}

impl Default for OtaController {
    fn default() -> Self {
        Self {
            state: TransferState::default(),
            erase_offset: OTA_SLOT_BYTES,
            verify_offset: 0,
            verifier: Sha256::new(),
            verified: false,
        }
    }
}

impl OtaController {
    pub fn restore(state: TransferState) -> Self {
        Self {
            state,
            erase_offset: OTA_SLOT_BYTES,
            verify_offset: 0,
            verifier: Sha256::new(),
            verified: false,
        }
    }

    pub fn begin(
        &mut self,
        active: Slot,
        candidate: Slot,
        expected_len: usize,
        expected_sha256: &[u8],
        build_id: &str,
    ) -> Result<(), OtaError> {
        self.state
            .begin(active, candidate, expected_len, expected_sha256, build_id)?;
        self.erase_offset = 0;
        self.verify_offset = 0;
        self.verifier = Sha256::new();
        self.verified = false;
        Ok(())
    }

    pub fn status(&self) -> CooperativeStatus {
        match self.state.phase() {
            TransferPhase::Idle => CooperativeStatus::Idle,
            TransferPhase::Receiving if self.erase_offset < OTA_SLOT_BYTES => {
                CooperativeStatus::Erasing {
                    erased: self.erase_offset,
                    total: OTA_SLOT_BYTES,
                }
            }
            TransferPhase::Receiving => CooperativeStatus::Receiving {
                durable: self.state.durable_offset(),
                total: self.state.expected_len(),
            },
            TransferPhase::Ready if self.verified => CooperativeStatus::ReadyToActivate,
            TransferPhase::Ready if self.verify_offset > 0 => CooperativeStatus::Verifying {
                verified: self.verify_offset,
                total: self.state.expected_len(),
            },
            TransferPhase::Ready => CooperativeStatus::Receiving {
                durable: self.state.durable_offset(),
                total: self.state.expected_len(),
            },
            TransferPhase::Committed => CooperativeStatus::Committed,
            TransferPhase::Aborted => CooperativeStatus::Aborted,
            TransferPhase::Failed => CooperativeStatus::Failed,
        }
    }

    pub fn erase_step<S: OtaSlotStorage>(
        &mut self,
        storage: &mut S,
        sector_bytes: usize,
    ) -> Result<CooperativeStatus, OtaError> {
        if self.state.phase() != TransferPhase::Receiving
            || self.erase_offset >= OTA_SLOT_BYTES
            || sector_bytes == 0
            || OTA_SLOT_BYTES % sector_bytes != 0
        {
            return Err(OtaError::Inactive);
        }
        let end = self
            .erase_offset
            .saturating_add(sector_bytes)
            .min(OTA_SLOT_BYTES);
        storage.erase(self.state.candidate(), self.erase_offset, end)?;
        self.erase_offset = end;
        Ok(self.status())
    }

    pub fn write_chunk<S: OtaSlotStorage>(
        &mut self,
        storage: &mut S,
        offset: usize,
        bytes: &[u8],
    ) -> Result<ChunkDisposition, OtaError> {
        if self.erase_offset != OTA_SLOT_BYTES {
            return Err(OtaError::Inactive);
        }
        let disposition = self.state.classify_chunk(offset, bytes.len())?;
        if disposition == ChunkDisposition::Write {
            storage.write(self.state.candidate(), offset, bytes)?;
            self.state.mark_chunk_durable(offset, bytes.len())?;
        }
        Ok(disposition)
    }

    pub fn verify_step<S: OtaSlotStorage>(
        &mut self,
        storage: &mut S,
        buffer: &mut [u8],
    ) -> Result<CooperativeStatus, OtaError> {
        if self.state.phase() != TransferPhase::Ready || self.verified || buffer.is_empty() {
            return Err(OtaError::Inactive);
        }
        let read_len = buffer
            .len()
            .min(self.state.expected_len().saturating_sub(self.verify_offset));
        storage.read(
            self.state.candidate(),
            self.verify_offset,
            &mut buffer[..read_len],
        )?;
        self.verifier.update(&buffer[..read_len]);
        self.verify_offset += read_len;
        if self.verify_offset == self.state.expected_len() {
            let digest: [u8; 32] = self.verifier.clone().finalize().into();
            if &digest != self.state.expected_sha256() {
                self.state.fail();
                return Err(OtaError::Verify);
            }
            self.verified = true;
        }
        Ok(self.status())
    }

    pub fn activate<S: OtaSlotStorage>(&mut self, storage: &mut S) -> Result<(), OtaError> {
        if !self.verified || self.state.phase() != TransferPhase::Ready {
            return Err(OtaError::Incomplete);
        }
        storage.activate(self.state.candidate())?;
        self.state.mark_committed()
    }

    pub fn abort(&mut self) {
        self.state.abort();
        self.erase_offset = OTA_SLOT_BYTES;
        self.verify_offset = 0;
        self.verified = false;
    }

    pub const fn transfer_state(&self) -> &TransferState {
        &self.state
    }
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
            .get_mut(..OTA_CHECKPOINT_BYTES)
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
        let crc = crc32fast::hash(&out[..OTA_CHECKPOINT_BYTES - 4]);
        out[OTA_CHECKPOINT_BYTES - 4..].copy_from_slice(&crc.to_le_bytes());
        Ok(OTA_CHECKPOINT_BYTES)
    }

    pub fn decode_checkpoint(bytes: &[u8]) -> Result<Self, OtaError> {
        let bytes = bytes
            .get(..OTA_CHECKPOINT_BYTES)
            .ok_or(OtaError::Checkpoint)?;
        if bytes[..4] != CHECKPOINT_MAGIC {
            return Err(OtaError::Checkpoint);
        }
        let expected_crc =
            u32::from_le_bytes(bytes[OTA_CHECKPOINT_BYTES - 4..].try_into().unwrap());
        if crc32fast::hash(&bytes[..OTA_CHECKPOINT_BYTES - 4]) != expected_crc {
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

    struct MockSlot {
        bytes: std::vec::Vec<u8>,
        activated: Option<Slot>,
        erases: usize,
        writes: usize,
    }

    impl MockSlot {
        fn new() -> Self {
            Self {
                bytes: vec![0; OTA_SLOT_BYTES],
                activated: None,
                erases: 0,
                writes: 0,
            }
        }
    }

    impl OtaSlotStorage for MockSlot {
        fn erase(&mut self, _slot: Slot, from: usize, to: usize) -> Result<(), OtaError> {
            self.bytes[from..to].fill(0xff);
            self.erases += 1;
            Ok(())
        }

        fn write(&mut self, _slot: Slot, offset: usize, bytes: &[u8]) -> Result<(), OtaError> {
            self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
            self.writes += 1;
            Ok(())
        }

        fn read(&mut self, _slot: Slot, offset: usize, out: &mut [u8]) -> Result<(), OtaError> {
            out.copy_from_slice(&self.bytes[offset..offset + out.len()]);
            Ok(())
        }

        fn activate(&mut self, slot: Slot) -> Result<(), OtaError> {
            self.activated = Some(slot);
            Ok(())
        }
    }

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
        let mut bytes = [0u8; OTA_CHECKPOINT_BYTES];
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

    #[test]
    fn controller_erases_cooperatively_and_activates_only_after_readback_hash() {
        let image = b"firmware-image";
        let hash: [u8; 32] = Sha256::digest(image).into();
        let mut controller = OtaController::default();
        let mut storage = MockSlot::new();
        controller
            .begin(Slot::App0, Slot::App1, image.len(), &hash, "build")
            .unwrap();
        assert_eq!(
            controller.status(),
            CooperativeStatus::Erasing {
                erased: 0,
                total: OTA_SLOT_BYTES
            }
        );
        while matches!(controller.status(), CooperativeStatus::Erasing { .. }) {
            controller.erase_step(&mut storage, 4096).unwrap();
        }
        assert_eq!(storage.erases, OTA_SLOT_BYTES / 4096);
        assert_eq!(
            controller
                .write_chunk(&mut storage, 0, &image[..4])
                .unwrap(),
            ChunkDisposition::Write
        );
        assert_eq!(
            controller
                .write_chunk(&mut storage, 0, &image[..4])
                .unwrap(),
            ChunkDisposition::AlreadyDurable
        );
        controller
            .write_chunk(&mut storage, 4, &image[4..])
            .unwrap();
        assert_eq!(storage.writes, 2);
        let mut readback = [0u8; 4];
        while !matches!(controller.status(), CooperativeStatus::ReadyToActivate) {
            controller.verify_step(&mut storage, &mut readback).unwrap();
        }
        assert_eq!(storage.activated, None);
        controller.activate(&mut storage).unwrap();
        assert_eq!(storage.activated, Some(Slot::App1));
        assert_eq!(controller.status(), CooperativeStatus::Committed);
    }

    #[test]
    fn controller_hash_failure_never_changes_activation_metadata() {
        let image = b"firmware-image";
        let hash: [u8; 32] = Sha256::digest(image).into();
        let mut controller = OtaController::default();
        let mut storage = MockSlot::new();
        controller
            .begin(Slot::App1, Slot::App0, image.len(), &hash, "build")
            .unwrap();
        while matches!(controller.status(), CooperativeStatus::Erasing { .. }) {
            controller.erase_step(&mut storage, 4096).unwrap();
        }
        controller.write_chunk(&mut storage, 0, image).unwrap();
        storage.bytes[3] ^= 1;
        let mut readback = [0u8; 8];
        let error = loop {
            match controller.verify_step(&mut storage, &mut readback) {
                Ok(_) => continue,
                Err(error) => break error,
            }
        };
        assert_eq!(error, OtaError::Verify);
        assert_eq!(controller.status(), CooperativeStatus::Failed);
        assert_eq!(storage.activated, None);
        assert_eq!(controller.activate(&mut storage), Err(OtaError::Incomplete));
    }
}
