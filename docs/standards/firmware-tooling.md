# Firmware Tooling Standard

XTEINK X4 is the sole supported firmware target. Use `squidc target` rather
than invoking Rust or flashing tools manually for normal work:

```bash
cargo run -p squidc -- target build --target xteink-x4
cargo run -p squidc -- target flash --target xteink-x4 --port <port>
cargo run -p squidc -- target monitor --target xteink-x4 --port <port>
cargo run -p squidc -- target doctor --target xteink-x4 --port <port>
cargo run -p squidc -- hardware test --target xteink-x4 --list
cargo run -p squidc -- hardware test --target xteink-x4 --port <port>
```

Firmware and hardware commands require host-visible toolchains and devices.
Run hardware-owning commands sequentially; never flash, monitor, or test the
same serial device concurrently. Probe the port before use rather than relying
on a remembered device name.

Build output includes the ELF and a separately generated OTA image. Flashing
uses the target-owned bootloader and partition table and writes the application
to `app0`. Diagnostics remain enabled in development builds through Rust
`debug_assertions`.

Use the full target-aware hardware inventory after runtime changes. Display
behavior requires fresh live camera evidence when visual confirmation is part
of the acceptance gate.
