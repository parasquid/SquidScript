# SquidScript Language Specification v0.2 Draft

Status: Draft
Target: ESP32-C3 low-RAM e-ink/display devices
Primary execution format: precompiled bytecode
Source language: JavaScript-like authoring language
Compiler: squidc
Runtime/VM: squidvm
Source extension: .squid
Bytecode extension: .sqbc
Optional debug metadata: source-map.json
Design companion: docs/language_philosophy.md

---

## 1. Overview

SquidScript is a small JavaScript-like DSL for user-authored miniapps on low-RAM e-ink/display devices.

SquidScript is not JavaScript.

SquidScript uses familiar JavaScript-style syntax for authoring, but it does not implement JavaScript's object model, prototype system, closures, async model, browser APIs, Node APIs, package system, or general dynamic execution.

SquidScript is an imperative, event-driven DSL with simple procedural functions and capability-oriented platform APIs. It is not object-oriented and not functional in v0.2.

SquidScript separates the core language from the standard platform capability set. Core language features define syntax, control flow, values, handlers, screens, state, and bytecode execution semantics. Standard platform capabilities are namespaced firmware/runtime APIs such as `display.*`, `state.*`, `content.*`, and `binbook.*`.

SquidScript v0.2 does not provide a user library or package system. Standard capabilities are built in from an app author's perspective, but they should be specified as permissioned, bounded, namespaced APIs known to `squidc`, `.sqbc` validation, and `squidvm`, not as special syntax.

SquidScript is compiled off-device by squidc into .sqbc bytecode.

Production firmware loads, validates, and executes .sqbc bytecode.

Production firmware is not required to parse or compile .squid source.

Developer firmware may optionally include an on-device source compiler, but this is not part of the normal production architecture.

The firmware owns:
- boot
- launcher
- input
- display
- storage
- power management
- permissions
- app lifecycle
- bytecode validation
- bytecode execution
- crash recovery
- document capabilities such as BinBook

SquidScript apps own:
- app behavior
- screen definitions
- input handling
- persistent app state
- simple file/data interactions through safe APIs
- calls to safe firmware capabilities

---

## 2. Design Goals

SquidScript should support:
- user-created SD-card miniapps
- off-device compilation
- compact bytecode execution
- deterministic event-driven behavior
- bounded memory use
- bounded execution time
- e-ink-friendly rendering
- persistent app state
- key/button handling
- safe document/file access
- optional source-level diagnostics through source maps

SquidScript should avoid:
- SD-loaded native binaries
- direct hardware access from apps
- direct memory access
- raw framebuffer mutation
- unbounded loops
- recursion in v0.2
- dynamic code evaluation
- full JavaScript compatibility
- arbitrary object mutation
- unrestricted filesystem access
- large runtime parser/compiler requirements on-device

SquidScript should not have undefined behavior.

Invalid source should be rejected by squidc when statically detectable.

Invalid dynamic behavior should stop the current app with a structured squidvm runtime error.

Firmware must not continue execution after type errors, invalid handles, permission failures, arithmetic faults, out-of-bounds access, unsupported bytecode, or malformed capability data.

---

## 3. Architecture Summary

Authoring flow:

```text
User writes .squid source
  -> squidc resolves includes
  -> squidc parses and validates source
  -> squidc emits .sqbc bytecode
  -> squidc optionally emits source-map.json
  -> app folder is copied to SD card
  -> ESP32-C3 firmware validates .sqbc
  -> squidvm executes bytecode
```

Production device execution flow:

1. Firmware loads the active launcher.
2. Launcher scans or requests the installed app list.
3. Launcher selects an app to run.
4. Runtime loads the selected app's app.json.
5. Runtime loads declared .sqbc file.
6. Runtime validates bytecode structure and permissions.
7. Runtime initializes state.
8. Runtime executes event handlers.
9. Runtime renders screens.
10. Runtime records recoverable errors and crash diagnostics.
11. When the app exits, crashes, or is rejected, firmware returns control to the launcher.

Production firmware must execute .sqbc.

Production firmware does not need to compile .squid.

---

## 4. File Layout

Minimum production app:

```text
/sd/apps/simple-counter/
|-- app.json
`-- main.sqbc
```

Debuggable app:

```text
/sd/apps/simple-counter/
|-- app.json
|-- main.sqbc
|-- source-map.json
|-- main.squid
`-- lib/
    `-- ui.squid
```

Complex app:

```text
/sd/apps/binbook-reader/
|-- app.json
|-- main.sqbc
|-- source-map.json
|-- main.squid
|-- lib/
|   `-- ui.squid
|-- screens/
|   |-- reader.squid
|   `-- menu.squid
|-- icon.bmp
`-- data/
```

System-managed files:

