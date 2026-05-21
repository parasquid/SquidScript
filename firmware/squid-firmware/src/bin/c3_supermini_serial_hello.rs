#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use core::fmt::Write;

use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    time::Instant,
    usb_serial_jtag::UsbSerialJtag,
};
use esp_storage::FlashStorage;
use squid_firmware::{
    dev_harness::{AppRegistry, AppSlot, AppStorageError},
    serial::{
        boot_main, handle_command, storage_error_from_persistent, trim_ascii, ActiveVm, LineBuffer,
        RuntimeSink, TempApp, BUILD_ID,
    },
    storage::{LittleFsAppStorage, SquidFlashRegion},
};
use squidvm_core::{error::VmError, limits::MAX_APP_BYTES};

static mut APP_LOAD_BYTES: [u8; MAX_APP_BYTES] = [0; MAX_APP_BYTES];

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let delay = Delay::new();
    let led = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());
    let external_indicator = Output::new(peripherals.GPIO10, Level::Low, OutputConfig::default());
    let flash = FlashStorage::new(peripherals.FLASH);
    let mut app_storage = LittleFsAppStorage::new(SquidFlashRegion::new(flash));
    let mut serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
    let mut line = LineBuffer::new();
    let mut registry = AppRegistry::new();
    let mut runtime = RuntimeSink::new(led, external_indicator);
    let mut vm: Option<ActiveVm> = None;
    let mut vm_slot: Option<AppSlot> = None;
    let mut temp_app = TempApp::empty();
    let mut last_error: Option<VmError> = None;
    let mut storage_error: Option<AppStorageError> = None;

    match registry.load_from_storage(&mut app_storage, unsafe { &mut APP_LOAD_BYTES }) {
        Ok(_) => {}
        Err(error) => {
            storage_error = Some(storage_error_from_persistent(error));
        }
    }

    writeln!(serial, "SquidScript reference firmware").ok();
    writeln!(serial, "target=esp32c3-super-mini build={BUILD_ID}").ok();
    writeln!(serial, "type help").ok();
    boot_main(
        &mut serial,
        &registry,
        &mut app_storage,
        unsafe { &mut APP_LOAD_BYTES },
        &mut temp_app,
        &mut runtime,
        &mut vm,
        &mut vm_slot,
        &mut last_error,
        &mut storage_error,
    );

    loop {
        runtime.breathe_once(&delay);
        runtime.advance_time(
            Instant::now(),
            &registry,
            &mut app_storage,
            unsafe { &mut APP_LOAD_BYTES },
            &mut temp_app,
            &mut vm,
            &mut vm_slot,
            &mut last_error,
            &mut storage_error,
        );
        if runtime.take_root_restart() {
            boot_main(
                &mut serial,
                &registry,
                &mut app_storage,
                unsafe { &mut APP_LOAD_BYTES },
                &mut temp_app,
                &mut runtime,
                &mut vm,
                &mut vm_slot,
                &mut last_error,
                &mut storage_error,
            );
        }
        match serial.read_byte() {
            Ok(byte) => {
                if let Some(command) = line.push(byte) {
                    let command = trim_ascii(command);
                    if !command.is_empty() {
                        handle_command(
                            command,
                            &mut serial,
                            &delay,
                            &mut registry,
                            &mut app_storage,
                            unsafe { &mut APP_LOAD_BYTES },
                            &mut temp_app,
                            &mut runtime,
                            &mut vm,
                            &mut vm_slot,
                            &mut last_error,
                            &mut storage_error,
                        );
                    }
                }
            }
            Err(_) => {}
        }
    }
}
