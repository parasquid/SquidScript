# SQBC Binary Format

Status: v1 browser development container plus v2 reference bytecode

SQBC is the executable path for SquidScript firmware.

## v1 Container

```text
offset  size  field
0       4     magic: "SQBC"
4       4     little-endian u32 version: 1
8       4     little-endian u32 payload length
12      n     payload
```

The temporary v1 payload is versioned IR JSON. This format is a browser
simulator development artifact only.

Firmware must reject SQBC v1 IR payloads and browser-only `entry.type = "ir"`
manifests.

## v2 Reference Bytecode

SQBC v2 is the first real bytecode format used by the reference firmware. It is
intentionally small and exists to exercise the SquidScript language spec on
constrained hardware.

```text
offset  size  field
0       4     magic: "SQBC"
4       2     little-endian u16 version: 2
6       2     little-endian u16 header length
8       4     little-endian u32 file length
12      4     little-endian u32 section count
16      12*n  section records
...     n     section payloads
```

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
6  display.clear
7  display.text
8  display.rect
9  display.line
10 hardware.gpio.write
11 hardware.gpio.toggle
12 hardware.gpio.read
13 app.launch
14 reserved
15 reserved
16 app.arm
17 app.disarm
18 event.addSource
```

The v2 format currently supports the headless reference VM subset. Display
draw commands are emitted as headless draw-log records on the ESP32-C3 Super
Mini reference firmware. GPIO builtins dispatch to target firmware hardware
modules; unsupported names return a VM operand error. The canonical lifecycle
surface is generic events plus `app.start`, `app.arm`, `app.disarm`, and
`event.addSource`. `app.launch` remains the app replacement/launch primitive.

SQBC v2 includes an explicit app metadata section so tools can read the app id
from bytecode without guessing from the string table. `squidc app install` uses this
metadata for raw `.sqbc` files. Source installs use the `app "id"` declaration;
if source omits it in a developer workflow, `squidc` generates a deterministic
id from the filename and content hash.

The ESP32-C3 reference firmware uses fixed RAM app slots as an E2E harness. It
can install named SQBC apps, start `main`, arm trigger registrations, dispatch
real timer events, and exercise app-stack behavior. Persistent app storage,
manifests, and source maps remain outside this firmware milestone.