```text
/sd/system/
|-- app-state/
|   `-- binbook-reader.json
|-- app-errors/
|   `-- binbook-reader.txt
|-- app-cache/
|-- launcher-state.json
`-- crashlog.txt
```

Apps may read their own app directory and data directory only if granted permissions.

Apps may read external content files only through explicit user selection or launcher-provided file association.

Apps may not directly write to /sd/system.

---

## 5. Manifest: app.json

Every app must include app.json.

Production bytecode app example:

```json
{
  "format": "squidapp-v1",
  "id": "simple-counter",
  "name": "Simple Counter",
  "kind": "app",
  "version": "1.0.0",
  "runtime": {
    "language": "squidscript",
    "version": "0.2"
  },
  "entry": {
    "type": "bytecode",
    "file": "main.sqbc"
  },
  "source": {
    "file": "main.squid",
    "optional": true
  },
  "sourceMap": {
    "file": "source-map.json",
    "optional": true
  },
  "permissions": [
    "state.read",
    "state.write",
    "display.draw"
  ],
  "requires": {
    "runtime": "squidscript>=0.2",
    "display": {
      "minWidth": 480,
      "minHeight": 800,
      "pixelFormats": ["GRAY1_PACKED", "GRAY2_PACKED"]
    },
    "keys": ["SELECT", "BACK"],
    "features": [
      "display.draw",
      "state.read",
      "state.write"
    ]
  },
  "opens": []
}
```

BinBook reader example:

```json
{
  "format": "squidapp-v1",
  "id": "binbook-reader",
  "name": "BinBook Reader",
  "kind": "app",
  "version": "1.0.0",
  "runtime": {
    "language": "squidscript",
    "version": "0.2"
  },
  "entry": {
    "type": "bytecode",
    "file": "main.sqbc"
  },
  "source": {
    "file": "main.squid",
    "optional": true
  },
  "sourceMap": {
    "file": "source-map.json",
    "optional": true
  },
  "permissions": [
    "state.read",
    "state.write",
    "display.draw",
    "content.pick",
    "content.read",
    "binbook.read"
  ],
  "requires": {
    "runtime": "squidscript>=0.2",
    "display": {
      "minWidth": 480,
      "minHeight": 800,
      "pixelFormats": ["GRAY2_PACKED"]
    },
    "keys": ["LEFT", "RIGHT", "BACK"],
    "features": [
      "display.draw",
      "state.read",
      "state.write",
      "content.pick",
      "content.read",
      "binbook.read"
    ]
  },
  "opens": [
    {
      "extension": ".binbook",
      "label": "BinBook"
    }
  ]
}
```

Required manifest fields:
- format
- id
- name
- kind
- version
- runtime.language
- runtime.version
- entry.type
- entry.file
- permissions

Optional manifest fields:
- source
- sourceMap
- requires
- targets
- opens
- icon
- description
- author
- data_formats

entry.type values:
- bytecode

kind values:
- app
- launcher

`kind = "app"` is the normal user-app kind.

`kind = "launcher"` declares that an app can act as a SquidScript launcher. Launcher apps are installed like other apps, but the active default launcher is selected through a user-mediated firmware/system flow.

Normal apps must not receive launcher capabilities merely by declaring `kind = "launcher"`. Firmware grants `launcher.*` capabilities only while running an app as the active launcher or inside an explicit user-mediated launcher-selection flow.

Any installed app may declare `kind = "launcher"` if it provides the required launcher behavior and permissions. This allows alternative launchers to be installed and selected easily, similar to changing launchers on Android.

Firmware must validate a candidate launcher before making it the active default. If the selected launcher is missing, invalid, incompatible, or repeatedly crashes before first render, firmware should fall back to a known-good launcher or a recovery launcher.

The optional `requires` object declares target capabilities that the app needs before launch. Firmware must reject an app if the current target cannot satisfy these requirements.

Supported `requires` fields:

- `runtime`: SquidScript runtime version constraint such as `squidscript>=0.2`
- `display.minWidth` and `display.minHeight`: minimum logical display size
- `display.pixelFormats`: acceptable display pixel formats such as `GRAY1_PACKED` and `GRAY2_PACKED`
- `keys`: required logical input keys
- `features`: required firmware/runtime capabilities such as `display.draw`, `state.read`, `state.write`, `content.read`, and `binbook.read`

The optional `targets` object restricts an app to exact hardware targets when capability matching is insufficient.

Example:

```json
{
  "targets": {
    "allow": ["xteink-x4"]
  }
}
```

Device targeting should be rare. Capability targeting through `requires` is the default.

Production firmware is required to support:
- entry.type = "bytecode"

Firmware is not required to support source entry points.

SquidScript v0.2 apps must not declare:
- entry.type = "source"

Developer tools may compile .squid source off-device and copy the resulting .sqbc to the app directory.

Recommended production policy:
- app.json should declare entry.type = "bytecode"
- source files may be included for inspection/debugging
- source-map.json may be included for better diagnostics

---

## 6. App ID Rules

App IDs:
- must use lowercase letters, numbers, and hyphens
- must start with a lowercase letter
- must not contain path separators
- must be unique among installed apps

Valid:
- simple-counter
- binbook-reader
- presentation-clicker

Invalid:
- SimpleCounter
- ../bad
- app/test
- 123-counter

---

## 7. Source Files

SquidScript source files use the .squid extension.

Source files are authoring files.

Production firmware does not need to parse source files.

The off-device compiler squidc parses source files and emits .sqbc bytecode.

A source file contains top-level declarations.

Allowed top-level declarations:
- include "path"
- state { ... }
- onStart() { ... }
- onResume() { ... }
- onSuspend() { ... }
- onKey("KEY") { ... }
- onTimer("name") { ... }
- screen("name") { ... }
- function name(...) { ... }

Top-level executable statements are not allowed.

Invalid:

```squid
count = count + 1
screen.refresh()
```

Valid:

```squid
onKey("RIGHT") {
  count = count + 1
  screen.refresh()
}
```

---

## 8. Includes

SquidScript supports compile-time includes.

Includes are resolved by squidc.

Production firmware does not resolve source includes.

Syntax:

```squid
include "lib/ui.squid"
include "screens/reader.squid"
```

Include rules:
- include is allowed only at top level
- include path must be a string literal
- include path is relative to the app directory
- include path must not contain ..
- include path must not be absolute
- included files must remain inside the app directory
- include cycles are rejected
- maximum include depth is enforced
- maximum number of included files is enforced
- maximum combined source size is enforced

Valid:

```squid
include "lib/common.squid"
include "screens/reader.squid"
```

Invalid:

```squid
include "../other-app/main.squid"
include "/sd/system/secret.squid"
include content.pickFile(".squid")
```

Recommended include limits:
- max include files: 16
- max include depth: 4
- max combined source size: 32 KB to 64 KB
- max include path length: 96 bytes

Includes behave as source-level compilation units.

They are not runtime imports.

SquidScript v0.2 does not support JavaScript-style import/export.

---

## 9. Comments

Line comments:

```squid
// This is a comment
```

Block comments:

```squid
/*
  This is a block comment.
*/
```

Comments are ignored by squidc.

---

## 10. Lexical Rules

Identifiers:

```squid
count
pageIndex
loadBook
current_slide
```

Recommended style is camelCase.

snake_case may be accepted.

Integer literals:

```squid
0
1
123
-5
```

String literals:

```squid
"hello"
"Line one\nLine two"
```

Boolean literals:

```squid
true
false
```

Null literal:

```squid
null
```

Symbols and operators:

```text
{ } ( ) , : ;
+ - * /
== != < <= > >=
&& || !
=
```

Semicolons:

Semicolons are optional if statements are separated by newlines.

These are equivalent:

```squid
count = count + 1
screen.refresh()
```

```squid
count = count + 1;
screen.refresh();
```

SquidScript does not implement full JavaScript automatic semicolon insertion.

A statement must end with one of:
- newline
- semicolon
- closing brace

---

## 11. Value Types

SquidScript v0.2 supports these value types:

- int
- bool
- string
- null
- list
- record
- handle

int:

```squid
123
```

Runtime `int` values are signed 32-bit integers.

Allowed range:

```text
-2147483648 to 2147483647
```

Integer arithmetic is checked.

If an operation overflows, underflows, or divides by zero, squidvm must stop the current app with a runtime error.

Integer division uses signed integer division and truncates toward zero.

bool:

```squid
true
false
```

string:

```squid
"hello"
```

Strings are UTF-8.

Runtime string length limits are measured in bytes unless a built-in explicitly says otherwise.

null:

```squid
null
```

`null` is allowed only where a type explicitly permits an absent value.

Accessing a field or calling a capability API on `null` is invalid. squidc should reject this when the type is known; otherwise squidvm must stop the current app with a runtime error.

list:

A bounded runtime-managed sequence.

Lists are read-only in v0.2 and are usually returned by built-ins.

Example:

```squid
let lines = data.fields(section, "body")
```

record:

A fixed-shape read-only object returned by built-ins.

Example:

```squid
let info = binbook.info(book)
title = info.title
pageCount = info.pageCount
```

handle:

An opaque firmware-owned reference.

Examples:

```squid
let book = binbook.open(file)
let page = binbook.page(book, pageIndex)
let doc = data.read(file)
```

Handles are not pointers.

Scripts can pass handles back to the firmware APIs that created or accept them, but scripts cannot inspect, serialize, forge, compare, or persist the underlying resource.

Handle lifetime is bounded to the current event or render turn unless a built-in explicitly says otherwise.

The runtime must release any remaining handles at the end of the current event or render turn.

Built-ins such as binbook.close(book) may release a handle earlier.

Using a released handle is a runtime error.

Handles are not serializable.

Handles may not be stored in persistent state.

Handles cannot be compared for equality except where a specific API explicitly permits comparison with `null`.

---

## 12. State Block

The state block declares persistent app variables.

Example:

```squid
state {
  count: 0,
  title: "",
  done: false
}
```

State variables are persisted by state.save() and restored by state.load().

Example:

```squid
onStart() {
  state.load()
  screen.open("main")
}
```

State is stored by firmware, not by direct script file writes.

The runtime stores persistent state under:

```text
/sd/system/app-state/{app_id}.json
```

Allowed persistent state types in v0.2:
- int
- bool
- string
- null

Optional future persistent state types:
- bounded lists
- fixed-shape records

Disallowed persistent state values:
- handles
- functions
- arbitrary objects
- raw binary blobs

Invalid:

```squid
state {
  book: binbook.open("x.binbook")
}
```

Valid:

```squid
state {
  file: "",
  pageIndex: 0
}
```

---

## 13. Local Variables

Local variables are declared with let.

Example:

```squid
function loadSlide() {
  let doc = data.read(file)
  let s = data.section(doc, "slide", current)
  title = data.field(s, "title")
}
```

Local variables are function-scoped inside functions and render-turn scoped inside screen blocks in v0.2.

Local variables are not persisted.

Local variables must not shadow state variables, function parameters, other locals in the same function, built-in namespaces, or function names.

Local variables may hold:
- int
- bool
- string
- null
- list
- record
- handle

---

## 14. Assignment

Assignment to state variables:

```squid
count = count + 1
```

Assignment to local variables:

```squid
let x = 10
x = x + 1
```

Assignment to record fields is not supported.

Invalid:

```squid
info.title = "New Title"
```

Dynamic property assignment is not supported.

Invalid:

```squid
obj[key] = value
```

Only existing state variables and local variables can be assigned.

Assignment cannot create a new variable. New local variables must use `let`.

---

## 15. Objects and Records

SquidScript v0.2 supports read-only fixed-shape records returned by built-ins.

Example:

```squid
let info = binbook.info(book)

