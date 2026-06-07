# SquidScript Target Profile Architecture

Status: Draft
Purpose: Define how SquidScript firmware, bytecode, apps, and compiler tooling support multiple hardware targets such as XTEINK X4 and ESP32-S3 + Waveshare e-ink development boards.

---

## 1. Goal

The system should support multiple hardware targets without hardcoding every device-specific difference into app code or scattering `#ifdef`s throughout the firmware.

Examples:

- XTEINK X4 production target
- ESP32-S3 development board + Waveshare e-paper display
- ESP32-S3 development board + different input peripheral
- future ESP32-C3 or ESP32-S3 e-ink devices

A target profile should describe what the device provides.

SquidScript apps should target capabilities where possible, not specific boards.

---

## 2. Core Concept

A target definition describes what one concrete firmware build target provides.

For integrated production devices, the canonical form is a single target JSON
file with sections for board hardware, display, input, storage, power, runtime
limits, and features.

Example:

```text
targets/xteink-x4.target.json
```

The detailed schema reference is maintained in:

```text
docs/target_definition_reference.md
```

Firmware build orchestration, backend selection, and simulator backend policy are described in:

```text
docs/firmware_build_architecture.md
```

Portable runtime scheduling, service actors, and RTOS backend mapping are
described in:

```text
docs/portable_rtos_kernel_architecture.md
```

Split component profiles are still useful for reusable development-board combinations, but they are an advanced composition mode rather than the default shape for integrated devices such as XTEINK X4.

Conceptually, every target still resolves to the same categories:

```text
Resolved target =
  board hardware
+ display behavior
+ input behavior
+ storage behavior
+ power behavior
+ runtime limits
+ optional simulator layout metadata
```

Every standalone target or profile JSON object must declare a `format` string
and an `id`.

The `format` value identifies the schema family. Tools must reject unknown
schema families rather than guessing.

Examples:

- `squid-target-board`
- `squid-target-display`
- `squid-target-input`
- `squid-target-storage`
- `squid-target-power`
- `squid-target-runtime`
- `squid-target-v1`

Split composition example:

```yaml
dev-esp32s3-waveshare-7in5:
  board: esp32s3-devkit
  display: waveshare-7in5-v2
  input: dev-buttons-7key
  storage: spi-sdcard-dev
  power: usb-dev-power
  runtime: esp32s3-psram
```

---

## 3. Profile Categories

### 3.1 Board Profile

The board profile describes the MCU and board-level hardware.

Examples:

- MCU type
- CPU frequency
- internal SRAM
- PSRAM
- flash size
- firmware update mechanism
- available buses
- GPIO assignments
- onboard peripherals

Example board profile:

```json
{
  "format": "squid-target-board",
  "id": "esp32s3-devkit",
  "mcu": "esp32s3",
  "cpu": {
    "cores": 2,
    "frequencyMHz": 240
  },
  "memory": {
    "internalSramKB": 512,
    "psramKB": 8192
  },
  "flash": {
    "sizeMB": 16
  },
  "firmwareUpdate": {
    "formats": ["uf2", "esp-idf-bin"],
    "preferredFormat": "uf2",
    "uf2FamilyId": "ESP32S3",
    "userReplacement": "usb-mass-storage"
  },
  "buses": {
    "spi": ["spi2", "spi3"],
    "i2c": ["i2c0"],
    "uart": ["uart0"]
  }
}
```

`firmwareUpdate.formats` lists the firmware image formats that the board and selected bootloader support. `preferredFormat` should be `uf2` for boards intended to be user-serviceable through drag-and-drop replacement. `uf2FamilyId` identifies the UF2 target family used by the conversion tool and bootloader validation. `userReplacement` describes the primary user-facing replacement path; `usb-mass-storage` means the device can enter an update mode where the host computer sees a removable drive and the user copies a `.uf2` file onto it.

Boards that cannot support UF2 should omit `uf2` from `firmwareUpdate.formats` and must still provide the native flashing artifacts required by their MCU/toolchain.

---

### 3.2 Display Profile

The display profile describes display behavior independently from the board.

It should include:

- display driver
- bus or transport binding
- physical width/height
- logical width/height
- rotation
- supported bit depths
- supported pixel formats
- logical grayscale levels exposed to SquidScript
- color mapping and dithering behavior
- text font-height support and default text height
- default bit depth
- default pixel format
- partial refresh support
- fast refresh support
- display render mode support and default initialization mode
- app-visible screen render policy support and default policy

Illustrative split-profile XTEINK-style display fragment:

This is not the canonical XTEINK X4 hardware source. Use `targets/xteink-x4.target.json` for verified XTEINK X4 pins and display wiring.

