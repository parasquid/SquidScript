# Unified Upload API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Use TDD: write the failing test for
> each behavior before implementation. Commit after each independently verified
> slice with a message that says what behavior changed, why, and what verified
> it.

**Goal:** Replace transport-specific upload authoring with one script-owned
`service.upload.*` API, then make HTTP and BLE feed the same staged upload
completion contract on native XTEINK X4.

**Architecture:** SquidScript apps declare one upload receiver with accepted
extensions, enabled transports, and a completion event. Firmware exposes each
transport as an adapter: HTTP parses `PUT`/`HEAD /upload/<safe-name>` over TCP,
BLE preserves the existing GATT wire protocol, and both call a shared route,
staging, dispatch, and cleanup core. Upload is an app concern; Wi-Fi/AP,
browser UI serving, and file organization remain separate script-level choices.

**Tech Stack:** SquidScript parser/semantic/IR/SQBC, `squidvm-core`,
`squidc`, native Rust ESP32-C3 firmware with Embassy/`embassy-net`, TrouBLE
GATT, SD-backed `tmp/` staging, XTEINK X4 hardware tests.

---

## Non-Negotiable Decisions

- Public app API is `service.upload.start`, `service.upload.stop`, and
  `service.upload.status`.
- This is a direct pre-1.0 replacement. Do not keep compatibility aliases for
  `service.http.start("file-upload", ...)`, `service.ble.start("file-transfer",
  ...)`, or `squidc device ble-put`.
- `service.upload.start` takes one static config object:

  ```squid
  let started = service.upload.start({
    id: "book-upload"
    accept: [".binbook"]
    transports: ["http", "ble"]
    events: {
      complete: "upload.complete"
    }
  })
  ```

- Bare statement form is allowed and discards the returned result:

  ```squid
  service.upload.start({
    id: "book-upload"
    accept: [".binbook"]
    transports: ["http"]
    events: { complete: "upload.complete" }
  })
  ```

- `service.upload.start(...)` returns:

  ```text
  { ok: bool, error: string?, id: string?, transports: list, httpPath: string? }
  ```

- `service.upload.status()` returns:

  ```text
  { active: bool, id: string?, transports: list, httpPath: string?,
    inFlight: bool, bytesReceived: string?, totalBytes: string?, error: string? }
  ```

- Completion event payload is:

  ```text
  ev.upload
  ev.name
  ev.bytesReceived
  ev.totalBytes
  ev.id
  ev.transport
  ```

- HTTP endpoint is fixed for this slice:

  ```text
  PUT  /upload/<safe-name>
  HEAD /upload/<safe-name>
  ```

- `service.upload.start(... transports: ["http"] ...)` must not start Wi-Fi or
  AP. Scripts explicitly call `service.wifi.startAP`, station connect, or other
  network setup.
- HTTP resume keeps the existing Zephyr contract: `HEAD` reports
  `X-Squid-Upload-Offset` and `X-Squid-Upload-Total`; resumed `PUT` uses
  `Content-Range`.
- No browser HTML upload page in this slice. Endpoint first; page/static asset
  serving is later work.
- Upload handlers decide what the file means. A book uploader copies to
  `books`; an app installer calls `app.install`; firmware does not infer this.

## Code Areas To Read Before Editing

- Compiler syntax and validation:
  - `compiler/rust/crates/squidc-core/src/parser/statements.rs`
  - `compiler/rust/crates/squidc-core/src/parser/expressions.rs`
  - `compiler/rust/crates/squidc-core/src/parser/objects.rs`
  - `compiler/rust/crates/squidc-core/src/ir.rs`
  - `compiler/rust/crates/squidc-core/src/semantic.rs`
  - `compiler/rust/crates/squidc-core/src/sqbc.rs`
  - `compiler/rust/crates/squidc-core/src/tests.rs`
- VM and SQBC readers:
  - `compiler/rust/crates/squidvm-core/src/bytecode.rs`
  - `compiler/rust/crates/squidvm-core/src/program.rs`
  - `compiler/rust/crates/squidvm-core/src/host.rs`
  - `compiler/rust/crates/squidvm-core/src/vm.rs`
  - `compiler/rust/crates/squidvm-core/src/tests.rs`
- Host CLI:
  - `compiler/rust/crates/squidc-cli/src/main.rs`
  - `compiler/rust/crates/squidc-cli/src/ble_push.rs`
  - `compiler/rust/crates/squidc-cli/src/serial.rs`