title = info.title
pageCount = info.pageCount
```

Records are not JavaScript objects.

Unsupported:
- classes
- prototypes
- this
- new
- object methods
- dynamic property creation
- computed property access
- mutation of record fields

Allowed:

```squid
info.title
info.pageCount
page.width
page.height
```

Not allowed:

```squid
info["title"]
info.title = "Changed"
let obj = {}
obj.name = "test"
```

Option objects are allowed only as direct built-in call arguments.

Example:

```squid
display.text("Hello", { x: 20, y: 40, size: "large" })
```

Option objects are parsed by squidc and encoded into bytecode metadata.

Option objects are not general mutable objects.

---

## 16. Expressions

Supported expressions:

```squid
123
"hello"
true
false
null

count
count + 1
count - 1
count * 2
count / 2

count == 0
count != 0
count < 10
count <= 10
count > 0
count >= 0

a && b
a || b
!done

functionCall(...)
namespace.functionCall(...)
record.field
```

Operator precedence:

1. !
2. * /
3. + -
4. < <= > >=
5. == !=
6. &&
7. ||

Parentheses may be used:

```squid
if ((count + 1) < maxCount) {
  count = count + 1
}
```

String concatenation with + is supported only when both operands are strings.

SquidScript does not perform automatic string conversion for +.

Example:

```squid
title = "Page " + suffix
```

Recommended formatting uses string.format():

```squid
display.text(string.format("{}/{}", pageIndex + 1, pageCount), { x: 360, y: 760 })
```

To combine strings and non-string values, use string.format().

Function arguments are evaluated left-to-right.

`&&` and `||` short-circuit left-to-right.

Equality is defined only for values of the same primitive type: int, bool, string, and null. Records, lists, and handles are not comparable in v0.2 unless a specific built-in returns a primitive identifier for comparison.

---

## 17. Statements

Supported statements:

```text
let name = expression
name = expression
functionCall(...)
if (condition) { ... }
if (condition) { ... } else { ... }
repeat (N) { ... }
for item in list max N { ... }
return
return expression
```

Unsupported statements:

```text
while (...) { ... }
for (;;) { ... }
do { ... } while (...)
switch (...)
try { ... } catch (...) { ... }
throw ...
break
continue
import ...
export ...
class ...
async function ...
await ...
```

---

## 17.1 Built-in Namespaces

Capability APIs are namespaced.

These namespaces form the standard platform capability set for v0.2. They are not user-imported libraries, and they are not core language syntax. `squidc` validates calls against known capability signatures and emits builtin IDs into `.sqbc`; `squidvm` validates and dispatches those IDs to firmware/runtime modules.

v0.2 uses these built-in namespaces:

- `app.*` for app-level actions such as exit and firmware dialogs
- `screen.*` for current-screen navigation and refresh
- `display.*` for drawing commands, logical display coordinates, and display-ready resources
- `state.*` for firmware-managed persistent state
- `content.*` for user-selected content files and bounded reads
- `data.*` for parsed declarative app data
- `string.*` for deterministic string utilities
- `binbook.*` for BinBook document handles and drawable page resources
- `launcher.*` for launcher apps to list, inspect, and launch installed apps

Global built-ins should not be added when a capability namespace is available.

New device or document behavior should normally be added as a namespaced capability rather than as new syntax. New core syntax should be reserved for behavior that cannot be expressed clearly, safely, or efficiently through capability calls and existing value types.

---

## 18. If Statements

Example:

```squid
if (count > 0) {
  count = count - 1
  state.save()
  screen.refresh()
}
```

With else:

```squid
if (pageIndex < pageCount - 1) {
  pageIndex = pageIndex + 1
} else {
  app.message("End", "This is the last page.")
}
```

Conditions must evaluate to bool.

squidc must reject implicit truthiness when it can prove the condition is not bool. squidvm must reject a non-bool condition at runtime if static validation cannot prove the type.

Recommended:

```squid
if (file != "") {
  openBook()
}
```

Avoid:

```squid
if (file) {
  openBook()
}
```

---

## 19. Bounded Loops

SquidScript supports only bounded loops.

repeat:

```squid
repeat (10) {
  count = count + 1
}
```

The repeat count must be:
- an integer literal, or
- an expression whose evaluated value is within runtime limits

for-in with max:

```squid
for line in lines max 50 {
  display.text(line, { x: 20, y: y })
  y = y + 24
}
```

The max clause is required.

Invalid:

```squid
for line in lines {
  display.text(line, { x: 20, y: y })
}
```

Unsupported:

```squid
while (true) {
  count = count + 1
}

for (;;) {
  count = count + 1
}
```

The runtime also enforces a global instruction limit per event.

---

## 20. Functions

Functions are declared with function.

Example:

```squid
function loadSlide() {
  let doc = data.read(file)
  let s = data.section(doc, "slide", pageIndex)

  title = data.field(s, "title")
  body = string.join(data.fields(s, "body"), "\n")
}
```

Functions may return values:

```squid
function nextPageIndex() {
  return pageIndex + 1
}
```

Function calls:

```squid
loadSlide()

