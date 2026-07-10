use core::fmt::Write as _;
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use heapless::String;
use littlefs2::{
    consts::{U16, U256},
    driver::Storage,
    fs::Filesystem,
    io::{Error, Read as _, Result, SeekFrom, Write as _},
    path::{Path, PathBuf},
};
use squidscript_fw_core::app_store::{AppStoreError, NativeAppStorage};

pub const SQUIDSCRIPT_PARTITION_OFFSET: usize = 0x510000;
pub const SQUIDSCRIPT_PARTITION_SIZE: usize = 0xae0000;
pub const FLASH_ERASE_SIZE: usize = 4096;

// littlefs 2.11 uses these libc helpers, but the ESP32-C3 no_std link has no libc.
#[cfg(target_arch = "riscv32")]
#[no_mangle]
pub unsafe extern "C" fn strspn(value: *const i8, accepted: *const i8) -> usize {
    c_span(value, accepted, true)
}

#[cfg(target_arch = "riscv32")]
#[no_mangle]
pub unsafe extern "C" fn strcspn(value: *const i8, rejected: *const i8) -> usize {
    c_span(value, rejected, false)
}

#[cfg(target_arch = "riscv32")]
unsafe fn c_span(value: *const i8, set: *const i8, while_in_set: bool) -> usize {
    let mut length = 0;
    while *value.add(length) != 0 {
        let current = *value.add(length);
        let mut cursor = 0;
        let mut found = false;
        while *set.add(cursor) != 0 {
            if *set.add(cursor) == current {
                found = true;
                break;
            }
            cursor += 1;
        }
        if found != while_in_set {
            break;
        }
        length += 1;
    }
    length
}

pub struct PartitionStorage<F, const OFFSET: usize, const SIZE: usize> {
    flash: F,
}

impl<F, const OFFSET: usize, const SIZE: usize> PartitionStorage<F, OFFSET, SIZE> {
    pub const fn new(flash: F) -> Self {
        Self { flash }
    }

    pub fn into_inner(self) -> F {
        self.flash
    }

    fn absolute_range(off: usize, len: usize) -> Result<(u32, u32)> {
        let end = off.checked_add(len).ok_or(Error::IO)?;
        if end > SIZE {
            return Err(Error::IO);
        }
        let start = OFFSET.checked_add(off).ok_or(Error::IO)?;
        let absolute_end = start.checked_add(len).ok_or(Error::IO)?;
        Ok((
            u32::try_from(start).map_err(|_| Error::IO)?,
            u32::try_from(absolute_end).map_err(|_| Error::IO)?,
        ))
    }
}

pub type X4AppPartition<F> =
    PartitionStorage<F, SQUIDSCRIPT_PARTITION_OFFSET, SQUIDSCRIPT_PARTITION_SIZE>;

pub struct LittleFsAppStorage<F> {
    storage: X4AppPartition<F>,
}

