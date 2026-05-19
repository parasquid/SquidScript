# squid-firmware

Rust reference firmware for exercising SquidScript language semantics.

The host-testable core is intentionally separated from ESP32-C3 hardware
entrypoints. Run TDD checks from this directory:

```sh
cargo test --target x86_64-unknown-linux-gnu
```

Build the ESP32-C3 Super Mini reference firmware:

```sh
cargo build --release --features hardware --bin c3-supermini-serial-hello
```

From the repository root, the preferred end-to-end hardware check is:

```sh
scripts/c3-supermini-smoke.sh
```

Use `--skip-flash` to keep the current firmware image and repeat only the SQBC
install/run/key/state/trace verification.

On Linux, `/dev/ttyACM0` is commonly owned by `root:dialout`. If the board is
visible but flashing cannot open the port, grant temporary access to the current
user:

```sh
sudo setfacl -m u:$USER:rw /dev/ttyACM0
```

That workaround is reset when the device node is recreated. For the persistent
fix, add the user to `dialout` and use a fresh login session:

```sh
sudo usermod -aG dialout $USER
```

The Super Mini firmware exposes a USB Serial/JTAG shell:

```text
help
info
install <len> <fnv32hex>
run
key SELECT
key BACK
state
trace
errors
reset
```

`install` receives raw SQBC v2 bytes into RAM. State persistence is in-memory
only for this milestone; `state.load` and `state.save` are traced but do not
write flash yet.

The crate still contains `x4-hello`, a separate XTEINK X4 display bring-up
binary. Keep X4 display work separate from the reference VM milestone.
