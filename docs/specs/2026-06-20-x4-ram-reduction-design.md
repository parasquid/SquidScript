# XTEINK X4 RAM Reduction Design

## Purpose

Reduce XTEINK X4 (ESP32-C3) static DRAM and runtime heap pressure so the
optional SQBC small-app cache (Stage 2 of the read-latency work) can be added
without violating stack guardrails or leaving the device with an unsafe free
heap. The aggressive pre-cache target is:

- at least **48 KiB linker DRAM headroom**, and
- at least **24 KiB free heap** under the worst representative pre-cache workload.

Stack safety margins are preserved by measurement, not by blind shrinkage. The
work is split into firmware-internal refactors that hit the static-DRAM target
now, a language-layer color-constant slice that removes the temporary firmware
string parser, and a later heap/stack right-sizing + radio demand-activation
plan informed by measured baselines.

## Current Baseline

After Stage 1 SQBC handle reuse (`9d74821`):

- Linker DRAM: 375,248 / 378,640 bytes (99.10%), leaving 3,392 bytes.
- Canonical grid-cursor workload: dispatch ~109 ms, heap free 8,840 bytes, heap
  allocated 56,584 bytes, max allocated 58,380 bytes.
- Protocol/main stack: 4,864 total, 2,444 unused. VM stack: 24,576 total, 7,504
  unused. Display-worker stack: 4,096 total, unmeasured.
- A 4 KiB SQBC cache at the current heap level would leave ~5 KiB free, which is
  not an acceptable basis for Stage 2.

Static DRAM attribution (`scripts/zephyr-static-buffer-report.sh`): platform
133,234 B / SquidScript 74,208 B / unknown 80,307 B. The largest SquidScript
attribution is the four retained display-op arrays totaling 72,224 B.

## Display-Op Representation

`struct sq_vm_runtime_display_op` (`firmware/zephyr/src/vm_runtime.h:283-294`)
is 376 bytes per op. It embeds three 64-byte char arrays (`text`, `fill_color`,
`stroke_color`), common coordinates, font height, and a full
`struct sq_vm_runtime_binbook_page` (160 bytes) by value. Four retained 48-op
arrays consume 72,224 bytes of static DRAM:

- `runtime->display_ops[48]` (18,048 B) — the VM-thread construction buffer.
- `sq_vm_runtime_display_active_job.ops[48]` (18,064 B) — the display-worker
  flush job.
- `sq_vm_runtime_display_pending_job.ops[48]` (18,064 B) — the single-slot
  pending queue.
- `previous_composed_ops[48]` (18,048 B) — the SSD1677 composed fast-refresh
  previous frame.

### Compact Tagged Union

The op becomes a tagged union with a common header and a variant payload per
kind. The four kinds are `CLEAR`, `TEXT`, `RECT`, and `BINBOOK_DRAWABLE`.

- Common header: `kind`, `x`, `y`.
- `CLEAR` payload: a typed color (the clear color).
- `TEXT` payload: `text[64]`, `font_height`, a typed text color.
- `RECT` payload: `w`, `h`, a typed fill color, a typed stroke color.
- `BINBOOK_DRAWABLE` payload: an `is_binbook` flag; the page travels out-of-band
  (see below).

The compact op is approximately 88 bytes. Four retained arrays become
4 x 48 x 88 = 16,896 bytes, freeing approximately 55 KiB of static DRAM and
clearing the 48 KiB headroom target with roughly 9 KiB surplus.

### Out-Of-Band BinBook Page

The 160-byte `sq_vm_runtime_binbook_page` is removed from every op slot. The
page travels as a single heap-allocated snapshot referenced by the flush job:

- Allocated when a `BINBOOK_DRAWABLE` op is produced and the flush job is
  handed off from the VM thread to the display worker.
- Freed by the display worker after the flush completes.
- Freed by the foreground on reset, app switch, install replacement, and storage
  format lifecycle boundaries (the same boundaries that release SQBC storage).
- Zero BinBook page RAM for non-BinBook apps such as grid-cursor.

The display worker already operates on a pinned by-value copy of the ops
(`runtime_display_copy_flush_job`, `vm_runtime.c:509-518`). The page snapshot is
taken at the same handoff point, so the worker never dereferences
`runtime->drawable.page`. `runtime->drawable.page` is written and read only on
the VM dispatch thread (`runtime_binbook_read_page` writes it,
`runtime_display_draw` reads it).

