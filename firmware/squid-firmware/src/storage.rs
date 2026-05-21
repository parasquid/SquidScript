use core::str;

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use littlefs2::{
    consts, driver,
    fs::Filesystem,
    io::{Error, Result, SeekFrom},
    path::PathBuf,
};

use crate::{
    dev_harness::{validate_app_id, AppName, AppStorage, AppStorageError, StoredApp},
    protocol::fnv1a,
};
use squidvm_core::limits::{MAX_APP_BYTES, MAX_SAVED_STATE_BYTES};

pub const SQUIDFS_OFFSET: u32 = 0x210000;
pub const SQUIDFS_LEN: usize = 0x1f0000;
pub const SQUIDFS_BLOCK_SIZE: usize = 4096;
pub const SQUIDFS_BLOCK_COUNT: usize = SQUIDFS_LEN / SQUIDFS_BLOCK_SIZE;

const APPS_DIR: &str = "/apps";
const STATE_DIR: &str = "/state";

pub struct SquidFlashRegion<F> {
    flash: F,
}

impl<F> SquidFlashRegion<F> {
    pub const fn new(flash: F) -> Self {
        Self { flash }
    }
}

impl<F> driver::Storage for SquidFlashRegion<F>
where
    F: ReadNorFlash + NorFlash,
{
    const READ_SIZE: usize = 4;
    const WRITE_SIZE: usize = 4;
    const BLOCK_SIZE: usize = SQUIDFS_BLOCK_SIZE;
    const BLOCK_COUNT: usize = SQUIDFS_BLOCK_COUNT;
    const BLOCK_CYCLES: isize = 500;
    type CACHE_SIZE = consts::U256;
    type LOOKAHEAD_SIZE = consts::U64;

    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize> {
        if off
            .checked_add(buf.len())
            .is_none_or(|end| end > SQUIDFS_LEN)
        {
            return Err(Error::IO);
        }
        self.flash
            .read(SQUIDFS_OFFSET + off as u32, buf)
            .map_err(|_| Error::IO)?;
        Ok(buf.len())
    }

    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize> {
        if off
            .checked_add(data.len())
            .is_none_or(|end| end > SQUIDFS_LEN)
        {
            return Err(Error::NO_SPACE);
        }
        self.flash
            .write(SQUIDFS_OFFSET + off as u32, data)
            .map_err(|_| Error::IO)?;
        Ok(data.len())
    }

    fn erase(&mut self, off: usize, len: usize) -> Result<usize> {
        if off.checked_add(len).is_none_or(|end| end > SQUIDFS_LEN) {
            return Err(Error::NO_SPACE);
        }
        self.flash
            .erase(
                SQUIDFS_OFFSET + off as u32,
                SQUIDFS_OFFSET + off as u32 + len as u32,
            )
            .map_err(|_| Error::IO)?;
        Ok(len)
    }
}

pub struct LittleFsAppStorage<S: driver::Storage> {
    storage: S,
}

impl<S: driver::Storage> LittleFsAppStorage<S> {
    pub const fn new(storage: S) -> Self {
        Self { storage }
    }

    fn is_blank(&mut self) -> bool {
        let mut probe = [0u8; 64];
        self.storage.read(0, &mut probe).is_ok() && probe.iter().all(|byte| *byte == 0xff)
    }

    fn mount<R>(&mut self, f: impl FnOnce(&Filesystem<'_, S>) -> Result<R>) -> Result<R> {
        Filesystem::mount_and_then(&mut self.storage, f)
    }

    fn apps_path(app_id: &str, suffix: &str) -> Result<PathBuf> {
        Self::path_in_dir(APPS_DIR, app_id, suffix)
    }

    fn state_path(app_id: &str, suffix: &str) -> Result<PathBuf> {
        Self::path_in_dir(STATE_DIR, app_id, suffix)
    }

    fn path_in_dir(dir: &str, app_id: &str, suffix: &str) -> Result<PathBuf> {
        validate_app_id(app_id).map_err(|_| Error::INVALID)?;
        let mut bytes = [0u8; 64];
        let mut len = 0usize;
        for part in [dir, "/", app_id, suffix] {
            let part_bytes = part.as_bytes();
            if len + part_bytes.len() >= bytes.len() {
                return Err(Error::FILENAME_TOO_LONG);
            }
            bytes[len..len + part_bytes.len()].copy_from_slice(part_bytes);
            len += part_bytes.len();
        }
        let path = str::from_utf8(&bytes[..len]).map_err(|_| Error::INVALID)?;
        PathBuf::try_from(path).map_err(|_| Error::FILENAME_TOO_LONG)
    }

    fn map_error(error: Error) -> AppStorageError {
        match error {
            Error::NO_SUCH_ENTRY => AppStorageError::NotFound,
            Error::NO_SPACE => AppStorageError::NoSpace,
            Error::INVALID | Error::FILENAME_TOO_LONG => AppStorageError::InvalidName,
            _ => AppStorageError::Io,
        }
    }
}

impl<S: driver::Storage> AppStorage for LittleFsAppStorage<S> {
    fn ensure_ready(&mut self) -> core::result::Result<(), AppStorageError> {
        if Filesystem::is_mountable(&mut self.storage) {
            return self
                .mount(|fs| {
                    fs.create_dir_all(&PathBuf::try_from(APPS_DIR).unwrap())?;
                    fs.create_dir_all(&PathBuf::try_from(STATE_DIR).unwrap())
                })
                .map_err(Self::map_error);
        }
        if !self.is_blank() {
            return Err(AppStorageError::NotMounted);
        }
        self.format()
    }