impl<F> LittleFsAppStorage<F>
where
    F: NorFlash + ReadNorFlash,
{
    pub const fn new(flash: F) -> Self {
        Self {
            storage: X4AppPartition::new(flash),
        }
    }

    pub fn initialize(&mut self) -> core::result::Result<(), AppStoreError> {
        if !Filesystem::is_mountable(&mut self.storage) {
            if !partition_is_blank(&mut self.storage)? {
                return Err(AppStoreError::Io);
            }
            Filesystem::format(&mut self.storage).map_err(map_littlefs_error)?;
        }
        self.mount(|fs| {
            ensure_store_dirs(fs)?;
            recover_interrupted_installs(fs)
        })
    }

    pub fn into_inner(self) -> F {
        self.storage.into_inner()
    }

    fn mount<R>(
        &mut self,
        operation: impl FnOnce(&Filesystem<'_, X4AppPartition<F>>) -> Result<R>,
    ) -> core::result::Result<R, AppStoreError> {
        Filesystem::mount_and_then(&mut self.storage, operation).map_err(map_littlefs_error)
    }
}

fn ensure_store_dirs<S: littlefs2::driver::Storage>(fs: &Filesystem<'_, S>) -> Result<()> {
    fs.create_dir_all(littlefs2::path!("/apps"))?;
    fs.create_dir_all(littlefs2::path!("/state"))?;
    fs.create_dir_all(littlefs2::path!("/lifecycle"))?;
    fs.create_dir_all(littlefs2::path!("/tmp"))
}

fn partition_is_blank<S: littlefs2::driver::Storage>(
    storage: &mut S,
) -> core::result::Result<bool, AppStoreError> {
    let mut block = [0; 256];
    let mut offset = 0;
    while offset < S::BLOCK_SIZE * S::BLOCK_COUNT {
        let read = S::read(storage, offset, &mut block).map_err(map_littlefs_error)?;
        if read != block.len() || block.iter().any(|byte| *byte != 0xff) {
            return Ok(false);
        }
        offset += block.len();
    }
    Ok(true)
}

fn recover_interrupted_installs<S: littlefs2::driver::Storage>(
    fs: &Filesystem<'_, S>,
) -> Result<()> {
    fs.read_dir_and_then(littlefs2::path!("/tmp"), |entries| {
        for entry in entries {
            let entry = entry?;
            if !entry.file_type().is_dir() {
                continue;
            }
            let name = entry.file_name().as_str();
            if let Some(app_id) = name.strip_prefix("previous-") {
                let app = app_dir_path(app_id).map_err(|_| Error::INVALID)?;
                if fs.metadata(app.as_path()).is_ok() {
                    fs.remove_dir_all(entry.path())?;
                } else {
                    fs.rename(entry.path(), app.as_path())?;
                }
            } else if name.starts_with("install-") {
                fs.remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    })
}

fn app_main_path(app_id: &str) -> core::result::Result<PathBuf, AppStoreError> {
    dynamic_path(core::format_args!("/apps/{app_id}/main.sqbc"))
}

fn app_dir_path(app_id: &str) -> core::result::Result<PathBuf, AppStoreError> {
    dynamic_path(core::format_args!("/apps/{app_id}"))
}

fn app_resource_path(app_id: &str, path: &str) -> core::result::Result<PathBuf, AppStoreError> {
    validate_resource_path(path)?;
    dynamic_path(core::format_args!("/apps/{app_id}/resources/{path}"))
}

fn install_temp_path(app_id: &str) -> core::result::Result<PathBuf, AppStoreError> {
    dynamic_path(core::format_args!("/tmp/install-{app_id}/main.sqbc"))
}

fn install_temp_dir(app_id: &str) -> core::result::Result<PathBuf, AppStoreError> {
    dynamic_path(core::format_args!("/tmp/install-{app_id}"))
}

fn install_resource_path(app_id: &str, path: &str) -> core::result::Result<PathBuf, AppStoreError> {
    validate_resource_path(path)?;
    dynamic_path(core::format_args!("/tmp/install-{app_id}/resources/{path}"))
}

fn previous_app_dir(app_id: &str) -> core::result::Result<PathBuf, AppStoreError> {
    dynamic_path(core::format_args!("/tmp/previous-{app_id}"))
}

fn state_path(app_id: &str) -> core::result::Result<PathBuf, AppStoreError> {
    dynamic_path(core::format_args!("/state/{app_id}.state"))
}

fn state_temp_path(app_id: &str) -> core::result::Result<PathBuf, AppStoreError> {
    dynamic_path(core::format_args!("/tmp/state-{app_id}.state"))
}

fn dynamic_path(args: core::fmt::Arguments<'_>) -> core::result::Result<PathBuf, AppStoreError> {
    let mut text = String::<256>::new();
    text.write_fmt(args)
        .map_err(|_| AppStoreError::InvalidPath)?;
    PathBuf::try_from(text.as_str()).map_err(|_| AppStoreError::InvalidPath)
}

fn validate_resource_path(path: &str) -> core::result::Result<(), AppStoreError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
    {
        return Err(AppStoreError::InvalidPath);
    }
    Ok(())
}

fn map_littlefs_error(error: Error) -> AppStoreError {
    if error == Error::NO_SUCH_ENTRY {
        AppStoreError::NotFound
    } else if error == Error::NO_SPACE {
        AppStoreError::NoSpace
    } else {
        AppStoreError::Io
    }
}

impl<F> NativeAppStorage for LittleFsAppStorage<F>
where
    F: NorFlash + ReadNorFlash,
{
    fn for_each_app(
        &mut self,
        visit: &mut dyn FnMut(&str, usize),
    ) -> core::result::Result<(), AppStoreError> {
        self.mount(|fs| {
            fs.read_dir_and_then(littlefs2::path!("/apps"), |entries| {
                for entry in entries {
                    let entry = entry?;
                    if !entry.file_type().is_dir() {
                        continue;
                    }
                    let app_id = entry.file_name().as_str();
                    if matches!(app_id, "." | "..") {
                        continue;
                    }
                    let main = app_main_path(app_id).map_err(|_| Error::INVALID)?;
                    if let Ok(metadata) = fs.metadata(main.as_path()) {
                        if metadata.is_file() {
                            visit(app_id, metadata.len());
                        }
                    }
                }
                Ok(())
            })
        })
    }

    fn app_size(&mut self, app_id: &str) -> core::result::Result<usize, AppStoreError> {
        let path = app_main_path(app_id)?;
        self.mount(|fs| fs.metadata(path.as_path()).map(|metadata| metadata.len()))
    }

    fn read_app_at(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> core::result::Result<(), AppStoreError> {
        let path = app_main_path(app_id)?;
        self.mount(|fs| read_file_at(fs, path.as_path(), offset, out))
    }

    fn begin_app_install(
        &mut self,
        app_id: &str,
        _total_len: usize,
    ) -> core::result::Result<(), AppStoreError> {
        let path = install_temp_path(app_id)?;
        let dir = install_temp_dir(app_id)?;
        self.mount(|fs| {
            fs.create_dir_all(dir.as_path())?;
            fs.create_file_and_then(path.as_path(), |_| Ok(()))
        })
    }

    fn write_app_install_chunk(
        &mut self,
        app_id: &str,
        offset: usize,
        bytes: &[u8],
    ) -> core::result::Result<(), AppStoreError> {
        let path = install_temp_path(app_id)?;
        self.mount(|fs| write_file_at(fs, path.as_path(), offset, bytes))
    }

    fn read_app_install_at(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> core::result::Result<(), AppStoreError> {
        let path = install_temp_path(app_id)?;
        self.mount(|fs| read_file_at(fs, path.as_path(), offset, out))
    }

    fn publish_app_install(&mut self, app_id: &str) -> core::result::Result<(), AppStoreError> {
        let source = install_temp_dir(app_id)?;
        let app_dir = app_dir_path(app_id)?;
        let previous = previous_app_dir(app_id)?;
        self.mount(|fs| {
            remove_dir_if_present(fs, previous.as_path())?;
            let had_previous = fs.metadata(app_dir.as_path()).is_ok();
            if had_previous {
                fs.rename(app_dir.as_path(), previous.as_path())?;
            }
            if let Err(error) = fs.rename(source.as_path(), app_dir.as_path()) {
                if had_previous {
                    let _ = fs.rename(previous.as_path(), app_dir.as_path());
                }
                return Err(error);
            }
            remove_dir_if_present(fs, previous.as_path())
        })
    }

    fn abort_app_install(&mut self, app_id: &str) -> core::result::Result<(), AppStoreError> {
        let path = install_temp_dir(app_id)?;
        self.mount(|fs| remove_dir_if_present(fs, path.as_path()))
    }

    fn begin_resource_install(
        &mut self,
        _app_id: &str,
        path: &str,
        _total_len: usize,
    ) -> core::result::Result<(), AppStoreError> {
        validate_resource_path(path)?;
        self.mount(|fs| fs.create_file_and_then(littlefs2::path!("/tmp/resource"), |_| Ok(())))
    }

    fn write_resource_install_chunk(
        &mut self,
        _app_id: &str,
        path: &str,
        offset: usize,
        bytes: &[u8],
    ) -> core::result::Result<(), AppStoreError> {
        validate_resource_path(path)?;
        self.mount(|fs| write_file_at(fs, littlefs2::path!("/tmp/resource"), offset, bytes))
    }

    fn publish_resource_install(
        &mut self,
        app_id: &str,
        path: &str,
    ) -> core::result::Result<(), AppStoreError> {
        let destination = install_resource_path(app_id, path)?;
        let parent = destination.parent().ok_or(AppStoreError::InvalidPath)?;
        self.mount(|fs| {
            fs.create_dir_all(parent.as_path())?;
            fs.rename(littlefs2::path!("/tmp/resource"), destination.as_path())
        })
    }

    fn read_resource_at(
        &mut self,
        app_id: &str,
        path: &str,
        offset: usize,
        out: &mut [u8],
    ) -> core::result::Result<(), AppStoreError> {
        let path = app_resource_path(app_id, path)?;
        self.mount(|fs| read_file_at(fs, path.as_path(), offset, out))
    }

    fn resource_size(
        &mut self,
        app_id: &str,
        path: &str,
    ) -> core::result::Result<usize, AppStoreError> {
        let path = app_resource_path(app_id, path)?;
        self.mount(|fs| fs.metadata(path.as_path()).map(|metadata| metadata.len()))
    }

    fn load_state(
        &mut self,
        app_id: &str,
        out: &mut [u8],
    ) -> core::result::Result<Option<usize>, AppStoreError> {
        let path = state_path(app_id)?;
        self.mount(|fs| {
            let metadata = match fs.metadata(path.as_path()) {
                Ok(metadata) => metadata,
                Err(Error::NO_SUCH_ENTRY) => return Ok(None),
                Err(error) => return Err(error),
            };
            if metadata.len() > out.len() {
                return Err(Error::NO_MEMORY);
            }
            read_file_at(fs, path.as_path(), 0, &mut out[..metadata.len()])?;
            Ok(Some(metadata.len()))
        })
    }

    fn save_state_atomic(
        &mut self,
        app_id: &str,
        bytes: &[u8],
    ) -> core::result::Result<(), AppStoreError> {
        let temporary = state_temp_path(app_id)?;
        let destination = state_path(app_id)?;
        self.mount(|fs| {
            fs.create_file_and_then(temporary.as_path(), |file| {
                file.write_all(bytes)?;
                file.sync()
            })?;
            fs.rename(temporary.as_path(), destination.as_path())
        })
    }

    fn delete_state(&mut self, app_id: &str) -> core::result::Result<(), AppStoreError> {
        let path = state_path(app_id)?;
        self.mount(|fs| match fs.remove(path.as_path()) {
            Ok(()) | Err(Error::NO_SUCH_ENTRY) => Ok(()),
            Err(error) => Err(error),
        })
    }

    fn format(&mut self) -> core::result::Result<(), AppStoreError> {
        Filesystem::format(&mut self.storage).map_err(map_littlefs_error)?;
        self.mount(|fs| ensure_store_dirs(fs))
    }

    fn capacity(&mut self) -> core::result::Result<(usize, usize), AppStoreError> {
        self.mount(|fs| Ok((fs.total_space(), fs.available_space()?)))
    }
}

fn read_file_at<S: littlefs2::driver::Storage>(
    fs: &Filesystem<'_, S>,
    path: &Path,
    offset: usize,
    out: &mut [u8],
) -> Result<()> {
    fs.open_file_and_then(path, |file| {
        file.seek(SeekFrom::Start(
            u32::try_from(offset).map_err(|_| Error::FILE_TOO_BIG)?,
        ))?;
        file.read_exact(out)
    })
}

fn write_file_at<S: littlefs2::driver::Storage>(
    fs: &Filesystem<'_, S>,
    path: &Path,
    offset: usize,
    bytes: &[u8],
) -> Result<()> {
    fs.open_file_with_options_and_then(
        |options| options.read(true).write(true),
        path,
        |file| {
            file.seek(SeekFrom::Start(
                u32::try_from(offset).map_err(|_| Error::FILE_TOO_BIG)?,
            ))?;
            file.write_all(bytes)?;
            file.sync()
        },
    )
}

fn remove_dir_if_present<S: littlefs2::driver::Storage>(
    fs: &Filesystem<'_, S>,
    path: &Path,
) -> Result<()> {
    match fs.remove_dir_all(path) {
        Ok(()) | Err(Error::NO_SUCH_ENTRY) => Ok(()),
        Err(error) => Err(error),
    }
}

impl<F, const OFFSET: usize, const SIZE: usize> Storage for PartitionStorage<F, OFFSET, SIZE>
where
    F: NorFlash + ReadNorFlash,
{
    type CACHE_SIZE = U256;
    type LOOKAHEAD_SIZE = U16;

    const READ_SIZE: usize = F::READ_SIZE;
    const WRITE_SIZE: usize = F::WRITE_SIZE;
    const BLOCK_SIZE: usize = FLASH_ERASE_SIZE;
    const BLOCK_COUNT: usize = SIZE / FLASH_ERASE_SIZE;
    const BLOCK_CYCLES: isize = 500;

    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize> {
        let (start, _) = Self::absolute_range(off, buf.len())?;
        self.flash.read(start, buf).map_err(|_| Error::IO)?;
        Ok(buf.len())
    }

    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize> {
        let (start, _) = Self::absolute_range(off, data.len())?;
        self.flash.write(start, data).map_err(|_| Error::IO)?;
        Ok(data.len())
    }

    fn erase(&mut self, off: usize, len: usize) -> Result<usize> {
        let (start, end) = Self::absolute_range(off, len)?;
        self.flash.erase(start, end).map_err(|_| Error::IO)?;
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use embedded_storage::nor_flash::{ErrorType, NorFlashError, NorFlashErrorKind};
    use std::vec;

    #[derive(Clone, Copy, Debug)]
    struct TestError;

    impl NorFlashError for TestError {
        fn kind(&self) -> NorFlashErrorKind {
            NorFlashErrorKind::Other
        }
    }

    struct RamFlash<const N: usize> {
        bytes: [u8; N],
        touched: Option<(u32, u32)>,
    }

    impl<const N: usize> RamFlash<N> {
        fn new() -> Self {
            Self {
                bytes: [0xff; N],
                touched: None,
            }
        }
    }

    impl<const N: usize> ErrorType for RamFlash<N> {
        type Error = TestError;
    }

    impl<const N: usize> ReadNorFlash for RamFlash<N> {
        const READ_SIZE: usize = 1;

        fn read(&mut self, offset: u32, bytes: &mut [u8]) -> core::result::Result<(), TestError> {
            let start = offset as usize;
            bytes.copy_from_slice(
                self.bytes
                    .get(start..start + bytes.len())
                    .ok_or(TestError)?,
            );
            self.touched = Some((offset, offset + bytes.len() as u32));
            Ok(())
        }

        fn capacity(&self) -> usize {
            N
        }
    }

    impl<const N: usize> NorFlash for RamFlash<N> {
        const WRITE_SIZE: usize = 1;
        const ERASE_SIZE: usize = 4;

        fn write(&mut self, offset: u32, bytes: &[u8]) -> core::result::Result<(), TestError> {
            let start = offset as usize;
            self.bytes
                .get_mut(start..start + bytes.len())
                .ok_or(TestError)?
                .copy_from_slice(bytes);
            self.touched = Some((offset, offset + bytes.len() as u32));
            Ok(())
        }

        fn erase(&mut self, from: u32, to: u32) -> core::result::Result<(), TestError> {
            self.bytes
                .get_mut(from as usize..to as usize)
                .ok_or(TestError)?
                .fill(0xff);
            self.touched = Some((from, to));
            Ok(())
        }
    }

    type TestPartition = PartitionStorage<RamFlash<64>, 16, 32>;

    #[test]
    fn offsets_operations_into_the_partition() {
        let mut storage = TestPartition::new(RamFlash::new());
        assert_eq!(Storage::write(&mut storage, 4, &[1, 2, 3, 4]), Ok(4));
        assert_eq!(storage.flash.touched, Some((20, 24)));

        let mut bytes = [0; 4];
        assert_eq!(Storage::read(&mut storage, 4, &mut bytes), Ok(4));
        assert_eq!(bytes, [1, 2, 3, 4]);
        assert_eq!(storage.flash.touched, Some((20, 24)));
    }

    #[test]
    fn rejects_every_cross_partition_operation_before_touching_flash() {
        let mut storage = TestPartition::new(RamFlash::new());
        assert_eq!(Storage::read(&mut storage, 31, &mut [0; 2]), Err(Error::IO));
        assert_eq!(Storage::write(&mut storage, 32, &[1]), Err(Error::IO));
        assert_eq!(Storage::erase(&mut storage, 28, 8), Err(Error::IO));
        assert_eq!(storage.flash.touched, None);
    }

    #[test]
    fn x4_geometry_exactly_ends_before_coredump() {
        assert_eq!(
            SQUIDSCRIPT_PARTITION_OFFSET + SQUIDSCRIPT_PARTITION_SIZE,
            0xff0000
        );
        assert_eq!(SQUIDSCRIPT_PARTITION_SIZE % FLASH_ERASE_SIZE, 0);
        assert_eq!(SQUIDSCRIPT_PARTITION_SIZE / FLASH_ERASE_SIZE, 2784);
    }

    struct HeapFlash {
        bytes: std::vec::Vec<u8>,
    }

    impl HeapFlash {
        fn new() -> Self {
            Self {
                bytes: vec![0xff; 16 * 1024 * 1024],
            }
        }
    }

    impl ErrorType for HeapFlash {
        type Error = TestError;
    }

    impl ReadNorFlash for HeapFlash {
        const READ_SIZE: usize = 1;
        fn read(&mut self, offset: u32, out: &mut [u8]) -> core::result::Result<(), TestError> {
            let start = offset as usize;
            out.copy_from_slice(self.bytes.get(start..start + out.len()).ok_or(TestError)?);
            Ok(())
        }
        fn capacity(&self) -> usize {
            self.bytes.len()
        }
    }

    impl NorFlash for HeapFlash {
        const WRITE_SIZE: usize = 1;
        const ERASE_SIZE: usize = FLASH_ERASE_SIZE;
        fn write(&mut self, offset: u32, bytes: &[u8]) -> core::result::Result<(), TestError> {
            let start = offset as usize;
            let destination = self
                .bytes
                .get_mut(start..start + bytes.len())
                .ok_or(TestError)?;
            for (destination, source) in destination.iter_mut().zip(bytes) {
                *destination &= *source;
            }
            Ok(())
        }
        fn erase(&mut self, from: u32, to: u32) -> core::result::Result<(), TestError> {
            self.bytes
                .get_mut(from as usize..to as usize)
                .ok_or(TestError)?
                .fill(0xff);
            Ok(())
        }
    }

    #[test]
    fn littlefs_app_storage_persists_apps_resources_and_state_across_remount() {
        let mut storage = LittleFsAppStorage::new(HeapFlash::new());
        storage.initialize().unwrap();
        storage
            .begin_resource_install("reader", "fonts/body.bin", 4)
            .unwrap();
        storage
            .write_resource_install_chunk("reader", "fonts/body.bin", 0, b"font")
            .unwrap();
        storage
            .publish_resource_install("reader", "fonts/body.bin")
            .unwrap();
        assert_eq!(
            storage.resource_size("reader", "fonts/body.bin"),
            Err(AppStoreError::NotFound)
        );
        storage.begin_app_install("reader", 6).unwrap();
        storage
            .write_app_install_chunk("reader", 0, b"sqb")
            .unwrap();
        storage
            .write_app_install_chunk("reader", 3, b"c!!")
            .unwrap();
        storage.publish_app_install("reader").unwrap();
        storage.save_state_atomic("reader", b"state").unwrap();
        let (total, available) = storage.capacity().unwrap();
        assert_eq!(total, SQUIDSCRIPT_PARTITION_SIZE);
        assert!(available < total);

        let flash = storage.into_inner();
        let mut remounted = LittleFsAppStorage::new(flash);
        remounted.initialize().unwrap();
        let mut apps = std::vec::Vec::new();
        remounted
            .for_each_app(&mut |id, size| apps.push((std::string::String::from(id), size)))
            .unwrap();
        assert_eq!(apps, [(std::string::String::from("reader"), 6)]);
        let mut app = [0; 6];
        remounted.read_app_at("reader", 0, &mut app).unwrap();
        assert_eq!(&app, b"sqbc!!");
        let mut resource = [0; 4];
        remounted
            .read_resource_at("reader", "fonts/body.bin", 0, &mut resource)
            .unwrap();
        assert_eq!(&resource, b"font");
        let mut state = [0; 8];
        assert_eq!(remounted.load_state("reader", &mut state).unwrap(), Some(5));
        assert_eq!(&state[..5], b"state");
        remounted.format().unwrap();
        let mut count = 0;
        remounted.for_each_app(&mut |_, _| count += 1).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn initialize_rejects_nonblank_corrupt_storage() {
        let mut flash = HeapFlash::new();
        flash.bytes[SQUIDSCRIPT_PARTITION_OFFSET] = 0;
        let mut storage = LittleFsAppStorage::new(flash);
        assert_eq!(storage.initialize(), Err(AppStoreError::Io));
    }

    #[test]
    fn initialize_restores_interrupted_replacement_and_discards_staging() {
        let mut storage = LittleFsAppStorage::new(HeapFlash::new());
        storage.initialize().unwrap();
        storage.begin_app_install("reader", 3).unwrap();
        storage
            .write_app_install_chunk("reader", 0, b"old")
            .unwrap();
        storage.publish_app_install("reader").unwrap();
        storage
            .begin_resource_install("stale", "data.bin", 3)
            .unwrap();
        storage
            .write_resource_install_chunk("stale", "data.bin", 0, b"new")
            .unwrap();
        storage
            .publish_resource_install("stale", "data.bin")
            .unwrap();
        storage
            .mount(|fs| {
                fs.rename(
                    littlefs2::path!("/apps/reader"),
                    littlefs2::path!("/tmp/previous-reader"),
                )
            })
            .unwrap();

        let flash = storage.into_inner();
        let mut recovered = LittleFsAppStorage::new(flash);
        recovered.initialize().unwrap();
        let mut bytes = [0; 3];
        recovered.read_app_at("reader", 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"old");
        assert_eq!(
            recovered
                .mount(|fs| fs.metadata(littlefs2::path!("/tmp/install-stale")))
                .map(|_| ()),
            Err(AppStoreError::NotFound)
        );
    }
}