```json
{
  "format": "squid-target-display",
  "id": "xteink-x4-display",
  "driver": "xteink_x4_epd",
  "bus": "spi2",
  "physical": {
    "width": 800,
    "height": 480
  },
  "logical": {
    "width": 480,
    "height": 800,
    "rotation": 90
  },
  "color": {
    "logicalGrayscaleLevels": 16,
    "supportedBpp": [1, 2],
    "defaultBpp": 2,
    "supportedPixelFormats": ["GRAY1_PACKED", "GRAY2_PACKED"],
    "defaultPixelFormat": "GRAY2_PACKED",
    "mapping": "nearest-or-dither",
    "dithering": ["none", "ordered", "error-diffusion"]
  },
  "text": {
    "fontHeights": {
      "supported": [16, 18, 20, 24, 32, 48],
      "default": 20,
      "selection": "nearest"
    }
  },
  "refresh": {
    "full": true,
    "partial": true,
    "fast": false
  },
  "rendering": {
    "screenPolicies": ["compose", "stream"],
    "defaultPolicy": "compose",
    "policyModeMap": {
      "compose": ["single", "strip"],
      "stream": ["strip", "single"]
    },
    "supportedModes": ["strip", "single"],
    "defaultMode": "strip",
    "stripBufferBytes": 4096,
    "singleBufferBytes1bpp": 48000,
    "singleBufferBytes2bpp": 96000,
    "maxFullBufferBpp": 2
  }
}
```

Example Waveshare display profile:

```json
{
  "format": "squid-target-display",
  "id": "waveshare-7in5-v2",
  "driver": "waveshare_epd_7in5_v2",
  "bus": "spi2",
  "physical": {
    "width": 800,
    "height": 480
  },
  "logical": {
    "width": 800,
    "height": 480,
    "rotation": 0
  },
  "color": {
    "logicalGrayscaleLevels": 16,
    "supportedBpp": [1],
    "defaultBpp": 1,
    "supportedPixelFormats": ["GRAY1_PACKED"],
    "defaultPixelFormat": "GRAY1_PACKED",
    "mapping": "nearest-or-dither",
    "dithering": ["none", "ordered"]
  },
  "text": {
    "fontHeights": {
      "supported": [16, 20, 24, 32],
      "default": 20,
      "selection": "nearest"
    }
  },
  "refresh": {
    "full": true,
    "partial": false,
    "fast": false
  },
  "rendering": {
    "screenPolicies": ["compose", "stream"],
    "defaultPolicy": "compose",
    "policyModeMap": {
      "compose": ["single", "strip"],
      "stream": ["strip", "single"]
    },
    "supportedModes": ["strip", "single"],
    "defaultMode": "strip",
    "maxFullBufferBpp": 1
  }
}
```

Display rendering has two related but separate concepts:

- Screen render policy is app-visible SquidScript intent, such as `compose` or `stream`.
- Display render mode is firmware-visible implementation, such as `single` or `strip`.

If a SquidScript screen omits `render`, firmware uses the target's `rendering.defaultPolicy`. A low-RAM target such as XTEINK X4 may default to `compose` for ordinary app screens while still initializing the display service with a `strip` default mode. `compose` means normal UI composition; `stream` means page- or image-dominant rendering. `single` means firmware keeps one full framebuffer in RAM. `strip` means firmware fills a bounded strip buffer and transfers that strip to the EPD.

Firmware should use `policyModeMap` as a preference order. For example, `compose` may prefer `single` when enough memory is available and fall back to `strip` when equivalent output can be preserved. `stream` should prefer `strip` for large drawables and may use `single` when memory is available or when a backend cannot stream efficiently.

SquidScript exposes a logical grayscale palette rather than native panel color
levels. The current language uses 16 grayscale levels, `gray0` through
`gray15`, where `gray0` is white and `gray15` is black. Firmware maps those
logical values to the target display's native pixel format. If the native
display has fewer levels than the logical palette, firmware may use the
target's declared dithering modes to approximate intermediate grays.

SquidScript text uses `fontHeight` in logical pixels. Targets declare the supported font heights, the default font height used when a text call omits `fontHeight`, and whether unsupported requested heights are rejected or mapped to a supported height. The initial XTEINK X4 policy uses nearest supported height selection.

---

### 3.3 Input Profile

The input profile maps physical inputs to logical keys.

SquidScript apps should see logical keys, not GPIOs.

Logical keys:

- `UP`
- `DOWN`
- `LEFT`
- `RIGHT`
- `SELECT`
- `BACK`
- `MENU`
- `HOME`
- `POWER`

Input profiles may also define long-press behavior and key combinations for logical keys. Long press is threshold-triggered: firmware fires the long-press event or system action when the key has been held for the configured duration, without waiting for release. This can apply to GPIO buttons, matrix keys, and ADC ladder buttons when the input driver can report stable press/release state.

Key combinations, or chords, are also target-defined. A chord such as `POWER+DOWN` should be emitted only when the input hardware can detect both logical keys reliably within the configured timing window.

Example input profile:

```json
{
  "format": "squid-target-input",
  "id": "dev-buttons-7key",
  "type": "gpio-buttons",
  "buttons": [
    { "logical": "UP", "gpio": 1, "activeLow": true },
    { "logical": "DOWN", "gpio": 2, "activeLow": true },
    { "logical": "LEFT", "gpio": 3, "activeLow": true },
    { "logical": "RIGHT", "gpio": 4, "activeLow": true },
    { "logical": "SELECT", "gpio": 5, "activeLow": true },
    { "logical": "BACK", "gpio": 6, "activeLow": true },
    { "logical": "POWER", "gpio": 7, "activeLow": true }
  ],
  "longPress": [
    {
      "logical": "POWER",
      "durationMs": 2000,
      "trigger": "threshold",
      "owner": "system",
      "action": "sleep",
      "suppressShortKey": true
    }
  ],
  "chords": [
    {
      "logical": ["POWER", "DOWN"],
      "name": "force-refresh",
      "owner": "system",
      "action": "refresh-display",
      "windowMs": 120,
      "suppressComponentKeys": true
    }
  ]
}
```