- Native runtime and firmware:
  - `firmware/native/crates/squidscript-fw-core/src/native_runtime.rs`
  - `firmware/native/crates/squidscript-fw-core/tests/native_runtime.rs`
  - `firmware/native/crates/squidscript-fw-x4/src/main.rs`
  - `firmware/native/crates/squidscript-fw-x4/src/lib.rs`
  - `firmware/native/crates/squidscript-fw-x4/Cargo.toml`
- Existing upload examples and hardware scripts:
  - `examples/ble-install/main.squid`
  - `examples/http-binbook-upload/main.squid`
  - `tests/hardware/xteink-x4/http-binbook-upload/main.squid`
  - `scripts/xteink-x4-test-http-binbook-upload.sh`
  - `scripts/xteink-x4-test-ble-transfer.sh`
- Docs to update:
  - `docs/language_spec.md`
  - `docs/sqbc_binary_format.md`
  - `docs/runtime_limits.md`
  - `docs/squidc_cli.md`
  - `docs/firmware_state_machines.md`
  - `docs/hardware_target_tests.md`
  - `ROADMAP.md`

## Task 1: Add Compiler Surface For `service.upload.*`

**Files:**

- Modify: `compiler/rust/crates/squidc-core/src/parser/expressions.rs`
- Modify: `compiler/rust/crates/squidc-core/src/parser/statements.rs`
- Modify: `compiler/rust/crates/squidc-core/src/ir.rs`
- Modify: `compiler/rust/crates/squidc-core/src/semantic.rs`
- Modify: `compiler/rust/crates/squidc-core/src/tests.rs`
- Modify only if needed: `compiler/rust/crates/squidc-core/src/formatter.rs`

- [ ] Add failing parser tests for `service.upload.start` as expression and
  as bare statement.

  Test source:

  ```squid
  app "upload-api"
  event.on("app.start") {
    let started = service.upload.start({
      id: "book-upload"
      accept: [".binbook", ".sqbc"]
      transports: ["http", "ble"]
      events: { complete: "upload.complete" }
    })
    debug.print(started.ok, started.httpPath)
    service.upload.stop()
  }
  event.on("upload.complete", ev) {
    debug.print(ev.transport, ev.name, ev.bytesReceived, ev.totalBytes, ev.id)
  }
  ```

  Expected IR facts:
  - `Let` expression is `IrExpr::Call { name: "service.upload.start", args: [...] }`.
  - The first argument is a static object literal containing `id`, `accept`,
    `transports`, and `events`.
  - `service.upload.stop()` is accepted as a bare call statement.

- [ ] Add failing semantic tests for invalid upload configs:
  - empty `id`
  - empty `accept`
  - accepted extension missing leading `.`
  - empty `transports`
  - unsupported transport not equal to `"http"` or `"ble"`
  - missing or empty `events.complete`
  - duplicate upload profile id with different config

  Use diagnostic code `E_UPLOAD_PROFILE` and message:

  ```text
  service.upload.start requires a non-empty id, accepted extensions, supported transports, and a complete event route
  ```

- [ ] Implement the targeted parser path.
  - Do not add general dynamic object semantics.
  - Add a targeted `service.upload.start({...})` expression parser that consumes
    one static object argument and returns `IrExpr::Call`.
  - Add `service.upload.status()` and `service.upload.stop()` as normal zero-arg
    service calls.
  - Let a bare `service.upload.start({...})` statement parse as `IrStatement::Call`
    so SQBC can emit the call and pop the returned record.

- [ ] Remove old upload-specific IR statement variants:
  - `ServiceBleStart`
  - `ServiceBleStop`
  - `ServiceHttpStart`
  - `ServiceHttpStop`

  Replace them with ordinary call expressions/statements for:
  - `service.upload.start`
  - `service.upload.stop`
  - `service.upload.status`

