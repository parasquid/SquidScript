# SSD1677 And GDEQ0426T82 Agent Reference

This note is a controller and panel integration reference for SquidScript
display work. It is intentionally MCU-agnostic: use it to reason about an
SSD1677-driven Good Display GDEQ0426T82-class panel regardless of whether the
host firmware is Zephyr, C, Rust, ESP-IDF, or another runtime.

Keep three facts separate:

- The MCU chooses GPIO pins, SPI peripheral ownership, scheduling, DMA, and
  power control.
- The SSD1677 controller owns SPI commands, RAM windows, update sequencing,
  waveform lookup tables, BUSY timing, and sleep.
- The panel defines the glass geometry, supported colors, refresh limits, and
  panel-specific waveform/power behavior.

## Display And Controller Facts

Source-backed panel facts for Good Display GDEQ0426T82-T01C:

- Panel size: 4.26 inch.
- Resolution: 800 x 480 pixels.
- Display colors: black and white.
- Interface: SPI.
- Driver IC: SSD1677.
- Partial refresh is listed by Good Display for this panel.

Source-backed SSD1677 protocol facts:

- The controller supports SPI host access. Prefer 4-wire SPI for integration:
  `SCK`, `DIN`/`MOSI`, `CS`, and `DC`, plus `RST` and `BUSY`.
- `BUSY` is active-high in the SSD1677 datasheet. Treat high as "controller is
  busy" unless a specific board inverts it externally.
- The SSD1677 datasheet gives a maximum SPI write clock of 20 MHz and a maximum
  SPI read clock of 2.5 MHz. Start below the limit during integration, then
  raise only after reset, RAM writes, and refresh are stable.
- `0x24` writes BW RAM. `0x26` writes RED RAM. On black/white panels, public
  drivers commonly use the two RAM planes for previous/current frame data,
  differential updates, or 2-bit grayscale modes.
- `0x20` triggers master activation. `0x22` selects display update behavior.
- `0x12` performs software reset. `0x10` enters deep sleep.
- `0x11`, `0x44`, `0x45`, `0x4E`, and `0x4F` control data entry mode, RAM
  window ranges, and RAM address counters.

Do not generalize SSD1677 command bytes to other e-paper controllers. Some
controllers reuse opcode values with different meanings.

## Integration Checklist

Use staged integration so display, controller, and scheduler problems are easy
to separate:

1. Confirm electrical wiring and voltage levels before attaching firmware:
   power, ground, `SCK`, `DIN`/`MOSI`, `CS`, `DC`, `RST`, and `BUSY`. `SDO` or
   `MISO` is optional for many write-only display paths.
2. Configure SPI mode and clock conservatively. Keep chip-select ownership in
   one driver layer so shared SPI devices do not interleave transactions.
3. Toggle hardware reset, send `0x12`, and wait for `BUSY` to deassert with a
   bounded timeout. A timeout should fail visibly and leave serial/runtime
   polling responsive.
4. Set panel-specific controller configuration from a source-backed driver or
   datasheet sequence. Mark any copied waveform, voltage, booster, or border
   values as panel-specific until verified.
5. Set a full-screen RAM window, clear both RAM planes, and perform a full
   refresh. Do not start with partial refresh or custom LUTs.
6. Write a known 1-bit test pattern to BW RAM and refresh. Use simple patterns:
   all white, all black, vertical stripes, horizontal stripes, and a border.
7. Add windowed writes only after full-screen writes are correct. Verify x/y
   addressing, byte ordering, scan direction, and off-by-one window endpoints.
8. Add fast, partial, or grayscale refresh modes only after ordinary black/white
   full refresh is reliable.
9. Enter deep sleep after refresh only when the power model requires it. A
   sleeping controller usually needs reset or reinitialization before the next
   update.

For SquidScript firmware work, avoid blocking the main loop while waiting for
e-paper refreshes. Long refreshes should become bounded polling, scheduled
steps, or explicit async progress rather than hidden busy loops.

## RAM And Framebuffer Strategy

An 800 x 480 one-bit plane is 48,000 bytes. Two one-bit planes are 96,000
bytes. Two framebuffer planes plus separate grayscale LSB/MSB planes can exceed
small-MCU RAM budgets quickly.

Prefer driver APIs that can stream rows or strips into SSD1677 RAM windows:

- A full framebuffer is simple, but it is expensive resident RAM.
- A single current framebuffer can work for basic full refreshes but complicates
  fast differential updates because the controller or firmware still needs a
  previous/current concept.