If a target lacks a required logical key, target validation should fail before
the app is launched.

---

### 3.4 Storage Profile

The storage profile describes available storage.

Examples:

- SD card over SPI
- SDMMC
- internal flash only
- mount point
- maximum file read size
- whether app loading from SD is supported

Example:

```json
{
  "format": "squid-target-storage",
  "id": "spi-sdcard-dev",
  "type": "sdcard",
  "bus": "spi3",
  "mount": "/sd",
  "supportsApps": true,
  "supportsFile": true,
  "maxFileReadSize": 65536
}
```

---

### 3.5 Power Profile

The power profile describes sleep and power behavior.

Examples:

- USB-powered development board
- battery-powered device
- deep sleep support
- wake buttons
- whether SD should be unmounted before sleep
- whether display should be put into sleep mode

Example:

```json
{
  "format": "squid-target-power",
  "id": "usb-dev-power",
  "type": "usb",
  "battery": false,
  "deepSleep": false,
  "wakeKeys": [],
  "sleepPolicy": {
    "displaySleep": true,
    "syncStorageBeforeSleep": true
  }
}
```

Illustrative split-profile battery target fragment:

This shows the older component-profile shape. Use `targets/xteink-x4.target.json` for the canonical XTEINK X4 power and battery metadata.

```json
{
  "format": "squid-target-power",
  "id": "xteink-x4-power",
  "type": "battery",
  "battery": true,
  "deepSleep": true,
  "wakeKeys": ["POWER"],
  "sleepPolicy": {
    "displaySleep": true,
    "syncStorageBeforeSleep": true,
    "saveAppStateBeforeSleep": true
  }
}
```

---

### 3.6 Runtime Profile

The runtime profile describes SquidScript VM limits and enabled capabilities.

Example ESP32-C3 low-RAM runtime:

```json
{
  "format": "squid-target-runtime",
  "id": "esp32c3-lowram",
  "squidscript": {
    "maxBytecodeSize": 32768,
    "maxStateVariables": 64,
    "maxSerializedStateSize": 8192,
    "maxStringLength": 1024,
    "maxFunctionCount": 64,
    "maxFunctionParameters": 8,
    "maxLocalVariablesPerFunction": 32,
    "maxCallDepth": 8,
    "maxInstructionsPerEvent": 2000,
    "maxLoopIterationsPerEvent": 100,
    "maxScreenDrawCommands": 128,
    "maxFileReadSize": 65536,
    "maxListItemsReturned": 256,
    "maxHandles": 16
  },
  "features": [
    "squidscript.bytecode",
    "service.display.draw",
    "state.read",
    "state.write",
    "file.pick",
    "file.read",
    "service.wifi.connect",
    "service.wifi.scan",
    "service.wifi.accessPoint",
    "service.ble.file-transfer"
  ]
}
```

Example ESP32-S3 PSRAM runtime:

```json
{
  "format": "squid-target-runtime",
  "id": "esp32s3-psram",
  "squidscript": {
    "maxBytecodeSize": 131072,
    "maxStateVariables": 256,
    "maxSerializedStateSize": 32768,
    "maxStringLength": 4096,
    "maxFunctionCount": 256,
    "maxFunctionParameters": 16,
    "maxLocalVariablesPerFunction": 64,
    "maxCallDepth": 16,
    "maxInstructionsPerEvent": 10000,
    "maxLoopIterationsPerEvent": 1000,
    "maxScreenDrawCommands": 512,
    "maxFileReadSize": 262144,
    "maxListItemsReturned": 1024,
    "maxHandles": 64
  },
  "features": [
    "squidscript.bytecode",
    "service.display.draw",
    "state.read",
    "state.write",
    "file.pick",
    "file.read",
    "service.wifi.connect",
    "service.wifi.scan",
    "service.wifi.accessPoint",
    "service.ble.file-transfer",
    "debug-ui"
  ]
}
```

---

## 4. Target Profile

A target profile describes one concrete build target.

For integrated devices, use one complete target file. The full XTEINK X4 reference target is maintained at:

```text
targets/xteink-x4.target.json
```

Trimmed XTEINK X4 shape:

```json
{
  "format": "squid-target-v1",
  "id": "xteink-x4",
  "name": "XTEINK X4",
  "mcu": {
    "part": "ESP32-C3",
    "family": "ESP32C3"
  },
  "pins": {},
  "buses": {},
  "devices": {},
  "display": {},
  "input": {},
  "storage": {},
  "power": {},
  "runtime": {},
  "features": [
    "sdcard",
    "buttons",
    "squidscript.bytecode"
  ]
}
```

Split composition remains useful for development targets that intentionally recombine reusable parts.

Example ESP32-S3 + Waveshare development target using split profile parts:

```json
{
  "format": "squid-target-v1",
  "id": "esp32s3-waveshare-7in5",
  "name": "ESP32-S3 DevKit + Waveshare 7.5in e-Paper",
  "board": "esp32s3-devkit",
  "display": "waveshare-7in5-v2",
  "input": "dev-buttons-7key",
  "storage": "spi-sdcard-dev",
  "power": "usb-dev-power",
  "runtime": "esp32s3-psram",
  "features": [
    "sdcard",
    "buttons",
    "usb-serial",
    "squidscript.bytecode",
    "debug-ui"
  ]
}
```

SquidScript bytecode is the executable bytecode format for SquidScript apps;
runtime support for loading and executing it is represented by
`squidscript.bytecode`.

---

## 5. Recommended Directory Layout

For the SquidScript repository, target source artifacts should live under:

```text
targets/
`-- xteink-x4.target.json
```

Firmware implementations may copy or vendor those target definitions, or consume them directly from this repository when build tooling allows it.

For firmware source trees, the recommended layout is:

```text
firmware/
|-- README.md
|-- targets/
|   |-- xteink-x4.target.json
|   `-- esp32s3-waveshare-7in5.target.json
|-- profile-parts/          # optional, for reusable dev-board composition
|   |-- boards/
|   |-- displays/
|   |-- inputs/
|   |-- storage/
|   |-- power/
|   `-- runtime-profiles/
|-- zephyr/                  # Zephyr app, CMake, Kconfig, devicetree overlays
`-- generated/
    |-- target_config.h      # generated target constants for firmware
    |-- app_capabilities.h
    `-- firmware_manifest.json
```

Integrated production targets should prefer one complete `*.target.json` file. Optional `profile-parts/` files should be introduced only when a component is genuinely reused across several targets.

The first real firmware implementation is Zephyr-backed. Rust remains the VM
semantics implementation through `squidvm-ffi`; Zephyr owns the target host,
drivers, storage, scheduler, and diagnostics.

---

## 6. Firmware Build-Time Target Selection

The firmware build should select a concrete target.

Example with the Zephyr backend:

```sh
cargo run -p squidc -- target build --target esp32c3-super-mini
```

Example target metadata:

```sh
make target=xteink-x4 build
make target=esp32s3-waveshare-7in5 build
```

The build system should load the target profile and generate backend-facing
target constants, such as Zephyr C headers, devicetree overlays, or Kconfig
fragments.

For production and developer handoff builds, the build should also produce a UF2 firmware image whenever the selected board profile includes `uf2` in `firmwareUpdate.formats`.

Expected firmware artifacts:

```text
build/
|-- firmware.bin          # native toolchain image or merged flash image
|-- firmware.uf2          # user-replaceable image when supported by target
|-- firmware.manifest.json
`-- target_config.h       # generated Zephyr-facing target constants
```

`firmware.uf2` is the preferred artifact for non-developer replacement flows. Users should be able to place the device in update mode, copy the UF2 file to the exposed USB mass-storage volume, and let the bootloader install it. The native `.bin` artifacts remain required for factory flashing, CI validation, recovery over serial/JTAG, and boards without UF2 support.

`firmware.manifest.json` should record at least the target ID, source revision,
native image hash, UF2 image hash when present, UF2 family ID when present,
build time, and diagnostic build ID. This manifest is a distribution and
diagnostics artifact; it is not loaded by SquidScript apps.

Example generated header:

```c
#define DEVICE_TARGET_ID "xteink-x4"
#define DEVICE_MCU_ESP32C3 1
#define FIRMWARE_UPDATE_UF2 0
/* UF2 support is desired but deferred for XTEINK X4 until the bootloader/update path is verified. */

#define DISPLAY_DRIVER_XTEINK_X4 1
#define DISPLAY_LOGICAL_WIDTH 480
#define DISPLAY_LOGICAL_HEIGHT 800
#define DISPLAY_PHYSICAL_WIDTH 800
#define DISPLAY_PHYSICAL_HEIGHT 480
#define DISPLAY_ROTATION 90
#define DISPLAY_DEFAULT_BPP 2
#define DISPLAY_PIXEL_FORMAT_GRAY2_PACKED 1
#define DISPLAY_DEFAULT_PIXEL_FORMAT DISPLAY_PIXEL_FORMAT_GRAY2_PACKED
#define DISPLAY_RENDER_MODE_STRIP 1
#define DISPLAY_RENDER_MODE_SINGLE 2
#define DISPLAY_DEFAULT_RENDER_MODE DISPLAY_RENDER_MODE_STRIP
#define DISPLAY_STRIP_BUFFER_BYTES 4096
#define DISPLAY_SINGLE_BUFFER_BYTES_1BPP 48000
#define DISPLAY_SINGLE_BUFFER_BYTES_2BPP 96000
#define DISPLAY_SUPPORTS_PARTIAL_REFRESH 1

#define SQUIDSCRIPT_VERSION_MAJOR 0
#define SQUIDSCRIPT_VERSION_MINOR 2
#define SQUID_MAX_BYTECODE_SIZE 32768
#define SQUID_MAX_STATE_VARIABLES 64
#define SQUID_MAX_SERIALIZED_STATE_SIZE 8192
#define SQUID_MAX_STRING_LENGTH 1024
#define SQUID_MAX_FUNCTION_COUNT 64
#define SQUID_MAX_CALL_DEPTH 8
#define SQUID_MAX_INSTRUCTIONS_PER_EVENT 2000
#define SQUID_MAX_SCREEN_DRAW_COMMANDS 128
#define SQUID_MAX_HANDLES 16
```