    fn format(&mut self) -> core::result::Result<(), AppStorageError> {
        Filesystem::format(&mut self.storage).map_err(Self::map_error)?;
        self.mount(|fs| {
            fs.create_dir_all(&PathBuf::try_from(APPS_DIR).unwrap())?;
            fs.create_dir_all(&PathBuf::try_from(STATE_DIR).unwrap())
        })
        .map_err(Self::map_error)
    }

    fn write_app(
        &mut self,
        app_id: &str,
        bytes: &[u8],
    ) -> core::result::Result<(), AppStorageError> {
        if bytes.len() > MAX_APP_BYTES {
            return Err(AppStorageError::NoSpace);
        }
        let tmp = Self::apps_path(app_id, ".tmp").map_err(Self::map_error)?;
        let final_path = Self::apps_path(app_id, ".sqbc").map_err(Self::map_error)?;
        self.mount(|fs| {
            let _ = fs.remove(&tmp);
            fs.write(&tmp, bytes)?;
            let _ = fs.remove(&final_path);
            fs.rename(&tmp, &final_path)
        })
        .map_err(Self::map_error)
    }

    fn read_app(
        &mut self,
        app_id: &str,
        out: &mut [u8],
    ) -> core::result::Result<usize, AppStorageError> {
        let path = Self::apps_path(app_id, ".sqbc").map_err(Self::map_error)?;
        self.mount(|fs| {
            fs.open_file_and_then(&path, |file| {
                let len = file.len()? as usize;
                if len > out.len() {
                    return Err(Error::NO_SPACE);
                }
                let read = file.read(&mut out[..len])?;
                Ok(read)
            })
        })
        .map_err(Self::map_error)
    }

    fn read_app_range(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> core::result::Result<usize, AppStorageError> {
        let path = Self::apps_path(app_id, ".sqbc").map_err(Self::map_error)?;
        self.mount(|fs| {
            fs.open_file_and_then(&path, |file| {
                let len = file.len()? as usize;
                let end = offset.checked_add(out.len()).ok_or(Error::NO_SPACE)?;
                if end > len {
                    return Err(Error::NO_SPACE);
                }
                file.seek(SeekFrom::Start(
                    u32::try_from(offset).map_err(|_| Error::NO_SPACE)?,
                ))?;
                file.read(out)
            })
        })
        .map_err(Self::map_error)
    }

    fn list_apps(
        &mut self,
        out: &mut [StoredApp],
        scratch: &mut [u8],
    ) -> core::result::Result<usize, AppStorageError> {
        let apps_path = PathBuf::try_from(APPS_DIR).unwrap();
        self.mount(|fs| {
            let mut count = 0usize;
            fs.read_dir_and_then(&apps_path, |dir| {
                for entry in dir {
                    let entry = entry?;
                    if count == out.len() || !entry.metadata().is_file() {
                        continue;
                    }
                    let Some(name) = entry.file_name().as_ref().strip_suffix(".sqbc") else {
                        continue;
                    };
                    let Ok(app_name) = AppName::new(name) else {
                        continue;
                    };
                    let len = fs.open_file_and_then(entry.path(), |file| {
                        let len = file.len()? as usize;
                        if len > scratch.len() || len > MAX_APP_BYTES {
                            return Err(Error::NO_SPACE);
                        }
                        file.read(&mut scratch[..len])
                    })?;
                    out[count] = StoredApp {
                        name: app_name,
                        len,
                        hash: fnv1a(&scratch[..len]),
                    };
                    count += 1;
                }
                Ok(count)
            })
        })
        .map_err(Self::map_error)
    }

    fn write_state(
        &mut self,
        app_id: &str,
        bytes: &[u8],
    ) -> core::result::Result<(), AppStorageError> {
        if bytes.len() > MAX_SAVED_STATE_BYTES {
            return Err(AppStorageError::NoSpace);
        }
        let tmp = Self::state_path(app_id, ".tmp").map_err(Self::map_error)?;
        let final_path = Self::state_path(app_id, ".state").map_err(Self::map_error)?;
        self.mount(|fs| {
            let _ = fs.remove(&tmp);
            fs.write(&tmp, bytes)?;
            let _ = fs.remove(&final_path);
            fs.rename(&tmp, &final_path)
        })
        .map_err(Self::map_error)
    }

    fn read_state(
        &mut self,
        app_id: &str,
        out: &mut [u8],
    ) -> core::result::Result<Option<usize>, AppStorageError> {
        let path = Self::state_path(app_id, ".state").map_err(Self::map_error)?;
        self.mount(|fs| {
            fs.open_file_and_then(&path, |file| {
                let len = file.len()? as usize;
                if len > out.len() {
                    return Err(Error::NO_SPACE);
                }
                let read = file.read(&mut out[..len])?;
                Ok(Some(read))
            })
        })
        .or_else(|error| {
            if matches!(error, Error::NO_SUCH_ENTRY) {
                Ok(None)
            } else {
                Err(error)
            }
        })
        .map_err(Self::map_error)
    }

    fn delete_state(&mut self, app_id: &str) -> core::result::Result<(), AppStorageError> {
        let path = Self::state_path(app_id, ".state").map_err(Self::map_error)?;
        self.mount(|fs| match fs.remove(&path) {
            Ok(()) | Err(Error::NO_SUCH_ENTRY) => Ok(()),
            Err(error) => Err(error),
        })
        .map_err(Self::map_error)
    }
}