- Separate LSB/MSB grayscale planes are useful for grayscale modes, but they
  should be optional and caller-owned.
- Strip writes are the preferred low-RAM path for SquidScript firmware:
  render or decode a bounded band, set the SSD1677 RAM window, write the band to
  BW or RED RAM, then reuse the same scratch buffer for the next band.

Acceptance criteria for adopting a display driver in constrained firmware:

- It must allow caller-owned buffers or bounded strip/window writes.
- It must not require a hidden full-screen framebuffer for every update path.
- Its BUSY wait path must be bounded or adaptable to a nonblocking scheduler.
- Any heap allocation must be explicit and avoid per-refresh full-screen
  temporaries.
- Custom LUT support should be available for later grayscale and refresh-mode
  work, but full refresh with source-backed panel defaults must work first.

## Rust Library Guidance

Evaluate existing Rust crates before writing a custom driver, but keep the
result subordinate to the SquidScript firmware path.

Recommended first implementation option: `ssd1677-driver`

- Pros: `no_std`, uses `embedded-hal` 1.0, documents 4-wire SPI, and documents
  testing on `GDEQ0426T82`.
- Pros: focused on this controller family rather than a broad e-paper catalog.
- Risks: current crate docs describe monochrome support and say async is not
  implemented.
- Adoption test: confirm whether the lower-level API can write bounded strips or
  can be wrapped without retaining a full framebuffer. If the high-level
  display API forces a full framebuffer, use only the lower-level pieces or move
  to the next option.

Second implementation option: `ssd1677`

- Pros: `no_std`, uses `embedded-hal` 1.0, supports configurable dimensions,
  custom LUTs, rotation, and full/fast refresh concepts.
- Risks: less direct public evidence for GDEQ0426T82 hardware than
  `ssd1677-driver`.
- Adoption test: confirm geometry, RAM window semantics, BW/RED plane writes,
  and whether update paths can operate on caller-owned data without hidden
  full-frame allocation.

Reference-only implementation option: `epd-waveshare`

- Pros: mature e-paper crate family and useful design reference.
- Risks: its primary surface targets Waveshare module families; do not assume
  exact SSD1677/GDEQ0426T82 compatibility without confirming a matching module
  and command sequence.

Non-Rust reference: GxEPD2

- GxEPD2 is a mature C++ e-paper reference and includes support for
  `GDEQ0426T82` with SSD1677. Use it to compare command sequences, refresh
  modes, and LUT handling, not as a direct Rust dependency.

Strong recommendation: evaluate `ssd1677-driver` first and apply the low-RAM
acceptance criteria before adopting it. If it cannot support strip/window
writes without a full resident framebuffer, inspect `ssd1677` next. Write a
custom driver only after both crates fail on concrete requirements.

## License Notes

SquidScript is licensed under AGPLv3. License compatibility is therefore a
practical implementation constraint, not only attribution housekeeping.

Current license findings:

- `ssd1677-driver`: MIT. Compatible with AGPLv3; preserve notices.
- `ssd1677`: MIT. Compatible with AGPLv3; preserve notices.
- `epd-waveshare`: ISC. Compatible with AGPLv3; preserve notices.
- CrossPoint: MIT. Compatible with AGPLv3; preserve notices if code is copied
  or adapted.
- `open-x4-sdk` `EInkDisplay`: MIT. Compatible with AGPLv3; preserve notices if
  code is copied or adapted.
- Papyrix: MIT. Compatible with AGPLv3; preserve notices if code is copied or
  adapted.
- GxEPD2: GPLv3. GPLv3 code is compatible with AGPLv3, but copying code into
  SquidScript means the combined work must follow AGPLv3 obligations. Prefer
  using GxEPD2 as a behavioral reference unless there is a clear reason to copy
  implementation details.
- TernOS: the repository contains GPLv2 license text and no project-specific
  `GPL-2.0-or-later` declaration was found in the inspected metadata or driver
  source. Treat it conservatively as GPL-2.0-only unless the author clarifies
  otherwise. GPL-2.0-only code is not compatible with AGPLv3, so do not copy
  TernOS code directly into SquidScript.

Using public projects as behavioral references is different from copying code.
When implementing SquidScript firmware, prefer permissively licensed Rust crates
or a fresh implementation from datasheet/source-backed behavior notes.

## Backend Implementation Requirements

The first implementation must establish the low-RAM driver shape before wiring
it into the portable SquidScript display service. Do not expose a
full-framebuffer API as the target backend contract.

