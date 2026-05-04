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

A target is a composition of profiles.

```text
Target profile =
  board profile
+ display profile
+ input profile
+ storage profile
+ power profile
+ runtime profile
```

Every profile JSON object must declare a `format` string and an `id`.

The `format` value identifies the schema family and major version. Tools must reject unknown required schema versions rather than guessing.

Examples:

- `squid-target-board-v1`
- `squid-target-display-v1`
- `squid-target-input-v1`
- `squid-target-storage-v1`
- `squid-target-power-v1`
- `squid-target-runtime-v1`
- `squid-target-v1`
- `squid-compat-profile-v1`

Example:

```yaml
xteink-x4:
  board: xteink-x4-board
  display: xteink-x4-display
  input: xteink-x4-buttons
  storage: xteink-x4-sdcard
  power: xteink-x4-power
  runtime: esp32c3-lowram

dev-esp32s3-waveshare-7in5:
  board: esp32s3-devkit
  display: waveshare-7in5-v2
  input: dev-buttons-6key
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
- available buses
- GPIO assignments
- onboard peripherals

Example board profile:

```json
{
  "format": "squid-target-board-v1",
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
  "buses": {
    "spi": ["spi2", "spi3"],
    "i2c": ["i2c0"],
    "uart": ["uart0"]
  }
}
```

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
- default bit depth
- default pixel format
- partial refresh support
- fast refresh support
- framebuffer strategy

Example XTEINK X4 display profile:

```json
{
  "format": "squid-target-display-v1",
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
    "supportedBpp": [1, 2],
    "defaultBpp": 2,
    "supportedPixelFormats": ["GRAY1_PACKED", "GRAY2_PACKED"],
    "defaultPixelFormat": "GRAY2_PACKED"
  },
  "refresh": {
    "full": true,
    "partial": true,
    "fast": false
  },
  "framebuffer": {
    "preferredMode": "tiled",
    "maxFullBufferBpp": 2
  }
}
```

Example Waveshare display profile:

```json
{
  "format": "squid-target-display-v1",
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
    "supportedBpp": [1],
    "defaultBpp": 1,
    "supportedPixelFormats": ["GRAY1_PACKED"],
    "defaultPixelFormat": "GRAY1_PACKED"
  },
  "refresh": {
    "full": true,
    "partial": false,
    "fast": false
  },
  "framebuffer": {
    "preferredMode": "tiled",
    "maxFullBufferBpp": 1
  }
}
```

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

Example input profile:

```json
{
  "format": "squid-target-input-v1",
  "id": "dev-buttons-6key",
  "type": "gpio-buttons",
  "buttons": [
    { "logical": "UP", "gpio": 1, "activeLow": true },
    { "logical": "DOWN", "gpio": 2, "activeLow": true },
    { "logical": "LEFT", "gpio": 3, "activeLow": true },
    { "logical": "RIGHT", "gpio": 4, "activeLow": true },
    { "logical": "SELECT", "gpio": 5, "activeLow": true },
    { "logical": "BACK", "gpio": 6, "activeLow": true }
  ]
}
```

If a target lacks a required logical key, the app may be marked incompatible.

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
  "format": "squid-target-storage-v1",
  "id": "spi-sdcard-dev",
  "type": "sdcard",
  "bus": "spi3",
  "mount": "/sd",
  "supportsApps": true,
  "supportsContent": true,
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
  "format": "squid-target-power-v1",
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

Example battery profile:

```json
{
  "format": "squid-target-power-v1",
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
  "format": "squid-target-runtime-v1",
  "id": "esp32c3-lowram",
  "squidscript": {
    "version": "0.2",
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
    "maxParsedDataSections": 256,
    "maxListItemsReturned": 256,
    "maxHandles": 16
  },
  "features": [
    "squidscript.bytecode",
    "display.draw",
    "input.text",
    "state.read",
    "state.write",
    "stateMachine",
    "content.pick",
    "content.read",
    "binbook.read",
    "wifi.connect",
    "wifi.setup",
    "httpServer.serve",
    "bluetoothHid.advertise",
    "bluetoothHid.keys"
  ]
}
```

Example ESP32-S3 PSRAM runtime:

```json
{
  "format": "squid-target-runtime-v1",
  "id": "esp32s3-psram",
  "squidscript": {
    "version": "0.2",
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
    "maxParsedDataSections": 1024,
    "maxListItemsReturned": 1024,
    "maxHandles": 64
  },
  "features": [
    "squidscript.bytecode",
    "display.draw",
    "input.text",
    "state.read",
    "state.write",
    "stateMachine",
    "content.pick",
    "content.read",
    "binbook.read",
    "wifi.connect",
    "wifi.setup",
    "httpServer.serve",
    "bluetoothHid.advertise",
    "bluetoothHid.keys",
    "debug-ui"
  ]
}
```

---

## 4. Target Profile

A target profile composes the other profiles.

Example XTEINK X4 target:

```json
{
  "format": "squid-target-v1",
  "id": "xteink-x4",
  "name": "XTEINK X4",
  "board": "xteink-x4-board",
  "display": "xteink-x4-display",
  "input": "xteink-x4-buttons",
  "storage": "xteink-x4-sdcard",
  "power": "xteink-x4-power",
  "runtime": "esp32c3-lowram",
  "compatibility": [
    "squidscript-0.2",
    "portrait-480x800",
    "pixel-format.GRAY1_PACKED",
    "pixel-format.GRAY2_PACKED",
    "binbook"
  ],
  "features": [
    "sdcard",
    "buttons",
    "binbook.read",
    "squidscript.bytecode"
  ]
}
```

Example ESP32-S3 + Waveshare development target:

```json
{
  "format": "squid-target-v1",
  "id": "esp32s3-waveshare-7in5",
  "name": "ESP32-S3 DevKit + Waveshare 7.5in e-Paper",
  "board": "esp32s3-devkit",
  "display": "waveshare-7in5-v2",
  "input": "dev-buttons-6key",
  "storage": "spi-sdcard-dev",
  "power": "usb-dev-power",
  "runtime": "esp32s3-psram",
  "compatibility": [
    "squidscript-0.2",
    "landscape-800x480",
    "pixel-format.GRAY1_PACKED",
    "binbook",
    "debug-ui"
  ],
  "features": [
    "sdcard",
    "buttons",
    "usb-serial",
    "binbook.read",
    "squidscript.bytecode",
    "debug-ui"
  ]
}
```

`binbook` names the document/book format. Runtime access to BinBook documents is represented by capabilities such as `binbook.read`. SquidScript bytecode is the executable bytecode format for SquidScript apps; runtime support for loading and executing it is represented by `squidscript.bytecode`.

---

## 5. Recommended Directory Layout

```text
firmware/
|-- targets/
|   |-- xteink-x4.target.json
|   `-- esp32s3-waveshare-7in5.target.json
|-- boards/
|   |-- xteink-x4-board.json
|   `-- esp32s3-devkit.json
|-- displays/
|   |-- xteink-x4-display.json
|   `-- waveshare-7in5-v2.json
|-- inputs/
|   |-- xteink-x4-buttons.json
|   `-- dev-buttons-6key.json
|-- storage/
|   |-- xteink-x4-sdcard.json
|   `-- spi-sdcard-dev.json
|-- power/
|   |-- xteink-x4-power.json
|   `-- usb-dev-power.json
|-- runtime-profiles/
|   |-- esp32c3-lowram.json
|   `-- esp32s3-psram.json
`-- src/
    |-- main.c
    |-- target/
    |-- hal/
    |-- drivers/
    |-- squidvm/
    |-- binbook/
    `-- app_lifecycle/
```

