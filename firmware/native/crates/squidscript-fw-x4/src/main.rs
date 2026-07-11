#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
extern crate alloc;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
#[cfg(target_arch = "riscv32")]
use esp_backtrace as _;

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
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    gpio::Pull,
};

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
    request_pending_display_flush,
    target_input::{adc_bucket, InputClassifier, INPUT_BUTTONS, INPUT_DEBOUNCE_MS},
    x4_storage::{X4BinBookFileBackend, X4ContentStorage, X4SdFileStorage, X4StorageTime},
    NativeDisplayFlushDriver,
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
    NativeWifiBackendOperation,
};

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use esp_hal::ram;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
use core::fmt::Write as _;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
use core::net::Ipv4Addr;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};

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

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
use squidscript_fw_core::native_runtime::{NativeUploadRouteError, NativeUploadTransport};

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
use squidscript_fw_x4::ble_pipeline::should_report_ble_stage;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
use squidscript_fw_x4::ble_pipeline::{
    ble_connection_watchdog_ms, BleStorageCommand, BleStorageSession, BleUploadRoute,
    TransferSessionId, BLE_GATT_ATTRIBUTE_CAPACITY, BLE_PIPELINE_CHUNK_BYTES, BLE_PIPELINE_DEPTH,
};

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
use squidscript_fw_x4::radio_probe::radio_stack_metadata;

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    feature = "native-radio-services"
))]
#[inline(always)]
fn native_radio_log_args(_args: core::fmt::Arguments<'_>) {}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi", feature = "ble"),
    feature = "native-radio-services"
))]
macro_rules! native_radio_log {
    ($($arg:tt)*) => {
        native_radio_log_args(core::format_args!($($arg)*))
    };
}

#[cfg(target_arch = "riscv32")]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(target_arch = "riscv32")]
use squidscript_fw_core::app_store::NativeAppStorage;

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
    X4ContentStorage<
        X4SdFileStorage<SdStorage<X4SdBlockDevice, X4StorageTime>>,
        X4NativeAppStorage,
    >,
    512,
    8,
    128,
    1024,
    128,
    4,
>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(feature = "x4-binbook")]
type X4LittleFsStorage =
    squidscript_fw_x4::flash_partition::LittleFsAppStorage<esp_storage::FlashStorage<'static>>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(feature = "x4-binbook")]
type X4NativeAppStorage =
    squidscript_fw_x4::flash_partition::SharedLittleFsStorage<esp_storage::FlashStorage<'static>>;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
struct X4OtaRuntime {
    storage: X4NativeAppStorage,
    controller: squidscript_fw_x4::ota::OtaController,
    partition_table: [u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN],
    candidate_base: u32,
    candidate_size: usize,
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
static OTA_RUNTIME_PTR: AtomicUsize = AtomicUsize::new(0);
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
static OTA_REBOOT_PENDING: AtomicBool = AtomicBool::new(false);

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(feature = "x4-binbook")]
type X4NativeRuntime = NativeRuntime<
    EspRadioBackend,
    X4CommandDisplaySink,
    NoopBinBookBackend,
    X4NativeFileBackend,
    X4NativeAppStorage,
>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
#[cfg(not(feature = "x4-binbook"))]
type X4NativeRuntime = NativeRuntime<EspRadioBackend>;

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
type SharedX4NativeRuntime = embassy_sync_07::mutex::Mutex<
    embassy_sync_07::blocking_mutex::raw::CriticalSectionRawMutex,
    &'static mut X4NativeRuntime,
>;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
#[inline(never)]
fn build_shared_x4_runtime(
    cell: &'static static_cell::StaticCell<SharedX4NativeRuntime>,
    runtime_cell: &'static static_cell::StaticCell<X4NativeRuntime>,
    file_backend: X4NativeFileBackend,
    app_storage: X4NativeAppStorage,
    app_store_ready: bool,
) -> &'static SharedX4NativeRuntime {
    let runtime = runtime_cell.uninit();
    unsafe {
        NativeRuntime::init_in_place(
            runtime.as_mut_ptr(),
            EspRadioBackend::new(),
            X4CommandDisplaySink::new(),
            NoopBinBookBackend,
            file_backend,
            app_storage,
        );
    }
    let runtime = unsafe { runtime.assume_init_mut() };
    {
        let registry_ready = app_store_ready && runtime.rebuild_app_registry().is_ok();
        let heap = esp_alloc::HEAP.stats();
        runtime.set_system_memory_metrics(
            TOTAL_SRAM_BYTES,
            heap.current_usage,
            heap.size.saturating_sub(heap.current_usage),
        );
        let wake_boot = matches!(
            esp_hal::rtc_cntl::wakeup_cause(),
            esp_hal::system::SleepSource::Timer | esp_hal::system::SleepSource::Gpio
        );
        let mut restored_wake = false;
        if !app_store_ready {
            runtime.record_error("app_store_mount_failed");
        } else if !registry_ready {
            runtime.record_error("app_store_registry_failed");
        } else if wake_boot {
            let mut checkpoint_bytes = [0_u8; squidscript_fw_core::power::POWER_CHECKPOINT_BYTES];
            match runtime.load_power_checkpoint(&mut checkpoint_bytes) {
                Ok(Some(checkpoint)) => {
                    if runtime.delete_power_checkpoint().is_err() {
                        runtime.record_error("power_wake_checkpoint_consume_failed");
                    } else if runtime.restore_power_checkpoint(&checkpoint).is_ok() {
                        restored_wake = true;
                    } else {
                        runtime.record_error("power_wake_restore_failed");
                    }
                }
                Ok(None) => runtime.record_error("power_wake_checkpoint_missing"),
                Err(_) => {
                    let _ = runtime.delete_power_checkpoint();
                    runtime.record_error("power_wake_checkpoint_corrupt");
                }
            }
        }
        if restored_wake {
            native_radio_log!("power_wake_stage restored");
        } else if registry_ready
            && runtime
                .app_registry()
                .iter()
                .flatten()
                .any(|entry| entry.app_id() == "main")
        {
            if runtime.boot_app("main").is_err() {
                runtime.record_error("installed_main_launch_failed");
            }
        } else if registry_ready
            && runtime
                .launch_fallback(include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "/fallback-main.sqbc"
                )))
                .is_err()
        {
            runtime.record_error("fallback_main_launch_failed");
        }
    }
    cell.init_with(|| SharedX4NativeRuntime::new(runtime))
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
type X4DisplayPanel = X4Panel<
    FreqManagedSpiDevice<'static, Output<'static>>,
    Output<'static>,
    Output<'static>,
    Input<'static>,
>;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_TRANSFER_CHUNK_BYTES: usize = BLE_PIPELINE_CHUNK_BYTES;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_FILE_SERVICE_UUID: trouble_host::prelude::Uuid =
    trouble_host::prelude::Uuid::new_long(0x7e57c0de00014a5b8c6d0123456789abu128.to_le_bytes());

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_FILE_CTRL_UUID: trouble_host::prelude::Uuid =
    trouble_host::prelude::Uuid::new_long(0x7e57c0de00024a5b8c6d0123456789abu128.to_le_bytes());

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_FILE_DATA_UUID: trouble_host::prelude::Uuid =
    trouble_host::prelude::Uuid::new_long(0x7e57c0de00034a5b8c6d0123456789abu128.to_le_bytes());

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_FILE_STAT_UUID: trouble_host::prelude::Uuid =
    trouble_host::prelude::Uuid::new_long(0x7e57c0de00044a5b8c6d0123456789abu128.to_le_bytes());

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_OP_BEGIN: u8 = 0x01;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_OP_NAME: u8 = 0x02;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_OP_ABORT: u8 = 0x03;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_STATUS_COMPLETE: u8 = 0x00;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_STATUS_ERROR: u8 = 0x01;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_STATUS_ROUTE_AMBIGUOUS: u8 = 0x11;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_STATUS_PENDING: u8 = 0x7f;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
static BLE_PROFILE_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
static POWER_SLEEP_READY: AtomicBool = AtomicBool::new(false);
#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
static POWER_SLEEP_WAKE_AFTER_MS: AtomicU32 = AtomicU32::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
static BLE_STORAGE_COMMANDS: embassy_sync_07::channel::Channel<
    embassy_sync_07::blocking_mutex::raw::CriticalSectionRawMutex,
    BleStorageCommand,
    BLE_PIPELINE_DEPTH,
> = embassy_sync_07::channel::Channel::new();

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
static BLE_RUNTIME_STATUSES: embassy_sync_07::channel::Channel<
    embassy_sync_07::blocking_mutex::raw::CriticalSectionRawMutex,
    u8,
    2,
