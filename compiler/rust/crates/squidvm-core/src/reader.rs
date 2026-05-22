use crate::{error::VmError, host::StorageRequest, host::TraceSink};

pub trait SqbcReader {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError>;

    fn storage_request_at(
        &mut self,
        _offset: usize,
        _len: usize,
    ) -> Result<Option<StorageRequest>, VmError> {
        Ok(None)
    }
}

pub trait ChunkedVmHost: SqbcReader + TraceSink {}

impl<T: SqbcReader + TraceSink> ChunkedVmHost for T {}

pub struct SliceSqbcReader<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceSqbcReader<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl SqbcReader for SliceSqbcReader<'_> {
    fn read_exact_at(&mut self, offset: usize, out: &mut [u8]) -> Result<(), VmError> {
        let end = offset
            .checked_add(out.len())
            .ok_or(VmError::InvalidSection)?;
        let bytes = self.bytes.get(offset..end).ok_or(VmError::InvalidSection)?;
        out.copy_from_slice(bytes);
        Ok(())
    }
}
