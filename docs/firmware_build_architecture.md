# SquidScript Firmware Build Architecture

Status: Draft
Purpose: Define how SquidScript firmware builds are orchestrated across vendor SDKs, hardware targets, and simulator backends.

---

## 1. Purpose

SquidScript should support multiple firmware targets without making one vendor SDK, IDE, or board ecosystem the center of the project.

The project should provide a stable repo-level build interface for developers while using the correct native backend for each hardware family.

Examples:

- XTEINK X4 on ESP32-C3
- Xiao BLE or nRF52840 dongle on nRF52
- browser-based simulator for trying SquidScript apps without hardware

The top-level SquidScript build tooling should coordinate target definitions, generated firmware config, app compilation, firmware builds, flashing, and simulator bundles.

---

## 2. Goals

- Keep `targets/*.target.json` as the canonical description of device hardware and capabilities.
- Provide an Espruino-like developer experience with simple build commands.
- Use native vendor SDKs underneath instead of hiding major platform differences.
- Avoid PlatformIO as a canonical build dependency.
- Make simulator builds first-class so SquidScript apps can be tested in a browser.
- Keep generated backend artifacts out of hand-maintained target definitions.
- Let firmware services, SquidScript, mruby, native apps, and simulator code share the same platform capability model.

---

## 3. Non-Goals

- Do not create a universal replacement for ESP-IDF, Zephyr, or Nordic tooling.
- Do not require PlatformIO.
- Do not make Arduino the primary firmware architecture.
- Do not make every backend expose identical low-level hardware APIs.
- Do not require production firmware to parse target JSON at runtime.

---

## 4. Top-Level Build UX

The repo should expose stable commands through a top-level `Makefile` or equivalent small wrapper.

Examples:

```sh
make TARGET=xteink-x4 firmware
make TARGET=xteink-x4 flash
make TARGET=xteink-x4 app APP=examples/binbook-reader
make TARGET=xteink-x4 package

make TARGET=xiao-ble firmware
make TARGET=nrf52840-dongle firmware

make TARGET=browser-sim app APP=examples/binbook-reader
make TARGET=browser-sim serve
```

The wrapper should:

1. Load the target definition.
2. Validate it.
3. Resolve the backend.
4. Generate backend-specific config files.
5. Invoke the backend-native build tool.
6. Collect firmware/app/simulator outputs in predictable output paths.

---

## 5. Backend Model

A backend is the build/runtime family used to implement a target.

The target definition should eventually include a backend identifier or resolve to one through target tooling.

Example backend IDs:

```text
esp-idf
zephyr-nrf
browser-sim
native-host-sim
```

Backend selection is a build-time concern. SquidScript apps should still target capabilities such as `display.draw`, `library.books.read`, `wifi.accessPoint`, or `bleTransfer.receive`.

---

## 6. ESP32-C3 Backend

The ESP32-C3 backend should use ESP-IDF directly.

Backend ID:

```text
esp-idf
```

Native tool:

```text
idf.py
```

Generated artifacts may include:

- `target_config.h`
- static C/C++ target config structs
- `sdkconfig.defaults`
- `partitions.csv`
- generated firmware manifest metadata
- generated app capability tables

Expected outputs:

- app firmware `.bin`
- bootloader `.bin`
- partition table `.bin`
- optional merged flash image
- optional UF2 only when the selected bootloader/update path is verified to support it

XTEINK X4 should use this backend unless future hardware evidence points to a different firmware stack.

---

## 7. nRF52 Backend

The forward-looking nRF52 backend should use Nordic's nRF Connect SDK, which is Zephyr-based.

Backend ID:

```text
zephyr-nrf
```

Native tool:

```text
west
```

Generated artifacts may include:

- `target_config.h`
- Zephyr devicetree overlay
- Kconfig fragment
- board overlay
- partition/flash map fragments
- generated firmware manifest metadata
- generated app capability tables

Expected outputs:

- `.elf`
- `.hex`
- `.bin`
- optional signed image when MCUBoot is used
- optional UF2 when the board bootloader supports UF2

The older Nordic nRF5 SDK is in maintenance mode. It may remain useful for simple legacy nRF52 targets, but it should not be the default backend for new SquidScript nRF52 work unless a specific board or bootloader requires it.

---

## 8. Browser Simulator Backend

The browser simulator should be a first-class backend, not just a debug afterthought.

Backend ID:

```text
browser-sim
```

Purpose:

- run SquidScript apps without device hardware
- test launcher, reader, upload, and file-manager flows
- exercise target compatibility checks
- preview display rendering at target resolution
- simulate buttons, storage, Wi-Fi/AP status, BLE upload events, and file uploads
- support browser/WASM compiler workflows

The simulator should use the same target definition model as firmware targets. It may provide simulated devices instead of physical drivers.

Example simulated target:

```text
targets/browser-sim-xteink-x4.target.json
```

The browser simulator should model:

- logical display dimensions and pixel formats
- e-ink refresh modes as visual or timing hints
- target logical keys
- optional board layout metadata for drawing buttons, screens, LEDs, ports, labels, and device outline in the correct positions
- app state persistence
- target-defined libraries such as `books`, `apps-inbox`, and `appdata`
- upload staging and post-transfer validation
- app package installation from `.squidapp.zip`
- Wi-Fi/AP status records for UI testing
- BLE transfer events for UI testing

The simulator should not pretend to validate hardware timing, power draw, flash endurance, radio performance, or real SD-card failure behavior. Those remain firmware/hardware concerns.

### 8.1 Board Layout Metadata

The simulator should be able to render a board or device from target-adjacent layout metadata, similar in spirit to Espruino's board-definition information.