The firmware should use interfaces and driver registration instead of device-specific logic spread throughout app/runtime code.

---

## 7. HAL Interfaces

The firmware should expose hardware through common interfaces.

The examples below are interface sketches. Zephyr firmware should express these
as C-facing service/driver interfaces backed by Zephyr subsystems and
target-specific devicetree/Kconfig.

### 7.1 Display Interface

```rust
enum PixelFormat {
    Gray1Packed,
    Gray2Packed,
}

enum DisplayRenderMode {
    Strip,
    Single,
}

enum ScreenRenderPolicy {
    Compose,
    Stream,
}

struct DisplayInfo {
    logical_width: u16,
    logical_height: u16,
    physical_width: u16,
    physical_height: u16,
    rotation: u8,
    default_bpp: u8,
    default_pixel_format: PixelFormat,
    supported_render_modes: &'static [DisplayRenderMode],
    default_render_mode: DisplayRenderMode,
    supported_screen_policies: &'static [ScreenRenderPolicy],
    default_screen_policy: ScreenRenderPolicy,
    partial_refresh: bool,
    fast_refresh: bool,
}

trait DisplayDriver {
    fn init(&mut self, mode: DisplayRenderMode) -> Result<(), DisplayError>;
    fn info(&self) -> DisplayInfo;
    fn clear(&mut self, color: u8) -> Result<(), DisplayError>;
    fn draw_tile(&mut self, x: i32, y: i32, w: u16, h: u16, pixels: &[u8], bpp: u8) -> Result<(), DisplayError>;
    fn refresh_full(&mut self) -> Result<(), DisplayError>;
    fn refresh_partial(&mut self, x: i32, y: i32, w: u16, h: u16) -> Result<(), DisplayError>;
    fn sleep(&mut self);
}
```

`ScreenRenderPolicy` values such as `Compose` and `Stream` come from SquidScript screen declarations. `DisplayRenderMode::Strip` and `DisplayRenderMode::Single` are display initialization choices. The display service receives logical SquidScript draw operations and renders them through the selected mode. `Strip` uses a bounded strip buffer and may replay draw operations per strip; `Single` keeps one full framebuffer and transfers it after composition.

### 7.2 Input Interface

```c
typedef enum {
    KEY_UP,
    KEY_DOWN,
    KEY_LEFT,
    KEY_RIGHT,
    KEY_SELECT,
    KEY_BACK,
    KEY_MENU,
    KEY_HOME,
    KEY_POWER
} KeyCode;

typedef struct {
    KeyCode code;
    bool pressed;
    bool long_press;
} KeyEvent;

typedef struct {
    bool (*init)(void);
    bool (*poll)(KeyEvent *out_event);
} InputDriver;
```

### 7.3 Storage Interface

```c
typedef struct {
    bool (*init)(void);
    bool (*mount)(void);
    bool (*unmount)(void);
    bool (*exists)(const char *path);
    int  (*read)(const char *path, void *buf, size_t max_len);
    int  (*write_atomic)(const char *path, const void *buf, size_t len);
    bool (*list_dir)(const char *path, DirEntry *entries, size_t max_entries);
    bool (*sync)(void);
} StorageDriver;
```

### 7.4 Power Interface

```c
typedef struct {
    bool (*init)(void);
    bool (*battery_present)(void);
    int  (*battery_percent)(void);
    void (*prepare_sleep)(void);
    void (*sleep)(void);
} PowerDriver;
```

---

## 8. Firmware Source Organization

Recommended source organization for Zephyr firmware:

```text
firmware/
`-- zephyr/
    |-- CMakeLists.txt
    |-- Kconfig
    |-- prj.conf
    |-- boards/
    |-- src/
    `-- include/
```

The firmware app-lifecycle host owns app registry access, bytecode validation,
launch transitions, crash recovery, root `main.sqbc` restart, and returning
control to the previous foreground return target.

A user-facing app picker or home screen is an ordinary SquidScript app,
commonly installed as root `main.sqbc`. Target profiles must not require a
special public launcher app kind.

The selected target determines:

- which drivers are compiled
- which drivers are registered
- which runtime limits are used
- which firmware capabilities are exposed

---

## 9. SquidScript Target Compatibility

Apps should declare requirements in terms of capabilities where possible.

Example:

```json
{
  "requires": {
    "runtime": "squidscript>=0.2",
    "display": {
      "minWidth": 480,
      "minHeight": 800,
      "pixelFormats": ["GRAY1_PACKED", "GRAY2_PACKED"]
    },
    "keys": ["LEFT", "RIGHT", "BACK"],
    "features": [
      "service.display.draw",
      "state.read",
      "state.write",
      "service.wifi.connect",
      "service.wifi.accessPoint"
    ]
  }
}
```

If an app must target a specific device, it may declare that explicitly.

Example:

```json
{
  "targets": {
    "allow": ["xteink-x4"]
  }
}
```

Specific target locking should be avoided unless necessary.