- [ ] Run:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core service_upload -- --nocapture
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core parses_service_upload -- --nocapture
  ```

  Expected: new tests fail before implementation and pass after implementation.

- [ ] Commit:

  ```bash
  git add compiler/rust/crates/squidc-core/src/parser/expressions.rs \
    compiler/rust/crates/squidc-core/src/parser/statements.rs \
    compiler/rust/crates/squidc-core/src/ir.rs \
    compiler/rust/crates/squidc-core/src/semantic.rs \
    compiler/rust/crates/squidc-core/src/tests.rs \
    compiler/rust/crates/squidc-core/src/formatter.rs
  git commit -m "feat: add script-owned upload service syntax"
  ```

## Task 2: Encode Unified Upload Profiles In SQBC

**Files:**

- Modify: `compiler/rust/crates/squidc-core/src/sqbc.rs`
- Modify: `compiler/rust/crates/squidc-core/src/tests.rs`
- Modify: `docs/sqbc_binary_format.md`

- [ ] Add failing SQBC tests that compile the source from Task 1 and assert a
  single upload profile section with this wire format:

  ```text
  offset  size  field
  0       2     little-endian u16 profile count
  ...           profile records

  Profile record:
  0       2     little-endian u16 profile id string id
  2       2     little-endian u16 role string id, fixed string "server"
  4       2     little-endian u16 accepted extension count
  6       2*n   accepted extension string ids
  ...     2     little-endian u16 transport count
  ...     2*n   transport string ids, values "http" and/or "ble"
  ...     2     little-endian u16 event route count
  ...     4*n   event route pairs: kind string id, event string id
  ```

- [ ] Use one SQBC section for upload profiles:
  - Define `SECTION_UPLOAD_PROFILES = 10`.
  - Delete old BLE profile section terminology from current docs.
  - Delete old HTTP profile section `11` from current docs.
  - Do not add SQBC compatibility readers for the old sections.

- [ ] Update string collection so `id`, accepted extensions, transports, event
  kinds, and event names are all interned.

- [ ] Emit builtins:
  - `BUILTIN_SERVICE_UPLOAD_START = 0xc1`
  - `BUILTIN_SERVICE_UPLOAD_STOP = 0xc2`
  - `BUILTIN_SERVICE_UPLOAD_STATUS = 0xc3`

  Reuse these old BLE/HTTP builtin slots because SQBC has no pre-1.0
  compatibility contract. Remove old `BUILTIN_SERVICE_BLE_*` and
  `BUILTIN_SERVICE_HTTP_*` names.

- [ ] `service.upload.start` SQBC emission:
  - Push the profile id string id.
  - Emit `BUILTIN_SERVICE_UPLOAD_START`.
  - If the call is a bare statement, pop the returned record.
  - If it is a `let` expression, leave the record on the stack.

- [ ] `service.upload.status` SQBC emission:
  - Emit `BUILTIN_SERVICE_UPLOAD_STATUS`.
  - Leave returned record on the stack for expressions.

- [ ] `service.upload.stop` SQBC emission:
  - Emit `BUILTIN_SERVICE_UPLOAD_STOP`.
  - Return `null` or no meaningful value; bare statement form must not leave a
    stack item.

- [ ] Run:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core sqbc_upload -- --nocapture
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core service_upload -- --nocapture
  ```

- [ ] Commit:

  ```bash
  git add compiler/rust/crates/squidc-core/src/sqbc.rs \
    compiler/rust/crates/squidc-core/src/tests.rs \
    docs/sqbc_binary_format.md
  git commit -m "feat: encode unified upload profiles in sqbc"
  ```

## Task 3: Add VM Upload Builtins And Record Shapes

**Files:**

- Modify: `compiler/rust/crates/squidvm-core/src/bytecode.rs`
- Modify: `compiler/rust/crates/squidvm-core/src/program.rs`
- Modify: `compiler/rust/crates/squidvm-core/src/host.rs`
- Modify: `compiler/rust/crates/squidvm-core/src/vm.rs`
- Modify: `compiler/rust/crates/squidvm-core/src/tests.rs`

- [ ] Add failing tests that parse upload profile metadata from a SQBC reader.
  Expected profile:

  ```text
  id = "book-upload"
  role = "server"
  accept = [".binbook", ".sqbc"]
  transports = ["http", "ble"]
  events = [{ kind: "complete", event: "upload.complete" }]
  ```

- [ ] Replace BLE-specific `ProgramIndex::ble_profile_*` readers with
  upload-profile readers:
  - `ProgramIndex::upload_profile_count_from_reader`
  - `ProgramIndex::upload_profile_from_reader`
  - `UploadProfile`
  - `UploadProfileTextList`
  - `UploadProfileEventRoutes`

