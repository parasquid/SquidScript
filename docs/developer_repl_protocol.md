# Developer Device Protocol

Status: shared host/firmware codec established; command, lifecycle,
diagnostics, state, resources, and storage helpers use framed requests.

The current real firmware path is Zephyr. The developer device protocol is the
Zephyr-owned command surface used by `squidc run`, `squidc app`, `squidc repl`,
and `squidc device`.

The old ESP32-C3 Rust firmware line protocol is obsolete reference material.
Do not preserve its command names, response markers, or storage behavior unless
the same shape is deliberately implemented as the current Zephyr protocol.

## Required Commands

The Zephyr command surface must cover:

- firmware hello/identity and target diagnostics
- app install, temp run, launch, app list, and reset
- key and generic event dispatch
- output, trace, draw log, state, errors, and resources
- storage format
- Wi-Fi status, scan, AP, station, and profile provisioning

## Frame Format

The current protocol is binary framed. The old text-line protocol markers such
as `INSTALL.APP`, `RUN.TEMP`, `APP.LIST`, `BEGIN`, `END`, `OK`, and `ERR` are
not current wire compatibility requirements.

Every frame starts with a 20-byte little-endian header:

| Offset | Size | Field |
| --- | ---: | --- |
| 0 | 4 | magic bytes `SQDP` |
| 4 | 1 | frame kind: `1` request, `2` response, `3` event |
| 5 | 1 | opcode |
| 6 | 1 | status: `0` ok, `1` error, `2` pending |
| 7 | 1 | reserved, currently `0` |
| 8 | 4 | sequence |
| 12 | 4 | payload length |
| 16 | 4 | payload CRC32 |

The payload is a compact TLV stream. Each field uses one byte for tag, one byte
for type, two little-endian bytes for value length, then the value bytes. Field
types are `0` bytes, `1` UTF-8 string, `3` bool, `4` signed 64-bit integer, `5`
unsigned 64-bit integer, `6` unsigned 32-bit integer, and `32` nested record.
Repeated records are preserved as repeated TLV fields with the same tag.

The authoritative host codec lives in the shared Rust
`squid-device-protocol` crate. Zephyr links that code through `squidvm-ffi`
for heap-free response encoders and install/temp/resource session validators
exposed with the `sqdp_` C ABI. Rust owns TLV field extraction, byte-count
limits, chunk offset checks, incremental CRC32 progress, and commit readiness
for large writes. Zephyr C still owns UART, LittleFS, VM runtime, GPIO/Wi-Fi
drivers, work queues, timers, and ztest glue, and only advances a Rust-validated
session after the corresponding Zephyr storage operation succeeds. Production
Zephyr C keeps only frame decode and payload CRC validation in its local
protocol module; C TLV builders/readers are test-local harness helpers.

Large writes use begin/chunk/commit opcode groups for installed apps, temp
apps, and resources. Chunk payloads must carry explicit byte lengths and
integrity through frame length and payload CRC32. Credential provisioning must
report profile names and byte lengths only; it must not echo SSIDs, passwords,
BSSIDs, MAC addresses, local IP addresses, or other environment-identifying
values unless the user explicitly asks for raw identifiers.

`storage-format` is an administrative app-store maintenance command with
bounded protocol progress. The first accepted request clears the foreground
runtime, transfer sessions, and mutable registry, then starts a format job in
protocol scratch. Each request deletes or prepares one bounded filesystem unit
and returns `PENDING` with a progress line until the final request returns
`OK`. Host tooling should repeat the same command until it receives `OK`;
single-frame protocol callers must not assume storage format completes in one
firmware handler call.

## Host Tooling

Normal workflows should use grouped `squidc` commands. Raw protocol access is
only for low-level protocol troubleshooting. `squidc protocol raw` builds one
binary request frame from an opcode name plus typed TLV fields and prints hex
request/response data.

