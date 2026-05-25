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

Initial built-in IDs:

```text
1  state.load
2  state.save
3  app.exit
4  debug.print
5  screen.open
6  service.display.clear
7  service.display.text
8  service.display.rect
9  service.display.line
10 hardware.gpio.write
11 hardware.gpio.toggle
12 hardware.gpio.read
13 app.launch
14 state.reset
15 screen.refresh
16 app.arm
17 app.disarm
18 service.timer.every
19 service.timer.after
20 system.memory
21 system.storage
22 service.display.select
23 service.display.image
24 service.display.draw
25 device.config.load
26 device.config.set
27 service.indicator.write
28 service.indicator.toggle
29 service.indicator.read
30 service.wifi.startAp
31 service.wifi.stopAp
32 service.wifi.status
33 service.wifi.getApIp
34 service.indicator.breathe
35 service.wifi.connect
36 service.wifi.disconnect
37 service.wifi.scan
38 app.registry
39 app.registry.get
40 app.processStack
41 app.armedStack
42 app.armedStack.get
43 device.config.rebind
44 device.config.save
45 service.indicator.blink
```

The current format supports the headless VM subset. Display draw commands are
emitted as headless draw-log records by firmware hosts that implement the
display service. The current Zephyr draw-log records cover clear, text, rect,
line, select, image, and draw commands. GPIO builtins dispatch to target firmware hardware modules;
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
2       2     little-endian u16 flags
4       4     little-endian u32 bytecode offset
8       4     little-endian u32 bytecode length
```

Handler flags:

```text
bit 0  preload hint from @preload
```

The preload bit is advisory. Firmware may use it to load or retain
latency-sensitive handler chunks, but app correctness must not depend on it.

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
`.sqdevice` path or a simple inline GPIO endpoint such as `gpio:GPIO8`.
Firmware and browser runtimes use this metadata to apply bindings before
`event.on("app.start")`. Package installers store `.sqdevice` resources as
ordinary read-only package files; active resolved config is firmware-owned SQDC,
not embedded mutable package state. Inline GPIO resources normalize to the same
active binding model without installing a package resource.

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