- [ ] Add VM host result structs:

  ```rust
  pub struct UploadStartResult<'a> {
      pub ok: bool,
      pub error: Option<&'a str>,
      pub id: Option<&'a str>,
      pub transports: &'a [&'a str],
      pub http_path: Option<&'a str>,
  }

  pub struct UploadStatus<'a> {
      pub active: bool,
      pub id: Option<&'a str>,
      pub transports: &'a [&'a str],
      pub http_path: Option<&'a str>,
      pub in_flight: bool,
      pub bytes_received: Option<&'a str>,
      pub total_bytes: Option<&'a str>,
      pub error: Option<&'a str>,
  }
  ```

  If borrowed slices do not fit existing no-alloc host patterns, use fixed
  bounded writer/list helpers consistent with Wi-Fi scan records. Preserve the
  public record field names above.

- [ ] Add host callbacks:
  - `service_upload_start(&mut self, id: &str) -> Result<UploadStartResult<'_>, VmError>`
  - `service_upload_stop(&mut self) -> Result<(), VmError>`
  - `service_upload_status(&mut self) -> Result<UploadStatus<'_>, VmError>`

- [ ] Add VM record builders for start/status records.
  - Include `httpPath` as the SquidScript field name for `http_path`.
  - Include `bytesReceived` and `totalBytes` in status.
  - Keep record field count within existing runtime limits; if needed, update
    the explicit runtime-record limit source and tests in the same commit.

- [ ] Add VM dispatch tests:
  - `let result = service.upload.start(...)` records `service.upload.start book-upload`.
  - `service.upload.stop()` records `service.upload.stop`.
  - `let status = service.upload.status()` exposes `active` and `httpPath`.
  - Existing upload completion handler can read `ev.transport`.

- [ ] Run:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidvm-core upload -- --nocapture
  RUSTUP_TOOLCHAIN=stable cargo test -p squidvm-core
  ```

- [ ] Commit:

  ```bash
  git add compiler/rust/crates/squidvm-core/src/bytecode.rs \
    compiler/rust/crates/squidvm-core/src/program.rs \
    compiler/rust/crates/squidvm-core/src/host.rs \
    compiler/rust/crates/squidvm-core/src/vm.rs \
    compiler/rust/crates/squidvm-core/src/tests.rs
  git commit -m "feat: add vm upload service builtins"
  ```

## Task 4: Unify Native Runtime Upload Routing And Staging

**Files:**

- Modify: `firmware/native/crates/squidscript-fw-core/src/native_runtime.rs`
- Modify: `firmware/native/crates/squidscript-fw-core/tests/native_runtime.rs`

- [ ] Add failing native runtime tests for:
  - `service.upload.start` with `transports: ["http"]`
  - `service.upload.start` with `transports: ["ble"]`
  - `service.upload.start` with both transports
  - `service.upload.status` after start and after stop
  - route mismatch by extension
  - ambiguous extension routes
  - stale route after app replacement
  - cleanup on stop, reset, storage format, runtime error, and app replacement
  - `tmp/<safe-name>` readable during completion dispatch and deleted after
    the handler returns
  - completion payload includes `transport`

- [ ] Replace `NativeBleUploadRoute` and `NativeBleRouteError` with transport
  neutral names:
  - `NativeUploadRoute`
  - `NativeUploadRouteError`

- [ ] Replace `resolve_ble_upload_route(name)` with:

  ```rust
  resolve_upload_route(name: &str, transport: NativeUploadTransport)
  ```

  Where `NativeUploadTransport` supports `Http` and `Ble` and serializes to
  event payload values `"http"` and `"ble"`.

- [ ] Store one active upload profile on the host:
  - profile id
  - enabled transport bits
  - start event count
  - stop event count
  - last upload error
  - in-flight byte strings

  Do not store transport-specific accept lists in RAM. Read the SQBC
  `SECTION_UPLOAD_PROFILES` metadata by active profile id when resolving a
  route.

- [ ] Implement host callbacks:
  - `service_upload_start`
  - `service_upload_stop`
  - `service_upload_status`

  `start` must:
  - resolve profile metadata by id
  - reject unsupported target transports with `{ ok: false, error:
    "unsupported" }`
  - activate BLE advertising only when `"ble"` is enabled
  - activate HTTP listener route only when `"http"` is enabled
  - not start Wi-Fi/AP

- [ ] Generalize staging methods so both transports call:
  - `begin_ephemeral_upload(name, total_len, id, transport)`
  - `write_ephemeral_upload_chunk(upload_path, offset, bytes)`
  - `commit_ephemeral_upload(upload_path, bytes_received)`
  - `dispatch_active_upload_complete(event, upload_path)`
  - `abort_ephemeral_upload(upload_path)`

  The completion dispatch must include `transport`.

- [ ] Update resource metrics:
  - replace `ble_profile_*` metrics with `upload_profile_*`
  - add `upload_transport_http_active`
  - add `upload_transport_ble_active`
  - keep radio lease metrics unchanged

- [ ] Run:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-core native_runtime_upload -- --nocapture
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-core
  ```