---

## 6. Firmware Build-Time Target Selection

The firmware build should select a concrete target.

Example with ESP-IDF:

```sh
idf.py -DDEVICE_TARGET=xteink-x4 build
```

Example wrapper:

```sh
make target=xteink-x4 build
make target=esp32s3-waveshare-7in5 build
```

The build system should load the target profile and generate a C header.

Example generated header:

```c
#define DEVICE_TARGET_ID "xteink-x4"
#define DEVICE_MCU_ESP32C3 1

#define DISPLAY_DRIVER_XTEINK_X4 1
#define DISPLAY_LOGICAL_WIDTH 480
#define DISPLAY_LOGICAL_HEIGHT 800
#define DISPLAY_PHYSICAL_WIDTH 800
#define DISPLAY_PHYSICAL_HEIGHT 480
#define DISPLAY_ROTATION 90
#define DISPLAY_DEFAULT_BPP 2
#define DISPLAY_PIXEL_FORMAT_GRAY2_PACKED 1
#define DISPLAY_DEFAULT_PIXEL_FORMAT DISPLAY_PIXEL_FORMAT_GRAY2_PACKED
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

### 7.1 Display Interface

```c
typedef enum {
    PIXEL_FORMAT_GRAY1_PACKED,
    PIXEL_FORMAT_GRAY2_PACKED
} PixelFormat;

