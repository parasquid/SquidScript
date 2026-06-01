# SQBC Binary Format

Status: Current reference bytecode

SQBC is the executable path for SquidScript firmware.

## Browser Development Container

Older browser-sim experiments used an SQBC-looking wrapper around IR JSON. That
wrapper is not a firmware executable and must not be described as a supported
SQBC generation.

Firmware must reject IR JSON payloads. Browser-sim IR JSON is a development
artifact, not a production firmware executable.

## Reference Bytecode

```text
offset  size  field
0       4     magic: "SQBC"
4       2     little-endian u16 header length
6       4     little-endian u32 file length
10      4     little-endian u32 section count
14      12*n  section records
...     n     section payloads
```

This is the current real bytecode format used by Zephyr firmware. It is
intentionally small and exists to exercise the SquidScript language spec on
constrained hardware while moving installed apps toward metadata-first loading.

Section record:

```text
offset  size  field
0       2     little-endian u16 section kind
2       2     little-endian u16 flags, currently 0
4       4     little-endian u32 payload offset
8       4     little-endian u32 payload length
```

Initial section kinds:

```text
1  string pool
2  state table
3  function table
4  handler table
5  bytecode instruction stream
6  screen table
7  app metadata
8  device binding table
9  timer trigger table
10 BLE profile trigger table
```

Initial value tags:

```text
0  null
1  bool
2  i32
3  string id
```

Initial opcode subset:

```text
1   PUSH_INT i32
2   PUSH_BOOL u8
3   PUSH_STRING u16
4   PUSH_NULL
10  GET_STATE u16
11  SET_STATE u16
12  GET_LOCAL u16
13  SET_LOCAL u16
20  ADD
21  SUB
22  EQ
23  NE
24  LT
25  LTE
26  GT
27  GTE
30  JUMP u32
31  JUMP_IF_FALSE u32
40  CALL_FUNCTION u16 function_id, u16 arg_count
41  RETURN
42  HALT
50  CALL_BUILTIN u8
60  POP
```

Built-in IDs are grouped by service family. SQBC is pre-1.0, so this table may
be refreshed as capability boundaries become clearer.

```text
0x01 state.load
0x02 state.save
0x03 state.reset
0x04 debug.print
0x05 system.memory
0x06 system.storage
0x07 system.startReason

0x10 app.exit
0x11 app.launch
0x12 app.arm
0x13 app.disarm
0x14 app.registry
0x15 app.registry.get
0x16 app.processStack
0x17 app.armedStack
0x18 app.armedStack.get

0x20 screen.open
0x21 screen.refresh
0x22 service.timer.every
0x23 service.timer.after

0x30 service.display.clear
0x31 service.display.text
0x32 service.display.rect
0x33 service.display.line
0x34 service.display.select
0x35 service.display.image
0x36 service.display.draw
0x37 service.display.info

0x40 hardware.gpio.write
0x41 hardware.gpio.toggle
0x42 hardware.gpio.read
0x48 service.indicator.write
0x49 service.indicator.toggle
0x4a service.indicator.read
0x4b service.indicator.breathe
0x4c service.indicator.blink

0x50 service.wifi.startAp
0x51 service.wifi.stopAp
0x52 service.wifi.status
0x53 service.wifi.getApIp
0x54 service.wifi.connect
0x55 service.wifi.disconnect
0x56 service.wifi.scan
0x57 service.wifi.operation
0x58 service.wifi.result
0x59 service.wifi.cancel
0x5a service.wifi.scanNetwork

0x60 reserved for service.ble/service.bluetooth

0x70 device.config.load
0x71 device.config.set
0x72 device.config.rebind
0x73 device.config.save

0x80 reserved for binbook

0x90 file.pickFile
0x91 file.readText
0x92 file.readLines

0xa0 reserved for service.storage
0xb0 reserved for service.input
0xc0 service.power.sleep
0xd0 reserved for service.time
```

Reserved ranges do not imply implemented language APIs. They keep likely future
service families from forcing unrelated renumbering while SQBC is still
pre-1.0. Candidate future families include Bluetooth/BLE around `0x60`,
top-level `binbook.*` around `0x80`, expanded file operations around `0x90`,
portable storage operations around `0xa0`, input-device operations around
`0xb0`, power and battery operations around `0xc0`, and time/clock operations
around `0xd0` once their portable SquidScript contracts are specified.

The current format supports the headless VM subset. Display draw commands are
emitted as headless draw-log records by firmware hosts that implement the
display service. The current Zephyr draw-log records cover clear, text, rect,
line, select, image, and draw commands. `service.display.info` returns the
active display service descriptor as a read-only result record. GPIO builtins dispatch to target firmware hardware modules;
unsupported names return a VM operand error. The canonical lifecycle surface is
generic events plus `app.start`, `app.triggers`, `app.arm`, `app.disarm`, and
`service.timer.*`. `app.triggers` is the authored trigger-registration surface;
the compiler encodes its timer declarations in a dedicated trigger metadata
section so firmware can arm an installed app without dispatching foreground
code or keeping a background VM resident. `app.launch` remains the app
replacement/launch primitive.

