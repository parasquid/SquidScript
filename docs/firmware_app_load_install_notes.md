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

## Known issue: install-and-launch from one handler still fails (`-5`)

The DoD-#6 flow — a `ble.object.complete` handler that does `app.install(...)`
then `app.launch(...)` — still fails to launch the installed app with `-5`,
**even with all of the above fixed**. Verified facts that scope it:

- A valid >2 KB app installs byte-exact over BLE and, after a **reboot**,
  launches fine (`active=installed-app`, prints, no `-5`). So the installed
  bytes are correct on flash.
- The same app installed from an **idle** device (VM not mid-dispatch) and
  launched in the same session launches fine. Protocol/CLI install while another
  app is merely *resident* (VM idle) also works.
- A handler that only **launches** a pre-existing app works.
- It fails only when **install and launch run from inside the same VM
  dispatch** (the handler).

Root cause (as far as traced): **writing the app store to flash from inside a
VM dispatch corrupts the flash read cache** for subsequently-read bytecode, so
the VM reads stale bytes and faults (`vm_error -5`). It is *not* a
lifecycle-target bug — the post-install `START_APP` for `app=ble-install` is the
**normal** "exit the current app before launching the target" step
(`LAUNCH_REQUESTED` → `EXIT_FOR_LAUNCH` dispatches the current app's `app.exit`,
then launches `lifecycle_target_app`); the stale read just happens to surface on
that read. Crucially, a **LittleFS unmount+remount did not fix it** (the stale
read persisted), so the stale cache is **below** LittleFS — the ESP32 flash
read/XIP cache — and only a reboot clears it. Installing when the VM is idle
(protocol/CLI path, even with another app resident) does not corrupt it.

Candidate fixes (both non-trivial; needs a decision):
- **Defer the install to VM-idle.** Make `app.install` from a handler record a
  pending install (file ref + app id) and perform the flash write after the
  dispatch slice completes, like the serial protocol install (which works).
  Requires keeping the BLE staging file alive until the deferred install and
  sequencing install-then-launch in the lifecycle.
- **Flush the ESP32 flash read cache after the install write.** Zephyr exposes
  only `fs_sync(file)` (the install already closes its files); an ESP-IDF-level
  cache writeback/invalidate would be needed, which the Zephyr flash API does
  not surface today.
