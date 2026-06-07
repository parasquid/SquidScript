# Firmware App Load & Install — Hard-Won Notes

Things that are easy to get wrong when an app is compiled, installed, and
launched on the constrained Zephyr VM. Read this before touching the SQBC
encoder, the VM program loader, the app-store install path, or the BLE
install-and-launch flow.

## VM load/execution limits are a compile-time contract

The constrained VM has hard caps. They live in one place — the **`squidvm-limits`**
crate — and **both** the VM (`squidvm-core`) and the compiler (`squidc-core`)
depend on it. The compiler rejects any app that exceeds a cap, so an app the VM
cannot load or run fails at **compile time**, not on-device with an opaque
error. Do not duplicate these constants; do not let them drift.

Caps that matter for loadability (see `squidvm-limits/src/lib.rs`):

| Cap | Value | Why it bites |
| --- | --- | --- |
| `MAX_CODE_CHUNK_BYTES` | 640 | **Per-frame** code limit. The VM loads a whole handler/function/screen frame into one 640-byte code-chunk buffer. A single frame's compiled code over 640 bytes cannot execute (`ChunkTooLarge` → `-5`). Split logic across functions. |
| `MAX_PROGRAM_STRING_BYTES` | 768 | Total interned string content. |
| `MAX_STRINGS` | 64 | Distinct interned strings. |
| `MAX_APP_BYTES` | 4096 | Whole SQBC container. |
| `MAX_FUNCTIONS`/`MAX_HANDLERS`/`MAX_SCREENS`/`MAX_STATE`/`MAX_TRIGGERS` | 16 | Table sizes. |

Note: `display.text` etc. inside a `screen { ... }` compile into that screen's
**render code**, so a screen with many draws hits the 640-byte per-frame code
limit, not the string limit. A big app must spread code across multiple frames.

## Program-load scratch must fit a whole section

`sqvm_context_init_in_place` reads each whole SQBC section (strings, state,
functions, handlers, screens) into the scratch the firmware passes. Code is read
in 640-byte chunks; everything else is read **whole**. So the scratch must be ≥
the largest single section. Since no section can exceed the total app size, the
firmware sizes it to `MAX_APP_BYTES` (`SQ_VM_RUNTIME_SCRATCH_BYTES` in
`vm_runtime.h`). This is **distinct** from `SQVM_STORAGE_TRANSFER_CAPACITY`
(640), which sizes the storage-transfer completion buffer (one code chunk /
state read) and matches the VM's `MAX_STORAGE_TRANSFER_BYTES`. Conflating the
two (reusing the 640-byte transfer buffer as the parse scratch) makes any app
with a section >640 fail to load with `InvalidSection` → `-EIO`.

## Installing a file: never hold two handles open on the same LittleFS

`sq_app_store_install_from_file_ref` copies a staging file (e.g. a BLE-received
`/sq/tmp/...`) into the app store. Both live on the **same LittleFS mount**.
Holding the source and destination open at once across an interleaved
read/write **aliases LittleFS's shared read/prog cache** and corrupts the copy:
the install reports the right size but the app reads back garbage and the VM
faults on launch. Copy one chunk at a time, opening/closing each side per chunk
(matching the serial install path, whose source is the UART stream — never a
second open file).

## Debugging gotchas (these cost real time)

- **`-5 (EIO)` hides the real cause.** The FFI boundary collapses any nonzero
  host-callback return into a generic VM error that surfaces as `code=-5
  (EIO)`. Record the real errno at the source (`sq_errno_name`) before it is
  flattened, or you cannot tell `InvalidSection` from `ENOMEM` from a genuine
  I/O error.
- **`runtime=host_error` vs `runtime=vm_error`.** `host_error` (VM status OK,
  nonzero result_code) means a host callback or the firmware launch path failed
  *before/around* the VM; `vm_error` means the VM itself faulted (e.g. parsed
  garbage code). They point at different layers.
- **No Zephyr console on the XIAO USB-CDC.** `LOG_INF`/`LOG_ERR` go to the UART0
  console pin, not the USB-CDC that carries the SquidScript protocol. A raw read
  of `/dev/ttyACM0` shows nothing (the protocol is request/response). Diagnose
  via `squidc device errors` / `trace` / `output`, recording into those buffers.
- **`device errors` is a tiny ring.** Only `SQ_VM_RUNTIME_DEVICE_ERROR_MAX`
  entries (2) are kept, and the persistent `display=unavailable` boot error
  takes one. Widen it temporarily when a failure records more than one line.
- **BLE advertised name is truncated.** The XIAO advertises ~29 chars of
  `CONFIG_BT_DEVICE_NAME`, so an exact-name scan filter misses it. Match by
  address (`tools/ots-push` accepts an address or name).