typedef struct {
    uint16_t logical_width;
    uint16_t logical_height;
    uint16_t physical_width;
    uint16_t physical_height;
    uint8_t rotation;
    uint8_t default_bpp;
    PixelFormat default_pixel_format;
    bool partial_refresh;
    bool fast_refresh;
} DisplayInfo;

typedef struct {
    bool (*init)(void);
    DisplayInfo (*info)(void);
    bool (*clear)(uint8_t color);
    bool (*draw_tile)(int x, int y, int w, int h, const uint8_t *pixels, uint8_t bpp);
    bool (*refresh_full)(void);
    bool (*refresh_partial)(int x, int y, int w, int h);
    void (*sleep)(void);
} DisplayDriver;
```

### 7.1.1 Firmware Rendering Note: Strip-Streamed E-Paper

For low-RAM e-paper targets such as XTEINK X4, firmware should prefer strip-streamed page rendering over a full-screen framebuffer for large document surfaces. This should not force every UI path into the same rendering model.

Reference idea: Pulp-OS for XTEINK X4 renders the 800x480 SSD1677 display in horizontal physical strips, using a small strip buffer instead of a full 1bpp framebuffer. A 40-row strip at 800 pixels wide is about 4 KB at 1bpp, while a full 800x480 framebuffer is about 48 KB at 1bpp and about 96 KB at 2bpp.

The SquidScript-facing display API should not expose this detail. Apps still submit bounded draw commands and display-ready drawables through `display.*`. The firmware renderer may replay the current screen's draw command list once per physical strip or partial-refresh window:

```text
screen render
  -> bounded draw command list
  -> for each physical strip/window:
       clear reusable strip buffer
       replay intersecting draw commands into the strip
       ask drawables to render only intersecting rows/pixels
       stream strip bytes to the display controller over SPI
  -> trigger full or partial e-paper refresh
```

`binbook.pageImage(page)` should therefore be allowed to return a transient drawable descriptor rather than a decoded full-page pixel buffer. The BinBook renderer can decode, convert, dither, or copy only the rows needed for the active strip. This keeps page rendering compatible with low-memory targets and avoids making BinBook page buffers part of app-visible state.

Recommended split:

- layout is performed off-device when producing BinBook, or cached by firmware when runtime layout is unavoidable
- large page rendering is strip-based, especially for BinBook pages and other document-like surfaces
- interactive UI uses small dirty-region buffers or direct small-window updates where that is simpler than replaying the whole screen by strips
- firmware may combine these strategies in one render pass as long as the app-visible behavior remains `display.*` draw commands and drawables

Implementation rules:

- keep strip buffers firmware-owned and target-specific
- keep display rotation and logical-to-physical mapping in the display driver/profile
- make drawables renderable into an arbitrary clipped strip/window
- allow small dirty-region buffers for menus, cursors, modal UI, and other interactive surfaces
- avoid requiring the VM, app state, or language spec to know whether the target uses a full framebuffer, tiles, strips, or direct streaming
- account for pixel format honestly: `GRAY1_PACKED` can use the smallest strips; `GRAY2_PACKED` needs larger strips or per-strip conversion to the panel's native format

References:

- AnswerOverflow thread: https://www.answeroverflow.com/m/1478448257716850871
- Pulp-OS repository: https://github.com/hansmrtn/pulp-os
- Pulp-OS strip renderer: `kernel/src/drivers/strip.rs`
- Pulp-OS SSD1677 driver: `kernel/src/drivers/ssd1677.rs`

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

Recommended source organization:

```text
src/
|-- main.c
|-- target/
|   |-- target_config.h
|   `-- target_registry.c
|-- hal/
|   |-- display_driver.h
|   |-- input_driver.h
|   |-- storage_driver.h
|   `-- power_driver.h
|-- drivers/
|   |-- display_xteink_x4.c
|   |-- display_waveshare_7in5.c
|   |-- input_gpio_buttons.c
|   |-- storage_sd_spi.c
|   `-- power_usb_dev.c
|-- squidvm/
|   |-- squid_vm.c
|   |-- squid_bytecode.c
|   |-- squid_loader.c
|   `-- squid_builtins.c
|-- binbook/
|   |-- binbook.c
|   `-- binbook_render.c
`-- app_lifecycle/
    |-- launcher_host.c
    `-- app_registry.c
```