let n = nextPageIndex()
```

Restrictions:
- no recursion
- no closures
- no anonymous functions
- no function values
- no callbacks
- no variable argument lists
- limited call depth

squidc must reject statically visible recursion.

squidvm must reject excessive call depth at runtime.

---

## 21. Lifecycle Handlers

Supported lifecycle handlers:

```squid
onStart()
onResume()
onSuspend()
```

onStart runs when the app is launched.

Example:

```squid
onStart() {
  state.load()
  screen.open("main")
}
```

onResume runs when returning to an already active app.

Example:

```squid
onResume() {
  screen.refresh()
}
```

onSuspend runs before the app is suspended or exited.

Example:

```squid
onSuspend() {
  state.save()
}
```

Lifecycle handlers are optional.

---

## 22. Key Handlers

Supported key handlers:

```squid
onKey("UP") {}
onKey("DOWN") {}
onKey("LEFT") {}
onKey("RIGHT") {}
onKey("SELECT") {}
onKey("BACK") {}
onKey("MENU") {}
onKey("HOME") {}
```

Example:

```squid
onKey("RIGHT") {
  count = count + 1
  state.save()
  screen.refresh()
}
```

Key names are logical input names, not raw GPIOs.

The firmware maps hardware buttons to logical keys.

---

## 23. Screens

Screens define renderable views.

The `screen.*` namespace controls app-level view selection and refresh.

The `display.*` namespace draws into the current render pass using the target's logical display coordinate system.

In other words, `screen.open(...)` and `screen.refresh()` decide which view is active and when it is re-rendered; `display.clear(...)`, `display.text(...)`, and `display.draw(...)` describe what appears during that render.

Example:

```squid
screen("main") {
  display.clear("white")
  display.text("Count", { x: 20, y: 40, size: "large" })
  display.text(count, { x: 20, y: 120, size: "huge" })
}
```

Only one screen is current at a time.

The current screen is selected by screen.open("screenName").

Example:

```squid
onStart() {
  screen.open("main")
}
```

In v0.2, screen blocks should be render-pure.

Allowed in screen blocks:
- display.clear(...)
- display.text(...)
- display.line(...)
- display.rect(...)
- display.image(...)
- display.draw(...)
- local let bindings for display-only calculations
- string.format(...)
- safe read-only value access
- render-safe handle creation for drawing APIs

Disallowed in screen blocks:
- persistent state mutation
- state.save()
- state.load()
- file writes
- app exit
- screen.open(...)
- long loops
- non-render-safe handle creation

Invalid:

```squid
screen("main") {
  count = count + 1
}
```

Rationale:

Screen blocks may be re-rendered at any time.

Rendering should not change persistent app state.

Handles created during screen rendering are transient and must be released automatically by the runtime after the render turn.

If a screen block calls a user-defined function, that function must also be render-pure.

squidc should reject calls from screen blocks to functions that perform state writes, app navigation, file writes, app exit, or other non-render-safe operations.

---

## 24. Display Coordinate System

SquidScript apps draw in the target's logical display coordinate system.

The default XTEINK X4 target profile uses a portrait logical display:

```text
width: 480
height: 800
origin: top-left
```

Coordinates:

x increases to the right.
y increases downward.

The physical XTEINK X4 panel is 800x480 rotated 90 degrees clockwise.

Apps should use logical coordinates from the selected target profile, not physical panel coordinates.

The display driver owns rotation and physical mapping.

---

## 25. Drawing Built-ins

display.clear(color)

Example:

```squid
display.clear("white")
```

Supported colors in v0.2:
- "white"
- "black"
- "gray1"
- "gray2"

The exact gray support depends on display capabilities.

display.text(value, options)

Example:

```squid
display.text("Hello", { x: 20, y: 40, size: "large" })
```

Example with wrapping:

```squid
display.text(body, {
  x: 20,
  y: 120,
  w: 440,
  h: 560,
  wrap: true
})
```

Required options:
- x
- y

Optional options:
- w
- h
- size
- align
- wrap
- color

display.line(x1, y1, x2, y2)

Example:

```squid
display.line(20, 96, 460, 96)
```

display.rect(x, y, w, h, options)

Example:

```squid
display.rect(20, 100, 440, 80, { stroke: "black" })
```

Example filled:

```squid
display.rect(0, 0, 480, 40, { fill: "black" })
```

display.image(path, options)

Example:

```squid
display.image("data/icon.bmp", { x: 20, y: 20 })
```

display.draw(drawable, options)

Draws a display-ready resource, such as an image resource, canvas surface, or BinBook page image.

Example:

```squid
display.draw(drawable, { x: 0, y: 0 })
```

The runtime may clip drawing outside the logical screen.

The runtime may reject excessive draw commands.

---

## 26. System Built-ins

screen.open(screenName)

Selects the current screen.

Example:

```squid
screen.open("main")
```

screen.refresh()

Requests a re-render of the current screen.

The runtime reruns the current `screen("...")` block. Drawing still happens through `display.*` calls inside that screen block.

Example:

```squid
screen.refresh()
```

app.exit()

Exits the app and returns to launcher.

Example:

```squid
app.exit()
```

app.message(title, body)

Shows a firmware-provided message dialog.

Example:

```squid
app.message("Error", "Could not open file.")
```

app.confirm(title, body)

Optional v0.2 or later.

Returns bool if supported.

Example:

```squid
let ok = app.confirm("Reset", "Reset app state?")
```

string.format(template, ...)

Returns a string by replacing `{}` placeholders in `template` with the remaining arguments in order.

Example:

```squid
let label = string.format("{}/{}", pageIndex + 1, pageCount)
```

Rules:
- template must be a string
- each `{}` consumes one argument
- argument count must match placeholder count
- supported argument types are int, bool, string, and null
- formatting is deterministic and locale-independent
- to render a literal brace, use `{{` or `}}`

---

## 27. State Built-ins

state.load()

Loads firmware-managed persistent state.

Requires permission:

```text
state.read
```

Example:

```squid
onStart() {
  state.load()
}
```

state.save()

Saves firmware-managed persistent state atomically.

Requires permission:

```text
state.write
```

Example:

```squid
count = count + 1
state.save()
```

state.reset()

Resets persistent state to defaults from the state block.

Requires permission:

```text
state.write
```

Example:

```squid
state.reset()
```

State writes must be atomic at firmware level.

Recommended write strategy:

1. write state.tmp
2. flush
3. rename current state to state.bak
4. rename state.tmp to state.json
5. optionally delete state.bak later

---

## 28. File and Data Built-ins

content.pickFile(extension)

Opens a firmware-controlled file picker.

Requires permission:

```text
content.pick
```

Example:

```squid
file = content.pickFile(".binbook")
```

content.readText(path)

Reads a bounded text file.

Requires permission:

```text
content.read or appdata.read, depending on path.
```

Example:

```squid
let text = content.readText(file)
```

content.readLines(path, maxLines)

Reads bounded lines from a text file.

Example:

```squid
let lines = content.readLines("data/notes.txt", 100)
```

data.read(path)

Reads and parses a generic structured data file.

Example:

```squid
let doc = data.read(file)
```

data.countSections(doc, name)

Example:

```squid
let total = data.countSections(doc, "slide")
```

data.section(doc, name, index)

Example:

```squid
let s = data.section(doc, "slide", current)
```

data.field(section, name)

Example:

```squid
title = data.field(s, "title")
```

data.fields(section, name)

Example:

```squid
body = string.join(data.fields(s, "body"), "\n")
```

string.join(list, separator)

Example:

```squid
body = string.join(lines, "\n")
```

Path restrictions:
- scripts may read own app data if appdata.read is granted
- scripts may write own app data if appdata.write is granted
- scripts may read user-selected content if content.read is granted
- scripts may not directly write arbitrary external content
- scripts may not read /sd/system directly
- scripts may not use ../ path traversal

---

## 29. Generic Data Format

SquidScript may support a generic structured text format for app-specific data.

Example presentation file:

```text
presentation {
  title: "Demo"
}

slide {
  title: "ESP32-C3 Miniapps"
  body: """
  Firmware provides the runtime.
  Apps live on the SD card.
  Scripts handle buttons and drawing.
  """
}