This implementation is for SquidScript. A narrow diagnostic harness is
acceptable only when it directly supports a SquidScript firmware or service
implementation. Do not create a separate Rust firmware project, board demo, or
display app as an end in itself. If Rust crates are evaluated, treat them as
driver research for SquidScript's firmware path; the current canonical
ESP32-C3 firmware lives under `firmware/zephyr`, so a Rust crate is not
automatically a drop-in target dependency.

Use this driver shape for the target backend:

```rust
pub enum Ssd1677RefreshMode {
    Full,
    Fast,
    Partial,
}

pub enum Ssd1677BusyState {
    Busy,
    Ready,
    Timeout,
}

pub trait Ssd1677Panel {
    type Error;

    fn init(&mut self) -> Result<(), Self::Error>;
    fn set_full_window(&mut self) -> Result<(), Self::Error>;
    fn set_window(&mut self, x: u16, y: u16, width: u16, height: u16)
        -> Result<(), Self::Error>;
    fn write_bw_strip(&mut self, y: u16, rows: &[u8]) -> Result<(), Self::Error>;
    fn write_red_strip(&mut self, y: u16, rows: &[u8]) -> Result<(), Self::Error>;
    fn refresh(&mut self, mode: Ssd1677RefreshMode) -> Result<(), Self::Error>;
    fn poll_busy(&mut self) -> Result<Ssd1677BusyState, Self::Error>;
    fn sleep(&mut self) -> Result<(), Self::Error>;
}
```

The names are illustrative, not a committed Rust API. The required behavior is
the contract: init, set a RAM window, write bounded BW/RED strips, trigger a
refresh, poll BUSY without monopolizing the firmware loop, and sleep.

Backend acceptance checks:

1. Initialize the controller and clear both SSD1677 RAM planes to white.
2. Refresh full-screen white.
3. Draw a black border using strip/window writes only.
4. Draw alternating vertical and horizontal patterns using the same bounded
   scratch buffer.
5. Refresh and verify the pattern on the physical panel.
6. Enter sleep, reinitialize, clear, and repeat the border/pattern sequence.
7. Confirm the backend never requires a resident 48,000-byte full-screen
   framebuffer in firmware state.

RAM budget defaults:

- Start with a scratch buffer for one to eight rows.
- For an 800-pixel-wide one-bit strip, one row is 100 bytes and eight rows are
  800 bytes.
- Do not add a 48,000-byte full-screen framebuffer, 96,000-byte double buffer,
  or separate full-screen grayscale LSB/MSB buffers unless the user explicitly
  approves that RAM tradeoff.
- Any allocation larger than the strip scratch buffer must be called
  out in the implementation notes and replaced before the driver becomes a
  normal firmware service.

Scheduler rule:

- A hardware-only diagnostic may use a blocking BUSY wait only if it is clearly
  marked as diagnostic-only and not wired into the service path.
- The service-quality implementation must use bounded polling or a step state
  machine so serial input, timers, and VM/runtime polling remain responsive.
- Timeout handling must surface a display error instead of spinning forever.

Implementation decisions to resolve:

- Does `ssd1677-driver` expose enough low-level RAM window and RAM plane writes,
  or does it need a wrapper/fork?
- Does `ssd1677` expose a better strip/window path if `ssd1677-driver` is too
  framebuffer-oriented?
- Is partial refresh acceptable on the actual panel after repeated updates, or
  does it ghost too heavily for the intended UI?
- Is 2-bit grayscale part of the initial backend, or is the initial backend
  black/white-only until full refresh and strip writes are stable?

The canonical XIAO e-paper dev setup is a XIAO ESP32-C3 directly wired to the
Good Display DESPI-C02 connector board and GDEQ0426T82 panel. The following
display wiring is verified for that setup. Boot-risk notes come from the XIAO
ESP32-C3 strapping-pin guidance.

