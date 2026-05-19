pub const TARGET_ID: &str = "xteink-x4";
pub const TARGET_NAME: &str = "XTEINK X4";
pub const MCU: &str = "ESP32-C3";
pub const DISPLAY_CONTROLLER: &str = "SSD1677";
pub const DISPLAY_PANEL: &str = "GDEQ0426T82";
pub const DISPLAY_WIDTH: u16 = 800;
pub const DISPLAY_HEIGHT: u16 = 480;
pub const FLASH_SIZE_BYTES: u32 = 16 * 1024 * 1024;

pub mod pins {
    pub const DISPLAY_DC: u8 = 4;
    pub const DISPLAY_RST: u8 = 5;
    pub const DISPLAY_BUSY: u8 = 6;
    pub const SPI_MISO: u8 = 7;
    pub const SPI_SCK: u8 = 8;
    pub const SPI_MOSI: u8 = 10;
    pub const SD_CS: u8 = 12;
    pub const DISPLAY_CS: u8 = 21;
}
