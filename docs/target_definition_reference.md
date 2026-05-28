# SquidScript Target Definition Reference

Status: Draft
Purpose: Define the practical target-definition format consumed by future SquidScript firmware target tooling.

---

## 1. Purpose

A target definition describes one concrete firmware build target.

For integrated devices such as XTEINK X4, the canonical source artifact should be a single JSON file:

```text
targets/xteink-x4.target.json
```

The target definition is a build-time source file. It is not parsed by SquidScript apps and should not be parsed by production firmware at runtime.

The firmware target compiler should read the target JSON, validate it, and generate firmware-facing artifacts such as:

- `target_config.h`
- optional static C/C++ config structs
- firmware manifest metadata
- target requirement metadata embedded into firmware or app artifacts when
  explicitly requested

Production firmware consumes generated constants and structs, not raw JSON.

Firmware build orchestration, backend selection, and simulator backend policy are described in:

```text
docs/firmware_build_architecture.md
```

---

## 2. Design Rules

Target definitions must be explicit enough to prevent hidden board assumptions from leaking into firmware code.

Rules:

1. A target JSON object must declare `format`, `id`, and `name`.
2. `format` must be `squid-target` for this schema.
3. Integrated production devices should use one target file with named sections.
4. Split board/display/input/storage/power/runtime profiles are optional advanced composition, mainly for reusable development-board combinations.
5. GPIOs, buses, onboard devices, and logical capabilities must be described in the target file.
6. SquidScript apps must see logical capabilities such as `event.on("key.UP")` and `service.display.draw(...)`, not raw GPIOs.
7. Placeholder, guessed, or unverified hardware values must be explicitly marked as such.

---

## 3. Source and Resolved Forms

The source target file is the human-maintained JSON document.

The resolved target is the target compiler's normalized in-memory model after validation.

The source and resolved forms may be nearly identical. Tooling may expand
aliases, apply inheritance, or normalize sections.

Example flow:

```text
targets/xteink-x4.target.json
  -> target compiler validates and resolves
  -> target_config.h
  -> optional generated config structs
  -> firmware build
```

The generated artifacts are allowed to be C/C++ specific. The source JSON should remain tooling-neutral.

---

## 4. Top-Level Fields

Required fields:

- `format`: schema identifier. Must be `squid-target`.
- `id`: stable lowercase target ID, such as `xteink-x4`.
- `name`: human-readable device name.
- `mcu`: MCU and memory facts.
- `pins`: all GPIOs used by the firmware target.
- `buses`: named buses used by onboard devices.
- `devices`: named onboard devices.
- `display`: display hardware exposed through `service.display.*`.
- `input`: physical input mapping to logical keys.
- `storage`: content/app storage configuration.
- `power`: battery, sleep, wake, and USB detection behavior.
- `runtime`: SquidScript VM limits for this target.
- `features`: firmware/runtime features exposed by this target.

Optional fields:

- `status`: lifecycle marker such as `draft`, `reference`, or `production`.
- `sourceAttribution`: list of sources used to verify hardware facts.
- `firmwareUpdate`: firmware image and replacement metadata.
- `radios`: Wi-Fi, BLE, or other radio hardware available on the target.
- `simulator`: optional simulator and layout metadata.

---

## 5. MCU Section

`mcu` describes the processor and memory available to firmware.

Example:

```json
{
  "part": "ESP32-C3",
  "family": "ESP32C3",
  "cpu": {
    "cores": 1,
    "frequencyMHz": 160
  },
  "memory": {
    "internalSramKB": 400,
    "psramKB": 0
  },
  "flash": {
    "sizeMB": 16
  }
}
```

The target compiler should use this section to select MCU-specific build settings, validate known GPIO names, validate memory-sensitive runtime limits, and emit generated firmware constants.

---

## 5.1 Simulator Section

`simulator` describes optional metadata used by simulator backends, documentation renderers, and web tools.

This section must not replace electrical hardware definitions. Firmware wiring still comes from `pins`, `buses`, `devices`, `display`, `input`, `storage`, and `power`.

Example:

```json
{
  "simulator": {
    "layout": "targets/layouts/xteink-x4.layout.json",
    "defaultBackend": "browser-sim"
  }
}
```

Fields:

- `layout`: path to a `squid-layout` layout file.
- `defaultBackend`: optional simulator backend hint such as `browser-sim`.

