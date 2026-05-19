# Squid Firmware

This directory contains the SquidScript reference firmware, provisionally called `squid-firmware`.

The initial reference target is XTEINK X4 on ESP32-C3. The planned implementation direction is Rust-first, using the current embedded ESP Rust stack where practical:

- `esp-hal` for hardware access
- Embassy and `esp-hal-embassy` for async execution
- `esp-radio` when Wi-Fi or BLE support is introduced

ESP-IDF remains a candidate backend if the Rust ESP stack cannot satisfy a required hardware, storage, radio, or maintenance need.

The first implementation milestone is a boot console available over both serial and the EPD. SquidVM, SD app loading, launcher behavior, and BinBook rendering should build on that after hardware bring-up is reliable.

Current status:

- `squid-firmware` builds a minimal XTEINK X4 hello-world image.
- Host unit tests drive the bring-up sequence with a mock display.
- The hardware binary prints a serial banner and refreshes the SSD1677 e-paper panel with a diagnostic screen.
- SquidScript execution, SD app loading, launcher behavior, and BinBook rendering are not implemented yet.

Useful commands from the repository root:

```sh
scripts/x4-firmware-build.sh
scripts/x4-firmware-backup.sh
scripts/x4-firmware-flash.sh
scripts/x4-firmware-monitor.sh
```
