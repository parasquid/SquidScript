#![cfg_attr(not(test), no_std)]

pub mod ble_pipeline;
#[cfg(feature = "x4-flash-filesystem")]
pub mod flash_partition;
pub mod ota;
pub mod target_config;
pub mod target_input;

pub mod radio_probe {
    use core::fmt;

    use squidscript_fw_core::radio_lifecycle::{
        evaluate_reusable_reclaim, format_reclaim_summary, CycleSnapshot, RadioKind, ReclaimGate,
        ReclaimSummary,
    };

    pub const REUSABLE_RECLAIM_GATE: ReclaimGate = ReclaimGate {
        min_absolute_reclaim_bytes: 4 * 1024,
        max_unreclaimed_ratio_per_mille: 100,
        warmup_cycle_count: 1,
    };
    pub const ESP_RADIO_VERSION: &str = "1.0.0-beta.0";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct RadioStackMetadata {
        pub stack: &'static str,
        pub version: &'static str,
        pub features: &'static [&'static str],
    }

    pub const fn radio_stack_metadata() -> RadioStackMetadata {
        RadioStackMetadata {
            stack: "esp-radio",
            version: ESP_RADIO_VERSION,
            features: &["esp32c3", "wifi", "ble", "unstable"],
        }
    }

    pub trait RadioCycleRunner {
        type Error;

        fn run_cycle(&mut self, radio: RadioKind) -> Result<CycleSnapshot, Self::Error>;
    }

    pub fn run_probe_cycles<R: RadioCycleRunner>(
        radio: RadioKind,
        runner: &mut R,
        snapshots: &mut [CycleSnapshot],
        serial_line: &mut dyn fmt::Write,
    ) -> Result<ReclaimSummary, R::Error> {
        for snapshot in snapshots.iter_mut() {
            *snapshot = runner.run_cycle(radio)?;
        }
        let summary = evaluate_reusable_reclaim(radio, snapshots, REUSABLE_RECLAIM_GATE);
        let _ = format_reclaim_summary(&summary, serial_line);
        Ok(summary)
    }
}

pub mod board {
    use embedded_hal::{
        digital::OutputPin,
        spi::{ErrorType, Operation, SpiBus, SpiDevice},
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum BoardError {
        Gpio,
        Spi,
    }

    impl embedded_hal::spi::Error for BoardError {
        fn kind(&self) -> embedded_hal::spi::ErrorKind {
            embedded_hal::spi::ErrorKind::Other
        }
    }

    pub struct BoardSpiDevice<BUS, CS> {
        bus: BUS,
        chip_select: CS,
    }

    impl<BUS, CS> BoardSpiDevice<BUS, CS> {
        pub fn new(bus: BUS, chip_select: CS) -> Self {
            Self { bus, chip_select }
        }
    }

    impl<BUS, CS> ErrorType for BoardSpiDevice<BUS, CS>
    where
        BUS: SpiBus<u8>,
        CS: OutputPin,
    {
        type Error = BoardError;
    }

    impl<BUS, CS> SpiDevice<u8> for BoardSpiDevice<BUS, CS>
    where
        BUS: SpiBus<u8>,
        CS: OutputPin,
    {
        fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
            self.chip_select.set_low().map_err(|_| BoardError::Gpio)?;
            let result = operations
                .iter_mut()
                .try_for_each(|operation| match operation {
                    Operation::Read(buffer) => self.bus.read(buffer).map_err(|_| BoardError::Spi),
                    Operation::Write(buffer) => self.bus.write(buffer).map_err(|_| BoardError::Spi),
                    Operation::Transfer(read, write) => {
                        self.bus.transfer(read, write).map_err(|_| BoardError::Spi)
                    }
                    Operation::TransferInPlace(buffer) => self
                        .bus
                        .transfer_in_place(buffer)
                        .map_err(|_| BoardError::Spi),
                    Operation::DelayNs(_) => Ok(()),
                })
                .and_then(|()| self.bus.flush().map_err(|_| BoardError::Spi));
            let deselect = self.chip_select.set_high().map_err(|_| BoardError::Gpio);
            result.and(deselect)
        }
    }

    #[cfg(all(feature = "firmware-bin", target_arch = "riscv32"))]
    pub struct DisplayDelay;

    #[cfg(all(feature = "firmware-bin", target_arch = "riscv32"))]
    impl embedded_hal_async::delay::DelayNs for DisplayDelay {
        async fn delay_ns(&mut self, ns: u32) {
            embassy_time::Timer::after_nanos(u64::from(ns)).await;
        }
    }

    #[cfg(all(feature = "firmware-bin", target_arch = "riscv32"))]
    impl embedded_hal::delay::DelayNs for DisplayDelay {
        fn delay_ns(&mut self, ns: u32) {
            embassy_time::block_for(embassy_time::Duration::from_nanos(u64::from(ns)));
        }
    }

    #[cfg(all(feature = "firmware-bin", target_arch = "riscv32"))]
    pub use shared_spi::{FreqManagedSpiDevice, SharedSpi2};

    #[cfg(all(feature = "firmware-bin", target_arch = "riscv32"))]
    mod shared_spi {
        use core::cell::RefCell;

        use embedded_hal::{
            digital::OutputPin,
            spi::{ErrorType, Operation, SpiBus, SpiDevice},
        };
        use esp_hal::{
            spi::{
                master::{Config as SpiConfig, Spi},
                Mode,
            },
            time::Rate,
            Blocking,
        };

        use super::BoardError;

        pub struct SharedSpi2 {
            bus: RefCell<Spi<'static, Blocking>>,
        }

        impl SharedSpi2 {
            pub fn new(
                spi2: esp_hal::peripherals::SPI2<'static>,
                gpio8: esp_hal::peripherals::GPIO8<'static>,
                gpio10: esp_hal::peripherals::GPIO10<'static>,
                gpio7: esp_hal::peripherals::GPIO7<'static>,
            ) -> Self {
                let spi = Spi::new(
                    spi2,
                    SpiConfig::default()
                        .with_frequency(Rate::from_mhz(20))
                        .with_mode(Mode::_0),
                )
                .expect("SPI2 init")
                .with_sck(gpio8)
                .with_mosi(gpio10)
                .with_miso(gpio7);
                Self {
                    bus: RefCell::new(spi),
                }
            }
        }

        pub struct FreqManagedSpiDevice<'a, CS: OutputPin> {
            shared: &'a SharedSpi2,
            cs: CS,
            freq_hz: u32,
        }

        impl<'a, CS: OutputPin> FreqManagedSpiDevice<'a, CS> {
            pub fn new(shared: &'a SharedSpi2, cs: CS, freq_hz: u32) -> Self {
                Self {
                    shared,
                    cs,
                    freq_hz,
                }
            }
        }

        impl<'a, CS: OutputPin> ErrorType for FreqManagedSpiDevice<'a, CS> {
            type Error = BoardError;
        }

        impl<'a, CS: OutputPin> SpiDevice<u8> for FreqManagedSpiDevice<'a, CS> {
            fn transaction(
                &mut self,
                operations: &mut [Operation<'_, u8>],
            ) -> Result<(), Self::Error> {
                let mut bus = self.shared.bus.borrow_mut();
                bus.apply_config(
                    &SpiConfig::default()
                        .with_frequency(Rate::from_hz(self.freq_hz))
                        .with_mode(Mode::_0),
                )
                .map_err(|_| BoardError::Spi)?;
                self.cs.set_low().map_err(|_| BoardError::Gpio)?;
                let result = operations
                    .iter_mut()
                    .try_for_each(|operation| match operation {
                        Operation::Read(buffer) => {
                            SpiBus::read(&mut *bus, buffer).map_err(|_| BoardError::Spi)
                        }
                        Operation::Write(buffer) => {
                            SpiBus::write(&mut *bus, buffer).map_err(|_| BoardError::Spi)
                        }
                        Operation::Transfer(read, write) => {
                            SpiBus::transfer(&mut *bus, read, write).map_err(|_| BoardError::Spi)
                        }
                        Operation::TransferInPlace(buffer) => {
                            SpiBus::transfer_in_place(&mut *bus, buffer)
                                .map_err(|_| BoardError::Spi)
                        }
                        Operation::DelayNs(_) => Ok(()),
                    })
                    .and_then(|()| SpiBus::flush(&mut *bus).map_err(|_| BoardError::Spi));
                let _ = self.cs.set_high();
                result
            }
        }
    }
}

#[cfg(test)]
mod board_tests {
    use core::convert::Infallible;
    use std::{cell::RefCell, rc::Rc};

    use embedded_hal::{
        digital::{ErrorType as DigitalErrorType, OutputPin},
        spi::{ErrorType as SpiErrorType, Operation, SpiBus, SpiDevice},
    };

    use crate::board::BoardSpiDevice;

    #[derive(Clone, Default)]
    struct Trace(Rc<RefCell<Vec<&'static str>>>);

    struct Bus(Trace);

    impl SpiErrorType for Bus {
        type Error = Infallible;
    }

    impl SpiBus<u8> for Bus {
        fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            self.0 .0.borrow_mut().push("read");
            words.fill(0x5a);
            Ok(())
        }

        fn write(&mut self, _: &[u8]) -> Result<(), Self::Error> {
            self.0 .0.borrow_mut().push("write");
            Ok(())
        }

        fn transfer(&mut self, read: &mut [u8], _: &[u8]) -> Result<(), Self::Error> {
            self.0 .0.borrow_mut().push("transfer");
            read.fill(0xa5);
            Ok(())
        }

        fn transfer_in_place(&mut self, _: &mut [u8]) -> Result<(), Self::Error> {
            self.0 .0.borrow_mut().push("transfer-in-place");
            Ok(())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            self.0 .0.borrow_mut().push("flush");
            Ok(())
        }
    }

    struct ChipSelect(Trace);

    impl DigitalErrorType for ChipSelect {
        type Error = Infallible;
    }

    impl OutputPin for ChipSelect {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.0 .0.borrow_mut().push("select");
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.0 .0.borrow_mut().push("deselect");
            Ok(())
        }
    }

    #[test]
    fn board_spi_device_owns_chip_select_for_whole_transaction() {
        let trace = Trace::default();
        let mut device = BoardSpiDevice::new(Bus(trace.clone()), ChipSelect(trace.clone()));
        let mut read = [0_u8; 2];

        device
            .transaction(&mut [Operation::Write(&[1, 2]), Operation::Read(&mut read)])
            .unwrap();

        assert_eq!(
            &*trace.0.borrow(),
            &["select", "write", "read", "flush", "deselect"]
        );
        assert_eq!(read, [0x5a; 2]);
    }
}

#[cfg(feature = "x4-storage")]
pub mod http_upload {
    const ROUTE_PREFIX: &str = "/upload/";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Method {
        Head,
        Put,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ContentRange {
        pub start: usize,
        pub end: usize,
        pub total: usize,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Request<'a> {
        pub method: Method,
        pub name: &'a str,
        pub content_length: usize,
        pub content_range: Option<ContentRange>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ParseError {
        Incomplete,
        Invalid,
    }

    pub fn header_end(bytes: &[u8]) -> Option<usize> {
        bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
    }

    pub fn parse_request(headers: &str) -> Result<Request<'_>, ParseError> {
        let header_len = header_end(headers.as_bytes()).ok_or(ParseError::Incomplete)?;
        let mut lines = headers[..header_len].split("\r\n");
        let mut request_line = lines.next().ok_or(ParseError::Invalid)?.split(' ');
        let method = match request_line.next() {
            Some("HEAD") => Method::Head,
            Some("PUT") => Method::Put,
            _ => return Err(ParseError::Invalid),
        };
        let path = request_line.next().ok_or(ParseError::Invalid)?;
        let version = request_line.next().ok_or(ParseError::Invalid)?;
        if request_line.next().is_some() || !version.starts_with("HTTP/") {
            return Err(ParseError::Invalid);
        }
        let name = path.strip_prefix(ROUTE_PREFIX).ok_or(ParseError::Invalid)?;
        if !safe_name(name) {
            return Err(ParseError::Invalid);
        }

        let mut content_length = None;
        let mut content_range = None;
        for line in lines {
            if line.is_empty() {
                break;
            }
            let Some((key, value)) = line.split_once(':') else {
                return Err(ParseError::Invalid);
            };
            let value = value.trim_matches([' ', '\t']);
            if key.eq_ignore_ascii_case("Content-Length") {
                if content_length.is_some() {
                    return Err(ParseError::Invalid);
                }
                content_length = Some(parse_usize(value)?);
            } else if key.eq_ignore_ascii_case("Content-Range") {
                if content_range.is_some() {
                    return Err(ParseError::Invalid);
                }
                content_range = Some(parse_content_range(value)?);
            }
        }

        let content_length = match method {
            Method::Head => 0,
            Method::Put => content_length
                .filter(|length| *length > 0)
                .ok_or(ParseError::Invalid)?,
        };
        if let Some(range) = content_range {
            let range_len = range
                .end
                .checked_sub(range.start)
                .and_then(|length| length.checked_add(1))
                .ok_or(ParseError::Invalid)?;
            if method != Method::Put || range_len != content_length {
                return Err(ParseError::Invalid);
            }
        }
        Ok(Request {
            method,
            name,
            content_length,
            content_range,
        })
    }

    fn safe_name(name: &str) -> bool {
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && !name.contains('\\')
            && !name.contains(':')
    }

    fn parse_usize(value: &str) -> Result<usize, ParseError> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ParseError::Invalid);
        }
        value.parse().map_err(|_| ParseError::Invalid)
    }

    fn parse_content_range(value: &str) -> Result<ContentRange, ParseError> {
        let value = value.strip_prefix("bytes ").ok_or(ParseError::Invalid)?;
        let (bounds, total) = value.split_once('/').ok_or(ParseError::Invalid)?;
        let (start, end) = bounds.split_once('-').ok_or(ParseError::Invalid)?;
        let range = ContentRange {
            start: parse_usize(start)?,
            end: parse_usize(end)?,
            total: parse_usize(total)?,
        };
        if range.total == 0 || range.end < range.start || range.end >= range.total {
            return Err(ParseError::Invalid);
        }
        Ok(range)
    }

    #[cfg(test)]
    mod tests {
        use super::{header_end, parse_request, ContentRange, Method, ParseError, Request};

        #[test]
        fn parses_head_and_put_upload_requests() {
            assert_eq!(
                parse_request("HEAD /upload/book.binbook HTTP/1.1\r\nHost: device\r\n\r\n"),
                Ok(Request {
                    method: Method::Head,
                    name: "book.binbook",
                    content_length: 0,
                    content_range: None,
                })
            );
            assert_eq!(
                parse_request(
                    "PUT /upload/book.binbook HTTP/1.1\r\ncontent-length: 4\r\nContent-Range: bytes 6-9/10\r\n\r\n"
                ),
                Ok(Request {
                    method: Method::Put,
                    name: "book.binbook",
                    content_length: 4,
                    content_range: Some(ContentRange {
                        start: 6,
                        end: 9,
                        total: 10,
                    }),
                })
            );
        }

        #[test]
        fn rejects_unsafe_names_and_invalid_ranges() {
            for request in [
                "PUT /upload/../book HTTP/1.1\r\nContent-Length: 1\r\n\r\n",
                "PUT /upload/book HTTP/1.1\r\nContent-Length: 3\r\nContent-Range: bytes 2-3/4\r\n\r\n",
                "PUT /upload/book HTTP/1.1\r\nContent-Length: 1\r\nContent-Range: bytes 4-4/4\r\n\r\n",
            ] {
                assert_eq!(parse_request(request), Err(ParseError::Invalid));
            }
        }

        #[test]
        fn finds_headers_when_body_arrives_in_same_read() {
            let bytes = b"PUT /upload/book HTTP/1.1\r\nContent-Length: 2\r\n\r\nok";
            assert_eq!(header_end(bytes), Some(bytes.len() - 2));
        }
    }
}

