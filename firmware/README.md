# SquidScript Firmware

The sole firmware implementation is the native Rust workspace under
`firmware/native`. The supported hardware target is XTEINK X4, described by
`targets/xteink-x4.target.json`.

Use the target-aware CLI from the repository root:

```bash
cargo run -p squidc -- target build --target xteink-x4
cargo run -p squidc -- target flash --target xteink-x4
cargo run -p squidc -- target monitor --target xteink-x4
cargo run -p squidc -- hardware test --target xteink-x4 --list
```

The target definition supplies the Rust package, compilation target, firmware
features, partition table, bootloader, ELF, and OTA image paths. Firmware code
uses `squidvm-core` directly and implements target services through the native
runtime and ESP32-C3 hardware bindings.

Development builds retain diagnostics through Rust `debug_assertions`.
Release builds compile that instrumentation out.