- [ ] Commit:

  ```bash
  git add firmware/native/crates/squidscript-fw-core/src/native_runtime.rs \
    firmware/native/crates/squidscript-fw-core/tests/native_runtime.rs
  git commit -m "feat: unify native upload routing and staging"
  ```

## Task 5: Migrate Native BLE Transport Onto Unified Upload

**Files:**

- Modify: `firmware/native/crates/squidscript-fw-x4/src/main.rs`
- Modify: `firmware/native/crates/squidscript-fw-x4/src/lib.rs`
- Modify tests if present in: `firmware/native/crates/squidscript-fw-x4/src/main.rs`

- [ ] Add or update tests that compile the native BLE fallback/receiver app
  using `service.upload.start({ transports: ["ble"] })`.

- [ ] In the BLE storage task:
  - call `runtime.resolve_upload_route(route.name.as_str(), NativeUploadTransport::Ble)`
  - pass transport `"ble"` into staging
  - keep the existing GATT UUIDs and client wire protocol unchanged
  - keep bounded queue depth and 192-byte chunk behavior unchanged
  - keep existing watchdog and cleanup behavior unchanged

- [ ] Rename diagnostics from `ble-route-*` to upload-neutral names only where
  the diagnostic is not BLE-specific. Keep truly BLE transport diagnostics under
  `ble-*`.

- [ ] Build and test:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-x4 --features x4-binbook
  RUSTUP_TOOLCHAIN=nightly PATH=/var/home/tristan/codex-box/.cargo/bin:$PATH \
    cargo run -p squidc -- target build --target xteink-x4 --backend native
  ```

- [ ] Hardware verify BLE before moving to HTTP:

  ```bash
  RUSTUP_TOOLCHAIN=stable PATH=/var/home/tristan/codex-box/.cargo/bin:$PATH \
    cargo run -p squidc -- target flash --target xteink-x4 --backend native

  RUSTUP_TOOLCHAIN=stable scripts/xteink-x4-test-ble-transfer.sh \
    --target xteink-x4 --backend native --port /dev/ttyACM0 --skip-flash
  ```

  Expected:
  - upload completes through unified `service.upload`
  - receiving app sees `ev.transport == "ble"`
  - `file.copy` succeeds
  - `device content-check` verifies size and CRC
  - `device errors` is empty
  - radio/resource metrics return to zero after cleanup

- [ ] Commit:

  ```bash
  git add firmware/native/crates/squidscript-fw-x4/src/main.rs \
    firmware/native/crates/squidscript-fw-x4/src/lib.rs \
    tests/hardware/xteink-x4/ble-transfer-regression \
    scripts/xteink-x4-test-ble-transfer.sh
  git commit -m "feat: route native ble uploads through service.upload"
  ```

## Task 6: Implement Native HTTP Upload Transport

**Files:**

- Modify: `firmware/native/crates/squidscript-fw-x4/Cargo.toml`
- Modify: `firmware/native/crates/squidscript-fw-x4/src/main.rs`
- Modify: `firmware/native/crates/squidscript-fw-x4/src/lib.rs` if storage
  adapters need small additions

- [ ] Add `embassy-net` TCP support:

  ```toml
  embassy-net = { version = "0.9.1", default-features = false, features = ["medium-ethernet", "proto-ipv4", "dhcpv4", "udp", "tcp"], optional = true }
  ```

- [ ] Add native HTTP upload task:
  - Spawn only when `wifi` and `native-radio-services` are enabled.
  - Bind TCP port `80` on the AP stack.
  - Continue running even when no upload profile is active; return `404` or
    close cleanly until an HTTP upload transport is active.
  - Use fixed buffers; do not allocate per request proportional to file size.
  - Parse only the required HTTP subset:
    - request line
    - `Content-Length`
    - optional `Content-Range`
    - `Expect:` may be ignored because test scripts send `-H "Expect:"`
  - Reject unsafe names through the same `safe_upload_name` rules as BLE.

- [ ] Implement `HEAD /upload/<safe-name>`:
  - if active partial upload for same safe name exists, return current offset
  - otherwise return offset `0`
  - include `X-Squid-Upload-Offset`
  - include `X-Squid-Upload-Total` when known
  - never dispatch app events

- [ ] Implement `PUT /upload/<safe-name>`:
  - resolve route with `NativeUploadTransport::Http`
  - begin staging at offset `0` for a fresh upload
  - resume when `Content-Range` starts at the retained offset
  - write body chunks incrementally to SD
  - commit only when received bytes equal total bytes
  - dispatch configured complete event with `ev.transport == "http"`
  - delete `tmp/` after handler returns
  - return `200 OK` body `ok\n` only after commit and handler dispatch succeed

- [ ] Error responses:
  - `400` invalid name/header/range
  - `404` no active route or extension mismatch
  - `409` route ambiguous or offset mismatch
  - `413` target limit exceeded
  - `500` storage or dispatch failure

  Also record bounded device diagnostics for firmware-visible failures.

- [ ] Preserve serial/runtime responsiveness:
  - no blocking loops while reading sockets or writing SD
  - yield between chunks
  - do not hold the runtime mutex across long socket reads
  - hold the runtime mutex only while resolving route, writing a storage chunk,
    committing, or dispatching

- [ ] Run:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-x4 --features x4-binbook
  RUSTUP_TOOLCHAIN=nightly PATH=/var/home/tristan/codex-box/.cargo/bin:$PATH \
    cargo run -p squidc -- target build --target xteink-x4 --backend native
  ```