pub mod x4_storage {
    use embedded_sd_storage::{sd_filesystem::StorageError, SdStorage};
    use embedded_sdmmc::{BlockDevice, TimeSource, Timestamp};
    use squidscript_fw_core::native_runtime::{
        BoundedNativeFileBackend, NativeFileStorage, NativeFileStorageError,
    };
    #[cfg(feature = "x4-binbook")]
    use {
        binbook_core::{Book, Error as BinBookError, PixelFormat, PlaneSlot, ReadAt},
        embedded_hal::{
            delay::DelayNs,
            digital::{InputPin, OutputPin},
            spi::SpiDevice,
        },
        squidscript_fw_core::native_runtime::NativeContentCheckResult,
        squidscript_fw_core::native_runtime::NativeFileBackend,
        squidvm_core::{
            error::VmError,
            host::{
                BinBookChapterEntry, BinBookChapterListSummary, BinBookChapterListWriter,
                BinBookChapterResult, BinBookInfoResult, BinBookOpenResult, BinBookReadPageResult,
                ContentBinBookListSummary, ContentBinBookListWriter, FileCopyResult,
                FileListSummary, FileListWriter, FilePickFileResult, FileReadLinesResult,
                FileReadLinesSummary, FileReadLinesWriter, FileReadTextResult,
            },
            value::{Handle, HandleKind},
        },
        ssd1677_driver::RefreshMode,
        xteink_x4_display::{
            buffers::RenderBuffers,
            page_source::{read_x4_page, PlaneDecoder},
            panel::X4Panel,
            profile::{PHYSICAL_HEIGHT, PHYSICAL_WIDTH, ROW_BYTES},
            DisplayError,
        },
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct X4StorageTime;

    impl TimeSource for X4StorageTime {
        fn get_timestamp(&self) -> Timestamp {
            Timestamp::from_calendar(2026, 1, 1, 0, 0, 0).unwrap_or(Timestamp {
                year_since_1970: 56,
                zero_indexed_month: 0,
                zero_indexed_day: 0,
                hours: 0,
                minutes: 0,
                seconds: 0,
            })
        }
    }

    pub trait FlatSdStorage {
        fn flat_for_each_file(
            &mut self,
            visit: &mut dyn FnMut(&str, u64),
        ) -> Result<(), NativeFileStorageError>;

        fn flat_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError>;

        fn flat_read_at(
            &mut self,
            name: &str,
            offset: u64,
            out: &mut [u8],
        ) -> Result<(), NativeFileStorageError>;

        fn flat_create_or_truncate(&mut self, name: &str) -> Result<(), NativeFileStorageError>;

        fn flat_begin_write(
            &mut self,
            name: &str,
            _expected_size: u64,
        ) -> Result<(), NativeFileStorageError> {
            self.flat_create_or_truncate(name)
        }

        fn flat_write_at(
            &mut self,
            name: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError>;

        fn flat_write_chunk(
            &mut self,
            name: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            self.flat_write_at(name, offset, data)
        }

        fn flat_flush(&mut self, name: &str) -> Result<(), NativeFileStorageError>;

        fn flat_commit_write(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            self.flat_flush(name)
        }

        fn flat_delete(&mut self, name: &str) -> Result<(), NativeFileStorageError>;

        fn tmp_file_size(&mut self, _name: &str) -> Result<u64, NativeFileStorageError> {
            Err(NativeFileStorageError::NotFound)
        }

        fn tmp_read_at(
            &mut self,
            _name: &str,
            _offset: u64,
            _out: &mut [u8],
        ) -> Result<(), NativeFileStorageError> {
            Err(NativeFileStorageError::NotFound)
        }

        fn tmp_create_or_truncate(&mut self, _name: &str) -> Result<(), NativeFileStorageError> {
            Err(NativeFileStorageError::Io)
        }

        fn tmp_begin_write(
            &mut self,
            name: &str,
            _expected_size: u64,
        ) -> Result<(), NativeFileStorageError> {
            self.tmp_create_or_truncate(name)
        }

        fn tmp_write_at(
            &mut self,
            _name: &str,
            _offset: u64,
            _data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            Err(NativeFileStorageError::Io)
        }

        fn tmp_write_chunk(
            &mut self,
            name: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            self.tmp_write_at(name, offset, data)
        }

        fn tmp_flush(&mut self, _name: &str) -> Result<(), NativeFileStorageError> {
            Ok(())
        }

        fn tmp_commit_write(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            self.tmp_flush(name)
        }

        fn tmp_delete(&mut self, _name: &str) -> Result<(), NativeFileStorageError> {
            Err(NativeFileStorageError::NotFound)
        }

        fn copy_tmp_to_flat(
            &mut self,
            _source_name: &str,
            _destination_name: &str,
            _scratch: &mut [u8],
        ) -> Result<Option<u64>, NativeFileStorageError> {
            Ok(None)
        }

        fn flat_format(&mut self) -> Result<(), NativeFileStorageError> {
            const MAX_FORMAT_DELETE_STEPS: usize = 1024;

            for _ in 0..MAX_FORMAT_DELETE_STEPS {
                let mut first_name = heapless::String::<256>::new();
                self.flat_for_each_file(&mut |name, _| {
                    if first_name.is_empty() {
                        let _ = first_name.push_str(name);
                    }
                })?;
                if first_name.is_empty() {
                    return Ok(());
                }
                self.flat_delete(first_name.as_str())?;
            }

            Err(NativeFileStorageError::Io)
        }
    }

    pub struct X4SdFileStorage<S> {
        storage: S,
    }

    impl<S> X4SdFileStorage<S> {
        pub const fn new(storage: S) -> Self {
            Self { storage }
        }

        pub const fn storage(&self) -> &S {
            &self.storage
        }

        pub fn storage_mut(&mut self) -> &mut S {
            &mut self.storage
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ContentWriteVolume {
        Sd,
        Internal,
    }

    pub struct X4ContentStorage<SD, INTERNAL> {
        sd: SD,
        internal: INTERNAL,
        sd_missing: bool,
        selected_read: Option<ContentWriteVolume>,
        active_write: Option<ContentWriteVolume>,
    }

    impl<SD, INTERNAL> X4ContentStorage<SD, INTERNAL> {
        pub const fn new(sd: SD, internal: INTERNAL) -> Self {
            Self {
                sd,
                internal,
                sd_missing: false,
                selected_read: None,
                active_write: None,
            }
        }

        fn note_sd_result<T>(
            &mut self,
            result: Result<T, NativeFileStorageError>,
        ) -> Result<T, NativeFileStorageError> {
            if matches!(result, Err(NativeFileStorageError::VolumeMissing)) {
                self.sd_missing = true;
            }
            result
        }
    }

    impl<SD, INTERNAL> NativeFileStorage for X4ContentStorage<SD, INTERNAL>
    where
        SD: NativeFileStorage,
        INTERNAL: NativeFileStorage,
    {
        fn for_each_file(
            &mut self,
            visit: &mut dyn FnMut(&str, u64),
        ) -> Result<(), NativeFileStorageError> {
            let sd_result = if self.sd_missing {
                Err(NativeFileStorageError::VolumeMissing)
            } else {
                let result = self.sd.for_each_file(visit);
                self.note_sd_result(result)
            };
            match sd_result {
                Ok(()) | Err(NativeFileStorageError::VolumeMissing) => {}
                Err(error) => return Err(error),
            }
            let sd_missing = self.sd_missing;
            let sd = &mut self.sd;
            let mut probe_error = None;
            let mut volume_missing = false;
            self.internal.for_each_file(&mut |path, size| {
                if probe_error.is_some() {
                    return;
                }
                let result = if sd_missing {
                    Err(NativeFileStorageError::VolumeMissing)
                } else {
                    sd.file_size(path)
                };
                match result {
                    Ok(_) => {}
                    Err(NativeFileStorageError::NotFound) => visit(path, size),
                    Err(NativeFileStorageError::VolumeMissing) => {
                        volume_missing = true;
                        visit(path, size);
                    }
                    Err(error) => probe_error = Some(error),
                }
            })?;
            if volume_missing {
                self.sd_missing = true;
            }
            probe_error.map_or(Ok(()), Err)
        }

        fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError> {
            self.selected_read = None;
            let result = if self.sd_missing {
                Err(NativeFileStorageError::VolumeMissing)
            } else {
                let result = self.sd.file_size(path);
                self.note_sd_result(result)
            };
            match result {
                Ok(size) => {
                    self.selected_read = Some(ContentWriteVolume::Sd);
                    Ok(size)
                }
                Err(NativeFileStorageError::NotFound)
                | Err(NativeFileStorageError::VolumeMissing) => {
                    let size = self.internal.file_size(path)?;
                    self.selected_read = Some(ContentWriteVolume::Internal);
                    Ok(size)
                }
                Err(error) => Err(error),
            }
        }

        fn read_at(
            &mut self,
            path: &str,
            offset: u64,
            out: &mut [u8],
        ) -> Result<(), NativeFileStorageError> {
            match self.selected_read {
                Some(ContentWriteVolume::Sd) => return self.sd.read_at(path, offset, out),
                Some(ContentWriteVolume::Internal) => {
                    return self.internal.read_at(path, offset, out)
                }
                None => {}
            }
            let result = if self.sd_missing {
                Err(NativeFileStorageError::VolumeMissing)
            } else {
                let result = self.sd.read_at(path, offset, out);
                self.note_sd_result(result)
            };
            match result {
                Ok(()) => Ok(()),
                Err(NativeFileStorageError::NotFound)
                | Err(NativeFileStorageError::VolumeMissing) => {
                    let result = self.internal.read_at(path, offset, out);
                    if result.is_ok() {
                        self.selected_read = Some(ContentWriteVolume::Internal);
                    }
                    result
                }
                Err(error) => Err(error),
            }
        }

        fn create_or_truncate(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            let result = if self.sd_missing {
                Err(NativeFileStorageError::VolumeMissing)
            } else {
                let result = self.sd.create_or_truncate(path);
                self.note_sd_result(result)
            };
            match result {
                Ok(()) => {
                    self.active_write = Some(ContentWriteVolume::Sd);
                    Ok(())
                }
                Err(NativeFileStorageError::VolumeMissing) => {
                    self.internal.create_or_truncate(path)?;
                    self.active_write = Some(ContentWriteVolume::Internal);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }

        fn begin_write(
            &mut self,
            path: &str,
            expected_size: u64,
        ) -> Result<(), NativeFileStorageError> {
            let result = if self.sd_missing {
                Err(NativeFileStorageError::VolumeMissing)
            } else {
                let result = self.sd.begin_write(path, expected_size);
                self.note_sd_result(result)
            };
            match result {
                Ok(()) => {
                    self.active_write = Some(ContentWriteVolume::Sd);
                    Ok(())
                }
                Err(NativeFileStorageError::VolumeMissing) => {
                    self.internal.begin_write(path, expected_size)?;
                    self.active_write = Some(ContentWriteVolume::Internal);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }

        fn write_at(
            &mut self,
            path: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            match self.active_write {
                Some(ContentWriteVolume::Sd) => self.sd.write_at(path, offset, data),
                Some(ContentWriteVolume::Internal) => self.internal.write_at(path, offset, data),
                None => Err(NativeFileStorageError::Io),
            }
        }

        fn write_chunk(
            &mut self,
            path: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            match self.active_write {
                Some(ContentWriteVolume::Sd) => self.sd.write_chunk(path, offset, data),
                Some(ContentWriteVolume::Internal) => self.internal.write_chunk(path, offset, data),
                None => Err(NativeFileStorageError::Io),
            }
        }

        fn flush(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            let result = match self.active_write {
                Some(ContentWriteVolume::Sd) => self.sd.flush(path),
                Some(ContentWriteVolume::Internal) => self.internal.flush(path),
                None => Err(NativeFileStorageError::Io),
            };
            if result.is_ok() {
                self.active_write = None;
            }
            result
        }

        fn commit_write(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            let result = match self.active_write {
                Some(ContentWriteVolume::Sd) => self.sd.commit_write(path),
                Some(ContentWriteVolume::Internal) => self.internal.commit_write(path),
                None => Err(NativeFileStorageError::Io),
            };
            if result.is_ok() {
                self.active_write = None;
            }
            result
        }

        fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            let sd = if self.sd_missing {
                Err(NativeFileStorageError::VolumeMissing)
            } else {
                let result = self.sd.delete(path);
                self.note_sd_result(result)
            };
            let internal = self.internal.delete(path);
            match (sd, internal) {
                (Ok(()), _) | (_, Ok(())) => Ok(()),
                (Err(NativeFileStorageError::VolumeMissing), error)
                | (Err(NativeFileStorageError::NotFound), error) => error,
                (Err(error), _) => Err(error),
            }
        }

        fn format(&mut self) -> Result<(), NativeFileStorageError> {
            Ok(())
        }

        fn copy_file(
            &mut self,
            source: &str,
            destination: &str,
            scratch: &mut [u8],
        ) -> Result<Option<u64>, NativeFileStorageError> {
            let result = if self.sd_missing {
                Err(NativeFileStorageError::VolumeMissing)
            } else {
                let result = self.sd.copy_file(source, destination, scratch);
                self.note_sd_result(result)
            };
            match result {
                Ok(result) => Ok(result),
                Err(NativeFileStorageError::NotFound)
                | Err(NativeFileStorageError::VolumeMissing) => {
                    self.internal.copy_file(source, destination, scratch)
                }
                Err(error) => Err(error),
            }
        }
    }

    #[cfg(feature = "x4-binbook")]
    pub struct X4BinBookFileBackend<
        S,
        const TEXT_BYTES: usize,
        const LINE_COUNT: usize,
        const LINE_BYTES: usize,
        const SECTION_SCRATCH_BYTES: usize,
        const TITLE_BYTES: usize,
        const HANDLE_COUNT: usize,
    > {
        files: BoundedNativeFileBackend<S, TEXT_BYTES, LINE_COUNT, LINE_BYTES>,
        handle_paths: [[u8; TEXT_BYTES]; HANDLE_COUNT],
        handle_lens: [usize; HANDLE_COUNT],
        drawable_books: [u16; HANDLE_COUNT],
        drawable_pages: [u32; HANDLE_COUNT],
        drawable_active: [bool; HANDLE_COUNT],
        title: [u8; TITLE_BYTES],
        section_scratch: [u8; SECTION_SCRATCH_BYTES],
        metadata_scratch: [u8; 64],
    }

    #[cfg(feature = "x4-binbook")]
    impl<
            S,
            const TEXT_BYTES: usize,
            const LINE_COUNT: usize,
            const LINE_BYTES: usize,
            const SECTION_SCRATCH_BYTES: usize,
            const TITLE_BYTES: usize,
            const HANDLE_COUNT: usize,
        >
        X4BinBookFileBackend<
            S,
            TEXT_BYTES,
            LINE_COUNT,
            LINE_BYTES,
            SECTION_SCRATCH_BYTES,
            TITLE_BYTES,
            HANDLE_COUNT,
        >
    {
        pub const fn new(storage: S) -> Self {
            Self {
                files: BoundedNativeFileBackend::new(storage),
                handle_paths: [[0; TEXT_BYTES]; HANDLE_COUNT],
                handle_lens: [0; HANDLE_COUNT],
                drawable_books: [0; HANDLE_COUNT],
                drawable_pages: [0; HANDLE_COUNT],
                drawable_active: [false; HANDLE_COUNT],
                title: [0; TITLE_BYTES],
                section_scratch: [0; SECTION_SCRATCH_BYTES],
                metadata_scratch: [0; 64],
            }
        }

        pub const fn files(
            &self,
        ) -> &BoundedNativeFileBackend<S, TEXT_BYTES, LINE_COUNT, LINE_BYTES> {
            &self.files
        }

        pub fn files_mut(
            &mut self,
        ) -> &mut BoundedNativeFileBackend<S, TEXT_BYTES, LINE_COUNT, LINE_BYTES> {
            &mut self.files
        }
    }

    #[cfg(feature = "x4-binbook")]
    impl<
            S,
            const TEXT_BYTES: usize,
            const LINE_COUNT: usize,
            const LINE_BYTES: usize,
            const SECTION_SCRATCH_BYTES: usize,
            const TITLE_BYTES: usize,
            const HANDLE_COUNT: usize,
        >
        X4BinBookFileBackend<
            S,
            TEXT_BYTES,
            LINE_COUNT,
            LINE_BYTES,
            SECTION_SCRATCH_BYTES,
            TITLE_BYTES,
            HANDLE_COUNT,
        >
    where
        S: NativeFileStorage,
    {
        fn normalize_binbook_path<'a>(
            path: &'a str,
            mapped: &'a mut heapless::String<TEXT_BYTES>,
        ) -> Result<&'a str, &'static str> {
            let content_name = path
                .strip_prefix("content:books/r/")
                .or_else(|| path.strip_prefix("content:books/p/"));
            let Some(name) = content_name else {
                return Ok(path);
            };
            mapped.push_str("books/").map_err(|_| "too-large")?;
            mapped.push_str(name).map_err(|_| "too-large")?;
            Ok(mapped.as_str())
        }

        fn store_handle(&mut self, path: &str) -> Result<Handle, &'static str> {
            let bytes = path.as_bytes();
            if bytes.len() > TEXT_BYTES {
                return Err("too-large");
            }
            let slot = self
                .handle_lens
                .iter()
                .position(|len| *len == 0)
                .ok_or("too-many-open")?;
            self.handle_paths[slot][..bytes.len()].copy_from_slice(bytes);
            self.handle_lens[slot] = bytes.len();
            let slot = u16::try_from(slot).map_err(|_| "too-many-open")?;
            Ok(Handle::new(HandleKind::BinBook, slot))
        }

        fn copy_handle_path(
            &self,
            book: Handle,
            out: &mut [u8; TEXT_BYTES],
        ) -> Result<usize, &'static str> {
            if book.kind != HandleKind::BinBook {
                return Err("invalid-handle");
            }
            let slot = usize::from(book.id);
            let len = *self.handle_lens.get(slot).ok_or("invalid-handle")?;
            if len == 0 {
                return Err("invalid-handle");
            }
            out[..len].copy_from_slice(&self.handle_paths[slot][..len]);
            Ok(len)
        }

        fn store_drawable(
            &mut self,
            book: Handle,
            page_index: u32,
        ) -> Result<Handle, &'static str> {
            let slot = self
                .drawable_active
                .iter()
                .position(|active| !*active)
                .unwrap_or(0);
            self.drawable_books[slot] = book.id;
            self.drawable_pages[slot] = page_index;
            self.drawable_active[slot] = true;
            let slot = u16::try_from(slot).map_err(|_| "too-many-open")?;
            Ok(Handle::new(HandleKind::Drawable, slot))
        }

        fn drawable_book_page(&self, drawable: Handle) -> Result<(Handle, u32), &'static str> {
            if drawable.kind != HandleKind::Drawable {
                return Err("invalid-handle");
            }
            let slot = usize::from(drawable.id);
            if !self
                .drawable_active
                .get(slot)
                .copied()
                .ok_or("invalid-handle")?
            {
                return Err("invalid-handle");
            }
            Ok((
                Handle::new(HandleKind::BinBook, self.drawable_books[slot]),
                self.drawable_pages[slot],
            ))
        }

        pub fn render_drawable_absolute_gray<SPI, DC, RST, BUSY, D>(
            &mut self,
            drawable: Handle,
            panel: &mut X4Panel<SPI, DC, RST, BUSY>,
            delay: &mut D,
            buffers: &mut RenderBuffers<'_>,
        ) -> Result<(), &'static str>
        where
            SPI: SpiDevice<u8>,
            DC: OutputPin,
            RST: OutputPin,
            BUSY: InputPin,
            D: DelayNs,
        {
            let (book, page_index) = self.drawable_book_page(drawable)?;
            let mut path_buf = [0u8; TEXT_BYTES];
            let path_len = self.copy_handle_path(book, &mut path_buf)?;
            let path = core::str::from_utf8(&path_buf[..path_len]).map_err(|_| "invalid-name")?;
            let source = FileStorageReadAt {
                storage: self.files.storage_mut(),
                path,
            };
            let mut opened =
                Book::open(source, &mut self.section_scratch).map_err(map_binbook_error)?;
            render_absolute_gray_with_bounded_settle(panel, &mut opened, page_index, buffers, delay)
                .map_err(map_display_error)
        }
    }

    #[cfg(feature = "x4-binbook")]
    fn render_absolute_gray_with_bounded_settle<R, SPI, DC, RST, BUSY, D>(
        panel: &mut X4Panel<SPI, DC, RST, BUSY>,
        book: &mut Book<R>,
        page: u32,
        buffers: &mut RenderBuffers<'_>,
        delay: &mut D,
    ) -> Result<(), DisplayError>
    where
        R: ReadAt,
        SPI: SpiDevice<u8>,
        DC: OutputPin,
        RST: OutputPin,
        BUSY: InputPin,
        D: DelayNs,
    {
        let page = read_x4_page(book, page)?;
        if page.pixel_format == PixelFormat::Gray1Packed {
            panel.init_bw(delay)?;
            let plane = page
                .planes
                .get(PlaneSlot::FastBase)
                .ok_or(DisplayError::InvalidPage)?;
            if buffers.compressed.is_empty() {
                return Err(DisplayError::BufferTooSmall {
                    required: 1,
                    provided: 0,
                });
            }
            require_row(buffers.decoded)?;

            let mut decoder = PlaneDecoder::new(plane);
            panel
                .controller()
                .set_window(0, 0, PHYSICAL_WIDTH, PHYSICAL_HEIGHT)?;
            let mut error = None;
            panel.controller().write_red_frame_rows::<ROW_BYTES>(
                PHYSICAL_HEIGHT,
                |_, output| {
                    if error.is_some() {
                        output.fill(0xff);
                        return;
                    }
                    if let Err(value) =
                        decoder.fill(book, buffers.compressed, &mut buffers.decoded[..ROW_BYTES])
                    {
                        error = Some(value);
                        output.fill(0xff);
                    } else {
                        output.copy_from_slice(&buffers.decoded[..ROW_BYTES]);
                    }
                },
            )?;
            if let Some(error) = error {
                return Err(error);
            }
            decoder.finish()?;

            let mut decoder = PlaneDecoder::new(plane);
            panel
                .controller()
                .set_window(0, 0, PHYSICAL_WIDTH, PHYSICAL_HEIGHT)?;
            let mut error = None;
            panel
                .controller()
                .write_frame_rows::<ROW_BYTES>(PHYSICAL_HEIGHT, |_, output| {
                    if error.is_some() {
                        output.fill(0xff);
                        return;
                    }
                    if let Err(value) =
                        decoder.fill(book, buffers.compressed, &mut buffers.decoded[..ROW_BYTES])
                    {
                        error = Some(value);
                        output.fill(0xff);
                    } else {
                        output.copy_from_slice(&buffers.decoded[..ROW_BYTES]);
                    }
                })?;
            if let Some(error) = error {
                return Err(error);
            }
            decoder.finish()?;
            panel.controller().trigger_refresh(RefreshMode::Full)?;
            delay.delay_ms(8_000);
            return Ok(());
        }

        panel.init_absolute_gray(delay)?;
        let (input0, input1, input2) = split_three(buffers.compressed)?;
        let (row0, row1, row2) = row_triplet(buffers.decoded)?;
        require_row(buffers.red)?;
        require_row(buffers.black)?;

        let mut msb = PlaneDecoder::new(
            page.planes
                .get(binbook_core::PlaneSlot::OverlayMsb)
                .ok_or(DisplayError::InvalidPage)?,
        );
        let mut lsb = PlaneDecoder::new(
            page.planes
                .get(binbook_core::PlaneSlot::OverlayLsb)
                .ok_or(DisplayError::InvalidPage)?,
        );
        let mut base = PlaneDecoder::new(
            page.planes
                .get(binbook_core::PlaneSlot::FastBase)
                .ok_or(DisplayError::InvalidPage)?,
        );

        panel
            .controller()
            .set_window(0, 0, PHYSICAL_WIDTH, PHYSICAL_HEIGHT)?;
        let mut error = None;
        panel
            .controller()
            .write_red_frame_rows::<ROW_BYTES>(PHYSICAL_HEIGHT, |_, output| {
                if error.is_some() {
                    output.fill(0xff);
                    return;
                }
                let result = fill_absolute_row(
                    book,
                    (&mut msb, input0, row0),
                    (&mut lsb, input1, row1),
                    (&mut base, input2, row2),
                    output,
                    buffers.black,
                );
                if let Err(value) = result {
                    error = Some(value);
                    output.fill(0xff);
                }
            })?;
        if let Some(error) = error {
            return Err(error);
        }
        msb.finish()?;
        lsb.finish()?;
        base.finish()?;

        let mut lsb = PlaneDecoder::new(
            page.planes
                .get(binbook_core::PlaneSlot::OverlayLsb)
                .ok_or(DisplayError::InvalidPage)?,
        );
        let mut base = PlaneDecoder::new(
            page.planes
                .get(binbook_core::PlaneSlot::FastBase)
                .ok_or(DisplayError::InvalidPage)?,
        );
        row0.fill(0);
        panel
            .controller()
            .set_window(0, 0, PHYSICAL_WIDTH, PHYSICAL_HEIGHT)?;
        let mut error = None;
        panel
            .controller()
            .write_frame_rows::<ROW_BYTES>(PHYSICAL_HEIGHT, |_, output| {
                if error.is_some() {
                    output.fill(0xff);
                    return;
                }
                let result = lsb
                    .fill(book, input1, row1)
                    .and_then(|()| base.fill(book, input2, row2))
                    .and_then(|()| {
                        gray2_render::staged_row_to_absolute(row0, row1, row2, buffers.red, output)
                            .map_err(Into::into)
                    });
                if let Err(value) = result {
                    error = Some(value);
                    output.fill(0xff);
                }
            })?;
        if let Some(error) = error {
            return Err(error);
        }
        lsb.finish()?;
        base.finish()?;
        panel.controller().trigger_refresh(RefreshMode::Grayscale)?;
        delay.delay_ms(8_000);
        Ok(())
    }

    #[cfg(feature = "x4-binbook")]
    fn split_three(buffer: &mut [u8]) -> Result<(&mut [u8], &mut [u8], &mut [u8]), DisplayError> {
        if buffer.len() < 3 {
            return Err(DisplayError::BufferTooSmall {
                required: 3,
                provided: buffer.len(),
            });
        }
        let third = buffer.len() / 3;
        let (first, rest) = buffer.split_at_mut(third);
        let (second, third_buffer) = rest.split_at_mut(third);
        Ok((first, second, third_buffer))
    }

    #[cfg(feature = "x4-binbook")]
    fn row_triplet(buffer: &mut [u8]) -> Result<(&mut [u8], &mut [u8], &mut [u8]), DisplayError> {
        const REQUIRED: usize = ROW_BYTES * 3;
        if buffer.len() < REQUIRED {
            return Err(DisplayError::BufferTooSmall {
                required: REQUIRED,
                provided: buffer.len(),
            });
        }
        let (first, rest) = buffer.split_at_mut(ROW_BYTES);
        let (second, rest) = rest.split_at_mut(ROW_BYTES);
        Ok((first, second, &mut rest[..ROW_BYTES]))
    }

    #[cfg(feature = "x4-binbook")]
    fn require_row(buffer: &[u8]) -> Result<(), DisplayError> {
        if buffer.len() < ROW_BYTES {
            Err(DisplayError::BufferTooSmall {
                required: ROW_BYTES,
                provided: buffer.len(),
            })
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "x4-binbook")]
    fn fill_absolute_row<R: ReadAt>(
        book: &mut Book<R>,
        msb: (&mut PlaneDecoder, &mut [u8], &mut [u8]),
        lsb: (&mut PlaneDecoder, &mut [u8], &mut [u8]),
        base: (&mut PlaneDecoder, &mut [u8], &mut [u8]),
        red: &mut [u8],
        black: &mut [u8],
    ) -> Result<(), DisplayError> {
        msb.0.fill(book, msb.1, msb.2)?;
        lsb.0.fill(book, lsb.1, lsb.2)?;
        base.0.fill(book, base.1, base.2)?;
        gray2_render::staged_row_to_absolute(msb.2, lsb.2, base.2, red, black)?;
        Ok(())
    }

    #[cfg(feature = "x4-binbook")]
    impl<
            S,
            const TEXT_BYTES: usize,
            const LINE_COUNT: usize,
            const LINE_BYTES: usize,
            const SECTION_SCRATCH_BYTES: usize,
            const TITLE_BYTES: usize,
            const HANDLE_COUNT: usize,
        > NativeFileBackend
        for X4BinBookFileBackend<
            S,
            TEXT_BYTES,
            LINE_COUNT,
            LINE_BYTES,
            SECTION_SCRATCH_BYTES,
            TITLE_BYTES,
            HANDLE_COUNT,
        >
    where
        S: NativeFileStorage,
    {
        fn reset_runtime_state(&mut self) {
            self.handle_lens.fill(0);
            self.drawable_active.fill(false);
        }

        fn file_pick_file<'a>(
            &'a mut self,
            extension: &str,
        ) -> Result<FilePickFileResult<'a>, VmError> {
            self.files.file_pick_file(extension)
        }

        fn file_read_text<'a>(&'a mut self, path: &str) -> Result<FileReadTextResult<'a>, VmError> {
            self.files.file_read_text(path)
        }

        fn file_read_lines<'a>(
            &'a mut self,
            path: &str,
            max_lines: i32,
        ) -> Result<FileReadLinesResult<'a>, VmError> {
            self.files.file_read_lines(path, max_lines)
        }

        fn file_read_lines_into<'a>(
            &'a mut self,
            path: &str,
            max_lines: i32,
            writer: &mut dyn FileReadLinesWriter,
        ) -> Result<FileReadLinesSummary<'a>, VmError> {
            self.files.file_read_lines_into(path, max_lines, writer)
        }

        fn file_copy<'a>(
            &'a mut self,
            source: &str,
            library: &str,
            name: &str,
        ) -> Result<FileCopyResult<'a>, VmError> {
            self.files.file_copy(source, library, name)
        }

        fn file_list_into<'a>(
            &'a mut self,
            library: &str,
            offset: i32,
            limit: i32,
            writer: &mut dyn FileListWriter,
        ) -> Result<FileListSummary<'a>, VmError> {
            self.files.file_list_into(library, offset, limit, writer)
        }

        fn content_binbook_list_into<'a>(
            &'a mut self,
            library: &str,
            offset: i32,
            limit: i32,
            writer: &mut dyn ContentBinBookListWriter,
        ) -> Result<ContentBinBookListSummary<'a>, VmError> {
            self.files
                .content_binbook_list_into(library, offset, limit, writer)
        }

        fn content_install_begin<'a>(
            &'a mut self,
            name: &str,
            total_len: usize,
        ) -> Result<&'a str, &'static str> {
            self.files.content_install_begin(name, total_len)
        }

        fn content_install_chunk(
            &mut self,
            path: &str,
            offset: usize,
            bytes: &[u8],
        ) -> Result<(), &'static str> {
            self.files.content_install_chunk(path, offset, bytes)
        }

        fn content_install_commit(&mut self, path: &str) -> Result<(), &'static str> {
            self.files.content_install_commit(path)
        }

        fn content_check<'a>(
            &'a mut self,
            name: &str,
        ) -> Result<NativeContentCheckResult<'a>, &'static str> {
            self.files.content_check(name)
        }

        fn content_delete<'a>(&'a mut self, name: &str) -> Result<&'a str, &'static str> {
            self.files.content_delete(name)
        }

        fn storage_format(&mut self) -> Result<(), &'static str> {
            self.files.storage_format()
        }

        fn file_ref_size(&mut self, path: &str) -> Result<u64, &'static str> {
            self.files.file_ref_size(path)
        }

        fn file_ref_read_at(
            &mut self,
            path: &str,
            offset: u64,
            out: &mut [u8],
        ) -> Result<(), &'static str> {
            self.files.file_ref_read_at(path, offset, out)
        }

        fn upload_stage_begin<'a>(
            &'a mut self,
            safe_name: &str,
            total_len: usize,
        ) -> Result<&'a str, &'static str> {
            self.files.upload_stage_begin(safe_name, total_len)
        }

        fn upload_stage_chunk(
            &mut self,
            path: &str,
            offset: usize,
            bytes: &[u8],
        ) -> Result<(), &'static str> {
            self.files.upload_stage_chunk(path, offset, bytes)
        }

        fn upload_stage_commit(&mut self, path: &str) -> Result<(), &'static str> {
            self.files.upload_stage_commit(path)
        }

        fn upload_stage_delete(&mut self, path: &str) -> Result<(), &'static str> {
            self.files.upload_stage_delete(path)
        }

        fn binbook_open<'a>(&'a mut self, path: &str) -> Result<BinBookOpenResult<'a>, VmError> {
            let mut mapped = heapless::String::<TEXT_BYTES>::new();
            let path = match Self::normalize_binbook_path(path, &mut mapped) {
                Ok(path) => path,
                Err(error) => {
                    return Ok(BinBookOpenResult {
                        ok: false,
                        error: Some(error),
                        book: None,
                    })
                }
            };
            if !path.ends_with(".binbook") {
                return Ok(BinBookOpenResult {
                    ok: false,
                    error: Some("invalid-name"),
                    book: None,
                });
            }
            let source = FileStorageReadAt {
                storage: self.files.storage_mut(),
                path,
            };
            if let Err(error) = Book::open(source, &mut self.section_scratch) {
                return Ok(BinBookOpenResult {
                    ok: false,
                    error: Some(map_binbook_error(error)),
                    book: None,
                });
            }
            match self.store_handle(path) {
                Ok(book) => Ok(BinBookOpenResult {
                    ok: true,
                    error: None,
                    book: Some(book),
                }),
                Err(error) => Ok(BinBookOpenResult {
                    ok: false,
                    error: Some(error),
                    book: None,
                }),
            }
        }

        fn binbook_info<'a>(&'a mut self, book: Handle) -> Result<BinBookInfoResult<'a>, VmError> {
            let mut path_buf = [0u8; TEXT_BYTES];
            let path_len = match self.copy_handle_path(book, &mut path_buf) {
                Ok(len) => len,
                Err(error) => {
                    return Ok(BinBookInfoResult {
                        ok: false,
                        error: Some(error),
                        title: None,
                        page_count: 0,
                        chapter_count: 0,
                    })
                }
            };
            let path = match core::str::from_utf8(&path_buf[..path_len]) {
                Ok(path) => path,
                Err(_) => {
                    return Ok(BinBookInfoResult {
                        ok: false,
                        error: Some("invalid-name"),
                        title: None,
                        page_count: 0,
                        chapter_count: 0,
                    })
                }
            };
            let source = FileStorageReadAt {
                storage: self.files.storage_mut(),
                path,
            };
            let mut book = match Book::open(source, &mut self.section_scratch) {
                Ok(book) => book,
                Err(error) => {
                    return Ok(BinBookInfoResult {
                        ok: false,
                        error: Some(map_binbook_error(error)),
                        title: None,
                        page_count: 0,
                        chapter_count: 0,
                    })
                }
            };
            let metadata = match book.book_metadata(&mut self.metadata_scratch) {
                Ok(metadata) => metadata,
                Err(error) => {
                    return Ok(BinBookInfoResult {
                        ok: false,
                        error: Some(map_binbook_error(error)),
                        title: None,
                        page_count: 0,
                        chapter_count: 0,
                    })
                }
            };
            let title = match book.read_string(metadata.title, &mut self.title) {
                Ok(title) => core::str::from_utf8(title).ok(),
                Err(_) => None,
            };
            let page_count = i32::try_from(book.page_count()).unwrap_or(i32::MAX);
            let chapter_count = i32::try_from(book.chapter_count()).unwrap_or(i32::MAX);
            Ok(BinBookInfoResult {
                ok: true,
                error: None,
                title,
                page_count,
                chapter_count,
            })
        }

        fn binbook_read_page<'a>(
            &'a mut self,
            book: Handle,
            page_index: i32,
        ) -> Result<BinBookReadPageResult<'a>, VmError> {
            if page_index < 0 {
                return Ok(BinBookReadPageResult {
                    ok: false,
                    error: Some("out-of-range"),
                    drawable: None,
                });
            }
            let mut path_buf = [0u8; TEXT_BYTES];
            let path_len = match self.copy_handle_path(book, &mut path_buf) {
                Ok(len) => len,
                Err(error) => {
                    return Ok(BinBookReadPageResult {
                        ok: false,
                        error: Some(error),
                        drawable: None,
                    })
                }
            };
            let path = match core::str::from_utf8(&path_buf[..path_len]) {
                Ok(path) => path,
                Err(_) => {
                    return Ok(BinBookReadPageResult {
                        ok: false,
                        error: Some("invalid-name"),
                        drawable: None,
                    })
                }
            };
            let source = FileStorageReadAt {
                storage: self.files.storage_mut(),
                path,
            };
            let mut opened = match Book::open(source, &mut self.section_scratch) {
                Ok(book) => book,
                Err(error) => {
                    return Ok(BinBookReadPageResult {
                        ok: false,
                        error: Some(map_binbook_error(error)),
                        drawable: None,
                    })
                }
            };
            let page_index = page_index as u32;
            if let Err(error) = read_x4_page(&mut opened, page_index) {
                return Ok(BinBookReadPageResult {
                    ok: false,
                    error: Some(map_display_error(error)),
                    drawable: None,
                });
            }
            match self.store_drawable(book, page_index) {
                Ok(drawable) => Ok(BinBookReadPageResult {
                    ok: true,
                    error: None,
                    drawable: Some(drawable),
                }),
                Err(error) => Ok(BinBookReadPageResult {
                    ok: false,
                    error: Some(error),
                    drawable: None,
                }),
            }
        }

        fn binbook_chapters_into<'a>(
            &'a mut self,
            book: Handle,
            offset: i32,
            limit: i32,
            writer: &mut dyn BinBookChapterListWriter,
        ) -> Result<BinBookChapterListSummary<'a>, VmError> {
            if offset < 0 || limit < 0 {
                return Ok(BinBookChapterListSummary {
                    ok: false,
                    error: Some("out-of-range"),
                    count: 0,
                    has_more: false,
                });
            }
            let mut path_buf = [0u8; TEXT_BYTES];
            let path_len = match self.copy_handle_path(book, &mut path_buf) {
                Ok(len) => len,
                Err(error) => {
                    return Ok(BinBookChapterListSummary {
                        ok: false,
                        error: Some(error),
                        count: 0,
                        has_more: false,
                    })
                }
            };
            let path = match core::str::from_utf8(&path_buf[..path_len]) {
                Ok(path) => path,
                Err(_) => {
                    return Ok(BinBookChapterListSummary {
                        ok: false,
                        error: Some("invalid-name"),
                        count: 0,
                        has_more: false,
                    })
                }
            };
            let source = FileStorageReadAt {
                storage: self.files.storage_mut(),
                path,
            };
            let mut opened = match Book::open(source, &mut self.section_scratch) {
                Ok(book) => book,
                Err(error) => {
                    return Ok(BinBookChapterListSummary {
                        ok: false,
                        error: Some(map_binbook_error(error)),
                        count: 0,
                        has_more: false,
                    })
                }
            };
            let total = opened.chapter_count();
            let start = offset as u32;
            let requested = limit as u32;
            let end = start.saturating_add(requested).min(total);
            for raw in start..end {
                let number = match opened.chapter_number(raw) {
                    Ok(number) => number,
                    Err(_) => {
                        return Ok(BinBookChapterListSummary {
                            ok: false,
                            error: Some("out-of-range"),
                            count: i32::try_from(total).unwrap_or(i32::MAX),
                            has_more: false,
                        })
                    }
                };
                let chapter = match opened.chapter(number, &mut self.metadata_scratch) {
                    Ok(chapter) => chapter,
                    Err(error) => {
                        return Ok(BinBookChapterListSummary {
                            ok: false,
                            error: Some(map_binbook_error(error)),
                            count: i32::try_from(total).unwrap_or(i32::MAX),
                            has_more: false,
                        })
                    }
                };
                let title = match opened.read_string(chapter.title, &mut self.title) {
                    Ok(title) => match core::str::from_utf8(title) {
                        Ok(title) => title,
                        Err(_) => {
                            return Ok(BinBookChapterListSummary {
                                ok: false,
                                error: Some("invalid-content"),
                                count: i32::try_from(total).unwrap_or(i32::MAX),
                                has_more: false,
                            })
                        }
                    },
                    Err(error) => {
                        return Ok(BinBookChapterListSummary {
                            ok: false,
                            error: Some(map_binbook_error(error)),
                            count: i32::try_from(total).unwrap_or(i32::MAX),
                            has_more: false,
                        })
                    }
                };
                writer.push_entry(BinBookChapterEntry {
                    index: i32::try_from(raw).unwrap_or(i32::MAX),
                    title,
                    page_index: i32::try_from(chapter.page.get()).unwrap_or(i32::MAX),
                    level: i32::from(chapter.level),
                    entry_type: i32::from(chapter.nav_type),
                })?;
            }
            Ok(BinBookChapterListSummary {
                ok: true,
                error: None,
                count: i32::try_from(total).unwrap_or(i32::MAX),
                has_more: end < total,
            })
        }

        fn binbook_chapter<'a>(
            &'a mut self,
            book: Handle,
            index: i32,
        ) -> Result<BinBookChapterResult<'a>, VmError> {
            if index < 0 {
                return Ok(BinBookChapterResult {
                    ok: false,
                    error: Some("out-of-range"),
                    chapter: None,
                });
            }
            let mut path_buf = [0u8; TEXT_BYTES];
            let path_len = match self.copy_handle_path(book, &mut path_buf) {
                Ok(len) => len,
                Err(error) => {
                    return Ok(BinBookChapterResult {
                        ok: false,
                        error: Some(error),
                        chapter: None,
                    })
                }
            };
            let path = match core::str::from_utf8(&path_buf[..path_len]) {
                Ok(path) => path,
                Err(_) => {
                    return Ok(BinBookChapterResult {
                        ok: false,
                        error: Some("invalid-name"),
                        chapter: None,
                    })
                }
            };
            let source = FileStorageReadAt {
                storage: self.files.storage_mut(),
                path,
            };
            let mut opened = match Book::open(source, &mut self.section_scratch) {
                Ok(book) => book,
                Err(error) => {
                    return Ok(BinBookChapterResult {
                        ok: false,
                        error: Some(map_binbook_error(error)),
                        chapter: None,
                    })
                }
            };
            let raw = index as u32;
            let number = match opened.chapter_number(raw) {
                Ok(number) => number,
                Err(_) => {
                    return Ok(BinBookChapterResult {
                        ok: false,
                        error: Some("out-of-range"),
                        chapter: None,
                    })
                }
            };
            let chapter = match opened.chapter(number, &mut self.metadata_scratch) {
                Ok(chapter) => chapter,
                Err(error) => {
                    return Ok(BinBookChapterResult {
                        ok: false,
                        error: Some(map_binbook_error(error)),
                        chapter: None,
                    })
                }
            };
            let title = match opened.read_string(chapter.title, &mut self.title) {
                Ok(title) => match core::str::from_utf8(title) {
                    Ok(title) => title,
                    Err(_) => {
                        return Ok(BinBookChapterResult {
                            ok: false,
                            error: Some("invalid-content"),
                            chapter: None,
                        })
                    }
                },
                Err(error) => {
                    return Ok(BinBookChapterResult {
                        ok: false,
                        error: Some(map_binbook_error(error)),
                        chapter: None,
                    })
                }
            };
            Ok(BinBookChapterResult {
                ok: true,
                error: None,
                chapter: Some(BinBookChapterEntry {
                    index,
                    title,
                    page_index: i32::try_from(chapter.page.get()).unwrap_or(i32::MAX),
                    level: i32::from(chapter.level),
                    entry_type: i32::from(chapter.nav_type),
                }),
            })
        }
    }

    #[cfg(feature = "x4-binbook")]
    struct FileStorageReadAt<'a, S> {
        storage: &'a mut S,
        path: &'a str,
    }

    #[cfg(feature = "x4-binbook")]
    impl<S: NativeFileStorage> ReadAt for FileStorageReadAt<'_, S> {
        type Error = NativeFileStorageError;

        fn len(&mut self) -> Result<u64, Self::Error> {
            self.storage.file_size(self.path)
        }

        fn read_exact_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Self::Error> {
            self.storage.read_at(self.path, offset, out)
        }
    }

    #[cfg(feature = "x4-binbook")]
    fn map_binbook_error(error: BinBookError<NativeFileStorageError>) -> &'static str {
        match error {
            BinBookError::Source(error) => error.as_file_error(),
            BinBookError::Format(_) => "invalid-content",
            BinBookError::BufferTooSmall { .. } => "too-large",
        }
    }

    #[cfg(feature = "x4-binbook")]
    fn map_display_error(error: xteink_x4_display::DisplayError) -> &'static str {
        match error {
            xteink_x4_display::DisplayError::Source => "io-error",
            xteink_x4_display::DisplayError::BufferTooSmall { .. } => "too-large",
            _ => "invalid-content",
        }
    }

    impl<S: FlatSdStorage> NativeFileStorage for X4SdFileStorage<S> {
        fn for_each_file(
            &mut self,
            visit: &mut dyn FnMut(&str, u64),
        ) -> Result<(), NativeFileStorageError> {
            let mut mapped = heapless::String::<256>::new();
            self.storage.flat_for_each_file(&mut |name, size| {
                mapped.clear();
                if mapped.push_str("books/").is_err() || mapped.push_str(name).is_err() {
                    return;
                }
                visit(mapped.as_str(), size);
            })
        }

        fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_file_size(name),
                X4StoragePath::Tmp(name) => self.storage.tmp_file_size(name),
            }
        }

        fn read_at(
            &mut self,
            path: &str,
            offset: u64,
            out: &mut [u8],
        ) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_read_at(name, offset, out),
                X4StoragePath::Tmp(name) => self.storage.tmp_read_at(name, offset, out),
            }
        }

        fn create_or_truncate(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_create_or_truncate(name),
                X4StoragePath::Tmp(name) => self.storage.tmp_create_or_truncate(name),
            }
        }

        fn begin_write(
            &mut self,
            path: &str,
            expected_size: u64,
        ) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_begin_write(name, expected_size),
                X4StoragePath::Tmp(name) => self.storage.tmp_begin_write(name, expected_size),
            }
        }

        fn write_at(
            &mut self,
            path: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_write_at(name, offset, data),
                X4StoragePath::Tmp(name) => self.storage.tmp_write_at(name, offset, data),
            }
        }

        fn write_chunk(
            &mut self,
            path: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_write_chunk(name, offset, data),
                X4StoragePath::Tmp(name) => self.storage.tmp_write_chunk(name, offset, data),
            }
        }

        fn flush(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_flush(name),
                X4StoragePath::Tmp(name) => self.storage.tmp_flush(name),
            }
        }

        fn commit_write(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_commit_write(name),
                X4StoragePath::Tmp(name) => self.storage.tmp_commit_write(name),
            }
        }

        fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
            match map_x4_storage_path(path)? {
                X4StoragePath::Book(name) => self.storage.flat_delete(name),
                X4StoragePath::Tmp(name) => self.storage.tmp_delete(name),
            }
        }

        fn format(&mut self) -> Result<(), NativeFileStorageError> {
            self.storage.flat_format()
        }

        fn copy_file(
            &mut self,
            source: &str,
            destination: &str,
            scratch: &mut [u8],
        ) -> Result<Option<u64>, NativeFileStorageError> {
            match (
                map_x4_storage_path(source)?,
                map_x4_storage_path(destination)?,
            ) {
                (X4StoragePath::Tmp(source), X4StoragePath::Book(destination)) => {
                    self.storage.copy_tmp_to_flat(source, destination, scratch)
                }
                (X4StoragePath::Book(_), X4StoragePath::Book(_))
                | (X4StoragePath::Book(_), X4StoragePath::Tmp(_))
                | (X4StoragePath::Tmp(_), X4StoragePath::Tmp(_)) => Ok(None),
            }
        }
    }

    impl<D, TIME> FlatSdStorage for SdStorage<D, TIME>
    where
        D: BlockDevice,
        D::Error: core::fmt::Debug,
        TIME: TimeSource,
    {
        fn flat_for_each_file(
            &mut self,
            visit: &mut dyn FnMut(&str, u64),
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::for_each_entry(self, visit).map_err(map_storage_error)
        }

        fn flat_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError> {
            SdStorage::file_size(self, name).map_err(map_storage_error)
        }

        fn flat_read_at(
            &mut self,
            name: &str,
            offset: u64,
            out: &mut [u8],
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::read_at(self, name, offset, out).map_err(map_storage_error)
        }

        fn flat_create_or_truncate(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::create_or_truncate(self, name).map_err(map_storage_error)
        }

        fn flat_begin_write(
            &mut self,
            name: &str,
            expected_size: u64,
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::begin_upload(self, name, expected_size).map_err(map_storage_error)
        }

        fn flat_write_at(
            &mut self,
            name: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::write_at(self, name, offset, data).map_err(map_storage_error)
        }

        fn flat_write_chunk(
            &mut self,
            name: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::write_upload_chunk(self, name, offset, data).map_err(map_storage_error)
        }

        fn flat_flush(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::flush(self, name).map_err(map_storage_error)
        }

        fn flat_commit_write(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::commit_upload(self, name).map_err(map_storage_error)
        }

        fn flat_delete(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::delete_file(self, name).map_err(map_storage_error)
        }

        fn tmp_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError> {
            SdStorage::tmp_file_size(self, name).map_err(map_storage_error)
        }

        fn tmp_read_at(
            &mut self,
            name: &str,
            offset: u64,
            out: &mut [u8],
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_read_at(self, name, offset, out).map_err(map_storage_error)
        }

        fn tmp_create_or_truncate(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_create_or_truncate(self, name).map_err(map_storage_error)
        }

        fn tmp_begin_write(
            &mut self,
            name: &str,
            expected_size: u64,
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_begin_upload(self, name, expected_size).map_err(map_storage_error)
        }

        fn tmp_write_at(
            &mut self,
            name: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_write_at(self, name, offset, data).map_err(map_storage_error)
        }

        fn tmp_write_chunk(
            &mut self,
            name: &str,
            offset: u64,
            data: &[u8],
        ) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_write_upload_chunk(self, name, offset, data).map_err(map_storage_error)
        }

        fn tmp_flush(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_flush(self, name).map_err(map_storage_error)
        }

        fn tmp_commit_write(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_commit_upload(self, name).map_err(map_storage_error)
        }

        fn tmp_delete(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
            SdStorage::tmp_delete_file(self, name).map_err(map_storage_error)
        }

        fn copy_tmp_to_flat(
            &mut self,
            source_name: &str,
            destination_name: &str,
            scratch: &mut [u8],
        ) -> Result<Option<u64>, NativeFileStorageError> {
            SdStorage::copy_tmp_to_books(self, source_name, destination_name, scratch)
                .map(Some)
                .map_err(map_storage_error)
        }
    }

    enum X4StoragePath<'a> {
        Book(&'a str),
        Tmp(&'a str),
    }

    fn map_x4_storage_path(path: &str) -> Result<X4StoragePath<'_>, NativeFileStorageError> {
        if let Some(name) = path.strip_prefix("books/") {
            validate_x4_flat_name(name)?;
            return Ok(X4StoragePath::Book(name));
        }
        if let Some(name) = path.strip_prefix("tmp/") {
            validate_x4_flat_name(name)?;
            return Ok(X4StoragePath::Tmp(name));
        }
        Err(NativeFileStorageError::NotFound)
    }

    fn validate_x4_flat_name(name: &str) -> Result<(), NativeFileStorageError> {
        if name.is_empty()
            || name.starts_with('/')
            || name.contains('/')
            || name.contains('\\')
            || name == "."
            || name == ".."
            || name.contains(':')
        {
            return Err(NativeFileStorageError::InvalidName);
        }
        Ok(())
    }

    fn map_storage_error<D>(error: StorageError<D>) -> NativeFileStorageError
    where
        D: BlockDevice,
        D::Error: core::fmt::Debug,
    {
        match error {
            StorageError::NotFound => NativeFileStorageError::NotFound,
            StorageError::NameTooLong => NativeFileStorageError::InvalidName,
            StorageError::Device(_) => NativeFileStorageError::VolumeMissing,
            StorageError::BadUploadOffset => NativeFileStorageError::InvalidName,
            StorageError::UploadInProgress
            | StorageError::NoUploadInProgress
            | StorageError::UploadNameMismatch
            | StorageError::UploadTooFragmented
            | StorageError::FileTooFragmented
            | StorageError::Fat(_) => NativeFileStorageError::Io,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[derive(Default)]
        struct MemoryVolume {
            available: bool,
            path: Option<std::string::String>,
            bytes: std::vec::Vec<u8>,
            copy_calls: usize,
            read_calls: usize,
        }

        impl MemoryVolume {
            fn available() -> Self {
                Self {
                    available: true,
                    ..Self::default()
                }
            }

            fn require_available(&self) -> Result<(), NativeFileStorageError> {
                if self.available {
                    Ok(())
                } else {
                    Err(NativeFileStorageError::VolumeMissing)
                }
            }
        }

        impl NativeFileStorage for MemoryVolume {
            fn for_each_file(
                &mut self,
                visit: &mut dyn FnMut(&str, u64),
            ) -> Result<(), NativeFileStorageError> {
                self.require_available()?;
                if let Some(path) = self.path.as_deref() {
                    visit(path, self.bytes.len() as u64);
                }
                Ok(())
            }

            fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError> {
                self.require_available()?;
                (self.path.as_deref() == Some(path))
                    .then_some(self.bytes.len() as u64)
                    .ok_or(NativeFileStorageError::NotFound)
            }

            fn read_at(
                &mut self,
                path: &str,
                offset: u64,
                out: &mut [u8],
            ) -> Result<(), NativeFileStorageError> {
                self.read_calls += 1;
                self.require_available()?;
                if self.path.as_deref() != Some(path) {
                    return Err(NativeFileStorageError::NotFound);
                }
                let start = offset as usize;
                out.copy_from_slice(
                    self.bytes
                        .get(start..start + out.len())
                        .ok_or(NativeFileStorageError::Io)?,
                );
                Ok(())
            }

            fn create_or_truncate(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
                self.require_available()?;
                self.path = Some(path.to_string());
                self.bytes.clear();
                Ok(())
            }

            fn write_at(
                &mut self,
                path: &str,
                offset: u64,
                data: &[u8],
            ) -> Result<(), NativeFileStorageError> {
                self.require_available()?;
                if self.path.as_deref() != Some(path) || offset as usize != self.bytes.len() {
                    return Err(NativeFileStorageError::Io);
                }
                self.bytes.extend_from_slice(data);
                Ok(())
            }

            fn flush(&mut self, _path: &str) -> Result<(), NativeFileStorageError> {
                self.require_available()
            }

            fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
                self.require_available()?;
                if self.path.as_deref() != Some(path) {
                    return Err(NativeFileStorageError::NotFound);
                }
                self.path = None;
                self.bytes.clear();
                Ok(())
            }

            fn format(&mut self) -> Result<(), NativeFileStorageError> {
                Ok(())
            }

            fn copy_file(
                &mut self,
                source: &str,
                destination: &str,
                _scratch: &mut [u8],
            ) -> Result<Option<u64>, NativeFileStorageError> {
                self.require_available()?;
                if self.path.as_deref() != Some(source) {
                    return Err(NativeFileStorageError::NotFound);
                }
                self.path = Some(destination.to_string());
                self.copy_calls += 1;
                Ok(Some(self.bytes.len() as u64))
            }
        }

        #[test]
        fn content_storage_uses_internal_volume_when_sd_is_missing() {
            let mut storage =
                X4ContentStorage::new(MemoryVolume::default(), MemoryVolume::available());

            storage
                .create_or_truncate("books/internal.binbook")
                .unwrap();
            storage
                .write_at("books/internal.binbook", 0, b"book")
                .unwrap();
            storage.flush("books/internal.binbook").unwrap();

            assert_eq!(storage.sd.path, None);
            assert_eq!(
                storage.internal.path.as_deref(),
                Some("books/internal.binbook")
            );
            let mut out = [0; 4];
            storage
                .read_at("books/internal.binbook", 0, &mut out)
                .unwrap();
            assert_eq!(&out, b"book");
        }

        #[derive(Default)]
        struct CountingMissingVolume {
            calls: usize,
        }

        impl NativeFileStorage for CountingMissingVolume {
            fn for_each_file(
                &mut self,
                _visit: &mut dyn FnMut(&str, u64),
            ) -> Result<(), NativeFileStorageError> {
                self.calls += 1;
                Err(NativeFileStorageError::VolumeMissing)
            }

            fn file_size(&mut self, _path: &str) -> Result<u64, NativeFileStorageError> {
                self.calls += 1;
                Err(NativeFileStorageError::VolumeMissing)
            }

            fn read_at(
                &mut self,
                _path: &str,
                _offset: u64,
                _out: &mut [u8],
            ) -> Result<(), NativeFileStorageError> {
                self.calls += 1;
                Err(NativeFileStorageError::VolumeMissing)
            }

            fn create_or_truncate(&mut self, _path: &str) -> Result<(), NativeFileStorageError> {
                self.calls += 1;
                Err(NativeFileStorageError::VolumeMissing)
            }

            fn write_at(
                &mut self,
                _path: &str,
                _offset: u64,
                _data: &[u8],
            ) -> Result<(), NativeFileStorageError> {
                panic!("missing SD must not own writes")
            }

            fn flush(&mut self, _path: &str) -> Result<(), NativeFileStorageError> {
                panic!("missing SD must not own commits")
            }

            fn delete(&mut self, _path: &str) -> Result<(), NativeFileStorageError> {
                self.calls += 1;
                Err(NativeFileStorageError::VolumeMissing)
            }

            fn format(&mut self) -> Result<(), NativeFileStorageError> {
                Err(NativeFileStorageError::VolumeMissing)
            }
        }

        #[test]
        fn content_storage_caches_sd_unavailable_after_first_probe() {
            let mut internal = MemoryVolume::available();
            internal
                .create_or_truncate("books/internal.binbook")
                .unwrap();
            internal
                .write_at("books/internal.binbook", 0, b"book")
                .unwrap();
            let mut storage = X4ContentStorage::new(CountingMissingVolume::default(), internal);

            assert_eq!(storage.file_size("books/internal.binbook"), Ok(4));
            assert_eq!(storage.file_size("books/internal.binbook"), Ok(4));
            storage.create_or_truncate("books/new.binbook").unwrap();

            assert_eq!(storage.sd.calls, 1);
            assert_eq!(storage.internal.path.as_deref(), Some("books/new.binbook"));
        }

        #[test]
        fn content_storage_keeps_internal_read_chunks_off_an_available_sd() {
            let sd = MemoryVolume::available();
            let mut internal = MemoryVolume::available();
            internal
                .create_or_truncate("books/internal.binbook")
                .unwrap();
            internal
                .write_at("books/internal.binbook", 0, b"abcdefgh")
                .unwrap();
            let mut storage = X4ContentStorage::new(sd, internal);

            assert_eq!(storage.file_size("books/internal.binbook"), Ok(8));
            let mut first = [0; 4];
            let mut second = [0; 4];
            storage
                .read_at("books/internal.binbook", 0, &mut first)
                .unwrap();
            storage
                .read_at("books/internal.binbook", 4, &mut second)
                .unwrap();

            assert_eq!([first, second].concat(), b"abcdefgh");
            assert_eq!(storage.sd.read_calls, 0);
            assert_eq!(storage.internal.read_calls, 2);
        }

        #[test]
        fn content_storage_prefers_sd_for_new_uploads_and_falls_back_for_reads() {
            let mut internal = MemoryVolume::available();
            internal
                .create_or_truncate("books/internal.binbook")
                .unwrap();
            internal
                .write_at("books/internal.binbook", 0, b"old")
                .unwrap();
            let mut storage = X4ContentStorage::new(MemoryVolume::available(), internal);

            storage.create_or_truncate("books/sd.binbook").unwrap();
            storage.write_at("books/sd.binbook", 0, b"new").unwrap();
            storage.flush("books/sd.binbook").unwrap();

            assert_eq!(storage.sd.path.as_deref(), Some("books/sd.binbook"));
            let mut out = [0; 3];
            storage
                .read_at("books/internal.binbook", 0, &mut out)
                .unwrap();
            assert_eq!(&out, b"old");
        }

        #[test]
        fn content_storage_delegates_same_volume_copy_to_sd() {
            let mut sd = MemoryVolume::available();
            sd.create_or_truncate("tmp/upload.binbook").unwrap();
            sd.write_at("tmp/upload.binbook", 0, b"book").unwrap();
            let mut storage = X4ContentStorage::new(sd, MemoryVolume::available());

            let mut scratch = [0; 8];
            assert_eq!(
                storage.copy_file("tmp/upload.binbook", "books/upload.binbook", &mut scratch),
                Ok(Some(4))
            );
            assert_eq!(storage.sd.copy_calls, 1);
            assert_eq!(storage.internal.copy_calls, 0);
            assert_eq!(storage.sd.path.as_deref(), Some("books/upload.binbook"));
        }

        #[test]
        fn content_storage_delegates_copy_to_internal_when_sd_is_missing() {
            let mut internal = MemoryVolume::available();
            internal.create_or_truncate("tmp/upload.binbook").unwrap();
            internal.write_at("tmp/upload.binbook", 0, b"book").unwrap();
            let mut storage = X4ContentStorage::new(MemoryVolume::default(), internal);

            let mut scratch = [0; 8];
            assert_eq!(
                storage.copy_file("tmp/upload.binbook", "books/upload.binbook", &mut scratch),
                Ok(Some(4))
            );
            assert_eq!(storage.sd.copy_calls, 0);
            assert_eq!(storage.internal.copy_calls, 1);
            assert_eq!(
                storage.internal.path.as_deref(),
                Some("books/upload.binbook")
            );
        }

        #[derive(Default)]
        struct FakeFlatStorage {
            last_name: Option<&'static str>,
        }

        impl FlatSdStorage for FakeFlatStorage {
            fn flat_for_each_file(
                &mut self,
                visit: &mut dyn FnMut(&str, u64),
            ) -> Result<(), NativeFileStorageError> {
                visit("note.txt", 5);
                Ok(())
            }

            fn flat_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError> {
                self.last_name = Some(match name {
                    "note.txt" => "note.txt",
                    _ => return Err(NativeFileStorageError::NotFound),
                });
                Ok(5)
            }

            fn flat_read_at(
                &mut self,
                name: &str,
                _offset: u64,
                out: &mut [u8],
            ) -> Result<(), NativeFileStorageError> {
                self.last_name = Some(match name {
                    "note.txt" => "note.txt",
                    _ => return Err(NativeFileStorageError::NotFound),
                });
                out[..5].copy_from_slice(b"ready");
                Ok(())
            }

            fn flat_create_or_truncate(
                &mut self,
                name: &str,
            ) -> Result<(), NativeFileStorageError> {
                self.last_name = Some(match name {
                    "note.txt" => "note.txt",
                    _ => return Err(NativeFileStorageError::NotFound),
                });
                Ok(())
            }

            fn flat_write_at(
                &mut self,
                name: &str,
                _offset: u64,
                _data: &[u8],
            ) -> Result<(), NativeFileStorageError> {
                self.last_name = Some(match name {
                    "note.txt" => "note.txt",
                    _ => return Err(NativeFileStorageError::NotFound),
                });
                Ok(())
            }

            fn flat_flush(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
                self.last_name = Some(match name {
                    "note.txt" => "note.txt",
                    _ => return Err(NativeFileStorageError::NotFound),
                });
                Ok(())
            }

            fn flat_delete(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
                self.last_name = Some(match name {
                    "note.txt" => "note.txt",
                    _ => return Err(NativeFileStorageError::NotFound),
                });
                Ok(())
            }
        }

        #[test]
        fn maps_squidscript_books_refs_to_flat_sd_names() {
            let mut storage = X4SdFileStorage::new(FakeFlatStorage::default());

            assert_eq!(storage.file_size("books/note.txt"), Ok(5));
            let mut out = [0_u8; 5];
            assert_eq!(storage.read_at("books/note.txt", 0, &mut out), Ok(()));

            assert_eq!(&out, b"ready");
            assert_eq!(storage.storage().last_name, Some("note.txt"));
        }

        #[test]
        fn lists_flat_sd_names_as_squidscript_books_refs() {
            let mut storage = X4SdFileStorage::new(FakeFlatStorage::default());
            let mut entries = std::vec::Vec::new();

            storage
                .for_each_file(&mut |path, size| entries.push((path.to_string(), size)))
                .unwrap();

            assert_eq!(entries, std::vec![("books/note.txt".to_string(), 5)]);
        }

        #[test]
        fn rejects_physical_or_nested_paths_before_sd_access() {
            let mut storage = X4SdFileStorage::new(FakeFlatStorage::default());

            assert_eq!(
                storage.file_size("/BOOKS/note.txt"),
                Err(NativeFileStorageError::NotFound)
            );
            assert_eq!(
                storage.file_size("books/../note.txt"),
                Err(NativeFileStorageError::InvalidName)
            );
            assert_eq!(
                storage.file_size("books/nested/note.txt"),
                Err(NativeFileStorageError::InvalidName)
            );
            assert_eq!(storage.storage().last_name, None);
        }

        #[test]
        fn x4_binbook_file_backend_checks_and_deletes_content_via_generic_storage() {
            #[derive(Default)]
            struct FakeContentStorage {
                name: Option<std::string::String>,
                bytes: std::vec::Vec<u8>,
                deleted: std::vec::Vec<std::string::String>,
            }

            impl FlatSdStorage for FakeContentStorage {
                fn flat_for_each_file(
                    &mut self,
                    _visit: &mut dyn FnMut(&str, u64),
                ) -> Result<(), NativeFileStorageError> {
                    Ok(())
                }

                fn flat_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError> {
                    if self.name.as_deref() == Some(name) {
                        Ok(self.bytes.len() as u64)
                    } else {
                        Err(NativeFileStorageError::NotFound)
                    }
                }

                fn flat_read_at(
                    &mut self,
                    name: &str,
                    offset: u64,
                    out: &mut [u8],
                ) -> Result<(), NativeFileStorageError> {
                    if self.name.as_deref() != Some(name) {
                        return Err(NativeFileStorageError::NotFound);
                    }
                    let start = usize::try_from(offset).map_err(|_| NativeFileStorageError::Io)?;
                    let end = start
                        .checked_add(out.len())
                        .ok_or(NativeFileStorageError::Io)?;
                    out.copy_from_slice(
                        self.bytes
                            .get(start..end)
                            .ok_or(NativeFileStorageError::Io)?,
                    );
                    Ok(())
                }

                fn flat_create_or_truncate(
                    &mut self,
                    name: &str,
                ) -> Result<(), NativeFileStorageError> {
                    self.name = Some(name.to_string());
                    self.bytes.clear();
                    Ok(())
                }

                fn flat_write_at(
                    &mut self,
                    name: &str,
                    offset: u64,
                    data: &[u8],
                ) -> Result<(), NativeFileStorageError> {
                    if self.name.as_deref() != Some(name) || offset as usize != self.bytes.len() {
                        return Err(NativeFileStorageError::InvalidName);
                    }
                    self.bytes.extend_from_slice(data);
                    Ok(())
                }

                fn flat_flush(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
                    if self.name.as_deref() == Some(name) {
                        Ok(())
                    } else {
                        Err(NativeFileStorageError::NotFound)
                    }
                }

                fn flat_delete(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
                    if self.name.as_deref() != Some(name) {
                        return Err(NativeFileStorageError::NotFound);
                    }
                    self.deleted.push(name.to_string());
                    self.name = None;
                    self.bytes.clear();
                    Ok(())
                }

                fn tmp_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError> {
                    self.flat_file_size(&format!("tmp/{name}"))
                }

                fn tmp_read_at(
                    &mut self,
                    name: &str,
                    offset: u64,
                    out: &mut [u8],
                ) -> Result<(), NativeFileStorageError> {
                    self.flat_read_at(&format!("tmp/{name}"), offset, out)
                }

                fn tmp_create_or_truncate(
                    &mut self,
                    name: &str,
                ) -> Result<(), NativeFileStorageError> {
                    self.flat_create_or_truncate(&format!("tmp/{name}"))
                }

                fn tmp_write_at(
                    &mut self,
                    name: &str,
                    offset: u64,
                    data: &[u8],
                ) -> Result<(), NativeFileStorageError> {
                    self.flat_write_at(&format!("tmp/{name}"), offset, data)
                }

                fn tmp_flush(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
                    self.flat_flush(&format!("tmp/{name}"))
                }

                fn tmp_delete(&mut self, name: &str) -> Result<(), NativeFileStorageError> {
                    self.flat_delete(&format!("tmp/{name}"))
                }
            }

            let storage = X4SdFileStorage::new(FakeContentStorage::default());
            let mut backend = X4BinBookFileBackend::<_, 64, 4, 32, 128, 64, 2>::new(storage);
            let path = backend
                .content_install_begin("proof.dat", 4)
                .unwrap()
                .to_string();
            backend.content_install_chunk(&path, 0, b"ABCD").unwrap();
            backend.content_install_commit(&path).unwrap();

            let checked = backend.content_check("proof.dat").unwrap();
            assert_eq!(checked.name, "proof.dat");
            assert_eq!(checked.size, 4);
            assert_eq!(checked.crc32, 0xdb17_20a5);

            assert_eq!(backend.content_delete("proof.dat"), Ok("proof.dat"));
            assert_eq!(backend.files().storage().storage().deleted, ["proof.dat"]);

            let upload_path = backend
                .upload_stage_begin("upload.sqbc", 5)
                .unwrap()
                .to_string();
            assert_eq!(upload_path, "tmp/upload.sqbc");
            backend
                .upload_stage_chunk(&upload_path, 0, b"ready")
                .unwrap();
            backend.upload_stage_commit(&upload_path).unwrap();
            backend.upload_stage_delete(&upload_path).unwrap();
            assert_eq!(
                backend.files().storage().storage().deleted,
                ["proof.dat", "tmp/upload.sqbc"]
            );
        }

        #[cfg(feature = "x4-binbook")]
        #[test]
        fn file_backed_binbook_backend_opens_and_reads_info_from_generic_storage() {
            use squidscript_fw_core::native_runtime::NativeFileBackend;
            use squidvm_core::value::{Handle, HandleKind};

            const BOOK_A: &[u8] = include_bytes!(
                "../../../../../../binbook/crates/binbook-storage/tests/fixtures/book_a.binbook"
            );

            struct FakeBookStorage;

            impl FlatSdStorage for FakeBookStorage {
                fn flat_for_each_file(
                    &mut self,
                    visit: &mut dyn FnMut(&str, u64),
                ) -> Result<(), NativeFileStorageError> {
                    visit("book_a.binbook", BOOK_A.len() as u64);
                    Ok(())
                }

                fn flat_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError> {
                    if name == "book_a.binbook" {
                        Ok(BOOK_A.len() as u64)
                    } else {
                        Err(NativeFileStorageError::NotFound)
                    }
                }

                fn flat_read_at(
                    &mut self,
                    name: &str,
                    offset: u64,
                    out: &mut [u8],
                ) -> Result<(), NativeFileStorageError> {
                    if name != "book_a.binbook" {
                        return Err(NativeFileStorageError::NotFound);
                    }
                    let start = usize::try_from(offset).map_err(|_| NativeFileStorageError::Io)?;
                    let end = start
                        .checked_add(out.len())
                        .ok_or(NativeFileStorageError::Io)?;
                    out.copy_from_slice(BOOK_A.get(start..end).ok_or(NativeFileStorageError::Io)?);
                    Ok(())
                }

                fn flat_create_or_truncate(
                    &mut self,
                    _name: &str,
                ) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }

                fn flat_write_at(
                    &mut self,
                    _name: &str,
                    _offset: u64,
                    _data: &[u8],
                ) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }

                fn flat_flush(&mut self, _name: &str) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }

                fn flat_delete(&mut self, _name: &str) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }
            }

            let storage = X4SdFileStorage::new(FakeBookStorage);
            let mut backend = X4BinBookFileBackend::<_, 512, 8, 128, 1024, 96, 2>::new(storage);

            let opened = backend
                .binbook_open("content:books/r/book_a.binbook")
                .unwrap();
            assert_eq!(opened.ok, true);
            assert_eq!(opened.book, Some(Handle::new(HandleKind::BinBook, 0)));

            let info = backend
                .binbook_info(Handle::new(HandleKind::BinBook, 0))
                .unwrap();
            assert_eq!(info.ok, true);
            assert_eq!(info.error, None);
            assert_eq!(info.page_count, 1);
            assert!(info.title.is_some());

            let page = backend
                .binbook_read_page(Handle::new(HandleKind::BinBook, 0), 0)
                .unwrap();
            assert_eq!(page.ok, true);
            assert_eq!(page.error, None);
            assert_eq!(page.drawable, Some(Handle::new(HandleKind::Drawable, 0)));

            assert!(
                backend
                    .binbook_open("content:books/p/book_a.binbook")
                    .unwrap()
                    .ok
            );
            assert_eq!(
                backend.binbook_open("books/book_a.binbook").unwrap().error,
                Some("too-many-open")
            );

            backend.reset_runtime_state();

            let reopened = backend.binbook_open("books/book_a.binbook").unwrap();
            assert!(reopened.ok);
            assert_eq!(reopened.book, Some(Handle::new(HandleKind::BinBook, 0)));
        }

        #[cfg(feature = "x4-binbook")]
        #[test]
        fn file_backed_binbook_backend_lists_and_reads_chapters_from_generic_storage() {
            use std::io::Cursor;

            use binbook_core::CompressionMethod;
            use binbook_encode::{
                BookBuilder, BookConfig, CompiledChunk, CompiledPage, CompiledPlane,
                NavigationEntry,
            };
            use squidscript_fw_core::native_runtime::NativeFileBackend;
            use squidvm_core::value::{Handle, HandleKind};

            fn page(seed: u8) -> CompiledPage {
                let planes = (0_u8..3)
                    .map(|slot| {
                        let chunks = (0_u8..30)
                            .map(|index| CompiledChunk {
                                compressed: vec![0x80, seed.wrapping_add(slot).wrapping_add(index)],
                                row_start: u16::from(index) * 16,
                                row_count: 16,
                                uncompressed_size: 1_600,
                            })
                            .collect();
                        CompiledPlane {
                            slot,
                            compression: CompressionMethod::RlePackBits,
                            chunks,
                        }
                    })
                    .collect();
                CompiledPage::new_gray2(800, 480, planes)
            }

            fn chapter_book() -> Vec<u8> {
                let mut builder = BookBuilder::new(BookConfig::xteink_x4());
                builder.add_page(page(0x11));
                builder.add_page(page(0x22));
                builder.add_navigation(NavigationEntry::chapter("Opening", 0));
                builder.add_navigation(NavigationEntry::chapter("Second", 1));
                let mut output = Cursor::new(Vec::new());
                builder.write_to(&mut output).unwrap();
                output.into_inner()
            }

            struct FakeBookStorage {
                bytes: Vec<u8>,
            }

            impl FlatSdStorage for FakeBookStorage {
                fn flat_for_each_file(
                    &mut self,
                    visit: &mut dyn FnMut(&str, u64),
                ) -> Result<(), NativeFileStorageError> {
                    visit("chapters.binbook", self.bytes.len() as u64);
                    Ok(())
                }

                fn flat_file_size(&mut self, name: &str) -> Result<u64, NativeFileStorageError> {
                    if name == "chapters.binbook" {
                        Ok(self.bytes.len() as u64)
                    } else {
                        Err(NativeFileStorageError::NotFound)
                    }
                }

                fn flat_read_at(
                    &mut self,
                    name: &str,
                    offset: u64,
                    out: &mut [u8],
                ) -> Result<(), NativeFileStorageError> {
                    if name != "chapters.binbook" {
                        return Err(NativeFileStorageError::NotFound);
                    }
                    let start = usize::try_from(offset).map_err(|_| NativeFileStorageError::Io)?;
                    let end = start
                        .checked_add(out.len())
                        .ok_or(NativeFileStorageError::Io)?;
                    out.copy_from_slice(
                        self.bytes
                            .get(start..end)
                            .ok_or(NativeFileStorageError::Io)?,
                    );
                    Ok(())
                }

                fn flat_create_or_truncate(
                    &mut self,
                    _name: &str,
                ) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }

                fn flat_write_at(
                    &mut self,
                    _name: &str,
                    _offset: u64,
                    _data: &[u8],
                ) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }

                fn flat_flush(&mut self, _name: &str) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }

                fn flat_delete(&mut self, _name: &str) -> Result<(), NativeFileStorageError> {
                    Err(NativeFileStorageError::Io)
                }
            }

            #[derive(Default)]
            struct ChapterWriter {
                entries: Vec<(i32, String, i32, i32, i32)>,
            }

            impl BinBookChapterListWriter for ChapterWriter {
                fn push_entry(
                    &mut self,
                    entry: BinBookChapterEntry<'_>,
                ) -> Result<(), squidvm_core::error::VmError> {
                    self.entries.push((
                        entry.index,
                        entry.title.to_string(),
                        entry.page_index,
                        entry.level,
                        entry.entry_type,
                    ));
                    Ok(())
                }
            }

            let storage = X4SdFileStorage::new(FakeBookStorage {
                bytes: chapter_book(),
            });
            let mut backend = X4BinBookFileBackend::<_, 512, 8, 128, 1024, 96, 2>::new(storage);

            let opened = backend.binbook_open("books/chapters.binbook").unwrap();
            assert_eq!(opened.book, Some(Handle::new(HandleKind::BinBook, 0)));

            let mut writer = ChapterWriter::default();
            let chapters = backend
                .binbook_chapters_into(Handle::new(HandleKind::BinBook, 0), 0, 1, &mut writer)
                .unwrap();
            assert_eq!(chapters.ok, true);
            assert_eq!(chapters.error, None);
            assert_eq!(chapters.count, 2);
            assert_eq!(chapters.has_more, true);
            assert_eq!(writer.entries, [(0, "Opening".to_string(), 0, 0, 3)]);

            let chapter = backend
                .binbook_chapter(Handle::new(HandleKind::BinBook, 0), 1)
                .unwrap();
            assert_eq!(chapter.ok, true);
            assert_eq!(chapter.error, None);
            let chapter = chapter.chapter.expect("chapter entry");
            assert_eq!(chapter.index, 1);
            assert_eq!(chapter.title, "Second");
            assert_eq!(chapter.page_index, 1);
            assert_eq!(chapter.level, 0);
            assert_eq!(chapter.entry_type, 3);
        }
    }
}