The firmware app-lifecycle host owns app registry access, bytecode validation, launch transitions, crash recovery, and returning control to the active launcher.

The user-facing launcher UI should be a SquidScript app with `kind = "launcher"`. Different launcher designs can be installed like apps and selected by the user through firmware/system UI.

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
      "display.draw",
      "input.text",
      "state.read",
      "state.write",
      "binbook.read",
      "wifi.connect",
      "httpServer.serve"
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

`squidc` should compile against a target profile or compatibility profile.

Concrete target build:

```sh
squidc build apps/binbook-reader \
  --target xteink-x4 \
  --out apps/binbook-reader/main.sqbc \
  --source-map
```

Development target build:

```sh
squidc build apps/binbook-reader \
  --target esp32s3-waveshare-7in5 \
  --out apps/binbook-reader/main.sqbc \
  --source-map
```

Compatibility profile build:

```sh
squidc build apps/simple-counter \
  --profile lowram-portrait-480x800-gray1-gray2 \
  --out apps/simple-counter/main.sqbc
```

`squidc` should use the selected target/profile to check:

- logical display size
- orientation
- available pixel formats
- available pixel formats
- available logical keys
- available built-ins
- required permissions
- foreground radio/server capabilities such as `wifi.*`, `httpServer.*`, and `bluetoothHid.*`
- bytecode size limit
- draw command limit
- max file read size
- max handle count
- BinBook support
- runtime version compatibility

---

## 11. SQBC Target Metadata

The emitted `.sqbc` should include target or compatibility metadata in binary sections.

The metadata is conceptual at the compiler level, but the file representation must follow the `.sqbc` binary rules:

- little-endian integer fields
- explicit section headers and lengths
- string references through the string pool where practical
- no YAML, JSON, CBOR, protobuf, or host C struct padding inside `.sqbc`

Suggested target-requirements section:

```c
struct SqbcTargetRequirementsSection {
  uint16_t runtime_min_major;
  uint16_t runtime_min_minor;
  uint16_t runtime_max_major;
  uint16_t runtime_max_minor;
  uint16_t min_display_width;
  uint16_t min_display_height;
  uint16_t required_key_count;
  uint16_t required_feature_count;
  uint16_t required_pixel_format_count;
  uint16_t compiled_target_string_id;       // 0xffff if not target-locked
  uint16_t compatibility_profile_string_id; // 0xffff if none
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
  - display.draw
  - state.read
  - state.write
  - binbook.read
display_min_width: 480
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
- requires display.draw
- requires 480x800 minimum display
- requires LEFT, RIGHT, BACK keys
- requires binbook.read

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

A concrete target can declare compatibility with one or more compatibility profiles.

Example:

```yaml
xteink-x4:
  compatible_with:
    - lowram-portrait-480x800-gray1-gray2

esp32s3-waveshare-7in5:
  compatible_with:
    - landscape-800x480-gray1
    - psram-large-display
```

Apps should usually compile against compatibility profiles rather than exact devices.

---

## 14. Runtime Compatibility Check on Device

When launching an app, firmware should check:

- app requires a supported SquidScript runtime version
- `.sqbc` bytecode version is supported
- current target provides required features
- current display satisfies display requirements
- current input profile provides required keys
- current runtime profile can satisfy bytecode resource limits
- app permissions are declared and allowed
- required document capabilities are available
- optional source map matches bytecode hash if source map is used

If incompatible, launcher should show a clear error.

The user-facing launcher may be implemented in SquidScript, but firmware still owns this compatibility check. A launcher requests app launches through `launcher.*` capabilities; it does not directly execute `.sqbc`.

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
let d = displayInfo()
```

Example record:

```json
{
  "logicalWidth": 480,
  "logicalHeight": 800,
  "bpp": 2,
  "pixelFormat": "GRAY2_PACKED",
  "partialRefresh": true,
  "fastRefresh": false
}
```

Apps may also use compile-time constants generated by `squidc`:

```text
SCREEN_WIDTH
SCREEN_HEIGHT
SCREEN_BPP
SCREEN_PIXEL_FORMAT
```

Example:

```squid
screen("main") {
  display.text("Hello", {
    x: 20,
    y: 20,
    w: SCREEN_WIDTH - 40,
    h: SCREEN_HEIGHT - 40,
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
  "input": "dev-buttons-6key",
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
  "format": "squid-target-display-v1",
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

Conceptual target schema:

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
  "compatibility": [
    "squidscript-0.2",
    "portrait-480x800",
    "pixel-format.GRAY1_PACKED",
    "pixel-format.GRAY2_PACKED",
    "binbook"
  ],
  "features": [
    "sdcard",
    "buttons",
    "binbook.read",
    "squidscript.bytecode"
  ]
}
```

---

## 19. Build System Responsibilities

The firmware build system should:

1. Accept `DEVICE_TARGET`.
2. Load the target JSON.
3. Resolve target inheritance.
4. Load referenced board/display/input/storage/power/runtime profiles.
5. Resolve component profile inheritance.
6. Validate the combined profile.
7. Validate bus and GPIO bindings.
8. Validate feature, compatibility, and pixel-format names.
9. Validate that target features are backed by selected profiles and drivers.
10. Generate `target_config.h`.
11. Compile only required drivers where practical.
12. Register selected drivers.
13. Set runtime limits.
14. Embed target ID and compatibility info into firmware.
15. Expose target info to launcher and SquidScript runtime.

Combined profile validation must check:

- referenced profile IDs exist
- all profile `format` values are known and compatible
- display/input/storage bus references exist on the selected board
- GPIO assignments are valid for the selected MCU/board
- GPIO assignments do not conflict unless explicitly marked as shared
- storage `maxFileReadSize` does not exceed the runtime limit exposed to apps
- display `defaultPixelFormat` is listed in `supportedPixelFormats`
- display `defaultBpp` is consistent with `defaultPixelFormat`
- target capabilities such as `binbook.read` and `squidscript.bytecode` are provided by the selected firmware/runtime modules
- compatibility profile claims are satisfied by the resolved target

The compiler tooling should:

1. Accept `--target` or `--profile`.
2. Load the target/profile.
3. Validate app requirements.
4. Enforce target-specific limits.
5. Emit compatible `.sqbc`.
6. Include compatibility metadata in `.sqbc`.
7. Optionally emit `source-map.json`.

---

## 20. Avoiding `#ifdef` Sprawl

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

## 21. Recommended Rules

1. Treat target profiles as first-class project artifacts.
2. Compose targets from: board + display + input + storage + power + runtime.
3. Apps should target capabilities, not boards, unless necessary.
4. `squidc` should compile against a target or compatibility profile.
5. `.sqbc` should include compatibility metadata.
6. Firmware should validate app compatibility before launch.
7. SquidScript apps should use logical coordinates.
8. Display rotation belongs in the display driver/profile, not in apps.
9. Development targets should be normal targets, not hacks.
10. Keep runtime limits target-specific.
11. Avoid scattering device-specific conditionals through the codebase.
12. Use profile inheritance for prototypes and families, but validate only fully resolved profiles.

---

## 22. Practical Recommendation

For your initial targets, define at least:

Targets:

- xteink-x4
- esp32s3-waveshare-7in5

Boards:

- xteink-x4-board
- esp32s3-devkit

Displays:

- xteink-x4-display
- waveshare-7in5-v2

Inputs:

- xteink-x4-buttons
- dev-buttons-6key

Storage:

- xteink-x4-sdcard
- spi-sdcard-dev

Power:

- xteink-x4-power
- usb-dev-power

Runtime profiles:

- esp32c3-lowram
- esp32s3-psram

Compatibility profiles:

- lowram-portrait-480x800-gray1-gray2
- landscape-800x480-gray1
- psram-large-display

This gives you:

- one production-style XTEINK X4 target
- one generous ESP32-S3 development target
- portable app compatibility checks
- a cleaner compiler/runtime contract
- less firmware target sprawl