- [ ] Commit:

  ```bash
  git add firmware/native/crates/squidscript-fw-x4/Cargo.toml \
    firmware/native/crates/squidscript-fw-x4/src/main.rs \
    firmware/native/crates/squidscript-fw-x4/src/lib.rs
  git commit -m "feat: add native x4 http upload transport"
  ```

## Task 7: Add Unified Host CLI Upload Command

**Files:**

- Modify: `compiler/rust/crates/squidc-cli/src/main.rs`
- Modify: `compiler/rust/crates/squidc-cli/src/ble_push.rs`
- Create if useful: `compiler/rust/crates/squidc-cli/src/http_upload.rs`
- Modify: `docs/squidc_cli.md`

- [ ] Add failing CLI parse tests for:

  ```bash
  squidc device upload book.binbook --name book.binbook --transport http --host 192.168.4.1
  squidc device upload book.binbook --name book.binbook --transport ble --device SquidScript
  ```

- [ ] Implement `device upload`.
  - `--transport http` requires `--host`.
  - `--transport ble` requires `--device`.
  - `--name` is required and must pass existing safe-name validation.
  - default port for HTTP is `80`; allow `--port` only if the current CLI
    option model can express it without ambiguity with serial `--port`.

- [ ] HTTP upload client:
  - build URL `http://<host>/upload/<safe-name>`
  - use `HEAD` first to query resume offset
  - use `PUT` with body file
  - use `Content-Range` when resuming
  - report bytes, elapsed time, and bytes/sec in the same style as BLE upload
  - do not require `squidc` for normal curl compatibility; this is a helper

- [ ] BLE upload client:
  - move current `device ble-put` implementation behind
    `device upload --transport ble`
  - remove `device ble-put` from docs/tests and command parser
  - keep BLE UUIDs and firmware wire behavior unchanged

