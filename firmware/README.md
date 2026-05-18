# Squid Firmware Placeholder

This directory is reserved for the SquidScript reference firmware, provisionally called `squid-firmware`.

The initial reference target is XTEINK X4 on ESP32-C3. The planned implementation direction is Rust-first, using the current embedded ESP Rust stack where practical:

- `esp-hal` for hardware access
- Embassy and `esp-hal-embassy` for async execution
- `esp-radio` when Wi-Fi or BLE support is introduced

ESP-IDF remains a candidate backend if the Rust ESP stack cannot satisfy a required hardware, storage, radio, or maintenance need.

The first implementation milestone is a boot console available over both serial and the EPD. SquidVM, SD app loading, launcher behavior, and BinBook rendering should build on that after hardware bring-up is reliable.

No firmware code is checked in yet.