Layout metadata is not electrical truth. It is presentation metadata for simulators, documentation, screenshots, and web tools.

It should describe what the user sees:

- device outline
- screen rectangle
- physical buttons
- LEDs
- ports
- labels
- optional hit targets for mouse/touch simulation
- optional board/device background image

It should not replace:

- `pins`
- `buses`
- `devices`
- `input`
- `display`
- electrical active-high/active-low behavior
- ADC ladder ranges

Recommended source layout:

```text
targets/xteink-x4.target.json
targets/layouts/xteink-x4.layout.json
```

The target file may reference the layout:

```json
{
  "simulator": {
    "layout": "targets/layouts/xteink-x4.layout.json"
  }
}
```

The layout should use stable logical IDs that refer back to target-defined devices and inputs.

Example:

```json
{
  "format": "squid-layout-v1",
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
    },
    {
      "id": "usb-c",
      "kind": "port",
      "port": "usb-c",
      "x": 300,
      "y": 1050,
      "width": 120,
      "height": 18
    }
  ]
}
```

The browser simulator should use `kind: "button"` elements to generate pointer/touch hit targets for `onKey(...)` events. The simulator must still use the target's `input` section as the source of which logical keys exist.

---

## 9. Native Host Simulator Backend

A native host simulator may also be useful for CI and fast tests.

Backend ID:

```text
native-host-sim
```

Purpose:

- run VM and capability tests without a browser
- validate `.sqbc` bytecode
- run golden fixtures
- test library, upload, and package installation behavior
- support CI environments where browser automation is not needed

The browser simulator and native host simulator may share core VM and platform-service logic where practical.

---

## 10. PlatformIO Policy

PlatformIO should not be the canonical build system.

Reasons:

- it is an orchestration layer over vendor SDKs, not the source of platform behavior
- it can pin or lag SDK versions
- it adds another project model on top of ESP-IDF and Zephyr
- it is not needed for the repo-level build UX

PlatformIO support may be added later as an optional wrapper for contributors who prefer it, but docs, CI, target definitions, generated artifacts, and release builds should not depend on PlatformIO.

---

## 11. Target Definition Integration

The target definition is the source of truth for:

- MCU family
- pins and buses
- display
- input
- storage
- power
- runtime limits
- app-visible features
- compatibility strings
- firmware update metadata

The firmware build wrapper should never guess missing hardware values. Placeholder, guessed, or unverified values must be explicitly marked in the target definition.

Example flow:

```text
targets/xteink-x4.target.json
  -> target validator
  -> resolved target model
  -> backend generator
  -> ESP-IDF generated files
  -> idf.py build
```

Simulator flow:

```text
targets/browser-sim-xteink-x4.target.json
  -> target validator
  -> resolved target model
  -> simulator config
  -> browser app bundle
```

---

## 12. Generated Files

Generated files should be reproducible from source target definitions and build inputs.

Generated files should normally live under a build directory such as:

```text
build/generated/<target>/
```

Examples:

```text
build/generated/xteink-x4/target_config.h
build/generated/xteink-x4/sdkconfig.defaults
build/generated/xteink-x4/partitions.csv

build/generated/xiao-ble/target_config.h
build/generated/xiao-ble/app.overlay
build/generated/xiao-ble/prj.conf

build/generated/browser-sim-xteink-x4/target.json
build/generated/browser-sim-xteink-x4/app_manifest.json
```

Hand-edited source files should stay in `targets/`, `docs/`, `firmware/`, `compiler/`, or `simulator/` rather than in generated output directories.

---

## 13. Relationship To Device Drivers

Device drivers should be firmware-native code.

SquidScript, mruby, and browser-simulator apps should call platform services, not own hardware drivers directly.

Layering:

```text
Apps
  SquidScript app
  optional mruby app
  native C/C++ app

Language bindings
  SquidScript built-ins
  Ruby modules/classes
  native app service API

Platform services
  display
  input
  libraries/storage
  upload/install
  Wi-Fi
  BLE transfer
  BinBook
  power

Backend/device drivers
  ESP-IDF drivers
  Zephyr/nRF drivers
  browser simulator devices
```

Low-level GPIO/I2C/SPI APIs may be added later for targets that explicitly expose user-available pins or buses. They should be target-declared and should not conflict with display, SD, buttons, battery, USB detect, or other reserved hardware.

---

## 14. Release Artifacts

Firmware release artifacts should be backend-specific but collected consistently.

ESP-IDF target release example:

```text
dist/xteink-x4/firmware.bin
dist/xteink-x4/bootloader.bin
dist/xteink-x4/partition-table.bin
dist/xteink-x4/manifest.json
```

nRF target release example:

```text
dist/xiao-ble/firmware.hex
dist/xiao-ble/firmware.bin
dist/xiao-ble/manifest.json
```

Browser simulator release example:

```text
dist/browser-sim/index.html
dist/browser-sim/assets/
dist/browser-sim/manifest.json
```

UF2 should be emitted only for targets whose bootloader/update path is verified to support UF2.

---

## 15. Open Questions

- Should backend selection be stored directly in `targets/*.target.json`, or in a separate build-target registry?
- Should browser simulator targets be exact simulated hardware targets or generic compatibility targets?
- Should the browser simulator run the production Rust/WASM compiler in-browser, or consume precompiled `.sqbc` first?
- Should simulator storage use IndexedDB, in-memory storage, local files through browser APIs, or all three?
- Should BLE transfer simulation model throughput/timing, or only event shape and staged upload behavior?
- Should nRF52 support start with nRF52840-class boards before smaller nRF52832-class devices?
