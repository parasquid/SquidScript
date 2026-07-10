use squidvm_core::{
    error::VmError,
    limits::{MAX_APP_BYTES, MAX_APP_ID_BYTES, MAX_INSTALLED_APPS, MAX_SAVED_STATE_BYTES},
    program::ProgramIndex,
    reader::SqbcReader,
};

pub const MAX_APP_RESOURCE_PATH_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppStoreError {
    NotFound,
    InvalidAppId,
    InvalidPath,
    TooLarge,
    RegistryFull,
    Incomplete,
    OutOfOrder,
    CorruptSqbc,
    AppIdMismatch,
    NoSpace,
    Io,
}

pub trait NativeAppStorage {
    fn for_each_app(&mut self, visit: &mut dyn FnMut(&str, usize)) -> Result<(), AppStoreError>;
    fn app_size(&mut self, app_id: &str) -> Result<usize, AppStoreError>;
    fn read_app_at(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), AppStoreError>;
    fn begin_app_install(&mut self, app_id: &str, total_len: usize) -> Result<(), AppStoreError>;
    fn write_app_install_chunk(
        &mut self,
        app_id: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), AppStoreError>;
    fn read_app_install_at(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), AppStoreError>;
    fn publish_app_install(&mut self, app_id: &str) -> Result<(), AppStoreError>;
    fn abort_app_install(&mut self, app_id: &str) -> Result<(), AppStoreError>;
    fn begin_resource_install(
        &mut self,
        app_id: &str,
        path: &str,
        total_len: usize,
    ) -> Result<(), AppStoreError>;
    fn write_resource_install_chunk(
        &mut self,
        app_id: &str,
        path: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), AppStoreError>;
    fn publish_resource_install(&mut self, app_id: &str, path: &str) -> Result<(), AppStoreError>;
    fn read_resource_at(
        &mut self,
        app_id: &str,
        path: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), AppStoreError>;
    fn resource_size(&mut self, app_id: &str, path: &str) -> Result<usize, AppStoreError>;
    fn load_state(&mut self, app_id: &str, out: &mut [u8]) -> Result<Option<usize>, AppStoreError>;
    fn save_state_atomic(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStoreError>;
    fn delete_state(&mut self, app_id: &str) -> Result<(), AppStoreError>;
    fn format(&mut self) -> Result<(), AppStoreError>;
    fn capacity(&mut self) -> Result<(usize, usize), AppStoreError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppRegistryEntry {
    app_id: AppId,
    pub sqbc_bytes: usize,
}

impl AppRegistryEntry {
    pub fn app_id(&self) -> &str {
        self.app_id.as_str()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AppId {
    bytes: [u8; MAX_APP_ID_BYTES],
    len: usize,
}

impl AppId {
    const fn empty() -> Self {
        Self {
            bytes: [0; MAX_APP_ID_BYTES],
            len: 0,
        }
    }

    fn parse(value: &str) -> Result<Self, AppStoreError> {
        if value.is_empty()
            || value.len() >= MAX_APP_ID_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(AppStoreError::InvalidAppId);
        }
        let mut result = Self::empty();
        result.bytes[..value.len()].copy_from_slice(value.as_bytes());
        result.len = value.len();
        Ok(result)
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

pub struct NativeAppStore<S> {
    storage: S,
    registry: [Option<AppRegistryEntry>; MAX_INSTALLED_APPS],
    registry_len: usize,
    pending_app: AppId,
    pending_expected: usize,
    pending_received: usize,
}

impl<S: NativeAppStorage> NativeAppStore<S> {
    pub const fn new(storage: S) -> Self {
        Self {
            storage,
            registry: [None; MAX_INSTALLED_APPS],
            registry_len: 0,
            pending_app: AppId::empty(),
            pending_expected: 0,
            pending_received: 0,
        }
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn registry(&self) -> &[Option<AppRegistryEntry>] {
        &self.registry[..self.registry_len]
    }

    pub fn find(&self, app_id: &str) -> Option<AppRegistryEntry> {
        self.registry[..self.registry_len]
            .iter()
            .flatten()
            .copied()
            .find(|entry| entry.app_id() == app_id)
    }

    pub fn rebuild(&mut self, scratch: &mut [u8]) -> Result<(), AppStoreError> {
        self.registry.fill(None);
        self.registry_len = 0;
        let mut discovered = [None; MAX_INSTALLED_APPS];
        let mut count = 0usize;
        let mut error = None;
        self.storage.for_each_app(&mut |app_id, size| {
            if error.is_some() {
                return;
            }
            if count == discovered.len() {
                error = Some(AppStoreError::RegistryFull);
                return;
            }
            match AppId::parse(app_id) {
                Ok(app_id) => {
                    discovered[count] = Some(AppRegistryEntry {
                        app_id,
                        sqbc_bytes: size,
                    });
                    count += 1;
                }
                Err(value) => error = Some(value),
            }
        })?;
        if let Some(error) = error {
            return Err(error);
        }
        for entry in discovered[..count].iter().flatten().copied() {
            self.validate_published(entry, scratch)?;
            self.insert_or_replace(entry)?;
        }
        Ok(())
    }

    pub fn begin_install(&mut self, app_id: &str, total_len: usize) -> Result<(), AppStoreError> {
        let app_id = AppId::parse(app_id)?;
        if total_len == 0 || total_len > MAX_APP_BYTES {
            return Err(AppStoreError::TooLarge);
        }
        if self.find(app_id.as_str()).is_none() && self.registry_len == MAX_INSTALLED_APPS {
            return Err(AppStoreError::RegistryFull);
        }
        self.storage.begin_app_install(app_id.as_str(), total_len)?;
        self.pending_app = app_id;
        self.pending_expected = total_len;
        self.pending_received = 0;
        Ok(())
    }

    pub fn write_install_chunk(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), AppStoreError> {
        if self.pending_expected == 0 || bytes.is_empty() || offset != self.pending_received {
            return Err(AppStoreError::OutOfOrder);
        }
        let end = offset
            .checked_add(bytes.len())
            .ok_or(AppStoreError::TooLarge)?;
        if end > self.pending_expected {
            return Err(AppStoreError::TooLarge);
        }
        self.storage
            .write_app_install_chunk(self.pending_app.as_str(), offset, bytes)?;
        self.pending_received = end;
        Ok(())
    }

    pub fn commit_install(&mut self, scratch: &mut [u8]) -> Result<(), AppStoreError> {
        if self.pending_expected == 0 || self.pending_received != self.pending_expected {
            return Err(AppStoreError::Incomplete);
        }
        let entry = AppRegistryEntry {
            app_id: self.pending_app,
            sqbc_bytes: self.pending_expected,
        };
        self.validate_pending(entry, scratch)?;
        self.storage.publish_app_install(entry.app_id())?;
        self.insert_or_replace(entry)?;
        self.clear_pending();
        Ok(())
    }

    pub fn abort_install(&mut self) -> Result<(), AppStoreError> {
        if self.pending_expected != 0 {
            self.storage.abort_app_install(self.pending_app.as_str())?;
        }
        self.clear_pending();
        Ok(())
    }

    pub fn read_app_at(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), AppStoreError> {
        let entry = self.find(app_id).ok_or(AppStoreError::NotFound)?;
        if offset
            .checked_add(out.len())
            .is_none_or(|end| end > entry.sqbc_bytes)
        {
            return Err(AppStoreError::TooLarge);
        }
        self.storage.read_app_at(app_id, offset, out)
    }

    pub fn save_state(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStoreError> {
        if bytes.len() > MAX_SAVED_STATE_BYTES || self.find(app_id).is_none() {
            return Err(AppStoreError::TooLarge);
        }
        self.storage.save_state_atomic(app_id, bytes)
    }

    pub fn load_state(
        &mut self,
        app_id: &str,
        out: &mut [u8],
    ) -> Result<Option<usize>, AppStoreError> {
        self.storage.load_state(app_id, out)
    }

    pub fn format(&mut self) -> Result<(), AppStoreError> {
        self.storage.format()?;
        self.registry.fill(None);
        self.registry_len = 0;
        self.clear_pending();
        Ok(())
    }

    fn validate_pending(
        &mut self,
        entry: AppRegistryEntry,
        scratch: &mut [u8],
    ) -> Result<(), AppStoreError> {
        let mut reader = StorageReader {
            storage: &mut self.storage,
            app_id: entry.app_id(),
            pending: true,
        };
        validate_reader(&mut reader, entry, scratch)
    }

    fn validate_published(
        &mut self,
        entry: AppRegistryEntry,
        scratch: &mut [u8],
    ) -> Result<(), AppStoreError> {
        if entry.sqbc_bytes == 0 || entry.sqbc_bytes > MAX_APP_BYTES {
            return Err(AppStoreError::TooLarge);
        }
        let mut reader = StorageReader {
            storage: &mut self.storage,
            app_id: entry.app_id(),
            pending: false,
        };
        validate_reader(&mut reader, entry, scratch)
    }

    fn insert_or_replace(&mut self, entry: AppRegistryEntry) -> Result<(), AppStoreError> {
        if let Some(slot) = self.registry[..self.registry_len]
            .iter_mut()
            .find(|slot| slot.is_some_and(|current| current.app_id() == entry.app_id()))
        {
            *slot = Some(entry);
            return Ok(());
        }
        if self.registry_len == self.registry.len() {
            return Err(AppStoreError::RegistryFull);
        }
        self.registry[self.registry_len] = Some(entry);
        self.registry_len += 1;
        Ok(())
    }

    fn clear_pending(&mut self) {
        self.pending_app = AppId::empty();
        self.pending_expected = 0;
        self.pending_received = 0;
    }
}

fn validate_reader(
    reader: &mut impl SqbcReader,
    entry: AppRegistryEntry,
    scratch: &mut [u8],
) -> Result<(), AppStoreError> {
    ProgramIndex::parse_from_reader(reader, scratch).map_err(|_| AppStoreError::CorruptSqbc)?;
    let app_id = ProgramIndex::app_id_from_reader(reader, scratch)
        .map_err(|_| AppStoreError::CorruptSqbc)?;
    if app_id != entry.app_id() {
        return Err(AppStoreError::AppIdMismatch);
    }
    Ok(())
}

struct StorageReader<'a, S> {
    storage: &'a mut S,
    app_id: &'a str,
    pending: bool,
}

impl<S: NativeAppStorage> SqbcReader for StorageReader<'_, S> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let result = if self.pending {
            self.storage.read_app_install_at(self.app_id, offset, out)
        } else {
            self.storage.read_app_at(self.app_id, offset, out)
        };
        result.map_err(|_| VmError::ReadFailed)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::{collections::BTreeMap, vec::Vec};

    use squidc_core::compile::{compile, CompileRequest};

    use super::*;

    #[derive(Default)]
    struct MemoryStorage {
        apps: BTreeMap<std::string::String, Vec<u8>>,
        pending: BTreeMap<std::string::String, Vec<u8>>,
        states: BTreeMap<std::string::String, Vec<u8>>,
        fail_publish: bool,
    }

    impl NativeAppStorage for MemoryStorage {
        fn for_each_app(
            &mut self,
            visit: &mut dyn FnMut(&str, usize),
        ) -> Result<(), AppStoreError> {
            for (id, bytes) in &self.apps {
                visit(id, bytes.len());
            }
            Ok(())
        }
        fn app_size(&mut self, app_id: &str) -> Result<usize, AppStoreError> {
            self.apps
                .get(app_id)
                .map(Vec::len)
                .ok_or(AppStoreError::NotFound)
        }
        fn read_app_at(
            &mut self,
            app_id: &str,
            offset: usize,
            out: &mut [u8],
        ) -> Result<(), AppStoreError> {
            read_map(&self.apps, app_id, offset, out)
        }
        fn begin_app_install(
            &mut self,
            app_id: &str,
            total_len: usize,
        ) -> Result<(), AppStoreError> {
            self.pending
                .insert(app_id.into(), Vec::with_capacity(total_len));
            Ok(())
        }
        fn write_app_install_chunk(
            &mut self,
            app_id: &str,
            offset: usize,
            bytes: &[u8],
        ) -> Result<(), AppStoreError> {
            let pending = self
                .pending
                .get_mut(app_id)
                .ok_or(AppStoreError::NotFound)?;
            if pending.len() != offset {
                return Err(AppStoreError::OutOfOrder);
            }
            pending.extend_from_slice(bytes);
            Ok(())
        }
        fn read_app_install_at(
            &mut self,
            app_id: &str,
            offset: usize,
            out: &mut [u8],
        ) -> Result<(), AppStoreError> {
            read_map(&self.pending, app_id, offset, out)
        }
        fn publish_app_install(&mut self, app_id: &str) -> Result<(), AppStoreError> {
            if self.fail_publish {
                return Err(AppStoreError::Io);
            }
            let bytes = self.pending.remove(app_id).ok_or(AppStoreError::NotFound)?;
            self.apps.insert(app_id.into(), bytes);
            Ok(())
        }
        fn abort_app_install(&mut self, app_id: &str) -> Result<(), AppStoreError> {
            self.pending.remove(app_id);
            Ok(())
        }
        fn begin_resource_install(
            &mut self,
            _: &str,
            _: &str,
            _: usize,
        ) -> Result<(), AppStoreError> {
            Ok(())
        }
        fn write_resource_install_chunk(
            &mut self,
            _: &str,
            _: &str,
            _: usize,
            _: &[u8],
        ) -> Result<(), AppStoreError> {
            Ok(())
        }
        fn publish_resource_install(&mut self, _: &str, _: &str) -> Result<(), AppStoreError> {
            Ok(())
        }
        fn read_resource_at(
            &mut self,
            _: &str,
            _: &str,
            _: usize,
            _: &mut [u8],
        ) -> Result<(), AppStoreError> {
            Err(AppStoreError::NotFound)
        }
        fn resource_size(&mut self, _: &str, _: &str) -> Result<usize, AppStoreError> {
            Err(AppStoreError::NotFound)
        }
        fn load_state(
            &mut self,
            app_id: &str,
            out: &mut [u8],
        ) -> Result<Option<usize>, AppStoreError> {
            let Some(bytes) = self.states.get(app_id) else {
                return Ok(None);
            };
            out[..bytes.len()].copy_from_slice(bytes);
            Ok(Some(bytes.len()))
        }
        fn save_state_atomic(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStoreError> {
            self.states.insert(app_id.into(), bytes.into());
            Ok(())
        }
        fn delete_state(&mut self, app_id: &str) -> Result<(), AppStoreError> {
            self.states.remove(app_id);
            Ok(())
        }
        fn format(&mut self) -> Result<(), AppStoreError> {
            self.apps.clear();
            self.pending.clear();
            self.states.clear();
            Ok(())
        }
        fn capacity(&mut self) -> Result<(usize, usize), AppStoreError> {
            Ok((1024 * 1024, 1024 * 1024))
        }
    }

    fn read_map(
        map: &BTreeMap<std::string::String, Vec<u8>>,
        key: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), AppStoreError> {
        let bytes = map.get(key).ok_or(AppStoreError::NotFound)?;
        out.copy_from_slice(
            bytes
                .get(offset..offset + out.len())
                .ok_or(AppStoreError::TooLarge)?,
        );
        Ok(())
    }

    fn sqbc(app_id: &str) -> Vec<u8> {
        let compiled = compile(CompileRequest {
            source: std::format!(
                "app \"{app_id}\"\nevent.on(\"app.start\") {{ debug.print(\"ok\") }}\n"
            ),
            target_id: "xteink-x4".into(),
        });
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap()
    }

    fn install(
        store: &mut NativeAppStore<MemoryStorage>,
        app_id: &str,
        bytes: &[u8],
    ) -> Result<(), AppStoreError> {
        let mut scratch = [0; 1024];
        store.begin_install(app_id, bytes.len())?;
        for (offset, chunk) in bytes.chunks(31).enumerate() {
            store.write_install_chunk(offset * 31, chunk)?;
        }
        store.commit_install(&mut scratch)
    }

    #[test]
    fn installs_valid_sqbc_and_rebuilds_registry() {
        let bytes = sqbc("reader");
        let mut store = NativeAppStore::new(MemoryStorage::default());
        install(&mut store, "reader", &bytes).unwrap();
        assert_eq!(store.find("reader").unwrap().sqbc_bytes, bytes.len());
        let storage = store.storage;
        let mut rebuilt = NativeAppStore::new(storage);
        rebuilt.rebuild(&mut [0; 1024]).unwrap();
        assert_eq!(rebuilt.find("reader").unwrap().sqbc_bytes, bytes.len());
    }

    #[test]
    fn rejects_mismatched_corrupt_incomplete_and_out_of_order_installs() {
        let bytes = sqbc("reader");
        let mut store = NativeAppStore::new(MemoryStorage::default());
        store.begin_install("other", bytes.len()).unwrap();
        store.write_install_chunk(0, &bytes).unwrap();
        assert_eq!(
            store.commit_install(&mut [0; 1024]),
            Err(AppStoreError::AppIdMismatch)
        );
        store.abort_install().unwrap();
        store.begin_install("reader", bytes.len()).unwrap();
        assert_eq!(
            store.write_install_chunk(1, &bytes[..1]),
            Err(AppStoreError::OutOfOrder)
        );
        store.write_install_chunk(0, &bytes[..1]).unwrap();
        assert_eq!(
            store.commit_install(&mut [0; 1024]),
            Err(AppStoreError::Incomplete)
        );
    }

    #[test]
    fn failed_atomic_replacement_keeps_previous_app() {
        let first = sqbc("reader");
        let mut store = NativeAppStore::new(MemoryStorage::default());
        install(&mut store, "reader", &first).unwrap();
        let replacement = sqbc("reader");
        store.storage.fail_publish = true;
        store.begin_install("reader", replacement.len()).unwrap();
        store.write_install_chunk(0, &replacement).unwrap();
        assert_eq!(store.commit_install(&mut [0; 1024]), Err(AppStoreError::Io));
        assert_eq!(store.storage.apps.get("reader"), Some(&first));
    }

    #[test]
    fn enforces_eight_app_registry_and_persists_state() {
        let mut store = NativeAppStore::new(MemoryStorage::default());
        for index in 0..MAX_INSTALLED_APPS {
            let id = std::format!("app-{index}");
            install(&mut store, &id, &sqbc(&id)).unwrap();
        }
        let ninth = sqbc("ninth");
        assert_eq!(
            store.begin_install("ninth", ninth.len()),
            Err(AppStoreError::RegistryFull)
        );
        store.save_state("app-0", b"state").unwrap();
        let mut state = [0; MAX_SAVED_STATE_BYTES];
        assert_eq!(store.load_state("app-0", &mut state).unwrap(), Some(5));
        assert_eq!(&state[..5], b"state");
        store.format().unwrap();
        assert!(store.registry().is_empty());
        assert_eq!(store.load_state("app-0", &mut state).unwrap(), None);
    }
}