The current implemented Zephyr command handler covers framed `hello` identity,
installed-app begin/chunk/commit, resource begin/chunk/commit, temp-run
begin/chunk/commit, app launch, app list, key dispatch, generic event dispatch,
output, trace, draw log, state export/import, errors, resources, reset, and
storage format over the UART serial transport. App install and temp run begin
with field tag `1` app ID string, tag `2` total byte length, and tag `3` CRC32
encoded as an unsigned 64-bit integer. Resource install begin uses tag `1` for
the app ID, tag `2` for the package-relative resource path, tag `3` for total
byte length, and tag `4` for CRC32. The firmware-side resource path staging
capacity is 80 bytes including the terminating NUL, so package-relative
resource paths must be at most 79 bytes in this reference firmware.
Each chunk uses tag `1` byte offset and tag `2` byte payload.
Host app launch follows the same foreground lifecycle path as app-requested
launches. The launch command enqueues the lifecycle transition and returns a
protocol OK response once the request is accepted; later app-start failures such
as target-rejected device bindings remain visible through `device errors`.

The ESP32-C3 Zephyr reference firmware accepts app IDs up to 39 bytes. The
shared Rust host protocol, Rust FFI validator, and Zephyr app-store buffers use
the same 40-byte storage capacity including the terminating NUL byte.

Install commit verifies byte count and CRC32 before publishing
`/sq/apps/<app-id>/main.sqbc`; resource commit publishes under
`/sq/apps/<app-id>/<resource-path>`; temp-run commit verifies byte count and
CRC32 before launching the temporary foreground app from
`/sq/tmp/temp-run.sqbc.tmp`.

Installed-app, temp-run, and resource begin/chunk/commit commands use
caller-owned buffers across the C/Rust boundary. Zephyr passes the received
frame and its fixed session storage to Rust; Rust returns a bounded action with
borrowed byte slices or stored session strings; Zephyr performs the filesystem
or VM operation and then calls the completion function so Rust updates progress.
Host tooling derives upload chunk payload size from the 256-byte encoded
protocol frame budget so install, resource, and temp-run chunk frames fit the
firmware's fixed serial receive buffer without increasing firmware RAM. The
current transfer chunk frame has 36 bytes of protocol overhead, leaving a
220-byte upload payload per chunk.
Rust also encodes `app-list`, lifecycle diagnostics, state export responses,
and protocol error responses directly into Zephyr's caller-owned response
buffer so Zephyr C does not stage duplicate TLV payload arrays for those command
responses. Repeated diagnostic line responses use the same Rust encoder path.
Zephyr C directly encodes resource diagnostics into the same caller-owned
response buffer because those values are already native runtime measurements;
it does not keep a resident metric staging array. App launch, generic event
dispatch, state import, and Wi-Fi profile requests are parsed by Rust `sqdp_`
FFI helpers, which return borrowed field slices to Zephyr C for runtime/storage
actions.

`app-list` responses use repeated record fields: response field tag `1` is one
app record, record field tag `1` is the app ID string, and record field tag `2`
is the SQBC length as an unsigned 64-bit integer.

`output-get`, `trace-get`, `drawlog-get`, `lifecycle-get`, and `errors-get` responses use
repeated string fields with tag `1`, one field per line. `state-get` returns
state bytes as field tag `1`; Rust owns the response encoder while Zephyr owns
the storage load and supplies the caller-owned state byte buffer.
Runtime error lines include the mapped VM FFI status label and errno, for
example `runtime=vm_error code=-5` or
`runtime=invalid_argument code=-22`.
`state-import` request TLV parsing is owned by the Rust `sqdp_` FFI helper and
returns a borrowed state byte slice to Zephyr C for storage. `resources-get`
returns repeated record fields: response field tag `1` is one resource record,
record field tag `1` is the metric key string, and record field tag `2` is the
value as an unsigned 64-bit integer.