slide {
  title: "Safe Extensibility"
  body: """
  No native binaries.
  No unbounded loops.
  No raw filesystem writes.
  """
}
```

The runtime may expose this through:

```squid
data.read(path)
data.countSections(doc, "slide")
data.section(doc, "slide", index)
data.field(section, "title")
data.fields(section, "body")
```

The data format is declarative.

It must not contain executable code.

---

## 30. BinBook Capability

BinBook support is provided as a firmware-native capability module.

BinBook is part of the standard platform capability set, not the core language syntax.

The draft capability contract is:

```text
capabilities/binbook.cap.json
```

The contract is a source/spec/build artifact used by compiler and firmware implementations. It must not be embedded as JSON in `.sqbc`.

SquidScript does not parse BinBook bytes directly.

SquidScript uses opaque handles and read-only records.

The authoritative BinBook file-format reference is the GitHub-hosted BinBook specification:

```text
https://github.com/parasquid/binbook/blob/main/BINBOOK_FORMAT_SPEC.md
```

`.sqbc` is executable SquidScript bytecode. `.binbook` is a separate compiled raster-book document container.

Required permission:

```text
binbook.read
```

Typical usage:

```squid
let book = binbook.open(file)
let info = binbook.info(book)
let page = binbook.page(book, pageIndex)
let image = binbook.pageImage(page)
display.draw(image, { x: 0, y: 0 })
```

The BinBook capability owns document-specific work. The display capability owns final composition. Prefer this style of composition over BinBook-specific rendering syntax or all-in-one helpers that bypass `display.*`.

Built-ins:

```text
binbook.open(path)
binbook.info(book)
binbook.pageCount(book)
binbook.pageInfo(book, pageIndex)
binbook.page(book, pageIndex)
binbook.pageImage(page)
binbook.navCount(book)
binbook.navEntry(book, navIndex)
binbook.close(book)
display.draw(drawable, options)
```

Minimum API:

```text
binbook.open(path)
binbook.info(book)
binbook.page(book, pageIndex)
binbook.pageImage(page)
display.draw(drawable, options)
```

binbook.open(path)

Opens and validates a BinBook file.

Example:

```squid
let book = binbook.open(file)
```

Returns:

```text
handle
```

binbook.info(book)

Returns a read-only record.

Example record:

```text
{
  title: "My Book",
  author: "Unknown",
  pageCount: 42,
  logicalWidth: 480,
  logicalHeight: 800,
  bpp: 2
}
```

Example:

```squid
let info = binbook.info(book)
title = info.title
pageCount = info.pageCount
```

binbook.page(book, pageIndex)

Returns an opaque page handle.

Example:

```squid
let page = binbook.page(book, pageIndex)
```

binbook.pageImage(page)

Returns a display-ready drawable resource for a BinBook page.

Example:

```squid
let image = binbook.pageImage(page)
display.draw(image, { x: 0, y: 0 })
```

The firmware module owns:
- header validation
- index validation
- page decoding
- bit-depth conversion
- bounds checks
- memory management
- display tiling if needed
- rotation handling if needed
- error reporting

Scripts should persist:
- file path
- page index

Scripts should not persist:
- book handles
- page handles
- decoded pixel buffers

Valid state:

```squid
state {
  file: "",
  pageIndex: 0
}
```

Invalid state:

```squid
state {
  book: null
}
```

---

## 30.1 Launcher Capability

Launcher support is provided as a firmware-native capability module exposed to SquidScript launcher apps.

Launcher apps are SquidScript apps with:

```json
{
  "kind": "launcher"
}
```

The active launcher is selected by the user through firmware/system UI. Any installed app may declare `kind = "launcher"`, but firmware grants `launcher.*` capabilities only while running the selected app in the launcher role.

Normal apps must not silently replace the default launcher.

Suggested launcher permissions:

```text
launcher.apps.list
launcher.apps.inspect
launcher.apps.launch
system.launcher.chooseDefault
```

Minimum launcher capability shape:

```text
launcher.apps()
launcher.app(apps, index)
launcher.launch(appId)
system.launcher.chooseDefault()
```

`launcher.apps()`

Returns a bounded list handle or list-like firmware-owned value containing installed launchable apps.

Requires permission:

```text
launcher.apps.list
```

`launcher.app(apps, index)`

Returns a read-only record with safe manifest summary fields.

Example record:

```text
{
  id: "binbook-reader",
  name: "BinBook Reader",
  kind: "app",
  version: "1.0.0",
  description: "Read BinBook files"
}
```

Requires permission:

```text
launcher.apps.inspect
```

`launcher.launch(appId)`

Requests that firmware launch an installed app by app ID.

Requires permission:

```text
launcher.apps.launch
```

The firmware owns:
- manifest lookup
- target compatibility checks
- permission validation
- bytecode validation
- suspending launcher VM state
- starting the target app
- returning control to the launcher when the app exits or crashes

The launcher does not directly execute `.sqbc`.

`system.launcher.chooseDefault()`

Opens firmware/system UI for choosing the default launcher from installed apps that declare `kind = "launcher"`.

Requires permission:

```text
system.launcher.chooseDefault
```

This API is user-mediated. It must not silently change the default launcher.

Firmware should validate a candidate launcher before saving it as the default. If the selected launcher is missing, invalid, incompatible, or crash-looping before first render, firmware should fall back to a known-good launcher or recovery launcher.

Example launcher manifest:

```json
{
  "format": "squidapp-v1",
  "id": "simple-launcher",
  "name": "Simple Launcher",
  "kind": "launcher",
  "version": "1.0.0",
  "runtime": {
    "language": "squidscript",
    "version": "0.2"
  },
  "entry": {
    "type": "bytecode",
    "file": "main.sqbc"
  },
  "permissions": [
    "display.draw",
    "state.read",
    "state.write",
    "launcher.apps.list",
    "launcher.apps.inspect",
    "launcher.apps.launch",
    "system.launcher.chooseDefault"
  ]
}
```

Example launcher flow:

```squid
state {
  selected: 0
}

onStart() {
  state.load()
  screen.open("apps")
}

onKey("SELECT") {
  let apps = launcher.apps()
  let app = launcher.app(apps, selected)
  launcher.launch(app.id)
}
```

---

## 31. Permissions

Permissions are declared in app.json.

Suggested permissions:

display.draw
- Allows display drawing operations such as display.clear, display.text, display.line, display.rect, display.image, and display.draw.

state.read
- Allows state.load.

state.write
- Allows state.save and state.reset.

appdata.read
- Allows reading files under the app's own data directory.

appdata.write
- Allows writing files under the app's own data directory.

content.pick
- Allows content.pickFile.

content.read
- Allows reading a user-selected external content file.

binbook.read
- Allows BinBook document APIs.

system.info
- Allows safe device info queries.

launcher.apps.list
- Allows the active launcher to request the installed app list.

launcher.apps.inspect
- Allows the active launcher to read safe manifest summaries for installed apps.

launcher.apps.launch
- Allows the active launcher to request that firmware launch another app.

system.launcher.chooseDefault
- Allows opening the firmware/user-mediated default-launcher chooser. This does not allow silent launcher replacement.

Permission checks happen during source compilation, bytecode validation, and runtime execution.

If bytecode calls a built-in without declared permission, firmware must reject the app or stop execution with an error.

---

## 32. Bytecode Execution Model

The canonical executable format is .sqbc.

Production firmware executes bytecode.

Production firmware is not required to parse .squid source.

squidc compiles:

```text
.squid source + includes
  -> .sqbc bytecode
  -> optional source-map.json
```

Runtime execution:

1. Load app.json.
2. Load .sqbc.
3. Validate .sqbc header and sections.
4. Validate bytecode instruction stream.
5. Validate required permissions against app.json.
6. Initialize runtime state.
7. Execute event handlers.
8. Render screens.
9. Record errors and crash diagnostics.

---

## 33. SQBC Bytecode File

.sqbc is the SquidScript bytecode format.

All multi-byte integer fields in .sqbc are little-endian.

Fixed-size binary records must use explicit integer widths such as u8, u16, u32, i32, and u64.

The bytecode format must not depend on host C struct padding or alignment.

If padding bytes are needed for alignment, they must be explicit and zero-filled.

Suggested sections:

```text
SQBC header
|-- magic/version
|-- target runtime version
|-- compiler version
|-- app ID hash
|-- required permissions
|-- target requirements
|-- source hash
|-- string pool
|-- state table
|-- function table
|-- event handler table
|-- screen table
|-- bytecode instructions
|-- draw command templates
|-- builtin call table
`-- checksum/signature
```

Minimal header sketch:

```c
struct SqbcHeader {
  char magic[4];              // "SQBC"
  uint16_t bytecode_version;  // e.g. 1
  uint16_t runtime_min;       // e.g. 2
  uint16_t runtime_max;       // optional
  uint32_t flags;
  uint32_t file_size;
  uint32_t app_id_hash;
  uint32_t target_requirements_offset;
  uint32_t string_pool_offset;
  uint32_t state_table_offset;
  uint32_t function_table_offset;
  uint32_t handler_table_offset;
  uint32_t screen_table_offset;
  uint32_t code_offset;
  uint32_t code_size;
  uint32_t bytecode_hash;
  uint32_t source_map_hash;
  uint32_t checksum;
};
```

