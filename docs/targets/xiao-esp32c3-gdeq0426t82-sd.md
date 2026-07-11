# XIAO ESP32-C3 ePaper 4.26 + SD

Hardware research retained for a possible future native target. This board and
display combination has no current SquidScript firmware target.

## Pin Availability

| Pin | Availability | Capabilities | Used by | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| `GPIO0` | Truly unavailable |  |  | not-exposed-on-xiao-header | Not part of the documented XIAO D0-D10 connector mapping. |
| `GPIO1` | Truly unavailable |  |  | not-exposed-on-xiao-header | Not part of the documented XIAO D0-D10 connector mapping. |
| `GPIO2` | used-with-boot-caution | `gpio`, `pwm`, `adc`, `strapping` | `display.epd.rst` | used-with-boot-caution | Seeed maps XIAO D0 to GPIO2 and warns that GPIO2 is an ESP32-C3 strapping pin. The e-paper reset line must not hold it in an invalid boot state. |
| `GPIO3` | used | `gpio`, `pwm`, `adc` | `display.epd.cs` | used | Seeed maps XIAO D1 to GPIO3; the canonical DESPI-C02 wiring uses it as e-paper chip select. |
| `GPIO4` | used | `gpio`, `pwm`, `adc` | `display.epd.busy` | used | Seeed maps XIAO D2 to GPIO4; the canonical DESPI-C02 wiring uses it as e-paper BUSY. |
| `GPIO5` | used | `gpio`, `pwm`, `adc` | `display.epd.dc` | used | Seeed maps XIAO D3 to GPIO5; the canonical DESPI-C02 wiring uses it as e-paper D/C. |
| `GPIO6` | reserved-unverified | `gpio`, `pwm`, `adc`, `i2c_sda` | `storage.sd.cs.unverified` | reserved-unverified | Planned SD chip-select candidate only; confirm direct jumper wiring before enabling runtime SD. |
| `GPIO7` | reserved-unverified | `gpio`, `pwm`, `adc`, `i2c_scl`, `spi_miso` | `storage.sd.miso.unverified` | reserved-unverified | Planned SD MISO candidate only; confirm direct jumper wiring before enabling runtime SD. |
| `GPIO8` | used-with-boot-caution | `gpio`, `pwm`, `spi_sck`, `strapping` | `spi.shared.sck`, `display.epd.sck`, `storage.sd.sck` | used-with-boot-caution | Seeed maps XIAO D8 to GPIO8 and warns that GPIO8 is an ESP32-C3 strapping pin. The shared SPI clock line must not be pulled into an invalid boot state. |
| `GPIO9` | avoid-for-default-target | `gpio`, `pwm`, `spi_miso`, `strapping` |  | avoid-for-default-target | Seeed maps XIAO D9 to GPIO9 and identifies it with BOOT behavior. This target avoids GPIO9 for default e-paper and SD wiring. |
| `GPIO10` | used | `gpio`, `pwm`, `spi_mosi` | `spi.shared.mosi`, `display.epd.mosi`, `storage.sd.mosi` | used | Seeed maps XIAO D10 to GPIO10; the canonical DESPI-C02 wiring uses it as display SDI/DIN/MOSI and the planned SD reader shares it. |
| `GPIO18` | Truly unavailable | `usb_d-` | `usb.native` | reserved | Native USB Serial/JTAG D- for the XIAO ESP32-C3. |
| `GPIO19` | Truly unavailable | `usb_d+` | `usb.native` | reserved | Native USB Serial/JTAG D+ for the XIAO ESP32-C3. |
| `GPIO20` | Available with caveats | `gpio`, `pwm`, `uart_rx` | `console.uart0_rx` | available-with-uart-caution | Common UART0 RX mapping; native USB serial is preferable for future bring-up. |
| `GPIO21` | Available with caveats | `gpio`, `pwm`, `uart_tx` | `console.uart0_tx` | available-with-uart-caution | Common UART0 TX mapping; native USB serial is preferable for future bring-up. |

## Logical Devices

| Device | Type | GPIO / Pins | Status | Notes |
| --- | --- | --- | --- | --- |
| `display.epd` | epaper-display | cs=`GPIO3`, dc=`GPIO5`, rst=`GPIO2`, busy=`GPIO4` | historical-hardware-verified |  |
| `indicator.default` | not-present |  | metadata-confirmed | The canonical XIAO ESP32-C3 + DESPI-C02 setup has no firmware-controllable onboard LED wired to a numbered GPIO. |
| `storage.sd` | external-spi-sdcard | sck=`GPIO8`, mosi=`GPIO10`, miso=`None`, cs=`None` | planned-unverified | The SD reader is planned as direct external wiring to the shared SPI bus. SCK and MOSI share the e-paper SPI bus; MISO and CS must be filled in only after physical wiring is confirmed. |
| `usb.native` | esp32c3-native-usb-serial-jtag | d-=`GPIO18`, d+=`GPIO19` | preferred-for-this-bringup |  |