The backend's `binbook_previous_page` (`ssd1677_gdeq0426t82_display.c:202`) is a
separate N-1 snapshot owned by the display worker for BW differential refresh.
It remains a static single slot (160 B) and is not fed by the runtime page ring;
it is copied from the flush job's page at flush completion
(`binbook_remember_previous_page`).

### Typed Color

Colors are stored as a `uint8_t` encoding the 18-name palette
(`white`, `black`, `gray0`..`gray15`) plus an unset sentinel. The palette is
fixed by `docs/language_spec.md:1752-1761`. In this firmware-internal slice the
conversion from the existing string API to the typed value happens at the FFI
producer boundary (`runtime_display_clear/text/rect` in
`vm_runtime_display.c`), which already receive the byte string and length. The
public SquidScript API and the FFI option structs are unchanged in this slice.

`ssd1677_color_is_black` (`ssd1677_gdeq0426t82_display.c:18-24`) becomes a typed
comparison. `composed_op_equal` (`ssd1677_gray2.c:134`) compares typed colors
instead of `strcmp` on 64-byte arrays; it continues to ignore the BinBook page
field, matching current behavior.

### BinBook Page Ring

A 3-slot heap circular buffer of page metadata (prev / current / next) is
allocated lazily on the first `binbook.readPage` call and freed on reset/app
exit. Today every page turn re-opens the book file and re-reads 76 bytes of
page-index metadata from flash (`runtime_binbook_read_page`,
`vm_runtime_binbook.c:542-600`); the reader screen body even re-calls
`binbook.open` each refresh. The ring caches page metadata (160-byte structs),
not pixel data (pixels stream from flash at flush time via
`stream_binbook_gray2_plane`). A forward page turn is a ring hit plus a prefetch
of the new next page.

The ring is VM-thread-owned. The flush snapshot remains a by-value copy, so the
worker is unaffected. The ring is bounded (3 x 160 = 480 bytes heap when a
BinBook app runs) and is not allocated for non-BinBook apps.

### Preserved Behaviors

The representation change must preserve:

- Op production order (CLEAR / TEXT / RECT / BINBOOK_DRAWABLE).
- The 48-op ring-shift keeping CLEAR (`runtime_display_append_op`,
  `vm_runtime_display.c:43-51`); the test
  `test_display_op_buffer_preserves_clear_for_library_like_screen` encodes this.
- `display_dirty` / `display_op_count` semantics.
- Full `binbook_page` snapshot semantics for `BINBOOK_DRAWABLE`.
- `composed_op_equal` equality behavior used by the dirty-window computation.
- The composed and BinBook refresh-decide state machines.
- `sq_display_backend_reset` invoked from `sq_vm_runtime_reset`.
- The active/pending/previous buffer ownership model: the four buffers remain
  distinct; the worker reads `active_job.ops` and `previous_composed_ops`
  concurrently with the foreground writing `display_ops` and (under mutex)
  `pending_job.ops`. No buffer may alias another.

### Estimated Static DRAM Outcome

- Before: 72,224 B (op arrays) + 168 B (`runtime->drawable`) + 160 B
  (`binbook_previous_page`) ~ 72.5 KiB.
- After: 16,896 B (op arrays) + ~16 B (`runtime->drawable` ring control) + 160 B
  (`binbook_previous_page`) ~ 16.7 KiB.
- Saved ~ 55 KiB. Headroom 3,392 + 55,480 ~ 57.5 KiB, clearing the 48 KiB target.
- Heap when a BinBook app runs: ring 480 B + a brief 160 B flush snapshot,
  negligible against the 24 KiB free-heap target. Zero for non-BinBook apps.

## Color Constants

A compile-time color value namespace `color` is added with constants
`color.GRAY0`..`color.GRAY15`, `color.WHITE`, and `color.BLACK`. The compiler
resolves `color.*` references to the typed `uint8` value and emits typed colors
in SQBC display options. The FFI display option structs carry the typed `uint8`
instead of a byte string. Firmware passes the typed value straight into the op
and the temporary Plan 2 string-to-uint8 conversion is removed.

String color values (`"gray15"`, `"white"`, etc.) are replaced entirely. All
examples and tests migrate to `color.*`. This follows the pre-1.0 direct
replacement rule: no compatibility bridge, no dual-form acceptance.

## Telemetry

Two resource metrics are wired but currently zero:

- `heap_largest_free_supported` (ID 8) and `heap_largest_free_bytes` (ID 9) are
  initialized to 0 and never computed (`device_protocol.c:2591-2592,2704-2706`).
  The host harness already records these columns
  (`scripts/lib/ram-workload-harness.sh:143,167-168`).