The layout file is presentation metadata. It may describe device outline, screen placement, button positions, LEDs, ports, labels, and simulator hit targets.

Example layout:

```json
{
  "format": "squid-layout",
  "id": "xteink-x4-layout",
  "target": "xteink-x4",
  "units": "px",
  "canvas": {
    "width": 720,
    "height": 1120
  },
  "device": {
    "shape": "rounded-rect",
    "x": 40,
    "y": 40,
    "width": 640,
    "height": 1040,
    "radius": 24
  },
  "elements": [
    {
      "id": "display",
      "kind": "display",
      "device": "display.epd",
      "x": 120,
      "y": 120,
      "width": 480,
      "height": 800
    },
    {
      "id": "button-select",
      "kind": "button",
      "logical": "SELECT",
      "x": 320,
      "y": 980,
      "width": 80,
      "height": 48,
      "label": "Select"
    }
  ]
}
```

Layout rules:

1. `kind: "display"` elements should reference a target `devices` ID through `device`.
2. `kind: "button"` elements should reference target logical keys through `logical`.
3. Layout coordinates are simulator/documentation coordinates, not physical millimeters unless `units` says otherwise.
4. Placeholder, guessed, or approximate positions must be explicitly marked with `status: "placeholder"` or equivalent notes.
5. The browser simulator may use button elements as pointer/touch hit targets, but the target `input` section remains the source of valid logical keys.

---

## 6. Firmware Update Section

`firmwareUpdate` describes firmware replacement artifacts and user-facing replacement paths.

Example:

```json
{
  "formats": ["esp-idf-bin"],
  "preferredFormat": "esp-idf-bin",
  "userReplacement": "serial-or-web-flash",
  "uf2": {
    "status": "deferred",
    "reason": "UF2 support is desired but not yet verified for this bootloader."
  }
}
```

If UF2 support is verified later, `formats` may include `uf2`, `preferredFormat` may become `uf2`, and a target-specific UF2 family ID or conversion rule should be added.

Do not claim UF2 support in a production target unless the bootloader and generated image path have been tested on hardware.

---

## 7. Pins Section

`pins` maps physical GPIO names to capabilities and ownership.

Example:

```json
{
  "GPIO8": {
    "capabilities": ["gpio", "spi_sck"],
    "voltage": "3v3",
    "usedBy": ["bus.spi.shared.sck"]
  }
}
```

Fields:

- `capabilities`: hardware functions available on the pin, such as `gpio`, `adc`, `spi_sck`, `spi_mosi`, `spi_miso`, `uart_rx`, or `wake`.
- `voltage`: electrical level. XTEINK X4 uses `3v3`.
- `usedBy`: list of target paths that consume the pin.

Validation should reject:

- unknown GPIO names for the selected MCU
- duplicate exclusive pin ownership
- a pin used for a capability it does not declare
- missing pins referenced by buses or devices

Shared bus pins are allowed when all consumers reference the same bus and each device has a distinct chip-select or compatible sharing rule.

---

## 8. Buses Section

`buses` defines named hardware buses.

The XTEINK X4 display and microSD card share SPI clock and MOSI. The SD card additionally uses MISO.

Example:

```json
{
  "spi": {
    "shared": {
      "sck": "GPIO8",
      "mosi": "GPIO10",
      "miso": "GPIO7",
      "mode": 0,
      "bitOrder": "msb-first",
      "maxFrequencyHz": 40000000,
      "devices": ["display.epd", "storage.sd"]
    }
  }
}
```

The target compiler should validate that every bus pin exists in `pins`, that bus consumers exist in `devices`, and that bus sharing is intentional.

---

## 9. Devices Section

`devices` names onboard hardware at the board level.

Examples:

```json
{
  "display.epd": {
    "type": "epaper-display",
    "controller": "SSD1677",
    "panel": "GDEQ0426T82",
    "bus": "spi.shared",
    "pins": {
      "cs": "GPIO21",
      "dc": "GPIO4",
      "rst": "GPIO5",
      "busy": "GPIO6"
    }
  },
  "storage.sd": {
    "type": "spi-sdcard",
    "bus": "spi.shared",
    "pins": {
      "cs": "GPIO12",
      "miso": "GPIO7"
    }
  }
}
```

The `devices` section describes wiring. Behavior-specific sections such as `display`, `storage`, `input`, and `power` describe how firmware should use that wiring.

