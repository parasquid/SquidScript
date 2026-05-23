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
unsigned 64-bit integer, and `32` nested record. Repeated records are preserved
as repeated TLV fields with the same tag.

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

## Host Tooling

Normal workflows should use grouped `squidc` commands. Raw protocol access is
only for low-level protocol troubleshooting. `squidc protocol raw` builds one
binary request frame from an opcode name plus typed TLV fields and prints hex
request/response data.

The current implemented Zephyr command handler covers framed `hello` identity,
installed-app begin/chunk/commit, resource begin/chunk/commit, temp-run
begin/chunk/commit, app launch, app list, key dispatch, generic event dispatch,
output, trace, draw log, state export/import, errors, resources, reset, and
storage format over the UART serial transport. App install, temp run, and
resource install begin with field tag `1` app ID string, tag `2` total byte
length, and tag `3` CRC32 encoded as an unsigned 64-bit integer. Resource
install begin also uses field tag `4` for the package-relative resource path.
Each chunk uses tag `1` byte offset and tag `2` byte payload.

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
Rust also encodes `app-list`, lifecycle diagnostics, resource diagnostics,
state export responses, and protocol error responses directly into Zephyr's
caller-owned response buffer so Zephyr C does not stage duplicate TLV payload
arrays for those command responses. Repeated diagnostic line responses use the
same Rust encoder path. App launch, generic event dispatch, state import, and
Wi-Fi profile requests are parsed by Rust `sqdp_` FFI helpers, which return
borrowed field slices to Zephyr C for runtime/storage actions.

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
SquidScript VM calls to `service.wifi.status()`, `service.wifi.scan()`,
`service.wifi.startAP(...)`, `service.wifi.stopAP()`, and
`service.wifi.getAPIP()` are connected to Zephyr Wi-Fi management callbacks.
`service.wifi.connect(...)` and `service.wifi.disconnect()` use the provisioned
volatile profile and Zephyr station connect/disconnect requests. Wi-Fi command
output and hardware checks must stay redacted unless the user explicitly
requests raw identifiers.
`system.memory()` and `system.storage("apps")` are connected through the same
Zephyr VM FFI host boundary as other runtime services. `system.memory()` returns
a display-oriented RAM/heap diagnostic string. `system.storage("apps")` returns
a display-oriented free-space string for the mounted SquidScript app store.
SquidScript VM calls to `app.launch`, `app.arm`, and `app.disarm` are also
connected through the Zephyr FFI host. The `app-launch` and generic
`event-dispatch` command requests are parsed by Rust `sqdp_` FFI helpers before
Zephyr starts or dispatches the installed app.
`app.launch` and `app.exit` drive the Zephyr foreground return stack for
installed apps and clear foreground timers when a different installed app
becomes active. Zephyr preserves the active foreground VM's in-memory state
across non-lifecycle foreground event dispatches, such as key and foreground
timer handlers, so apps do not need to call `state.load()` for every event.
App launch, app-exit returns, and armed trigger activations start fresh VM
sessions and must use explicit persistent state when they need continuity.
`app.arm` reads the target app's SQBC trigger metadata, records bounded timer
registrations, and exposes them through `lifecycle-get`. Trigger registration
does not keep a background VM resident and does not dispatch a synthetic
foreground event. When an armed timer fires, Zephyr starts the armed app as
foreground and dispatches the registered event. `app.disarm` removes that
app's armed timer registrations.

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
`resources-get` reports `protocol_thread_stack_size_bytes`,
`protocol_thread_stack_unused_bytes`, `protocol_thread_stack_used_bytes`,
`vm_worker_stack_size_bytes`, `vm_worker_stack_unused_bytes`, and
`vm_worker_stack_used_bytes` so future stack budget reductions can be based on
representative real-device high-water data. It also reports live Zephyr heap
telemetry as `ram_heap_count`, `ram_heap_free_bytes`,
`ram_heap_allocated_bytes`, and `ram_heap_max_allocated_bytes`, so system-heap
budget reductions can be based on allocator high-water data instead of static
map size alone. `runtime_static_bytes` includes the Zephyr VM runtime object;
the runtime shares its VM initialization scratch buffer with later storage
completion transfer storage because those buffers are not live at the same
time; the ESP32-C3 Super Mini hardware suite currently reports
`runtime_static_bytes=16608`. The current ESP32-C3 reference configuration
keeps Zephyr's system heap at 49152 bytes because representative Wi-Fi status,
scan, list, and AP workloads measured `ram_heap_max_allocated_bytes=36764`;
remeasure before adding TCP, AP client throughput, BLE coexistence, or larger
Wi-Fi workloads.

Wi-Fi diagnostics should distinguish internal firmware/driver state from
external RF proof. A successful Zephyr Wi-Fi status record does not by itself
prove that another device can see or join an AP.
