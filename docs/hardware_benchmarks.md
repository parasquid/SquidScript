# Hardware Benchmarks

This document defines portable hardware benchmark contracts for SquidScript
firmware targets. Target-specific scripts may vary in flashing, serial-port
detection, and board setup, but benchmark output fields should stay stable so
ESP32-C3 Super Mini, nRF52, and RP2350 runs can be compared later.

## Lazy-Load Screen Transition

Purpose: measure foreground VM screen transition cost when installed SQBC is
read lazily from target storage instead of fully resident in RAM.

The benchmark app must contain about 10 small screens. It should start on the
first screen, arm a foreground timer, then repeatedly switch to the next screen
from that timer event. The measured dispatch window is firmware-owned: timer
event dispatch start through lazy SQBC read/resume and screen block completion.
It excludes host serial command latency and physical display refresh.

Required output fields:

- `benchmark=lazy_load_screen_transition`
- `mode=representative` or `mode=worst`
- `target=<target-name>`
- `transition_count=<counted-transitions>`
- `dispatch_elapsed_us_min=<microseconds>`
- `dispatch_elapsed_us_median=<microseconds>`
- `dispatch_elapsed_us_p95=<microseconds>`
- `dispatch_elapsed_us_max=<microseconds>`
- `sqbc_read_count_total=<count>`
- `sqbc_read_bytes_total=<bytes>`

The representative mode should use normal small screen blocks. Worst mode
should keep the app as real SquidScript but pad each screen block with enough
non-optimized statements to make each lazy screen-code read approach the
target's `vm_sqbc_chunk_bytes` limit without exceeding it.

Future runners should install an equivalent app, drive the same logical
transitions, and report the same fields. If a target cannot expose
firmware-owned dispatch timing, the runner should fail clearly rather than
substituting host wall-clock timing.
