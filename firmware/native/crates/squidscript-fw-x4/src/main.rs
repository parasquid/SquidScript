#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]

#[cfg(target_arch = "riscv32")]
use esp_backtrace as _;

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use esp_println::println;

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "vm-radio-measure",
        feature = "native-radio-services"
    )
))]
use squidscript_fw_core::native_runtime::{NativeRuntime, NoopBinBookBackend};

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    not(any(feature = "wifi", feature = "ble"))
))]
use squidscript_fw_core::native_runtime::NoopRadioBackend;

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    not(any(feature = "wifi", feature = "ble"))
))]
use squidscript_fw_x4::binbook_stack::X4FramebufferDisplaySink;

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
use squidscript_fw_x4::binbook_stack::X4CommandDisplaySink;

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
use squidscript_fw_x4::{
    binbook_stack::{
        RenderBuffers, SdStorage, X4CooperativeDisplayTaskStatus, X4Panel,
        X4StreamingDisplayFlushTask, CHUNK_COUNT, ROW_BYTES,
    },
    board::{DisplayDelay, FreqManagedSpiDevice, SharedSpi2},
    x4_storage::{X4BinBookFileBackend, X4SdFileStorage, X4StorageTime},
};

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
use squidscript_fw_core::native_runtime::{
    NativeBinBookBackend, NativeDisplaySink, NativeFileBackend, NativeRadioBackend,
};

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use esp_hal::ram;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services", feature = "wifi"))]
use core::fmt::Write as _;

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
use esp_alloc::export::enumset::EnumSet;

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
use squidscript_fw_core::radio_lifecycle::{
    format_cycle_snapshot, CycleSnapshot, RadioKind, ReclaimSummary,
};

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
use squidscript_fw_core::radio_lifecycle::RadioKind;

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use squidscript_fw_x4::radio_probe::radio_stack_metadata;

#[cfg(target_arch = "riscv32")]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure"))]
use squidscript_fw_core::{
    radio_lifecycle::RadioKind,
    radio_service::{RadioLeaseManager, ServiceLeaseError},
};

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "vm-radio-measure", feature = "native-radio-services")
))]
const TOTAL_SRAM_BYTES: usize = 400 * 1024;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(feature = "x4-binbook")]
type X4SdSpiDevice = FreqManagedSpiDevice<'static, Output<'static>>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(feature = "x4-binbook")]
type X4SdBlockDevice = embedded_sdmmc::SdCard<X4SdSpiDevice, DisplayDelay>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(feature = "x4-binbook")]
type X4NativeFileBackend = X4BinBookFileBackend<
    X4SdFileStorage<SdStorage<X4SdBlockDevice, X4StorageTime>>,
    512,
    8,
    128,
    1024,
    128,
    4,
>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(feature = "x4-binbook")]
type X4NativeRuntime =
    NativeRuntime<EspRadioBackend, X4CommandDisplaySink, NoopBinBookBackend, X4NativeFileBackend>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(not(feature = "x4-binbook"))]
type X4NativeRuntime = NativeRuntime<EspRadioBackend>;

#[cfg(target_arch = "riscv32")]
#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    #[cfg(not(any(feature = "wifi", feature = "ble")))]
    {
        #[cfg(feature = "x4-binbook")]
        type DefaultRuntime = NativeRuntime<NoopRadioBackend, X4FramebufferDisplaySink>;
        #[cfg(not(feature = "x4-binbook"))]
        type DefaultRuntime = NativeRuntime;

        static RUNTIME: static_cell::StaticCell<DefaultRuntime> = static_cell::StaticCell::new();
        static BUFFERS: static_cell::StaticCell<SerialProtocolBuffers> =
            static_cell::StaticCell::new();
        #[cfg(feature = "x4-binbook")]
        let runtime = RUNTIME.init_with(|| {
            NativeRuntime::with_radio_and_display(NoopRadioBackend, X4FramebufferDisplaySink::new())
        });
        #[cfg(not(feature = "x4-binbook"))]
        let runtime = RUNTIME.init_with(NativeRuntime::new);
        let buffers = BUFFERS.init_with(SerialProtocolBuffers::new);
        let mut display_flush = NoDisplayFlushTask;
        run_serial_protocol(
            UsbSerialJtag::new(peripherals.USB_DEVICE),
            runtime,
            buffers,
            &mut display_flush,
        );
    }

    #[cfg(any(feature = "wifi", feature = "ble"))]
    {
        #[cfg(feature = "vm-radio-measure")]
        static RUNTIME: static_cell::StaticCell<NativeRuntime> = static_cell::StaticCell::new();
        #[cfg(feature = "vm-radio-measure")]
        static BUFFERS: static_cell::StaticCell<SerialProtocolBuffers> =
            static_cell::StaticCell::new();
        #[cfg(feature = "vm-radio-measure")]
        static RADIO_LEASES: static_cell::StaticCell<RadioLeaseManager> =
            static_cell::StaticCell::new();
        #[cfg(feature = "vm-radio-measure")]
        let runtime = RUNTIME.init_with(NativeRuntime::new);
        #[cfg(feature = "vm-radio-measure")]
        let buffers = BUFFERS.init_with(SerialProtocolBuffers::new);
        #[cfg(feature = "vm-radio-measure")]
        let radio_leases = RADIO_LEASES.init_with(RadioLeaseManager::new);
        #[cfg(all(feature = "native-radio-services", not(feature = "vm-radio-measure")))]
        static RUNTIME: static_cell::StaticCell<X4NativeRuntime> = static_cell::StaticCell::new();
        #[cfg(all(feature = "native-radio-services", not(feature = "vm-radio-measure")))]
        static BUFFERS: static_cell::StaticCell<SerialProtocolBuffers> =
            static_cell::StaticCell::new();
        let radio = radio_stack_metadata();
        println!(
            "squidscript native x4 radio_probe stack={} version={} features={:?}",
            radio.stack, radio.version, radio.features
        );
        println!("radio_probe_stage allocator_init");
        esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
        esp_alloc::heap_allocator!(size: 36 * 1024);
        #[cfg(feature = "vm-radio-measure")]
        print_vm_static_measurement("allocator_ready", runtime, buffers, radio_leases);
        #[cfg(feature = "vm-radio-measure")]
        print_combined_heap_measurement("allocator_ready", radio_leases);

        let timer = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let software_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        println!("radio_probe_stage rtos_start");
        esp_rtos::start(timer.timer0, software_interrupt.software_interrupt0);
        println!("radio_probe_stage rtos_ready");

        #[cfg(feature = "vm-radio-measure")]
        run_combined_vm_radio_measurement(runtime, buffers, radio_leases);

        #[cfg(all(feature = "native-radio-services", not(feature = "vm-radio-measure")))]
        {
            #[cfg(not(feature = "x4-binbook"))]
            let runtime =
                RUNTIME.init_with(|| NativeRuntime::with_radio_backend(EspRadioBackend::new()));
            let buffers = BUFFERS.init_with(SerialProtocolBuffers::new);
            println!("native_radio_services_stage serial_ready");
            #[cfg(feature = "x4-binbook")]
            {
                static SHARED_SPI: static_cell::StaticCell<SharedSpi2> =
                    static_cell::StaticCell::new();
                let shared_spi = SHARED_SPI.init_with(|| {
                    SharedSpi2::new(
                        peripherals.SPI2,
                        peripherals.GPIO8,
                        peripherals.GPIO10,
                        peripherals.GPIO7,
                    )
                });
                let sd_spi = FreqManagedSpiDevice::new(
                    shared_spi,
                    Output::new(peripherals.GPIO12, Level::High, OutputConfig::default()),
                    400_000,
                );
                let sd_storage =
                    X4SdFileStorage::new(SdStorage::new(sd_spi, DisplayDelay, X4StorageTime));
                let file_backend =
                    X4BinBookFileBackend::<_, 512, 8, 128, 1024, 128, 4>::new(sd_storage);
                let runtime = RUNTIME.init_with(|| {
                    NativeRuntime::with_radio_display_binbook_and_file(
                        EspRadioBackend::new(),
                        X4CommandDisplaySink::new(),
                        NoopBinBookBackend,
                        file_backend,
                    )
                });
                let display_spi = FreqManagedSpiDevice::new(
                    shared_spi,
                    Output::new(peripherals.GPIO21, Level::High, OutputConfig::default()),
                    20_000_000,
                );
                let dc = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
                let reset = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
                let busy = Input::new(peripherals.GPIO6, InputConfig::default());
                let mut panel = X4Panel::new(display_spi, dc, reset, busy);
                let mut delay = DisplayDelay;
                match panel.init_bw(&mut delay) {
                    Ok(()) => println!("display_flush_stage panel_ready"),
                    Err(_) => println!("display_flush_error stage=panel_init error=controller"),
                }
                let mut display_flush = StreamingDisplayFlushTask::new(panel);
                run_serial_protocol(
                    UsbSerialJtag::new(peripherals.USB_DEVICE),
                    runtime,
                    buffers,
                    &mut display_flush,
                );
            }
            #[cfg(not(feature = "x4-binbook"))]
            {
                let mut display_flush = NoDisplayFlushTask;
                run_serial_protocol(
                    UsbSerialJtag::new(peripherals.USB_DEVICE),
                    runtime,
                    buffers,
                    &mut display_flush,
                );
            }
        }

        #[cfg(all(
            not(feature = "vm-radio-measure"),
            not(feature = "native-radio-services")
        ))]
        {
            #[cfg(feature = "wifi")]
            run_radio_probe(RadioKind::Wifi);

            #[cfg(feature = "ble")]
            run_radio_probe(RadioKind::Ble);
        }

        #[cfg(not(feature = "native-radio-services"))]
        loop {
            core::hint::spin_loop();
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "vm-radio-measure",
        feature = "native-radio-services"
    )
))]
struct SerialProtocolBuffers {
    request: [u8; 4096],
    response: [u8; 1088],
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "vm-radio-measure",
        feature = "native-radio-services"
    )
))]
impl SerialProtocolBuffers {
    const fn new() -> Self {
        Self {
            request: [0; 4096],
            response: [0; 1088],
        }
    }

