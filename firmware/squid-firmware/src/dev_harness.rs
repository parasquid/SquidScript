//! Temporary development harness helpers for the ESP32-C3 Super Mini reference
//! firmware.
//!
//! This module models the bounded development app registry used by the
//! reference firmware. The registry is an in-memory cache over firmware-owned
//! app storage.

use crate::protocol::fnv1a;
use squidvm_core::{error::VmError, limits::MAX_APP_BYTES, program::Program};

pub const APP_REGISTRY_CAP: usize = 6;
pub const APP_ID_CAP: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppSlot(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppRegistryError {
    Full,
    InvalidAppId,
    TooLarge,
    InvalidSlot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppStorageError {
    Io,
    NotFound,
    NotMounted,
    NoSpace,
    InvalidName,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentAppError {
    Registry(AppRegistryError),
    Storage(AppStorageError),
    HashMismatch { expected: u32, actual: u32 },
    InvalidBytecode(VmError),
}

impl From<AppRegistryError> for PersistentAppError {
    fn from(value: AppRegistryError) -> Self {
        Self::Registry(value)
    }
}

impl From<AppStorageError> for PersistentAppError {
    fn from(value: AppStorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<VmError> for PersistentAppError {
    fn from(value: VmError) -> Self {
        Self::InvalidBytecode(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppName {
    bytes: [u8; APP_ID_CAP],
    len: usize,
}

impl AppName {
    pub fn new(value: &str) -> Result<Self, AppRegistryError> {
        validate_app_id(value)?;
        let mut bytes = [0u8; APP_ID_CAP];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Self {
            bytes,
            len: value.len(),
        })
    }

    pub const fn empty() -> Self {
        Self {
            bytes: [0; APP_ID_CAP],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppRegistryEntry {
    name: AppName,
    len: usize,
    hash: u32,
    occupied: bool,
}

impl AppRegistryEntry {
    pub const fn empty() -> Self {
        Self {
            name: AppName::empty(),
            len: 0,
            hash: 0,
            occupied: false,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn hash(&self) -> u32 {
        self.hash
    }

    pub const fn occupied(&self) -> bool {
        self.occupied
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredApp {
    pub name: AppName,
    pub len: usize,
    pub hash: u32,
}

impl StoredApp {
    pub const fn empty() -> Self {
        Self {
            name: AppName::empty(),
            len: 0,
            hash: 0,
        }
    }
}

pub trait AppStorage {
    fn ensure_ready(&mut self) -> Result<(), AppStorageError>;
    fn format(&mut self) -> Result<(), AppStorageError>;
    fn write_app(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStorageError>;
    fn read_app(&mut self, app_id: &str, out: &mut [u8]) -> Result<usize, AppStorageError>;
    fn read_app_range(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<usize, AppStorageError>;
    fn list_apps(
        &mut self,
        out: &mut [StoredApp],
        scratch: &mut [u8],
    ) -> Result<usize, AppStorageError>;
    fn write_state(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStorageError>;
    fn read_state(
        &mut self,
        app_id: &str,
        out: &mut [u8],
    ) -> Result<Option<usize>, AppStorageError>;
    fn delete_state(&mut self, app_id: &str) -> Result<(), AppStorageError>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct AppRegistry {
    entries: [AppRegistryEntry; APP_REGISTRY_CAP],
}

impl AppRegistry {
    pub const fn new() -> Self {
        Self {
            entries: [AppRegistryEntry::empty(); APP_REGISTRY_CAP],
        }
    }

    pub fn reserve_install(
        &mut self,
        app_id: &str,
        len: usize,
        max_len: usize,
    ) -> Result<AppSlot, AppRegistryError> {
        validate_app_id(app_id)?;
        if len > max_len {
            return Err(AppRegistryError::TooLarge);
        }
        if let Some(slot) = self.find(app_id) {
            return Ok(slot);
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if !entry.occupied {
                return Ok(AppSlot(index));
            }
        }
        Err(AppRegistryError::Full)
    }

    pub fn commit_install(
        &mut self,
        slot: AppSlot,
        app_id: &str,
        len: usize,
        hash: u32,
    ) -> Result<(), AppRegistryError> {
        validate_app_id(app_id)?;
        let entry = self
            .entries
            .get_mut(slot.0)
            .ok_or(AppRegistryError::InvalidSlot)?;
        entry.name = AppName::new(app_id)?;
        entry.len = len;
        entry.hash = hash;
        entry.occupied = true;
        Ok(())
    }

    pub fn find(&self, app_id: &str) -> Option<AppSlot> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.occupied && entry.name() == app_id)
            .map(|(index, _)| AppSlot(index))
    }

    pub fn entry(&self, slot: AppSlot) -> Option<&AppRegistryEntry> {
        self.entries.get(slot.0).filter(|entry| entry.occupied)
    }

    pub fn len_for_slot(&self, slot: AppSlot) -> Option<usize> {
        self.entry(slot).map(AppRegistryEntry::len)
    }

    pub fn iter(&self) -> impl Iterator<Item = (AppSlot, &AppRegistryEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.occupied)
            .map(|(index, entry)| (AppSlot(index), entry))
    }

    pub fn clear(&mut self) {
        self.entries = [AppRegistryEntry::empty(); APP_REGISTRY_CAP];
    }

    pub fn install_persistent<S: AppStorage>(
        &mut self,
        storage: &mut S,
        app_id: &str,
        bytes: &[u8],
        expected_hash: u32,
    ) -> Result<AppSlot, PersistentAppError> {
        let len = bytes.len();
        let slot = self.reserve_install(app_id, len, MAX_APP_BYTES)?;
        let actual_hash = fnv1a(bytes);
        if actual_hash != expected_hash {
            return Err(PersistentAppError::HashMismatch {
                expected: expected_hash,
                actual: actual_hash,
            });
        }
        Program::parse(bytes)?;
        storage.write_app(app_id, bytes)?;
        self.commit_install(slot, app_id, len, actual_hash)?;
        Ok(slot)
    }

    pub fn load_from_storage<S: AppStorage>(
        &mut self,
        storage: &mut S,
        scratch: &mut [u8],
    ) -> Result<usize, PersistentAppError> {
        storage.ensure_ready()?;
        self.clear();
        let mut stored = [StoredApp::empty(); APP_REGISTRY_CAP];
        let count = storage.list_apps(&mut stored, scratch)?;
        let mut loaded = 0usize;
        for app in stored.iter().take(count) {
            let app_id = app.name.as_str();
            let slot = self.reserve_install(app_id, app.len, MAX_APP_BYTES)?;
            if validate_stored_sqbc(storage, app_id, app.len, scratch).is_err() {
                continue;
            }
            self.commit_install(slot, app_id, app.len, app.hash)?;
            loaded += 1;
        }
        Ok(loaded)
    }
}

fn validate_stored_sqbc<S: AppStorage>(
    storage: &mut S,
    app_id: &str,
    expected_len: usize,
    scratch: &mut [u8],
) -> Result<(), PersistentAppError> {
    if scratch.len() < 16 {
        return Err(PersistentAppError::InvalidBytecode(VmError::InvalidHeader));
    }
    storage.read_app_range(app_id, 0, &mut scratch[..16])?;
    let header = Program::parse_header(&scratch[..16])?;
    if header.file_len != expected_len || header.header_len > scratch.len() {
        return Err(PersistentAppError::InvalidBytecode(VmError::InvalidHeader));
    }
    storage.read_app_range(app_id, 0, &mut scratch[..header.header_len])?;
    for index in 0..header.section_count {
        Program::parse_section_record(&scratch[..header.header_len], index)?;
    }
    Ok(())
}

impl Default for AppRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn validate_app_id(value: &str) -> Result<(), AppRegistryError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > APP_ID_CAP {
        return Err(AppRegistryError::InvalidAppId);
    }
    if !bytes[0].is_ascii_lowercase() {
        return Err(AppRegistryError::InvalidAppId);
    }
    for byte in bytes {
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-') {
            return Err(AppRegistryError::InvalidAppId);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevTimerEvent {
    Debug,
    Clock,
    Break,
}

impl DevTimerEvent {
    pub fn from_event(event: &str) -> Option<Self> {
        match event {
            "timer.clock" => Some(Self::Clock),
            "timer.break" => Some(Self::Break),
            "timer.debug" => Some(Self::Debug),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "timer.debug",
            Self::Clock => "timer.clock",
            Self::Break => "timer.break",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use squidvm_core::limits::MAX_SAVED_STATE_BYTES;
    use std::vec::Vec;

    #[test]
    fn installs_new_app_and_finds_slot() {
        let mut registry = AppRegistry::new();
        let slot = registry.reserve_install("reader-clock", 10, 100).unwrap();
        registry
            .commit_install(slot, "reader-clock", 10, 0x1234)
            .unwrap();

        assert_eq!(registry.find("reader-clock"), Some(slot));
        let entry = registry.entry(slot).unwrap();
        assert_eq!(entry.name(), "reader-clock");
        assert_eq!(entry.len(), 10);
        assert_eq!(entry.hash(), 0x1234);
    }

    #[test]
    fn reinstall_reuses_existing_slot() {
        let mut registry = AppRegistry::new();
        let slot = registry.reserve_install("main", 10, 100).unwrap();
        registry.commit_install(slot, "main", 10, 1).unwrap();

        let second = registry.reserve_install("main", 20, 100).unwrap();
        registry.commit_install(second, "main", 20, 2).unwrap();

        assert_eq!(second, slot);
        assert_eq!(registry.entry(slot).unwrap().len(), 20);
        assert_eq!(registry.iter().count(), 1);
    }

    #[test]
    fn rejects_invalid_app_ids() {
        for value in ["", "Bad", "123-bad", "bad/path", "bad_name"] {
            assert_eq!(
                AppName::new(value).unwrap_err(),
                AppRegistryError::InvalidAppId
            );
        }
    }

    #[test]
    fn rejects_too_large_app() {
        let mut registry = AppRegistry::new();
        assert_eq!(
            registry.reserve_install("main", 101, 100),
            Err(AppRegistryError::TooLarge)
        );
    }

    #[test]
    fn rejects_when_full() {
        let mut registry = AppRegistry::new();
        for index in 0..APP_REGISTRY_CAP {
            let name = match index {
                0 => "app-a",
                1 => "app-b",
                2 => "app-c",
                3 => "app-d",
                4 => "app-e",
                _ => "app-f",
            };
            let slot = registry.reserve_install(name, 1, 100).unwrap();
            registry
                .commit_install(slot, name, 1, index as u32)
                .unwrap();
        }

        assert_eq!(
            registry.reserve_install("app-g", 1, 100),
            Err(AppRegistryError::Full)
        );
    }

    #[test]
    fn lists_installed_apps() {
        let mut registry = AppRegistry::new();
        let main = registry.reserve_install("main", 4, 100).unwrap();
        registry.commit_install(main, "main", 4, 0xaa).unwrap();
        let worker = registry.reserve_install("worker", 8, 100).unwrap();
        registry.commit_install(worker, "worker", 8, 0xbb).unwrap();

        let mut iter = registry.iter().map(|(_, entry)| entry.name());
        assert_eq!(iter.next(), Some("main"));
        assert_eq!(iter.next(), Some("worker"));
        assert_eq!(iter.next(), None);
    }

    #[derive(Clone, Copy)]
    struct MemoryApp {
        name: AppName,
        len: usize,
        hash: u32,
        bytes: [u8; MAX_APP_BYTES],
        state_len: usize,
        state_bytes: [u8; MAX_SAVED_STATE_BYTES],
        state_occupied: bool,
        occupied: bool,
    }

    impl MemoryApp {
        const fn empty() -> Self {
            Self {
                name: AppName::empty(),
                len: 0,
                hash: 0,
                bytes: [0; MAX_APP_BYTES],
                state_len: 0,
                state_bytes: [0; MAX_SAVED_STATE_BYTES],
                state_occupied: false,
                occupied: false,
            }
        }
    }

    struct MemoryAppStorage {
        files: [MemoryApp; APP_REGISTRY_CAP],
        ready: bool,
    }

    impl MemoryAppStorage {
        const fn new() -> Self {
            Self {
                files: [MemoryApp::empty(); APP_REGISTRY_CAP],
                ready: true,
            }
        }

        fn find(&self, app_id: &str) -> Option<usize> {
            self.files
                .iter()
                .enumerate()
                .find(|(_, file)| file.occupied && file.name.as_str() == app_id)
                .map(|(index, _)| index)
        }
    }

    impl AppStorage for MemoryAppStorage {
        fn ensure_ready(&mut self) -> Result<(), AppStorageError> {
            if self.ready {
                Ok(())
            } else {
                Err(AppStorageError::NotMounted)
            }
        }

        fn format(&mut self) -> Result<(), AppStorageError> {
            self.files = [MemoryApp::empty(); APP_REGISTRY_CAP];
            self.ready = true;
            Ok(())
        }

        fn write_app(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStorageError> {
            self.ensure_ready()?;
            validate_app_id(app_id).map_err(|_| AppStorageError::InvalidName)?;
            if bytes.len() > MAX_APP_BYTES {
                return Err(AppStorageError::NoSpace);
            }
            let index = self
                .find(app_id)
                .or_else(|| {
                    self.files
                        .iter()
                        .enumerate()
                        .find(|(_, file)| !file.occupied)
                        .map(|(index, _)| index)
                })
                .ok_or(AppStorageError::NoSpace)?;
            let file = &mut self.files[index];
            file.name = AppName::new(app_id).map_err(|_| AppStorageError::InvalidName)?;
            file.len = bytes.len();
            file.hash = fnv1a(bytes);
            file.bytes[..bytes.len()].copy_from_slice(bytes);
            file.occupied = true;
            Ok(())
        }

        fn read_app(&mut self, app_id: &str, out: &mut [u8]) -> Result<usize, AppStorageError> {
            self.ensure_ready()?;
            let index = self.find(app_id).ok_or(AppStorageError::NotFound)?;
            let file = &self.files[index];
            if out.len() < file.len {
                return Err(AppStorageError::NoSpace);
            }
            out[..file.len].copy_from_slice(&file.bytes[..file.len]);
            Ok(file.len)
        }

        fn read_app_range(
            &mut self,
            app_id: &str,
            offset: usize,
            out: &mut [u8],
        ) -> Result<usize, AppStorageError> {
            self.ensure_ready()?;
            let index = self.find(app_id).ok_or(AppStorageError::NotFound)?;
            let file = &self.files[index];
            let end = offset
                .checked_add(out.len())
                .ok_or(AppStorageError::NoSpace)?;
            if end > file.len {
                return Err(AppStorageError::NoSpace);
            }
            out.copy_from_slice(&file.bytes[offset..end]);
            Ok(out.len())
        }

        fn list_apps(
            &mut self,
            out: &mut [StoredApp],
            _scratch: &mut [u8],
        ) -> Result<usize, AppStorageError> {
            self.ensure_ready()?;
            let mut count = 0usize;
            for file in &self.files {
                if !file.occupied {
                    continue;
                }
                if count == out.len() {
                    break;
                }
                out[count] = StoredApp {
                    name: file.name,
                    len: file.len,
                    hash: file.hash,
                };
                count += 1;
            }
            Ok(count)
        }

        fn write_state(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStorageError> {
            self.ensure_ready()?;
            if bytes.len() > MAX_SAVED_STATE_BYTES {
                return Err(AppStorageError::NoSpace);
            }
            let index = self.find(app_id).ok_or(AppStorageError::NotFound)?;
            let file = &mut self.files[index];
            file.state_bytes[..bytes.len()].copy_from_slice(bytes);
            file.state_len = bytes.len();
            file.state_occupied = true;
            Ok(())
        }

        fn read_state(
            &mut self,
            app_id: &str,
            out: &mut [u8],
        ) -> Result<Option<usize>, AppStorageError> {
            self.ensure_ready()?;
            let index = self.find(app_id).ok_or(AppStorageError::NotFound)?;
            let file = &self.files[index];
            if !file.state_occupied {
                return Ok(None);
            }
            if out.len() < file.state_len {
                return Err(AppStorageError::NoSpace);
            }
            out[..file.state_len].copy_from_slice(&file.state_bytes[..file.state_len]);
            Ok(Some(file.state_len))
        }

        fn delete_state(&mut self, app_id: &str) -> Result<(), AppStorageError> {
            self.ensure_ready()?;
            let index = self.find(app_id).ok_or(AppStorageError::NotFound)?;
            self.files[index].state_len = 0;
            self.files[index].state_occupied = false;
            Ok(())
        }
    }

    #[test]
    fn app_storage_reads_exact_ranges_without_loading_whole_app() {
        let mut storage = MemoryAppStorage::new();
        let bytes = sqbc_fixture();
        storage.write_app("main", &bytes).unwrap();
        let mut header = [0u8; 16];

        let read = storage.read_app_range("main", 0, &mut header).unwrap();

        assert_eq!(read, header.len());
        assert_eq!(&header[0..4], b"SQBC");
        assert_eq!(u16::from_le_bytes(header[4..6].try_into().unwrap()), 3);
    }

    fn sqbc_fixture() -> Vec<u8> {
        let source = r#"
app "main"

state { count: int = 0 }

event.on("app.start") {
  debug.print("start", count)
}

screen("main") {}
"#;
        let response = squidc_core::compile_with_profile(
            squidc_core::CompileRequest {
                source: source.to_string(),
                target_id: squidc_core::PORTABLE_TARGET_ID.to_string(),
            },
            squidc_core::BuildProfile::Dev,
        );
        assert_eq!(response.diagnostics, Vec::new());
        squidc_core::sqbc_v2::encode_sqbc_v2(&response.ir.unwrap()).unwrap()
    }

    #[test]
    fn persistent_install_writes_storage_and_cache() {
        let mut registry = AppRegistry::new();
        let mut storage = MemoryAppStorage::new();
        let bytes = sqbc_fixture();
        let hash = fnv1a(&bytes);

        let slot = registry
            .install_persistent(&mut storage, "main", &bytes, hash)
            .unwrap();

        assert_eq!(registry.find("main"), Some(slot));
        assert_eq!(
            storage
                .list_apps(&mut [StoredApp::empty(); 1], &mut [0u8; MAX_APP_BYTES])
                .unwrap(),
            1
        );
        let entry = registry.entry(slot).unwrap();
        assert_eq!(entry.name(), "main");
        assert_eq!(entry.len(), bytes.len());
        assert_eq!(entry.hash(), hash);
    }

    #[test]
    fn persistent_install_rejects_bad_hash_before_publish() {
        let mut registry = AppRegistry::new();
        let mut storage = MemoryAppStorage::new();
        let bytes = sqbc_fixture();

        let error = registry
            .install_persistent(&mut storage, "main", &bytes, 0)
            .unwrap_err();

        assert!(matches!(error, PersistentAppError::HashMismatch { .. }));
        assert_eq!(registry.find("main"), None);
        assert_eq!(
            storage
                .list_apps(&mut [StoredApp::empty(); 1], &mut [0u8; MAX_APP_BYTES])
                .unwrap(),
            0
        );
    }

    #[test]
    fn startup_scan_rebuilds_registry_metadata_with_one_scratch_buffer() {
        let mut storage = MemoryAppStorage::new();
        let bytes = sqbc_fixture();
        storage.write_app("main", &bytes).unwrap();

        let mut registry = AppRegistry::new();
        let mut scratch = [0u8; MAX_APP_BYTES];
        assert_eq!(
            registry
                .load_from_storage(&mut storage, &mut scratch)
                .unwrap(),
            1
        );

        let slot = registry.find("main").unwrap();
        assert_eq!(registry.entry(slot).unwrap().hash(), fnv1a(&bytes));
        assert_eq!(registry.entry(slot).unwrap().len(), bytes.len());
    }

    #[test]
    fn format_clears_persistent_apps() {
        let mut storage = MemoryAppStorage::new();
        let bytes = sqbc_fixture();
        storage.write_app("main", &bytes).unwrap();

        storage.format().unwrap();

        assert_eq!(
            storage
                .list_apps(&mut [StoredApp::empty(); 1], &mut [0u8; MAX_APP_BYTES])
                .unwrap(),
            0
        );
    }

    #[test]
    fn maps_timer_event_names() {
        assert_eq!(
            DevTimerEvent::from_event("timer.clock"),
            Some(DevTimerEvent::Clock)
        );
        assert_eq!(DevTimerEvent::from_event("timer.unknown"), None);
        assert_eq!(DevTimerEvent::Debug.as_str(), "timer.debug");
        assert_eq!(DevTimerEvent::Break.as_str(), "timer.break");
    }
}
