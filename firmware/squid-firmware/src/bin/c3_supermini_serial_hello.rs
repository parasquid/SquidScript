#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use core::fmt::Write;
use core::mem::MaybeUninit;

use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{DriveMode, Level, Output, OutputConfig},
    interrupt::software::SoftwareInterruptControl,
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    main,
    time::{Instant, Rate},
    timer::timg::TimerGroup,
    usb_serial_jtag::UsbSerialJtag,
};
use esp_radio::Controller as RadioController;
use esp_rtos::CurrentThreadHandle;
use esp_storage::FlashStorage;
use squid_firmware::{
    dev_harness::{AppRegistry, AppSlot, AppStorageError, StorageActor},
    serial::{
        boot_main, handle_command, install_wifi_event_diagnostics, storage_error_from_persistent,
        trim_ascii, ActiveVm, FirmwareWifiBackend, LineBuffer, OnboardIndicator, RuntimeSink,
        TempApp, BUILD_ID,
    },
    storage::{LittleFsAppStorage, SquidFlashRegion},
};
use squidvm_core::{error::VmError, limits::MAX_APP_BYTES};

static mut APP_LOAD_BYTES: [u8; MAX_APP_BYTES] = [0; MAX_APP_BYTES];
static mut RADIO_CONTROLLER: MaybeUninit<RadioController<'static>> = MaybeUninit::uninit();

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(size: 96 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);
    install_wifi_event_diagnostics();
    let wifi = init_wifi_backend(peripherals.WIFI);
    let delay = Delay::new();
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut indicator_timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    indicator_timer
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty10Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(1),
        })
        .ok();
    let mut indicator_channel =
        ledc.channel::<LowSpeed>(channel::Number::Channel0, peripherals.GPIO8);
    indicator_channel
        .configure(channel::config::Config {
            timer: &indicator_timer,
            duty_pct: 100,
            drive_mode: DriveMode::PushPull,
        })
        .ok();
    let led = OnboardIndicator::new(indicator_channel);
    let external_indicator = Output::new(peripherals.GPIO10, Level::Low, OutputConfig::default());
    let flash = FlashStorage::new(peripherals.FLASH);
    let mut app_storage =
        StorageActor::<_, 32>::new(LittleFsAppStorage::new(SquidFlashRegion::new(flash)));
    let mut serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
    let mut line = LineBuffer::new();
    let mut registry = AppRegistry::new();
    let mut runtime = RuntimeSink::new(led, external_indicator, wifi);
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
        runtime.poll_wifi();
        runtime.poll_indicator();
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
        CurrentThreadHandle::get().delay(esp_hal::time::Duration::from_millis(1));
    }
}

fn init_wifi_backend(wifi: esp_hal::peripherals::WIFI<'static>) -> FirmwareWifiBackend<'static> {
    let Ok(controller) = esp_radio::init() else {
        return FirmwareWifiBackend::Unavailable;
    };
    let radio = unsafe {
        RADIO_CONTROLLER.write(controller);
        &*RADIO_CONTROLLER.as_ptr()
    };
    FirmwareWifiBackend::new_esp(radio, wifi)
}