## Known issue: a BLE-received app cannot be launched until reboot (`-5`)

An app whose bytes arrived over the BLE Object Transfer path cannot be launched
in the same session — the VM faults with `runtime=vm_error code=-5 (EIO)`. After
a reboot the very same on-flash app launches cleanly. This is the open blocker
for DoD #6 (push a `.sqbc` over BLE, install, launch in one flow).

### What the evidence rules in and out

Hardware-verified facts (XIAO ESP32-C3):

- **The bytes are correct on flash.** After a reboot the BLE-received app
  launches fine (`active=installed-app`, prints, no `-5`). The transfer and
  install are byte-exact.
- **The launch trigger is irrelevant.** It fails the same whether the launch
  comes from the receiving handler (`app.launch` in `ble.object.complete`) or
  from a *separate* CLI `app launch` issued seconds later. A handler that only
  installs (no launch) records no error; the failure is on the later read.
- **The install method is irrelevant.** Copy-into-the-app-store, rename the
  staging file into place, defer the install to VM-idle, and a 200 ms settle
  delay before launch all fail identically.
- **It is not the radio being active during the read.** A **serial/protocol**
  install of the same `.sqbc` followed by a CLI launch **succeeds even while the
  BLE radio is advertising**. The only thing that differs between this passing
  case and the failing case is *which path wrote the file's bytes to flash*.
- **It is not a hardware flash cache.** This build sets
  `CONFIG_ESP_FLASH_HOST=y`, so the Zephyr esp32 flash driver writes via
  `esp_flash_write` (the IDF API that disables and invalidates the cache around
  the write) and reads via `esp_flash_read` (direct SPI). A LittleFS
  unmount+remount does **not** fix it either.
- **It is not the BT RX thread stack** (raising `CONFIG_BT_RX_STACK_SIZE` from
  4096 to 8192 did not change it), and there is no stack sentinel/PMP guard on
  this port to catch a silent overflow (the espressif esp32c3 port does not wire
  up RISC-V PMP regions, so `HW_STACK_PROTECTION` will not link;
  `CONFIG_STACK_SENTINEL` is the only loud option and broke the protocol link
  when tried).
- **It is not the per-chunk write pattern.** Holding one file handle open for
  the whole transfer (one `fs_open`, sequential `fs_write`s, one `fs_close` —
  far fewer LittleFS metadata commits) reproduces it identically.
- **It is not the writing thread's identity or its preemptibility.** Moving BT
  RX processing — and thus the OTS write callback — onto the system workqueue
  (`CONFIG_BT_RECV_WORKQ_SYS=y`, sysworkq stack 8192) reproduces it. Note the
  system workqueue is *cooperative* (priority -1) and the BT RX workqueue is
  *preemptible* (priority 8); both fail.
- **It is not an active BLE connection during the write.** Holding a BLE
  connection open and doing a **serial** install + launch in that window
  **works**.
- **It is not simply write-thread ≠ read-thread.** The VM dispatch (and thus the
  launch-time SQBC read) runs on its own worker thread
  (`sq_vm_runtime_work_stack`), so even the working serial path writes on one
  thread and reads on another.

### Current understanding (mechanism NOT yet identified)

Every variable above has been ruled out on hardware. The one stable fact:
**bytes written to flash by the serial/protocol path read back correctly
in-session; bytes written by the BLE OTS path do not, until a reboot** — though
they are byte-correct on flash either way (reboot launches the OTS-received app
fine), and the corruption is localized to that file (directory listing and size
read back fine via `app list`). The trigger is therefore something specific to
the OTS object-write code path
(`sq_ble_ots_obj_write_internal` in `firmware/zephyr/src/ble_object_transfer.c`)
that is not the thread, the preemptibility, the write pattern, the connection
state, or the install/launch step that follows — and it has resisted
black-box isolation.

Cracking it likely needs source-level instrumentation rather than more
configuration sweeps:
- In the deferred install (poll/protocol thread), read the **whole** OTS staging
  file back and compare it byte-for-byte against the known-good `.sqbc` to find
  *where* the in-session read diverges (first page only? every page? aligned to
  a LittleFS block / cache boundary?). The magic (first 8 bytes) does read back
  correctly, so divergence is past the first read.
- Instrument the LittleFS `read`/`prog`/lookahead cache state and the
  `esp_flash` guard around an OTS write vs a serial write to see what differs.

### Workaround available today

A BLE-received app installs correctly and launches after a **reboot**. Until the
mechanism is found, a receive-then-reboot-to-run flow is the reliable path.
