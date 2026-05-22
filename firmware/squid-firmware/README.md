# Obsolete Rust Firmware

This tree is obsolete reference material from the pre-Zephyr ESP32-C3 firmware
prototype. It is retained only so behavior can be inspected while the Zephyr
host reaches feature parity.

Do not add new firmware behavior here. Current real firmware work belongs under
`firmware/zephyr`, with VM semantics shared through `compiler/rust/crates/squidvm-ffi`.

The old build, flash, serial protocol, storage layout, and hardware tests are
not compatibility contracts. If current ESP32-C3 behavior fails under Zephyr,
treat it as a Zephyr implementation, driver, configuration, or workaround task.