Capability targeting is preferred.

---

## 10. Compiler Target Selection

`squidc` should compile SquidScript source against the portable language/runtime
API by default. Target profiles are optional metadata inputs for explicit
target capability checks, simulator configuration, firmware build metadata,
docs, and autocomplete. Hardware aliases are resolved by firmware/runtime; if
the current device lacks an alias or capability, execution should fail with a
device/runtime error.

Concrete target build:

```sh
squidc app build apps/hello-menu \
  --out apps/hello-menu/main.sqbc
```

Explicit target capability check:

```sh
squidc app build apps/hello-menu \
  --target targets/xteink-x4.target.json \
  --check-target \
  --out apps/hello-menu/main.sqbc
```

`squidc` should use the selected target/profile to check:

- logical display size
- orientation
- available pixel formats
- available logical keys
- available built-ins
- SQBC target requirements
- foreground radio capabilities such as `service.wifi.*`
- bytecode size limit
- draw command limit
- max file read size
- max handle count
- required runtime service support

---

## 11. SQBC Target Metadata

The emitted `.sqbc` should include current target requirement metadata in binary
sections when an explicit target check is requested.

The metadata is conceptual at the compiler level, but the file representation must follow the `.sqbc` binary rules:

- little-endian integer fields
- explicit section headers and lengths
- string references through the string pool where practical
- no YAML, JSON, CBOR, protobuf, or host C struct padding inside `.sqbc`

Suggested target-requirements section:

```c
struct SqbcTargetRequirementsSection {
  uint16_t min_display_width;
  uint16_t min_display_height;
  uint16_t required_key_count;
  uint16_t required_feature_count;
  uint16_t required_pixel_format_count;
  uint16_t compiled_target_string_id;       // 0xffff if not target-locked
  uint16_t runtime_profile_string_id;       // 0xffff if not fixed
};
```

The section is followed by fixed-width arrays of string-pool IDs:

- required logical key names
- required feature names
- required pixel format names

Equivalent logical metadata:

```yaml
compiled_for_target: xteink-x4
runtime_profile: esp32c3-lowram
required_features:
  - service.display.draw
  - state.read
  - state.write
display_min_width: 480
```
display_min_height: 800
display_pixel_formats:
  - GRAY1_PACKED
  - GRAY2_PACKED
required_keys:
  - LEFT
  - RIGHT
  - BACK
```

Firmware should validate that the current device satisfies the bytecode requirements.

---

## 12. Capability-Targeted vs Device-Targeted Apps

There are two app targeting modes.

### 12.1 Capability-Targeted App

A capability-targeted app can run on any device that satisfies its requirements.

Recommended default.

Example:

- requires SquidScript 0.2
- requires service.display.draw
- requires 480x800 minimum display
- requires LEFT, RIGHT, BACK keys

This app can run on multiple devices if they provide those capabilities.

### 12.2 Device-Targeted App

A device-targeted app is compiled for one exact target.

Useful for:

- demos
- product-specific apps
- special hardware layouts
- device-specific assumptions

Example:

- allow only xteink-x4

Device targeting should be used sparingly.

---

## 13. Compatibility Profiles

Compatibility profiles describe portable target envelopes.

Examples:

`lowram-portrait-480x800-gray1-gray2`

- SquidScript 0.2
- portrait logical display
- minimum 480x800
- supports `GRAY1_PACKED` and `GRAY2_PACKED`
- low-RAM runtime limits
- BinBook support

`landscape-800x480-gray1`

- SquidScript 0.2
- landscape logical display
- minimum 800x480
- supports `GRAY1_PACKED`
- low-RAM or dev runtime limits

`psram-large-display`

- SquidScript 0.2
- larger bytecode/state/string limits
- larger display
- PSRAM available

## 14. Runtime Target Check on Device

When launching an app, firmware should check:

- current target provides required features
- current display satisfies display requirements
- current input profile provides required keys
- current runtime profile can satisfy bytecode resource limits
- app feature requirements are supported by the current firmware/runtime
- required document capabilities are available
- optional source map matches bytecode hash if source map is used

If the target cannot satisfy the app requirements, firmware or the current app
should show a clear error.

An app picker or home screen may be implemented in SquidScript, but firmware
still owns target checks. Apps request launches through the app
registry/lifecycle API; they do not directly execute `.sqbc`.

Example:

```text
Cannot run app:
BinBook Reader

Reason:
Requires portrait 480x800 display with `GRAY2_PACKED` support.
This target provides landscape 800x480 `GRAY1_PACKED`.
```

Example:

```text
Cannot run app:
Presentation Clicker

Reason:
Requires key RIGHT.
This target has no RIGHT key mapping.
```

Example:

```text
Cannot run app:
Large Flashcard App