Wi-Fi profile provisioning uses the framed opcode to store one volatile,
bounded station profile in Zephyr runtime memory. Rust `sqdp_` FFI code owns
the request TLV parsing and returns borrowed field slices to Zephyr C, which
only applies the validated profile to the runtime. The command response is
empty on success and must not echo SSIDs or passwords.
SquidScript VM calls to `service.wifi.status()`, `service.wifi.startAP(...)`,
`service.wifi.stopAP()`, `service.wifi.connect(...)`,
`service.wifi.disconnect()`, `service.wifi.scan()`,
`service.wifi.operation()`, `service.wifi.result()`,
`service.wifi.cancel()`, `service.wifi.scanNetwork(index)`, and
`service.wifi.getAPIP()` are connected to Zephyr Wi-Fi management callbacks.
Operation-starting calls return immediately; apps poll operation/result records
from timers instead of blocking serial/runtime progress. Station connect and
disconnect use the provisioned volatile profile and Zephyr station requests.
Wi-Fi command output and hardware checks must stay redacted unless the user
explicitly requests raw identifiers.
`system.memory()` and `system.storage("apps")` are connected through the same
Zephyr VM FFI host boundary as other runtime services. `system.memory()` returns
a display-oriented RAM/heap diagnostic string. `system.storage("apps")` returns
a display-oriented free-space string for the mounted SquidScript app store.
SquidScript VM calls to `app.launch`, `app.arm`, and `app.disarm` are also
connected through the Zephyr FFI host. The `app-launch` and generic
`event-dispatch` command requests are parsed by Rust `sqdp_` FFI helpers before
Zephyr starts or dispatches the installed app.
`app.launch`, host `app-launch`, and `app.exit` drive the Zephyr foreground
return stack for installed apps and clear foreground timers when a different
foreground app becomes active. Host `app-launch` follows the same lifecycle
chain as an in-app `app.launch`: if there is no current foreground app, firmware
starts the logical root first; then the current foreground app receives
`app.exit`, its app id remains on the return stack, and the requested app starts
as foreground. If the logical root is needed and installed `main` is absent,
firmware uses the target-specific built-in fallback app as logical `main`.
The command response is delayed until that bounded lifecycle chain drains, so a
successful host launch means the target app has been selected through the same
foreground handoff path used by app-driven launch.
See `docs/app_lifecycle_state_machine.md` for the full lifecycle state machine,
failure cases, fallback `main` behavior, and reset versus storage-format test
isolation guidance.
Zephyr preserves the active foreground VM's in-memory state across
non-lifecycle foreground event dispatches, such as key and foreground timer
handlers, so apps do not need to call `state.load()` for every event. App
launch, app-exit returns, and armed trigger activations start fresh VM sessions
and must use explicit persistent state when they need continuity.
`app.arm` reads the target app's SQBC trigger metadata, records bounded timer
registrations, and exposes them through `lifecycle-get`. Trigger registration
uses a dedicated installed-app VM storage backend so metadata reads cannot
overwrite the active foreground app backend. It does not keep a background VM
resident and does not dispatch a synthetic foreground event. When an armed
timer fires, Zephyr starts the armed app as foreground and dispatches the
registered event. `app.disarm` removes that app's armed timer registrations.

Planned sleep is requested from SquidScript with
`service.power.sleep({ wakeAfterMs })`. Zephyr waits until the current VM event
returns, dispatches `power.sleep` to the current foreground app, writes the
planned-resume lifecycle record, configures the target wake source, and then
enters sleep. On ESP32-C3 the first supported wake source is timer wake from
`wakeAfterMs`.

The planned-resume record restores only foreground lifecycle routing: active
foreground app id, foreground return stack app ids, and armed app ids. Firmware
does not persist foreground timers or VM execution frames. On wake restore the
foreground app receives `app.start` and `system.startReason()` returns `"wake"`;
if it later calls `app.exit()`, the restored return stack restarts the previous
app with start reason `"return"`.

## Diagnostics

Resource diagnostics should report RAM numbers separately from flash storage
numbers. When the user asks for "memory" without qualification, report RAM by
default.