| SSD1677 signal | MCU pin | Source | Notes |
| --- | --- | --- | --- |
| `VCC` | XIAO 3V3 | DESPI-C02 verified wiring | Power from the XIAO 3.3 V rail, not 5 V. Confirm the display FPC and board are powered from the expected rail before first refresh. |
| `GND` | XIAO GND | DESPI-C02 verified wiring | Shared ground. |
| `SCK` | D8 / GPIO8 | DESPI-C02 verified wiring + XIAO pin map | Shared SPI clock. GPIO8 is an ESP32-C3 strapping pin; confirm external wiring does not pull it into an invalid boot state. |
| `DIN`/`MOSI`/`SDI` | D10 / GPIO10 | DESPI-C02 verified wiring + XIAO pin map | Shared SPI data from MCU to controller. DESPI-C02 labels this signal `SDI`. |
| `CS` | D1 / GPIO3 | DESPI-C02 verified wiring + XIAO pin map | Display chip select. |
| `DC` | D3 / GPIO5 | DESPI-C02 verified wiring + XIAO pin map | Command/data select. |
| `RST` | D0 / GPIO2 | DESPI-C02 verified wiring + XIAO pin map | Hardware reset. GPIO2 is an ESP32-C3 strapping pin; confirm reset wiring does not block normal boot. |
| `BUSY` | D2 / GPIO4 | DESPI-C02 verified wiring + XIAO pin map | Active-high at the tested adapter pin. |
| `SDO`/`MISO` | Not connected by default | Target decision | The initial SquidScript display path is write-only. Do not route display reads through GPIO9/BOOT by default. |

The planned external SD reader shares display `SCK` and `MOSI` through direct
external wiring. SD `MISO` and `CS` are not part of the verified display wiring
and must remain unverified in target metadata until the physical jumper choices
are confirmed.

Verified XIAO ESP32-C3 + DESPI-C02 + GDEQ0426T82 diagnostic wiring:

| DESPI-C02 signal | XIAO pin | GPIO | Notes |
| --- | --- | --- | --- |
| `BUSY` | D2 | GPIO4 | BUSY asserted during software reset and refresh, then returned ready. Current bench wire color: white. |
| `RES`/`RST` | D0 | GPIO2 | Hardware reset. GPIO2 is an ESP32-C3 strapping pin; avoid external pulls that block boot. |
| `D/C` | D3 | GPIO5 | Command/data select. |
| `CS` | D1 | GPIO3 | Chip select. |
| `SCK` | D8 | GPIO8 | SPI clock. GPIO8 is an ESP32-C3 strapping pin; avoid external pulls that block boot. |
| `SDI` | D10 | GPIO10 | MCU-to-display data, equivalent to `DIN`/`MOSI`. |
| `GND` | GND |  | Shared ground. |
| `3.3V` | 3V3 |  | Power from the XIAO 3.3 V rail, not 5 V. |

Current bench wire colors:

| Wire color | Signal | XIAO pin | GPIO | Status |
| --- | --- | --- | --- | --- |
| black | shared `GND` | `GND` |  | verified display wiring; shared with SD |
| yellow | shared `3.3V` | `3V3` |  | verified display wiring; shared with SD |
| orange | shared `SCK` | `D8` | `GPIO8` | verified display wiring; wired as SD SCK candidate |
| green | shared `SDI`/`MOSI` | `D10` | `GPIO10` | verified display wiring; wired as SD MOSI candidate |
| blue | e-paper `CS` | `D1` | `GPIO3` | verified display wiring |
| different blue | SD `CS` | `D4` | `GPIO6` | wired candidate; not firmware-verified yet |
| violet | e-paper `D/C` | `D3` | `GPIO5` | verified display wiring |
| violet | SD `MISO` | `D5` | `GPIO7` | wired candidate; not firmware-verified yet |
| grey | e-paper `RST`/`RES` | `D0` | `GPIO2` | verified display wiring |
| white | e-paper `BUSY` | `D2` | `GPIO4` | verified display wiring |

The confirmed smoke-test command path is implemented in
`tests/hardware/xiao-esp32c3/epaper-hello`. It uses direct GPIO bit-banged SPI
rather than the Zephyr SPI peripheral so the smoke test can isolate display
wiring, panel command sequencing, and BUSY behavior. The working initialization
sequence uses software reset (`0x12`), temperature sensor selection (`0x18`
with `0x80`), booster soft start (`0x0c` with
`0xae, 0xc7, 0xc3, 0xc0, 0x80`), gate output control (`0x01` with gate end
479 and scan byte `0x02`), entry mode (`0x11` with `0x03`), border waveform
(`0x3c` with `0x01`), full RAM windows (`0x44` for X `0..799`, `0x45` for Y
`0..479`), BW RAM write (`0x24`), update control (`0x22` with `0xf7`), and
master activation (`0x20`). The smoke test writes one row buffer at a time and
maps logical X to panel RAM X in reverse so rendered text is not mirrored.
`BUSY` is active-high at the tested adapter pin.