The exact structure may change during implementation.

The loader must reject malformed headers.

Target requirements must be encoded as binary `.sqbc` sections, not JSON, YAML, CBOR, or protobuf.

Suggested target requirements section:

```c
struct SqbcTargetRequirementsSection {
  uint16_t runtime_min_major;
  uint16_t runtime_min_minor;
  uint16_t runtime_max_major;
  uint16_t runtime_max_minor;
  uint16_t min_display_width;
  uint16_t min_display_height;
  uint16_t required_key_count;
  uint16_t required_feature_count;
  uint16_t required_pixel_format_count;
  uint16_t compiled_target_string_id;       // 0xffff if not target-locked
  uint16_t compatibility_profile_string_id; // 0xffff if none
  uint16_t runtime_profile_string_id;       // 0xffff if not fixed
};
```

The fixed header is followed by arrays of string-pool IDs for required logical keys, required feature names, and required pixel format names.

---

## 34. Bytecode Validation

Precompiled bytecode is untrusted input.

Firmware must validate .sqbc before execution.

Validation checks:
- magic is correct
- bytecode version is supported
- runtime version is compatible
- file size matches header
- offsets are within file bounds
- sections do not overlap illegally
- string pool is valid
- state table is valid
- function table is valid
- handler table is valid
- screen table is valid
- instruction stream is valid
- jumps target valid instruction boundaries
- call targets are valid
- builtin IDs are known
- required permissions match app.json
- target requirements are structurally valid
- required features are declared in app.json and provided by the current target
- required logical keys are provided by the current input profile
- required pixel formats are provided by the current display profile
- stack depth is bounded
- call depth is bounded
- table sizes are within limits
- checksum/hash matches

If validation fails, the app is marked invalid and must not run.

---

## 35. Internal Bytecode Sketch

Possible opcodes:

```text
OP_NOP

OP_PUSH_INT
OP_PUSH_BOOL
OP_PUSH_STRING
OP_PUSH_NULL

OP_GET_GLOBAL
OP_SET_GLOBAL
OP_GET_LOCAL
OP_SET_LOCAL

OP_GET_FIELD

OP_ADD
OP_SUB
OP_MUL
OP_DIV

OP_EQ
OP_NEQ
OP_LT
OP_LTE
OP_GT
OP_GTE

OP_AND
OP_OR
OP_NOT

OP_JUMP
OP_JUMP_IF_FALSE

OP_CALL_BUILTIN
OP_CALL_FUNCTION
OP_RETURN

OP_LOOP_GUARD
```

Function table entry:

```c
struct SquidFunction {
  const char *name;
  uint16_t start_ip;
  uint16_t end_ip;
  uint8_t arg_count;
  uint8_t local_count;
};
```

State slot:

```c
struct SquidStateSlot {
  const char *name;
  SquidValue default_value;
  SquidValue current_value;
};
```

Value type:

```c
enum SquidValueType {
  SQUID_NULL,
  SQUID_INT,
  SQUID_BOOL,
  SQUID_STRING,
  SQUID_LIST,
  SQUID_RECORD,
  SQUID_HANDLE
};
```

---

## 36. Screen Compilation

Screen blocks may be compiled into draw-command templates.

Example source:

```squid
screen("main") {
  display.clear("white")
  display.text(title, { x: 20, y: 40, size: "large" })
  display.line(20, 96, 460, 96)
}
```

Possible draw IR:

```text
DRAW_CLEAR "white"
DRAW_TEXT title x=20 y=40 size=large
DRAW_LINE 20 96 460 96
```

The draw IR is:
- bounded
- validated
- clipped by renderer
- executed only during render
- not allowed to mutate persistent state

---

## 37. Source Maps

Source maps are optional debug metadata.

The device does not need source maps to run an app.

If source-map.json is present and valid, firmware may use it for:
- friendlier runtime errors
- crash logs
- launcher broken-app diagnostics
- source-level function/handler names
- file and line references

If source-map.json is missing, corrupt, or mismatched, firmware must ignore it and continue using bytecode-level diagnostics.

source-map.json is non-authoritative.

source-map.json must not affect:
- bytecode validation
- permissions
- execution behavior
- app security

Example source-map.json:

```json
{
  "format": "squid-source-map-v1",
  "appId": "binbook-reader",
  "bytecodeFile": "main.sqbc",
  "bytecodeHash": "b2a4e1f9c0d3",
  "sources": [
    "main.squid",
    "lib/ui.squid",
    "screens/reader.squid"
  ],
  "symbols": [
    {
      "kind": "handler",
      "name": "onStart",
      "source": 0,
      "lineStart": 12,
      "lineEnd": 24,
      "ipStart": 0,
      "ipEnd": 42
    },
    {
      "kind": "handler",
      "name": "onKey.RIGHT",
      "source": 2,
      "lineStart": 10,
      "lineEnd": 18,
      "ipStart": 80,
      "ipEnd": 124
    },
    {
      "kind": "function",
      "name": "loadPage",
      "source": 2,
      "lineStart": 30,
      "lineEnd": 42,
      "ipStart": 160,
      "ipEnd": 220
    },
    {
      "kind": "screen",
      "name": "reader",
      "source": 2,
      "lineStart": 44,
      "lineEnd": 58,
      "ipStart": 240,
      "ipEnd": 310
    }
  ],
  "lines": [
    {
      "ipStart": 176,
      "ipEnd": 184,
      "source": 2,
      "line": 38
    }
  ]
}
```

The source map must bind to the bytecode file using bytecodeHash.

If source-map.json.bytecodeHash does not match the loaded .sqbc, firmware must ignore the source map.

Source files do not need to be present for source maps to be useful.

If source files are present, developer firmware may show source snippets.

Production firmware may show only file names and line numbers.

---

## 38. Runtime Diagnostics

On runtime error, squidvm should record:
- app ID
- bytecode file
- error code
- handler/function ID
- instruction pointer
- call stack function IDs and instruction pointers
- active screen name, if any
- current key/event
- source file and line, if source map is valid

Error with source map:

```text
App: binbook-reader
Error: binbook.page index out of range
Event: KEY_RIGHT
Handler: onKey("RIGHT")
Function: loadPage()
Source: screens/reader.squid:38
```

Fallback without source map:

```text
App: binbook-reader
Error: binbook.page index out of range
Event: KEY_RIGHT
Function: fn#4
Instruction: 182
```

Crash log example:

```text
App: binbook-reader
Error: binbook.page index out of range
Event: KEY_RIGHT

Bytecode:
- function: onKey.RIGHT
- ip: 92

Call stack:
- onKey.RIGHT @ ip 92
- loadPage @ ip 178

Source:
- screens/reader.squid:38
```

---

## 39. Runtime Quotas

Suggested v0.2 limits:

- max manifest size: 4 KB
- max bytecode file size: 32 KB to 64 KB
- max optional source size: 32 KB to 64 KB
- max source-map size: 16 KB
- max state variables: 64
- max serialized state size: 8 KB
- max string length: 1024 bytes
- max function count: 64
- max function parameters: 8
- max local variables per function: 32
- max call depth: 8
- max instructions per event: 2000
- max loop iterations per event: 100
- max screen draw commands: 128
- max file read size: 64 KB
- max parsed data sections: 256
- max list items returned by a built-in: 256
- max handle count: 16 to 32

The exact values may be tuned per firmware target.

The runtime may reject apps or stop execution if limits are exceeded.

---

## 40. Memory Model

Runtime memory should be bounded.