Zephyr builds should keep RAM usage visible with `scripts/zephyr-ram-audit.sh`.
The default guard for the ESP32-C3 Zephyr slice is `266240` bytes in
`dram0_0_seg`; override it only with `SQUID_ZEPHYR_DRAM_LIMIT_BYTES` when a
change intentionally changes the measured budget. The audit also emits
structured `ram_symbol[N]=size=... addr=... type=... name=...` lines for the
largest static DRAM symbols; adjust the count with
`SQUID_ZEPHYR_RAM_SYMBOL_COUNT` when investigating RAM optimization candidates.
When `SQUID_ZEPHYR_TARGET_JSON` is supplied, the RAM audit derives its default
limit from target SRAM metadata and `SQUID_ZEPHYR_RAM_PROFILE_PERCENT`, which
defaults to 65. For the ESP32-C3 Super Mini target, the metadata declares
400 KiB internal SRAM, so the 65% profile limit is 266240 bytes. The audit
still reports the Zephyr linker section bytes, because `dram0_0_seg` placement
is the immediate firmware build constraint.
`resources-get` reports `proto_stack_size_bytes`,
`proto_stack_unused_bytes`, `proto_stack_used_bytes`,
`vm_stack_size_bytes`, `vm_stack_unused_bytes`, and
`vm_stack_used_bytes` so future stack budget reductions can be based on
representative real-device high-water data. It also reports
`runtime_status`, `runtime_dispatch_started`, `runtime_dispatch_age_us`,
`runtime_work_submitted`, `runtime_current_app_present`,
`runtime_lifecycle_phase`, and `runtime_arm_phase` so a responsive serial
protocol can distinguish a running or wedged VM dispatch from an idle runtime
while triaging lockups. It also reports live Zephyr heap
telemetry as `heap_count`, `heap_free_bytes`,
`heap_alloc_bytes`, `heap_max_alloc_bytes`,
`heap_largest_free_supported`, and `heap_largest_free_bytes`, so system-heap
budget reductions can be based on allocator high-water data and explicit
fragmentation-probe availability instead of static map size alone. A
`resources-get` request may include bool field tag `1` set to `true` to reset
Zephyr's heap allocation high-water statistic to the current allocated bytes
before the response is sampled; the CLI exposes this as
`device resources --reset-heap-max` for workload-boundary attribution. Current
Zephyr public heap stats do not expose a safe non-mutating largest-free-block
query: `sys_heap_runtime_stats_get()` returns only free, allocated, and
max-allocated bytes, while `sys_heap_print_info()` prints bucket details instead
of returning a bounded numeric value. The heap listener API reports allocation
and free events, not the current largest free block. ESP32-C3 firmware therefore
reports `heap_largest_free_supported=0` and `heap_largest_free_bytes=0` until a
safe probe or allocation-failure mitigation is added. `system.memory()`
remains a display-oriented summary; use `device resources` for raw heap
diagnostics. `runtime_static_bytes` includes the Zephyr VM runtime object;
the runtime shares its VM initialization scratch buffer with later storage
completion transfer storage because those buffers are not live at the same
time. `vm_sqbc_chunk_bytes` reports the bounded 768-byte SQBC code/read
transfer window used for file-backed installed app dispatch; the full installed
`main.sqbc` payload is not resident in that window. Use
`scripts/zephyr-static-buffer-report.sh` for static ownership attribution: the
current report separates SquidScript-owned runtime/protocol buffers from
Zephyr, ESP, network, Wi-Fi, heap, and stack symbols. The ESP32-C3 Super Mini
canonical configuration keeps Zephyr's system heap at 45,056 bytes.
Representative app, display, system-resource, and Wi-Fi AP start/stop workloads
with reset-bounded heap high-water rows measured Wi-Fi AP start at
`heap_max_alloc_bytes=36432` and Wi-Fi AP stop at `heap_max_alloc_bytes=36460`,
leaving at least 8,596 bytes below the configured heap ceiling. Remeasure before
adding TCP, AP client throughput, BLE coexistence, or larger Wi-Fi workloads.

Wi-Fi diagnostics should distinguish internal firmware/driver state from
external RF proof. A successful Zephyr Wi-Fi status record does not by itself
prove that another device can see or join an AP.