Reason:
Bytecode requires maxStateVariables=128.
This target allows maxStateVariables=64.
```

---

## 15. Display Portability

SquidScript apps should not assume a physical display orientation.

Apps use logical coordinates.

The display profile defines:

- physical width/height
- logical width/height
- rotation

Example XTEINK X4:

- physical: 800x480
- logical: 480x800
- rotation: 90 degrees

Example Waveshare dev target:

- physical: 800x480
- logical: 800x480
- rotation: 0 degrees

Apps may query display information:

```squid
let d = display.info()
```

Example record:

```json
{
  "ok": true,
  "available": true,
  "binding": "display.default",
  "driver": "ssd1677",
  "transport": "spi",
  "width": 480,
  "height": 800,
  "nativeBpp": 2,
  "nativePixelFormat": "GRAY2_PACKED",
  "supportsPartialRefresh": true,
  "supportsFastRefresh": false
}
```

Example:

```squid
screen("main", { render: "compose" }) {
  let d = display.info()
  service.display.text("Hello", {
    x: 20,
    y: 20,
    w: d.width - 40,
    h: d.height - 40,
    wrap: true
  })
}
```

If using compile-time constants, the bytecode should declare its compiled display assumptions.

---

## 16. Development Peripheral Composition

Development targets may combine separate peripherals.

Example:

```json
{
  "format": "squid-target-v1",
  "id": "esp32s3-dev-waveshare",
  "board": "esp32s3-devkit",
  "display": "waveshare-7in5-v2",
  "input": "dev-buttons-7key",
  "storage": "spi-sdcard-dev",
  "power": "usb-dev-power",
  "runtime": "esp32s3-psram"
}
```

Another development target using the same board and display but different input:

```json
{
  "format": "squid-target-v1",
  "id": "esp32s3-dev-waveshare-touch",
  "board": "esp32s3-devkit",
  "display": "waveshare-7in5-v2",
  "input": "waveshare-touch",
  "storage": "spi-sdcard-dev",
  "power": "usb-dev-power",
  "runtime": "esp32s3-psram"
}
```

This avoids creating monolithic board definitions for every experiment.

---

## 17. Profile Inheritance and Overrides

Profiles may inherit from a base profile to support quick prototypes and future profile families without duplicating every field.

Inheritance is optional. A profile without `extends` is a base profile.

Example:

```json
{
  "format": "squid-target-display",
  "id": "waveshare-7in5-v2-rotated-dev",
  "extends": "waveshare-7in5-v2",
  "logical": {
    "width": 480,
    "height": 800,
    "rotation": 90
  }
}
```

Resolution rules:

1. Load the base profile named by `extends`.
2. Confirm the child and parent have the same `format`.
3. Apply object fields as a deep merge.
4. Replace arrays by default.
5. Replace scalar values directly.
6. Reject inheritance cycles.
7. Validate the fully resolved profile, not just the child fragment.

Targets may also inherit from other targets:

```json
{
  "format": "squid-target-v1",
  "id": "esp32s3-waveshare-7in5-touch-prototype",
  "extends": "esp32s3-waveshare-7in5",
  "input": "waveshare-touch"
}
```

Keep target-level overrides shallow. Prefer creating a new component profile when the override describes reusable hardware behavior.

Array replacement is intentional because lists such as `features`, `buttons`, and `supportedPixelFormats` are easier to reason about when inherited arrays do not merge implicitly. If merged arrays are needed later, add an explicit operator instead of changing the default behavior.

---

## 18. Target Profile Schema

The canonical practical schema is `squid-target-v1` and is documented in detail in
`docs/target_definition_reference.md`.

Integrated target schema shape:

```json
{
  "format": "squid-target-v1",
  "id": "target-id",
  "name": "Human Name",
  "mcu": {},
  "firmwareUpdate": {},
  "pins": {},
  "buses": {},
  "devices": {},
  "display": {},
  "input": {},
  "storage": {},
  "power": {},
  "runtime": {},
  "features": [
    "sdcard",
    "buttons",
    "squidscript.bytecode"
  ]
}
```

Optional split composition schema shape:

```json
{
  "format": "squid-target-v1",
  "id": "target-id",
  "name": "Human Name",
  "extends": "optional-base-target-id",
  "board": "board-id",
  "display": "display-id",
  "input": "input-id",
  "storage": "storage-id",
  "power": "power-id",
  "runtime": "runtime-id",
  "features": []
}
```

Split composition should resolve to the same internal model as the integrated schema before firmware config generation.

---

## 19. Build System Responsibilities

The firmware build system should:

1. Accept `DEVICE_TARGET`.
2. Load the target JSON.
3. Resolve target inheritance if used.
4. Load referenced board/display/input/storage/power/runtime profile parts only when the target uses split composition.
5. Resolve component profile inheritance only when split composition is used.
6. Normalize the integrated or split source form into one resolved target model.
7. Validate bus and GPIO bindings.
8. Validate feature and pixel-format names.
9. Validate that target features are backed by selected profiles and drivers.
10. Generate backend-facing target configuration such as `target_config.rs` or `target_config.h`.
11. Compile only required drivers where practical.
12. Register selected drivers.
13. Set runtime limits.
14. Embed target ID and diagnostics info into firmware.
15. Expose target info to app registry tooling and SquidScript runtime.
16. Emit native flashing artifacts for the selected MCU/toolchain.
17. Emit `firmware.uf2` when the selected board supports UF2.
18. Emit a firmware manifest with hashes for all distributed images.

Combined profile validation must check:

- referenced profile IDs exist when split composition is used
- all `format` values are known
- display/input/storage bus references exist on the selected board
- GPIO assignments are valid for the selected MCU/board
- GPIO assignments do not conflict unless explicitly marked as shared
- UF2 is requested only when the selected board profile declares UF2 support
- UF2 family ID is present when `firmwareUpdate.formats` includes `uf2`
- ADC ladder ranges on the same ADC input do not overlap
- logical key names are known to the SquidScript input model
- storage `maxFileReadSize` does not exceed the runtime limit exposed to apps
- display `defaultPixelFormat` is listed in `supportedPixelFormats`
- display `defaultBpp` is consistent with `defaultPixelFormat`
- target capabilities such as `squidscript.bytecode` and `service.display.draw`
  are provided by the selected firmware/runtime modules

Target-check tooling should:

1. Accept `--target` or `--profile` only when an explicit check is requested.
2. Load the target/profile.
3. Validate app requirements.
4. Enforce target-specific limits.
5. Emit current-format `.sqbc`.
6. Include target requirement metadata in `.sqbc` when requested.
7. Optionally emit `source-map.json`.

---

## 20. Firmware Replacement Artifacts

UF2 support is a firmware distribution feature, not a SquidScript app packaging feature.

The firmware build should prefer UF2 for user-facing replacement when the bootloader and board can support it because the replacement flow is simple:

1. User enters firmware update mode.
2. Device exposes a USB mass-storage volume.
3. User copies `firmware.uf2` onto the volume.
4. Bootloader validates the UF2 family and writes the firmware image.
5. Device reboots into the new firmware.

The UF2 image must contain only firmware flash payloads for the selected target. It must not be used to install `.sqbc` apps, `.binbook` content, source maps, or user state. Those remain storage-level files managed by the app registry, app installer, or content workflows.

The bootloader/update flow should preserve user storage by default. Firmware replacement must not erase installed apps, BinBook files, app registry state, Wi-Fi profiles, or app state unless the user explicitly chooses a factory reset or recovery image documented as destructive.

Recommended artifact naming:

```text
squidscript-firmware-${target}-${build}.uf2
squidscript-firmware-${target}-${build}.bin
squidscript-firmware-${target}-${build}.manifest.json
```

The firmware manifest should mark the intended replacement mode:

```json
{
  "format": "squidscript-zephyr-firmware-manifest",
  "target": "xteink-x4",
  "build": "source-or-build-id",
  "replacement": {
    "preferredFormat": "uf2",
    "userReplacement": "usb-mass-storage",
    "preservesUserStorage": true
  },
  "artifacts": [
    {
      "path": "squidscript-firmware-xteink-x4-source-or-build-id.uf2",
      "format": "uf2",
      "sha256": "..."
    },
    {
      "path": "squidscript-firmware-xteink-x4-source-or-build-id.bin",
      "format": "esp-idf-bin",
      "sha256": "..."
    }
  ]
}
```

When UF2 is not supported, the manifest should still describe the native replacement path, such as serial flashing, factory flashing, or recovery flashing.

---

## 21. Avoiding `#ifdef` Sprawl

