#![no_std]
#![no_main]

use core::{convert::Infallible, ptr, slice};

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    prelude::*,
    text::Text,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    main,
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::Rate,
};
use esp_println::println;
use squid_firmware::{
    run_bringup, BuildInfo, Color, Console, DisplayError, DisplaySurface, TextCommand,
};
use ssd1677::{
    Builder, DeepSleepMode, Dimensions, Display, GraphicDisplay, Interface, RefreshMode, Rotation,
};

const DISPLAY_BUFFER_BYTES: usize = 800 * 480 / 8;

static mut BLACK_BUFFER: [u8; DISPLAY_BUFFER_BYTES] = [0xFF; DISPLAY_BUFFER_BYTES];
static mut RED_BUFFER: [u8; DISPLAY_BUFFER_BYTES] = [0x00; DISPLAY_BUFFER_BYTES];

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

    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(4))
            .with_mode(Mode::_0),
    )
    .expect("SPI2 init failed")
    .with_sck(peripherals.GPIO8)
    .with_mosi(peripherals.GPIO10)
    .with_miso(peripherals.GPIO7);

    let cs = Output::new(peripherals.GPIO21, Level::High, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    let busy = Input::new(
        peripherals.GPIO6,
        InputConfig::default().with_pull(Pull::None),
    );

    let spi_device = ExclusiveDevice::new(spi, cs, delay).expect("SPI device init failed");
    let interface = Interface::new(spi_device, dc, rst, busy);
    let dimensions = Dimensions::new(800, 480).expect("valid X4 display dimensions");
    let display_config = Builder::new()
        .dimensions(dimensions)
        .rotation(Rotation::Rotate0)
        .build()
        .expect("valid SSD1677 config");
    let display = Display::new(interface, display_config);
    let graphic_display = unsafe {
        let black = slice::from_raw_parts_mut(
            ptr::addr_of_mut!(BLACK_BUFFER) as *mut u8,
            DISPLAY_BUFFER_BYTES,
        );
        let red = slice::from_raw_parts_mut(
            ptr::addr_of_mut!(RED_BUFFER) as *mut u8,
            DISPLAY_BUFFER_BYTES,
        );
        GraphicDisplay::new(display, black, red)
    };

    let mut display = X4Display {
        display: graphic_display,
        delay: Delay::new(),
    };
    let mut console = SerialConsole;
    let result = run_bringup(
        &mut display,
        &mut console,
        BuildInfo::new(BUILD_ID, "release"),
    );

    match result {
        Ok(()) => println!("SquidScript X4 bring-up complete"),
        Err(error) => println!("SquidScript X4 bring-up failed: {:?}", error),
    }

    loop {}
}

struct SerialConsole;

impl Console for SerialConsole {
    fn log(&mut self, message: &str) {
        println!("{}", message);
    }
}

struct X4Display<SPI, DC, RST, BUSY>
where
    SPI: embedded_hal::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin<Error = Infallible>,
    RST: embedded_hal::digital::OutputPin<Error = Infallible>,
    BUSY: embedded_hal::digital::InputPin<Error = Infallible>,
{
    display: GraphicDisplay<Interface<SPI, DC, RST, BUSY>, &'static mut [u8], &'static mut [u8]>,
    delay: Delay,
}

impl<SPI, DC, RST, BUSY> DisplaySurface for X4Display<SPI, DC, RST, BUSY>
where
    SPI: embedded_hal::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin<Error = Infallible>,
    RST: embedded_hal::digital::OutputPin<Error = Infallible>,
    BUSY: embedded_hal::digital::InputPin<Error = Infallible>,
{
    fn init(&mut self) -> Result<(), DisplayError> {
        self.display
            .display_mut()
            .reset(&mut self.delay)
            .map_err(|_| DisplayError::Init)
    }

    fn clear(&mut self, color: Color) -> Result<(), DisplayError> {
        let color = match color {
            Color::White => ssd1677::Color::White,
            Color::Black => ssd1677::Color::Black,
        };
        self.display.clear(color);
        Ok(())
    }

    fn draw_text(&mut self, command: TextCommand) -> Result<(), DisplayError> {
        let color = match command.color {
            Color::White => ssd1677::Color::White,
            Color::Black => ssd1677::Color::Black,
        };
        let style = MonoTextStyle::new(&FONT_10X20, color);
        Text::new(
            command.text,
            Point::new(command.x as i32, command.y as i32),
            style,
        )
        .draw(&mut self.display)
        .map(|_| ())
        .map_err(|_| DisplayError::Draw)
    }

    fn refresh(&mut self) -> Result<(), DisplayError> {
        self.display
            .update_with_mode(RefreshMode::Full, &mut self.delay)
            .map_err(|_| DisplayError::BusyTimeout)
    }

    fn sleep(&mut self) -> Result<(), DisplayError> {
        self.display
            .display_mut()
            .deep_sleep(&mut self.delay, DeepSleepMode::Normal)
            .map_err(|_| DisplayError::Sleep)
    }
}
