#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
};
use esp_println::println;
use squid_firmware::{run_serial_probe, Console, SerialProbeInfo};

const BREATH_STEPS: [u32; 17] = [
    0, 1, 2, 4, 7, 11, 16, 24, 35, 50, 65, 76, 84, 89, 93, 96, 100,
];
const PWM_PERIOD_US: u32 = 2_000;

const BUILD_ID: &str = match option_env!("SQUID_FIRMWARE_BUILD_ID") {
    Some(value) => value,
    None => "dev-build",
};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let delay = Delay::new();
    let mut console = SerialConsole;

    run_serial_probe(&mut console, SerialProbeInfo::esp32c3_super_mini(BUILD_ID));

    // GPIO8 is the common ESP32-C3 Super Mini onboard LED, but clone boards vary.
    // Serial output is the primary acceptance signal; this LED is only a bonus probe.
    let mut candidate_led = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());

    loop {
        println!("heartbeat: esp32c3-super-mini {}", BUILD_ID);
        breathe(&mut candidate_led, &delay);
    }
}

fn breathe(led: &mut Output<'_>, delay: &Delay) {
    for duty in BREATH_STEPS {
        pulse(led, delay, duty);
    }
    for duty in BREATH_STEPS.iter().rev().copied() {
        pulse(led, delay, duty);
    }
}

fn pulse(led: &mut Output<'_>, delay: &Delay, duty_percent: u32) {
    for _ in 0..24 {
        let on_us = PWM_PERIOD_US * duty_percent / 100;
        let off_us = PWM_PERIOD_US - on_us;

        if on_us > 0 {
            led.set_high();
            delay.delay_micros(on_us);
        }
        if off_us > 0 {
            led.set_low();
            delay.delay_micros(off_us);
        }
    }
}

struct SerialConsole;

impl Console for SerialConsole {
    fn log(&mut self, message: &str) {
        println!("{}", message);
    }
}
