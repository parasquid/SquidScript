# Squid Firmware

Minimal XTEINK X4 firmware bring-up crate.

The host-testable core is intentionally separated from the ESP32-C3 hardware
entrypoint. Run TDD checks from this directory:

```sh
cargo test --target x86_64-unknown-linux-gnu
```

Build the flashable X4 image:

```sh
cargo build --release --features hardware
```

Use the repository-level helper scripts for backup, flash, and monitor.