LED-like logical devices may describe both the raw GPIO endpoint and the
preferred drive mechanism. For example, an indicator can be a PWM-capable LED
while still naming the GPIO used by raw hardware diagnostics:

```json
{
  "indicator.default": {
    "type": "pwm-led",
    "gpio": "GPIO8",
    "activeLow": true,
    "pwm": {
      "controller": "ledc0",
      "channel": 0,
      "timer": 0,
      "frequencyHz": 1000
    }
  }
}
```

---

## 10. Display Section

`display` describes the display exposed through SquidScript display capabilities.

For XTEINK X4:

- controller: SSD1677
- panel: GDEQ0426T82
- physical resolution: 800 x 480
- logical app coordinate system: 480 x 800, rotated 90 degrees
- supported app pixel formats: `GRAY1_PACKED` and `GRAY2_PACKED`
- SquidScript logical grayscale palette: `gray0` through `gray15`
- supported text font heights and default font height
- supported display render modes: `strip` and `single`, with `strip` as the low-RAM default
- supported SquidScript screen render policies: `compose` and `stream`, with `compose` as the default when a screen omits `render`

Example:

```json
{
  "device": "display.epd",
  "driver": "ssd1677_xteink_x4",
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

The target compiler should generate display constants for dimensions, rotation, driver selection, SPI pins, control pins, logical grayscale levels, pixel formats, color mapping, dithering modes, text font-height support, screen render policies, display render modes, buffer sizes, and refresh capability flags.

SquidScript apps should use logical coordinates. Firmware owns physical panel rotation and packed-pixel conversion.

Runtime apps inspect the active display through `service.display.info()` or the
`display.info()` sugar form. That query returns a cached service record for the
active display binding, including logical dimensions, physical dimensions,
driver, transport, color model, native pixel format, font default, refresh
flags, and current availability. The display capability remains a portable
service API; raw bus and GPIO details stay in target metadata, SQDEVICE records,
and firmware backend code rather than `hardware.*` rendering APIs.

Display SQDEVICE records may select a firmware-supported driver and provide or
override descriptor fields such as `driver`, `transport`, bus/address/pins,
`width`, `height`, `physicalWidth`, `physicalHeight`, `rotation`, `colorModel`,
`nativeBpp`, `nativePixelFormat`, and `defaultFontHeight`. Rebinding
`display.default` validates and probes the descriptor. The firmware image must
already include the selected driver.

SquidScript apps should use logical grayscale colors such as `gray0`, `gray4`, `gray8`, and `gray15`. Firmware maps these values to the selected display pixel format. On displays with fewer native levels than the logical palette, firmware should either map to the nearest native gray or apply a target-supported dithering strategy.

SquidScript apps request text size through `fontHeight` in logical pixels. Firmware maps requested font heights through `service.display.text.fontHeights.selection`. For XTEINK X4, unsupported requested heights are mapped to the nearest supported height.

Screen render policy is app-visible SquidScript intent. `compose` means normal UI composition. `stream` means page- or image-dominant rendering. Display render mode is a firmware/display initialization choice. `strip` means the display service renders into bounded strips and transfers those strips to the EPD. `single` means the display service keeps one full framebuffer for composition before transfer.

If a SquidScript screen omits `render`, firmware should use `rendering.defaultPolicy`. For XTEINK X4 that default is `compose`. Zephyr firmware should initialize the display service with `strip` as the low-RAM default mode, while allowing `single` for debug builds or workflows where the extra RAM is justified. `policyModeMap` is a firmware preference order for mapping app-visible policy to target-supported display modes.

---

## 11. Storage Section

`storage` describes app and content storage.

For XTEINK X4, the microSD card is on the shared SPI bus with CS on GPIO12 and MISO on GPIO7.

Example:

```json
{
  "type": "spi-sdcard",
  "device": "storage.sd",
  "mount": "/sd",
  "filesystem": {
    "required": ["fat32"],
    "optional": ["exfat"]
  },
  "removable": true,
  "cardDetect": null,
  "writeProtect": null,
  "hotSwap": {
    "detect": "io-error-or-remount",
    "requiresUnmountForRemoval": true
  },
  "supportsApps": true,
  "supportsContent": true,
  "maxFileReadSize": 65536
}
```

`maxFileReadSize` must not exceed the SquidScript runtime's exposed file read limit.

`cardDetect` is `null` for XTEINK X4 because the verified public pinouts do not list a card-detect GPIO. `writeProtect` is also `null`. Without a detect switch, firmware cannot receive an immediate hardware eject signal. It must infer removal or replacement from failed I/O, failed periodic probes, remount attempts, or changed volume identity.

This matches the public Papyrix and CrossPoint approaches at the time this reference was written. Papyrix initializes SdFat and returns storage errors such as `SdCardNotFound`/`IOError`; CrossPoint initializes SD storage before normal UI flow and cleans up incomplete web uploads. Neither public implementation relies on a documented XTEINK card-detect GPIO. If a later schematic or board revision exposes a card-detect signal, update this target and mark the older value as superseded.

Targets with a wired card-detect switch should model it explicitly:

```json
{
  "cardDetect": {
    "gpio": "GPIO18",
    "activeLow": true,
    "debounceMs": 50
  }
}
```

Removable storage rules:

- firmware should avoid long-lived open file handles on removable media
- writes should flush and sync before reporting success
- library writes should use temporary files and atomic rename where possible
- on SD I/O failure, firmware should mark affected volumes unavailable
- when no card-detect pin exists, file-manager views should trigger mount/probe retries
- apps should be able to query library or volume status before large operations

Logical libraries and physical volumes are distinct. A logical library such as `books` may merge entries from `sd` and `flash` volumes, but file-manager apps should still be able to display the volume for each entry. Write operations should either specify a volume or use a documented target default.

Flash-backed user libraries are allowed only when the target reserves and mounts an explicit writable flash filesystem partition. Do not infer writable storage from unused firmware image space. Targets may mark flash libraries as `deferred` until the partition table and filesystem are defined.

Upload and installation rules:

- uploads are first written to firmware-managed staging storage
- validation happens after transfer completion and flush, not while only partial chunks are available
- invalid staged files must be deleted or quarantined and must not be published into a library
- BinBook uploads should be validated as BinBook content before publishing
- SquidScript app uploads should use `.sqbc` or `.squid.zip` packages
- uploaded `.sqbc` artifacts are installed only after bytecode and target
  requirement validation
- Wi-Fi HTTP, BLE upload, USB-copy, and SD-card-copy workflows should share the same post-transfer validation and install pipeline even though their transport protocols differ

Example merged library entry:

```json
{
  "name": "example.binbook",
  "path": "/example.binbook",
  "library": "books",
  "volume": "sd",
  "size": 1234567
}
```

---

## 12. Input Section

`input` maps physical inputs to logical keys.

XTEINK X4 has seven buttons:

- `POWER` is a direct GPIO button on GPIO3.
- `BACK`, `SELECT`, `LEFT`, and `RIGHT` are read from an ADC ladder on GPIO1.
- `UP` and `DOWN` are read from an ADC ladder on GPIO2.

The ADC ranges in `targets/xteink-x4.target.json` are sourced from Papyrix `InputManager`. They are firmware calibration values. If hardware variation causes unreliable detection, the target compiler should preserve the values but firmware may need a calibration or tolerance strategy.

Range semantics:

- `minExclusive`: lower ADC bound. `null` means negative infinity.
- `maxInclusive`: upper ADC bound.
- a button is pressed when `minExclusive < adcValue <= maxInclusive`.

Example:

```json
{
  "logical": "SELECT",
  "type": "adc-ladder-button",
  "adc": "GPIO1",
  "range": {
    "minExclusive": 2090,
    "maxInclusive": 3100
  }
}
```

The target compiler should validate that ranges on the same ADC do not overlap and that logical key names are known.

Targets may describe long-press behavior separately from the physical button definition.

Example:

```json
{
  "longPress": [
    {
      "logical": "POWER",
      "durationMs": 2000,
      "owner": "system",
      "action": "sleep"
    }
  ]
}
```

Fields:

- `logical`: logical key name.
- `durationMs`: press duration threshold. The event/action fires when the button has been held for this duration; firmware must not wait for release.
- `owner`: `"system"` or `"app"`.
- `action`: optional system action name, such as `"sleep"`.

Rules:

- Short and long key events are distinct.
- A system-owned long press should not be delivered to app code unless firmware policy explicitly allows it.
- A target may allow short `POWER` presses to reach foreground apps while reserving long `POWER` for sleep.
- Firmware should document whether long press also emits a short press. The default should be no duplicate short press after a long press.
- A threshold-triggered system action such as long-press sleep should execute as soon as the threshold is crossed, even if the user continues holding the button.
- Long press is valid for GPIO buttons, key-matrix keys, and ADC ladder buttons when the input driver can report a stable pressed/released state over time.
- Matrix-key long press depends on the matrix scanner's debounce, ghosting, and rollover behavior. Targets should document any combinations that cannot be detected reliably.
- ADC-ladder long press depends on stable ADC ranges. If multiple simultaneous ADC-ladder buttons collapse into ambiguous values, firmware should treat long press as valid only for unambiguous single-key states.

Targets may also describe key combinations, also called chords.

Example:

```json
{
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

Fields:

- `logical`: list of logical key names in the chord.
- `name`: stable chord ID for diagnostics, simulator UI, and generated firmware constants.
- `owner`: `"system"` or `"app"`.
- `action`: optional system action name.
- `windowMs`: maximum interval between first and last key press for chord recognition.
- `suppressComponentKeys`: whether recognized chords suppress individual short key events.

Chord rules:

- Chords are defined on logical keys, not GPIOs.
- Firmware should emit a chord only when all listed keys are observed as pressed within the target's chord timing window.
- Chords must be validated against what the input hardware can detect.
- GPIO buttons can usually participate in chords when independently readable.
- Matrix-key chords depend on rollover and ghosting behavior.
- ADC-ladder chords are valid only when simultaneous button states produce unambiguous ADC values. If an ADC ladder can only identify one key at a time, chords between keys on that same ladder should be marked unsupported or omitted.
- Chord and long-press precedence must be explicit. System-owned long press, such as long `POWER` sleep, should normally outrank app-owned chords.

---

## 13. Power Section

`power` describes battery monitoring, wake behavior, and sleep policy.

For XTEINK X4:

- battery voltage is read from GPIO0 through a 2:1 voltage divider
- USB detection uses GPIO20, active high
- deep sleep wakes from the power button

Example:

```json
{
  "type": "battery",
  "battery": {
    "monitor": "battery.voltage",
    "emptyMillivolts": 3000,
    "fullMillivolts": 4200
  },
  "usbDetect": "power.usb_detect",
  "deepSleep": true,
  "wakeKeys": ["POWER"]
}
```

Firmware owns battery percentage calculations. The target definition provides only the electrical source and basic voltage bounds.

---

## 14. Runtime Section

`runtime` sets SquidScript VM limits.

The XTEINK X4 has no PSRAM and should use conservative limits.

Example fields:

- `maxBytecodeSize`
- `maxStateVariables`
- `maxSerializedStateSize`
- `maxStringLength`
- `maxFunctionCount`
- `maxInstructionsPerEvent`
- `maxScreenDrawCommands`
- `maxFileReadSize`
- `maxHandles`

The target compiler should emit these limits into firmware and compiler-facing target metadata. `squidc` should use the same limits when compiling apps for the target.

---

## 15. Features and Compatibility

`features` lists runtime and firmware capabilities exposed by this target.
Radio hardware in `radios` does not automatically expose SquidScript runtime
features. For example, an ESP32-C3 target may record MCU-supported Wi-Fi or BLE
with `status: "mcu-supported-runtime-unsupported"` while omitting
`service.wifi.*` or `bleTransfer.*` from `features` until the firmware backend
implements and verifies those services.

Examples:

- `squidscript.bytecode`
- `service.display.draw`
- `display.epaper.ssd1677`
- `buttons`
- `adc-button-ladder`
- `sdcard`
- `content.read`
- `service.wifi.connect`
- `service.wifi.scan`
- `service.wifi.accessPoint`
- `service.wifi.configureIp`
- `service.wifi.setup`
- `httpServer.serve`
- `bleTransfer.receive`
- `binbook.read`

Features and target requirements should be backed by concrete target fields.
Do not use compatibility strings as a substitute for explicit display, input,
storage, runtime-limit, or service capability data.

---

## 16. Firmware Target Compiler Requirements

The firmware target compiler should:

1. Parse the target JSON.
2. Reject unknown `format` values.
3. Validate required top-level sections.
4. Validate GPIO names against the selected MCU.
5. Validate that every referenced pin exists in `pins`.
6. Validate bus references from `devices`.
7. Validate shared bus rules.
8. Validate display dimensions, rotation, logical grayscale levels, pixel formats, color mapping, dithering modes, text font heights, screen render policies, render modes, and buffer sizes.
9. Validate ADC ladder ranges.
10. Validate logical key names.
11. Validate storage limits against runtime limits.
12. Validate firmware update metadata.
13. Validate feature names and target requirement metadata.
14. Generate firmware config artifacts.

The target compiler must not silently guess missing values. Missing required values should be diagnostics.

---

## 17. Generated Firmware Interface

Initial generated firmware configuration should include constants equivalent to the following. Zephyr firmware may emit these as C headers, devicetree overlays, or Kconfig fragments as appropriate.

```c
#define DEVICE_TARGET_ID "xteink-x4"
#define DEVICE_MCU_ESP32C3 1
#define DEVICE_FLASH_SIZE_MB 16

#define DISPLAY_DRIVER_SSD1677_XTEINK_X4 1
#define DISPLAY_PHYSICAL_WIDTH 800
#define DISPLAY_PHYSICAL_HEIGHT 480
#define DISPLAY_LOGICAL_WIDTH 480
#define DISPLAY_LOGICAL_HEIGHT 800
#define DISPLAY_ROTATION 90
#define DISPLAY_SPI_SCK 8
#define DISPLAY_SPI_MOSI 10
#define DISPLAY_SPI_MISO -1
#define DISPLAY_SPI_HZ 40000000
#define DISPLAY_PIN_CS 21
#define DISPLAY_PIN_DC 4
#define DISPLAY_PIN_RST 5
#define DISPLAY_PIN_BUSY 6
#define DISPLAY_LOGICAL_GRAYSCALE_LEVELS 16
#define DISPLAY_COLOR_MAPPING_NEAREST_OR_DITHER 1
#define DISPLAY_DITHERING_NONE 1
#define DISPLAY_DITHERING_ORDERED 1
#define DISPLAY_DITHERING_ERROR_DIFFUSION 1
#define DISPLAY_TEXT_DEFAULT_FONT_HEIGHT 20
#define DISPLAY_TEXT_FONT_HEIGHT_SELECTION_NEAREST 1
#define DISPLAY_RENDER_MODE_STRIP 1
#define DISPLAY_RENDER_MODE_SINGLE 2
#define DISPLAY_DEFAULT_RENDER_MODE DISPLAY_RENDER_MODE_STRIP
#define SCREEN_RENDER_POLICY_COMPOSE 1
#define SCREEN_RENDER_POLICY_STREAM 2
#define SCREEN_DEFAULT_RENDER_POLICY SCREEN_RENDER_POLICY_COMPOSE
#define DISPLAY_STRIP_BUFFER_BYTES 4096
#define DISPLAY_SINGLE_BUFFER_BYTES_1BPP 48000
#define DISPLAY_SINGLE_BUFFER_BYTES_2BPP 96000

#define SD_SPI_SCK 8
#define SD_SPI_MOSI 10
#define SD_SPI_MISO 7
#define SD_PIN_CS 12

#define BUTTON_ADC_1 1
#define BUTTON_ADC_2 2
#define POWER_BUTTON_GPIO 3

#define BATTERY_ADC_GPIO 0
#define USB_DETECT_GPIO 20
```

Richer generated structs may be added when firmware drivers need table-driven configuration, especially for ADC ladder buttons.

Example:

```c
typedef struct {
  const char* logical;
  int adc_gpio;
  int min_exclusive;
  int max_inclusive;
} TargetAdcButtonRange;
```

---

## 18. XTEINK X4 Source Notes

Verified facts in `targets/xteink-x4.target.json` are based on:

- Papyrix X4 specifications: `https://github.com/bigbag/papyrix-reader/blob/main/docs/x4-specifications.md`
- Papyrix display constants: `https://github.com/bigbag/papyrix-reader/blob/main/src/main.cpp`
- Papyrix ADC button ranges: `https://github.com/bigbag/papyrix-reader/blob/main/lib/InputManager/src/InputManager.cpp`
- Papyrix SD CS source: `https://github.com/bigbag/papyrix-reader/blob/main/lib/SDCardManager/src/SDCardManager.cpp`
- Adafruit XTEINK X4 pinout guide: `https://learn.adafruit.com/circuitpython-on-the-xteink-x4-ereader/pinouts`

Current deferred or future-looking items:

- UF2 support is desired but not yet marked verified for XTEINK X4 in the target JSON.
- Exact generated C/C++ struct shapes are illustrative until firmware implementation starts.
- ADC ladder thresholds may need calibration review after testing multiple physical devices.

Any future update to these items should either cite a source or explicitly mark the value as placeholder/deferred.