SQBC includes an explicit app metadata section so tools can read the app id from
bytecode without guessing from the string table. `squidc app install` uses this
metadata for raw `.sqbc` files. Source installs use the `app "id"` declaration;
if source omits it in a developer workflow, `squidc` generates a deterministic
id from the filename and content hash.

Handler table entries are:

```text
offset  size  field
0       2     little-endian u16 event string id
2       1     preload hint from @preload, 0 or 1
3       1     reserved, must be 0
4       2     little-endian u16 handler payload parameter count
6       2     little-endian u16 local slot count
8       4     little-endian u32 bytecode offset
12      4     little-endian u32 bytecode length
```

The preload hint is advisory. Firmware may use it to load or retain
latency-sensitive handler chunks, but app correctness must not depend on it.
Handler payload parameters are currently used for event records such as
`event.on("ble.object.complete", ev)`.

Trigger table entries are:

```text
offset  size  field
0       2     little-endian u16 event string id
2       1     repeating flag, 0 for one-shot and 1 for repeating
3       1     reserved, must be 0
4       4     little-endian i32 interval in milliseconds
```

The trigger table contains the compiled `app.triggers` declarations for timer
sources. Firmware reads this section during `app.arm(appId)` and records the
timer registrations directly.

BLE profile trigger table payload:

```text
offset  size  field
0       2     little-endian u16 profile count
...           variable profile records
```

Each profile record is:

```text
offset  size  field
0       2     little-endian u16 profile name string id
2       2     little-endian u16 app-local profile id string id
4       2     little-endian u16 role string id
6       2     little-endian u16 accept count
8       2*n   accepted extension string ids
...     2     little-endian u16 event route count
...     4*n   event route pairs: kind string id, event string id
```

Current BLE profile metadata is emitted for
`service.ble.profile("object-transfer", ...)` declarations inside
`app.triggers`. Firmware reads this metadata to arm object-transfer trigger
profiles without dispatching foreground app code.

Zephyr firmware must install named SQBC apps, start `main`, arm trigger
registrations, dispatch real timer events, and exercise app-stack behavior.
Installed SQBC payloads live in Zephyr-owned app storage. Startup registry
rebuilds validate installed apps from the header and section table with bounded
reads rather than mirroring full app bodies in RAM.

For a headless app entry source with no authored `screen(...)` declarations,
the compiler emits one empty synthesized `main` screen. This keeps runtime
screen metadata uniform without adding display side effects.

The device binding table encodes top-level `device {}` declarations as a count
followed by service string id, binding-name string id, and resource string id
for each binding. The resource string is either a safe package-relative
`.sqdevice` path, a simple inline GPIO endpoint such as `gpio:GPIO8`, or an
inline GPIO-button input endpoint such as
`gpio-button:GPIO9:key.SELECT:activeLow`.
Firmware and browser runtimes use this metadata to apply bindings before
`event.on("app.start")`. Package installers store `.sqdevice` resources as
ordinary read-only package files; active resolved config is firmware-owned SQDC,
not embedded mutable package state. Inline resources normalize to SQDC metadata
without installing a package resource.

## Chunk/Index Execution

Installed-app SQBC execution reads a small owned metadata/index first, then
loads executable handler/function/screen ranges from the code section on demand.
The execution shape is:

```text
|-- header and section table
|-- app metadata
|-- string pool
|-- state table
|-- function index
|-- handler index with preload flags
|-- screen index
|-- executable chunks
```

Firmware loads handler/function/screen chunks from app storage as needed.
Active chunks cannot be evicted. Inactive chunks are cache entries and may be
evicted at any time. Preloaded chunks have higher cache priority but are not
pinned. Dropping a chunk is not app lifecycle behavior and does not dispatch
`event.on("app.exit")`.

Installed-app and temp-run execution use bounded indexed reads from Zephyr-owned
storage rather than assuming a memory-mapped contiguous app image. Use LittleFS
where a file layout is needed and NVS or LittleFS records for app state based on
implementation tests. `RUN.TEMP` stages bytecode as a temporary app-store file
but keeps temp app state volatile.

The current Zephyr implementation stores installed `main.sqbc` files in
LittleFS and dispatches them through the `SqvmStorageRequest::SqbcRead`
boundary. The resident runtime holds the parsed metadata/index, VM state, and
one bounded SQBC code/read window; it does not need to keep the full installed
file in RAM. App trigger registration reads only trigger metadata from the same
reader path so an armed app does not need a background VM instance.
