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

For `.squid.zip` packages, host tooling validates and unpacks the ZIP first.
Firmware is not required to parse ZIP archives directly. The host installs
`main.sqbc` with `INSTALL.APP` and then streams each normalized package resource
with:

```text
INSTALL.RESOURCE <app-id> <package-relative-path> <len> <fnv32hex>
```

Firmware validates the app id, byte count, FNV-1a hash, and SQBC structure
before publishing the app in the registry. A successful install writes the SQBC
payload to persistent app storage and then updates the in-memory registry
metadata entry.

On firmware startup, the registry scans persistent app directories and rebuilds
metadata from valid `main.sqbc` files. Installed package resources live beside
`main.sqbc` under the same app directory and are read-only to apps. App bodies
are not mirrored per registry slot in RAM. Startup validation reads the SQBC
header and section table with bounded storage reads. If an installed `main` app
is present and valid, firmware boots it as the root foreground app and
dispatches `event.on("app.start")`. If `main` is missing or invalid, firmware
stays in dev shell mode and reports the boot status over serial.

The current ESP32-C3 reference firmware keeps `RUN.TEMP` RAM-backed. This is a
pre-1.0 developer workflow decision: repeated `squidc run` iterations should
avoid flash writes, even though the temp path reserves RAM.

App state is a separate logical storage model from installed app bytecode.
Scripts declare typed state slots in SQBC metadata; firmware persists only
those declared primitive values. It does not persist the VM stack, current
screen, timers, armed triggers, or handles. Apps that need to resume a timer
or workflow should persist their own intent, for example a nullable
`retryAt: int? = null` slot.

On the ESP32-C3 internal LittleFS store, installed-app state uses a small
binary record owned by firmware under `/state/<app-id>.state`. Initial limits
are intentionally small:

- 16 state slots per app
- 512 saved bytes per app
- 48 bytes per saved string

`RUN.TEMP` state is volatile and RAM-backed. `state.load()`, `state.save()`,
and `state.reset()` for temp apps must not write flash.

The binary record starts with `SQST` and a field count. Each
field stores the state name, declared primitive type, nullable flag, and value.
`state.load()` applies matching slots by name and declared type, leaves missing
slots at their current/default values, ignores removed slots, and fails with a
VM error for malformed records or matching-name type/value mismatches.

For future removable or human-readable state exchange, use SQSTATE text lines
rather than JSON. The format keeps the compiled declared type visible and
remains straightforward for MCU parsing:

```text
SQSTATE 1
stateVersion int 2
count int 42
enabled bool true
title string 5:Hello
selectedBook string? null
retryAt int? 0
```

Each SQSTATE text record is `<name> <declared-type> <value>`. Strings are
length-prefixed. Scripts should use an app-level `stateVersion: int` slot and
call `state.reset()` when the stored shape no longer matches what the new
script expects.

## Device Config Records

SQDEVICE is the human-authorable text resource format for device service
configuration. SQDC is the compact firmware-owned binary persistence format for
the active device config. Apps may package `.sqdevice` resources and bind them
through top-level `device {}` declarations, but package install stores those
resources read-only and does not activate them by itself.

SQDEVICE text starts with:

```text
SQDEVICE
```

Blank lines and `#` line comments are allowed. Each record is:

```text
<key> <type> <value>
```

Strings use the same length-prefixed style as SQSTATE text:

```text
backend string 7:ssd1677
width int 800
spi.sck string 5:GPIO4
spi.miso null
```

Initial SQDEVICE value types:

- `string <byte-len>:<utf8-bytes>`
- `int <signed-32-bit-decimal>`
- `bool true`
- `bool false`
- `null`

Duplicate keys are import errors. Unknown keys are retained so future or
unused driver sections can round-trip. Firmware hard-fails unknown GPIO names
and missing required fields when a binding is initialized. Duplicate or
conflicting known pins should return a bounded warning string, not a compiler
error. The current draft does not define strapping-pin or electrical-hazard
policy checks.

SQDC uses repo-owned binary typed records rather than CBOR or JSON. It should
share bounded typed-record parser helpers with SQST where practical:

```text
offset  size  field
0       4     magic: "SQDC"
4       2     little-endian u16 record count
6       n     typed records
```

Each SQDC typed record stores a key string and one typed primitive value. Exact
record packing is implementation-defined until the parser lands, but the
format must remain bounded, duplicate-key rejecting, and MCU-parseable without
heap-heavy generic serialization.

`device.config.save("flash")` persists SQDC as the global active device config.
`RUN.TEMP` and other temp-run workflows may import or rebind config in RAM, but
must not persist those changes unless code explicitly saves.

Measured on the ESP32-C3 Super Mini reference firmware after adding resource
introspection and reducing `MAX_APP_BYTES` to 4 KiB:

```text
.data  1164
.bss   4976
.stack 311416
RESOURCES.GET memory_available_bytes=311416
RESOURCES.GET temp_app_buffer_bytes=0
RESOURCES.GET installed_code_cache_bytes=1024
```

Installed-app SQBC execution keeps LittleFS as the app storage abstraction.
Firmware reads an owned app index first, then loads executable
handler/function/screen chunks on demand from the code section. Handler chunks
are cache entries, not app sessions. Active chunks cannot be evicted; inactive
chunks may be evicted; `@preload` chunks are preferred but not guaranteed.
Chunk eviction does not fire `event.on("app.exit")` or any other SquidScript
lifecycle event. `RUN.TEMP` remains the only full-buffer execution path.

## ESP32-C3 Super Mini Layout

The current Super Mini firmware uses LittleFS on a dedicated 4 MB flash region:

```text
factory app: 0x10000..0x20ffff
squidfs:     0x210000..0x3fffff
```

The `squidfs` partition stores apps under:

```text
/apps/<app-id>/main.sqbc
/apps/<app-id>/<package-resource>
/state/<app-id>.state
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
It also reports temp app buffer usage separately from installed-app code cache
usage so RAM comparisons distinguish volatile developer runs from persistent
chunk execution. Total `.bss` may not drop much while `RUN.TEMP` remains
RAM-backed.

`STORAGE.FORMAT` formats the firmware-owned app store, clears the registry
cache, clears runtime traces/timers, unloads the current VM, and leaves
firmware in dev shell mode because root `main` no longer exists. It is a
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