    #[cfg(feature = "vm-radio-measure")]
    fn capacity_bytes(&self) -> usize {
        self.request.len() + self.response.len()
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
trait SerialDisplayFlushRequest<D: NativeDisplaySink, FB: NativeFileBackend> {
    fn request_flush(
        &mut self,
        display_sink: &mut D,
        file_backend: &mut FB,
    ) -> Result<(), &'static str>;

    fn step(&mut self) {}
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
#[allow(dead_code)]
struct NoDisplayFlushTask;

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
impl<D: NativeDisplaySink, FB: NativeFileBackend> SerialDisplayFlushRequest<D, FB>
    for NoDisplayFlushTask
{
    fn request_flush(
        &mut self,
        _display_sink: &mut D,
        _file_backend: &mut FB,
    ) -> Result<(), &'static str> {
        Err("display_flush_task_unavailable")
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
struct StreamingDisplayFlushTask<PANEL> {
    task: X4StreamingDisplayFlushTask,
    panel: PANEL,
    delay: DisplayDelay,
    compressed: [u8; 768],
    decoded: [u8; ROW_BYTES * 3],
    black: [u8; ROW_BYTES],
    red: [u8; ROW_BYTES],
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
impl<PANEL> StreamingDisplayFlushTask<PANEL> {
    fn new(panel: PANEL) -> Self {
        Self {
            task: X4StreamingDisplayFlushTask::new(CHUNK_COUNT),
            panel,
            delay: DisplayDelay,
            compressed: [0; 768],
            decoded: [0; ROW_BYTES * 3],
            black: [0; ROW_BYTES],
            red: [0; ROW_BYTES],
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
impl<SPI, DC, RST, BUSY> SerialDisplayFlushRequest<X4CommandDisplaySink, X4NativeFileBackend>
    for StreamingDisplayFlushTask<X4Panel<SPI, DC, RST, BUSY>>
where
    SPI: embedded_hal::spi::SpiDevice<u8>,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin,
    BUSY: embedded_hal::digital::InputPin,
{
    fn request_flush(
        &mut self,
        display_sink: &mut X4CommandDisplaySink,
        file_backend: &mut X4NativeFileBackend,
    ) -> Result<(), &'static str> {
        let Some(snapshot) = display_sink.pending_snapshot() else {
            return Err("display_flush_no_pending_refresh");
        };
        if let Some(drawable) = snapshot.drawable_handle(0) {
            let mut buffers = RenderBuffers::new(
                &mut self.compressed,
                &mut self.decoded,
                &mut self.black,
                &mut self.red,
            );
            file_backend.render_drawable_absolute_gray(
                drawable,
                &mut self.panel,
                &mut self.delay,
                &mut buffers,
            )?;
            display_sink.mark_snapshot_enqueued();
            return Ok(());
        }
        self.task.request(display_sink)
    }

    fn step(&mut self) {
        match self.task.step(&mut self.panel) {
            Ok(X4CooperativeDisplayTaskStatus::Idle | X4CooperativeDisplayTaskStatus::Pending) => {}
            Ok(X4CooperativeDisplayTaskStatus::Complete) => {
                println!("display_flush_stage complete");
            }
            Err(_) => {
                println!("display_flush_error stage=step error=controller");
                self.task = X4StreamingDisplayFlushTask::new(CHUNK_COUNT);
            }
        }
    }
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
fn native_radio_resource_metrics<B, D, C, FB, F>(
) -> [squid_device_protocol::ResourceMetric<'static>; 8] {
    let stats = esp_alloc::HEAP.stats();
    let heap_free_bytes = stats.size.saturating_sub(stats.current_usage);
    let runtime_static_bytes = core::mem::size_of::<NativeRuntime<B, D, C, FB>>();
    let serial_buffer_bytes = core::mem::size_of::<SerialProtocolBuffers>();
    let display_flush_task_bytes = core::mem::size_of::<F>();
    let known_static_bytes = runtime_static_bytes + serial_buffer_bytes + display_flush_task_bytes;
    let known_used_bytes = known_static_bytes + stats.current_usage;
    let nonheap_remainder_bytes = TOTAL_SRAM_BYTES.saturating_sub(known_static_bytes + stats.size);
    [
        squid_device_protocol::ResourceMetric {
            key: "serial_buffer_bytes",
            value: serial_buffer_bytes as u64,
        },
        squid_device_protocol::ResourceMetric {
            key: "known_static_bytes",
            value: known_static_bytes as u64,
        },
        squid_device_protocol::ResourceMetric {
            key: "heap_free_bytes",
            value: heap_free_bytes as u64,
        },
        squid_device_protocol::ResourceMetric {
            key: "heap_alloc_bytes",
            value: stats.current_usage as u64,
        },
        squid_device_protocol::ResourceMetric {
            key: "heap_max_alloc_bytes",
            value: stats.max_usage as u64,
        },
        squid_device_protocol::ResourceMetric {
            key: "heap_pool_bytes",
            value: stats.size as u64,
        },
        squid_device_protocol::ResourceMetric {
            key: "known_used_bytes",
            value: known_used_bytes as u64,
        },
        squid_device_protocol::ResourceMetric {
            key: "nonheap_remainder_bytes",
            value: nonheap_remainder_bytes as u64,
        },
    ]
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn encode_serial_request<B, D, C, FB, F>(
    runtime: &mut NativeRuntime<B, D, C, FB>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    parsed: &squid_device_protocol::DeviceRequest<'_>,
    request_bytes: &[u8],
    event_buf: &mut [u8; 64],
    display_flush: &mut F,
    response: &mut [u8],
) -> Result<usize, squid_device_protocol::DecodeError>
where
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
    F: SerialDisplayFlushRequest<D, FB>,
{
    use squid_device_protocol::{
        encode_app_list_response_into, encode_content_check_response_into,
        encode_content_delete_response_into, encode_empty_response_into,
        encode_error_response_into, encode_hello_response_into, encode_lifecycle_response_into,
        encode_line_response_into, encode_resources_response_into, encode_state_response_into,
        key_event_from_request_into, request_bytes_field, request_string_field, AppListEntry,
        Opcode, ResourceMetric, Status,
    };

    const SERIAL_MAX_FRAME_BYTES: u64 = 4096;

    match parsed.opcode {
        Opcode::Hello => encode_hello_response_into(
            Opcode::Hello,
            parsed.sequence,
            "xteink-x4",
            "squidscript-native-x4",
            false,
            SERIAL_MAX_FRAME_BYTES,
            response,
        ),
        Opcode::Reset => {
            runtime.reset();
            *sessions = squid_device_protocol::ProtocolSessions::default();
            encode_empty_response_into(Opcode::Reset, Status::Ok, parsed.sequence, response)
        }
        Opcode::StorageFormat => match runtime.storage_format() {
            Ok(()) => {
                *sessions = squid_device_protocol::ProtocolSessions::default();
                encode_empty_response_into(
                    Opcode::StorageFormat,
                    Status::Ok,
                    parsed.sequence,
                    response,
                )
            }
            Err(error) => encode_error_response_into(
                Opcode::StorageFormat,
                parsed.sequence,
                -1,
                error,
                response,
            ),
        },
        Opcode::WifiProfileSet => match request_string_field(parsed, 1)
            .ok()
            .flatten()
            .zip(request_string_field(parsed, 2).ok().flatten())
            .zip(request_string_field(parsed, 3).ok().flatten())
            .ok_or(())
            .and_then(|((profile, ssid), password)| {
                runtime
                    .set_wifi_profile(profile, ssid, password)
                    .map_err(|_| ())
            }) {
            Ok(()) => encode_empty_response_into(
                Opcode::WifiProfileSet,
                Status::Ok,
                parsed.sequence,
                response,
            ),
            Err(()) => encode_error_response_into(
                Opcode::WifiProfileSet,
                parsed.sequence,
                -1,
                "invalid-request",
                response,
            ),
        },
        Opcode::TempRunBegin | Opcode::TempRunChunk | Opcode::TempRunCommit => {
            handle_temp_run_request(runtime, sessions, parsed, response)
        }
        Opcode::AppInstallBegin | Opcode::AppInstallChunk | Opcode::AppInstallCommit => {
            handle_app_install_request(runtime, sessions, parsed, response)
        }
        Opcode::ContentInstallBegin
        | Opcode::ContentInstallChunk
        | Opcode::ContentInstallCommit => {
            handle_content_install_request(runtime, sessions, parsed, response)
        }
        Opcode::ContentCheck => match request_string_field(parsed, 1).ok().flatten() {
            Some(name) => match runtime.check_content(name) {
                Ok(result) => encode_content_check_response_into(
                    parsed.sequence,
                    result.name,
                    result.size,
                    u64::from(result.crc32),
                    response,
                ),
                Err(error) => {
                    encode_error_response_into(parsed.opcode, parsed.sequence, -1, error, response)
                }
            },
            None => encode_error_response_into(
                parsed.opcode,
                parsed.sequence,
                -1,
                "invalid-request",
                response,
            ),
        },
        Opcode::ContentDelete => match request_string_field(parsed, 1).ok().flatten() {
            Some(name) => match runtime.delete_content(name) {
                Ok(deleted) => {
                    encode_content_delete_response_into(parsed.sequence, deleted, response)
                }
                Err(error) => {
                    encode_error_response_into(parsed.opcode, parsed.sequence, -1, error, response)
                }
            },
            None => encode_error_response_into(
                parsed.opcode,
                parsed.sequence,
                -1,
                "invalid-request",
                response,
            ),
        },
        Opcode::AppLaunch => {
            match request_string_field(parsed, 1)
                .ok()
                .flatten()
                .ok_or(())
                .and_then(|app_id| runtime.launch_app(app_id).map_err(|_| ()))
            {
                Ok(()) => encode_empty_response_into(
                    Opcode::AppLaunch,
                    Status::Ok,
                    parsed.sequence,
                    response,
                ),
                Err(()) => encode_error_response_into(
                    Opcode::AppLaunch,
                    parsed.sequence,
                    -1,
                    "launch_failed",
                    response,
                ),
            }
        }
        Opcode::AppList => {
            let mut entries = [AppListEntry {
                app_id: "",
                sqbc_len: 0,
            }];
            let len = if let Some((app_id, sqbc_len)) = runtime.installed_app() {
                entries[0] = AppListEntry {
                    app_id,
                    sqbc_len: sqbc_len as u64,
                };
                1
            } else {
                0
            };
            encode_app_list_response_into(parsed.sequence, entries[..len].iter().copied(), response)
        }
        Opcode::Key => {
            match key_event_from_request_into(request_bytes, event_buf)
                .ok()
                .and_then(|len| core::str::from_utf8(&event_buf[..len]).ok())
                .ok_or(())
                .and_then(|event| runtime.dispatch_event(event).map_err(|_| ()))
            {
                Ok(()) => {
                    encode_empty_response_into(Opcode::Key, Status::Ok, parsed.sequence, response)
                }
                Err(()) => encode_error_response_into(
                    Opcode::Key,
                    parsed.sequence,
                    -1,
                    "key_dispatch_failed",
                    response,
                ),
            }
        }
        Opcode::EventDispatch => {
            match request_string_field(parsed, 1)
                .ok()
                .flatten()
                .zip(request_string_field(parsed, 2).ok().flatten())
                .ok_or(())
                .and_then(|(app_id, event)| {
                    runtime.dispatch_app_event(app_id, event).map_err(|_| ())
                }) {
                Ok(()) => encode_empty_response_into(
                    Opcode::EventDispatch,
                    Status::Ok,
                    parsed.sequence,
                    response,
                ),
                Err(()) => encode_error_response_into(
                    Opcode::EventDispatch,
                    parsed.sequence,
                    -1,
                    "event_dispatch_failed",
                    response,
                ),
            }
        }
        Opcode::StateImport => {
            match request_bytes_field(parsed, 1)
                .ok()
                .flatten()
                .ok_or(())
                .and_then(|bytes| runtime.import_state(bytes).map_err(|_| ()))
            {
                Ok(()) => encode_empty_response_into(
                    Opcode::StateImport,
                    Status::Ok,
                    parsed.sequence,
                    response,
                ),
                Err(()) => encode_error_response_into(
                    Opcode::StateImport,
                    parsed.sequence,
                    -1,
                    "state_import_failed",
                    response,
                ),
            }
        }
        Opcode::OutputGet => encode_line_response_into(
            Opcode::OutputGet,
            parsed.sequence,
            runtime.output_lines().iter(),
            response,
        ),
        Opcode::TraceGet => encode_line_response_into(
            Opcode::TraceGet,
            parsed.sequence,
            runtime.trace_lines().iter(),
            response,
        ),
        Opcode::DrawlogGet => encode_line_response_into(
            Opcode::DrawlogGet,
            parsed.sequence,
            runtime.drawlog_lines().iter(),
            response,
        ),
        Opcode::StateGet => {
            encode_state_response_into(parsed.sequence, runtime.state_bytes(), response)
        }
        Opcode::ResourcesGet => {
            let metrics = runtime.resource_metrics();
            #[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
            let platform_metrics = native_radio_resource_metrics::<B, D, C, FB, F>();
            #[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
            let metric_iter = metrics
                .iter()
                .map(|metric| ResourceMetric {
                    key: metric.key,
                    value: metric.value,
                })
                .chain(platform_metrics.iter().copied());
            #[cfg(not(all(target_arch = "riscv32", feature = "native-radio-services")))]
            let metric_iter = metrics.iter().map(|metric| ResourceMetric {
                key: metric.key,
                value: metric.value,
            });
            encode_resources_response_into(parsed.sequence, metric_iter, response)
        }
        Opcode::LifecycleGet => encode_lifecycle_response_into(
            parsed.sequence,
            runtime.active_app(),
            core::iter::empty(),
            core::iter::empty(),
            response,
        ),
        Opcode::DisplayWindowProbe => match {
            let (display_sink, file_backend) = runtime.display_sink_and_file_backend_mut();
            display_flush.request_flush(display_sink, file_backend)
        } {
            Ok(()) => encode_empty_response_into(
                Opcode::DisplayWindowProbe,
                Status::Ok,
                parsed.sequence,
                response,
            ),
            Err(error) => encode_error_response_into(
                Opcode::DisplayWindowProbe,
                parsed.sequence,
                -1,
                error,
                response,
            ),
        },
        opcode => encode_empty_response_into(opcode, Status::Error, parsed.sequence, response),
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn process_serial_byte<B, D, C, FB, F>(
    byte: u8,
    runtime: &mut NativeRuntime<B, D, C, FB>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    request_len: &mut usize,
    event_buf: &mut [u8; 64],
    display_flush: &mut F,
    buffers: &mut SerialProtocolBuffers,
) -> Option<usize>
where
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
    F: SerialDisplayFlushRequest<D, FB>,
{
    if *request_len == buffers.request.len() {
        *request_len = 0;
    }
    buffers.request[*request_len] = byte;
    *request_len += 1;

    if *request_len >= squid_device_protocol::MAGIC.len()
        && buffers.request[..squid_device_protocol::MAGIC.len()] != squid_device_protocol::MAGIC
    {
        if let Some(start) = find_magic(&buffers.request[..*request_len]) {
            buffers.request.copy_within(start..*request_len, 0);
            *request_len -= start;
        } else {
            let keep = squid_device_protocol::MAGIC
                .len()
                .saturating_sub(1)
                .min(*request_len);
            buffers
                .request
                .copy_within(*request_len - keep..*request_len, 0);
            *request_len = keep;
        }
        return None;
    }

    let frame_len = complete_request_len(&buffers.request[..*request_len])?;
    if frame_len > *request_len {
        return None;
    }

    let encoded_len = squid_device_protocol::DeviceRequest::decode(&buffers.request[..frame_len])
        .ok()
        .and_then(|parsed| {
            encode_serial_request(
                runtime,
                sessions,
                &parsed,
                &buffers.request[..frame_len],
                event_buf,
                display_flush,
                &mut buffers.response,
            )
            .ok()
        });

    let remaining = *request_len - frame_len;
    buffers.request.copy_within(frame_len..*request_len, 0);
    *request_len = remaining;
    encoded_len
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn run_serial_protocol<B, D, C, FB, F>(
    mut serial: UsbSerialJtag<'static, esp_hal::Blocking>,
    runtime: &'static mut NativeRuntime<B, D, C, FB>,
    buffers: &'static mut SerialProtocolBuffers,
    display_flush: &mut F,
) -> !
where
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
    F: SerialDisplayFlushRequest<D, FB>,
{
    let mut request_len = 0usize;
    let mut sessions = squid_device_protocol::ProtocolSessions::default();
    let mut event_buf = [0u8; 64];

    loop {
        match serial.read_byte() {
            Ok(byte) => {
                if let Some(response_len) = process_serial_byte(
                    byte,
                    runtime,
                    &mut sessions,
                    &mut request_len,
                    &mut event_buf,
                    display_flush,
                    buffers,
                ) {
                    let _ = serial.write(&buffers.response[..response_len]);
                }
            }
            Err(_) => core::hint::spin_loop(),
        }
        display_flush.step();
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn handle_app_install_request<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
>(
    runtime: &mut NativeRuntime<B, D, C, FB>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    request: &squid_device_protocol::DeviceRequest<'_>,
    response: &mut [u8],
) -> Result<usize, squid_device_protocol::DecodeError> {
    use squid_device_protocol::{
        encode_empty_response_into, encode_error_response_into, HostAction, Status,
    };

    match sessions.next_action(request) {
        Ok(HostAction::BeginInstall { app_id, total_len }) => {
            if let Err(error) = runtime.begin_app_install(app_id, total_len) {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            let _ = sessions.complete_begin_install("/sq/apps/native-installed.sqbc");
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::WriteInstallChunk { offset, bytes, .. }) => {
            if let Err(error) = runtime.write_app_install_chunk(offset, bytes) {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            let chunk_ptr = bytes.as_ptr();
            let chunk_len = bytes.len();
            let bytes = unsafe { core::slice::from_raw_parts(chunk_ptr, chunk_len) };
            let _ = sessions.complete_install_chunk(bytes);
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::CommitInstall { .. }) => {
            if let Err(error) = runtime.commit_app_install() {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            sessions.complete_install_commit();
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(_) => encode_error_response_into(
            request.opcode,
            request.sequence,
            -1,
            "unsupported_transfer_action",
            response,
        ),
        Err(_) => encode_error_response_into(
            request.opcode,
            request.sequence,
            -1,
            "invalid_transfer_request",
            response,
        ),
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn handle_content_install_request<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
>(
    runtime: &mut NativeRuntime<B, D, C, FB>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    request: &squid_device_protocol::DeviceRequest<'_>,
    response: &mut [u8],
) -> Result<usize, squid_device_protocol::DecodeError> {
    use squid_device_protocol::{
        encode_empty_response_into, encode_error_response_into, HostAction, Status,
    };

    match sessions.next_action(request) {
        Ok(HostAction::BeginContentInstall { name, total_len }) => {
            let path = match runtime.begin_content_install(name, total_len) {
                Ok(path) => path,
                Err(error) => {
                    return encode_error_response_into(
                        request.opcode,
                        request.sequence,
                        -1,
                        error,
                        response,
                    )
                }
            };
            let _ = sessions.complete_begin_content_install(path);
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::WriteContentChunk {
            path,
            offset,
            bytes,
        }) => {
            if let Err(error) = runtime.write_content_install_chunk(path, offset, bytes) {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    error,
                    response,
                );
            }
            let chunk_ptr = bytes.as_ptr();
            let chunk_len = bytes.len();
            let bytes = unsafe { core::slice::from_raw_parts(chunk_ptr, chunk_len) };
            let _ = sessions.complete_content_chunk(bytes);
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::CommitContentInstall { path, .. }) => {
            if let Err(error) = runtime.commit_content_install(path) {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    error,
                    response,
                );
            }
            sessions.complete_content_commit();
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(_) => encode_error_response_into(
            request.opcode,
            request.sequence,
            -1,
            "unsupported_transfer_action",
            response,
        ),
        Err(_) => encode_error_response_into(
            request.opcode,
            request.sequence,
            -1,
            "invalid_transfer_request",
            response,
        ),
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn handle_temp_run_request<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
>(
    runtime: &mut NativeRuntime<B, D, C, FB>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    request: &squid_device_protocol::DeviceRequest<'_>,
    response: &mut [u8],
) -> Result<usize, squid_device_protocol::DecodeError> {
    use squid_device_protocol::{
        encode_empty_response_into, encode_error_response_into, HostAction, Status,
    };

    match sessions.next_action(request) {
        Ok(HostAction::BeginTempRun { app_id, total_len }) => {
            if let Err(error) = runtime.begin_temp_run(app_id, total_len) {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            let _ = sessions.complete_begin_temp_run("/sq/tmp/native-temp.sqbc");
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::WriteTempRunChunk { offset, bytes, .. }) => {
            if let Err(error) = runtime.write_temp_run_chunk(offset, bytes) {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            let chunk_ptr = bytes.as_ptr();
            let chunk_len = bytes.len();
            let bytes = unsafe { core::slice::from_raw_parts(chunk_ptr, chunk_len) };
            let _ = sessions.complete_temp_run_chunk(bytes);
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::CommitTempRun { .. }) => {
            if let Err(error) = runtime.commit_temp_run() {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            sessions.complete_temp_run_commit();
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(_) => encode_error_response_into(
            request.opcode,
            request.sequence,
            -1,
            "unsupported_transfer_action",
            response,
        ),
        Err(_) => encode_error_response_into(
            request.opcode,
            request.sequence,
            -1,
            "invalid_transfer_request",
            response,
        ),
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn native_runtime_error_name(
    error: squidscript_fw_core::native_runtime::NativeRuntimeError,
) -> &'static str {
    match error {
        squidscript_fw_core::native_runtime::NativeRuntimeError::TooLarge => "too_large",
        squidscript_fw_core::native_runtime::NativeRuntimeError::InvalidOffset => "invalid_offset",
        squidscript_fw_core::native_runtime::NativeRuntimeError::IncompleteTempRun => {
            "incomplete_temp_run"
        }
        squidscript_fw_core::native_runtime::NativeRuntimeError::AppNotInstalled => {
            "app_not_installed"
        }
        squidscript_fw_core::native_runtime::NativeRuntimeError::AppIdMismatch => "app_id_mismatch",
        squidscript_fw_core::native_runtime::NativeRuntimeError::Inactive => "inactive",
        squidscript_fw_core::native_runtime::NativeRuntimeError::Vm(_) => "vm_error",
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn complete_request_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < squid_device_protocol::HEADER_LEN {
        return None;
    }
    let payload_len = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    squid_device_protocol::HEADER_LEN.checked_add(payload_len)
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn find_magic(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(squid_device_protocol::MAGIC.len())
        .position(|window| window == squid_device_protocol::MAGIC)
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
struct EspRadioBackend {
    #[cfg(feature = "wifi")]
    wifi: Option<esp_radio::wifi::WifiController<'static>>,
    #[cfg(feature = "wifi")]
    wifi_ap_active: bool,
    #[cfg(feature = "wifi")]
    wifi_sta_active: bool,
    #[cfg(feature = "wifi")]
    wifi_ap_ssid: heapless::String<32>,
    #[cfg(feature = "wifi")]
    wifi_ap_start_events: i32,
    #[cfg(feature = "wifi")]
    wifi_ap_stop_events: i32,
    #[cfg(feature = "wifi")]
    wifi_last_backend_code: Option<&'static str>,
    #[cfg(feature = "wifi")]
    wifi_scan_results: heapless::Vec<squidvm_core::host::WifiAccessPoint, 8>,
    #[cfg(feature = "wifi")]
    wifi_sta_connected_events: i32,
    #[cfg(feature = "wifi")]
    wifi_sta_disconnected_events: i32,
    #[cfg(feature = "wifi")]
    wifi_last_disconnect_reason: Option<&'static str>,
    #[cfg(feature = "wifi")]
    wifi_last_disconnect_reason_code: i32,
    #[cfg(feature = "wifi")]
    wifi_sta_auth: Option<&'static str>,
    #[cfg(feature = "wifi")]
    wifi_sta_ssid: heapless::String<32>,
    #[cfg(feature = "wifi")]
    wifi_sta_bssid: heapless::String<17>,
    #[cfg(feature = "ble")]
    ble: Option<esp_radio::ble::controller::BleConnector<'static>>,
    #[cfg(feature = "ble")]
    ble_profile_id: heapless::String<32>,
    #[cfg(feature = "ble")]
    ble_profile_start_events: u32,
    #[cfg(feature = "ble")]
    ble_profile_stop_events: u32,
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
impl EspRadioBackend {
    const fn new() -> Self {
        Self {
            #[cfg(feature = "wifi")]
            wifi: None,
            #[cfg(feature = "wifi")]
            wifi_ap_active: false,
            #[cfg(feature = "wifi")]
            wifi_sta_active: false,
            #[cfg(feature = "wifi")]
            wifi_ap_ssid: heapless::String::new(),
            #[cfg(feature = "wifi")]
            wifi_ap_start_events: 0,
            #[cfg(feature = "wifi")]
            wifi_ap_stop_events: 0,
            #[cfg(feature = "wifi")]
            wifi_last_backend_code: None,
            #[cfg(feature = "wifi")]
            wifi_scan_results: heapless::Vec::new(),
            #[cfg(feature = "wifi")]
            wifi_sta_connected_events: 0,
            #[cfg(feature = "wifi")]
            wifi_sta_disconnected_events: 0,
            #[cfg(feature = "wifi")]
            wifi_last_disconnect_reason: None,
            #[cfg(feature = "wifi")]
            wifi_last_disconnect_reason_code: 0,
            #[cfg(feature = "wifi")]
            wifi_sta_auth: None,
            #[cfg(feature = "wifi")]
            wifi_sta_ssid: heapless::String::new(),
            #[cfg(feature = "wifi")]
            wifi_sta_bssid: heapless::String::new(),
            #[cfg(feature = "ble")]
            ble: None,
            #[cfg(feature = "ble")]
            ble_profile_id: heapless::String::new(),
            #[cfg(feature = "ble")]
            ble_profile_start_events: 0,
            #[cfg(feature = "ble")]
            ble_profile_stop_events: 0,
        }
    }
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services", feature = "wifi"))]
fn wifi_auth_label(auth: Option<esp_radio::wifi::AuthenticationMethod>) -> Option<&'static str> {
    match auth {
        None => None,
        Some(esp_radio::wifi::AuthenticationMethod::None) => Some("OPEN"),
        Some(esp_radio::wifi::AuthenticationMethod::Wep) => Some("WEP"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa) => Some("WPA"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa2Personal) => Some("WPA2_PSK"),
        Some(esp_radio::wifi::AuthenticationMethod::WpaWpa2Personal) => Some("WPA_WPA2_PSK"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa2Enterprise) => Some("WPA2_ENTERPRISE"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa3Personal) => Some("WPA3_PSK"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa2Wpa3Personal) => Some("WPA2_WPA3_PSK"),
        Some(esp_radio::wifi::AuthenticationMethod::WapiPersonal) => Some("WAPI_PSK"),
        Some(esp_radio::wifi::AuthenticationMethod::Owe) => Some("OWE"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa3EntSuiteB192Bit) => {
            Some("WPA3_ENTERPRISE")
        }
        Some(esp_radio::wifi::AuthenticationMethod::Wpa3ExtPsk) => Some("WPA3_PSK"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa3ExtPskMixed) => Some("WPA3_PSK_MIXED"),
        Some(esp_radio::wifi::AuthenticationMethod::Dpp) => Some("DPP"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa3Enterprise) => Some("WPA3_ENTERPRISE"),
        Some(esp_radio::wifi::AuthenticationMethod::Wpa2Wpa3Enterprise) => {
            Some("WPA2_WPA3_ENTERPRISE")
        }
        Some(esp_radio::wifi::AuthenticationMethod::WpaEnterprise) => Some("WPA_ENTERPRISE"),
        Some(_) => Some("UNKNOWN"),
    }
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services", feature = "wifi"))]
fn write_bssid_text(out: &mut heapless::String<17>, bssid: [u8; 6]) {
    out.clear();
    let _ = write!(
        out,
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        bssid[0], bssid[1], bssid[2], bssid[3], bssid[4], bssid[5]
    );
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
impl NativeRadioBackend for EspRadioBackend {
    fn acquire(&mut self, radio: RadioKind) -> Result<(), ()> {
        match radio {
            #[cfg(feature = "wifi")]
            RadioKind::Wifi => {
                if self.wifi.is_some() {
                    return Ok(());
                }
                let wifi = unsafe { esp_hal::peripherals::WIFI::steal() };
                let controller = esp_radio::wifi::WifiController::new(wifi, Default::default())
                    .map_err(|_| ())?;
                self.wifi = Some(controller);
                Ok(())
            }
            #[cfg(feature = "ble")]
            RadioKind::Ble => {
                if self.ble.is_some() {
                    return Ok(());
                }
                let bt = unsafe { esp_hal::peripherals::BT::steal() };
                let connector =
                    esp_radio::ble::controller::BleConnector::new(bt, Default::default())
                        .map_err(|_| ())?;
                self.ble = Some(connector);
                Ok(())
            }
            #[allow(unreachable_patterns)]
            _ => Err(()),
        }
    }

    fn release(&mut self, radio: RadioKind) {
        match radio {
            #[cfg(feature = "wifi")]
            RadioKind::Wifi => {
                self.wifi = None;
                if self.wifi_ap_active {
                    self.wifi_ap_stop_events += 1;
                }
                self.wifi_ap_active = false;
                self.wifi_sta_active = false;
                self.wifi_ap_ssid.clear();
                self.wifi_scan_results.clear();
                self.wifi_sta_auth = None;
                self.wifi_sta_ssid.clear();
                self.wifi_sta_bssid.clear();
                self.wifi_last_disconnect_reason = None;
                self.wifi_last_disconnect_reason_code = 0;
            }
            #[cfg(feature = "ble")]
            RadioKind::Ble => {
                self.ble = None;
                if !self.ble_profile_id.is_empty() {
                    self.ble_profile_stop_events += 1;
                    self.ble_profile_id.clear();
                }
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    fn start_ble_profile(&mut self, id: &str) -> Result<(), ()> {
        #[cfg(feature = "ble")]
        {
            if self.ble.is_none() {
                return Err(());
            }
            self.ble_profile_id.clear();
            self.ble_profile_id.push_str(id).map_err(|_| ())?;
            self.ble_profile_start_events += 1;
            Ok(())
        }
        #[cfg(not(feature = "ble"))]
        {
            let _ = id;
            Err(())
        }
    }

    fn stop_ble_profile(&mut self) {
        #[cfg(feature = "ble")]
        {
            if !self.ble_profile_id.is_empty() {
                self.ble_profile_stop_events += 1;
                self.ble_profile_id.clear();
            }
        }
    }

    fn start_wifi_ap(&mut self, ssid: &str) -> Result<(), ()> {
        #[cfg(feature = "wifi")]
        {
            let Some(controller) = self.wifi.as_mut() else {
                return Err(());
            };
            let config = esp_radio::wifi::Config::AccessPoint(
                esp_radio::wifi::ap::AccessPointConfig::default().with_ssid(ssid),
            );
            if controller.set_config(&config).is_err() {
                self.wifi_last_backend_code = Some("set-config");
                return Err(());
            }
            self.wifi_ap_ssid.clear();
            self.wifi_ap_ssid.push_str(ssid).map_err(|_| ())?;
            self.wifi_ap_active = true;
            self.wifi_sta_active = false;
            self.wifi_sta_auth = None;
            self.wifi_sta_ssid.clear();
            self.wifi_sta_bssid.clear();
            self.wifi_ap_start_events += 1;
            self.wifi_last_backend_code = None;
            Ok(())
        }
        #[cfg(not(feature = "wifi"))]
        {
            let _ = ssid;
            Err(())
        }
    }

    fn connect_wifi_station(&mut self, ssid: &str, password: &str) -> Result<(), ()> {
        #[cfg(feature = "wifi")]
        {
            let Some(controller) = self.wifi.as_mut() else {
                return Err(());
            };
            let config = esp_radio::wifi::Config::Station(
                esp_radio::wifi::sta::StationConfig::default()
                    .with_ssid(ssid)
                    .with_password(password.into()),
            );
            if controller.set_config(&config).is_err() {
                self.wifi_last_backend_code = Some("set-config");
                return Err(());
            }
            match embassy_futures::block_on(controller.connect_async()) {
                Ok(info) => {
                    self.wifi_sta_connected_events += 1;
                    self.wifi_sta_auth = wifi_auth_label(Some(info.authmode));
                    self.wifi_sta_ssid.clear();
                    self.wifi_sta_ssid
                        .push_str(info.ssid.as_str())
                        .map_err(|_| ())?;
                    write_bssid_text(&mut self.wifi_sta_bssid, info.bssid);
                }
                Err(esp_radio::wifi::WifiError::Disconnected(info)) => {
                    self.wifi_sta_disconnected_events += 1;
                    self.wifi_last_disconnect_reason = Some("disconnected");
                    self.wifi_last_disconnect_reason_code = 0;
                    self.wifi_last_backend_code = Some("connect-disconnected");
                    write_bssid_text(&mut self.wifi_sta_bssid, info.bssid);
                    return Err(());
                }
                Err(_) => {
                    self.wifi_last_backend_code = Some("connect");
                    return Err(());
                }
            }
            self.wifi_ap_active = false;
            self.wifi_ap_ssid.clear();
            self.wifi_sta_active = true;
            self.wifi_last_backend_code = None;
            Ok(())
        }
        #[cfg(not(feature = "wifi"))]
        {
            let _ = ssid;
            let _ = password;
            Err(())
        }
    }

    fn wifi_mode(&self) -> Option<&'static str> {
        #[cfg(feature = "wifi")]
        {
            if self.wifi_ap_active {
                Some("ap")
            } else if self.wifi_sta_active {
                Some("sta")
            } else {
                None
            }
        }
        #[cfg(not(feature = "wifi"))]
        {
            None
        }
    }

    fn wifi_status(&self) -> squidscript_fw_core::native_runtime::NativeWifiStatus<'_> {
        #[cfg(feature = "wifi")]
        {
            let connected = self
                .wifi
                .as_ref()
                .map(|controller| controller.is_connected())
                .unwrap_or(false);
            let channel = self
                .wifi
                .as_ref()
                .and_then(|controller| controller.channel().ok().map(|(channel, _)| channel as i32))
                .unwrap_or(0);
            let rssi = self
                .wifi
                .as_ref()
                .and_then(|controller| controller.rssi().ok())
                .unwrap_or(0);
            let sta_info = self
                .wifi
                .as_ref()
                .and_then(|controller| controller.ap_info().ok());
            let auth = sta_info
                .as_ref()
                .and_then(|info| wifi_auth_label(info.auth_method))
                .or(self.wifi_sta_auth);
            squidscript_fw_core::native_runtime::NativeWifiStatus {
                mode: self.wifi_mode(),
                ssid: if self.wifi_ap_active {
                    Some(self.wifi_ap_ssid.as_str())
                } else if self.wifi_sta_active && !self.wifi_sta_ssid.is_empty() {
                    Some(self.wifi_sta_ssid.as_str())
                } else {
                    None
                },
                ip_address: None,
                state: if self.wifi_ap_active {
                    "started"
                } else if connected {
                    "connected"
                } else if self.wifi_sta_active {
                    "starting"
                } else if self.wifi_ap_stop_events > 0 {
                    "stopped"
                } else {
                    "idle"
                },
                driver_started: self.wifi_ap_active || self.wifi_sta_active,
                configured: self.wifi_ap_active || self.wifi_sta_active,
                channel,
                clients: 0,
                ap_start_events: self.wifi_ap_start_events,
                ap_stop_events: self.wifi_ap_stop_events,
                probe_events: 0,
                sta_connected_events: self.wifi_sta_connected_events,
                sta_disconnected_events: self.wifi_sta_disconnected_events,
                last_backend_code: self.wifi_last_backend_code,
                connected,
                scan_matches: self.wifi_scan_results.len() as i32,
                rssi,
                auth,
                bssid: if self.wifi_sta_bssid.is_empty() {
                    None
                } else {
                    Some(self.wifi_sta_bssid.as_str())
                },
                disconnect_reason: self.wifi_last_disconnect_reason,
                disconnect_reason_code: self.wifi_last_disconnect_reason_code,
            }
        }
        #[cfg(not(feature = "wifi"))]
        {
            squidscript_fw_core::native_runtime::NativeWifiStatus::idle()
        }
    }

    fn scan_wifi(&mut self) -> Result<i32, &'static str> {
        #[cfg(feature = "wifi")]
        {
            let Some(controller) = self.wifi.as_mut() else {
                return Err("unavailable");
            };
            let config = esp_radio::wifi::scan::ScanConfig::default().with_max(8);
            let results = embassy_futures::block_on(controller.scan_async(&config)).map_err(|_| {
                self.wifi_last_backend_code = Some("scan");
                "scan failed"
            })?;
            self.wifi_scan_results.clear();
            for info in results {
                let network = squidvm_core::host::WifiAccessPoint::new(
                    info.ssid.as_str().as_bytes(),
                    Some(info.bssid),
                    info.channel as i32,
                    info.signal_strength as i32,
                    wifi_auth_label(info.auth_method),
                    info.ssid.is_empty(),
                )
                .map_err(|_| {
                    self.wifi_last_backend_code = Some("scan-record");
                    "scan record invalid"
                })?;
                let _ = self.wifi_scan_results.push(network);
            }
            self.wifi_last_backend_code = None;
            Ok(self.wifi_scan_results.len() as i32)
        }
        #[cfg(not(feature = "wifi"))]
        {
            Err("unsupported")
        }
    }

    fn wifi_scan_network(
        &self,
        index: i32,
    ) -> Result<Option<squidvm_core::host::WifiAccessPoint>, &'static str> {
        #[cfg(feature = "wifi")]
        {
            if index < 0 {
                return Ok(None);
            }
            Ok(self.wifi_scan_results.get(index as usize).copied())
        }
        #[cfg(not(feature = "wifi"))]
        {
            let _ = index;
            Err("unsupported")
        }
    }
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure"))]
fn run_combined_vm_radio_measurement(
    runtime: &'static mut NativeRuntime,
    buffers: &'static mut SerialProtocolBuffers,
    radio_leases: &'static mut RadioLeaseManager,
) {
    print_vm_static_measurement("measurement_start", runtime, buffers, radio_leases);
    print_combined_heap_measurement("radio_idle", radio_leases);

    #[cfg(feature = "wifi")]
    {
        println!("vm_radio_measure_stage wifi_only_init");
        match activate_wifi_only(radio_leases) {
            Ok(controller) => {
                println!("vm_radio_measure_stage wifi_only_active");
                print_combined_heap_measurement("wifi_only_active", radio_leases);
                drop(controller);
                settle_after_deinit();
                print_combined_heap_measurement("wifi_only_settled", radio_leases);
            }
            Err(error) => println!(
                "vm_radio_measure_error stage=wifi_only_init error={}",
                error
            ),
        }
    }

    #[cfg(feature = "ble")]
    {
        println!("vm_radio_measure_stage ble_only_init");
        match activate_ble_only(radio_leases) {
            Ok(connector) => {
                println!("vm_radio_measure_stage ble_only_active");
                print_combined_heap_measurement("ble_only_active", radio_leases);
                drop(connector);
                settle_after_deinit();
                print_combined_heap_measurement("ble_only_settled", radio_leases);
            }
            Err(error) => println!("vm_radio_measure_error stage=ble_only_init error={}", error),
        }
    }

    #[cfg(all(feature = "wifi", feature = "ble"))]
    {
        println!("vm_radio_measure_stage both_init");
        match activate_wifi_and_ble(radio_leases) {
            Ok((wifi_controller, ble_connector)) => {
                println!("vm_radio_measure_stage both_active");
                print_combined_heap_measurement("wifi_ble_active", radio_leases);
                drop(ble_connector);
                drop(wifi_controller);
                settle_after_deinit();
                print_combined_heap_measurement("wifi_ble_settled", radio_leases);
            }
            Err(error) => println!("vm_radio_measure_error stage=both_init error={}", error),
        }
    }

    print_vm_static_measurement("measurement_done", runtime, buffers, radio_leases);
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "vm-radio-measure",
    feature = "wifi"
))]
fn activate_wifi_only(
    radio_leases: &mut RadioLeaseManager,
) -> Result<WifiServiceLease, &'static str> {
    radio_leases
        .acquire(RadioKind::Wifi)
        .map_err(service_lease_error_name)?;
    let wifi = unsafe { esp_hal::peripherals::WIFI::steal() };
    match esp_radio::wifi::WifiController::new(wifi, Default::default()) {
        Ok(controller) => Ok(WifiServiceLease {
            controller,
            radio_leases: radio_leases as *mut RadioLeaseManager,
        }),
        Err(_) => {
            let _ = radio_leases.release(RadioKind::Wifi);
            Err("wifi_init")
        }
    }
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure", feature = "ble"))]
fn activate_ble_only(
    radio_leases: &mut RadioLeaseManager,
) -> Result<BleServiceLease, &'static str> {
    radio_leases
        .acquire(RadioKind::Ble)
        .map_err(service_lease_error_name)?;
    let bt = unsafe { esp_hal::peripherals::BT::steal() };
    match esp_radio::ble::controller::BleConnector::new(bt, Default::default()) {
        Ok(connector) => Ok(BleServiceLease {
            connector,
            radio_leases: radio_leases as *mut RadioLeaseManager,
        }),
        Err(_) => {
            let _ = radio_leases.release(RadioKind::Ble);
            Err("ble_init")
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "vm-radio-measure",
    feature = "wifi",
    feature = "ble"
))]
fn activate_wifi_and_ble(
    radio_leases: &mut RadioLeaseManager,
) -> Result<(WifiServiceLease, BleServiceLease), &'static str> {
    let wifi_controller = activate_wifi_only(radio_leases)?;
    match activate_ble_only(radio_leases) {
        Ok(ble_connector) => Ok((wifi_controller, ble_connector)),
        Err(error) => {
            drop(wifi_controller);
            Err(error)
        }
    }
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure"))]
fn print_vm_static_measurement(
    stage: &str,
    runtime: &NativeRuntime,
    buffers: &SerialProtocolBuffers,
    radio_leases: &RadioLeaseManager,
) {
    let runtime_bytes = core::mem::size_of_val(runtime);
    let serial_buffer_bytes = buffers.capacity_bytes();
    let vm_and_serial_bytes = runtime_bytes + serial_buffer_bytes;
    print_total_ram_line(
        "vm_static",
        stage,
        runtime_bytes,
        serial_buffer_bytes,
        0,
        heap_free_bytes(),
        esp_alloc::HEAP.stats().size,
        radio_leases,
    );
    println!(
        "vm_radio_measure_static stage={} runtime_static_bytes={} runtime_static_pct_x100={} serial_buffer_bytes={} serial_buffer_pct_x100={} vm_serial_static_bytes={} vm_serial_static_pct_x100={}",
        stage,
        runtime_bytes,
        percent_x100(runtime_bytes),
        serial_buffer_bytes,
        percent_x100(serial_buffer_bytes),
        vm_and_serial_bytes,
        percent_x100(vm_and_serial_bytes)
    );
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure"))]
fn print_combined_heap_measurement(stage: &str, radio_leases: &RadioLeaseManager) {
    let stats = esp_alloc::HEAP.stats();
    let free = stats.size.saturating_sub(stats.current_usage);
    print_total_ram_line(
        "heap",
        stage,
        core::mem::size_of::<NativeRuntime>(),
        core::mem::size_of::<SerialProtocolBuffers>(),
        stats.current_usage,
        free,
        stats.size,
        radio_leases,
    );
    println!(
        "vm_radio_measure_heap stage={} heap_pool_bytes={} heap_pool_pct_x100={} heap_used_bytes={} heap_used_pct_x100={} heap_free_bytes={} heap_free_pct_x100={} heap_max_used_bytes={} heap_max_used_pct_x100={}",
        stage,
        stats.size,
        percent_x100(stats.size),
        stats.current_usage,
        percent_x100(stats.current_usage),
        free,
        percent_x100(free),
        stats.max_usage,
        percent_x100(stats.max_usage)
    );
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure"))]
fn print_total_ram_line(
    kind: &str,
    stage: &str,
    runtime_bytes: usize,
    serial_buffer_bytes: usize,
    heap_used_bytes: usize,
    heap_free_bytes: usize,
    heap_pool_bytes: usize,
    radio_leases: &RadioLeaseManager,
) {
    let known_static_bytes = runtime_bytes + serial_buffer_bytes;
    let known_used_bytes = known_static_bytes + heap_used_bytes;
    let nonheap_remainder_bytes =
        TOTAL_SRAM_BYTES.saturating_sub(known_static_bytes + heap_pool_bytes);
    println!(
        "vm_radio_measure_total kind={} stage={} total_ram_bytes={} active_radio_leases={} known_static_bytes={} known_static_pct_x100={} heap_used_bytes={} heap_used_pct_x100={} known_used_bytes={} known_used_pct_x100={} heap_free_bytes={} heap_free_pct_x100={} heap_pool_bytes={} heap_pool_pct_x100={} nonheap_remainder_bytes={} nonheap_remainder_pct_x100={}",
        kind,
        stage,
        TOTAL_SRAM_BYTES,
        radio_leases.active_count(),
        known_static_bytes,
        percent_x100(known_static_bytes),
        heap_used_bytes,
        percent_x100(heap_used_bytes),
        known_used_bytes,
        percent_x100(known_used_bytes),
        heap_free_bytes,
        percent_x100(heap_free_bytes),
        heap_pool_bytes,
        percent_x100(heap_pool_bytes),
        nonheap_remainder_bytes,
        percent_x100(nonheap_remainder_bytes)
    );
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure"))]
fn percent_x100(bytes: usize) -> usize {
    bytes.saturating_mul(10_000) / TOTAL_SRAM_BYTES
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure"))]
fn service_lease_error_name(error: ServiceLeaseError) -> &'static str {
    match error {
        ServiceLeaseError::AlreadyActive => "lease_already_active",
        ServiceLeaseError::NotActive => "lease_not_active",
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "vm-radio-measure",
    feature = "wifi"
))]
struct WifiServiceLease {
    controller: esp_radio::wifi::WifiController<'static>,
    radio_leases: *mut RadioLeaseManager,
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "vm-radio-measure",
    feature = "wifi"
))]
impl Drop for WifiServiceLease {
    fn drop(&mut self) {
        let _ = &self.controller;
        unsafe {
            let _ = (*self.radio_leases).release(RadioKind::Wifi);
        }
    }
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure", feature = "ble"))]
struct BleServiceLease {
    connector: esp_radio::ble::controller::BleConnector<'static>,
    radio_leases: *mut RadioLeaseManager,
}

#[cfg(all(target_arch = "riscv32", feature = "vm-radio-measure", feature = "ble"))]
impl Drop for BleServiceLease {
    fn drop(&mut self) {
        let _ = &self.connector;
        unsafe {
            let _ = (*self.radio_leases).release(RadioKind::Ble);
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
fn run_radio_probe(radio: RadioKind) {
    const CYCLE_COUNT: usize = 5;
    let mut cycles = [CycleSnapshot {
        radio,
        before_free_bytes: 0,
        active_free_bytes: 0,
        after_deinit_free_bytes: 0,
        before_largest_free_block: None,
        after_largest_free_block: None,
    }; CYCLE_COUNT];

    for (index, cycle) in cycles.iter_mut().enumerate() {
        println!(
            "radio_probe_stage cycle_start radio={} cycle={}",
            squidscript_fw_core::radio_lifecycle::radio_name(radio),
            index + 1
        );
        *cycle = match run_radio_cycle(radio, index + 1) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                println!(
                    "radio_probe_error radio={} cycle={} error={}",
                    squidscript_fw_core::radio_lifecycle::radio_name(radio),
                    index + 1,
                    error
                );
                return;
            }
        };
        let mut line = StackLine::<192>::new();
        let _ = format_cycle_snapshot(index + 1, cycle, &mut line);
        println!("{}", line.as_str());
    }

    let summary = squidscript_fw_core::radio_lifecycle::evaluate_reusable_reclaim(
        radio,
        &cycles,
        squidscript_fw_x4::radio_probe::REUSABLE_RECLAIM_GATE,
    );
    print_summary(&summary);
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
fn run_radio_cycle(radio: RadioKind, cycle_index: usize) -> Result<CycleSnapshot, &'static str> {
    print_heap_stats("before", radio, cycle_index);
    let before_free = heap_free_bytes();
    match radio {
        #[cfg(feature = "wifi")]
        RadioKind::Wifi => {
            println!("radio_probe_stage wifi_init");
            // The lifecycle harness recreates the singleton handle only after the
            // previous controller has been dropped.
            let wifi = unsafe { esp_hal::peripherals::WIFI::steal() };
            let controller = esp_radio::wifi::WifiController::new(wifi, Default::default())
                .map_err(|_| "wifi_init")?;
            println!("radio_probe_stage wifi_active");
            print_heap_stats("wifi_active", radio, cycle_index);
            let active_free = heap_free_bytes();
            drop(controller);
            println!("radio_probe_stage wifi_dropped");
            print_heap_stats("wifi_dropped", radio, cycle_index);
            settle_after_deinit();
            print_heap_stats("wifi_settled", radio, cycle_index);
            Ok(CycleSnapshot {
                radio,
                before_free_bytes: before_free,
                active_free_bytes: active_free,
                after_deinit_free_bytes: heap_free_bytes(),
                before_largest_free_block: None,
                after_largest_free_block: None,
            })
        }
        #[cfg(feature = "ble")]
        RadioKind::Ble => {
            println!("radio_probe_stage ble_init");
            // The lifecycle harness recreates the singleton handle only after the
            // previous connector has been dropped.
            let bt = unsafe { esp_hal::peripherals::BT::steal() };
            let connector = esp_radio::ble::controller::BleConnector::new(bt, Default::default())
                .map_err(|_| "ble_init")?;
            println!("radio_probe_stage ble_active");
            print_heap_stats("ble_active", radio, cycle_index);
            let active_free = heap_free_bytes();
            drop(connector);
            println!("radio_probe_stage ble_dropped");
            print_heap_stats("ble_dropped", radio, cycle_index);
            settle_after_deinit();
            print_heap_stats("ble_settled", radio, cycle_index);
            Ok(CycleSnapshot {
                radio,
                before_free_bytes: before_free,
                active_free_bytes: active_free,
                after_deinit_free_bytes: heap_free_bytes(),
                before_largest_free_block: None,
                after_largest_free_block: None,
            })
        }
        #[allow(unreachable_patterns)]
        _ => Err("radio_feature_disabled"),
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    any(feature = "vm-radio-measure", not(feature = "native-radio-services"))
))]
fn heap_free_bytes() -> usize {
    esp_alloc::HEAP.free()
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
fn print_heap_stats(stage: &str, radio: RadioKind, cycle: usize) {
    let stats = esp_alloc::HEAP.stats();
    println!(
        "heap_stats stage={} radio={} cycle={} free={} used={} max_used={} total_allocated={} total_freed={}",
        stage,
        squidscript_fw_core::radio_lifecycle::radio_name(radio),
        cycle,
        stats.size.saturating_sub(stats.current_usage),
        stats.current_usage,
        stats.max_usage,
        stats.total_allocated,
        stats.total_freed
    );
    for (index, region) in stats.region_stats.iter().enumerate() {
        if let Some(region) = region {
            println!(
                "heap_region stage={} radio={} cycle={} region={} size={} used={} free={}",
                stage,
                squidscript_fw_core::radio_lifecycle::radio_name(radio),
                cycle,
                index,
                region.size,
                region.used,
                region.free
            );
        }
    }
    #[cfg(feature = "alloc-trace")]
    print_live_allocations(stage, radio, cycle);
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    any(feature = "vm-radio-measure", not(feature = "native-radio-services"))
))]
fn settle_after_deinit() {
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
fn print_summary(summary: &ReclaimSummary) {
    let mut line = StackLine::<192>::new();
    let _ = squidscript_fw_core::radio_lifecycle::format_reclaim_summary(summary, &mut line);
    println!("{}", line.as_str());
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
struct StackLine<const N: usize> {
    buf: [u8; N],
    len: usize,
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
impl<const N: usize> StackLine<N> {
    const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("invalid_utf8")
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    not(feature = "vm-radio-measure"),
    not(feature = "native-radio-services")
))]
impl<const N: usize> core::fmt::Write for StackLine<N> {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        let available = N.saturating_sub(self.len);
        if value.len() > available {
            return Err(core::fmt::Error);
        }
        let end = self.len + value.len();
        self.buf[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
#[derive(Clone, Copy)]
struct AllocationRecord {
    ptr: usize,
    size: usize,
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
static mut LIVE_ALLOCATIONS: [AllocationRecord; 512] = [AllocationRecord { ptr: 0, size: 0 }; 512];

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
#[no_mangle]
pub extern "Rust" fn _esp_alloc_alloc(
    _heap: &esp_alloc::EspHeap,
    _caps: EnumSet<esp_alloc::MemoryCapability>,
    ptr: usize,
    size: usize,
) {
    unsafe {
        let base = core::ptr::addr_of_mut!(LIVE_ALLOCATIONS).cast::<AllocationRecord>();
        for index in 0..512 {
            let slot = base.add(index);
            if (*slot).ptr == 0 {
                *slot = AllocationRecord { ptr, size };
                break;
            }
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
#[no_mangle]
pub extern "Rust" fn _esp_alloc_dealloc(_heap: &esp_alloc::EspHeap, ptr: usize, _size: usize) {
    unsafe {
        let base = core::ptr::addr_of_mut!(LIVE_ALLOCATIONS).cast::<AllocationRecord>();
        for index in 0..512 {
            let slot = base.add(index);
            if (*slot).ptr == ptr {
                *slot = AllocationRecord { ptr: 0, size: 0 };
                break;
            }
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "alloc-trace",
    any(feature = "wifi", feature = "ble")
))]
fn print_live_allocations(stage: &str, radio: RadioKind, cycle: usize) {
    let mut total = 0;
    let mut count = 0;
    let mut sizes = [0usize; 8];
    unsafe {
        let base = core::ptr::addr_of!(LIVE_ALLOCATIONS).cast::<AllocationRecord>();
        for index in 0..512 {
            let slot = *base.add(index);
            if slot.ptr == 0 {
                continue;
            }
            total += slot.size;
            count += 1;
            if let Some(size_slot) = sizes
                .iter_mut()
                .find(|size_slot| **size_slot == 0 || **size_slot == slot.size)
            {
                *size_slot = slot.size;
            }
        }
    }
    println!(
        "live_allocs stage={} radio={} cycle={} count={} total={} sample_sizes={},{},{},{},{},{},{},{}",
        stage,
        squidscript_fw_core::radio_lifecycle::radio_name(radio),
        cycle,
        count,
        total,
        sizes[0],
        sizes[1],
        sizes[2],
        sizes[3],
        sizes[4],
        sizes[5],
        sizes[6],
        sizes[7]
    );
}

#[cfg(not(target_arch = "riscv32"))]
fn main() {}