- [ ] Run:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc upload -- --nocapture
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc
  ```

- [ ] Commit:

  ```bash
  git add compiler/rust/crates/squidc-cli/src/main.rs \
    compiler/rust/crates/squidc-cli/src/ble_push.rs \
    compiler/rust/crates/squidc-cli/src/http_upload.rs \
    docs/squidc_cli.md
  git commit -m "feat: add unified device upload command"
  ```

## Task 8: Update Examples, Hardware Fixtures, And Docs

**Files:**

- Modify: `examples/ble-install/main.squid`
- Modify: `examples/ble-install/README.md`
- Modify: `examples/http-binbook-upload/main.squid`
- Modify: `examples/http-binbook-upload/README.md`
- Modify: `tests/hardware/xteink-x4/http-binbook-upload/main.squid`
- Modify: `scripts/xteink-x4-test-http-binbook-upload.sh`
- Modify: `scripts/xteink-x4-test-ble-transfer.sh`
- Modify: `docs/language_spec.md`
- Modify: `docs/runtime_limits.md`
- Modify: `docs/firmware_state_machines.md`
- Modify: `docs/hardware_target_tests.md`
- Modify: `ROADMAP.md`

- [ ] Replace example API calls:

  Old:

  ```squid
  service.ble.start("file-transfer", { ... })
  service.http.start("file-upload", { ... })
  ```

  New:

  ```squid
  service.upload.start({
    id: "book-upload"
    accept: [".binbook"]
    transports: ["http", "ble"]
    events: { complete: "upload.complete" }
  })
  ```

  Use `transports: ["ble"]` for BLE-only examples and `transports: ["http"]`
  for HTTP-only examples.

- [ ] Update handlers to use `upload.complete` and assert/log `ev.transport`.

- [ ] Update hardware scripts:
  - replace `device ble-put` with `device upload --transport ble`
  - add `device upload --transport http` coverage in addition to raw curl, or
    replace the script's curl upload with the CLI and keep one raw curl smoke
    step for protocol compatibility
  - preserve host Wi-Fi auto-detection and redaction discipline

- [ ] Update docs:
  - language spec section becomes `service.upload.*`
  - BLE and HTTP sections describe transports, not separate app-owned upload
    APIs
  - runtime limits describe upload profile count, accepted extensions,
    transport count, event route count, HTTP chunk buffer, BLE chunk queue, and
    native BLE watchdog
  - firmware state machines describe shared route/stage/dispatch core with
    transport-specific producers
  - CLI docs describe `device upload`

- [ ] Add this ROADMAP entry exactly under Developer Tooling or File/Content
  follow-ups:

  ```markdown
  - Add app-facing file management APIs: rename, move, copy, delete, and
    related result records for firmware-owned file references and logical
    libraries, so upload handlers can organize files beyond the current
    content-specific `file.copy` path.
  ```

- [ ] Run static doc/script tests:

  ```bash
  .venv/bin/python -m unittest scripts.tests.test_zephyr_hardware_suite
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidvm-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc
  ```

  If Python dependencies are missing, use the repo-local uv venv:

  ```bash
  uv venv .venv
  uv pip install --python .venv/bin/python Pillow
  .venv/bin/python -m unittest scripts.tests.test_zephyr_hardware_suite
  ```

- [ ] Commit:

  ```bash
  git add examples/ble-install examples/http-binbook-upload \
    tests/hardware/xteink-x4/http-binbook-upload \
    scripts/xteink-x4-test-http-binbook-upload.sh \
    scripts/xteink-x4-test-ble-transfer.sh \
    docs/language_spec.md docs/runtime_limits.md docs/firmware_state_machines.md \
    docs/hardware_target_tests.md ROADMAP.md
  git commit -m "docs: document unified upload service"
  ```

## Task 9: Run Full Automated Verification

**Files:** No source edits unless failures reveal missing implementation.

- [ ] Run core tests:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidvm-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-x4 --features x4-binbook
  RUSTUP_TOOLCHAIN=stable cargo test -p squid-device-protocol
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc
  ```

- [ ] Run native X4 build:

  ```bash
  RUSTUP_TOOLCHAIN=nightly PATH=/var/home/tristan/codex-box/.cargo/bin:$PATH \
    cargo run -p squidc -- target build --target xteink-x4 --backend native
  ```

- [ ] If any command fails, fix the failure in the owning layer with a failing
  test first. Do not work around stale tests until proving whether the test is
  stale or the code is wrong.

- [ ] Commit any fixes with explicit verification in the commit body.

## Task 10: Hardware Verify Native X4 Uploads

**Files:** No source edits unless hardware verification exposes a bug.

- [ ] Read `AGENTS.local.md` if present. Do not print or commit raw USB by-id,
  SSID, BSSID, MAC, local IP, or credential values.

- [ ] Flash the verified native X4 build:

  ```bash
  RUSTUP_TOOLCHAIN=stable PATH=/var/home/tristan/codex-box/.cargo/bin:$PATH \
    cargo run -p squidc -- target flash --target xteink-x4 --backend native
  ```

