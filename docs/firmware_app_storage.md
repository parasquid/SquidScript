# Firmware App Storage

Status: ESP32-C3 Super Mini reference firmware implementation note.

The reference firmware stores installed SquidScript bytecode in firmware-owned
app storage. The in-memory app registry is a metadata-only startup cache and
runtime index over that storage.

## Logical Model

Host tooling installs a compiled SQBC payload with:

```text
INSTALL.APP <app-id> <len> <fnv32hex>
```

Firmware validates the app id, byte count, FNV-1a hash, and SQBC structure
before publishing the app in the registry. A successful install writes the SQBC
payload to persistent app storage and then updates the in-memory registry
metadata entry.

On firmware startup, the registry scans persistent app storage and rebuilds
metadata from valid `.sqbc` files. App bodies are not mirrored per registry
slot in RAM. Startup validation reads the SQBC v3 header and section table with
bounded storage reads. The current VM still parses the active app from a
contiguous SQBC byte slice when launching it; replacing that execution path with
handler/function/screen chunk reads remains the active loader milestone.

The current ESP32-C3 reference firmware keeps `RUN.TEMP` RAM-backed. This is a
pre-1.0 developer workflow decision: repeated `squidc run` iterations should
avoid flash writes, even though the temp path reserves RAM.

Measured on the ESP32-C3 Super Mini reference firmware after adding resource
introspection and reducing `MAX_APP_BYTES` to 4 KiB:

```text
.data  1164
.bss   4976
.stack 311416
RESOURCES.GET memory_available_bytes=311416
```

Installed-app SQBC v3 execution should keep LittleFS as the app storage
abstraction but read an app index first, then load executable
handler/function/screen chunks on demand. Handler chunks are cache entries, not
app sessions. Active chunks cannot be evicted; inactive chunks may be evicted;
`@preload` chunks are preferred but not guaranteed. Chunk eviction does not fire
`event.on("app.exit")` or any other SquidScript lifecycle event.

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
RESOURCES.GET
STORAGE.FORMAT
```

`APP.LIST` reports the metadata cache rebuilt from storage at startup plus apps
installed during the current session.

`RESOURCES.GET` reports raw target-specific byte counts for runtime memory and
firmware app storage. App-storage availability is currently calculated from the
LittleFS partition size minus installed app byte sizes, so filesystem metadata
and wear-leveling overhead are not exposed as a precise free-block count yet.

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
