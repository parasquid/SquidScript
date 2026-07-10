use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use littlefs2::{
    consts::{U16, U256},
    driver::Storage,
    io::{Error, Result},
};

pub const SQUIDSCRIPT_PARTITION_OFFSET: usize = 0xc90000;
pub const SQUIDSCRIPT_PARTITION_SIZE: usize = 0x360000;
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
    use super::*;
    use embedded_storage::nor_flash::{ErrorType, NorFlashError, NorFlashErrorKind};

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
        assert_eq!(SQUIDSCRIPT_PARTITION_SIZE / FLASH_ERASE_SIZE, 864);
    }
}