Bad pattern:

```c
#ifdef XTEINK_X4
  rotate_display_90();
  use_2bpp();
#endif

#ifdef WAVESHARE_7IN5
  use_1bpp();
#endif
```

Better pattern:

```c
DisplayInfo info = display_driver->info();

renderer_set_logical_size(info.logical_width, info.logical_height);
renderer_set_rotation(info.rotation);
renderer_set_bpp(info.default_bpp);
```

The selected driver and generated target config provide the behavior.

Use `#ifdef` only for:

- selecting compiled drivers
- MCU-specific low-level code
- SDK-specific configuration

Do not use `#ifdef` throughout app logic, VM logic, or renderer logic unless unavoidable.

---

## 22. Recommended Rules

1. Treat target profiles as first-class project artifacts.
2. Use one integrated `*.target.json` file for fixed production devices.
3. Apps should target capabilities, not boards, unless necessary.
4. `squidc` should compile against the portable language/runtime API by default.
   Target profiles should be explicit opt-in inputs for target checks,
   simulator configuration, firmware metadata, docs, and autocomplete.
5. `.sqbc` may include target requirement metadata.
6. Firmware should validate target requirements before launch.
7. SquidScript apps should use logical coordinates.
8. Display rotation belongs in the target display section and display driver, not in apps.
9. Development targets should be normal targets, not hacks.
10. Keep runtime limits target-specific.
11. Avoid scattering device-specific conditionals through the codebase.
12. Use split profile parts and inheritance only when they reduce real reuse burden, and validate only fully resolved targets.

---

## 23. Practical Recommendation

For your initial targets, define at least:

Targets:

- `targets/xteink-x4.target.json`
- `targets/esp32s3-waveshare-7in5.target.json`

Optional reusable profile parts:

- `esp32s3-devkit`
- `waveshare-7in5-v2`
- `dev-buttons-7key`
- `spi-sdcard-dev`
- `usb-dev-power`
- `esp32s3-psram`

Compatibility profiles:

- lowram-portrait-480x800-gray1-gray2
- landscape-800x480-gray1
- psram-large-display

This gives you:

- one production-style XTEINK X4 target
- one generous ESP32-S3 development target
- portable app target checks
- a cleaner compiler/runtime contract
- less firmware target sprawl
