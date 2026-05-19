//! Temporary development harness helpers for the ESP32-C3 Super Mini reference
//! firmware.
//!
//! This module intentionally models the current RAM-only app store. It is not
//! the final persistent app registry.

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
    const fn empty() -> Self {
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
            registry.commit_install(slot, name, 1, index as u32).unwrap();
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
