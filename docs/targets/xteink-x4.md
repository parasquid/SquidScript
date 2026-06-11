# XTEINK X4

Generated from `targets/xteink-x4.target.json`. Do not hand-edit this file; update the target JSON and regenerate it.

## Pin Availability

| Pin | Availability | Capabilities | Used by | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| `GPIO0` | Unknown | `gpio`, `adc` | `battery.voltage` |  |  |
| `GPIO1` | Unknown | `gpio`, `adc` | `input.buttons_adc_1` |  |  |
| `GPIO2` | Unknown | `gpio`, `adc` | `input.buttons_adc_2` |  |  |
| `GPIO3` | Unknown | `gpio`, `wake` | `input.power_button` |  |  |
| `GPIO4` | Unknown | `gpio` | `display.dc` |  |  |
| `GPIO5` | Unknown | `gpio` | `display.rst` |  |  |
| `GPIO6` | Unknown | `gpio` | `display.busy` |  |  |
| `GPIO7` | Unknown | `gpio`, `spi_miso` | `storage.sd.miso` |  |  |
| `GPIO8` | Unknown | `gpio`, `spi_sck` | `bus.spi.shared.sck` |  |  |
| `GPIO10` | Unknown | `gpio`, `spi_mosi` | `bus.spi.shared.mosi` |  |  |
| `GPIO12` | Unknown | `gpio` | `storage.sd.cs` |  |  |
| `GPIO20` | Unknown | `gpio`, `uart_rx` | `power.usb_detect` |  |  |
| `GPIO21` | Unknown | `gpio` | `display.cs` |  |  |

## Logical Devices

| Device | Type | GPIO / Pins | Status | Notes |
| --- | --- | --- | --- | --- |
| `battery.voltage` | adc-voltage-divider |  |  |  |
| `display.epd` | epaper-display | cs=`GPIO21`, dc=`GPIO4`, rst=`GPIO5`, busy=`GPIO6` |  |  |
| `power.usb_detect` | gpio-usb-detect | `GPIO20` |  |  |
| `storage.sd` | spi-sdcard | cs=`GPIO12`, miso=`GPIO7` |  |  |