pub trait NativeDisplayFlushDriver<D, FB>
where
    D: squidscript_fw_core::native_runtime::NativeDisplaySink,
    FB: squidscript_fw_core::native_runtime::NativeFileBackend,
{
    fn request_flush(
        &mut self,
        display_sink: &mut D,
        file_backend: &mut FB,
    ) -> Result<(), &'static str>;

    fn step(&mut self) {}

    fn is_idle(&self) -> bool {
        true
    }
}

pub fn request_pending_display_flush<D, FB, F>(
    display_sink: &mut D,
    file_backend: &mut FB,
    display_flush: &mut F,
) -> Result<bool, &'static str>
where
    D: squidscript_fw_core::native_runtime::NativeDisplaySink,
    FB: squidscript_fw_core::native_runtime::NativeFileBackend,
    F: NativeDisplayFlushDriver<D, FB>,
{
    if display_sink.pending_refreshes() == 0 {
        return Ok(false);
    }
    match display_flush.request_flush(display_sink, file_backend) {
        Ok(()) => Ok(true),
        Err("display_flush_in_progress") => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod display_flush_driver_tests {
    use squidscript_fw_core::native_runtime::{NativeDisplaySink, NoopFileBackend};

    use super::{request_pending_display_flush, NativeDisplayFlushDriver};

    #[derive(Default)]
    struct PendingSink {
        pending: u32,
    }

    impl NativeDisplaySink for PendingSink {
        fn pending_refreshes(&self) -> u32 {
            self.pending
        }
    }

    struct RecordingFlush {
        calls: u32,
        result: Result<(), &'static str>,
    }

    impl Default for RecordingFlush {
        fn default() -> Self {
            Self {
                calls: 0,
                result: Ok(()),
            }
        }
    }

    impl NativeDisplayFlushDriver<PendingSink, NoopFileBackend> for RecordingFlush {
        fn request_flush(
            &mut self,
            _display_sink: &mut PendingSink,
            _file_backend: &mut NoopFileBackend,
        ) -> Result<(), &'static str> {
            self.calls += 1;
            self.result
        }
    }

    #[test]
    fn request_pending_display_flush_enqueues_pending_refresh_once() {
        let mut sink = PendingSink { pending: 1 };
        let mut files = NoopFileBackend;
        let mut flush = RecordingFlush::default();

        assert_eq!(
            request_pending_display_flush(&mut sink, &mut files, &mut flush),
            Ok(true)
        );
        assert_eq!(flush.calls, 1);
    }

    #[test]
    fn request_pending_display_flush_skips_idle_or_active_flush() {
        let mut idle_sink = PendingSink { pending: 0 };
        let mut files = NoopFileBackend;
        let mut flush = RecordingFlush::default();

        assert_eq!(
            request_pending_display_flush(&mut idle_sink, &mut files, &mut flush),
            Ok(false)
        );
        assert_eq!(flush.calls, 0);

        let mut pending_sink = PendingSink { pending: 1 };
        flush.result = Err("display_flush_in_progress");
        assert_eq!(
            request_pending_display_flush(&mut pending_sink, &mut files, &mut flush),
            Ok(false)
        );
        assert_eq!(flush.calls, 1);
    }

    #[test]
    fn request_pending_display_flush_reports_unexpected_errors() {
        let mut sink = PendingSink { pending: 1 };
        let mut files = NoopFileBackend;
        let mut flush = RecordingFlush {
            calls: 0,
            result: Err("display_flush_task_unavailable"),
        };

        assert_eq!(
            request_pending_display_flush(&mut sink, &mut files, &mut flush),
            Err("display_flush_task_unavailable")
        );
        assert_eq!(flush.calls, 1);
    }
}

#[cfg(feature = "x4-binbook")]
pub mod binbook_stack {
    use core::mem;

    use embedded_hal::{
        digital::{InputPin, OutputPin},
        spi::SpiDevice,
    };
    use embedded_hal_async::delay::DelayNs as AsyncDelayNs;
    use squidscript_fw_core::native_runtime::{
        DisplayLineOptions, DisplayRectOptions, DisplayResourceOptions, DisplayTextOptions,
        NativeDisplaySink,
    };
    use squidvm_core::value::Handle;

    pub use binbook_core::{Book, CompressionMethod, PixelFormat, ReadAt};
    pub use binbook_decompress::{decode_exact, DecodeError, PackBitsDecoder};
    pub use embedded_sd_storage::SdStorage;
    pub use gray2_render::{canonical_bits, CanonicalGray2, PlaneBits};
    pub use ssd1677_driver::{Command, PanelConfig, RefreshMode, Ssd1677};
    pub use xteink_x4_display::{
        buffers::RenderBuffers,
        framebuffer::{Gray2Color, Gray2Framebuffer, FRAMEBUFFER_BYTES},
        page_source::{decode_plane, read_x4_page, PlaneDecoder},
        panel::{panel_config, X4Panel},
        profile::{
            physical_to_logical, CHUNK_COUNT, CHUNK_ROWS, LOGICAL_HEIGHT, LOGICAL_WIDTH,
            PHYSICAL_HEIGHT, PHYSICAL_WIDTH, PLANE_BYTES, ROW_BYTES,
        },
        ui_render::{render_ui_bw, render_ui_gray_overlay},
        DisplayError, DisplayResult,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct NativeBinBookStackMetadata {
        pub logical_width: u16,
        pub logical_height: u16,
        pub physical_width: u16,
        pub physical_height: u16,
        pub plane_bytes: usize,
        pub framebuffer_bytes: usize,
        pub page_record_bytes: usize,
        pub display_error_bytes: usize,
        pub packbits_decoder_bytes: usize,
        pub ssd1677_write_ram_command: u8,
    }

    pub const fn native_binbook_stack_metadata() -> NativeBinBookStackMetadata {
        NativeBinBookStackMetadata {
            logical_width: LOGICAL_WIDTH,
            logical_height: LOGICAL_HEIGHT,
            physical_width: PHYSICAL_WIDTH,
            physical_height: PHYSICAL_HEIGHT,
            plane_bytes: PLANE_BYTES,
            framebuffer_bytes: FRAMEBUFFER_BYTES,
            page_record_bytes: binbook_core::PAGE_RECORD_SIZE,
            display_error_bytes: mem::size_of::<DisplayError>(),
            packbits_decoder_bytes: mem::size_of::<PackBitsDecoder>(),
            ssd1677_write_ram_command: Command::WRITE_RAM,
        }
    }

    pub struct X4FramebufferDisplaySink {
        framebuffer: Gray2Framebuffer,
        rendered_screen: [u8; 40],
        rendered_screen_len: usize,
        pending_refreshes: u32,
    }

    impl X4FramebufferDisplaySink {
        pub fn new() -> Self {
            Self {
                framebuffer: Gray2Framebuffer::new(),
                rendered_screen: [0; 40],
                rendered_screen_len: 0,
                pending_refreshes: 0,
            }
        }

        pub fn framebuffer(&self) -> &Gray2Framebuffer {
            &self.framebuffer
        }

        pub fn framebuffer_mut(&mut self) -> &mut Gray2Framebuffer {
            &mut self.framebuffer
        }

        pub fn rendered_screen(&self) -> Option<&str> {
            (self.rendered_screen_len > 0).then(|| {
                core::str::from_utf8(&self.rendered_screen[..self.rendered_screen_len])
                    .unwrap_or("")
            })
        }

        pub fn pending_refreshes(&self) -> u32 {
            self.pending_refreshes
        }

        pub fn take_pending_refreshes(&mut self) -> u32 {
            let count = self.pending_refreshes;
            self.pending_refreshes = 0;
            count
        }
    }

    impl Default for X4FramebufferDisplaySink {
        fn default() -> Self {
            Self::new()
        }
    }

    impl NativeDisplaySink for X4FramebufferDisplaySink {
        fn draw_clear(&mut self, color: u8) {
            self.framebuffer.clear(gray2_from_display_color(color));
        }

        fn draw_text(&mut self, text: &str, options: DisplayTextOptions<'_>) {
            if let Some(color) = options.background_color {
                fill_rect(
                    &mut self.framebuffer,
                    options.x,
                    options.y,
                    options.w,
                    options.h,
                    color,
                );
            }
            let color = options.text_color.unwrap_or(15);
            draw_text(
                &mut self.framebuffer,
                text,
                options.x,
                options.y,
                options.font_height,
                color,
            );
        }

        fn draw_rect(&mut self, options: DisplayRectOptions) {
            let Some(color) = options.fill_color.or(options.stroke_color) else {
                return;
            };
            fill_rect(
                &mut self.framebuffer,
                options.x,
                options.y,
                options.w,
                options.h,
                color,
            );
        }

        fn draw_line(&mut self, options: DisplayLineOptions) {
            let Some(color) = options.color else {
                return;
            };
            draw_line(
                &mut self.framebuffer,
                options.x1,
                options.y1,
                options.x2,
                options.y2,
                color,
            );
        }

        fn draw_image(&mut self, _path: &str, _options: DisplayResourceOptions) {}

        fn draw_resource(&mut self, _drawable: &str, _options: DisplayResourceOptions) {}

        fn screen_rendered(&mut self, name: &str) {
            let len = name.len().min(self.rendered_screen.len());
            self.rendered_screen[..len].copy_from_slice(&name.as_bytes()[..len]);
            self.rendered_screen_len = len;
            self.pending_refreshes = self.pending_refreshes.saturating_add(1);
        }

        fn pending_refreshes(&self) -> u32 {
            self.pending_refreshes
        }
    }

    const COMMAND_DISPLAY_MAX_DRAWS: usize = 64;
    const COMMAND_DISPLAY_TEXT_BYTES: usize = 512;
    const COMMAND_DISPLAY_SCREEN_BYTES: usize = 40;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum DisplayCommand {
        Clear {
            color: u8,
        },
        Text {
            offset: u16,
            len: u16,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            font_height: i32,
            text_color: Option<u8>,
            background_color: Option<u8>,
        },
        Rect {
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            fill_color: Option<u8>,
            stroke_color: Option<u8>,
        },
        Line {
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            color: Option<u8>,
        },
        Drawable {
            handle: Handle,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
        },
    }

    pub struct X4CommandDisplaySink {
        commands: [Option<DisplayCommand>; COMMAND_DISPLAY_MAX_DRAWS],
        command_len: usize,
        text: [u8; COMMAND_DISPLAY_TEXT_BYTES],
        text_len: usize,
        rendered_screen: [u8; COMMAND_DISPLAY_SCREEN_BYTES],
        rendered_screen_len: usize,
        pending_refreshes: u32,
        dropped_draws: u32,
    }

    #[derive(Clone)]
    pub struct X4DisplayFlushSnapshot {
        commands: [Option<DisplayCommand>; COMMAND_DISPLAY_MAX_DRAWS],
        command_len: usize,
        text: [u8; COMMAND_DISPLAY_TEXT_BYTES],
        text_len: usize,
        refreshes: u32,
    }

    impl X4DisplayFlushSnapshot {
        pub fn refreshes(&self) -> u32 {
            self.refreshes
        }

        pub fn recorded_draws(&self) -> usize {
            self.command_len
        }

        pub fn drawable_count(&self) -> usize {
            self.commands[..self.command_len]
                .iter()
                .flatten()
                .filter(|command| matches!(command, DisplayCommand::Drawable { .. }))
                .count()
        }

        pub fn drawable_handle(&self, index: usize) -> Option<Handle> {
            self.commands[..self.command_len]
                .iter()
                .flatten()
                .filter_map(|command| match command {
                    DisplayCommand::Drawable { handle, .. } => Some(*handle),
                    _ => None,
                })
                .nth(index)
        }

        pub fn render_into(&self, framebuffer: &mut Gray2Framebuffer) {
            render_display_commands(
                &self.commands[..self.command_len],
                &self.text[..self.text_len],
                framebuffer,
            );
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum X4CooperativeFlushStatus {
        Pending,
        ReadyToFlush,
    }

    pub struct X4CooperativeDisplayFlushJob {
        snapshot: X4DisplayFlushSnapshot,
        next_command: usize,
        max_commands_per_step: usize,
    }

    impl X4CooperativeDisplayFlushJob {
        pub fn new(snapshot: X4DisplayFlushSnapshot, max_commands_per_step: usize) -> Self {
            Self {
                snapshot,
                next_command: 0,
                max_commands_per_step: max_commands_per_step.max(1),
            }
        }

        pub fn refreshes(&self) -> u32 {
            self.snapshot.refreshes()
        }

        pub fn recorded_draws(&self) -> usize {
            self.snapshot.recorded_draws()
        }

        pub fn render_step(
            &mut self,
            framebuffer: &mut Gray2Framebuffer,
        ) -> X4CooperativeFlushStatus {
            if self.next_command < self.snapshot.command_len {
                let end = self
                    .next_command
                    .saturating_add(self.max_commands_per_step)
                    .min(self.snapshot.command_len);
                render_display_commands(
                    &self.snapshot.commands[self.next_command..end],
                    &self.snapshot.text[..self.snapshot.text_len],
                    framebuffer,
                );
                self.next_command = end;
            }
            if self.next_command < self.snapshot.command_len {
                X4CooperativeFlushStatus::Pending
            } else {
                X4CooperativeFlushStatus::ReadyToFlush
            }
        }
    }

    pub trait X4CooperativePanel {
        type Error;

        fn write_red_strip(&mut self, strip: u8) -> Result<(), Self::Error>;
        fn write_black_strip(&mut self, strip: u8) -> Result<(), Self::Error>;
        fn trigger_full_refresh(&mut self) -> Result<(), Self::Error>;
        fn is_busy(&mut self) -> Result<bool, Self::Error>;
    }

    pub trait X4CooperativePanelIo {
        type Error;

        fn write_red_strip<F>(&mut self, strip: u8, fill: F) -> Result<(), Self::Error>
        where
            F: FnMut(u16, &mut [u8; ROW_BYTES]);

        fn write_black_strip<F>(&mut self, strip: u8, fill: F) -> Result<(), Self::Error>
        where
            F: FnMut(u16, &mut [u8; ROW_BYTES]);

        fn trigger_full_refresh(&mut self) -> Result<(), Self::Error>;
        fn is_busy(&mut self) -> Result<bool, Self::Error>;
    }

    pub struct X4FramebufferPanelAdapter<'a, IO> {
        io: &'a mut IO,
        framebuffer: &'a Gray2Framebuffer,
    }

    impl<'a, IO> X4FramebufferPanelAdapter<'a, IO> {
        pub fn new(io: &'a mut IO, framebuffer: &'a Gray2Framebuffer) -> Self {
            Self { io, framebuffer }
        }
    }

    impl<IO> X4CooperativePanel for X4FramebufferPanelAdapter<'_, IO>
    where
        IO: X4CooperativePanelIo,
    {
        type Error = IO::Error;

        fn write_red_strip(&mut self, strip: u8) -> Result<(), Self::Error> {
            let framebuffer = self.framebuffer;
            self.io.write_red_strip(strip, |row, output| {
                fill_absolute_bw_row(framebuffer, strip, row, output);
            })
        }

        fn write_black_strip(&mut self, strip: u8) -> Result<(), Self::Error> {
            let framebuffer = self.framebuffer;
            self.io.write_black_strip(strip, |row, output| {
                fill_absolute_bw_row(framebuffer, strip, row, output);
            })
        }

        fn trigger_full_refresh(&mut self) -> Result<(), Self::Error> {
            self.io.trigger_full_refresh()
        }

        fn is_busy(&mut self) -> Result<bool, Self::Error> {
            self.io.is_busy()
        }
    }

    impl<SPI, DC, RST, BUSY> X4CooperativePanelIo for X4Panel<SPI, DC, RST, BUSY>
    where
        SPI: SpiDevice<u8>,
        DC: OutputPin,
        RST: OutputPin,
        BUSY: InputPin,
    {
        type Error = DisplayError;

        fn write_red_strip<F>(&mut self, strip: u8, fill: F) -> Result<(), Self::Error>
        where
            F: FnMut(u16, &mut [u8; ROW_BYTES]),
        {
            self.controller().set_window(
                0,
                u16::from(strip) * CHUNK_ROWS,
                PHYSICAL_WIDTH,
                CHUNK_ROWS,
            )?;
            self.controller()
                .write_red_frame_rows::<ROW_BYTES>(CHUNK_ROWS, fill)?;
            Ok(())
        }

        fn write_black_strip<F>(&mut self, strip: u8, fill: F) -> Result<(), Self::Error>
        where
            F: FnMut(u16, &mut [u8; ROW_BYTES]),
        {
            self.controller().set_window(
                0,
                u16::from(strip) * CHUNK_ROWS,
                PHYSICAL_WIDTH,
                CHUNK_ROWS,
            )?;
            self.controller()
                .write_frame_rows::<ROW_BYTES>(CHUNK_ROWS, fill)?;
            Ok(())
        }

        fn trigger_full_refresh(&mut self) -> Result<(), Self::Error> {
            self.controller().trigger_refresh(RefreshMode::Full)?;
            Ok(())
        }

        fn is_busy(&mut self) -> Result<bool, Self::Error> {
            Ok(self.controller().is_busy()?)
        }
    }

    fn fill_absolute_bw_row(
        framebuffer: &Gray2Framebuffer,
        strip: u8,
        row: u16,
        output: &mut [u8; ROW_BYTES],
    ) {
        output.fill(0xff);
        let phys_y = u16::from(strip) * CHUNK_ROWS + row;

        for phys_x in 0..PHYSICAL_WIDTH {
            let (logical_x, logical_y) = physical_to_logical(phys_x, phys_y);
            let gray = framebuffer.get_pixel(logical_x, logical_y);
            if gray.value() <= 1 {
                let ram_bit = usize::from(PHYSICAL_WIDTH - 1 - phys_x);
                let byte_idx = ram_bit / 8;
                let bit_idx = ram_bit % 8;
                output[byte_idx] &= !(0x80 >> bit_idx);
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum X4CooperativePanelFlushStatus {
        Pending,
        Complete,
    }

    enum X4PanelFlushPhase {
        RedRam { strip: u8 },
        BlackRam { strip: u8 },
        TriggerRefresh,
        WaitReady,
        Complete,
    }

    pub struct X4CooperativePanelFlushJob {
        strip_count: u8,
        phase: X4PanelFlushPhase,
    }

    impl X4CooperativePanelFlushJob {
        pub fn new(strip_count: u8) -> Self {
            Self {
                strip_count,
                phase: X4PanelFlushPhase::RedRam { strip: 0 },
            }
        }

        pub fn step<P: X4CooperativePanel>(
            &mut self,
            panel: &mut P,
        ) -> Result<X4CooperativePanelFlushStatus, P::Error> {
            loop {
                match self.phase {
                    X4PanelFlushPhase::RedRam { strip } => {
                        if strip < self.strip_count {
                            panel.write_red_strip(strip)?;
                            self.phase = X4PanelFlushPhase::RedRam {
                                strip: strip.saturating_add(1),
                            };
                            return Ok(X4CooperativePanelFlushStatus::Pending);
                        }
                        self.phase = X4PanelFlushPhase::BlackRam { strip: 0 };
                    }
                    X4PanelFlushPhase::BlackRam { strip } => {
                        if strip < self.strip_count {
                            panel.write_black_strip(strip)?;
                            self.phase = X4PanelFlushPhase::BlackRam {
                                strip: strip.saturating_add(1),
                            };
                            return Ok(X4CooperativePanelFlushStatus::Pending);
                        }
                        self.phase = X4PanelFlushPhase::TriggerRefresh;
                    }
                    X4PanelFlushPhase::TriggerRefresh => {
                        panel.trigger_full_refresh()?;
                        self.phase = X4PanelFlushPhase::WaitReady;
                        return Ok(X4CooperativePanelFlushStatus::Pending);
                    }
                    X4PanelFlushPhase::WaitReady => {
                        if panel.is_busy()? {
                            return Ok(X4CooperativePanelFlushStatus::Pending);
                        }
                        self.phase = X4PanelFlushPhase::Complete;
                        return Ok(X4CooperativePanelFlushStatus::Complete);
                    }
                    X4PanelFlushPhase::Complete => {
                        return Ok(X4CooperativePanelFlushStatus::Complete);
                    }
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum X4CooperativeDisplayTaskStatus {
        Idle,
        Pending,
        Complete,
    }

    pub struct X4CooperativeDisplayFlushTask {
        render_job: Option<X4CooperativeDisplayFlushJob>,
        panel_job: Option<X4CooperativePanelFlushJob>,
        max_commands_per_step: usize,
        strip_count: u8,
        active_refreshes: u32,
    }

    impl X4CooperativeDisplayFlushTask {
        pub fn new(max_commands_per_step: usize, strip_count: u8) -> Self {
            Self {
                render_job: None,
                panel_job: None,
                max_commands_per_step: max_commands_per_step.max(1),
                strip_count,
                active_refreshes: 0,
            }
        }

        pub fn is_active(&self) -> bool {
            self.render_job.is_some() || self.panel_job.is_some()
        }

        pub fn active_refreshes(&self) -> u32 {
            self.active_refreshes
        }

        pub fn request(&mut self, sink: &mut X4CommandDisplaySink) -> Result<(), &'static str> {
            if self.is_active() {
                return Err("display_flush_in_progress");
            }
            let Some(snapshot) = sink.pending_snapshot() else {
                return Err("display_flush_no_pending_refresh");
            };
            self.active_refreshes = snapshot.refreshes();
            self.render_job = Some(X4CooperativeDisplayFlushJob::new(
                snapshot,
                self.max_commands_per_step,
            ));
            self.panel_job = None;
            sink.mark_snapshot_enqueued();
            Ok(())
        }

        pub fn step<IO>(
            &mut self,
            framebuffer: &mut Gray2Framebuffer,
            io: &mut IO,
        ) -> Result<X4CooperativeDisplayTaskStatus, IO::Error>
        where
            IO: X4CooperativePanelIo,
        {
            if let Some(render_job) = self.render_job.as_mut() {
                if render_job.render_step(framebuffer) == X4CooperativeFlushStatus::ReadyToFlush {
                    self.render_job = None;
                    self.panel_job = Some(X4CooperativePanelFlushJob::new(self.strip_count));
                }
                return Ok(X4CooperativeDisplayTaskStatus::Pending);
            }

            let Some(panel_job) = self.panel_job.as_mut() else {
                return Ok(X4CooperativeDisplayTaskStatus::Idle);
            };
            let mut panel = X4FramebufferPanelAdapter::new(io, framebuffer);
            match panel_job.step(&mut panel)? {
                X4CooperativePanelFlushStatus::Pending => {
                    Ok(X4CooperativeDisplayTaskStatus::Pending)
                }
                X4CooperativePanelFlushStatus::Complete => {
                    self.panel_job = None;
                    self.active_refreshes = 0;
                    Ok(X4CooperativeDisplayTaskStatus::Complete)
                }
            }
        }
    }

    pub struct X4SnapshotPanelAdapter<'a, IO> {
        io: &'a mut IO,
        snapshot: &'a X4DisplayFlushSnapshot,
    }

    impl<'a, IO> X4SnapshotPanelAdapter<'a, IO> {
        pub fn new(io: &'a mut IO, snapshot: &'a X4DisplayFlushSnapshot) -> Self {
            Self { io, snapshot }
        }
    }

    impl<IO> X4CooperativePanel for X4SnapshotPanelAdapter<'_, IO>
    where
        IO: X4CooperativePanelIo,
    {
        type Error = IO::Error;

        fn write_red_strip(&mut self, strip: u8) -> Result<(), Self::Error> {
            let snapshot = self.snapshot;
            self.io.write_red_strip(strip, |row, output| {
                fill_snapshot_absolute_bw_row(snapshot, strip, row, output);
            })
        }

        fn write_black_strip(&mut self, strip: u8) -> Result<(), Self::Error> {
            let snapshot = self.snapshot;
            self.io.write_black_strip(strip, |row, output| {
                fill_snapshot_absolute_bw_row(snapshot, strip, row, output);
            })
        }

        fn trigger_full_refresh(&mut self) -> Result<(), Self::Error> {
            self.io.trigger_full_refresh()
        }

        fn is_busy(&mut self) -> Result<bool, Self::Error> {
            self.io.is_busy()
        }
    }

    pub struct X4StreamingDisplayFlushTask {
        snapshot: Option<X4DisplayFlushSnapshot>,
        panel_job: Option<X4CooperativePanelFlushJob>,
        strip_count: u8,
    }

    impl X4StreamingDisplayFlushTask {
        pub fn new(strip_count: u8) -> Self {
            Self {
                snapshot: None,
                panel_job: None,
                strip_count,
            }
        }

        pub fn is_active(&self) -> bool {
            self.snapshot.is_some() || self.panel_job.is_some()
        }

        pub fn request(&mut self, sink: &mut X4CommandDisplaySink) -> Result<(), &'static str> {
            if self.is_active() {
                return Err("display_flush_in_progress");
            }
            let Some(snapshot) = sink.pending_snapshot() else {
                return Err("display_flush_no_pending_refresh");
            };
            self.snapshot = Some(snapshot);
            self.panel_job = Some(X4CooperativePanelFlushJob::new(self.strip_count));
            sink.mark_snapshot_enqueued();
            Ok(())
        }

        pub fn step<IO>(&mut self, io: &mut IO) -> Result<X4CooperativeDisplayTaskStatus, IO::Error>
        where
            IO: X4CooperativePanelIo,
        {
            let (Some(snapshot), Some(panel_job)) =
                (self.snapshot.as_ref(), self.panel_job.as_mut())
            else {
                return Ok(X4CooperativeDisplayTaskStatus::Idle);
            };
            let mut panel = X4SnapshotPanelAdapter::new(io, snapshot);
            match panel_job.step(&mut panel)? {
                X4CooperativePanelFlushStatus::Pending => {
                    Ok(X4CooperativeDisplayTaskStatus::Pending)
                }
                X4CooperativePanelFlushStatus::Complete => {
                    self.snapshot = None;
                    self.panel_job = None;
                    Ok(X4CooperativeDisplayTaskStatus::Complete)
                }
            }
        }
    }

    pub trait X4DisplayFlusher {
        type Error;

        fn flush(&mut self, framebuffer: &Gray2Framebuffer) -> Result<(), Self::Error>;
    }

    pub struct X4PanelDisplayFlusher<PANEL, DELAY> {
        panel: PANEL,
        delay: DELAY,
        initialized: bool,
    }

    impl<PANEL, DELAY> X4PanelDisplayFlusher<PANEL, DELAY> {
        pub const fn new(panel: PANEL, delay: DELAY) -> Self {
            Self {
                panel,
                delay,
                initialized: false,
            }
        }

        pub fn panel_mut(&mut self) -> &mut PANEL {
            &mut self.panel
        }
    }

    impl<SPI, DC, RST, BUSY, DELAY> X4PanelDisplayFlusher<X4Panel<SPI, DC, RST, BUSY>, DELAY>
    where
        SPI: SpiDevice<u8>,
        DC: OutputPin,
        RST: OutputPin,
        BUSY: InputPin,
        DELAY: AsyncDelayNs,
    {
        pub async fn flush_bw(&mut self, framebuffer: &Gray2Framebuffer) -> DisplayResult<()> {
            if !self.initialized {
                self.panel.init_bw_async(&mut self.delay).await?;
                self.initialized = true;
            }
            render_ui_bw(&mut self.panel, framebuffer, &mut self.delay).await
        }
    }

    #[cfg(all(feature = "firmware-bin", target_arch = "riscv32"))]
    impl<SPI, DC, RST, BUSY, DELAY> X4DisplayFlusher
        for X4PanelDisplayFlusher<X4Panel<SPI, DC, RST, BUSY>, DELAY>
    where
        SPI: SpiDevice<u8>,
        DC: OutputPin,
        RST: OutputPin,
        BUSY: InputPin,
        DELAY: AsyncDelayNs,
    {
        type Error = DisplayError;

        fn flush(&mut self, framebuffer: &Gray2Framebuffer) -> Result<(), Self::Error> {
            embassy_futures::block_on(self.flush_bw(framebuffer))
        }
    }

    impl X4CommandDisplaySink {
        pub const fn new() -> Self {
            Self {
                commands: [None; COMMAND_DISPLAY_MAX_DRAWS],
                command_len: 0,
                text: [0; COMMAND_DISPLAY_TEXT_BYTES],
                text_len: 0,
                rendered_screen: [0; COMMAND_DISPLAY_SCREEN_BYTES],
                rendered_screen_len: 0,
                pending_refreshes: 0,
                dropped_draws: 0,
            }
        }

        pub fn rendered_screen(&self) -> Option<&str> {
            (self.rendered_screen_len > 0).then(|| {
                core::str::from_utf8(&self.rendered_screen[..self.rendered_screen_len])
                    .unwrap_or("")
            })
        }

        pub fn recorded_draws(&self) -> usize {
            self.command_len
        }

        pub fn dropped_draws(&self) -> u32 {
            self.dropped_draws
        }

        pub fn pending_refreshes(&self) -> u32 {
            self.pending_refreshes
        }

        pub fn pending_snapshot(&self) -> Option<X4DisplayFlushSnapshot> {
            (self.pending_refreshes > 0).then(|| X4DisplayFlushSnapshot {
                commands: self.commands,
                command_len: self.command_len,
                text: self.text,
                text_len: self.text_len,
                refreshes: self.pending_refreshes,
            })
        }

        pub fn mark_snapshot_enqueued(&mut self) {
            self.clear_recorded_draws();
            self.pending_refreshes = 0;
        }

        pub fn render_pending_into(&mut self, framebuffer: &mut Gray2Framebuffer) -> u32 {
            let refreshes = self.pending_refreshes;
            self.render_commands_into(framebuffer);
            self.clear_recorded_draws();
            self.pending_refreshes = 0;
            refreshes
        }

        pub fn flush_pending_with<F: X4DisplayFlusher>(
            &mut self,
            framebuffer: &mut Gray2Framebuffer,
            flusher: &mut F,
        ) -> Result<u32, F::Error> {
            let refreshes = self.pending_refreshes;
            if refreshes == 0 {
                return Ok(0);
            }
            self.render_commands_into(framebuffer);
            flusher.flush(framebuffer)?;
            self.clear_recorded_draws();
            self.pending_refreshes = 0;
            Ok(refreshes)
        }

        fn render_commands_into(&self, framebuffer: &mut Gray2Framebuffer) {
            render_display_commands(
                &self.commands[..self.command_len],
                &self.text[..self.text_len],
                framebuffer,
            );
        }

        fn push_command(&mut self, command: DisplayCommand) {
            let Some(slot) = self.commands.get_mut(self.command_len) else {
                self.dropped_draws = self.dropped_draws.saturating_add(1);
                return;
            };
            *slot = Some(command);
            self.command_len += 1;
        }

        fn push_text(&mut self, text: &str) -> Option<(u16, u16)> {
            let bytes = text.as_bytes();
            let available = self.text.len().saturating_sub(self.text_len);
            if bytes.len() > available || self.text_len > usize::from(u16::MAX) {
                self.dropped_draws = self.dropped_draws.saturating_add(1);
                return None;
            }
            let offset = self.text_len;
            let len = bytes.len().min(usize::from(u16::MAX));
            self.text[offset..offset + len].copy_from_slice(&bytes[..len]);
            self.text_len += len;
            Some((offset as u16, len as u16))
        }

        fn clear_recorded_draws(&mut self) {
            for slot in &mut self.commands[..self.command_len] {
                *slot = None;
            }
            self.command_len = 0;
            self.text_len = 0;
        }
    }

    impl Default for X4CommandDisplaySink {
        fn default() -> Self {
            Self::new()
        }
    }

    impl NativeDisplaySink for X4CommandDisplaySink {
        fn draw_clear(&mut self, color: u8) {
            self.clear_recorded_draws();
            self.push_command(DisplayCommand::Clear { color });
        }

        fn draw_text(&mut self, text: &str, options: DisplayTextOptions<'_>) {
            let Some((offset, len)) = self.push_text(text) else {
                return;
            };
            self.push_command(DisplayCommand::Text {
                offset,
                len,
                x: options.x,
                y: options.y,
                w: options.w,
                h: options.h,
                font_height: options.font_height,
                text_color: options.text_color,
                background_color: options.background_color,
            });
        }

        fn draw_rect(&mut self, options: DisplayRectOptions) {
            self.push_command(DisplayCommand::Rect {
                x: options.x,
                y: options.y,
                w: options.w,
                h: options.h,
                fill_color: options.fill_color,
                stroke_color: options.stroke_color,
            });
        }

        fn draw_line(&mut self, options: DisplayLineOptions) {
            self.push_command(DisplayCommand::Line {
                x1: options.x1,
                y1: options.y1,
                x2: options.x2,
                y2: options.y2,
                color: options.color,
            });
        }

        fn draw_image(&mut self, _path: &str, _options: DisplayResourceOptions) {
            self.dropped_draws = self.dropped_draws.saturating_add(1);
        }

        fn draw_resource(&mut self, _drawable: &str, _options: DisplayResourceOptions) {
            self.dropped_draws = self.dropped_draws.saturating_add(1);
        }

        fn draw_drawable(&mut self, handle: Handle, options: DisplayResourceOptions) {
            self.push_command(DisplayCommand::Drawable {
                handle,
                x: options.x,
                y: options.y,
                w: options.w,
                h: options.h,
            });
        }

        fn screen_rendered(&mut self, name: &str) {
            let len = name.len().min(self.rendered_screen.len());
            self.rendered_screen[..len].copy_from_slice(&name.as_bytes()[..len]);
            self.rendered_screen_len = len;
            self.pending_refreshes = self.pending_refreshes.saturating_add(1);
        }

        fn pending_refreshes(&self) -> u32 {
            self.pending_refreshes
        }

        fn recorded_draws(&self) -> u32 {
            self.command_len as u32
        }

        fn dropped_draws(&self) -> u32 {
            self.dropped_draws
        }
    }

    fn gray2_from_display_color(color: u8) -> Gray2Color {
        match color {
            0 => Gray2Color::WHITE,
            1..=4 => Gray2Color::LIGHT_GRAY,
            5..=10 => Gray2Color::DARK_GRAY,
            _ => Gray2Color::BLACK,
        }
    }

    fn render_display_commands(
        commands: &[Option<DisplayCommand>],
        text_store: &[u8],
        framebuffer: &mut Gray2Framebuffer,
    ) {
        for command in commands.iter().flatten().copied() {
            match command {
                DisplayCommand::Clear { color } => {
                    framebuffer.clear(gray2_from_display_color(color));
                }
                DisplayCommand::Text {
                    offset,
                    len,
                    x,
                    y,
                    w,
                    h,
                    font_height,
                    text_color,
                    background_color,
                } => {
                    if let Some(color) = background_color {
                        fill_rect(framebuffer, x, y, w, h, color);
                    }
                    let start = usize::from(offset);
                    let end = start.saturating_add(usize::from(len)).min(text_store.len());
                    if let Ok(text) = core::str::from_utf8(&text_store[start..end]) {
                        draw_text(
                            framebuffer,
                            text,
                            x,
                            y,
                            font_height,
                            text_color.unwrap_or(15),
                        );
                    }
                }
                DisplayCommand::Rect {
                    x,
                    y,
                    w,
                    h,
                    fill_color,
                    stroke_color,
                } => {
                    if let Some(color) = fill_color.or(stroke_color) {
                        fill_rect(framebuffer, x, y, w, h, color);
                    }
                }
                DisplayCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color,
                } => {
                    if let Some(color) = color {
                        draw_line(framebuffer, x1, y1, x2, y2, color);
                    }
                }
                DisplayCommand::Drawable { .. } => {}
            }
        }
    }

    fn fill_snapshot_absolute_bw_row(
        snapshot: &X4DisplayFlushSnapshot,
        strip: u8,
        row: u16,
        output: &mut [u8; ROW_BYTES],
    ) {
        output.fill(0xff);
        let phys_y = u16::from(strip) * CHUNK_ROWS + row;

        for phys_x in 0..PHYSICAL_WIDTH {
            let (logical_x, logical_y) = physical_to_logical(phys_x, phys_y);
            let gray = snapshot_pixel(snapshot, logical_x, logical_y);
            if gray.value() <= 1 {
                let ram_bit = usize::from(PHYSICAL_WIDTH - 1 - phys_x);
                let byte_idx = ram_bit / 8;
                let bit_idx = ram_bit % 8;
                output[byte_idx] &= !(0x80 >> bit_idx);
            }
        }
    }

    fn snapshot_pixel(snapshot: &X4DisplayFlushSnapshot, x: u16, y: u16) -> Gray2Color {
        let mut color = Gray2Color::WHITE;
        let x = i32::from(x);
        let y = i32::from(y);
        for command in snapshot.commands[..snapshot.command_len]
            .iter()
            .flatten()
            .copied()
        {
            match command {
                DisplayCommand::Clear { color: clear } => {
                    color = gray2_from_display_color(clear);
                }
                DisplayCommand::Text {
                    offset,
                    len,
                    x: text_x,
                    y: text_y,
                    w,
                    h,
                    font_height,
                    text_color,
                    background_color,
                } => {
                    if let Some(background) = background_color {
                        if point_in_rect(x, y, text_x, text_y, w, h) {
                            color = gray2_from_display_color(background);
                        }
                    }
                    let start = usize::from(offset);
                    let end = start
                        .saturating_add(usize::from(len))
                        .min(snapshot.text_len);
                    if text_pixel_set(
                        &snapshot.text[start..end],
                        x,
                        y,
                        text_x,
                        text_y,
                        font_height,
                    ) {
                        color = gray2_from_display_color(text_color.unwrap_or(15));
                    }
                }
                DisplayCommand::Rect {
                    x: rect_x,
                    y: rect_y,
                    w,
                    h,
                    fill_color,
                    stroke_color,
                } => {
                    if let Some(rect_color) = fill_color.or(stroke_color) {
                        if point_in_rect(x, y, rect_x, rect_y, w, h) {
                            color = gray2_from_display_color(rect_color);
                        }
                    }
                }
                DisplayCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color: line_color,
                } => {
                    if let Some(line_color) = line_color {
                        if point_on_line(x, y, x1, y1, x2, y2) {
                            color = gray2_from_display_color(line_color);
                        }
                    }
                }
                DisplayCommand::Drawable { .. } => {}
            }
        }
        color
    }

    fn point_in_rect(px: i32, py: i32, x: i32, y: i32, w: i32, h: i32) -> bool {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = x.saturating_add(w).clamp(0, i32::from(LOGICAL_WIDTH));
        let y1 = y.saturating_add(h).clamp(0, i32::from(LOGICAL_HEIGHT));
        px >= x0 && px < x1 && py >= y0 && py < y1
    }

    fn text_pixel_set(text: &[u8], px: i32, py: i32, x: i32, y: i32, font_height: i32) -> bool {
        let scale = (font_height / 7).max(1);
        if py < y || py >= y.saturating_add(7 * scale) {
            return false;
        }
        let mut cursor_x = x;
        for byte in text.iter().copied() {
            if glyph_pixel_set(byte, px, py, cursor_x, y, scale) {
                return true;
            }
            cursor_x = cursor_x.saturating_add(6 * scale);
        }
        false
    }

    fn glyph_pixel_set(byte: u8, px: i32, py: i32, x: i32, y: i32, scale: i32) -> bool {
        let rel_x = px - x;
        let rel_y = py - y;
        if rel_x < 0 || rel_y < 0 || rel_x >= 5 * scale || rel_y >= 7 * scale {
            return false;
        }
        let col = rel_x / scale;
        let row = rel_y / scale;
        let glyph = glyph_5x7(byte);
        glyph[row as usize] & (0b10000 >> col) != 0
    }

    fn point_on_line(px: i32, py: i32, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
        let mut x = x1;
        let mut y = y1;
        let dx = (x2 - x1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let dy = -(y2 - y1).abs();
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if x == px && y == py {
                return true;
            }
            if x == x2 && y == y2 {
                return false;
            }
            let e2 = err.saturating_mul(2);
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn fill_rect(framebuffer: &mut Gray2Framebuffer, x: i32, y: i32, w: i32, h: i32, color: u8) {
        let color = gray2_from_display_color(color);
        let x0 = x.max(0) as u16;
        let y0 = y.max(0) as u16;
        let x1 = x.saturating_add(w).clamp(0, i32::from(LOGICAL_WIDTH)) as u16;
        let y1 = y.saturating_add(h).clamp(0, i32::from(LOGICAL_HEIGHT)) as u16;
        for py in y0..y1 {
            for px in x0..x1 {
                framebuffer.set_pixel(px, py, color);
            }
        }
    }

    fn draw_line(
        framebuffer: &mut Gray2Framebuffer,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u8,
    ) {
        let color = gray2_from_display_color(color);
        let mut x = x1;
        let mut y = y1;
        let dx = (x2 - x1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let dy = -(y2 - y1).abs();
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if x >= 0 && x < i32::from(LOGICAL_WIDTH) && y >= 0 && y < i32::from(LOGICAL_HEIGHT) {
                framebuffer.set_pixel(x as u16, y as u16, color);
            }
            if x == x2 && y == y2 {
                break;
            }
            let e2 = err.saturating_mul(2);
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn draw_text(
        framebuffer: &mut Gray2Framebuffer,
        text: &str,
        x: i32,
        y: i32,
        font_height: i32,
        color: u8,
    ) {
        let scale = (font_height / 7).max(1);
        let color = gray2_from_display_color(color);
        let mut cursor_x = x;
        for byte in text.bytes() {
            draw_glyph(framebuffer, byte, cursor_x, y, scale, color);
            cursor_x = cursor_x.saturating_add(6 * scale);
        }
    }

    fn draw_glyph(
        framebuffer: &mut Gray2Framebuffer,
        byte: u8,
        x: i32,
        y: i32,
        scale: i32,
        color: Gray2Color,
    ) {
        let glyph = glyph_5x7(byte);
        for (row, bits) in glyph.iter().copied().enumerate() {
            for col in 0..5 {
                if bits & (0b10000 >> col) == 0 {
                    continue;
                }
                fill_scaled_pixel(
                    framebuffer,
                    x + col * scale,
                    y + row as i32 * scale,
                    scale,
                    color,
                );
            }
        }
    }

    fn fill_scaled_pixel(
        framebuffer: &mut Gray2Framebuffer,
        x: i32,
        y: i32,
        scale: i32,
        color: Gray2Color,
    ) {
        for py in y..y.saturating_add(scale) {
            for px in x..x.saturating_add(scale) {
                if px >= 0
                    && px < i32::from(LOGICAL_WIDTH)
                    && py >= 0
                    && py < i32::from(LOGICAL_HEIGHT)
                {
                    framebuffer.set_pixel(px as u16, py as u16, color);
                }
            }
        }
    }

    fn glyph_5x7(byte: u8) -> [u8; 7] {
        match byte.to_ascii_uppercase() {
            b'A' => [
                0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
            ],
            b'B' => [
                0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
            ],
            b'C' => [
                0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
            ],
            b'D' => [
                0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
            ],
            b'E' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
            ],
            b'F' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
            ],
            b'G' => [
                0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
            ],
            b'H' => [
                0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
            ],
            b'I' => [
                0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
            ],
            b'J' => [
                0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
            ],
            b'K' => [
                0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
            ],
            b'L' => [
                0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
            ],
            b'M' => [
                0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
            ],
            b'N' => [
                0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
            ],
            b'O' => [
                0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
            ],
            b'P' => [
                0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
            ],
            b'Q' => [
                0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
            ],
            b'R' => [
                0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
            ],
            b'S' => [
                0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
            ],
            b'T' => [
                0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
            ],
            b'U' => [
                0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
            ],
            b'V' => [
                0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
            ],
            b'W' => [
                0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
            ],
            b'X' => [
                0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
            ],
            b'Y' => [
                0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
            ],
            b'Z' => [
                0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
            ],
            b'0' => [
                0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
            ],
            b'1' => [
                0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
            ],
            b'2' => [
                0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
            ],
            b'3' => [
                0b11110, 0b00001, 0b00001, 0b00110, 0b00001, 0b00001, 0b11110,
            ],
            b'4' => [
                0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
            ],
            b'5' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
            ],
            b'6' => [
                0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
            ],
            b'7' => [
                0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
            ],
            b'8' => [
                0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
            ],
            b'9' => [
                0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
            ],
            b' ' => [0, 0, 0, 0, 0, 0, 0],
            _ => [0b11111, 0b10001, 0b00010, 0b00100, 0b00100, 0, 0b00100],
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use squidscript_fw_core::native_runtime::{
            DisplayRectOptions, DisplayResourceOptions, DisplayTextOptions, NativeDisplaySink,
        };
        use squidvm_core::value::{Handle, HandleKind};

        #[test]
        fn imports_current_x4_binbook_stack_crates() {
            let metadata = native_binbook_stack_metadata();
            let framebuffer = Gray2Framebuffer::new();

            assert_eq!(metadata.logical_width, 480);
            assert_eq!(metadata.logical_height, 800);
            assert_eq!(metadata.physical_width, 800);
            assert_eq!(metadata.physical_height, 480);
            assert_eq!(metadata.plane_bytes, 48_000);
            assert_eq!(metadata.framebuffer_bytes, 96_000);
            assert_eq!(metadata.ssd1677_write_ram_command, Command::WRITE_RAM);
            assert_eq!(framebuffer.as_bytes().len(), FRAMEBUFFER_BYTES);
            assert_eq!(framebuffer.get_pixel(0, 0), Gray2Color::WHITE);
            assert_eq!(
                canonical_bits(CanonicalGray2::White),
                PlaneBits {
                    red_active: false,
                    black_active: false,
                }
            );
            assert!(matches!(
                RefreshMode::StagedGrayscale,
                RefreshMode::StagedGrayscale
            ));
            assert!(metadata.page_record_bytes >= 128);
        }

        #[test]
        fn framebuffer_display_sink_tracks_draws_and_screen_render_boundaries() {
            let mut sink = X4FramebufferDisplaySink::new();

            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.screen_rendered("main");

            assert_eq!(sink.framebuffer().get_pixel(0, 0), Gray2Color::WHITE);
            assert_eq!(sink.framebuffer().get_pixel(2, 3), Gray2Color::BLACK);
            assert_eq!(sink.framebuffer().get_pixel(5, 7), Gray2Color::BLACK);
            assert_eq!(sink.rendered_screen(), Some("main"));
            assert_eq!(sink.pending_refreshes(), 1);
        }

        #[test]
        fn framebuffer_display_sink_draws_text_pixels() {
            let mut sink = X4FramebufferDisplaySink::new();

            sink.draw_clear(0);
            sink.draw_text(
                "A",
                DisplayTextOptions {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0,
                    font_height: 20,
                    text_color: Some(15),
                    background_color: None,
                    align: None,
                    valign: None,
                },
            );

            assert_eq!(sink.framebuffer().get_pixel(2, 0), Gray2Color::BLACK);
            assert_eq!(sink.framebuffer().get_pixel(0, 0), Gray2Color::WHITE);
        }

        #[test]
        fn command_display_sink_does_not_retain_full_framebuffer() {
            assert!(mem::size_of::<X4CommandDisplaySink>() < FRAMEBUFFER_BYTES / 8);
        }

        #[test]
        fn command_display_sink_tracks_refreshes_without_framebuffer() {
            let mut sink = X4CommandDisplaySink::new();

            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.screen_rendered("main");

            assert_eq!(sink.rendered_screen(), Some("main"));
            assert_eq!(sink.pending_refreshes(), 1);
            assert_eq!(sink.recorded_draws(), 2);
        }

        #[test]
        fn command_display_sink_renders_into_caller_owned_framebuffer() {
            let mut sink = X4CommandDisplaySink::new();
            let mut framebuffer = Gray2Framebuffer::new();

            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.draw_text(
                "A",
                DisplayTextOptions {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0,
                    font_height: 20,
                    text_color: Some(15),
                    background_color: None,
                    align: None,
                    valign: None,
                },
            );
            sink.screen_rendered("main");

            assert_eq!(sink.render_pending_into(&mut framebuffer), 1);
            assert_eq!(framebuffer.get_pixel(0, 0), Gray2Color::WHITE);
            assert_eq!(framebuffer.get_pixel(2, 3), Gray2Color::BLACK);
            assert_eq!(framebuffer.get_pixel(2, 0), Gray2Color::BLACK);
            assert_eq!(sink.pending_refreshes(), 0);
            assert_eq!(sink.recorded_draws(), 0);
        }

        #[test]
        fn command_display_sink_flush_clears_commands_only_after_success() {
            struct RecordingFlusher {
                fail: bool,
                calls: u32,
                observed: Gray2Color,
            }

            impl X4DisplayFlusher for RecordingFlusher {
                type Error = ();

                fn flush(&mut self, framebuffer: &Gray2Framebuffer) -> Result<(), Self::Error> {
                    self.calls += 1;
                    self.observed = framebuffer.get_pixel(2, 3);
                    if self.fail {
                        Err(())
                    } else {
                        Ok(())
                    }
                }
            }

            let mut sink = X4CommandDisplaySink::new();
            let mut framebuffer = Gray2Framebuffer::new();
            let mut flusher = RecordingFlusher {
                fail: true,
                calls: 0,
                observed: Gray2Color::WHITE,
            };

            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.screen_rendered("main");

            assert_eq!(
                sink.flush_pending_with(&mut framebuffer, &mut flusher),
                Err(())
            );
            assert_eq!(flusher.calls, 1);
            assert_eq!(flusher.observed, Gray2Color::BLACK);
            assert_eq!(sink.pending_refreshes(), 1);
            assert_eq!(sink.recorded_draws(), 2);

            flusher.fail = false;
            assert_eq!(
                sink.flush_pending_with(&mut framebuffer, &mut flusher),
                Ok(1)
            );
            assert_eq!(flusher.calls, 2);
            assert_eq!(sink.pending_refreshes(), 0);
            assert_eq!(sink.recorded_draws(), 0);
        }

        #[test]
        fn command_display_sink_snapshots_pending_commands_before_async_flush() {
            let mut sink = X4CommandDisplaySink::new();
            let mut framebuffer = Gray2Framebuffer::new();

            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.screen_rendered("main");

            let snapshot = sink.pending_snapshot().expect("pending snapshot");

            assert_eq!(snapshot.refreshes(), 1);
            assert_eq!(snapshot.recorded_draws(), 2);
            assert_eq!(sink.pending_refreshes(), 1);
            assert_eq!(sink.recorded_draws(), 2);

            snapshot.render_into(&mut framebuffer);
            assert_eq!(framebuffer.get_pixel(2, 3), Gray2Color::BLACK);

            sink.mark_snapshot_enqueued();
            assert_eq!(sink.pending_refreshes(), 0);
            assert_eq!(sink.recorded_draws(), 0);
        }

        #[test]
        fn command_display_sink_snapshots_drawable_handles() {
            let mut sink = X4CommandDisplaySink::new();
            let drawable = Handle::new(HandleKind::Drawable, 7);

            sink.draw_drawable(
                drawable,
                DisplayResourceOptions {
                    x: 1,
                    y: 2,
                    w: 3,
                    h: 4,
                },
            );
            sink.screen_rendered("main");

            let snapshot = sink.pending_snapshot().expect("pending snapshot");

            assert_eq!(snapshot.recorded_draws(), 1);
            assert_eq!(snapshot.drawable_count(), 1);
            assert_eq!(snapshot.drawable_handle(0), Some(drawable));
            assert_eq!(sink.dropped_draws(), 0);
        }

        #[test]
        fn cooperative_display_flush_job_renders_snapshot_in_bounded_steps() {
            let mut sink = X4CommandDisplaySink::new();
            let mut framebuffer = Gray2Framebuffer::new();

            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.screen_rendered("main");

            let snapshot = sink.pending_snapshot().expect("pending snapshot");
            let mut job = X4CooperativeDisplayFlushJob::new(snapshot, 1);

            assert_eq!(
                job.render_step(&mut framebuffer),
                X4CooperativeFlushStatus::Pending
            );
            assert_eq!(framebuffer.get_pixel(2, 3), Gray2Color::WHITE);
            assert_eq!(
                job.render_step(&mut framebuffer),
                X4CooperativeFlushStatus::ReadyToFlush
            );
            assert_eq!(framebuffer.get_pixel(2, 3), Gray2Color::BLACK);
            assert_eq!(job.refreshes(), 1);
            assert_eq!(job.recorded_draws(), 2);
        }

        #[test]
        fn cooperative_panel_flush_steps_through_ram_writes_and_busy_polls() {
            #[derive(Default)]
            struct MockPanel {
                calls: Vec<&'static str>,
                busy: [bool; 2],
                busy_index: usize,
            }

            impl X4CooperativePanel for MockPanel {
                type Error = ();

                fn write_red_strip(&mut self, strip: u8) -> Result<(), Self::Error> {
                    assert_eq!(strip, self.calls.len() as u8);
                    self.calls.push("red");
                    Ok(())
                }

                fn write_black_strip(&mut self, strip: u8) -> Result<(), Self::Error> {
                    assert_eq!(strip, self.calls.len().saturating_sub(2) as u8);
                    self.calls.push("black");
                    Ok(())
                }

                fn trigger_full_refresh(&mut self) -> Result<(), Self::Error> {
                    self.calls.push("trigger");
                    Ok(())
                }

                fn is_busy(&mut self) -> Result<bool, Self::Error> {
                    let busy = self.busy[self.busy_index];
                    self.busy_index += 1;
                    self.calls.push("busy");
                    Ok(busy)
                }
            }

            let mut panel = MockPanel {
                busy: [true, false],
                ..MockPanel::default()
            };
            let mut job = X4CooperativePanelFlushJob::new(2);

            assert_eq!(
                job.step(&mut panel),
                Ok(X4CooperativePanelFlushStatus::Pending)
            );
            assert_eq!(panel.calls, ["red"]);
            assert_eq!(
                job.step(&mut panel),
                Ok(X4CooperativePanelFlushStatus::Pending)
            );
            assert_eq!(panel.calls, ["red", "red"]);
            assert_eq!(
                job.step(&mut panel),
                Ok(X4CooperativePanelFlushStatus::Pending)
            );
            assert_eq!(panel.calls, ["red", "red", "black"]);
            assert_eq!(
                job.step(&mut panel),
                Ok(X4CooperativePanelFlushStatus::Pending)
            );
            assert_eq!(panel.calls, ["red", "red", "black", "black"]);
            assert_eq!(
                job.step(&mut panel),
                Ok(X4CooperativePanelFlushStatus::Pending)
            );
            assert_eq!(panel.calls, ["red", "red", "black", "black", "trigger"]);
            assert_eq!(
                job.step(&mut panel),
                Ok(X4CooperativePanelFlushStatus::Pending)
            );
            assert_eq!(
                job.step(&mut panel),
                Ok(X4CooperativePanelFlushStatus::Complete)
            );
            assert_eq!(
                panel.calls,
                ["red", "red", "black", "black", "trigger", "busy", "busy"]
            );
        }

        #[test]
        fn framebuffer_panel_adapter_writes_absolute_bw_strips_from_framebuffer() {
            #[derive(Default)]
            struct MockPanelIo {
                rows: Vec<(&'static str, u8, u16, u8)>,
                triggered: bool,
                busy: bool,
            }

            impl X4CooperativePanelIo for MockPanelIo {
                type Error = ();

                fn write_red_strip<F>(&mut self, strip: u8, mut fill: F) -> Result<(), Self::Error>
                where
                    F: FnMut(u16, &mut [u8; ROW_BYTES]),
                {
                    let mut row = [0xff; ROW_BYTES];
                    for y in 0..CHUNK_ROWS {
                        fill(y, &mut row);
                        self.rows.push(("red", strip, y, row[0]));
                    }
                    Ok(())
                }

                fn write_black_strip<F>(
                    &mut self,
                    strip: u8,
                    mut fill: F,
                ) -> Result<(), Self::Error>
                where
                    F: FnMut(u16, &mut [u8; ROW_BYTES]),
                {
                    let mut row = [0xff; ROW_BYTES];
                    for y in 0..CHUNK_ROWS {
                        fill(y, &mut row);
                        self.rows.push(("black", strip, y, row[0]));
                    }
                    Ok(())
                }

                fn trigger_full_refresh(&mut self) -> Result<(), Self::Error> {
                    self.triggered = true;
                    Ok(())
                }

                fn is_busy(&mut self) -> Result<bool, Self::Error> {
                    Ok(self.busy)
                }
            }

            let mut framebuffer = Gray2Framebuffer::new();
            framebuffer.set_pixel(2, 3, Gray2Color::BLACK);
            let mut io = MockPanelIo::default();
            {
                let mut adapter = X4FramebufferPanelAdapter::new(&mut io, &framebuffer);

                X4CooperativePanel::write_red_strip(&mut adapter, 0).unwrap();
                X4CooperativePanel::write_black_strip(&mut adapter, 0).unwrap();
                X4CooperativePanel::trigger_full_refresh(&mut adapter).unwrap();
                assert!(!X4CooperativePanel::is_busy(&mut adapter).unwrap());
            }

            assert_eq!(io.rows.len(), usize::from(CHUNK_ROWS) * 2);
            assert!(io.rows.contains(&("red", 0, 2, 0xef)));
            assert!(io.rows.contains(&("black", 0, 2, 0xef)));
            assert!(io.triggered);
        }

        #[test]
        fn cooperative_display_task_renders_then_steps_panel_without_retaining_framebuffer() {
            #[derive(Default)]
            struct MockPanelIo {
                calls: Vec<&'static str>,
                busy: bool,
            }

            impl X4CooperativePanelIo for MockPanelIo {
                type Error = ();

                fn write_red_strip<F>(&mut self, _strip: u8, _fill: F) -> Result<(), Self::Error>
                where
                    F: FnMut(u16, &mut [u8; ROW_BYTES]),
                {
                    self.calls.push("red");
                    Ok(())
                }

                fn write_black_strip<F>(&mut self, _strip: u8, _fill: F) -> Result<(), Self::Error>
                where
                    F: FnMut(u16, &mut [u8; ROW_BYTES]),
                {
                    self.calls.push("black");
                    Ok(())
                }

                fn trigger_full_refresh(&mut self) -> Result<(), Self::Error> {
                    self.calls.push("trigger");
                    Ok(())
                }

                fn is_busy(&mut self) -> Result<bool, Self::Error> {
                    self.calls.push("busy");
                    Ok(self.busy)
                }
            }

            let mut sink = X4CommandDisplaySink::new();
            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.screen_rendered("main");

            let mut task = X4CooperativeDisplayFlushTask::new(1, 1);
            assert_eq!(task.request(&mut sink), Ok(()));
            assert_eq!(sink.pending_refreshes(), 0);
            assert_eq!(sink.recorded_draws(), 0);

            let mut framebuffer = Gray2Framebuffer::new();
            let mut io = MockPanelIo::default();

            assert_eq!(
                task.step(&mut framebuffer, &mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert_eq!(io.calls, Vec::<&'static str>::new());
            assert_eq!(framebuffer.get_pixel(2, 3), Gray2Color::WHITE);

            assert_eq!(
                task.step(&mut framebuffer, &mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert_eq!(io.calls, Vec::<&'static str>::new());
            assert_eq!(framebuffer.get_pixel(2, 3), Gray2Color::BLACK);

            assert_eq!(
                task.step(&mut framebuffer, &mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert_eq!(io.calls, ["red"]);
            assert_eq!(
                task.step(&mut framebuffer, &mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert_eq!(io.calls, ["red", "black"]);
            assert_eq!(
                task.step(&mut framebuffer, &mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert_eq!(io.calls, ["red", "black", "trigger"]);
            assert_eq!(
                task.step(&mut framebuffer, &mut io),
                Ok(X4CooperativeDisplayTaskStatus::Complete)
            );
            assert_eq!(io.calls, ["red", "black", "trigger", "busy"]);
            assert!(!task.is_active());
        }

        #[test]
        fn cooperative_streaming_display_task_writes_rows_without_framebuffer() {
            #[derive(Default)]
            struct MockPanelIo {
                rows: Vec<(&'static str, u8, u16, u8)>,
                calls: Vec<&'static str>,
                busy: bool,
            }

            impl X4CooperativePanelIo for MockPanelIo {
                type Error = ();

                fn write_red_strip<F>(&mut self, strip: u8, mut fill: F) -> Result<(), Self::Error>
                where
                    F: FnMut(u16, &mut [u8; ROW_BYTES]),
                {
                    self.calls.push("red");
                    let mut row = [0xff; ROW_BYTES];
                    for y in 0..CHUNK_ROWS {
                        fill(y, &mut row);
                        self.rows.push(("red", strip, y, row[0]));
                    }
                    Ok(())
                }

                fn write_black_strip<F>(
                    &mut self,
                    strip: u8,
                    mut fill: F,
                ) -> Result<(), Self::Error>
                where
                    F: FnMut(u16, &mut [u8; ROW_BYTES]),
                {
                    self.calls.push("black");
                    let mut row = [0xff; ROW_BYTES];
                    for y in 0..CHUNK_ROWS {
                        fill(y, &mut row);
                        self.rows.push(("black", strip, y, row[0]));
                    }
                    Ok(())
                }

                fn trigger_full_refresh(&mut self) -> Result<(), Self::Error> {
                    self.calls.push("trigger");
                    Ok(())
                }

                fn is_busy(&mut self) -> Result<bool, Self::Error> {
                    self.calls.push("busy");
                    Ok(self.busy)
                }
            }

            let mut sink = X4CommandDisplaySink::new();
            sink.draw_clear(0);
            sink.draw_rect(DisplayRectOptions {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
                fill_color: Some(15),
                stroke_color: None,
            });
            sink.screen_rendered("main");

            let mut task = X4StreamingDisplayFlushTask::new(1);
            assert_eq!(task.request(&mut sink), Ok(()));
            assert_eq!(sink.pending_refreshes(), 0);
            assert_eq!(sink.recorded_draws(), 0);

            let mut io = MockPanelIo::default();
            assert_eq!(
                task.step(&mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert!(io.rows.contains(&("red", 0, 2, 0xe0)));
            assert_eq!(
                task.step(&mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert!(io.rows.contains(&("black", 0, 2, 0xe0)));
            assert_eq!(
                task.step(&mut io),
                Ok(X4CooperativeDisplayTaskStatus::Pending)
            );
            assert_eq!(io.calls, ["red", "black", "trigger"]);
            assert_eq!(
                task.step(&mut io),
                Ok(X4CooperativeDisplayTaskStatus::Complete)
            );
            assert_eq!(io.calls, ["red", "black", "trigger", "busy"]);
            assert!(!task.is_active());
        }
    }
}
