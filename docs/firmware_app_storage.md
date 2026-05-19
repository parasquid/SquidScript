# Firmware App Storage

Status: ESP32-C3 Super Mini reference firmware implementation note.

The reference firmware stores installed SquidScript bytecode in firmware-owned
app storage. The in-memory app registry is only a startup cache and runtime
index over that storage.

## Logical Model

Host tooling installs a compiled SQBC payload with:

```text
INSTALL.APP <app-id> <len> <fnv32hex>
```

Firmware validates the app id, byte count, FNV-1a hash, and SQBC structure
before publishing the app in the registry. A successful install writes the SQBC
payload to persistent app storage and then updates the in-memory registry
entry.

On firmware startup, the registry scans persistent app storage and rebuilds the
cache from valid `.sqbc` files. Invalid, oversized, or unreadable app records
are not treated as compiled SquidScript source.

## ESP32-C3 Super Mini Layout

The current Super Mini firmware uses LittleFS on a dedicated 4 MB flash region:

```text
factory app: 0x10000..0x20ffff
squidfs:     0x210000..0x3fffff
```

The `squidfs` partition stores apps under:

```text
/apps/<app-id>.sqbc
```

Normal firmware flashing writes the bootloader, partition table, and factory
app partition. It does not erase `squidfs`; use `STORAGE.FORMAT` when a clean
app store is required.

## Developer Commands

```text
APP.LIST
STORAGE.FORMAT
```

`APP.LIST` reports the registry cache rebuilt from storage at startup plus apps
installed during the current session.

`STORAGE.FORMAT` formats the firmware-owned app store, clears the registry
cache, clears runtime traces/timers, and unloads the current VM. It is a
developer command, not a SquidScript language API.

## Host Toolchain

The LittleFS dependency includes C sources, so the ESP32-C3 firmware build
needs a RISC-V ELF GCC toolchain in addition to the Rust
`riscv32imc-unknown-none-elf` target. `scripts/c3-supermini-build.sh` accepts:

- `riscv32-unknown-elf-gcc`
- `riscv64-elf-gcc` with `-march=rv32imc -mabi=ilp32`

Run `cargo run -p squidc -- doctor` to check the host setup.

If Homebrew installs `riscv64-elf-gcc` but does not link it into `PATH`, either
leave `brew` available so the build script can resolve `brew --prefix
riscv64-elf-gcc`, fix the Homebrew link, or export
`CC_riscv32imc_unknown_none_elf` to the compiler path for the current shell. Do
not commit machine-specific Cellar paths into the build script.