The default XIAO firmware uses the Zephyr SPI driver with `CS`, `D/C`, `RST`,
and `BUSY` as GPIOs. The primitive display path renders
`service.display.clear` and `service.display.text` to physical pixels and keeps
other primitives drawlog-only. The primitive renderer keeps a row-sized buffer,
applies the target JSON's default portrait logical orientation (`480 x 800`,
rotation `270`) to each draw op, maps physical X to panel RAM X in reverse,
writes BW RAM (`0x24`), and refreshes with the black/white full-update path.

The BinBook drawable path accepts target-native full-panel `GRAY2_PACKED`
resources and streams the compressed page twice without allocating a full-screen
framebuffer. It writes the two one-bit SSD1677 grayscale planes to RED RAM
(`0x26`) and BW RAM (`0x24`), loads the panel-specific 4-gray LUT with `0x32`,
sets the LUT-derived gate/source/VCOM voltage registers (`0x03`, `0x04`,
`0x2c`), enables normal RED RAM handling with display update control `0x21`
data `0x00, 0x00`, and refreshes with update control byte `0xc7` before
master activation (`0x20`). Canonical BinBook GRAY2 values are mapped to the
SSD1677 plane states in ascending order: `0=black`, `1=dark gray`,
`2=light gray`, and `3=white`.

The verified unattended firmware activity signal is a completed display refresh
with an empty `device errors` response. Hardware validation for BinBook GRAY2
also checks `device output` for `pages <n>`, `device drawlog` for
`draw=binbook id=<n>`, and resource metrics for healthy VM/protocol stack
headroom.

`EPAPER_HELLO_READY` proves that the diagnostic app booted, reached display
refresh, and observed BUSY complete. It does not prove the visible pixels. The
GDEQ0426T82 path used here is write-only: there is no display framebuffer
readback over the connected SPI signals, and BUSY does not report pixel
orientation, contrast, FPC seating, or whether the final image is mirrored. For
unattended smoke checks, BUSY asserting during refresh and returning ready is
the current proxy for e-paper activity. A host-visible USB webcam can provide a
coarse autonomous visible-output check for blank-vs-rendered and multi-band
grayscale output when the panel is close, well lit, and placed against a
high-contrast background. Human visual confirmation remains the more reliable
physical-display pass criterion for exact tone order, contrast, and subtle
grayscale differences.

Minimum SSD1677 command skeleton for the backend:

1. Hardware reset: drive `RST` high, low, then high with panel-source-backed
   delays.
2. Send `0x12` software reset and poll `BUSY` until ready or timeout.
3. Apply a source-backed panel initialization sequence. Mark each booster,
   voltage, temperature, border, and LUT value as datasheet-derived,
   driver-derived, or locally measured.
4. Send `0x11` data-entry mode.
5. Send `0x44` and `0x45` for the RAM x/y range.
6. Send `0x4E` and `0x4F` for the RAM x/y counters.
7. Send `0x24` and stream a bounded BW strip.
8. Send `0x26` and stream a bounded RED/previous-frame strip when the refresh
   mode needs the second plane.
9. Send `0x22` with the selected update-control byte, then `0x20` master
   activation.
10. Poll `BUSY` until ready or timeout.
11. Send `0x10` deep sleep only when the power model requires it; document
    whether the next operation must reinitialize the controller.

The skeleton intentionally omits raw initialization byte values. The
implementation must import those from the SSD1677 datasheet, Good Display panel
docs, a permissively licensed reference, or local measurement, and must record
the source.

Host-test the command layer before flashing hardware. Use a fake SPI/GPIO/delay
backend to assert:

- `init()` performs reset, software reset, BUSY polling, source-backed panel
  setup, and full-window setup in the expected order.
- `set_window(...)` rejects zero sizes, out-of-bounds windows, and ranges that
  are not byte-aligned where byte alignment is required.
- `write_bw_strip(...)` sends `0x24` and exactly the caller-owned strip bytes.
- `write_red_strip(...)` sends `0x26` and exactly the caller-owned strip bytes.
- `refresh(...)` sends `0x22`, the selected update-control byte, `0x20`, and
  then enters bounded BUSY polling.
- `sleep()` sends `0x10` and records that a later update needs reinit if the
  chosen controller state machine requires it.
- BUSY timeout returns a typed error and never loops forever.

Use this error taxonomy unless the implementation has a better existing local
pattern:

- SPI or bus transfer error.
- GPIO/reset/control-pin error.
- BUSY timeout.
- Invalid RAM window.
- Strip length does not match the selected window.
- Unsupported refresh mode for the current implementation.
- Controller state error, such as writing while asleep or refreshing before
  initialization.

Final implementation checklist:

- The implementation serves a SquidScript target, firmware service, hardware
  diagnostic, simulator fixture, or test path.
- No full-screen framebuffer is introduced as resident firmware state without
  explicit user approval.
- No unbounded BUSY wait exists in the service-quality path.
- Serial input, timers, VM/runtime polling, and hardware command handling remain
  responsive during display refresh.
- All physical pins are sourced from target metadata, wiring notes, or measured
  hardware evidence; no guessed pins are encoded.
- GPLv2-only reference code is not copied.
- The target-facing docs and roadmap are updated when the implementation
  resolves one of the listed decisions.

## Implementation Notes

- Keep portable SquidScript display semantics separate from physical controller
  details. SSD1677 commands and panel waveforms belong in target firmware or a
  target display backend, not in compiler core.
- Treat refresh as a long-running device operation. A service API should expose
  progress or schedule bounded work instead of monopolizing serial/runtime
  polling.
- Use target metadata for physical wiring and capabilities. This document is
  not a pinout source for any board.
- Before encoding panel values, preserve whether each value came from the
  datasheet, Good Display panel page, a public driver, or local measurement.
- For grayscale modes, document whether the mode is native panel support,
  a controller-plane trick, a custom LUT approximation, or an application-level
  dithering strategy.

## Named Reference Implementations

These public projects are useful implementation references, but their hardware
choices and application constraints should not define SquidScript's portable
display API.

CrossPoint (`crosspoint-reader/crosspoint-reader`)

- License: MIT.
- C++/PlatformIO application using an `EInkDisplay` library from `open-x4-sdk`.
- Builds with `EINK_DISPLAY_SINGLE_BUFFER_MODE=1`.
- The SDK display layer defines SSD1677 BW RAM (`0x24`) and RED RAM (`0x26`)
  writes.
- It exposes `writeGrayscalePlaneStrip(...)`, which is important precedent for
  low-RAM SSD1677 strip/window grayscale writes.
- It also contains whole-window helpers that allocate temporary buffers; treat
  those as cautionary examples for constrained RAM.

Papyrix (`bigbag/papyrix-reader`)

- License: MIT.
- C++/PlatformIO application with an in-repository `EInkDisplay` SSD1677 driver.
- Builds with `EINK_DISPLAY_SINGLE_BUFFER_MODE=1`.
- Its display driver is based on the GxEPD2 `GDEQ0426T82` implementation.
- Its rendering documentation describes a fixed maximum display buffer and a
  grayscale flow that writes LSB data to BW RAM, MSB data to RED RAM, then
  refreshes with a grayscale LUT.
- Its generated/derived SSD1677 guide explicitly says to verify critical
  details against the official datasheet; use the code and docs as reference
  material, not as source truth.

TernOS (`azw413/TernOS`)

- License: GPLv2 text in `LICENSE.txt`; treat as GPL-2.0-only unless clarified.
- Rust application/core with an X4 target-specific custom Rust SSD1677 driver.
- The target uses `esp-hal`, `embedded-hal` 1.0, `embedded-hal-bus`, and
  `embedded-graphics`, but does not depend on the published `ssd1677` or
  `ssd1677-driver` crates.
- The custom driver implements SSD1677 command constants, reset, BUSY waits,
  RAM windows, BW/RED plane writes, custom LUTs, deep sleep, and 4-level
  grayscale paths.
- It keeps two 48,000-byte monochrome framebuffers and allocates grayscale
  planes when needed. That is useful as a working Rust reference, but not a
  low-RAM design to copy directly.

## Sources

- SSD1677 datasheet: `https://www.e-paper-display.com/SSD1677Specification.pdf`
- Good Display GDEQ0426T82-T01C product page:
  `https://www.good-display.com/product/371.html`
- `ssd1677-driver`: `https://docs.rs/ssd1677-driver`
- `ssd1677`: `https://docs.rs/ssd1677`
- `epd-waveshare`: `https://docs.rs/epd-waveshare`
- GxEPD2: `https://github.com/ZinggJM/GxEPD2`
- CrossPoint: `https://github.com/crosspoint-reader/crosspoint-reader`
- Papyrix: `https://github.com/bigbag/papyrix-reader`
- TernOS: `https://github.com/azw413/TernOS`