> = embassy_sync_07::channel::Channel::new();

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
static BLE_TRANSFER_SESSION_SEQUENCE: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(1);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
static BLE_CANCELLED_SESSION: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
static BLE_DIAGNOSTIC_STAGE: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
static BLE_DIAGNOSTIC_FLAGS: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
static BLE_DIAGNOSTIC_QUEUE_HIGH_WATER: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
static BLE_DIAGNOSTIC_GATT_OTHER_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
static BLE_DIAGNOSTIC_DISCONNECT_REASON: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
static BLE_DIAGNOSTIC_ERROR_STAGE: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_RUNNER_EXIT: usize = 1;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_GATT_ATTACH_FAILURE: usize = 2;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_GATT_EVENT: usize = 4;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_BACKPRESSURE: usize = 8;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_BEGIN_WRITE: usize = 16;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_NAME_WRITE: usize = 32;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_NAME_ACCEPTED: usize = 64;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_WRITE_REJECTED: usize = 128;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_ACCEPT_FAILED: usize = 256;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_NOTIFY_SENT: usize = 512;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_NOTIFY_FAILED: usize = 1024;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_CONNECTION_WATCHDOG: usize = 2048;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_DISCONNECTED: usize = 4096;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_CONNECTION_PARAMS_REQUEST: usize = 8192;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_GATT_OTHER: usize = 16384;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_DATA_WRITE: usize = 32768;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_CONNECTION_PARAMS_ACCEPTED: usize = 65536;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_CONNECTION_PARAMS_FAILED: usize = 131072;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_STATUS_CCCD_INDICATE: usize = 262144;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_STATUS_CCCD_NOTIFY: usize = 524288;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_STATUS_CCCD_DISABLED: usize = 1048576;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_STATUS_INDICATE_ENABLED: usize = 2097152;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_STATUS_INDICATE_DISABLED: usize = 4194304;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_STATUS_NOTIFY_ENABLED: usize = 8388608;
#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
const BLE_DIAGNOSTIC_STATUS_READ: usize = 16777216;

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
const BLE_CONNECTION_WATCHDOG_MS: u64 =
    ble_connection_watchdog_ms(option_env!("SQUIDSCRIPT_BLE_CONNECTION_WATCHDOG_MS"));

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
fn ble_diagnostic_set_flag(flag: usize) {
    let flags = BLE_DIAGNOSTIC_FLAGS.load(Ordering::Acquire);
    BLE_DIAGNOSTIC_FLAGS.store(flags | flag, Ordering::Release);
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble",
    debug_assertions
))]
fn ble_diagnostic_record_queue_depth() {
    let depth = BLE_STORAGE_COMMANDS.len();
    let high_water = BLE_DIAGNOSTIC_QUEUE_HIGH_WATER.load(Ordering::Acquire);
    if depth > high_water {
        BLE_DIAGNOSTIC_QUEUE_HIGH_WATER.store(depth, Ordering::Release);
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
static BLE_STORAGE_CANCELS: embassy_sync_07::channel::Channel<
    embassy_sync_07::blocking_mutex::raw::CriticalSectionRawMutex,
    TransferSessionId,
    2,
> = embassy_sync_07::channel::Channel::new();

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
static WIFI_AP_CLIENT_COUNT: AtomicI32 = AtomicI32::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
static WIFI_DHCP_LEASE_COUNT: AtomicI32 = AtomicI32::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
static WIFI_CONTROLLER_PTR: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
static WIFI_RUNTIME_PTR: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
static WIFI_COMMANDS: embassy_sync_07::channel::Channel<
    embassy_sync_07::blocking_mutex::raw::CriticalSectionRawMutex,
    NativeWifiCommand,
    2,
> = embassy_sync_07::channel::Channel::new();

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
static WIFI_SCAN_RESULTS: critical_section::Mutex<
    core::cell::RefCell<heapless::Vec<squidvm_core::host::WifiAccessPoint, 8>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(heapless::Vec::new()));

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[derive(Clone)]
enum NativeWifiCommand {
    Scan,
    Connect {
        ssid: heapless::String<32>,
        password: heapless::String<64>,
    },
    StartAp {
        ssid: heapless::String<32>,
    },
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
#[derive(Clone)]
struct BleGattTransferState {
    name: heapless::String<{ squid_device_protocol::MAX_CONTENT_NAME_BYTES }>,
    expected_name_len: usize,
    total_len: usize,
    received: usize,
    session_id: Option<TransferSessionId>,
    begin_sent: bool,
    error: bool,
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
impl BleGattTransferState {
    const fn new() -> Self {
        Self {
            name: heapless::String::new(),
            expected_name_len: 0,
            total_len: 0,
            received: 0,
            session_id: None,
            begin_sent: false,
            error: false,
        }
    }

    fn clear(&mut self) {
        self.name.clear();
        self.expected_name_len = 0;
        self.total_len = 0;
        self.received = 0;
        self.session_id = None;
        self.begin_sent = false;
        self.error = false;
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
enum BleGattWriteOutcome {
    Reject,
    Accept,
    Enqueue(BleStorageCommand),
    EnqueueAndCommit(BleStorageCommand, TransferSessionId),
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
impl BleGattWriteOutcome {
    const fn queue_slots(&self) -> usize {
        match self {
            Self::Reject | Self::Accept => 0,
            Self::Enqueue(_) => 1,
            Self::EnqueueAndCommit(_, _) => 2,
        }
    }

    const fn is_accepted(&self) -> bool {
        !matches!(self, Self::Reject)
    }

    async fn enqueue(self) {
        match self {
            Self::Reject | Self::Accept => {}
            Self::Enqueue(command) => BLE_STORAGE_COMMANDS.send(command).await,
            Self::EnqueueAndCommit(command, session_id) => {
                BLE_STORAGE_COMMANDS.send(command).await;
                BLE_STORAGE_COMMANDS
                    .send(BleStorageCommand::Commit { session_id })
                    .await;
            }
        }
        #[cfg(debug_assertions)]
        ble_diagnostic_record_queue_depth();
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
fn ble_report_status(_stage: u32, status: u8) {
    #[cfg(debug_assertions)]
    BLE_DIAGNOSTIC_ERROR_STAGE.store(_stage as usize, Ordering::Release);
    let _ = BLE_RUNTIME_STATUSES.try_send(status);
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    not(feature = "x4-binbook")
))]
#[embassy_executor::task]
async fn native_radio_serial_task(
    serial: UsbSerialJtag<'static, esp_hal::Blocking>,
    runtime: &'static SharedX4NativeRuntime,
    buffers: &'static mut SerialProtocolBuffers,
    display_flush: &'static mut NoDisplayFlushTask,
) {
    run_serial_protocol_cooperative(serial, runtime, buffers, display_flush).await;
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
#[embassy_executor::task]
async fn native_input_task(
    adc1: esp_hal::peripherals::ADC1<'static>,
    gpio1: esp_hal::peripherals::GPIO1<'static>,
    gpio2: esp_hal::peripherals::GPIO2<'static>,
    gpio3: esp_hal::peripherals::GPIO3<'static>,
    lpwr: esp_hal::peripherals::LPWR<'static>,
    runtime: &'static SharedX4NativeRuntime,
) {
    let mut config = AdcConfig::new();
    let mut adc1_pin = config.enable_pin(gpio1, Attenuation::_11dB);
    let mut adc2_pin = config.enable_pin(gpio2, Attenuation::_11dB);
    let mut adc = Adc::new(adc1, config).into_async();
    let power = Input::new(gpio3, InputConfig::default().with_pull(Pull::Up));
    let mut rtc = esp_hal::rtc_cntl::Rtc::new(lpwr);
    let mut classifier = InputClassifier::new();
    let mut previous_raw = ("none", "none", true);
    let mut previous_stable = 0u8;
    let mut last_sample = embassy_time::Instant::now();

    loop {
        embassy_time::Timer::after_millis(INPUT_DEBOUNCE_MS as u64).await;
        if POWER_SLEEP_READY.load(Ordering::Acquire) {
            POWER_SLEEP_READY.store(false, Ordering::Release);
            let wake_after_ms = POWER_SLEEP_WAKE_AFTER_MS.load(Ordering::Acquire);
            drop(power);
            let mut wake_gpio = unsafe { esp_hal::peripherals::GPIO3::steal() };
            let mut wakeup_pins: [(
                &mut dyn esp_hal::gpio::RtcPinWithResistors,
                esp_hal::rtc_cntl::sleep::WakeupLevel,
            ); 1] = [(&mut wake_gpio, esp_hal::rtc_cntl::sleep::WakeupLevel::Low)];
            let power_wakeup = esp_hal::rtc_cntl::sleep::RtcioWakeupSource::new(&mut wakeup_pins);
            if wake_after_ms == 0 {
                rtc.sleep_deep(&[&power_wakeup]);
            }
            let timer_wakeup = esp_hal::rtc_cntl::sleep::TimerWakeupSource::new(
                core::time::Duration::from_millis(u64::from(wake_after_ms)),
            );
            rtc.sleep_deep(&[&power_wakeup, &timer_wakeup]);
        }
        let adc1_value = adc.read_oneshot(&mut adc1_pin).await;
        let adc2_value = adc.read_oneshot(&mut adc2_pin).await;
        let power_high = power.is_high();
        let now = embassy_time::Instant::now();
        let elapsed_ms = now
            .duration_since(last_sample)
            .as_millis()
            .clamp(1, u32::MAX as u64) as u32;
        last_sample = now;

        let raw = (
            adc_bucket(1, adc1_value),
            adc_bucket(2, adc2_value),
            power_high,
        );
        let mut events = [None; 8];
        let mut event_len = 0usize;
        classifier.sample(adc1_value, adc2_value, power_high, elapsed_ms, |event| {
            if event_len < events.len() {
                events[event_len] = Some(event);
                event_len += 1;
            }
        });
        let stable = classifier.stable_mask();

        #[cfg(debug_assertions)]
        if raw != previous_raw || stable != previous_stable || event_len != 0 {
            let mut runtime = runtime.lock().await;
            if raw != previous_raw {
                let mut line = heapless::String::<128>::new();
                let _ = core::fmt::write(
                    &mut line,
                    format_args!(
                        "input.raw adc1={} adc2={} power={}",
                        raw.0,
                        raw.1,
                        if raw.2 { "released" } else { "pressed" }
                    ),
                );
                runtime.record_debug_trace(line.as_str());
            }
            if stable != previous_stable {
                for (index, button) in INPUT_BUTTONS.iter().enumerate() {
                    let bit = 1u8 << index;
                    if (stable ^ previous_stable) & bit == 0 {
                        continue;
                    }
                    let mut line = heapless::String::<128>::new();
                    let _ = core::fmt::write(
                        &mut line,
                        format_args!(
                            "input.debounced key={} state={}",
                            button.logical,
                            if stable & bit == 0 {
                                "released"
                            } else {
                                "pressed"
                            }
                        ),
                    );
                    runtime.record_debug_trace(line.as_str());
                }
            }
            for event in events[..event_len].iter().flatten() {
                let mut line = heapless::String::<128>::new();
                let _ = core::fmt::write(&mut line, format_args!("input.classified event={event}"));
                runtime.record_debug_trace(line.as_str());
            }
        }

        for event in events[..event_len].iter().flatten() {
            let mut runtime = runtime.lock().await;
            let routed = runtime.enqueue_input_event(event).is_ok();
            #[cfg(debug_assertions)]
            {
                let mut line = heapless::String::<128>::new();
                let _ = core::fmt::write(
                    &mut line,
                    format_args!(
                        "input.route event={} result={}",
                        event,
                        if routed { "ok" } else { "error" }
                    ),
                );
                runtime.record_debug_trace(line.as_str());
            }
            let _ = routed;
        }
        previous_raw = raw;
        previous_stable = stable;
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
#[embassy_executor::task]
async fn native_ble_file_transfer_task(bt: esp_hal::peripherals::BT<'static>) {
    run_ble_file_transfer_task(bt).await;
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
#[embassy_executor::task]
async fn native_ble_storage_task(runtime: &'static SharedX4NativeRuntime) {
    run_ble_storage_task(runtime).await;
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[embassy_executor::task]
async fn native_wifi_event_task(runtime: &'static SharedX4NativeRuntime) {
    let subscriber = loop {
        let controller_ptr = WIFI_CONTROLLER_PTR.load(Ordering::Acquire);
        if controller_ptr == 0 {
            embassy_time::Timer::after_millis(25).await;
            continue;
        }
        // SAFETY: the pointer comes from a backend-owned `WifiController` stored in a
        // static runtime instance and remains valid for the process lifetime.
        let maybe_subscriber = unsafe {
            let _controller = &*(controller_ptr as *const esp_radio::wifi::WifiController<'static>);
            _controller.subscribe()
        };
        match maybe_subscriber {
            Ok(new_subscriber) => break new_subscriber,
            Err(_) => {
                embassy_time::Timer::after_millis(25).await;
                continue;
            }
        }
    };
    let mut subscriber = subscriber;
    loop {
        match subscriber.next_event_pure().await {
            esp_radio::wifi::event::EventInfo::AccessPointStationConnected { .. } => {
                let next = WIFI_AP_CLIENT_COUNT
                    .load(Ordering::Acquire)
                    .saturating_add(1);
                WIFI_AP_CLIENT_COUNT.store(next, Ordering::Release);
            }
            esp_radio::wifi::event::EventInfo::AccessPointStationDisconnected { .. } => {
                let next = WIFI_AP_CLIENT_COUNT
                    .load(Ordering::Acquire)
                    .saturating_sub(1);
                WIFI_AP_CLIENT_COUNT.store(next, Ordering::Release);
            }
            esp_radio::wifi::event::EventInfo::AccessPointStart => {
                let mut runtime = runtime.lock().await;
                runtime.radio_backend_mut().record_access_point_started();
                let _ = runtime.complete_wifi_start_ap();
            }
            esp_radio::wifi::event::EventInfo::StationConnected {
                ssid,
                bssid,
                authmode,
                ..
            } => {
                let mut runtime = runtime.lock().await;
                runtime.radio_backend_mut().record_station_connected_event(
                    ssid.as_str(),
                    bssid,
                    esp_radio::wifi::AuthenticationMethod::from_raw(authmode),
                );
                let _ = runtime.complete_wifi_connect();
            }
            esp_radio::wifi::event::EventInfo::StationDisconnected {
                ssid: _,
                bssid,
                reason,
                ..
            } => {
                let mut runtime = runtime.lock().await;
                runtime
                    .radio_backend_mut()
                    .record_station_disconnected_event(
                        bssid,
                        reason.into(),
                        Some("connect-disconnected"),
                    );
                let _ = runtime.fail_wifi_connect("connect");
            }
            _ => {}
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[embassy_executor::task]
async fn native_wifi_command_task(runtime: &'static SharedX4NativeRuntime) {
    loop {
        match WIFI_COMMANDS.receive().await {
            NativeWifiCommand::Scan => {
                let controller_ptr = loop {
                    let ptr = WIFI_CONTROLLER_PTR.load(Ordering::Acquire);
                    if ptr != 0 {
                        break ptr;
                    }
                    embassy_time::Timer::after_millis(10).await;
                };
                let config = esp_radio::wifi::scan::ScanConfig::default().with_max(8);
                // SAFETY: the runtime owns this controller for the process lifetime while
                // the Wi-Fi lease is active. The command path is the only native code that
                // mutably drives scan work.
                let result = unsafe {
                    let controller =
                        &mut *(controller_ptr as *mut esp_radio::wifi::WifiController<'static>);
                    let station_config = esp_radio::wifi::Config::Station(
                        esp_radio::wifi::sta::StationConfig::default(),
                    );
                    match controller.set_config(&station_config) {
                        Ok(()) => match controller.start() {
                            Ok(()) => controller.scan_async(&config).await,
                            Err(error) => Err(error),
                        },
                        Err(error) => Err(error),
                    }
                };
                match result {
                    Ok(results) => {
                        let mut count = 0i32;
                        critical_section::with(|cs| {
                            let mut stored = WIFI_SCAN_RESULTS.borrow_ref_mut(cs);
                            stored.clear();
                            for info in results {
                                if let Ok(network) = squidvm_core::host::WifiAccessPoint::new(
                                    info.ssid.as_str().as_bytes(),
                                    Some(info.bssid),
                                    info.channel as i32,
                                    info.signal_strength as i32,
                                    wifi_auth_label(info.auth_method),
                                    info.ssid.is_empty(),
                                ) {
                                    if stored.push(network).is_ok() {
                                        count = count.saturating_add(1);
                                    }
                                }
                            }
                        });
                        let mut runtime = runtime.lock().await;
                        let _ = runtime.complete_wifi_scan(count);
                    }
                    Err(_) => {
                        let mut runtime = runtime.lock().await;
                        let _ = runtime.fail_wifi_scan("scan failed");
                    }
                }
            }
            NativeWifiCommand::Connect { ssid, password } => {
                let controller_ptr = loop {
                    let ptr = WIFI_CONTROLLER_PTR.load(Ordering::Acquire);
                    if ptr != 0 {
                        break ptr;
                    }
                    embassy_time::Timer::after_millis(10).await;
                };
                let config = esp_radio::wifi::Config::Station(
                    esp_radio::wifi::sta::StationConfig::default()
                        .with_ssid(ssid.as_str())
                        .with_password(password.as_str().into()),
                );
                let result = unsafe {
                    let controller =
                        &mut *(controller_ptr as *mut esp_radio::wifi::WifiController<'static>);
                    match controller.set_config(&config) {
                        Ok(()) => match controller.start() {
                            Ok(()) => controller.connect_station(),
                            Err(error) => Err(error),
                        },
                        Err(error) => Err(error),
                    }
                };
                match result {
                    Ok(()) => {
                        embassy_time::Timer::after_secs(20).await;
                        let mut runtime = runtime.lock().await;
                        if !runtime.wifi_operation_result().ready {
                            runtime
                                .radio_backend_mut()
                                .record_station_connect_failure("connect-timeout");
                            let _ = runtime.fail_wifi_connect("timeout");
                        }
                    }
                    Err(_) => {
                        let mut runtime = runtime.lock().await;
                        runtime
                            .radio_backend_mut()
                            .record_station_connect_failure("connect");
                        let _ = runtime.fail_wifi_connect("connect");
                    }
                }
            }
            NativeWifiCommand::StartAp { ssid } => {
                let controller_ptr = loop {
                    let ptr = WIFI_CONTROLLER_PTR.load(Ordering::Acquire);
                    if ptr != 0 {
                        break ptr;
                    }
                    embassy_time::Timer::after_millis(10).await;
                };
                let config = esp_radio::wifi::Config::AccessPoint(
                    esp_radio::wifi::ap::AccessPointConfig::default().with_ssid(ssid.as_str()),
                );
                let result = unsafe {
                    let controller =
                        &mut *(controller_ptr as *mut esp_radio::wifi::WifiController<'static>);
                    controller.set_config(&config)
                };
                match result {
                    Ok(()) => {
                        let mut runtime = runtime.lock().await;
                        runtime.radio_backend_mut().record_access_point_start_ok();
                        let _ = runtime.complete_wifi_start_ap();
                    }
                    Err(_) => {
                        let mut runtime = runtime.lock().await;
                        runtime
                            .radio_backend_mut()
                            .record_access_point_start_failure("set-config");
                        let _ = runtime.fail_wifi_start_ap("set-config");
                    }
                }
            }
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[embassy_executor::task]
async fn native_wifi_ap_stack_task(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::Interface>,
) -> ! {
    runner.run().await
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[embassy_executor::task]
async fn native_wifi_sta_stack_task(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::Interface>,
) -> ! {
    runner.run().await
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[embassy_executor::task]
async fn native_wifi_sta_ip_task(stack: embassy_net::Stack<'static>) {
    loop {
        let runtime_ptr = loop {
            let ptr = WIFI_RUNTIME_PTR.load(Ordering::Acquire);
            if ptr != 0 {
                break ptr;
            }
            embassy_time::Timer::after_millis(25).await;
        };
        let ip = stack.config_v4().map(|config| config.address.address());
        // SAFETY: the pointer is published after the static runtime is initialized,
        // and that runtime lives for the firmware process lifetime.
        let runtime = unsafe { &*(runtime_ptr as *const SharedX4NativeRuntime) };
        {
            let mut runtime = runtime.lock().await;
            runtime.radio_backend_mut().record_station_ip(ip);
        }
        embassy_time::Timer::after_millis(500).await;
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[embassy_executor::task]
async fn native_wifi_ap_dhcp_task(stack: embassy_net::Stack<'static>) {
    use edge_dhcp::{
        server::{Server, ServerOptions},
        Options, Packet,
    };
    use embassy_net::udp::{PacketMetadata, UdpSocket};

    static RX_META: static_cell::StaticCell<[PacketMetadata; 4]> = static_cell::StaticCell::new();
    static TX_META: static_cell::StaticCell<[PacketMetadata; 4]> = static_cell::StaticCell::new();
    static RX_BUF: static_cell::StaticCell<[u8; 2048]> = static_cell::StaticCell::new();
    static TX_BUF: static_cell::StaticCell<[u8; 2048]> = static_cell::StaticCell::new();
    static WORK_BUF: static_cell::StaticCell<[u8; 1500]> = static_cell::StaticCell::new();
    static REPLY_BUF: static_cell::StaticCell<[u8; 1500]> = static_cell::StaticCell::new();

    let rx_meta = RX_META.init_with(|| [PacketMetadata::EMPTY; 4]);
    let tx_meta = TX_META.init_with(|| [PacketMetadata::EMPTY; 4]);
    let rx_buf = RX_BUF.init_with(|| [0; 2048]);
    let tx_buf = TX_BUF.init_with(|| [0; 2048]);
    let work_buf = WORK_BUF.init_with(|| [0; 1500]);
    let reply_buf = REPLY_BUF.init_with(|| [0; 1500]);

    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    if socket.bind(67).is_err() {
        return;
    }

    let server_ip = Ipv4Addr::new(192, 168, 4, 1);
    let mut gateway = [server_ip];
    let options = ServerOptions::new(server_ip, Some(&mut gateway));
    let mut server = Server::<_, 4>::new(|| embassy_time::Instant::now().as_secs(), server_ip);

    loop {
        let Ok((len, _remote)) = socket.recv_from(work_buf).await else {
            continue;
        };
        let Ok(request) = Packet::decode(&work_buf[..len]) else {
            continue;
        };
        let mut opt_buf = Options::buf();
        if let Some(reply) = server.handle_request(&mut opt_buf, &options, &request) {
            let Ok(encoded_reply) = reply.encode(reply_buf) else {
                continue;
            };
            WIFI_DHCP_LEASE_COUNT.store(server.leases.len() as i32, Ordering::Release);
            let _ = socket
                .send_to(encoded_reply, (embassy_net::Ipv4Address::BROADCAST, 68))
                .await;
        } else {
            WIFI_DHCP_LEASE_COUNT.store(server.leases.len() as i32, Ordering::Release);
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
#[embassy_executor::task]
async fn native_http_upload_task(stack: embassy_net::Stack<'static>) {
    use core::fmt::Write;
    use embassy_net::tcp::TcpSocket;
    use squidscript_fw_x4::http_upload::{header_end, parse_request, Method};

    static RX_BUF: static_cell::StaticCell<[u8; 1024]> = static_cell::StaticCell::new();
    static TX_BUF: static_cell::StaticCell<[u8; 512]> = static_cell::StaticCell::new();
    static HEADER_BUF: static_cell::StaticCell<[u8; 1024]> = static_cell::StaticCell::new();
    static BODY_BUF: static_cell::StaticCell<[u8; 512]> = static_cell::StaticCell::new();

    let rx_buf = RX_BUF.init_with(|| [0; 1024]);
    let tx_buf = TX_BUF.init_with(|| [0; 512]);
    let header_buf = HEADER_BUF.init_with(|| [0; 1024]);
    let body_buf = BODY_BUF.init_with(|| [0; 512]);
    let mut socket = TcpSocket::new(stack, rx_buf, tx_buf);
    socket.set_timeout(Some(embassy_time::Duration::from_secs(30)));
    let runtime = loop {
        let ptr = WIFI_RUNTIME_PTR.load(Ordering::Acquire);
        if ptr != 0 {
            // SAFETY: the published runtime is static and remains valid for firmware lifetime.
            break unsafe { &*(ptr as *const SharedX4NativeRuntime) };
        }
        embassy_time::Timer::after_millis(25).await;
    };

    loop {
        if socket.accept(80).await.is_err() {
            socket.abort();
            embassy_futures::yield_now().await;
            continue;
        }
        let result = handle_native_http_upload(&mut socket, runtime, header_buf, body_buf).await;
        if let Err(error) = result {
            let mut runtime = runtime.lock().await;
            runtime.record_error(error);
        }
        socket.close();
        let _ = socket.flush().await;
        socket.abort();
        header_buf.fill(0);
        body_buf.fill(0);
        embassy_futures::yield_now().await;
    }

    async fn handle_native_http_upload(
        socket: &mut TcpSocket<'_>,
        runtime: &'static SharedX4NativeRuntime,
        header_buf: &mut [u8; 1024],
        body_buf: &mut [u8; 512],
    ) -> Result<(), &'static str> {
        let mut used = 0usize;
        let header_len = loop {
            if let Some(end) = header_end(&header_buf[..used]) {
                break end;
            }
            if used == header_buf.len() {
                send_http_response(socket, 400, "Bad Request", "bad request\n").await;
                return Ok(());
            }
            let received = socket
                .read(&mut header_buf[used..])
                .await
                .map_err(|_| "http-header-read")?;
            if received == 0 {
                return Ok(());
            }
            used += received;
        };
        let headers = core::str::from_utf8(&header_buf[..header_len]).map_err(|_| "http-header")?;
        let request = match parse_request(headers) {
            Ok(request) => request,
            Err(_) => {
                send_http_response(socket, 400, "Bad Request", "bad request\n").await;
                return Ok(());
            }
        };
        let mut name = heapless::String::<64>::new();
        if name.push_str(request.name).is_err() {
            send_http_response(socket, 400, "Bad Request", "bad name\n").await;
            return Ok(());
        }

        let route = {
            let mut runtime = runtime.lock().await;
            match runtime.resolve_upload_route(name.as_str(), NativeUploadTransport::Http) {
                Ok(route) => route,
                Err(NativeUploadRouteError::RouteAmbiguous) => {
                    drop(runtime);
                    send_http_response(socket, 409, "Conflict", "route ambiguous\n").await;
                    return Ok(());
                }
                Err(NativeUploadRouteError::NoActiveProfile)
                | Err(NativeUploadRouteError::RouteMismatch) => {
                    drop(runtime);
                    send_http_response(socket, 404, "Not Found", "inactive\n").await;
                    return Ok(());
                }
                Err(NativeUploadRouteError::InvalidMetadata) => {
                    drop(runtime);
                    send_http_response(socket, 500, "Internal Server Error", "metadata error\n")
                        .await;
                    return Err("http-upload-metadata");
                }
            }
        };
        let mut profile_id = heapless::String::<32>::new();
        let mut complete_event = heapless::String::<64>::new();
        if profile_id.push_str(route.profile_id.as_str()).is_err()
            || complete_event
                .push_str(route.complete_event.as_str())
                .is_err()
        {
            send_http_response(socket, 500, "Internal Server Error", "route error\n").await;
            return Err("http-upload-route-size");
        }

        if request.method == Method::Head {
            let (offset, total) = {
                let runtime = runtime.lock().await;
                match runtime.active_upload_progress() {
                    Some(progress)
                        if progress.name == name.as_str()
                            && progress.transport == NativeUploadTransport::Http =>
                    {
                        (progress.bytes_received, progress.total_bytes)
                    }
                    _ => (0, 0),
                }
            };
            let mut response = heapless::String::<256>::new();
            if total > 0 {
                let _ = write!(
                    response,
                    "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nX-Squid-Upload-Offset: {offset}\r\nX-Squid-Upload-Total: {total}\r\nConnection: close\r\n\r\n"
                );
            } else {
                let _ = write!(
                    response,
                    "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nX-Squid-Upload-Offset: 0\r\nConnection: close\r\n\r\n"
                );
            }
            send_all(socket, response.as_bytes()).await;
            return Ok(());
        }

        let (start, total) = request
            .content_range
            .map(|range| (range.start, range.total))
            .unwrap_or((0, request.content_length));
        let staged_path = {
            let mut runtime = runtime.lock().await;
            if let Some(progress) = runtime.active_upload_progress() {
                let exact = progress.name == name.as_str()
                    && progress.id == profile_id.as_str()
                    && progress.transport == NativeUploadTransport::Http
                    && progress.total_bytes == total
                    && progress.bytes_received == start;
                if !exact {
                    drop(runtime);
                    send_http_response(socket, 409, "Conflict", "offset mismatch\n").await;
                    return Ok(());
                }
            } else if start != 0 {
                drop(runtime);
                send_http_response(socket, 409, "Conflict", "offset mismatch\n").await;
                return Ok(());
            }
            match runtime.begin_ephemeral_upload(
                name.as_str(),
                total,
                profile_id.as_str(),
                NativeUploadTransport::Http,
            ) {
                Ok(path) => {
                    let mut copied = heapless::String::<128>::new();
                    if copied.push_str(path).is_err() {
                        drop(runtime);
                        send_http_response(socket, 413, "Content Too Large", "too large\n").await;
                        return Ok(());
                    }
                    copied
                }
                Err(squidscript_fw_core::native_runtime::NativeRuntimeError::TooLarge) => {
                    drop(runtime);
                    send_http_response(socket, 413, "Content Too Large", "too large\n").await;
                    return Ok(());
                }
                Err(
                    squidscript_fw_core::native_runtime::NativeRuntimeError::UploadSessionActive,
                ) => {
                    drop(runtime);
                    send_http_response(socket, 409, "Conflict", "upload busy\n").await;
                    return Ok(());
                }
                Err(_) => {
                    drop(runtime);
                    send_http_response(socket, 500, "Internal Server Error", "storage error\n")
                        .await;
                    return Err("http-upload-begin");
                }
            }
        };

        let mut written = 0usize;
        let initial = used.saturating_sub(header_len).min(request.content_length);
        while written < initial {
            let chunk_len = body_buf.len().min(initial - written);
            {
                let mut runtime = runtime.lock().await;
                if runtime
                    .write_ephemeral_upload_chunk(
                        staged_path.as_str(),
                        start + written,
                        &header_buf[header_len + written..header_len + written + chunk_len],
                    )
                    .is_err()
                {
                    drop(runtime);
                    send_http_response(socket, 500, "Internal Server Error", "storage error\n")
                        .await;
                    return Err("http-upload-write");
                }
            }
            written += chunk_len;
            embassy_futures::yield_now().await;
        }
        while written < request.content_length {
            let want = body_buf.len().min(request.content_length - written);
            let received = match socket.read(&mut body_buf[..want]).await {
                Ok(0) | Err(_) => return Ok(()),
                Ok(received) => received,
            };
            {
                let mut runtime = runtime.lock().await;
                if runtime
                    .write_ephemeral_upload_chunk(
                        staged_path.as_str(),
                        start + written,
                        &body_buf[..received],
                    )
                    .is_err()
                {
                    drop(runtime);
                    send_http_response(socket, 500, "Internal Server Error", "storage error\n")
                        .await;
                    return Err("http-upload-write");
                }
            }
            written += received;
            embassy_futures::yield_now().await;
        }

        let completed = {
            let mut runtime = runtime.lock().await;
            runtime
                .commit_ephemeral_upload(staged_path.as_str(), start + written)
                .and_then(|()| {
                    runtime.dispatch_active_upload_complete(
                        complete_event.as_str(),
                        staged_path.as_str(),
                    )
                })
        };
        if completed.is_err() {
            send_http_response(socket, 500, "Internal Server Error", "dispatch error\n").await;
            return Err("http-upload-dispatch");
        }
        send_http_response(socket, 200, "OK", "ok\n").await;
        Ok(())
    }

    async fn send_http_response(socket: &mut TcpSocket<'_>, status: u16, reason: &str, body: &str) {
        let mut response = heapless::String::<256>::new();
        let _ = write!(
            response,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        send_all(socket, response.as_bytes()).await;
    }

    async fn send_all(socket: &mut TcpSocket<'_>, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            match socket.write(bytes).await {
                Ok(0) | Err(_) => return,
                Ok(written) => bytes = &bytes[written..],
            }
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "x4-binbook",
    feature = "native-radio-services"
))]
#[embassy_executor::task]
async fn native_radio_serial_task(
    serial: UsbSerialJtag<'static, esp_hal::Blocking>,
    runtime: &'static SharedX4NativeRuntime,
    buffers: &'static mut SerialProtocolBuffers,
    display_flush: &'static mut StreamingDisplayFlushTask<X4DisplayPanel>,
) {
    run_serial_protocol_cooperative(serial, runtime, buffers, display_flush).await;
}

#[cfg(all(target_arch = "riscv32", not(any(feature = "wifi", feature = "ble"))))]
#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    #[cfg(feature = "x4-binbook")]
    type DefaultRuntime = NativeRuntime<NoopRadioBackend, X4FramebufferDisplaySink>;
    #[cfg(not(feature = "x4-binbook"))]
    type DefaultRuntime = NativeRuntime;

    static RUNTIME: static_cell::StaticCell<DefaultRuntime> = static_cell::StaticCell::new();
    static BUFFERS: static_cell::StaticCell<SerialProtocolBuffers> = static_cell::StaticCell::new();
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

#[cfg(all(target_arch = "riscv32", any(feature = "wifi", feature = "ble")))]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) {
    #[cfg(feature = "vm-radio-measure")]
    static RUNTIME: static_cell::StaticCell<NativeRuntime> = static_cell::StaticCell::new();
    #[cfg(feature = "vm-radio-measure")]
    static BUFFERS: static_cell::StaticCell<SerialProtocolBuffers> = static_cell::StaticCell::new();
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
    static RUNTIME: static_cell::StaticCell<SharedX4NativeRuntime> = static_cell::StaticCell::new();
    static RUNTIME_VALUE: static_cell::StaticCell<X4NativeRuntime> = static_cell::StaticCell::new();
    #[cfg(all(feature = "native-radio-services", not(feature = "vm-radio-measure")))]
    static BUFFERS: static_cell::StaticCell<SerialProtocolBuffers> = static_cell::StaticCell::new();
    let radio = radio_stack_metadata();
    native_radio_log!(
        "squidscript native x4 radio_probe stack={} version={} features={:?}",
        radio.stack,
        radio.version,
        radio.features
    );
    native_radio_log!("radio_probe_stage allocator_init");
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);
    #[cfg(feature = "vm-radio-measure")]
    print_vm_static_measurement("allocator_ready", runtime, buffers, radio_leases);
    #[cfg(feature = "vm-radio-measure")]
    print_combined_heap_measurement("allocator_ready", radio_leases);

    let peripherals = esp_hal::init(esp_hal::Config::default());
    #[cfg(any(feature = "wifi", feature = "ble"))]
    {
        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let software_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);
    }
    native_radio_log!("radio_probe_stage rtos_ready");

    #[cfg(feature = "vm-radio-measure")]
    run_combined_vm_radio_measurement(runtime, buffers, radio_leases);

    #[cfg(all(feature = "native-radio-services", not(feature = "vm-radio-measure")))]
    {
        #[cfg(not(feature = "x4-binbook"))]
        let runtime = RUNTIME.init_with(|| {
            SharedX4NativeRuntime::new(NativeRuntime::with_radio_backend(EspRadioBackend::new()))
        });
        #[cfg(all(feature = "wifi", not(feature = "x4-binbook")))]
        WIFI_RUNTIME_PTR.store(runtime as *const _ as usize, Ordering::Release);
        let buffers = BUFFERS.init_with(SerialProtocolBuffers::new);
        native_radio_log!("native_radio_services_stage serial_ready");
        #[cfg(feature = "wifi")]
        {
            static STA_NET_RESOURCES: static_cell::StaticCell<embassy_net::StackResources<4>> =
                static_cell::StaticCell::new();
            static AP_NET_RESOURCES: static_cell::StaticCell<embassy_net::StackResources<4>> =
                static_cell::StaticCell::new();
            let sta_interface = esp_radio::wifi::Interface::station();
            let (sta_stack, sta_runner) = embassy_net::new(
                sta_interface,
                embassy_net::Config::dhcpv4(Default::default()),
                STA_NET_RESOURCES.init_with(embassy_net::StackResources::<4>::new),
                0x5c17_5c4d_0000_0002,
            );
            match native_wifi_sta_stack_task(sta_runner) {
                Ok(task) => spawner.spawn(task),
                Err(_) => native_radio_log!("native_radio_services_error stage=sta_stack_spawn"),
            }
            match native_wifi_sta_ip_task(sta_stack) {
                Ok(task) => spawner.spawn(task),
                Err(_) => native_radio_log!("native_radio_services_error stage=sta_ip_spawn"),
            }
            let ap_interface = esp_radio::wifi::Interface::access_point();
            let ap_config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
                address: embassy_net::Ipv4Cidr::new(
                    embassy_net::Ipv4Address::new(192, 168, 4, 1),
                    24,
                ),
                gateway: None,
                dns_servers: Default::default(),
            });
            let (ap_stack, ap_runner) = embassy_net::new(
                ap_interface,
                ap_config,
                AP_NET_RESOURCES.init_with(embassy_net::StackResources::<4>::new),
                0x5c17_5c4d_0000_0001,
            );
            match native_wifi_ap_stack_task(ap_runner) {
                Ok(task) => spawner.spawn(task),
                Err(_) => native_radio_log!("native_radio_services_error stage=ap_stack_spawn"),
            }
            match native_wifi_ap_dhcp_task(ap_stack) {
                Ok(task) => spawner.spawn(task),
                Err(_) => native_radio_log!("native_radio_services_error stage=ap_dhcp_spawn"),
            }
            match native_http_upload_task(ap_stack) {
                Ok(task) => spawner.spawn(task),
                Err(_) => native_radio_log!("native_radio_services_error stage=http_upload_spawn"),
            }
        }
        #[cfg(feature = "ble")]
        match native_ble_file_transfer_task(peripherals.BT) {
            Ok(task) => spawner.spawn(task),
            Err(_) => native_radio_log!("native_radio_services_error stage=ble_task_spawn"),
        }
        #[cfg(feature = "x4-binbook")]
        {
            static SHARED_SPI: static_cell::StaticCell<SharedSpi2> = static_cell::StaticCell::new();
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
            let flash = esp_storage::FlashStorage::new(peripherals.FLASH);
            static FLASH_STORAGE: static_cell::StaticCell<core::cell::RefCell<X4LittleFsStorage>> =
                static_cell::StaticCell::new();
            let flash_storage = FLASH_STORAGE.init_with(|| {
                core::cell::RefCell::new(
                    squidscript_fw_x4::flash_partition::LittleFsAppStorage::new(flash),
                )
            });
            let app_store_ready = flash_storage.borrow_mut().initialize().is_ok();
            let mut app_storage =
                squidscript_fw_x4::flash_partition::SharedLittleFsStorage::new(flash_storage);
            static OTA_RUNTIME: static_cell::StaticCell<core::cell::RefCell<X4OtaRuntime>> =
                static_cell::StaticCell::new();
            let mut checkpoint = [0u8; squidscript_fw_x4::ota::OTA_CHECKPOINT_BYTES];
            let controller = match app_storage
                .load_ota_checkpoint(&mut checkpoint)
                .ok()
                .flatten()
                .and_then(|len| {
                    squidscript_fw_x4::ota::TransferState::decode_checkpoint(&checkpoint[..len])
                        .ok()
                }) {
                Some(state) => squidscript_fw_x4::ota::OtaController::restore(state),
                None => {
                    let mut cleanup = app_storage;
                    let _ = cleanup.delete_ota_checkpoint();
                    squidscript_fw_x4::ota::OtaController::default()
                }
            };
            let ota_runtime = OTA_RUNTIME.init_with(|| {
                core::cell::RefCell::new(X4OtaRuntime {
                    storage: app_storage,
                    controller,
                    partition_table: [0;
                        esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN],
                    candidate_base: 0,
                    candidate_size: 0,
                })
            });
            OTA_RUNTIME_PTR.store(ota_runtime as *const _ as usize, Ordering::Release);
            let internal_storage = app_storage;
            let content_storage = X4ContentStorage::new(sd_storage, internal_storage);
            let file_backend =
                X4BinBookFileBackend::<_, 512, 8, 128, 1024, 128, 4>::new(content_storage);
            let runtime = build_shared_x4_runtime(
                &RUNTIME,
                &RUNTIME_VALUE,
                file_backend,
                app_storage,
                app_store_ready,
            );
            match native_input_task(
                peripherals.ADC1,
                peripherals.GPIO1,
                peripherals.GPIO2,
                peripherals.GPIO3,
                peripherals.LPWR,
                runtime,
            ) {
                Ok(task) => {
                    spawner.spawn(task);
                    native_radio_log!("native_input_stage task_ready");
                }
                Err(_) => native_radio_log!("native_input_error stage=task_spawn"),
            }
            #[cfg(feature = "wifi")]
            WIFI_RUNTIME_PTR.store(runtime as *const _ as usize, Ordering::Release);
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
                Ok(()) => native_radio_log!("display_flush_stage panel_ready"),
                Err(_) => {
                    native_radio_log!("display_flush_error stage=panel_init error=controller")
                }
            }
            static DISPLAY_FLUSH: static_cell::StaticCell<
                StreamingDisplayFlushTask<X4DisplayPanel>,
            > = static_cell::StaticCell::new();
            let display_flush = DISPLAY_FLUSH.init_with(|| StreamingDisplayFlushTask::new(panel));
            #[cfg(feature = "ble")]
            match native_ble_storage_task(runtime) {
                Ok(task) => spawner.spawn(task),
                Err(_) => native_radio_log!("native_radio_services_error stage=ble_storage_spawn"),
            }
            #[cfg(feature = "wifi")]
            match native_wifi_event_task(runtime) {
                Ok(task) => spawner.spawn(task),
                Err(_) => {
                    native_radio_log!("native_radio_services_error stage=wifi_event_task_spawn")
                }
            }
            #[cfg(feature = "wifi")]
            match native_wifi_command_task(runtime) {
                Ok(task) => spawner.spawn(task),
                Err(_) => {
                    native_radio_log!("native_radio_services_error stage=wifi_command_task_spawn")
                }
            }
            run_serial_protocol_cooperative(
                UsbSerialJtag::new(peripherals.USB_DEVICE),
                runtime,
                buffers,
                display_flush,
            )
            .await;
        }
        #[cfg(not(feature = "x4-binbook"))]
        {
            static DISPLAY_FLUSH: static_cell::StaticCell<NoDisplayFlushTask> =
                static_cell::StaticCell::new();
            let mut display_flush = DISPLAY_FLUSH.init_with(|| NoDisplayFlushTask);
            #[cfg(feature = "ble")]
            match native_ble_storage_task(runtime) {
                Ok(task) => spawner.spawn(task),
                Err(_) => native_radio_log!("native_radio_services_error stage=ble_storage_spawn"),
            }
            #[cfg(feature = "wifi")]
            match native_wifi_event_task(runtime) {
                Ok(task) => spawner.spawn(task),
                Err(_) => {
                    native_radio_log!("native_radio_services_error stage=wifi_event_task_spawn")
                }
            }
            #[cfg(feature = "wifi")]
            match native_wifi_command_task(runtime) {
                Ok(task) => spawner.spawn(task),
                Err(_) => {
                    native_radio_log!("native_radio_services_error stage=wifi_command_task_spawn")
                }
            }
            run_serial_protocol_cooperative(
                UsbSerialJtag::new(peripherals.USB_DEVICE),
                runtime,
                buffers,
                &mut display_flush,
            )
            .await;
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

    #[cfg(not(feature = "vm-radio-measure"))]
    loop {
        embassy_time::Timer::after_secs(3600).await;
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
        feature = "native-radio-services"
    )
))]
struct CooperativeContentCheck {
    name: [u8; 128],
    name_len: usize,
    path: [u8; 128],
    path_len: usize,
    size: u64,
    offset: u64,
    crc32: SerialCrc32,
    read_buf: [u8; 512],
    status: CooperativeContentCheckStatus,
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
#[derive(Clone, Copy, Eq, PartialEq)]
enum CooperativeContentCheckStatus {
    Idle,
    NeedSize,
    Reading,
    Complete,
    Failed(&'static str),
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
#[derive(Clone, Copy)]
struct SerialCrc32 {
    value: u32,
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
impl SerialCrc32 {
    const fn new() -> Self {
        Self { value: 0xffff_ffff }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let mut value = self.value ^ u32::from(*byte);
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(value & 1);
                value = (value >> 1) ^ (0xedb8_8320 & mask);
            }
            self.value = value;
        }
    }

    const fn finish(self) -> u32 {
        !self.value
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
impl CooperativeContentCheck {
    const fn new() -> Self {
        Self {
            name: [0; 128],
            name_len: 0,
            path: [0; 128],
            path_len: 0,
            size: 0,
            offset: 0,
            crc32: SerialCrc32::new(),
            read_buf: [0; 512],
            status: CooperativeContentCheckStatus::Idle,
        }
    }

    fn clear(&mut self) {
        self.name_len = 0;
        self.path_len = 0;
        self.size = 0;
        self.offset = 0;
        self.crc32 = SerialCrc32::new();
        self.status = CooperativeContentCheckStatus::Idle;
    }

    fn start(&mut self, name: &str) -> Result<(), &'static str> {
        if name.is_empty()
            || name.starts_with('.')
            || name.contains('/')
            || name.contains('\\')
            || name.contains(':')
        {
            return Err("invalid-name");
        }
        let path_len = 6usize.checked_add(name.len()).ok_or("too-large")?;
        if name.len() > self.name.len() || path_len > self.path.len() {
            return Err("too-large");
        }
        self.name[..name.len()].copy_from_slice(name.as_bytes());
        self.name_len = name.len();
        self.path[..6].copy_from_slice(b"books/");
        self.path[6..path_len].copy_from_slice(name.as_bytes());
        self.path_len = path_len;
        self.size = 0;
        self.offset = 0;
        self.crc32 = SerialCrc32::new();
        self.status = CooperativeContentCheckStatus::NeedSize;
        Ok(())
    }

    fn is_for(&self, name: &str) -> bool {
        self.name() == Some(name)
    }

    fn name(&self) -> Option<&str> {
        core::str::from_utf8(&self.name[..self.name_len]).ok()
    }

    fn path(&self) -> Option<&str> {
        core::str::from_utf8(&self.path[..self.path_len]).ok()
    }

    fn step<B, D, C, FB, AS>(&mut self, runtime: &mut NativeRuntime<B, D, C, FB, AS>)
    where
        B: NativeRadioBackend,
        D: NativeDisplaySink,
        C: NativeBinBookBackend,
        FB: NativeFileBackend,
        AS: NativeAppStorage,
    {
        match self.status {
            CooperativeContentCheckStatus::Idle
            | CooperativeContentCheckStatus::Complete
            | CooperativeContentCheckStatus::Failed(_) => {}
            CooperativeContentCheckStatus::NeedSize => {
                #[cfg(debug_assertions)]
                runtime.record_trace("diag.content-check.size-start");
                let Some(path) = self.path() else {
                    self.status = CooperativeContentCheckStatus::Failed("invalid-name");
                    return;
                };
                match runtime.file_ref_size(path) {
                    Ok(size) => {
                        #[cfg(debug_assertions)]
                        runtime.record_trace("diag.content-check.size-ok");
                        self.size = size;
                        self.offset = 0;
                        self.crc32 = SerialCrc32::new();
                        self.status = CooperativeContentCheckStatus::Reading;
                    }
                    Err(error) => self.status = CooperativeContentCheckStatus::Failed(error),
                }
            }
            CooperativeContentCheckStatus::Reading => {
                if self.offset >= self.size {
                    self.status = CooperativeContentCheckStatus::Complete;
                    return;
                }
                let remaining = usize::try_from(self.size - self.offset).unwrap_or(usize::MAX);
                let read_len = remaining.min(self.read_buf.len());
                #[cfg(debug_assertions)]
                {
                    let label = match self.offset {
                        0 => "diag.content-check.read-0",
                        1..=512 => "diag.content-check.read-512",
                        513..=1024 => "diag.content-check.read-1024",
                        _ => "diag.content-check.read-more",
                    };
                    runtime.record_trace(label);
                }
                let mut path_buf = [0u8; 128];
                path_buf[..self.path_len].copy_from_slice(&self.path[..self.path_len]);
                let Ok(path) = core::str::from_utf8(&path_buf[..self.path_len]) else {
                    self.status = CooperativeContentCheckStatus::Failed("invalid-name");
                    return;
                };
                match runtime.file_ref_read_at(path, self.offset, &mut self.read_buf[..read_len]) {
                    Ok(()) => {
                        #[cfg(debug_assertions)]
                        runtime.record_trace("diag.content-check.read-ok");
                        self.crc32.update(&self.read_buf[..read_len]);
                        self.offset += read_len as u64;
                    }
                    Err(error) => self.status = CooperativeContentCheckStatus::Failed(error),
                }
            }
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
impl<D: NativeDisplaySink, FB: NativeFileBackend> NativeDisplayFlushDriver<D, FB>
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
impl<SPI, DC, RST, BUSY> NativeDisplayFlushDriver<X4CommandDisplaySink, X4NativeFileBackend>
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
                native_radio_log!("display_flush_stage complete");
            }
            Err(_) => {
                native_radio_log!("display_flush_error stage=step error=controller");
                self.task = X4StreamingDisplayFlushTask::new(CHUNK_COUNT);
            }
        }
    }

    fn is_idle(&self) -> bool {
        !self.task.is_active()
    }
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
fn native_radio_resource_metrics<B, D, C, FB, AS, F>(
) -> [squid_device_protocol::ResourceMetric<'static>; 8] {
    let stats = esp_alloc::HEAP.stats();
    let heap_free_bytes = stats.size.saturating_sub(stats.current_usage);
    let runtime_static_bytes = core::mem::size_of::<NativeRuntime<B, D, C, FB, AS>>();
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
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
fn with_x4_ota<R>(operation: impl FnOnce(&mut X4OtaRuntime) -> R) -> Result<R, &'static str> {
    let pointer = OTA_RUNTIME_PTR.load(Ordering::Acquire);
    if pointer == 0 {
        return Err("ota-unavailable");
    }
    let cell = unsafe { &*(pointer as *const core::cell::RefCell<X4OtaRuntime>) };
    Ok(operation(&mut cell.borrow_mut()))
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
fn current_build_id() -> heapless::String<32> {
    use core::fmt::Write as _;
    let mut id = heapless::String::new();
    for byte in &ESP_APP_DESC.app_elf_sha256()[..8] {
        let _ = write!(id, "{byte:02x}");
    }
    id
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
fn persist_ota_state(runtime: &mut X4OtaRuntime) -> Result<(), &'static str> {
    let mut checkpoint = [0u8; squidscript_fw_x4::ota::OTA_CHECKPOINT_BYTES];
    let len = runtime
        .controller
        .transfer_state()
        .encode_checkpoint(&mut checkpoint)
        .map_err(|_| "ota-checkpoint-encode")?;
    runtime
        .storage
        .save_ota_checkpoint_atomic(&checkpoint[..len])
        .map_err(|_| "ota-checkpoint-write")
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
fn ensure_ota_candidate_geometry(runtime: &mut X4OtaRuntime) -> Result<(), &'static str> {
    if runtime.candidate_size != 0 {
        return Ok(());
    }
    let candidate = runtime.controller.transfer_state().candidate();
    let X4OtaRuntime {
        storage,
        partition_table,
        candidate_base,
        candidate_size,
        ..
    } = runtime;
    let (base, size) = storage
        .with_raw_flash_mut(|flash| {
            squidscript_fw_x4::ota::EspOtaSlotStorage::new(flash, partition_table)
                .inactive_geometry(candidate)
        })
        .map_err(|_| "ota-geometry")?;
    *candidate_base = base;
    *candidate_size = size;
    Ok(())
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "x4-binbook"
))]
fn encode_x4_ota_request(
    parsed: &squid_device_protocol::DeviceRequest<'_>,
    response: &mut [u8],
) -> Result<usize, squid_device_protocol::DecodeError> {
    use squid_device_protocol::{
        encode_empty_response_into, encode_error_response_into, encode_firmware_info_response_into,
        encode_firmware_update_status_response_into, request_bytes_field, request_string_field,
        request_u64_field, FirmwareInfoRef, FirmwareUpdateStatusRef, Opcode, Status,
    };
    use squidscript_fw_x4::ota::{CooperativeStatus, EspOtaSlotStorage, OtaError, Slot};

    let error_response = |message: &'static str, response: &mut [u8]| {
        encode_error_response_into(parsed.opcode, parsed.sequence, -1, message, response)
    };
    match parsed.opcode {
        Opcode::FirmwareInfo => {
            #[cfg(debug_assertions)]
            esp_println::println!("native_ota_stage info-start");
            let result = with_x4_ota(|runtime| {
                let X4OtaRuntime {
                    storage,
                    controller,
                    partition_table,
                    ..
                } = runtime;
                storage.with_raw_flash_mut(|flash| {
                    let mut storage = EspOtaSlotStorage::new(flash, partition_table);
                    #[cfg(debug_assertions)]
                    esp_println::println!("native_ota_stage info-active");
                    let active = storage.active_slot()?;
                    #[cfg(debug_assertions)]
                    esp_println::println!("native_ota_stage info-inactive");
                    let inactive = storage.inactive_slot()?;
                    let boot_state = if matches!(
                        controller.transfer_state().phase(),
                        squidscript_fw_x4::ota::TransferPhase::Ready
                            | squidscript_fw_x4::ota::TransferPhase::Committed
                    ) {
                        #[cfg(debug_assertions)]
                        esp_println::println!("native_ota_stage info-state");
                        storage.boot_state()?
                    } else {
                        "undefined"
                    };
                    Ok::<_, OtaError>((active, inactive, boot_state))
                })
            });
            #[cfg(debug_assertions)]
            esp_println::println!("native_ota_stage info-done");
            match result {
                Ok(Ok((active, inactive, boot_state))) => {
                    let build_id = current_build_id();
                    encode_firmware_info_response_into(
                        parsed.sequence,
                        FirmwareInfoRef {
                            active_slot: active.name(),
                            active_slot_size: squidscript_fw_x4::ota::OTA_SLOT_BYTES as u64,
                            inactive_slot: inactive.name(),
                            inactive_slot_size: squidscript_fw_x4::ota::OTA_SLOT_BYTES as u64,
                            build_id: build_id.as_str(),
                            boot_state,
                        },
                        response,
                    )
                }
                _ => error_response("ota-info", response),
            }
        }
        Opcode::FirmwareUpdateBegin => {
            let total_len = request_u64_field(parsed, 1)
                .ok()
                .flatten()
                .and_then(|value| usize::try_from(value).ok());
            let hash = request_bytes_field(parsed, 2).ok().flatten();
            let build_id = request_string_field(parsed, 3).ok().flatten();
            let Some((total_len, hash, build_id)) = total_len
                .zip(hash)
                .zip(build_id)
                .map(|((total_len, hash), build_id)| (total_len, hash, build_id))
            else {
                return error_response("invalid-request", response);
            };
            let result = with_x4_ota(|runtime| {
                let X4OtaRuntime {
                    storage,
                    controller,
                    partition_table,
                    ..
                } = runtime;
                let slots = storage
                    .with_raw_flash_mut(|flash| {
                        let mut storage = EspOtaSlotStorage::new(flash, partition_table);
                        Ok::<_, OtaError>((storage.active_slot()?, storage.inactive_slot()?))
                    })
                    .map_err(|_| "ota-slots")?;
                controller
                    .begin(slots.0, slots.1, total_len, hash, build_id)
                    .map_err(|_| "ota-begin")?;
                runtime.candidate_size = 0;
                ensure_ota_candidate_geometry(runtime)
            });
            match result {
                Ok(Ok(())) => {
                    encode_empty_response_into(parsed.opcode, Status::Ok, parsed.sequence, response)
                }
                _ => error_response("ota-begin", response),
            }
        }
        Opcode::FirmwareUpdateStatus => {
            let result = with_x4_ota(|runtime| {
                if matches!(
                    runtime.controller.status(),
                    CooperativeStatus::Erasing { .. }
                ) {
                    ensure_ota_candidate_geometry(runtime)?;
                    let X4OtaRuntime {
                        storage,
                        controller,
                        partition_table,
                        candidate_base,
                        candidate_size,
                    } = runtime;
                    storage
                        .with_raw_flash_mut(|flash| {
                            let mut flash = squidscript_fw_x4::ota::CachedEspOtaSlotStorage::new(
                                flash,
                                partition_table,
                                controller.transfer_state().candidate(),
                                *candidate_base,
                                *candidate_size,
                            );
                            controller.erase_step(&mut flash, 4096)
                        })
                        .map_err(|_| "ota-erase")?;
                    if !matches!(
                        runtime.controller.status(),
                        CooperativeStatus::Erasing { .. }
                    ) {
                        persist_ota_state(runtime)?;
                    }
                }
                Ok::<_, &'static str>(runtime.controller.status())
            });
            let Ok(Ok(status)) = result else {
                return error_response("ota-status", response);
            };
            let (state, progress) = match status {
                CooperativeStatus::Idle => ("idle", 0),
                CooperativeStatus::Erasing { erased, .. } => ("erasing", erased),
                CooperativeStatus::Receiving { durable, .. } => ("receiving", durable),
                CooperativeStatus::Verifying { verified, .. } => ("verifying", verified),
                CooperativeStatus::ReadyToActivate => ("ready", 0),
                CooperativeStatus::Committed => ("committed", 0),
                CooperativeStatus::Aborted => ("aborted", 0),
                CooperativeStatus::Failed => ("failed", 0),
            };
            let result = with_x4_ota(|runtime| {
                let transfer = runtime.controller.transfer_state();
                let candidate = if status == CooperativeStatus::Idle {
                    let X4OtaRuntime {
                        storage,
                        partition_table,
                        ..
                    } = runtime;
                    storage
                        .with_raw_flash_mut(|flash| {
                            EspOtaSlotStorage::new(flash, partition_table).inactive_slot()
                        })
                        .unwrap_or(Slot::App1)
                } else {
                    transfer.candidate()
                };
                let mut build_id = heapless::String::<32>::new();
                let _ = build_id.push_str(transfer.build_id());
                (
                    candidate,
                    transfer.expected_len(),
                    build_id,
                    *transfer.expected_sha256(),
                )
            });
            let Ok((candidate, expected_len, build_id, hash)) = result else {
                return error_response("ota-status", response);
            };
            encode_firmware_update_status_response_into(
                parsed.sequence,
                if matches!(
                    status,
                    CooperativeStatus::Committed | CooperativeStatus::Aborted
                ) {
                    Status::Ok
                } else {
                    Status::Pending
                },
                FirmwareUpdateStatusRef {
                    state,
                    candidate_slot: candidate.name(),
                    expected_len: expected_len as u64,
                    durable_offset: progress as u64,
                    build_id: build_id.as_str(),
                    expected_sha256: &hash,
                },
                response,
            )
        }
        Opcode::FirmwareUpdateChunk => {
            let offset = request_u64_field(parsed, 1)
                .ok()
                .flatten()
                .and_then(|value| usize::try_from(value).ok());
            let bytes = request_bytes_field(parsed, 2).ok().flatten();
            let Some((offset, bytes)) = offset.zip(bytes) else {
                return error_response("invalid-request", response);
            };
            let result = with_x4_ota(|runtime| {
                ensure_ota_candidate_geometry(runtime)?;
                let X4OtaRuntime {
                    storage,
                    controller,
                    partition_table,
                    candidate_base,
                    candidate_size,
                } = runtime;
                storage
                    .with_raw_flash_mut(|flash| {
                        let mut flash = squidscript_fw_x4::ota::CachedEspOtaSlotStorage::new(
                            flash,
                            partition_table,
                            controller.transfer_state().candidate(),
                            *candidate_base,
                            *candidate_size,
                        );
                        controller.write_chunk(&mut flash, offset, bytes)
                    })
                    .map_err(|_| "ota-write")?;
                let durable = runtime.controller.transfer_state().durable_offset();
                if offset / (64 * 1024) != durable / (64 * 1024)
                    || runtime.controller.transfer_state().phase()
                        == squidscript_fw_x4::ota::TransferPhase::Ready
                {
                    persist_ota_state(runtime)?;
                }
                Ok::<_, &'static str>(())
            });
            match result {
                Ok(Ok(())) => {
                    encode_empty_response_into(parsed.opcode, Status::Ok, parsed.sequence, response)
                }
                _ => error_response("ota-chunk", response),
            }
        }
        Opcode::FirmwareUpdateCommit => {
            let result = with_x4_ota(|runtime| {
                ensure_ota_candidate_geometry(runtime).map_err(|_| OtaError::Flash)?;
                let mut readback = [0u8; 4096];
                let X4OtaRuntime {
                    storage,
                    controller,
                    partition_table,
                    candidate_base,
                    candidate_size,
                } = runtime;
                let status = storage.with_raw_flash_mut(|flash| {
                    let mut flash = squidscript_fw_x4::ota::CachedEspOtaSlotStorage::new(
                        flash,
                        partition_table,
                        controller.transfer_state().candidate(),
                        *candidate_base,
                        *candidate_size,
                    );
                    controller.verify_step(&mut flash, &mut readback)
                })?;
                if status == CooperativeStatus::ReadyToActivate {
                    storage.with_raw_flash_mut(|flash| {
                        let mut flash = squidscript_fw_x4::ota::CachedEspOtaSlotStorage::new(
                            flash,
                            partition_table,
                            controller.transfer_state().candidate(),
                            *candidate_base,
                            *candidate_size,
                        );
                        controller.activate(&mut flash)
                    })?;
                    persist_ota_state(runtime).map_err(|_| OtaError::Checkpoint)?;
                    OTA_REBOOT_PENDING.store(true, Ordering::Release);
                }
                Ok::<_, OtaError>(status)
            });
            match result {
                Ok(Ok(CooperativeStatus::ReadyToActivate)) => {
                    encode_empty_response_into(parsed.opcode, Status::Ok, parsed.sequence, response)
                }
                Ok(Ok(_)) => encode_empty_response_into(
                    parsed.opcode,
                    Status::Pending,
                    parsed.sequence,
                    response,
                ),
                _ => error_response("ota-verify", response),
            }
        }
        Opcode::FirmwareUpdateAbort => {
            let result = with_x4_ota(|runtime| {
                runtime.controller.abort();
                runtime
                    .storage
                    .delete_ota_checkpoint()
                    .map_err(|_| "ota-abort")
            });
            match result {
                Ok(Ok(())) => {
                    encode_empty_response_into(parsed.opcode, Status::Ok, parsed.sequence, response)
                }
                _ => error_response("ota-abort", response),
            }
        }
        _ => error_response("invalid-request", response),
    }
}

#[cfg(all(
    target_arch = "riscv32",
    any(
        not(any(feature = "wifi", feature = "ble")),
        feature = "native-radio-services"
    )
))]
fn encode_serial_request<B, D, C, FB, AS, F>(
    runtime: &mut NativeRuntime<B, D, C, FB, AS>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    content_check: &mut CooperativeContentCheck,
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
    AS: NativeAppStorage,
    F: NativeDisplayFlushDriver<D, FB>,
{
    use squid_device_protocol::{
        encode_app_list_response_into, encode_content_check_response_into,
        encode_content_delete_response_into, encode_empty_response_into,
        encode_error_response_into, encode_hello_response_into, encode_line_response_into,
        encode_resources_response_into, encode_state_response_into, key_event_from_request_into,
        request_bytes_field, request_string_field, AppListEntry, Opcode, ResourceMetric, Status,
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
            content_check.clear();
            encode_empty_response_into(Opcode::Reset, Status::Ok, parsed.sequence, response)
        }
        Opcode::StorageFormat => match runtime.storage_format() {
            Ok(()) => {
                *sessions = squid_device_protocol::ProtocolSessions::default();
                content_check.clear();
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
        Opcode::ResourceInstallBegin
        | Opcode::ResourceInstallChunk
        | Opcode::ResourceInstallCommit => {
            handle_resource_install_request(runtime, sessions, parsed, response)
        }
        Opcode::ContentInstallBegin
        | Opcode::ContentInstallChunk
        | Opcode::ContentInstallCommit => {
            handle_content_install_request(runtime, sessions, parsed, response)
        }
        Opcode::ContentCheck => match request_string_field(parsed, 1).ok().flatten() {
            Some(name) => {
                #[cfg(debug_assertions)]
                runtime.record_trace("diag.content-check.start");
                match content_check.status {
                    CooperativeContentCheckStatus::Idle => match content_check.start(name) {
                        Ok(()) => encode_empty_response_into(
                            parsed.opcode,
                            Status::Pending,
                            parsed.sequence,
                            response,
                        ),
                        Err(error) => {
                            #[cfg(debug_assertions)]
                            runtime.record_trace("diag.content-check.error");
                            runtime.record_error(error);
                            encode_error_response_into(
                                parsed.opcode,
                                parsed.sequence,
                                -1,
                                error,
                                response,
                            )
                        }
                    },
                    CooperativeContentCheckStatus::NeedSize
                    | CooperativeContentCheckStatus::Reading => {
                        if content_check.is_for(name) {
                            encode_empty_response_into(
                                parsed.opcode,
                                Status::Pending,
                                parsed.sequence,
                                response,
                            )
                        } else {
                            encode_error_response_into(
                                parsed.opcode,
                                parsed.sequence,
                                -1,
                                "busy",
                                response,
                            )
                        }
                    }
                    CooperativeContentCheckStatus::Complete => {
                        if content_check.is_for(name) {
                            let checked_name = content_check
                                .name()
                                .ok_or(squid_device_protocol::DecodeError::InvalidUtf8)?;
                            let size = content_check.size;
                            let crc32 = content_check.crc32.finish();
                            let encoded = encode_content_check_response_into(
                                parsed.sequence,
                                checked_name,
                                size,
                                u64::from(crc32),
                                response,
                            );
                            content_check.clear();
                            encoded
                        } else {
                            encode_error_response_into(
                                parsed.opcode,
                                parsed.sequence,
                                -1,
                                "busy",
                                response,
                            )
                        }
                    }
                    CooperativeContentCheckStatus::Failed(error) => {
                        #[cfg(debug_assertions)]
                        runtime.record_trace("diag.content-check.error");
                        runtime.record_error(error);
                        content_check.clear();
                        encode_error_response_into(
                            parsed.opcode,
                            parsed.sequence,
                            -1,
                            error,
                            response,
                        )
                    }
                }
            }
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
        #[cfg(feature = "x4-binbook")]
        Opcode::FirmwareInfo
        | Opcode::FirmwareUpdateBegin
        | Opcode::FirmwareUpdateChunk
        | Opcode::FirmwareUpdateCommit
        | Opcode::FirmwareUpdateStatus
        | Opcode::FirmwareUpdateAbort => encode_x4_ota_request(parsed, response),
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
            }; squidvm_core::limits::MAX_INSTALLED_APPS];
            let mut len = 0;
            for entry in runtime.app_registry().iter().flatten() {
                entries[len] = AppListEntry {
                    app_id: entry.app_id(),
                    sqbc_len: entry.sqbc_bytes as u64,
                };
                len += 1;
            }
            encode_app_list_response_into(parsed.sequence, entries[..len].iter().copied(), response)
        }
        Opcode::Key => {
            match key_event_from_request_into(request_bytes, event_buf)
                .ok()
                .and_then(|len| core::str::from_utf8(&event_buf[..len]).ok())
                .ok_or(())
                .and_then(|event| runtime.enqueue_input_event(event).map_err(|_| ()))
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
        Opcode::ErrorsGet => encode_line_response_into(
            Opcode::ErrorsGet,
            parsed.sequence,
            runtime.error_lines().iter(),
            response,
        ),
        Opcode::StateGet => {
            encode_state_response_into(parsed.sequence, runtime.state_bytes(), response)
        }
        Opcode::ResourcesGet => {
            let metrics = runtime.resource_metrics();
            #[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
            let platform_metrics = native_radio_resource_metrics::<B, D, C, FB, AS, F>();
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
        Opcode::LifecycleGet => {
            use core::fmt::Write as _;
            let process_len = runtime.lifecycle_process_len();
            let armed_len = runtime.lifecycle_armed_len();
            let process = (0..process_len).filter_map(|index| runtime.lifecycle_process_at(index));
            let armed = (0..armed_len).filter_map(|index| {
                runtime
                    .lifecycle_armed_at(index)
                    .map(|(app_id, event)| squid_device_protocol::LifecycleTimer { app_id, event })
            });
            let mut phase = heapless::String::<64>::new();
            let mut reason = heapless::String::<64>::new();
            let mut queue = heapless::String::<64>::new();
            let _ = write!(phase, "lifecycle={}", runtime.lifecycle_phase());
            let _ = write!(reason, "start_reason={}", runtime.lifecycle_start_reason());
            let _ = write!(
                queue,
                "event_queue={} overflow={}",
                runtime.lifecycle_queue_len(),
                u8::from(runtime.lifecycle_queue_overflowed())
            );
            let details = [phase.as_str(), reason.as_str(), queue.as_str()];
            squid_device_protocol::encode_lifecycle_response_with_details_into(
                parsed.sequence,
                runtime.active_app(),
                process,
                armed,
                details.into_iter(),
                response,
            )
        }
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
fn process_serial_byte<B, D, C, FB, AS, F>(
    byte: u8,
    runtime: &mut NativeRuntime<B, D, C, FB, AS>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    content_check: &mut CooperativeContentCheck,
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
    AS: NativeAppStorage,
    F: NativeDisplayFlushDriver<D, FB>,
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

    let encoded_len =
        match squid_device_protocol::DeviceRequest::decode(&buffers.request[..frame_len]) {
            Ok(parsed) => match encode_serial_request(
                runtime,
                sessions,
                content_check,
                &parsed,
                &buffers.request[..frame_len],
                event_buf,
                display_flush,
                &mut buffers.response,
            ) {
                Ok(response_len) => Some(response_len),
                Err(_) => {
                    #[cfg(debug_assertions)]
                    runtime.record_trace("diag.serial.encode-error");
                    squid_device_protocol::encode_error_response_into(
                        parsed.opcode,
                        parsed.sequence,
                        -1,
                        "encode-error",
                        &mut buffers.response,
                    )
                    .ok()
                }
            },
            Err(_) => {
                #[cfg(debug_assertions)]
                runtime.record_trace("diag.serial.decode-error");
                None
            }
        };

    let remaining = *request_len - frame_len;
    buffers.request.copy_within(frame_len..*request_len, 0);
    *request_len = remaining;
    encoded_len
}

#[cfg(all(target_arch = "riscv32", not(any(feature = "wifi", feature = "ble"))))]
fn run_serial_protocol<B, D, C, FB, AS, F>(
    mut serial: UsbSerialJtag<'static, esp_hal::Blocking>,
    runtime: &'static mut NativeRuntime<B, D, C, FB, AS>,
    buffers: &'static mut SerialProtocolBuffers,
    display_flush: &mut F,
) -> !
where
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
    AS: NativeAppStorage,
    F: NativeDisplayFlushDriver<D, FB>,
{
    let mut request_len = 0usize;
    let mut sessions = squid_device_protocol::ProtocolSessions::default();
    let mut content_check = CooperativeContentCheck::new();
    let mut event_buf = [0u8; 64];

    loop {
        match serial.read_byte() {
            Ok(byte) => {
                if let Some(response_len) = process_serial_byte(
                    byte,
                    runtime,
                    &mut sessions,
                    &mut content_check,
                    &mut request_len,
                    &mut event_buf,
                    display_flush,
                    buffers,
                ) {
                    let _ = serial.write(&buffers.response[..response_len]);
                    let _ = serial.flush_tx();
                }
            }
            Err(_) => core::hint::spin_loop(),
        }
        content_check.step(runtime);
        {
            let (display_sink, file_backend) = runtime.display_sink_and_file_backend_mut();
            let _ = request_pending_display_flush(display_sink, file_backend, display_flush);
        }
        display_flush.step();
    }
}

#[cfg(all(target_arch = "riscv32", feature = "native-radio-services"))]
async fn run_serial_protocol_cooperative<B, D, C, FB, AS, F>(
    mut serial: UsbSerialJtag<'static, esp_hal::Blocking>,
    runtime: &'static embassy_sync_07::mutex::Mutex<
        embassy_sync_07::blocking_mutex::raw::CriticalSectionRawMutex,
        &'static mut NativeRuntime<B, D, C, FB, AS>,
    >,
    buffers: &'static mut SerialProtocolBuffers,
    display_flush: &mut F,
) where
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
    AS: NativeAppStorage,
    F: NativeDisplayFlushDriver<D, FB>,
{
    let mut request_len = 0usize;
    let mut sessions = squid_device_protocol::ProtocolSessions::default();
    let mut content_check = CooperativeContentCheck::new();
    let mut event_buf = [0u8; 64];
    let mut last_timer_tick = embassy_time::Instant::now();
    let mut pending_sleep = None;
    let mut checkpoint_bytes = [0_u8; squidscript_fw_core::power::POWER_CHECKPOINT_BYTES];
    #[cfg(all(feature = "ble", debug_assertions))]
    let mut reported_ble_stage = 0usize;
    #[cfg(all(feature = "ble", debug_assertions))]
    let mut reported_ble_flags = 0usize;
    #[cfg(all(feature = "ble", debug_assertions))]
    let mut reported_queue_high_water = 0usize;
    #[cfg(feature = "x4-binbook")]
    let health_pending = with_x4_ota(|ota| {
        matches!(
            ota.controller.transfer_state().phase(),
            squidscript_fw_x4::ota::TransferPhase::Ready
                | squidscript_fw_x4::ota::TransferPhase::Committed
        )
    })
    .unwrap_or(false);
    #[cfg(feature = "x4-binbook")]
    if health_pending {
        if option_env!("SQUIDSCRIPT_X4_FORCE_PREHEALTH_RESET") == Some("1") {
            native_radio_log!("native_ota_test stage=forced-prehealth-reset");
            esp_hal::system::software_reset();
        }
        let health_result = with_x4_ota(|ota| {
            let X4OtaRuntime {
                storage,
                partition_table,
                ..
            } = ota;
            storage.with_raw_flash_mut(|flash| {
                squidscript_fw_x4::ota::EspOtaSlotStorage::new(flash, partition_table)
                    .mark_running_valid()
            })?;
            storage
                .delete_ota_checkpoint()
                .map_err(|_| squidscript_fw_x4::ota::OtaError::Checkpoint)
        });
        if !matches!(health_result, Ok(Ok(()))) {
            native_radio_log!("native_ota_error stage=health-confirm");
        }
    }
    loop {
        let now = embassy_time::Instant::now();
        let elapsed_ms = (now
            .duration_since(last_timer_tick)
            .as_millis()
            .min(u32::MAX as u64)) as u32;
        last_timer_tick = now;
        let mut runtime = runtime.lock().await;
        let heap = esp_alloc::HEAP.stats();
        runtime.set_system_memory_metrics(
            TOTAL_SRAM_BYTES,
            heap.current_usage,
            heap.size.saturating_sub(heap.current_usage),
        );
        #[cfg(all(feature = "ble", debug_assertions))]
        {
            let stage = BLE_DIAGNOSTIC_STAGE.load(Ordering::Acquire);
            if should_report_ble_stage(reported_ble_stage, stage) {
                let label = match stage {
                    1 => "diag.ble.task",
                    2 => "diag.ble.profile",
                    3 => "diag.ble.stack",
                    4 => "diag.ble.advertising",
                    5 => "diag.ble.connected",
                    _ => "diag.ble.unknown-stage",
                };
                runtime.record_trace(label);
                reported_ble_stage = stage;
            }
            let flags = BLE_DIAGNOSTIC_FLAGS.load(Ordering::Acquire);
            let new_flags = flags & !reported_ble_flags;
            if new_flags & BLE_DIAGNOSTIC_RUNNER_EXIT != 0 {
                runtime.record_trace("diag.ble.runner-exit");
            }
            if new_flags & BLE_DIAGNOSTIC_GATT_ATTACH_FAILURE != 0 {
                runtime.record_trace("diag.ble.gatt-attach-failure");
            }
            if new_flags & BLE_DIAGNOSTIC_GATT_EVENT != 0 {
                runtime.record_trace("diag.ble.gatt-event");
            }
            if new_flags & BLE_DIAGNOSTIC_BACKPRESSURE != 0 {
                runtime.record_trace("diag.ble.backpressure");
            }
            if new_flags & BLE_DIAGNOSTIC_BEGIN_WRITE != 0 {
                runtime.record_trace("diag.ble.begin-write");
            }
            if new_flags & BLE_DIAGNOSTIC_NAME_WRITE != 0 {
                runtime.record_trace("diag.ble.name-write");
            }
            if new_flags & BLE_DIAGNOSTIC_NAME_ACCEPTED != 0 {
                runtime.record_trace("diag.ble.name-accepted");
            }
            if new_flags & BLE_DIAGNOSTIC_WRITE_REJECTED != 0 {
                runtime.record_trace("diag.ble.write-rejected");
            }
            if new_flags & BLE_DIAGNOSTIC_ACCEPT_FAILED != 0 {
                runtime.record_trace("diag.ble.accept-failed");
            }
            if new_flags & BLE_DIAGNOSTIC_NOTIFY_SENT != 0 {
                runtime.record_trace("diag.ble.notify-sent");
            }
            if new_flags & BLE_DIAGNOSTIC_NOTIFY_FAILED != 0 {
                runtime.record_trace("diag.ble.notify-failed");
            }
            if new_flags & BLE_DIAGNOSTIC_CONNECTION_WATCHDOG != 0 {
                runtime.record_trace("diag.ble.connection-watchdog");
            }
            if new_flags & BLE_DIAGNOSTIC_DISCONNECTED != 0 {
                runtime.record_trace("diag.ble.disconnected");
            }
            if new_flags & BLE_DIAGNOSTIC_CONNECTION_PARAMS_REQUEST != 0 {
                runtime.record_trace("diag.ble.connection-params-request");
            }
            if new_flags & BLE_DIAGNOSTIC_GATT_OTHER != 0 {
                runtime.record_trace("diag.ble.gatt-other");
            }
            if new_flags & BLE_DIAGNOSTIC_DATA_WRITE != 0 {
                runtime.record_trace("diag.ble.data-write");
            }
            if new_flags & BLE_DIAGNOSTIC_CONNECTION_PARAMS_ACCEPTED != 0 {
                runtime.record_trace("diag.ble.connection-params-accepted");
            }
            if new_flags & BLE_DIAGNOSTIC_CONNECTION_PARAMS_FAILED != 0 {
                runtime.record_trace("diag.ble.connection-params-failed");
            }
            if new_flags & BLE_DIAGNOSTIC_STATUS_CCCD_INDICATE != 0 {
                runtime.record_trace("diag.ble.status-cccd-indicate");
            }
            if new_flags & BLE_DIAGNOSTIC_STATUS_CCCD_NOTIFY != 0 {
                runtime.record_trace("diag.ble.status-cccd-notify");
            }
            if new_flags & BLE_DIAGNOSTIC_STATUS_CCCD_DISABLED != 0 {
                runtime.record_trace("diag.ble.status-cccd-disabled");
            }
            if new_flags & BLE_DIAGNOSTIC_STATUS_INDICATE_ENABLED != 0 {
                runtime.record_trace("diag.ble.status-indicate-enabled");
            }
            if new_flags & BLE_DIAGNOSTIC_STATUS_INDICATE_DISABLED != 0 {
                runtime.record_trace("diag.ble.status-indicate-disabled");
            }
            if new_flags & BLE_DIAGNOSTIC_STATUS_NOTIFY_ENABLED != 0 {
                runtime.record_trace("diag.ble.status-notify-enabled");
            }
            let gatt_other_count = BLE_DIAGNOSTIC_GATT_OTHER_COUNT.load(Ordering::Acquire);
            if gatt_other_count > 0 {
                let label = match gatt_other_count {
                    1 => "diag.ble.gatt-other-count-1",
                    2..=4 => "diag.ble.gatt-other-count-2-4",
                    5..=8 => "diag.ble.gatt-other-count-5-8",
                    _ => "diag.ble.gatt-other-count-9-plus",
                };
                runtime.record_trace(label);
                BLE_DIAGNOSTIC_GATT_OTHER_COUNT.store(0, Ordering::Release);
            }
            let disconnect_reason = BLE_DIAGNOSTIC_DISCONNECT_REASON.load(Ordering::Acquire);
            BLE_DIAGNOSTIC_DISCONNECT_REASON.store(0, Ordering::Release);
            if disconnect_reason != 0 {
                let label = match disconnect_reason {
                    0x08 => "diag.ble.disconnect-timeout",
                    0x13 => "diag.ble.disconnect-remote-user",
                    0x16 => "diag.ble.disconnect-local-host",
                    _ => "diag.ble.disconnect-other",
                };
                runtime.record_trace(label);
            }
            let error_stage = BLE_DIAGNOSTIC_ERROR_STAGE.load(Ordering::Acquire);
            BLE_DIAGNOSTIC_ERROR_STAGE.store(0, Ordering::Release);
            if error_stage != 0 {
                let label = match error_stage {
                    201..=207 => "diag.ble.control-error",
                    301 => "diag.ble.data-shape-error",
                    302 => "diag.ble.data-session-error",
                    401 => "diag.ble.storage-route-ambiguous",
                    402 => "diag.ble.storage-route-mismatch",
                    403 => "diag.ble.storage-route-too-large",
                    404 => "diag.ble.storage-begin-error",
                    405 => "diag.ble.storage-session-begin-error",
                    406 => "diag.ble.storage-chunk-state-error",
                    407 => "diag.ble.storage-write-error",
                    408 => "diag.ble.storage-commit-state-error",
                    409 => "diag.ble.storage-missing-route",
                    410 => "diag.ble.storage-complete",
                    411 => "diag.ble.storage-dispatch-error",
                    _ => "diag.ble.unknown-error-stage",
                };
                runtime.record_trace(label);
            }
            reported_ble_flags = flags;
            let high_water = BLE_DIAGNOSTIC_QUEUE_HIGH_WATER.load(Ordering::Acquire);
            if high_water > reported_queue_high_water {
                let label = match high_water {
                    1 => "diag.ble.queue-high-water-1",
                    2 => "diag.ble.queue-high-water-2",
                    3 => "diag.ble.queue-high-water-3",
                    _ => "diag.ble.queue-high-water-4",
                };
                runtime.record_trace(label);
                reported_queue_high_water = high_water;
            }
        }
        if elapsed_ms > 0 && runtime.wifi_operation_active_kind() != Some("connect") {
            let _ = runtime.tick_timers(elapsed_ms);
        }
        for _ in 0..256 {
            match serial.read_byte() {
                Ok(byte) => {
                    if let Some(response_len) = process_serial_byte(
                        byte,
                        &mut runtime,
                        &mut sessions,
                        &mut content_check,
                        &mut request_len,
                        &mut event_buf,
                        display_flush,
                        buffers,
                    ) {
                        let _ = serial.write(&buffers.response[..response_len]);
                        let _ = serial.flush_tx();
                        #[cfg(feature = "x4-binbook")]
                        if OTA_REBOOT_PENDING.load(Ordering::Acquire) {
                            OTA_REBOOT_PENDING.store(false, Ordering::Release);
                            esp_hal::system::software_reset();
                        }
                    }
                }
                Err(_) => break,
            }
        }
        content_check.step(&mut runtime);
        {
            let (display_sink, file_backend) = runtime.display_sink_and_file_backend_mut();
            if let Err(error) =
                request_pending_display_flush(display_sink, file_backend, display_flush)
            {
                #[cfg(debug_assertions)]
                runtime.record_error(error);
            }
        }
        display_flush.step();
        if pending_sleep.is_none() {
            match runtime.take_prepared_sleep_checkpoint() {
                Ok(checkpoint) => pending_sleep = checkpoint,
                Err(_) => runtime.record_error("power_sleep_checkpoint_build_failed"),
            }
        }
        if let Some(checkpoint) = pending_sleep.as_ref() {
            if runtime.display_sink().pending_refreshes() == 0 && display_flush.is_idle() {
                let prepared = runtime.prepare_hardware_sleep().and_then(|()| {
                    runtime.save_power_checkpoint(checkpoint, &mut checkpoint_bytes)
                });
                match prepared {
                    Ok(()) => {
                        POWER_SLEEP_WAKE_AFTER_MS
                            .store(checkpoint.wake_after_ms, Ordering::Release);
                        POWER_SLEEP_READY.store(true, Ordering::Release);
                    }
                    Err(_) => {
                        let _ = runtime.delete_power_checkpoint();
                        runtime.record_error("power_sleep_prepare_failed");
                    }
                }
                pending_sleep = None;
            }
        }
        drop(runtime);
        embassy_time::Timer::after_millis(1).await;
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
async fn run_ble_storage_task(runtime: &'static SharedX4NativeRuntime) {
    use embassy_futures::select::{select, Either};

    let mut session = BleStorageSession::new();
    let mut upload_path = heapless::String::<128>::new();
    loop {
        let command = match select(
            BLE_STORAGE_CANCELS.receive(),
            BLE_STORAGE_COMMANDS.receive(),
        )
        .await
        {
            Either::First(cancelled_id) => {
                if session.cancel(cancelled_id) {
                    if !upload_path.is_empty() {
                        let mut runtime = runtime.lock().await;
                        let _ = runtime.abort_ephemeral_upload(upload_path.as_str());
                    }
                    upload_path.clear();
                }
                continue;
            }
            Either::Second(command) => command,
        };

        let command_id = match &command {
            BleStorageCommand::Begin { session_id, .. }
            | BleStorageCommand::Chunk { session_id, .. }
            | BleStorageCommand::Commit { session_id } => *session_id,
        };
        if BLE_CANCELLED_SESSION.load(Ordering::Acquire) == command_id.get() {
            if session.cancel(command_id) {
                if !upload_path.is_empty() {
                    let mut runtime = runtime.lock().await;
                    let _ = runtime.abort_ephemeral_upload(upload_path.as_str());
                }
                upload_path.clear();
            }
            continue;
        }

        match command {
            BleStorageCommand::Begin { session_id, route } => {
                let mut runtime = runtime.lock().await;
                let resolved =
                    runtime.resolve_upload_route(route.name.as_str(), NativeUploadTransport::Ble);
                let active_route = match resolved {
                    Ok(active_route) => active_route,
                    Err(NativeUploadRouteError::RouteAmbiguous) => {
                        runtime.record_error("upload-route-ambiguous");
                        ble_report_status(401, BLE_STATUS_ROUTE_AMBIGUOUS);
                        continue;
                    }
                    Err(_) => {
                        runtime.record_error("upload-route-mismatch");
                        ble_report_status(402, BLE_STATUS_ERROR);
                        continue;
                    }
                };
                let Ok(active_route) = BleUploadRoute::new(
                    route.name.as_str(),
                    active_route.profile_id.as_str(),
                    active_route.complete_event.as_str(),
                    route.total_len,
                ) else {
                    runtime.record_error("upload-route-too-large");
                    ble_report_status(403, BLE_STATUS_ERROR);
                    continue;
                };
                let staged_path = match runtime.begin_ephemeral_upload(
                    active_route.name.as_str(),
                    active_route.total_len,
                    active_route.profile_id.as_str(),
                    NativeUploadTransport::Ble,
                ) {
                    Ok(path) => path,
                    Err(_) => {
                        runtime.record_error("ble-stage-begin");
                        ble_report_status(404, BLE_STATUS_ERROR);
                        continue;
                    }
                };
                upload_path.clear();
                let path_copied = upload_path.push_str(staged_path).is_ok();
                if !path_copied || session.begin(session_id, active_route).is_err() {
                    runtime.record_error("ble-stage-session-begin");
                    runtime.abort_active_ephemeral_upload();
                    upload_path.clear();
                    session.clear();
                    ble_report_status(405, BLE_STATUS_ERROR);
                    continue;
                }
                drop(runtime);
            }
            BleStorageCommand::Chunk {
                session_id,
                offset,
                len,
                bytes,
            } => {
                if len > bytes.len()
                    || session.accept_chunk(session_id, offset, len).is_err()
                    || upload_path.is_empty()
                {
                    let mut runtime = runtime.lock().await;
                    runtime.record_error("ble-stage-chunk-state");
                    drop(runtime);
                    ble_report_status(406, BLE_STATUS_ERROR);
                    BLE_CANCELLED_SESSION.store(session_id.get(), Ordering::Release);
                    let _ = BLE_STORAGE_CANCELS.try_send(session_id);
                    continue;
                }
                let mut runtime = runtime.lock().await;
                if runtime
                    .write_ephemeral_upload_chunk(upload_path.as_str(), offset, &bytes[..len])
                    .is_err()
                {
                    runtime.record_error("ble-stage-write");
                    let _ = runtime.abort_ephemeral_upload(upload_path.as_str());
                    upload_path.clear();
                    session.clear();
                    ble_report_status(407, BLE_STATUS_ERROR);
                }
            }
            BleStorageCommand::Commit { session_id } => {
                if session.commit(session_id).is_err() || upload_path.is_empty() {
                    let mut runtime = runtime.lock().await;
                    runtime.record_error("ble-stage-commit-state");
                    drop(runtime);
                    ble_report_status(408, BLE_STATUS_ERROR);
                    BLE_CANCELLED_SESSION.store(session_id.get(), Ordering::Release);
                    let _ = BLE_STORAGE_CANCELS.try_send(session_id);
                    continue;
                }
                let event = match session.route() {
                    Some(route) => route.complete_event.clone(),
                    None => {
                        let mut runtime = runtime.lock().await;
                        runtime.record_error("ble-stage-missing-route");
                        drop(runtime);
                        ble_report_status(409, BLE_STATUS_ERROR);
                        continue;
                    }
                };
                let received = session.received();
                let mut runtime = runtime.lock().await;
                let completed = runtime
                    .commit_ephemeral_upload(upload_path.as_str(), received)
                    .and_then(|()| {
                        runtime
                            .dispatch_active_upload_complete(event.as_str(), upload_path.as_str())
                    });
                if completed.is_ok() {
                    ble_report_status(410, BLE_STATUS_COMPLETE);
                } else {
                    runtime.record_error("ble-stage-dispatch");
                    let _ = runtime.abort_ephemeral_upload(upload_path.as_str());
                    ble_report_status(411, BLE_STATUS_ERROR);
                }
                upload_path.clear();
                session.clear();
            }
        }
        embassy_futures::yield_now().await;
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
async fn run_ble_file_transfer_task(bt: esp_hal::peripherals::BT<'static>) {
    use embassy_futures::select::{select, Either};
    use embassy_sync_07::blocking_mutex::raw::NoopRawMutex;
    use trouble_host::prelude::*;

    const CONNECTIONS_MAX: usize = 1;
    const L2CAP_CHANNELS_MAX: usize = 3;
    const ATTRIBUTE_MAX: usize = BLE_GATT_ATTRIBUTE_CAPACITY;

    #[cfg(debug_assertions)]
    BLE_DIAGNOSTIC_STAGE.store(1, Ordering::Release);

    while !BLE_PROFILE_ACTIVE.load(Ordering::Relaxed) {
        embassy_futures::yield_now().await;
    }
    #[cfg(debug_assertions)]
    BLE_DIAGNOSTIC_STAGE.store(2, Ordering::Release);

    let connector = match esp_radio::ble::controller::BleConnector::new(bt, Default::default()) {
        Ok(connector) => connector,
        Err(_) => {
            native_radio_log!("native_ble_error stage=connector_init");
            return;
        }
    };
    let controller = bt_hci::controller::ExternalController::<_, 4>::new(connector);
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let stack = trouble_host::new(controller, &mut resources);
    let Host {
        mut peripheral,
        mut runner,
        ..
    } = stack.build();
    #[cfg(debug_assertions)]
    BLE_DIAGNOSTIC_STAGE.store(3, Ordering::Release);

    let mut ctrl_storage = [0u8; 64];
    let mut data_storage = [0u8; BLE_TRANSFER_CHUNK_BYTES];
    let mut status_storage = [BLE_STATUS_PENDING; 1];
    let mut service_changed_storage = [0u8; 4];
    let mut table: AttributeTable<'_, NoopRawMutex, ATTRIBUTE_MAX> = AttributeTable::new();
    let mut gap_service = table.add_service(Service::new(0x1800u16));
    let _ = gap_service.add_characteristic_ro(0x2a00u16, b"XTEINK X4");
    let _ = gap_service.add_characteristic_ro(0x2a01u16, &[0u8, 0u8]);
    gap_service.build();
    let mut gatt_service = table.add_service(Service::new(0x1801u16));
    let service_changed_handle = gatt_service
        .add_characteristic(
            0x2a05u16,
            &[CharacteristicProp::Indicate],
            [0x01, 0x00, 0xff, 0xff],
            &mut service_changed_storage,
        )
        .build();
    let service_changed_cccd_handle = service_changed_handle.cccd_handle;
    gatt_service.build();
    let (ctrl_value_handle, data_value_handle, status_handle, status_cccd_handle) = {
        let mut service = table.add_service(Service::new(BLE_FILE_SERVICE_UUID.clone()));
        let ctrl_handle = service
            .add_characteristic(
                BLE_FILE_CTRL_UUID.clone(),
                &[CharacteristicProp::Write],
                heapless_09::Vec::<u8, 64>::new(),
                &mut ctrl_storage,
            )
            .build();
        let data_handle = service
            .add_characteristic(
                BLE_FILE_DATA_UUID.clone(),
                &[CharacteristicProp::Write],
                heapless_09::Vec::<u8, BLE_TRANSFER_CHUNK_BYTES>::new(),
                &mut data_storage,
            )
            .build();
        let status_handle = service
            .add_characteristic(
                BLE_FILE_STAT_UUID.clone(),
                &[
                    CharacteristicProp::Read,
                    CharacteristicProp::Notify,
                    CharacteristicProp::Indicate,
                ],
                [0u8; 1],
                &mut status_storage,
            )
            .build();
        (
            ctrl_handle.handle,
            data_handle.handle,
            status_handle,
            status_handle.cccd_handle,
        )
    };
    let server =
        AttributeServer::<NoopRawMutex, DefaultPacketPool, ATTRIBUTE_MAX, 2, CONNECTIONS_MAX>::new(
            table,
        );

    let _ = select(runner.run(), async {
        let service_uuid_bytes = match BLE_FILE_SERVICE_UUID {
            Uuid::Uuid128(bytes) => [bytes],
            Uuid::Uuid16(_) => [[0; 16]],
        };
        let mut adv_data = [0u8; 31];
        let adv_data_len = AdStructure::encode_slice(
            &[
                AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                AdStructure::ServiceUuids128(&service_uuid_bytes),
            ],
            &mut adv_data,
        )
        .unwrap_or(0);
        let mut scan_data = [0u8; 31];
        let scan_data_len = AdStructure::encode_slice(
            &[AdStructure::CompleteLocalName(b"XTEINK X4")],
            &mut scan_data,
        )
        .unwrap_or(0);
        let mut transfer = BleGattTransferState::new();
        loop {
            if !BLE_PROFILE_ACTIVE.load(Ordering::Relaxed) {
                transfer.clear();
                embassy_futures::yield_now().await;
                continue;
            }
            let acceptor = match peripheral
                .advertise(
                    &Default::default(),
                    Advertisement::ConnectableScannableUndirected {
                        adv_data: &adv_data[..adv_data_len],
                        scan_data: &scan_data[..scan_data_len],
                    },
                )
                .await
            {
                Ok(acceptor) => acceptor,
                Err(_) => {
                    embassy_futures::yield_now().await;
                    continue;
                }
            };
            #[cfg(debug_assertions)]
            BLE_DIAGNOSTIC_STAGE.store(4, Ordering::Release);
            let conn = match acceptor.accept().await {
                Ok(conn) => match conn.with_attribute_server(&server) {
                    Ok(conn) => conn,
                    Err(_) => {
                        #[cfg(debug_assertions)]
                        ble_diagnostic_set_flag(BLE_DIAGNOSTIC_GATT_ATTACH_FAILURE);
                        continue;
                    }
                },
                Err(_) => continue,
            };
            #[cfg(debug_assertions)]
            BLE_DIAGNOSTIC_STAGE.store(5, Ordering::Release);
            transfer.clear();
            let _ = status_handle.set(&server, &[BLE_STATUS_PENDING]);
            let mut pending_status = None;
            let mut latched_status = BLE_STATUS_PENDING;
            loop {
                let mut status_notified = false;
                while let Ok(status) = BLE_RUNTIME_STATUSES.try_receive() {
                    latched_status = status;
                    let _ = status_handle.set(&server, &[status]);
                    pending_status = Some(status);
                }
                if let Some(status) = pending_status {
                    let (notifications_enabled, indications_enabled) = status_cccd_handle
                        .and_then(|cccd_handle| {
                            server.get_cccd_table(conn.raw()).map(|table| {
                                table
                                    .inner()
                                    .iter()
                                    .find(|(handle, _)| *handle == cccd_handle)
                                    .map(|(_, cccd)| (cccd.should_notify(), cccd.should_indicate()))
                                    .unwrap_or((false, false))
                            })
                        })
                        .unwrap_or((false, false));
                    #[cfg(debug_assertions)]
                    if notifications_enabled {
                        ble_diagnostic_set_flag(BLE_DIAGNOSTIC_STATUS_NOTIFY_ENABLED);
                    } else if indications_enabled {
                        ble_diagnostic_set_flag(BLE_DIAGNOSTIC_STATUS_INDICATE_ENABLED);
                    } else {
                        ble_diagnostic_set_flag(BLE_DIAGNOSTIC_STATUS_INDICATE_DISABLED);
                    }
                    if notifications_enabled || indications_enabled {
                        let sent = if notifications_enabled {
                            status_handle.notify(&conn, &[status]).await
                        } else {
                            status_handle.indicate(&conn, &[status]).await
                        };
                        if sent.is_ok() {
                            #[cfg(debug_assertions)]
                            ble_diagnostic_set_flag(BLE_DIAGNOSTIC_NOTIFY_SENT);
                            status_notified = true;
                        } else {
                            #[cfg(debug_assertions)]
                            ble_diagnostic_set_flag(BLE_DIAGNOSTIC_NOTIFY_FAILED);
                        }
                        pending_status = None;
                    } else {
                        pending_status = None;
                    }
                }
                if status_notified && !BLE_PROFILE_ACTIVE.load(Ordering::Relaxed) {
                    embassy_time::Timer::after_millis(100).await;
                    cancel_ble_transfer(&transfer);
                    transfer.clear();
                    break;
                }
                let wait = if pending_status.is_some() {
                    embassy_time::Duration::from_millis(20)
                } else {
                    embassy_time::Duration::from_millis(BLE_CONNECTION_WATCHDOG_MS)
                };
                let activity = embassy_time::with_timeout(
                    wait,
                    select(conn.next(), BLE_RUNTIME_STATUSES.receive()),
                )
                .await;
                let Ok(activity) = activity else {
                    if pending_status.is_some() {
                        continue;
                    }
                    #[cfg(debug_assertions)]
                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_CONNECTION_WATCHDOG);
                    cancel_ble_transfer(&transfer);
                    transfer.clear();
                    while BLE_RUNTIME_STATUSES.try_receive().is_ok() {}
                    break;
                };
                match activity {
                    Either::First(event) => {
                        #[cfg(debug_assertions)]
                        ble_diagnostic_set_flag(BLE_DIAGNOSTIC_GATT_EVENT);
                        match event {
                            GattConnectionEvent::Disconnected { reason: _reason } => {
                                #[cfg(debug_assertions)]
                                {
                                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_DISCONNECTED);
                                    BLE_DIAGNOSTIC_DISCONNECT_REASON
                                        .store(_reason.into_inner() as usize, Ordering::Release);
                                }
                                cancel_ble_transfer(&transfer);
                                transfer.clear();
                                break;
                            }
                            GattConnectionEvent::Gatt {
                                event: GattEvent::Write(event),
                            } => {
                                let handle = event.handle();
                                let mut payload = [0u8; BLE_TRANSFER_CHUNK_BYTES];
                                let len = event.data().len().min(payload.len());
                                payload[..len].copy_from_slice(&event.data()[..len]);
                                let is_cccd = status_cccd_handle == Some(handle)
                                    || service_changed_cccd_handle == Some(handle);
                                #[cfg(debug_assertions)]
                                if status_cccd_handle == Some(handle) {
                                    let cccd_value = if len >= 2 {
                                        u16::from_le_bytes([payload[0], payload[1]])
                                    } else {
                                        0
                                    };
                                    if cccd_value & 0x02 != 0 {
                                        ble_diagnostic_set_flag(
                                            BLE_DIAGNOSTIC_STATUS_CCCD_INDICATE,
                                        );
                                    } else if cccd_value & 0x01 != 0 {
                                        ble_diagnostic_set_flag(BLE_DIAGNOSTIC_STATUS_CCCD_NOTIFY);
                                    } else {
                                        ble_diagnostic_set_flag(
                                            BLE_DIAGNOSTIC_STATUS_CCCD_DISABLED,
                                        );
                                    }
                                }
                                let outcome = if is_cccd {
                                    BleGattWriteOutcome::Accept
                                } else if handle == ctrl_value_handle {
                                    handle_ble_control_write(&mut transfer, &payload[..len])
                                } else if handle == data_value_handle {
                                    handle_ble_data_write(&mut transfer, &payload[..len])
                                } else {
                                    BleGattWriteOutcome::Reject
                                };
                                while BLE_PIPELINE_DEPTH.saturating_sub(BLE_STORAGE_COMMANDS.len())
                                    < outcome.queue_slots()
                                {
                                    #[cfg(debug_assertions)]
                                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_BACKPRESSURE);
                                    embassy_futures::yield_now().await;
                                }
                                if outcome.is_accepted() {
                                    match event.accept() {
                                        Ok(reply) => reply.send().await,
                                        Err(_) => {
                                            #[cfg(debug_assertions)]
                                            ble_diagnostic_set_flag(BLE_DIAGNOSTIC_ACCEPT_FAILED);
                                        }
                                    }
                                    embassy_time::Timer::after_millis(2).await;
                                    outcome.enqueue().await;
                                } else if let Ok(reply) = event
                                    .reject(trouble_host::prelude::AttErrorCode::UNLIKELY_ERROR)
                                {
                                    #[cfg(debug_assertions)]
                                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_WRITE_REJECTED);
                                    reply.send().await;
                                }
                            }
                            GattConnectionEvent::Gatt {
                                event: GattEvent::Read(event),
                            } => {
                                while let Ok(status) = BLE_RUNTIME_STATUSES.try_receive() {
                                    latched_status = status;
                                    let _ = status_handle.set(&server, &[status]);
                                    pending_status = Some(status);
                                }
                                if event.handle() == status_handle.handle {
                                    let _ = status_handle.set(&server, &[latched_status]);
                                    #[cfg(debug_assertions)]
                                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_STATUS_READ);
                                }
                                if let Ok(reply) = event.accept() {
                                    reply.send().await;
                                } else {
                                    #[cfg(debug_assertions)]
                                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_ACCEPT_FAILED);
                                }
                            }
                            GattConnectionEvent::Gatt { event } => {
                                #[cfg(debug_assertions)]
                                {
                                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_GATT_OTHER);
                                    let count =
                                        BLE_DIAGNOSTIC_GATT_OTHER_COUNT.load(Ordering::Acquire);
                                    BLE_DIAGNOSTIC_GATT_OTHER_COUNT
                                        .store(count.saturating_add(1), Ordering::Release);
                                }
                                if let Ok(reply) = event.accept() {
                                    reply.send().await;
                                } else {
                                    #[cfg(debug_assertions)]
                                    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_ACCEPT_FAILED);
                                }
                            }
                            GattConnectionEvent::RequestConnectionParams(request) => {
                                #[cfg(debug_assertions)]
                                ble_diagnostic_set_flag(BLE_DIAGNOSTIC_CONNECTION_PARAMS_REQUEST);
                                if request.accept(None, &stack).await.is_ok() {
                                    #[cfg(debug_assertions)]
                                    ble_diagnostic_set_flag(
                                        BLE_DIAGNOSTIC_CONNECTION_PARAMS_ACCEPTED,
                                    );
                                } else {
                                    #[cfg(debug_assertions)]
                                    ble_diagnostic_set_flag(
                                        BLE_DIAGNOSTIC_CONNECTION_PARAMS_FAILED,
                                    );
                                    cancel_ble_transfer(&transfer);
                                    transfer.clear();
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    Either::Second(status) => {
                        latched_status = status;
                        let _ = status_handle.set(&server, &[status]);
                        pending_status = Some(status);
                    }
                }
                if !BLE_PROFILE_ACTIVE.load(Ordering::Relaxed) {
                    while let Ok(status) = BLE_RUNTIME_STATUSES.try_receive() {
                        latched_status = status;
                        let _ = status_handle.set(&server, &[status]);
                        pending_status = Some(status);
                    }
                    if pending_status.is_some() {
                        continue;
                    }
                    cancel_ble_transfer(&transfer);
                    transfer.clear();
                    break;
                }
            }
        }
    })
    .await;
    #[cfg(debug_assertions)]
    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_RUNNER_EXIT);
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
fn handle_ble_control_write(
    transfer: &mut BleGattTransferState,
    data: &[u8],
) -> BleGattWriteOutcome {
    let Some((&op, rest)) = data.split_first() else {
        ble_report_status(201, BLE_STATUS_ERROR);
        return BleGattWriteOutcome::Reject;
    };
    match op {
        BLE_OP_BEGIN => {
            #[cfg(debug_assertions)]
            ble_diagnostic_set_flag(BLE_DIAGNOSTIC_BEGIN_WRITE);
            if rest.len() < 6 {
                ble_report_status(202, BLE_STATUS_ERROR);
                return BleGattWriteOutcome::Reject;
            }
            transfer.clear();
            transfer.total_len = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
            transfer.expected_name_len = u16::from_le_bytes([rest[4], rest[5]]) as usize;
            if transfer.total_len == 0
                || transfer.expected_name_len == 0
                || transfer.expected_name_len > transfer.name.capacity()
            {
                transfer.error = true;
                ble_report_status(203, BLE_STATUS_ERROR);
                return BleGattWriteOutcome::Reject;
            }
            BleGattWriteOutcome::Accept
        }
        BLE_OP_NAME => {
            #[cfg(debug_assertions)]
            ble_diagnostic_set_flag(BLE_DIAGNOSTIC_NAME_WRITE);
            if transfer.error || transfer.expected_name_len == 0 {
                ble_report_status(204, BLE_STATUS_ERROR);
                return BleGattWriteOutcome::Reject;
            }
            if transfer
                .name
                .push_str(core::str::from_utf8(rest).unwrap_or(""))
                .is_err()
                || transfer.name.len() > transfer.expected_name_len
            {
                transfer.error = true;
                ble_report_status(205, BLE_STATUS_ERROR);
                return BleGattWriteOutcome::Reject;
            }
            if transfer.name.len() == transfer.expected_name_len && !transfer.begin_sent {
                let sequence = BLE_TRANSFER_SESSION_SEQUENCE.load(Ordering::Acquire);
                BLE_TRANSFER_SESSION_SEQUENCE.store(sequence.wrapping_add(1), Ordering::Release);
                let session_id = TransferSessionId::new(sequence);
                let route =
                    match BleUploadRoute::new(transfer.name.as_str(), "", "", transfer.total_len) {
                        Ok(route) => route,
                        Err(_) => {
                            transfer.error = true;
                            ble_report_status(207, BLE_STATUS_ERROR);
                            return BleGattWriteOutcome::Reject;
                        }
                    };
                transfer.session_id = Some(session_id);
                transfer.begin_sent = true;
                #[cfg(debug_assertions)]
                ble_diagnostic_set_flag(BLE_DIAGNOSTIC_NAME_ACCEPTED);
                return BleGattWriteOutcome::Enqueue(BleStorageCommand::Begin {
                    session_id,
                    route,
                });
            }
            #[cfg(debug_assertions)]
            ble_diagnostic_set_flag(BLE_DIAGNOSTIC_NAME_ACCEPTED);
            BleGattWriteOutcome::Accept
        }
        BLE_OP_ABORT => {
            cancel_ble_transfer(transfer);
            transfer.clear();
            BleGattWriteOutcome::Accept
        }
        _ => {
            transfer.error = true;
            ble_report_status(206, BLE_STATUS_ERROR);
            BleGattWriteOutcome::Reject
        }
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
fn handle_ble_data_write(transfer: &mut BleGattTransferState, data: &[u8]) -> BleGattWriteOutcome {
    #[cfg(debug_assertions)]
    ble_diagnostic_set_flag(BLE_DIAGNOSTIC_DATA_WRITE);
    if transfer.error
        || !transfer.begin_sent
        || data.is_empty()
        || transfer.received.saturating_add(data.len()) > transfer.total_len
    {
        transfer.error = true;
        ble_report_status(301, BLE_STATUS_ERROR);
        return BleGattWriteOutcome::Reject;
    }
    let Some(session_id) = transfer.session_id else {
        transfer.error = true;
        ble_report_status(302, BLE_STATUS_ERROR);
        return BleGattWriteOutcome::Reject;
    };
    let mut bytes = [0u8; BLE_TRANSFER_CHUNK_BYTES];
    bytes[..data.len()].copy_from_slice(data);
    let command = BleStorageCommand::Chunk {
        session_id,
        offset: transfer.received,
        bytes,
        len: data.len(),
    };
    transfer.received += data.len();
    if transfer.received == transfer.total_len {
        BleGattWriteOutcome::EnqueueAndCommit(command, session_id)
    } else {
        BleGattWriteOutcome::Enqueue(command)
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "ble"
))]
fn cancel_ble_transfer(transfer: &BleGattTransferState) {
    let Some(session_id) = transfer.session_id else {
        return;
    };
    BLE_CANCELLED_SESSION.store(session_id.get(), Ordering::Release);
    let _ = BLE_STORAGE_CANCELS.try_send(session_id);
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
    AS: NativeAppStorage,
>(
    runtime: &mut NativeRuntime<B, D, C, FB, AS>,
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
fn handle_resource_install_request<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    FB: NativeFileBackend,
    AS: NativeAppStorage,
>(
    runtime: &mut NativeRuntime<B, D, C, FB, AS>,
    sessions: &mut squid_device_protocol::ProtocolSessions,
    request: &squid_device_protocol::DeviceRequest<'_>,
    response: &mut [u8],
) -> Result<usize, squid_device_protocol::DecodeError> {
    use squid_device_protocol::{
        encode_empty_response_into, encode_error_response_into, HostAction, Status,
    };

    match sessions.next_action(request) {
        Ok(HostAction::BeginResourceInstall {
            app_id,
            resource_path,
            total_len,
        }) => {
            if let Err(error) = runtime.begin_resource_install(app_id, resource_path, total_len) {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            let _ = sessions.complete_begin_resource_install("/tmp/resource");
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::WriteResourceChunk { offset, bytes, .. }) => {
            if let Err(error) = runtime.write_resource_install_chunk(offset, bytes) {
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
            let _ = sessions.complete_resource_chunk(bytes);
            encode_empty_response_into(request.opcode, Status::Ok, request.sequence, response)
        }
        Ok(HostAction::CommitResourceInstall { .. }) => {
            if let Err(error) = runtime.commit_resource_install() {
                return encode_error_response_into(
                    request.opcode,
                    request.sequence,
                    -1,
                    native_runtime_error_name(error),
                    response,
                );
            }
            sessions.complete_resource_commit();
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
    AS: NativeAppStorage,
>(
    runtime: &mut NativeRuntime<B, D, C, FB, AS>,
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
                    runtime.record_error(error);
                    return encode_error_response_into(
                        request.opcode,
                        request.sequence,
                        -1,
                        error,
                        response,
                    );
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
                runtime.record_error(error);
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
                runtime.record_error(error);
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
    AS: NativeAppStorage,
>(
    runtime: &mut NativeRuntime<B, D, C, FB, AS>,
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
        squidscript_fw_core::native_runtime::NativeRuntimeError::UploadSessionActive => {
            "upload_session_active"
        }
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
    #[cfg(feature = "wifi")]
    wifi_sta_ip: heapless::String<15>,
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
            #[cfg(feature = "wifi")]
            wifi_sta_ip: heapless::String::new(),
            #[cfg(feature = "ble")]
            ble_profile_id: heapless::String::new(),
            #[cfg(feature = "ble")]
            ble_profile_start_events: 0,
            #[cfg(feature = "ble")]
            ble_profile_stop_events: 0,
        }
    }

    #[cfg(feature = "wifi")]
    fn record_station_connected_event(
        &mut self,
        ssid: &str,
        bssid: [u8; 6],
        authmode: esp_radio::wifi::AuthenticationMethod,
    ) {
        self.wifi_sta_connected_events += 1;
        self.wifi_sta_auth = wifi_auth_label(Some(authmode));
        self.wifi_sta_ssid.clear();
        let _ = self.wifi_sta_ssid.push_str(ssid);
        write_bssid_text(&mut self.wifi_sta_bssid, bssid);
        self.wifi_ap_active = false;
        self.wifi_ap_ssid.clear();
        self.wifi_sta_active = true;
        self.wifi_last_disconnect_reason = None;
        self.wifi_last_disconnect_reason_code = 0;
        self.wifi_last_backend_code = None;
    }

    #[cfg(feature = "wifi")]
    fn record_station_ip(&mut self, ip: Option<embassy_net::Ipv4Address>) {
        self.wifi_sta_ip.clear();
        if let Some(ip) = ip {
            use core::fmt::Write;
            let _ = write!(&mut self.wifi_sta_ip, "{ip}");
        }
    }

    #[cfg(feature = "wifi")]
    fn record_station_disconnected_event(
        &mut self,
        bssid: [u8; 6],
        reason_code: u32,
        backend_code: Option<&'static str>,
    ) {
        self.wifi_sta_disconnected_events += 1;
        self.wifi_last_disconnect_reason = Some("disconnected");
        self.wifi_last_disconnect_reason_code = reason_code as i32;
        self.wifi_last_backend_code = backend_code;
        write_bssid_text(&mut self.wifi_sta_bssid, bssid);
        self.wifi_sta_ip.clear();
    }

    #[cfg(feature = "wifi")]
    fn record_station_connect_failure(&mut self, backend_code: &'static str) {
        self.wifi_sta_disconnected_events += 1;
        self.wifi_last_disconnect_reason = Some("failed");
        self.wifi_last_disconnect_reason_code = 0;
        self.wifi_last_backend_code = Some(backend_code);
        self.wifi_sta_ip.clear();
    }

    #[cfg(feature = "wifi")]
    fn record_access_point_started(&mut self) {
        self.wifi_ap_active = true;
        self.wifi_sta_active = false;
        self.wifi_sta_auth = None;
        self.wifi_sta_ssid.clear();
        self.wifi_sta_bssid.clear();
        self.wifi_ap_start_events += 1;
        self.wifi_last_backend_code = Some("ap-start-event");
    }

    #[cfg(feature = "wifi")]
    fn record_access_point_start_ok(&mut self) {
        self.wifi_ap_active = true;
        self.wifi_sta_active = false;
        self.wifi_sta_auth = None;
        self.wifi_sta_ssid.clear();
        self.wifi_sta_bssid.clear();
        self.wifi_last_backend_code = Some("ap-start-ok");
    }

    #[cfg(feature = "wifi")]
    fn record_access_point_start_failure(&mut self, backend_code: &'static str) {
        self.wifi_ap_active = false;
        self.wifi_last_backend_code = Some(backend_code);
    }
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
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
        Some(esp_radio::wifi::AuthenticationMethod::Wpa3EntSuiteB192Bit) => Some("WPA3_ENTERPRISE"),
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

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
fn write_bssid_text(out: &mut heapless::String<17>, bssid: [u8; 6]) {
    out.clear();
    let _ = write!(
        out,
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        bssid[0], bssid[1], bssid[2], bssid[3], bssid[4], bssid[5]
    );
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
fn wifi_ap_client_count() -> Option<i32> {
    Some(WIFI_AP_CLIENT_COUNT.load(Ordering::Acquire).max(0))
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
fn wifi_ap_ip_text() -> &'static str {
    "192.168.4.1"
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "native-radio-services",
    feature = "wifi"
))]
fn wifi_ap_netmask_text() -> &'static str {
    "255.255.255.0"
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
                let controller_ptr = self
                    .wifi
                    .as_ref()
                    .map(|controller| controller as *const _ as usize)
                    .unwrap_or(0);
                WIFI_CONTROLLER_PTR.store(controller_ptr, Ordering::Release);
                WIFI_AP_CLIENT_COUNT.store(0, Ordering::Release);
                Ok(())
            }
            #[cfg(feature = "ble")]
            RadioKind::Ble => Ok(()),
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
                self.wifi_sta_ip.clear();
                self.wifi_last_disconnect_reason = None;
                self.wifi_last_disconnect_reason_code = 0;
                WIFI_CONTROLLER_PTR.store(0, Ordering::Release);
                WIFI_AP_CLIENT_COUNT.store(0, Ordering::Release);
                WIFI_DHCP_LEASE_COUNT.store(0, Ordering::Release);
            }
            #[cfg(feature = "ble")]
            RadioKind::Ble => {
                if !self.ble_profile_id.is_empty() {
                    self.ble_profile_stop_events += 1;
                    self.ble_profile_id.clear();
                }
                BLE_PROFILE_ACTIVE.store(false, Ordering::Relaxed);
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    fn start_ble_profile(&mut self, id: &str) -> Result<(), ()> {
        #[cfg(feature = "ble")]
        {
            self.ble_profile_id.clear();
            self.ble_profile_id.push_str(id).map_err(|_| ())?;
            self.ble_profile_start_events += 1;
            BLE_PROFILE_ACTIVE.store(true, Ordering::Relaxed);
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
                BLE_PROFILE_ACTIVE.store(false, Ordering::Relaxed);
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
            WIFI_AP_CLIENT_COUNT.store(0, Ordering::Release);
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

    fn begin_start_wifi_ap(&mut self, ssid: &str) -> NativeWifiBackendOperation {
        #[cfg(feature = "wifi")]
        {
            if self.wifi.is_none() {
                return NativeWifiBackendOperation::Error {
                    error: "unavailable",
                };
            }
            let mut command_ssid = heapless::String::<32>::new();
            if command_ssid.push_str(ssid).is_err() {
                self.wifi_last_backend_code = Some("ap-ssid");
                return NativeWifiBackendOperation::Error { error: "invalid" };
            }
            WIFI_AP_CLIENT_COUNT.store(0, Ordering::Release);
            if WIFI_COMMANDS
                .try_send(NativeWifiCommand::StartAp { ssid: command_ssid })
                .is_err()
            {
                self.wifi_last_backend_code = Some("ap-queue");
                NativeWifiBackendOperation::Error { error: "wifi busy" }
            } else {
                self.wifi_ap_ssid.clear();
                let _ = self.wifi_ap_ssid.push_str(ssid);
                self.wifi_ap_active = true;
                self.wifi_sta_active = false;
                self.wifi_sta_auth = None;
                self.wifi_sta_ssid.clear();
                self.wifi_sta_bssid.clear();
                self.wifi_ap_start_events += 1;
                self.wifi_last_backend_code = Some("ap-pending");
                NativeWifiBackendOperation::Pending
            }
        }
        #[cfg(not(feature = "wifi"))]
        {
            let _ = ssid;
            NativeWifiBackendOperation::Error {
                error: "unsupported",
            }
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
            WIFI_AP_CLIENT_COUNT.store(0, Ordering::Release);
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

    fn begin_connect_wifi_station(
        &mut self,
        ssid: &str,
        password: &str,
    ) -> NativeWifiBackendOperation {
        #[cfg(feature = "wifi")]
        {
            if self.wifi.is_none() {
                return NativeWifiBackendOperation::Error {
                    error: "unavailable",
                };
            }
            let mut command_ssid = heapless::String::<32>::new();
            let mut command_password = heapless::String::<64>::new();
            if command_ssid.push_str(ssid).is_err() || command_password.push_str(password).is_err()
            {
                self.wifi_last_backend_code = Some("connect-credentials");
                return NativeWifiBackendOperation::Error { error: "invalid" };
            }
            WIFI_AP_CLIENT_COUNT.store(0, Ordering::Release);
            if WIFI_COMMANDS
                .try_send(NativeWifiCommand::Connect {
                    ssid: command_ssid,
                    password: command_password,
                })
                .is_err()
            {
                self.wifi_last_backend_code = Some("connect-queue");
                NativeWifiBackendOperation::Error { error: "wifi busy" }
            } else {
                self.wifi_ap_active = false;
                self.wifi_ap_ssid.clear();
                self.wifi_sta_active = true;
                self.wifi_last_backend_code = Some("connect-pending");
                NativeWifiBackendOperation::Pending
            }
        }
        #[cfg(not(feature = "wifi"))]
        {
            let _ = ssid;
            let _ = password;
            NativeWifiBackendOperation::Error {
                error: "unsupported",
            }
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
            let connected = self.wifi_sta_connected_events > self.wifi_sta_disconnected_events;
            let clients = if self.wifi_ap_active {
                wifi_ap_client_count()
                    .unwrap_or(0)
                    .max(WIFI_DHCP_LEASE_COUNT.load(Ordering::Acquire))
            } else {
                0
            };
            let channel = if self.wifi_ap_active { 1 } else { 0 };
            let rssi = 0;
            let auth = self.wifi_sta_auth;
            squidscript_fw_core::native_runtime::NativeWifiStatus {
                mode: self.wifi_mode(),
                ssid: if self.wifi_ap_active {
                    Some(self.wifi_ap_ssid.as_str())
                } else if self.wifi_sta_active && !self.wifi_sta_ssid.is_empty() {
                    Some(self.wifi_sta_ssid.as_str())
                } else {
                    None
                },
                ip_address: if self.wifi_ap_active {
                    Some(wifi_ap_ip_text())
                } else if connected && !self.wifi_sta_ip.is_empty() {
                    Some(self.wifi_sta_ip.as_str())
                } else {
                    None
                },
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
                driver_started: self.wifi_ap_active || self.wifi_sta_active || connected,
                configured: self.wifi_ap_active || self.wifi_sta_active || connected,
                channel,
                clients,
                ap_start_events: self.wifi_ap_start_events,
                ap_stop_events: self.wifi_ap_stop_events,
                probe_events: 0,
                sta_connected_events: self.wifi_sta_connected_events,
                sta_disconnected_events: self.wifi_sta_disconnected_events,
                last_backend_code: self.wifi_last_backend_code,
                connected,
                scan_matches: critical_section::with(|cs| {
                    WIFI_SCAN_RESULTS.borrow_ref(cs).len() as i32
                }),
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
            let results =
                embassy_futures::block_on(controller.scan_async(&config)).map_err(|_| {
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

    fn begin_scan_wifi(&mut self) -> NativeWifiBackendOperation {
        #[cfg(feature = "wifi")]
        {
            if self.wifi.is_none() {
                return NativeWifiBackendOperation::Error {
                    error: "unavailable",
                };
            }
            critical_section::with(|cs| {
                WIFI_SCAN_RESULTS.borrow_ref_mut(cs).clear();
            });
            if WIFI_COMMANDS.try_send(NativeWifiCommand::Scan).is_err() {
                self.wifi_last_backend_code = Some("scan-queue");
                NativeWifiBackendOperation::Error { error: "wifi busy" }
            } else {
                self.wifi_last_backend_code = Some("scan-pending");
                NativeWifiBackendOperation::Pending
            }
        }
        #[cfg(not(feature = "wifi"))]
        {
            NativeWifiBackendOperation::Error {
                error: "unsupported",
            }
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
            critical_section::with(|cs| {
                Ok(WIFI_SCAN_RESULTS
                    .borrow_ref(cs)
                    .get(index as usize)
                    .copied())
            })
        }
        #[cfg(not(feature = "wifi"))]
        {
            let _ = index;
            Err("unsupported")
        }
    }

    fn wifi_ap_ip(&self) -> squidscript_fw_core::native_runtime::NativeWifiApIp<'_> {
        #[cfg(feature = "native-radio-services")]
        {
            if self.wifi_ap_active {
                squidscript_fw_core::native_runtime::NativeWifiApIp {
                    ip: Some(wifi_ap_ip_text()),
                    gw: Some(wifi_ap_ip_text()),
                    netmask: Some(wifi_ap_netmask_text()),
                    error: None,
                }
            } else {
                squidscript_fw_core::native_runtime::NativeWifiApIp::unavailable()
            }
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