- Display-worker stack high-water has no metric ID and no accessor. Only the
  protocol/main stack and the VM work stack are measured.

### Largest Free Block

A bounded binary-search probe using `k_malloc` / `k_free` computes the largest
allocatable block. The probe is gated by `CONFIG_SYS_HEAP_RUNTIME_STATS` and uses
the measured `heap_free_bytes` as the upper bound. All heap stats are captured
before the probe so the reported `heap_max_allocated_bytes` is unaffected by the
probe's transient allocation. The probe is bounded to at most 32 iterations
(~log2 of the heap size). `heap_largest_free_supported` is 1 when runtime stats
are enabled and a heap is present, 0 otherwise.

### Display-Worker Stack

New accessors `sq_vm_runtime_display_work_stack_size` and
`sq_vm_runtime_display_work_stack_unused` mirror the existing VM work-stack
accessors (`vm_runtime.c:663-684`). New metric IDs 53, 54, 55
(`display_stack_size_bytes`, `display_stack_unused_bytes`,
`display_stack_used_bytes`) are emitted in `resources_response` and decoded by
the Rust host codec and the test mirror table.

### Static Buffer Report Classifier

`scripts/zephyr-static-buffer-report.sh` classifies symbols into
platform / squidscript / unknown. Display, BinBook, and HTTP symbols currently
fall into unknown. The classifier is extended so
`sq_vm_runtime_display_active_job`, `sq_vm_runtime_display_pending_job`,
`previous_composed_ops`, `sq_vm_runtime_display_work_stack`,
`sq_vm_runtime_display_work_thread`, and `sq_http_*` attribute to squidscript,
so before/after comparisons are clean.

### X4 RAM Workload Script

`scripts/xteink-x4-measure-ram-workloads.sh` adapts the XIAO template
(`scripts/xiao-esp32c3-measure-ram-workloads.sh`) and the shared harness
(`scripts/lib/ram-workload-harness.sh`) to run the X4 workload set serially:
storage-format baseline, grid-cursor, binbook-reader, Wi-Fi AP start/stop,
BLE transfer, HTTP transfer. Each workload is bracketed by
`device resources --reset-heap-max` and `device resources` snapshots.

### Stack Usage Build

`cargo run -p squidc -- target build --target xteink-x4 --stack-usage` adds
`-fstack-usage` to the firmware C sources (`firmware/zephyr/CMakeLists.txt:78-81`).
A pristine reconfigure is required because the option is cached in
`CMakeCache.txt`. The `.su` parser (`scripts/c3-supermini-stack-usage-report.sh`)
is build-dir-agnostic and is pointed at the X4 build tree.

## Demand-Activated Radio

`bt_disable()` is available on this Zephyr tree for ESP32-C3 and reclaims the
BT controller's dynamic heap allocations and aborts the BT RX workqueue thread.
Static BT stacks (`rx_thread_stack`, `bt_stack`, `bt_lw_stack_area`,
`bt_tx_processor_stack`, ~10.5 KB BSS) remain resident. BLE advertising is
already demand-gated on the profile table (`vm_runtime_ble.c:59-81`); a demand
activation extension would widen that seam to the full host/controller via
`bt_disable` on last consumer (`runtime_ble_stop`) and `bt_enable` on first
consumer (`runtime_ble_start`).

Wi-Fi has no supported Zephyr seam for `esp_wifi_deinit()`. The driver never
calls it, and SquidScript's Wi-Fi stop path only stops the radio link. The
~16 KB of static Wi-Fi RX buffers cannot be freed through a supported API.
Demand activation for Wi-Fi is therefore not pursued.

Demand-activated radio work is deferred to a later plan, written after the
Plan 1 baselines and Plan 2 measurements inform whether it is needed to reach
the 24 KiB free-heap target.

## Out Of Scope

- The `use binbook` / capability demand-loading keyword. This is a language
  architecture feature that generalizes to display fast-refresh, BLE, Wi-Fi, and
  HTTP; it is captured in `ICEBOX.md`.
- Lowering the 48-op cap. grid-cursor emits ~30+ ops and the cap is a documented
  runtime limit.
- Dropping the active/pending/previous display buffers. The nonblocking display
  concurrency and SSD1677 differential-refresh ownership model is preserved;
  only the element representation is compacted.
- Changing SQBC encoding, the FFI ABI for non-color fields, display refresh
  semantics, input debounce, or state persistence.