- [ ] Serial sanity checks:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo run --quiet -p squidc -- device reset --port /dev/ttyACM0
  RUSTUP_TOOLCHAIN=stable cargo run --quiet -p squidc -- device lifecycle --port /dev/ttyACM0
  RUSTUP_TOOLCHAIN=stable cargo run --quiet -p squidc -- device resources --port /dev/ttyACM0
  ```

  Expected: lifecycle responds and radio leases are zero when no upload app is
  active.

- [ ] BLE hardware gate:

  ```bash
  RUSTUP_TOOLCHAIN=stable scripts/xteink-x4-test-ble-transfer.sh \
    --target xteink-x4 --backend native --port /dev/ttyACM0 --skip-flash
  ```

  Expected:
  - script uses `service.upload.start({ transports: ["ble"] })`
  - CLI uses `device upload --transport ble`
  - handler sees `ev.transport == "ble"`
  - copied file passes size and CRC `device content-check`
  - interrupted transfer cleanup and reconnect still pass
  - `device errors` is empty

- [ ] HTTP hardware gate:

  ```bash
  RUSTUP_TOOLCHAIN=stable scripts/xteink-x4-test-http-binbook-upload.sh \
    --skip-flash
  ```

  Expected:
  - script uses `service.upload.start({ transports: ["http"] })`
  - host joins device AP using auto-detected Wi-Fi interface unless overridden
  - raw curl upload to `/upload/<safe-name>` succeeds
  - `device upload --transport http` succeeds
  - interrupted upload resumes through `HEAD` and `Content-Range`
  - handler sees `ev.transport == "http"`
  - copied file appears in `content.binbook.list("books")`
  - `device content-check` verifies size and CRC
  - `device errors` is empty
  - AP stop/reset cleanup returns radio leases to zero

- [ ] BinBook visible proof after HTTP upload:
  - launch the HTTP BinBook upload app
  - upload a real `.binbook`
  - open/read page zero
  - capture a fresh webcam image using `AGENTS.local.md`
  - inspect the capture and report only what is actually visible

- [ ] Commit any hardware-only script/doc fixes.

## Task 11: Final Cleanup And Push

**Files:** Whole repo.

- [ ] Search for old public upload API names:

  ```bash
  rg -n 'service\\.ble\\.start\\("file-transfer"|service\\.http\\.start\\("file-upload"|device ble-put|BLE profile trigger table|HTTP profile trigger table' .
  ```

  Expected: no current docs/tests/examples preserve old API names. It is
  acceptable for `ICEBOX.md` or explicitly historical commit text outside the
  working tree to mention old names; tracked current docs should not.

- [ ] Check dirty tree:

  ```bash
  git status --short --untracked-files=all
  ```

  Expected: only intentional changes remain.

- [ ] Run final verification bundle:

  ```bash
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidvm-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-core
  RUSTUP_TOOLCHAIN=stable cargo test -p squidscript-fw-x4 --features x4-binbook
  RUSTUP_TOOLCHAIN=stable cargo test -p squidc
  RUSTUP_TOOLCHAIN=nightly PATH=/var/home/tristan/codex-box/.cargo/bin:$PATH \
    cargo run -p squidc -- target build --target xteink-x4 --backend native
  ```

- [ ] Push after all commits are verified:

  ```bash
  git push
  ```

## Acceptance Criteria

- `service.upload.start/stop/status` are the only public app upload service APIs
  in current docs/examples/tests.
- Both HTTP and BLE transports can be enabled by one upload receiver.
- Both transports dispatch the same completion handler shape, including
  `ev.transport`.
- HTTP upload is reachable at fixed `PUT/HEAD /upload/<safe-name>`.
- Wi-Fi/AP remains explicitly script-owned.
- Native X4 HTTP and BLE hardware gates pass with size/CRC content proof.
- Uploaded file refs are ephemeral `tmp/<safe-name>` refs, valid during handler
  dispatch and deleted after return.
- Stop, reset, storage format, app replacement, runtime error, interrupted
  transfer, and route mismatch do not leave reusable partial files.
- `squidc device upload --transport http|ble` works; old `device ble-put` is
  removed from current docs/tests.
- ROADMAP contains the approved future file-management follow-up.