Recommended runtime memory components:
- bytecode buffer or mapped bytecode section
- string pool
- state slots
- value stack
- call stack
- handle table
- small draw command buffer
- app arena
- firmware-owned display/tile buffers

The runtime should avoid:
- keeping source text in RAM
- keeping token streams in RAM
- keeping ASTs in RAM
- exposing large decoded page buffers to scripts
- dynamic JavaScript-style object allocation
- general-purpose garbage collection

Production firmware should run bytecode directly or load compact bytecode sections.

Large document data such as BinBook pages should be streamed or tiled by firmware-native modules.

---

## 41. Execution Model

The runtime is event-driven.

Launch flow:

1. active launcher requests app launch through firmware
2. firmware suspends launcher VM state
3. runtime loads target app.json
4. runtime loads target .sqbc
5. runtime validates .sqbc
6. runtime optionally loads and verifies source-map.json
7. runtime initializes state defaults
8. runtime runs onStart()
9. runtime renders current screen
10. runtime waits for input event
11. runtime runs matching event handler
12. runtime renders if requested
13. runtime saves state if requested
14. runtime exits or suspends app when needed
15. firmware resumes or restarts launcher

Only one app is active at a time.

No background execution in v0.2.

No multitasking in v0.2.

Launcher apps are SquidScript apps with `kind = "launcher"` and launcher capabilities. They are still executed by `squidvm`; they do not directly execute arbitrary bytecode. A launcher requests app launches through firmware, and firmware owns validation, lifecycle transitions, crash recovery, and returning control to the launcher.

Default launcher selection is user-mediated. Apps may open a firmware-provided launcher chooser if granted `system.launcher.chooseDefault`, but they must not silently set themselves or another app as the default launcher.

---

## 42. Error Handling

Bytecode validation error:
- app is marked invalid
- app is not run
- launcher may show error details

Runtime error:
- execution stops
- error is recorded
- user is returned to launcher

Repeated runtime errors:
- app may be disabled until user re-enables it
- app state may be reset by user
- app files are not deleted automatically

Example error report:

```text
App: binbook-reader
File: main.sqbc
Source: screens/reader.squid:38
Error: permission binbook.read required for binbook.open()
```

If no valid source map exists:

```text
App: binbook-reader
File: main.sqbc
Function: fn#3
Instruction: 121
Error: permission binbook.read required for binbook.open()
```

---

## 43. Crash Recovery

Before launching an app, firmware records:
- app ID
- launch file, if any
- status: starting

After first successful render:
- status: running

On clean exit or suspend:
- status: clean

On boot:
- if previous app status was starting or running, assume crash/reset
- do not auto-resume that app
- return to launcher
- show recovery notice
- optionally increment crash count

Crash marker example:

```json
{
  "lastApp": "binbook-reader",
  "lastLaunchFile": "/books/example.binbook",
  "status": "running",
  "crashCount": {
    "binbook-reader": 1
  }
}
```

---

## 44. Security Rules

SquidScript apps are untrusted.

Rules:
- no native code
- no arbitrary memory access
- no raw pointers
- no arbitrary filesystem writes
- no path traversal
- no direct hardware access
- no raw display driver access
- no unbounded execution
- no hidden autostart without user action
- no persistent handles
- no executable app-specific data files
- bytecode must be validated before execution
- source maps must not affect execution

App-specific files may define declarative data formats, but not executable languages.

Allowed declarative app data:

```text
presentation {
  title: "Demo"
}

slide {
  title: "Hello"
  body: "World"
}
```

Not allowed as app data:

```squid
onSlideOpen() {
  while (true) {
    drawPixel(randomX(), randomY())
  }
}
```

---

## 45. Unsupported JavaScript Features

SquidScript v0.2 does not support:
- var
- const
- class
- new
- this
- prototype
- constructor
- import
- export
- async
- await
- Promise
- yield
- generator functions
- arrow functions
- anonymous functions
- closures
- eval
- Function constructor
- try/catch/finally
- throw
- switch
- while
- do/while
- general for loops
- arrays as mutable JS objects
- objects as mutable JS objects
- computed property access
- destructuring
- spread/rest
- regex literals
- Date
- Math object, except selected built-ins
- JSON.parse
- browser APIs
- Node APIs

Unsupported source syntax is a squidc compile error.

Unsupported bytecode is a firmware validation error.

---

## 46. Compatibility

Runtime version is declared in app.json.

Runtime version mismatch behavior:
- if app requires newer runtime: reject app
- if app requires older runtime: run if compatible
- if feature flags are later added: reject if unsupported

Bytecode version mismatch behavior:
- if bytecode version is unsupported: reject app
- if bytecode version is supported: validate normally

Source maps:
- may be ignored without affecting execution
- must match bytecode hash to be used

Future versions should avoid breaking v0.2 apps where possible.

---

## 47. Compiler: squidc

squidc is the off-device SquidScript compiler.

squidc responsibilities:
- read app.json
- resolve includes
- tokenize .squid source
- parse source
- validate language rules
- validate permissions
- compile to .sqbc
- emit source-map.json if requested
- emit diagnostics
- reject unsupported syntax
- reject invalid app structure

Suggested command:

```sh
squidc build /path/to/app --out /path/to/app/main.sqbc --source-map
```

Compiler diagnostics should include:
- file path
- line number
- column number if available
- error message
- missing permission if relevant
- function/screen/handler context if relevant

Example:

```text
screens/reader.squid:38: binbook.page requires permission binbook.read
```

---

## 48. Example: Simple Counter App

app.json:

```json
{
  "format": "squidapp-v1",
  "id": "simple-counter",
  "name": "Simple Counter",
  "kind": "app",
  "version": "1.0.0",
  "runtime": {
    "language": "squidscript",
    "version": "0.2"
  },
  "entry": {
    "type": "bytecode",
    "file": "main.sqbc"
  },
  "source": {
    "file": "main.squid",
    "optional": true
  },
  "sourceMap": {
    "file": "source-map.json",
    "optional": true
  },
  "permissions": [
    "state.read",
    "state.write",
    "display.draw"
  ],
  "opens": []
}
```

main.squid:

```squid
state {
  count: 0
}

onStart() {
  state.load()
  screen.open("main")
}

onKey("RIGHT") {
  count = count + 1
  state.save()
  screen.refresh()
}

onKey("LEFT") {
  if (count > 0) {
    count = count - 1
    state.save()
    screen.refresh()
  }
}

onKey("BACK") {
  app.exit()
}

screen("main") {
  display.clear("white")
  display.text("Count", { x: 20, y: 40, size: "large" })
  display.text(count, { x: 20, y: 120, size: "huge" })
}
```

Build:

```sh
squidc build /sd/apps/simple-counter --out /sd/apps/simple-counter/main.sqbc --source-map
```

---

## 49. Example: BinBook Reader App

The draft reference implementation is documented in:

```text
docs/binbook_reader_reference.md
```

Draft source files are available under:

```text
examples/binbook-reader/
```

This example is intentionally limited to reading and navigation:
- first-screen BinBook browser
- resume last book from app state
- page forward/back
- coarse page movement
- table of contents navigation
- jump-to-page

It does not include dictionaries, annotations, highlighting, search, bookmarks, or background indexing.

app.json:

```json
{
  "format": "squidapp-v1",
  "id": "binbook-reader",
  "name": "BinBook Reader",
  "kind": "app",
  "version": "1.0.0",
  "runtime": {
    "language": "squidscript",
    "version": "0.2"
  },
  "entry": {
    "type": "bytecode",
    "file": "main.sqbc"
  },
  "source": {
    "file": "main.squid",
    "optional": true
  },
  "sourceMap": {
    "file": "source-map.json",
    "optional": true
  },
  "permissions": [
    "state.read",
    "state.write",
    "display.draw",
    "content.pick",
    "content.read",
    "binbook.read"
  ],
  "requires": {
    "runtime": "squidscript>=0.2",
    "display": {
      "minWidth": 480,
      "minHeight": 800,
      "pixelFormats": ["GRAY2_PACKED"]
    },
    "keys": ["UP", "DOWN", "LEFT", "RIGHT", "SELECT", "BACK", "MENU"],
    "features": [
      "display.draw",
      "state.read",
      "state.write",
      "content.pick",
      "content.read",
      "binbook.read"
    ]
  ],
  "opens": [
    {
      "extension": ".binbook",
      "label": "BinBook"
    }
  ]
}
```

main.squid:

```squid
include "lib/ui.squid"
include "screens/browser.squid"
include "screens/reader.squid"
include "screens/toc.squid"
include "screens/jump.squid"

state {
  file: "",
  title: "",
  pageCount: 0,
  pageIndex: 0,
  navCount: 0,
  tocIndex: 0,
  tocTop: 0,
  jumpPage: 1,
  browserIndex: 0,
  view: "browser"
}

onStart() {
  state.load()
  openBrowser()
}

onKey("RIGHT") {
  if (view == "reader") {
    nextPage()
  } else {
    if (view == "jump") {
      jumpForward10()
    }
  }
}

onKey("LEFT") {
  if (view == "reader") {
    previousPage()
  } else {
    if (view == "jump") {
      jumpBack10()
    }
  }
}

onKey("UP") {
  if (view == "reader") {
    previousChapter()
  } else {
    if (view == "toc") {
      tocPrevious()
    } else {
      if (view == "jump") {
        jumpForward1()
      } else {
        if (view == "browser") {
          browserPrevious()
        }
      }
    }
  }
}

onKey("DOWN") {
  if (view == "reader") {
    nextChapter()
  } else {
    if (view == "toc") {
      tocNext()
    } else {
      if (view == "jump") {
        jumpBack1()
      } else {
        if (view == "browser") {
          browserNext()
        }
      }
    }
  }
}

onKey("SELECT") {
  if (view == "reader") {
    openJump()
  } else {
    if (view == "toc") {
      openSelectedTocEntry()
    } else {
      if (view == "jump") {
        commitJump()
      } else {
        if (view == "browser") {
          openSelectedBrowserItem()
        }
      }
    }
  }
}

onKey("MENU") {
  if (view == "reader") {
    openToc()
  } else {
    openReader()
  }
}

onKey("BACK") {
  if (view == "reader") {
    openBrowser()
  } else {
    if (view == "browser") {
      state.save()
      app.exit()
    } else {
      openReader()
    }
  }
}
```

lib/ui.squid:

```squid
function openBook() {
  let book = binbook.open(file)
  let info = binbook.info(book)

  title = info.title
  pageCount = info.pageCount
  navCount = info.navCount

  if (pageCount <= 0) {
    pageIndex = 0
  } else {
    if (pageIndex >= pageCount) {
      pageIndex = pageCount - 1
    }
  }
}

function openBrowser() {
  view = "browser"
  screen.open("browser")
}

function browseForBook() {
  let picked = content.pickFile(".binbook")

  if (picked != "") {
    file = picked
    pageIndex = 0
    tocIndex = 0
    tocTop = 0
    jumpPage = 1
    openBook()
    state.save()
    openReader()
  }
}

function resumeBook() {
  if (file != "") {
    openBook()
    state.save()
    openReader()
  }
}

function browserPrevious() {
  if (browserIndex > 0) {
    browserIndex = browserIndex - 1
    state.save()
    screen.refresh()
  }
}

function browserNext() {
  if (file != "") {
    if (browserIndex < 1) {
      browserIndex = browserIndex + 1
      state.save()
      screen.refresh()
    }
  }
}

function openSelectedBrowserItem() {
  if (browserIndex == 0) {
    browseForBook()
  } else {
    resumeBook()
  }
}

function nextPage() {
  if (pageIndex < pageCount - 1) {
    pageIndex = pageIndex + 1
    state.save()
    screen.refresh()
  }
}

function previousPage() {
  if (pageIndex > 0) {
    pageIndex = pageIndex - 1
    state.save()
    screen.refresh()
  }
}

function openToc() {
  view = "toc"
  screen.open("toc")
}

function openJump() {
  jumpPage = pageIndex + 1
  view = "jump"
  screen.open("jump")
}

function openReader() {
  view = "reader"
  screen.open("reader")
}

function openSelectedTocEntry() {
  if (navCount > 0) {
    let book = binbook.open(file)
    let entry = binbook.navEntry(book, tocIndex)
    pageIndex = entry.renderedPageNumber
    state.save()
    openReader()
  }
}
```

screens/reader.squid:

```squid
screen("reader") {
  display.clear("white")

  let book = binbook.open(file)
  let page = binbook.page(book, pageIndex)
  let image = binbook.pageImage(page)

  display.draw(image, { x: 0, y: 0 })

  drawFooter(string.format("{}/{}", pageIndex + 1, pageCount))
}
```

screens/toc.squid and screens/jump.squid are shown in the full reference document.

Additional helper used by the abbreviated reader screen:

```squid
function drawFooter(label) {
  display.line(0, 740, 480, 740)
  display.text(label, { x: 360, y: 760, size: "small" })
}
```

Build:

```sh
squidc build /sd/apps/binbook-reader --out /sd/apps/binbook-reader/main.sqbc --source-map
```

---

## 50. Recommended MVP

The first implementation should support:

Firmware:
- app.json parser
- SD app scanner
- .sqbc loader
- .sqbc validator
- bytecode VM
- state slots
- value stack
- call stack
- handle table
- screen.open
- screen.refresh
- app.exit
- display.clear
- display.text
- display.line
- display.rect
- display.image
- display.draw
- state.load
- state.save
- content.pickFile
- BinBook minimum capability:
  - binbook.open
  - binbook.info
  - binbook.page
  - binbook.pageImage
- optional source-map loader
- error/crash diagnostics

Compiler:
- include resolver
- tokenizer
- parser
- validator
- bytecode emitter
- source-map emitter
- permission checker
- diagnostics

Source language:
- state
- onStart
- onKey
- screen
- function
- let
- assignment
- if/else
- bounded repeat
- bounded for-in
- int/string/bool/null
- read-only records
- opaque handles

Test apps:
1. Simple Counter
2. BinBook Reader
3. Presentation Clicker

---

## 51. Summary

SquidScript is a JavaScript-like source language for low-RAM e-ink miniapps.

.squid is the authoring format.

.sqbc is the production executable format.

squidc compiles .squid to .sqbc off-device.

squidvm validates and executes .sqbc on the ESP32-C3.

Production firmware should not need a source compiler.

source-map.json is optional debug metadata.

Source maps improve crash/error messages but must never affect execution or security.

The intended architecture is:

Firmware:
- launcher
- bytecode VM
- display/input/storage/power
- permissions
- BinBook module
- crash recovery
- optional source-map diagnostics

SD card:
- app manifests
- .sqbc bytecode
- optional .squid source
- optional source maps
- declarative app data
- user content

SquidScript apps:
- define behavior
- draw screens
- handle buttons
- manage simple state
- call safe firmware capabilities

SquidScript's core language is intentionally small. First-party device behavior is exposed through standard platform capabilities known to the compiler and VM. Domain-heavy capabilities such as BinBook are acceptable when they lift parsing, validation, decoding, memory management, or target-specific work that app authors should not perform in SquidScript.

SquidScript intentionally avoids:
- full JavaScript semantics
- native plugins
- SD-loaded native binaries
- arbitrary filesystem access
- unbounded loops
- complex object mutation
- background execution
- unrestricted binary parsing
