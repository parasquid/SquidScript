# ESP32-C3 Super Mini

Generated from `targets/esp32c3-super-mini.target.json`. Do not hand-edit this file; update the target JSON and regenerate it.

## Pin Availability

| Pin | Availability | Capabilities | Used by | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| `GPIO0` | Free to use | `gpio`, `adc` |  | free-to-use | Commonly exposed on ESP32-C3 Super Mini headers as a general GPIO and ADC-capable pin. |
| `GPIO1` | Free to use | `gpio`, `adc` |  | free-to-use | Commonly exposed on ESP32-C3 Super Mini headers as a general GPIO and ADC-capable pin. |
| `GPIO2` | Available with caveats | `gpio`, `adc`, `strapping` |  | available-with-boot-caution | Commonly broken out on ESP32-C3 Super Mini boards, but participates in ESP32-C3 boot-mode strapping. |
| `GPIO3` | Available with caveats | `gpio`, `adc`, `strapping` |  | available-with-boot-caution | Commonly broken out on ESP32-C3 Super Mini boards, but participates in ESP32-C3 boot-mode strapping. |
| `GPIO4` | Free to use | `gpio`, `adc`, `spi_sck` |  | free-to-use | Commonly exposed as a general GPIO and often labeled as the conventional external SPI clock pin; ESP32-C3 firmware still assigns the peripheral route at runtime. |
| `GPIO5` | Free to use | `gpio`, `adc`, `spi_miso` |  | free-to-use | Commonly exposed as a general GPIO and often labeled as the conventional external SPI MISO pin; ESP32-C3 firmware still assigns the peripheral route at runtime. |
| `GPIO6` | Free to use | `gpio`, `spi_mosi` |  | free-to-use | Commonly exposed as a general GPIO and often labeled as the conventional external SPI MOSI pin; ESP32-C3 firmware still assigns the peripheral route at runtime. |
| `GPIO7` | Free to use | `gpio`, `spi_cs` |  | free-to-use | Commonly exposed as a general GPIO and often labeled as the conventional external SPI CS/SS pin; ESP32-C3 firmware still assigns the peripheral route at runtime. |
| `GPIO8` | Available with caveats | `gpio`, `strapping` | `indicator.default` | available-with-boot-caution | Common clone-board LED pin; variants may use GPIO2, GPIO7, GPIO9, or omit a controllable user LED. |
| `GPIO9` | Available with caveats | `gpio`, `strapping` | `input.boot_button` | available-with-boot-caution | Common clone-board BOOT button pin. Holding this low during reset enters download mode on typical ESP32-C3 Super Mini boards. |
| `GPIO10` | Free to use | `gpio` |  | free-to-use | Commonly exposed on ESP32-C3 Super Mini headers as a general GPIO and used by this repository's e-paper bring-up notes as a reset output candidate. |
| `GPIO11` | Truly unavailable |  |  | not-exposed | Not present in the common ESP32-C3 Super Mini header layout documented for this target. |
| `GPIO12` | Truly unavailable | `flash` | `spi_flash` | reserved | Reserved for ESP32-C3 SPI flash or internal flash-related signals; not available for app GPIO. |
| `GPIO13` | Truly unavailable | `flash` | `spi_flash` | reserved | Reserved for ESP32-C3 SPI flash or internal flash-related signals; not available for app GPIO. |
| `GPIO14` | Truly unavailable | `flash` | `spi_flash` | reserved | Reserved for ESP32-C3 SPI flash or internal flash-related signals; not available for app GPIO. |
| `GPIO15` | Truly unavailable | `flash` | `spi_flash` | reserved | Reserved for ESP32-C3 SPI flash or internal flash-related signals; not available for app GPIO. |
| `GPIO16` | Truly unavailable | `flash` | `spi_flash` | reserved | Reserved for ESP32-C3 SPI flash or internal flash-related signals; not available for app GPIO. |
| `GPIO17` | Truly unavailable | `flash` | `spi_flash` | reserved | Reserved for ESP32-C3 SPI flash or internal flash-related signals; not available for app GPIO. |
| `GPIO18` | Truly unavailable | `usb_d-` | `usb.native` | reserved | Native USB Serial/JTAG D- for this reference target; app GPIO use can break USB access. |
| `GPIO19` | Truly unavailable | `usb_d+` | `usb.native` | reserved | Native USB Serial/JTAG D+ for this reference target; app GPIO use can break USB access. |
| `GPIO20` | Available with caveats | `gpio`, `uart_rx` | `console.uart0_rx` | available-with-uart-caution | Commonly exposed as UART RX on some variants; use as GPIO only when UART bridge/debug access is not needed. |
| `GPIO21` | Available with caveats | `gpio`, `uart_tx` | `console.uart0_tx` | available-with-uart-caution | Commonly exposed as UART TX on some variants; use as GPIO only when UART bridge/debug access is not needed. |

## Logical Devices

| Device | Type | GPIO / Pins | Status | Notes |
| --- | --- | --- | --- | --- |
| `console.uart0` | uart-console | rx=`GPIO20`, tx=`GPIO21` | variant-dependent | Some ESP32-C3 Super Mini boards use native USB Serial/JTAG; some expose or bridge UART0. |
| `control.reset_button` | reset-button |  | typical | Common RST button pulls the ESP32-C3 enable/reset line and is not a normal GPIO breakout. |
| `indicator.default` | pwm-led | `GPIO8` | typical |  |
| `indicator.power_led` | power-led |  | typical | Common ESP32-C3 Super Mini power indicator LED is not controlled by a numbered GPIO. |
| `input.boot_button` | gpio-button | `GPIO9` | typical | Common BOOT button is wired to GPIO9, which is also exposed on the header and acts as a boot strapping pin. |
| `usb.native` | esp32c3-native-usb-serial-jtag | d-=`GPIO18`, d+=`GPIO19` | preferred-for-this-bringup |  |
