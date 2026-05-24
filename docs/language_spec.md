# SquidScript Language Specification Draft

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

SquidScript is a small JavaScript-like application language for low-RAM e-ink/display devices.

SquidScript apps are first-class device apps. They may provide app pickers, readers, upload tools, file managers, device utilities, or other user-facing workflows, just as firmware-native C/C++ apps or scripts written in another language may do on the same device.

SquidScript is not JavaScript.

SquidScript uses familiar JavaScript-style syntax for authoring, but it does not implement JavaScript's object model, prototype system, closures, async model, browser APIs, Node APIs, package system, or general dynamic execution.

SquidScript is an imperative, event-driven language with simple procedural functions and capability-oriented platform APIs. It is not object-oriented and not functional in current draft.

SquidScript separates the core language from the standard platform capability set. Core language features define syntax, control flow, values, handlers, screens, state, and bytecode execution semantics. Standard platform capabilities are namespaced firmware/runtime APIs such as `service.display.*`, `state.*`, `content.*`, and `binbook.*`.

The current SquidScript draft does not provide a user library or package system. Standard capabilities are built in from an app author's perspective, but they should be specified as declared, bounded, namespaced APIs known to `squidc`, `.sqbc` validation, and `squidvm`, not as special syntax.

Device binding declarations describe how abstract services such as display,
input, and storage bind to concrete runtime device configurations. They are a
runtime service-binding model, not a package default manifest or app trust
tier.

SquidScript is compiled off-device by squidc into .sqbc bytecode.

Production firmware loads, validates, and executes .sqbc bytecode.

Production firmware is not required to parse or compile .squid source.

Developer firmware may optionally include an on-device source compiler, but this is not part of the normal production architecture.

The firmware owns:
- boot
- process 0 system/root behavior
- starting and restarting the root `main.sqbc` app as process 1
- input
- display
- storage
- power management
- capability validation
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
- file, data, network, and device workflows through standard platform APIs
- calls to firmware capabilities exposed by the current runtime

---

## 2. Design Goals

SquidScript should support:
- user-created SD-card apps
- off-device compilation
- compact bytecode execution
- deterministic event-driven behavior
- bounded memory use
- bounded execution time
- e-ink-friendly rendering
- persistent app state
- key/button handling
- document/file access through target-defined libraries and platform APIs
- optional source-level diagnostics through source maps

SquidScript should avoid:
- SD-loaded native binaries
- direct hardware register access from apps
- direct memory access
- raw framebuffer mutation
- unbounded loops
- recursion in current draft
- dynamic code evaluation
- full JavaScript behavior
- arbitrary object mutation
- undefined or target-ambiguous filesystem access
- large runtime parser/compiler requirements on-device

SquidScript should not have undefined behavior.

Invalid source should be rejected by squidc when statically detectable.

Invalid dynamic behavior should stop the current app with a structured squidvm runtime error.

Firmware must not continue execution after type errors, invalid handles, API availability failures, arithmetic faults, out-of-bounds access, unsupported bytecode, or malformed capability data.

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

1. Firmware starts process 0 as the system/root host.
2. Firmware loads and validates the installed root `main.sqbc` app as process 1.
3. Runtime initializes state.
4. Runtime applies top-level `device {}` bindings, if any, before app code runs.
5. Runtime dispatches `event.on("app.start")`.
6. Runtime renders screens when requested by the app.
7. Runtime waits for input, timer, service, or app lifecycle events.
8. Runtime dispatches matching event handlers.
9. Runtime records recoverable errors and crash diagnostics.
10. If an app starts another app, firmware runs `event.on("app.exit")` and stores only the installed app id as a return target.
11. When an app exits, firmware starts the previous installed return target fresh with `event.on("app.start")`.
12. If no return target exists, firmware restarts installed `main.sqbc`.

The active foreground app session preserves in-memory state across
non-lifecycle foreground event dispatches, such as key and foreground timer
handlers. App launch, app-exit returns, and armed trigger activations start
fresh VM sessions; apps must use explicit persistent state when they need data
to survive those session boundaries.

Production firmware must execute .sqbc.

Production firmware does not need to compile .squid.

---

## 4. File Layout

Minimum production app:

```text
/sd/apps/simple-counter/
`-- main.sqbc
```

Debuggable app:

```text
/sd/apps/simple-counter/
|-- main.sqbc
|-- source-map.json
|-- main.squid
`-- lib/
    `-- ui.squid
```

Complex app:

```text
/sd/apps/binbook-reader/
|-- main.sqbc
|-- source-map.json
|-- main.squid
|-- device/
|   `-- epaper.sqdevice
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
|-- device-config.sqdc
|-- app-errors/
|   `-- binbook-reader.txt
|-- app-cache/
|-- app-registry.json
`-- crashlog.txt
```

Apps may read their own app directory and data directory through the normal
language/runtime storage APIs.

Apps may read external content files only through explicit user selection or firmware/app-registry-provided file association.

Apps may not directly write to /sd/system.

Installed `.sqdevice` files are read-only package resources. Installing a
package stores those resources but does not activate them by itself. Runtime
app launch activates device bindings from the app's top-level `device {}`
declaration, and explicit `device.config.*` calls may import or persist active
device configuration through firmware-owned storage.

`app.resource(...)` is deferred in current draft. Concrete APIs consume safe
package-relative paths directly until a handle-based resource API is specified.

---

## 5. App Artifact And Install Metadata

Production apps are installed and launched from their SQBC artifact. The normal
entry point is:

```text
/sd/apps/<app-id>/main.sqbc
```

The SQBC file contains the app metadata needed by firmware and tools, including
the app ID, display name, state schema, target requirements, and builtin/API
references. Firmware validates this metadata and the bytecode before execution.

There is no production `app.json`, `manifest.json`, or permission declaration
file. Capabilities are language/runtime APIs. The compiler validates
known APIs, firmware validates bytecode and target requirements, and runtime
calls fail with normal runtime or target errors when the current device cannot
perform the requested operation.

There is no public launcher app kind. A home screen, shell, or app picker is
just a SquidScript app. If it is installed as root `main.sqbc`, it is the first
app firmware starts.

`.squid` source may include optional app-level target requirements, but portable
SquidScript compilation does not require a board target. Apps should compile
against the language/runtime API. Hardware names and device aliases are
resolved by the firmware/runtime on the device. If an app uses a capability or
alias the current device does not provide, execution must fail with a
device/runtime error rather than requiring the host compiler to know the board
in the normal upload path.

Production firmware is required to support SQBC bytecode artifacts. Firmware is
not required to support source entry points.

Developer tools may compile .squid source off-device and copy the resulting .sqbc to the app directory.

Recommended production policy:
- `main.sqbc` is the executable app artifact
- `.squid.zip` is the canonical app transfer container for `main.sqbc`,
  optional static assets, `.sqdevice` files, and other read-only resources
- Current package tooling excludes `.squid` source files, dot-files,
  dot-directories,
  `source-map.json`, and existing `.squid.zip` outputs by default

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
- device { ... }
- @preload before `event.on(...)`
- event.on("event.name") { ... }
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
event.on("key.RIGHT") {
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

The current SquidScript draft does not support JavaScript-style import/export.

---

## 8.1 Attributes And Hints

Attributes attach advisory metadata to the declaration that follows them.

Supported attributes:

```squid
@preload
event.on("key.SELECT") {
  debug.print("fast select")
}
```

`@preload` is valid only before `event.on(...)` handlers. It tells firmware
that the handler is latency-sensitive and should be preloaded or kept in the
handler chunk cache when memory allows.

`@preload` is a hint, not a guarantee. Firmware may ignore it or evict the
handler chunk under memory pressure. App correctness must not depend on preload
behavior. Evicting a handler chunk is not app lifecycle behavior and does not
dispatch `event.on("app.exit")` or any other cleanup event.

`@preload` is not valid before `function`, `screen`, `state`, `include`, or
`app` declarations in current draft. Script authors should mark latency-sensitive event
handlers rather than internal helper functions.

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
@
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

The current SquidScript draft supports these value types:

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

Lists are read-only in current draft and are usually returned by built-ins.

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

The state block declares typed persistent app variables.

Example:

```squid
state({ store: "internal" }) {
  stateVersion: int = 2
  count: int = 0
  title: string = ""
  selectedBook: string? = null
  retryAt: int? = null
  done: bool = false
}
```

The store option is optional. Valid store classes are `default`, `internal`,
and `removable`; omitted store means `default`. Store classes are logical
runtime requests, not fixed filesystem paths.

State variables are persisted by `state.save()` and restored by `state.load()`.
`state.reset()` restores declared defaults for the current app and removes the
persisted state record for installed apps.

Example:

```squid
event.on("app.start") {
  state.load()
  if (stateVersion != 2) {
    state.reset()
    stateVersion = 2
    state.save()
  }
  screen.open("main")
}
```

State is stored by firmware, not by direct script file writes.

Allowed persistent state types in current draft:
- int
- bool
- string

`null` is a value, not a type. It is valid only for nullable slots declared
with `?`, such as `int?`, `bool?`, or `string?`. `retryAt: int? = 0` and
`retryAt: int? = null` are valid; `retryAt: int = null` and
`retryAt: int = "hello"` are invalid. If a saved state record contains a value
that does not match the compiled declaration, `state.load()` is a runtime error
instead of silently ignoring the bad value.

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
  file: string = ""
  pageIndex: int = 0
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

Local variables are function-scoped inside functions and render-turn scoped inside screen blocks in current draft.

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

The current SquidScript draft supports read-only fixed-shape records returned by built-ins.

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
service.display.text("Hello", { x: 20, y: 40, fontHeight: 32 })
```

Option objects are parsed by squidc and encoded into bytecode metadata.

Option objects are not general mutable objects.

---

## 15.1 Result Records

Recoverable failures use ordinary read-only result records. The current SquidScript draft
does not add exceptions, `try`, `catch`, `throw`, multiple returns, or special
error-handling keywords.

Fallible APIs return a record with at least:

- `ok`: bool
- `error`: string
- `warning`: string

On success, `ok` is `true` and `error` is `""`. On failure, `ok` is `false`
and `error` is a stable string code such as `unsupported`, `cancelled`,
`not-found`, `busy`, `no-space`, `invalid`, or `io-error`. `warning` is `""`
when there is no warning. The current draft result records use a single bounded warning
string rather than a warning list. Success payloads are additional named fields
on the same record.

Example:

```squid
let result = library.mkdir("books", "/manuals")
if (!result.ok) {
  service.display.text(result.error, { x: 20, y: 60, fontHeight: 24 })
}
```

Known fallible APIs that are unavailable on the current target return:

```text
{ ok: false, error: "unsupported" }
```

Ignoring a fallible result is valid but should produce a compiler warning.
Programmer errors, invalid bytecode, null field access, invalid handles,
arithmetic faults, and unknown built-ins remain fatal runtime errors.

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
service.display.text(string.format("{}/{}", pageIndex + 1, pageCount), { x: 360, y: 760 })
```

To combine strings and non-string values, use string.format().

Function arguments are evaluated left-to-right.

`&&` and `||` short-circuit left-to-right.

Equality is defined only for values of the same primitive type: int, bool, string, and null. Records, lists, and handles are not comparable in current draft unless a specific built-in returns a primitive identifier for comparison.

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

These namespaces form the standard platform capability set for the current draft. They are not user-imported libraries, and they are not core language syntax. `squidc` validates calls against known capability signatures and emits builtin IDs into `.sqbc`; `squidvm` validates and dispatches those IDs to firmware/runtime modules.

The current draft uses these built-in namespaces:

- `app.*` for app-level actions such as exit and firmware dialogs
- `screen.*` for current-screen navigation and refresh
- `service.*` for bindable runtime services such as `service.display.*` and `service.indicator.*`
- `device.config.*` for loading, editing, rebinding, and saving active device service configuration
- `hardware.*` for target-defined hardware capabilities such as GPIO
- `input.*` for firmware-owned text entry dialogs
- `state.*` for firmware-managed persistent state
- `stateMachine.*` for generic app state/mode helpers backed by persistent state variables
- `service.wifi.*` for firmware-owned Wi-Fi services; `wifi.*` is source sugar for the same calls
- `httpServer.*` for small foreground-only firmware-owned HTTP services
- `bluetoothHid.*` for foreground-only Bluetooth HID peripheral behavior
- `content.*` for user-selected content files and bounded reads
- `data.*` for parsed declarative app data
- `string.*` for deterministic string utilities
- `binbook.*` for BinBook document handles and drawable page resources
- `app.registry.*` for installed app listing and inspection
- `system.*` for safe target/firmware information

Global built-ins should not be added when a capability namespace is available.

New device or document behavior should normally be added as a namespaced capability rather than as new syntax. New core syntax should be reserved for behavior that cannot be expressed clearly, safely, or efficiently through capability calls and existing value types.

---

## 17.2 Device Binding Blocks

`device {}` is a top-level service binding declaration. It binds abstract
runtime services such as display, indicator, input, and storage to concrete SQDEVICE
resources packaged with the app.

Example:

```squid
device {
  indicator { use "device/indicator.sqdevice" }
  display { use "device/browser-canvas.sqdevice" }
  display "status" { use "device/status-display.sqdevice" }
  input { use "device/browser-keyboard.sqdevice" }
  input "buttons" { use "device/gpio-buttons.sqdevice" }
}
```

Shorthand service declarations omit the binding name and mean `default`:

```squid
device {
  display { use "device/epaper.sqdevice" }
}
```

The grammar shape is:

```text
device {
  serviceName { use "path.sqdevice" }
  serviceName "bindingName" { use "path.sqdevice" }
}
```

Rules:

- `device {}` is allowed only at top level.
- Service names are identifiers such as `indicator`, `display`, `input`, or `storage`.
- Binding names are string literals. Omitted binding name means `default`.
- Each binding body must contain exactly one `use` statement in current draft.
- The `use` path must be a string literal package-relative path.
- The path must end with `.sqdevice`.
- The path must be safe: no absolute paths, empty segments, parent traversal
  with `..`, backslash separators, or installer/system roots such as `sd/...`
  or `system/...`.

Runtime applies top-level device bindings before `event.on("app.start")`.
Failure to load, validate, or initialize a binding stops app launch with a
structured runtime error. Package install stores `.sqdevice` resources but does
not activate them by itself.

Active bindings are global until changed or reboot. A temp run may edit or
rebind configuration in RAM, but those changes remain volatile unless app code
explicitly calls `device.config.save("flash")`.

Display bindings:

- `service.display.*` commands use `display default` unless a render block calls
  `service.display.select("name")`.
- Multiple display bindings are allowed only when code uses
  `service.display.select(...)` to route draw commands.
- Each new screen or render block starts on `display default`.

Indicator bindings:

- `indicator { ... }` binds `indicator.default`.
- `service.indicator.write(value)`, `service.indicator.toggle()`,
  `service.indicator.read()`, and `service.indicator.breathe()` operate on
  `indicator.default` in current draft. `breathe()` returns the default indicator to the
  target-defined breathing pattern after app-driven writes or toggles.
- Named indicator bindings are deferred until a target has a real second
  app-facing indicator.

Input bindings:

- Multiple input bindings may feed the same logical key event stream.
- Binding-specific electrical details remain in SQDEVICE/SQDC and firmware
  runtime code, not in compiler core.

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
  service.display.text(line, { x: 20, y: y })
  y = y + 24
}
```

The max clause is required.

Invalid:

```squid
for line in lines {
  service.display.text(line, { x: 20, y: y })
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
event.on("app.start")
event.on("app.exit")
```

event.on("app.start") runs when the app is launched, returned to, or restarted
from cold boot. Apps should use this as the recovery entrypoint and reload any
state they need.

Example:

```squid
event.on("app.start") {
  state.load()
  screen.open("main")
}
```

event.on("app.exit") runs synchronously before the current app is replaced,
exits, or returns to another installed app. Use it to save state or release
session-local resources before handoff.

Example:

```squid
event.on("app.exit") {
  state.save()
}
```

Lifecycle handlers are optional.

---

## 22. Key Handlers

Supported key handlers:

```squid
event.on("key.UP") {}
event.on("key.DOWN") {}
event.on("key.LEFT") {}
event.on("key.RIGHT") {}
event.on("key.SELECT") {}
event.on("key.BACK") {}
event.on("key.MENU") {}
event.on("key.HOME") {}
event.on("key.POWER") {}
```

Example:

```squid
event.on("key.RIGHT") {
  count = count + 1
  state.save()
  screen.refresh()
}
```

Key names are logical input names, not raw GPIOs.

The firmware maps hardware buttons to logical keys.

Targets may also expose long-press events for logical keys:

```squid
onLongKey("POWER") {
  system.sleep()
}
```

Long-press handling is target- and firmware-policy-aware. Firmware may reserve some long-press actions for system behavior, such as putting the device to sleep, forcing a refresh, or entering recovery. When an app defines `onLongKey("POWER")`, firmware should dispatch it only if target policy allows app-visible long-press handling for that key. Otherwise the system action wins.

Short key events and long key events should be distinct. A long press should not also deliver `event.on("key.*")` unless the target explicitly opts into that behavior.

Long-press events are threshold-triggered. If a key is held past the target's long-press duration, firmware should fire the long-press event or system action immediately at that threshold. It should not wait for button release. For example, if long `POWER` sleeps after 2000 ms, the device should enter sleep once the button has been held for 2000 ms even if the user is still holding the button.

Long press is defined on logical keys, not on a specific electrical input type. GPIO buttons, key matrices, and ADC ladders may all support long press if the firmware input driver can report stable press/release state over time.

Targets may also expose key combinations, also called chords:

```squid
onChord(["POWER", "DOWN"]) {
  screen.refresh()
}
```

Combination presses are target-defined logical input events. Firmware should emit a chord only when all listed keys are pressed within the target's chord timing window and the underlying input driver can report the combination reliably.

Chord events should have explicit precedence over their component short key events. For example, if `POWER+DOWN` is recognized as a chord, firmware should suppress the individual short `POWER` and `DOWN` events for that press sequence unless the target explicitly opts into delivering both.

Chord and long-press precedence must be target-defined. A common policy is:

1. system-owned long press
2. system-owned chord
3. app-owned chord
4. app-owned long press
5. short key

This lets long `POWER` sleep remain reliable even if `POWER` participates in app-visible combinations.

---

## 23. Screens

Screens define renderable views.

The `screen.*` namespace controls app-level view selection and refresh.

The `service.display.*` namespace draws into the current render pass using the target's logical display coordinate system.

The `service.display.*` namespace is canonical. Source may use the shorter
`display.*` form as sugar for the same calls. `display.clear(...)`,
`display.text(...)`, `display.line(...)`, and `display.rect(...)` compile to the
same IR and bytecode operations as `service.display.clear(...)`,
`service.display.text(...)`, `service.display.line(...)`, and
`service.display.rect(...)`. The shorter form does not create a separate
runtime capability or a different display binding model.

In other words, `screen.open(...)` and `screen.refresh()` decide which view is active and when it is re-rendered; `service.display.clear(...)`, `service.display.text(...)`, and `service.display.draw(...)` describe what appears during that render. The sugar form may be used when writing source:

Example:

```squid
screen("main") {
  display.clear("white")
  display.text("Count", { x: 20, y: 40, fontHeight: 32 })
  display.text(count, { x: 20, y: 120, fontHeight: 48 })
}
```

Screens may optionally declare a render policy.

Syntax:

```squid
screen("name", { render: "compose" }) {
  ...
}

screen("name", { render: "stream" }) {
  ...
}
```

If `render` is omitted, the target's default screen render policy is used.

The current draft render policy values:

- `compose`: normal UI composition. This is intended for app pickers, menus, settings, dialogs, dashboards, and other screens made from several draw commands.
- `stream`: page- or image-dominant rendering. This is intended for reader pages and other screens where one large drawable should be streamed efficiently and lightweight overlays may be composed around it.

Render policy expresses app intent. It is not a request for a specific hardware buffer implementation. Firmware maps render policies to target-supported display render modes such as `single` or `strip`.

Example BinBook reader screen:

```squid
screen("reader", { render: "stream" }) {
  let book = binbook.open(file)
  let page = binbook.page(book, pageIndex)
  let image = binbook.pageImage(page)

  service.display.draw(image, { x: 0, y: 0 })
  drawBottomBar(string.format("{}/{}", pageIndex + 1, pageCount))
}
```

Only one screen is current at a time.

The current screen is selected by screen.open("screenName").

Example:

```squid
event.on("app.start") {
  screen.open("main")
}
```

In current draft, screen blocks should be render-pure.

Allowed in screen blocks:
- service.display.clear(...)
- service.display.text(...)
- service.display.line(...)
- service.display.rect(...)
- service.display.image(...)
- service.display.draw(...)
- local let bindings for display-only calculations
- string.format(...)
- safe read-only value access
- render-safe handle creation for drawing APIs

The equivalent `display.clear(...)`, `display.text(...)`,
`display.line(...)`, and `display.rect(...)` sugar forms are also allowed in
screen blocks.

Disallowed in screen blocks:
- persistent state mutation
- state.save()
- state.load()
- file writes
- app exit
- screen.open(...)
- hardware.gpio.write(...)
- hardware.gpio.toggle(...)
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

## 24. Draw Hierarchy And Refresh Semantics

The display model is redraw-from-state.

The app-visible hierarchy is:

```text
screen
  render pass
    ordered draw commands
      service.display.clear
      service.display.line
      service.display.rect
      service.display.text
      service.display.image
      service.display.draw
    drawable resources
      firmware-owned handles such as BinBook page images
```

Draw commands execute in source order. Later commands are composited over earlier commands.

Example:

```squid
service.display.clear("gray0")
service.display.rect(20, 120, 440, 48, { fillColor: "gray15" })
service.display.text("Selected", {
  x: 20,
  y: 120,
  w: 440,
  h: 48,
  fontHeight: 22,
  align: "center",
  valign: "middle",
  textColor: "gray0"
})
```

The visual order is:

1. clear to white
2. draw the black rectangle
3. draw the white text over the rectangle

Command-specific internal work is allowed when it preserves this source-order result. For example, `backgroundColor` on `service.display.text(...)` draws the text box background before drawing glyphs for that same command.

`screen.open(screenName)` stores the current screen name and renders that screen.

`screen.refresh()` does not save pixels, inspect the previous framebuffer, or reverse-engineer prior draw commands. It reruns the current screen block from bytecode using current app state and produces a fresh draw-command stream.

The source of truth for a refresh is:

- current screen name, owned by firmware/runtime
- current app state and variables, owned by squidvm
- compiled bytecode for the screen block

Firmware may keep private caches such as a previous draw-command stream, dirty regions, a framebuffer, partial-refresh regions, or strip-render plans. These are optimizations only. The language semantics remain full redraw from app state.

This is why screen blocks must be render-pure. Changing menu selection, page number, or other visual state should happen in event handlers. The handler updates state and calls `screen.refresh()` or `screen.open(...)`; the screen block then redraws the entire desired visual state.

The current draft has no app-visible retained scene graph, layers, groups, blend modes, opacity, transforms, or direct framebuffer mutation. Firmware may internally reorder or batch work only when the visual result is equivalent to source-order composition.

---

## 25. Display Coordinate System

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

Apps must not assume a framebuffer exists or that pixels can be read or mutated directly. The render policy on a screen influences how firmware should render that screen, but final composition, clipping, strip/full-buffer selection, packed-pixel conversion, physical rotation, refresh mode, and EPD transfer remain firmware responsibilities.

---

## 26. Drawing Built-ins

service.display.clear(color)

Source sugar:

```squid
display.clear(color)
```

Example:

```squid
display.clear("white")
```

Supported colors in current draft:
- "white"
- "black"
- "gray0" through "gray15"

`gray0` is white. `gray15` is black. Intermediate values are perceptual grayscale steps from light to dark.

Named aliases:
- "white" is equivalent to "gray0"
- "black" is equivalent to "gray15"

The exact native gray support depends on display capabilities. Firmware should map requested grayscale values to the nearest supported display level. If the selected target and render mode support dithering, firmware may dither intermediate grays when the physical display has fewer native levels than SquidScript's 16-level logical grayscale.

service.display.text(value, options)

Source sugar:

```squid
display.text(value, options)
```

Example:

```squid
display.text("Hello", { x: 20, y: 40, fontHeight: 32 })
```

Example with wrapping:

```squid
service.display.text(body, {
  x: 20,
  y: 120,
  w: 440,
  h: 560,
  fontHeight: 20,
  wrap: true
})
```

Example with centered highlighted text:

```squid
service.display.text("Open Book", {
  x: 20,
  y: 120,
  w: 440,
  h: 48,
  fontHeight: 22,
  align: "center",
  valign: "middle",
  textColor: "gray0",
  backgroundColor: "gray15"
})
```

Required options:
- x
- y

Optional options:
- w
- h
- fontHeight
- align
- valign
- wrap
- textColor
- backgroundColor

`w` and `h` define the text box. `fontHeight` defines the requested font height in logical pixels. Text is clipped to the text box by default. If no `w` or `h` is provided, firmware clips to the logical screen.

`align` values:
- "left"
- "center"
- "right"

`valign` values:
- "top"
- "middle"
- "bottom"

Text defaults:
- `fontHeight`: target text default
- `align`: "left"
- `valign`: "top"
- `textColor`: "gray15"
- `backgroundColor`: none
- `wrap`: false

service.display.line(x1, y1, x2, y2, options?)

Source sugar:

```squid
display.line(x1, y1, x2, y2, options?)
```

Example:

```squid
display.line(20, 96, 460, 96)
display.line(20, 96, 460, 96, { color: "gray15" })
```

Optional options:
- color

service.display.rect(x, y, w, h, options)

Source sugar:

```squid
display.rect(x, y, w, h, options)
```

Example:

```squid
display.rect(20, 100, 440, 80, { strokeColor: "gray15" })
```

Example filled:

```squid
display.rect(0, 0, 480, 40, { fillColor: "gray15" })
```

Optional options:
- strokeColor
- fillColor

service.display.image(path, options)

Example:

```squid
service.display.image("data/icon.bmp", { x: 20, y: 20 })
```

service.display.draw(drawable, options)

Draws a display-ready resource, such as an image resource, canvas surface, or BinBook page image.

Example:

```squid
service.display.draw(drawable, { x: 0, y: 0 })
```

The runtime may clip drawing outside the logical screen.

The runtime may reject excessive draw commands.

service.display.select(name)

Selects the named display binding for subsequent draw commands in the current
render block.

Example:

```squid
service.display.select("status")
service.display.clear("black")
service.display.text("OK", { x: 0, y: 0, textColor: "white" })
```

Rules:

- `name` must be a string value naming a display binding from `device {}`.
- `service.display.*` commands use `display default` until `service.display.select(...)` is
  called.
- Each new screen or render block resets selection to `display default`.
- Selecting an unknown display binding is a runtime error.
- Selecting a display that failed to bind is impossible because failed
  top-level device binding stops app launch.

---

## 27. System Built-ins

screen.open(screenName)

Selects the current screen.

Example:

```squid
screen.open("main")
```

screen.refresh()

Requests a re-render of the current screen.

The runtime reruns the current `screen("...")` block. Drawing still happens through `service.display.*` calls inside that screen block.

Example:

```squid
screen.refresh()
```

app.exit()

Exits the current app session. Firmware dispatches `event.on("app.exit")`,
clears session-local timers, then starts the next installed return target with
`event.on("app.start")`. If no return target exists, firmware restarts
installed `main.sqbc`.

Example:

```squid
app.exit()
```

app.launch(appId)

Launches an installed app immediately. Firmware dispatches the current app's
`event.on("app.exit")`, stores the current installed app id as the return
target, clears session-local timers, then starts the launched app with
`event.on("app.start")`. Temporary apps are current-only and are not stored as
return targets.

Example:

```squid
app.launch("binbook-reader")
```

app.arm(appId)

Arms an installed app for future trigger delivery. Firmware loads the target
app's compiled trigger metadata, records any `service.timer.*(...)`
registrations declared by `app.triggers`, and does not push an active session.
Trigger registration is declarative: it does not run foreground lifecycle
behavior, debug output, display work, state mutation, or app launch/exit
behavior.

Example:

```squid
app.arm("break-reminder")
```

app.disarm(appId)

Removes armed trigger registrations for an app.

service.timer.every(eventName, intervalMs)

Registers a repeating firmware timer service event source. Inside
`app.triggers`, registrations can launch the armed app later. Inside an active
app session, registrations are session-local.

`eventName` is an arbitrary app-defined event string. The timer dispatches the
matching `event.on(eventName)` handler exactly; dotted names such as
`"timer.clock"` or `"foo.bar"` are naming conventions only and do not create
namespaces or bind the event to a service.

Example:

```squid
service.timer.every("timer.clock", 60000)
```

service.timer.after(eventName, delayMs)

Registers a one-shot firmware timer service event source. `eventName` follows
the same exact string-matching rules as `service.timer.every(...)`.

Example:

```squid
service.timer.after("timer.break", 1500000)
```

system.memory()

Returns a display-oriented string describing the current target firmware's RAM
availability.

Example:

```squid
service.display.text(system.memory(), { x: 0, y: 0 })
```

The exact metric is target-specific. On Zephyr firmware it should include
static board RAM context plus live allocator/heap numbers that the target can
measure. The display string is for human diagnostics; scripts that need raw
diagnostics should use the device protocol or CLI resource command rather than
parsing this text.

system.storage(name)

Returns a display-oriented string for a firmware storage area. Zephyr firmware
supports:

```squid
system.storage("apps")
```

`"apps"` means firmware-managed writable SquidScript app storage. The physical
Zephyr flash-map, NVS, and LittleFS layout is target-specific firmware detail.

Generic events are canonical:

```squid
event.on("app.start") {}
event.on("key.SELECT") {}
event.on("key.POWER.doubleTap") {}
event.on("timer.clock") {}
event.on("service.pageTurn.forward") {}
```

`app.triggers` declares armed-app trigger registrations:

```squid
app.triggers {
  service.timer.after("timer.break", 1500000)
}
```

Break reminder example:

```squid
app "break-reminder"

app.triggers {
  service.timer.after("timer.break", 1500000)
}

event.on("timer.break") {
  screen.open("reminder")
}
```

app.message(title, body)

Shows a firmware-provided message dialog.

Example:

```squid
app.message("Error", "Could not open file.")
```

app.confirm(title, body)

Optional and not required by the current draft.

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

## 28. Input Built-ins

The `input.*` namespace provides firmware-owned text entry UI.

Input dialogs are system overlays, not SquidScript screens. While an input dialog is active, app bytecode is not executing. Firmware owns drawing, key navigation, cursor movement, editing, layout switching, cancellation, and restoration of the app screen after the dialog closes.

When an input dialog closes, firmware must invalidate the current screen. After the event handler that called `input.text(...)` returns, firmware must re-render the current `screen("...")` block automatically. Apps do not need to call `screen.refresh()` only to erase the keyboard or restore the screen that was underneath it.

input.text(title, options)

Shows a firmware-owned text entry dialog and returns the entered string.

Requires runtime support:

```text
input.text
```

Example:

```squid
let label = input.text("Book label", { maxLength: 40 })

if (label != "") {
  title = label
  state.save()
}
```

Allowed options:
- `maxLength`: int
- `initial`: string
- `placeholder`: string

The firmware may provide an e-ink-friendly grid keyboard with layout modes such as lowercase, uppercase, numbers, and symbols. Logical keys navigate and select keys:
- `LEFT` and `RIGHT` move across keyboard keys
- `UP` and `DOWN` move between rows
- `SELECT` activates the highlighted key
- `BACK` cancels or closes
- `MENU` may switch keyboard layout

Rules:
- `input.text(...)` may be called only from event handlers and user-defined functions reached from event handlers.
- `input.text(...)` is not render-safe and must not be called from screen blocks.
- returned strings are bounded by `maxLength` and target profile limits.
- cancellation returns an empty string.
- firmware may clamp `maxLength` according to target limits.
- app instruction limits do not advance while firmware input UI is active.
- closing the input dialog schedules an automatic re-render of the current screen after the calling event handler returns.
- password or credential entry for system services should use the owning system capability, such as `wifi.openSetup()`, instead of returning secrets to app code.

---

## 29. State Built-ins

state.load()

Loads firmware-managed persistent state.

Requires runtime support:

```text
state.read
```

Example:

```squid
event.on("app.start") {
  state.load()
}
```

state.save()

Saves firmware-managed persistent state atomically.

Requires runtime support:

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

Requires runtime support:

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
4. rename state.tmp to the firmware-owned state record
5. optionally delete state.bak later

---

## 30. State Machine Built-ins

The `stateMachine.*` namespace provides generic helpers for app modes and small finite-state workflows.

State machines are not new syntax and they do not store hidden runtime state. A state machine is backed by an existing string variable declared in the app's `state { ... }` block. The backing variable remains the source of truth: direct assignments to that variable immediately change the state observed by `stateMachine.current(...)` and `stateMachine.is(...)`.

Example:

```squid
state {
  uiState: string = "browser"
}

function openReader() {
  stateMachine.enter("uiState", "reader")
  screen.open("reader")
}

event.on("key.RIGHT") {
  if (stateMachine.is("uiState", "reader")) {
    nextPage()
  } else {
    if (stateMachine.is("uiState", "jump")) {
      jumpForward10()
    }
  }
}
```

stateMachine.current(backingStateName)

Returns the current state name from the backing state variable.

Example:

```squid
let current = stateMachine.current("uiState")
```

stateMachine.is(backingStateName, stateName)

Returns bool.

Example:

```squid
if (stateMachine.is("uiState", "reader")) {
  nextPage()
}
```

stateMachine.enter(backingStateName, stateName)

Sets the backing state variable to `stateName`.

Example:

```squid
stateMachine.enter("uiState", "toc")
screen.open("toc")
```

Rules:
- `backingStateName` must be a string literal naming an existing `state { ... }` variable
- the backing variable must have type string
- `stateName` must be a string literal
- `stateMachine.*` requires the `stateMachine` runtime feature but does not require a separate permission
- `stateMachine.enter(...)` mutates the in-memory backing variable but does not call `state.save()`
- screen navigation remains explicit; `stateMachine.enter(...)` does not call `screen.open(...)`
- state machines are generic app behavior helpers and must not contain domain-specific states

squidc should validate state machine use per backing variable:
- every referenced backing variable exists and is string-typed
- every entered state is a non-empty string literal
- every literal state tested with `stateMachine.is(...)` is either the backing variable's default value or appears in a `stateMachine.enter(...)` call for the same backing variable
- direct assignments to a backing variable are allowed, but assigned string literals must be in the same validated state set
- squidc should reject non-literal assignments to a backing variable unless it can prove the assigned value is one of the validated states

This validation allows misspelled states to be caught without adding enum declarations or new state-machine syntax.

---

## 31. Wi-Fi Service Built-ins

The canonical Wi-Fi namespace is `service.wifi.*`. Source may use the shorter
`wifi.*` sugar; the compiler normalizes it to the same IR and bytecode as
`service.wifi.*`.

The current draft implemented subset is AP-first with profile-based station
requests:

- `service.wifi.startAP(ssid)`
- `service.wifi.stopAP()`
- `service.wifi.connect(profileName)`
- `service.wifi.disconnect()`
- `service.wifi.scan()`
- `service.wifi.status()`
- `service.wifi.getAPIP()`

`startAP`, `stopAP`, `connect`, and `disconnect` return a result record:

- `ok`: bool
- `error`: string or null

`scan` returns a read-only snapshot record:

- `ok`: bool
- `error`: string or null
- `count`: int, the number of AP records returned after target/runtime bounds
- `networks`: read-only bounded list of AP records

Each AP record contains:

- `ssid`: string, empty for hidden or undecodable SSIDs
- `ssidLength`: int
- `bssid`: string or null
- `channel`: int
- `rssi`: int
- `auth`: string or null; stable values include `open`, `wep`, `wpa`, `wpa2`,
  `wpa/wpa2`, `wpa3`, `wpa2/wpa3`, and `unknown`
- `hidden`: bool

`status` returns a read-only record:

- `active`: bool
- `mode`: string or null, currently `"ap"` for an active access point or
  `"sta"` for a station connection attempt
- `ipAddress`: string or null
- `ssid`: string or null
- `clients`: int
- `error`: string or null
- `state`: string, one of `unavailable`, `idle`, `configuring`, `starting`,
  `started`, `stopping`, `stopped`, or `error`
- `backend`: string, currently `esp`, `sim`, or `unavailable`
- `driverStarted`: bool
- `configured`: bool
- `driverMode`: string or null
- `channel`: int, or `0` when no AP/station channel is known
- `apStartEvents`: int
- `apStopEvents`: int
- `probeEvents`: int
- `staConnectedEvents`: int
- `staDisconnectedEvents`: int
- `lastBackendCode`: string or null
- `profile`: string or null, the station profile name requested by the app
- `connected`: bool
- `scanMatches`: int
- `rssi`: int, or `0` when no station RSSI is known
- `auth`: string or null
- `bssid`: string or null
- `disconnectReason`: string or null
- `disconnectReasonCode`: int

`getAPIP` returns a read-only record:

- `ip`: string or null
- `gw`: string or null
- `netmask`: string or null
- `error`: string or null

Example:

```squid
let ap = service.wifi.startAP("SquidScript")
let status = wifi.status()

if (ap.ok && status.active) {
  service.display.text(status.ipAddress, { x: 20, y: 80, fontHeight: 24 })
}
```

For Zephyr firmware, AP defaults are target/runtime
chosen: open AP, target-chosen channel, conventional AP address
`192.168.4.1/24`, a bounded DHCPv4 lease pool on that subnet, and bounded
target-clamped client count. A successful `startAP` and
`status.state == "started"` prove that the firmware backend accepted and
reports the AP state; they do not prove that a phone or laptop can see, join,
obtain DHCP, or reach HTTP services unless the target's hardware test performs
that external-client check. Password/security policy, richer `startAP` options,
profile setup UI, hostnames, and configurable IP are deferred.

Station mode uses named profiles. SquidScript source passes only a profile name
such as `service.wifi.connect("dev")`; credentials are provisioned by firmware,
host tooling, or target setup outside SquidScript. Firmware must not expose
configured station SSIDs or passwords in SquidScript source, state, records,
logs, diagnostics, or source maps. Current ESP32-C3 development firmware
supports Wi-Fi status, scan, AP start/stop, volatile station profiles, and
station connect/disconnect through Zephyr. When the station interface has a
preferred DHCP IPv4 address, `service.wifi.status().ipAddress` reports it.

Rules:
- Apps may start a foreground-owned access point when the target exposes the Wi-Fi service.
- Wi-Fi scans are foreground-owned snapshots. Apps call `wifi.scan()` again to
  refresh results.
- If Wi-Fi AP or station mode is active, `wifi.scan()` returns
  `{ ok: false, error: "wifi busy", count: 0, networks: [] }` instead of
  interrupting radio state.
- If the target has no Wi-Fi or scanning is unsupported, `wifi.scan()` returns
  `{ ok: false, error: "unsupported", count: 0, networks: [] }`.
- Scan results may expose nearby SSID names, BSSIDs, channels, RSSI values, auth
  names, and hidden flags. They must not create, update, select, or reveal saved
  station profiles or credential values.
- Wi-Fi activity requested by a normal app is foreground-only in current draft.
- Firmware must stop or release app-owned Wi-Fi requests when the app exits, crashes, or loses foreground.
- Wi-Fi credentials must never be exposed to SquidScript source, state, records, logs, diagnostics, or source maps.
- Optional mDNS/captive-portal behavior is firmware-owned and target-dependent.

---

## 32. HTTP Server Built-ins

The `httpServer.*` namespace provides small foreground-only HTTP services owned by firmware.

SquidScript apps may use this capability to provide local web UIs, such as BinBook upload pages, library managers, or device setup tools.

SquidScript apps do not need raw sockets or manual HTTP parsing. Firmware owns request parsing, static asset serving, path matching, form parsing, upload limits, temporary file storage, cleanup, and server lifecycle. Apps start a named service and poll bounded events.

httpServer.start(serviceName, options)

Starts a named foreground HTTP service for the current app.

Requires runtime support:

```text
httpServer.serve
```

Example:

```squid
httpServer.start("uploads", {
  port: 8080,
  assets: "admin-ui",
  routes: ["upload-book"],
  uploadExtension: ".binbook",
  maxUploadBytes: 16777216
})
```

Allowed options:
- `port`: int
- `assets`: string path to a static asset directory inside the app directory
- `routes`: list of named app routes accepted for forms/uploads
- `uploadExtension`: string
- `maxUploadBytes`: int

The runtime may clamp or reject ports and upload limits according to the target profile.

Static web assets:
- this is browser-facing static file serving for a phone/computer web browser, not HTML rendering on the device display.
- firmware does not execute JavaScript from app web assets; browser-side JavaScript runs only in the user's browser.
- `assets` is an arbitrary safe package-relative directory inside the app
  directory; `web` is a convention, not a reserved name.
- paths must not be absolute and must not contain `..`.
- firmware serves files with fixed content types based on extension.
- firmware should prefer `index.html` for the asset root.
- assets are read-only to the HTTP server.
- firmware must not expose directory listings unless the app explicitly uses file-management APIs.
- current draft should support at least `.html`, `.css`, `.js`, `.png`, `.jpg`, `.jpeg`, `.svg`, `.ico`, `.txt`, and `.json`.
- static asset reads must be bounded by target limits.

httpServer.stop(serviceName)

Stops a named HTTP service owned by the current app.

Example:

```squid
httpServer.stop("uploads")
```

httpServer.status(serviceName)

Returns a read-only record describing service state.

Suggested fields:
- `running`: bool
- `url`: string
- `ipAddress`: string
- `hostname`: string
- `mode`: string such as `"station"` or `"accessPoint"`
- `port`: int
- `error`: string

httpServer.poll(serviceName)

Returns one bounded event record for the named service, or a record with `kind: "none"` if no event is pending.

Example:

```squid
let event = httpServer.poll("uploads")

if (event.kind == "formSubmit" && event.route == "upload-book") {
  let title = httpServer.field(event, "title")
  let upload = httpServer.upload(event, "book")
  let checked = binbook.inspect(upload)

  if (checked.ok) {
    library.installUpload(upload, {
      library: "books",
      folder: "/",
      name: title,
      extension: ".binbook"
    })
  }

  screen.refresh()
}
```

Suggested event fields:
- `kind`: string such as `"none"`, `"request"`, `"formSubmit"`, `"uploadStarted"`, `"uploadProgress"`, `"uploadComplete"`, or `"error"`
- `route`: string route name for app-defined form/upload routes
- `path`: string request path for static or routed requests
- `bytesReceived`: int
- `totalBytes`: int
- `error`: string

httpServer.field(event, name)

Returns a bounded string field from a `formSubmit` event.

Example:

```squid
let title = httpServer.field(event, "title")
```

httpServer.upload(event, name)

Returns an opaque upload handle from a `formSubmit` event.

Example:

```squid
let upload = httpServer.upload(event, "book")
```

Upload handles are transient firmware-owned references. Apps may pass them to APIs such as `library.installUpload(...)`, but may not inspect raw upload bytes in current draft.

Rules:
- HTTP server services are foreground-only in current draft.
- Firmware must stop all services owned by an app when that app exits, crashes, or loses foreground.
- Uploaded files must first be written to firmware-managed staging storage.
- Completed uploads should be exposed to apps as upload handles.
- Apps install or move uploaded files through library/content APIs.
- Firmware must enforce maximum body size, request count, service count, path length, header size, and event queue limits.
- Firmware must remove incomplete staged uploads when a client disconnects, the server stops, or the owning app exits.
- Firmware must reject path traversal and must not let upload form filenames choose final storage paths directly.
- Firmware should validate uploaded content only after the transfer completes and the staged file is flushed. Failed validation must delete or quarantine the staged file without publishing it.
- App uploads through HTTP should hand completed SQBC artifacts or future
  resource packages to the firmware-owned app installer. Resource package
  format is intentionally left for a separate design pass.
- Raw sockets, arbitrary outbound HTTP clients, TLS configuration, and general web frameworks are not part of the current draft.

---

## 33. Bluetooth Built-ins

The `bluetoothHid.*` namespace provides bounded Bluetooth HID peripheral behavior for apps such as presentation clickers.

Generic Bluetooth scanning, arbitrary GATT services, and raw Bluetooth data transfer are not part of the current draft. Bounded BLE upload is exposed as a firmware-owned transfer service, not raw GATT access.

bluetoothHid.start(deviceName)

Starts foreground-only Bluetooth HID advertising or reconnect behavior for the current app.

Requires runtime support:

```text
bluetoothHid.advertise
```

Example:

```squid
bluetoothHid.start("Squid Clicker")
```

bluetoothHid.stop()

Stops Bluetooth HID behavior owned by the current app.

bluetoothHid.status()

Returns a read-only record describing HID state.

Suggested fields:
- `active`: bool
- `connected`: bool
- `paired`: bool
- `deviceName`: string
- `error`: string

bluetoothHid.sendKey(keyName)

Sends one approved HID key press/release sequence.

Requires runtime support:

```text
bluetoothHid.keys
```

Example:

```squid
bluetoothHid.sendKey("PAGE_DOWN")
```

Suggested key names:
- `PAGE_UP`
- `PAGE_DOWN`
- `LEFT`
- `RIGHT`
- `UP`
- `DOWN`
- `ENTER`
- `ESCAPE`
- `SPACE`
- `VOLUME_UP`
- `VOLUME_DOWN`

Rules:
- Bluetooth HID is foreground-only in current draft.
- Firmware must stop advertising, disconnect, or release app-owned HID behavior when the app exits, crashes, or loses foreground.
- Firmware owns pairing, bonding, host trust decisions, HID report descriptors,
  rate limiting, and target/platform behavior.
- Apps may request sending only allowlisted keys supported by the target profile.
- Apps must not construct raw HID reports in current draft.

### Bluetooth File Transfer Built-ins

The `bleTransfer.*` namespace provides small foreground-only BLE upload services for devices where Wi-Fi is unavailable, disabled, or inconvenient.

BLE upload is a transport, not a separate installer. BLE, HTTP, USB-copy, and
SD-card-copy workflows should all hand completed files to the same
firmware-owned staging and installation pipeline. That shared pipeline
validates the finished file/package, sanitizes names, selects a target
library/volume, writes atomically where possible, validates SQBC bytecode and
target requirements, and then publishes the result.

BLE is usually slower than Wi-Fi and should not be the primary large-book path unless the target and client tooling make that acceptable. It is a reasonable fallback for small scripts, small books, settings bundles, and recovery workflows.

bleTransfer.start(serviceName, options)

Starts a named BLE upload service owned by the current foreground app.

Requires runtime support:

```text
bleTransfer.receive
```

Example:

```squid
bleTransfer.start("uploads", {
  name: "SquidScript XTEINK",
  uploadExtension: ".sqbc",
  maxUploadBytes: 1048576
})
```

Allowed options:
- `name`: string advertised device/service name
- `uploadExtension`: string
- `maxUploadBytes`: int
- `pairingRequired`: bool

bleTransfer.stop(serviceName)

Stops a named BLE upload service owned by the current app.

bleTransfer.status(serviceName)

Returns service state.

Suggested fields:
- `running`: bool
- `advertising`: bool
- `connected`: bool
- `bytesReceived`: int
- `totalBytes`: int
- `error`: string

bleTransfer.poll(serviceName)

Returns one bounded event record or `kind: "none"`.

Suggested event fields:
- `kind`: string such as `"none"`, `"connected"`, `"uploadStarted"`, `"uploadProgress"`, `"uploadComplete"`, or `"error"`
- `bytesReceived`: int
- `totalBytes`: int
- `error`: string

bleTransfer.upload(event)

Returns an opaque upload handle from an `uploadComplete` event.

Example:

```squid
let event = bleTransfer.poll("uploads")

if (event.kind == "uploadComplete") {
  let upload = bleTransfer.upload(event)
  library.installUpload(upload, {
    library: "apps-inbox",
    volume: "sd",
    folder: "/",
    extension: ".sqbc"
  })
}
```

Rules:
- BLE upload services are foreground-only in current draft.
- Firmware must stop services owned by an app when that app exits, crashes, or loses foreground.
- Firmware must stream BLE chunks to staging storage rather than app RAM.
- Firmware should validate uploaded content only after the transfer completes and the staged file is flushed.
- Failed validation must delete or quarantine the staged file without publishing it.
- Firmware must expose upload progress so apps can render a progress UI.
- Firmware should clamp BLE upload size below Wi-Fi upload size unless the target explicitly supports larger BLE transfers.
- App artifacts uploaded through BLE should use `.sqbc` until a resource
  package format is specified, and they should follow the same installer rules
  as HTTP uploads.

---

## 34. File and Data Built-ins

SquidScript file management should use target-defined libraries rather than raw device paths.

Libraries are named storage roots exposed by firmware. A target may provide libraries such as:

- `books`: user book/content library, normally on SD
- `apps-inbox`: uploaded app packages awaiting firmware/app-installer validation
- `appdata`: current app's private data area
- `flash-library`: internal flash-backed user library when the target defines an explicit writable flash partition

The exact backing filesystem is a firmware and target-definition concern. On XTEINK X4, SD is the primary large-content store. Internal flash may be exposed as `flash-library` only when firmware provides a mounted flash filesystem partition; it is not implicit unused firmware image space.

Logical libraries and physical volumes are separate concepts.

Examples:

```text
Logical libraries:
- books
- apps-inbox
- appdata

Physical volumes:
- sd
- flash
```

`library.list("books", { volume: "all" })` may return a merged view across SD and flash when both volumes provide book storage. Each entry should still identify its backing volume so file-manager apps can show whether a file lives on removable SD or internal flash.

Write operations should either specify a target volume or use a documented default. Large books should default to SD. Flash should be chosen explicitly or through a target-defined fallback policy.

Removable volumes may become unavailable while the device is running. Some hardware provides a card-detect GPIO; XTEINK X4 does not currently have a verified card-detect or write-protect signal in the public pinouts. When no detect signal exists, firmware must infer SD removal from I/O errors, mount probes, or changed volume identity.

Reference implementations for the XTEINK family use this same broad pattern. Papyrix initializes SdFat at startup, reports `SdCardNotFound` when mounting fails, and exposes storage operations through result/error records. CrossPoint initializes SD storage before normal app flow, gates file/web operations behind that storage layer, and removes incomplete uploads when a transfer aborts. Neither public code path depends on a documented XTEINK card-detect GPIO.

Normal storage failures should be returned as result records, not treated as VM crashes. Programmer errors, invalid bytecode, and invalid handle or API use may still stop the current app.

Common storage error codes:
- `not-found`
- `already-exists`
- `read-only`
- `volume-missing`
- `volume-changed`
- `no-space`
- `invalid-name`
- `unsupported`
- `io-error`

content.pickFile(extension)

Opens a firmware-controlled file picker.

Requires runtime support:

```text
content.pick
```

Example:

```squid
let picked = content.pickFile(".binbook")
if (picked.ok) {
  file = picked.path
}
```

content.readText(path)

Reads a bounded text file.

Requires runtime support:

```text
content.read or appdata.read, depending on path.
```

Example:

```squid
let result = content.readText(file)
if (result.ok) {
  text = result.text
}
```

content.readLines(path, maxLines)

Reads bounded lines from a text file.

Example:

```squid
let result = content.readLines("data/notes.txt", 100)
if (result.ok) {
  lines = result.lines
}
```

data.read(path)

Reads and parses a generic structured data file.

Example:

```squid
let loaded = data.read(file)
if (loaded.ok) {
  doc = loaded.doc
}
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
- apps may read and write target-defined libraries through standard capability APIs
- apps may read own app data if `appdata.read` is declared
- apps may write own app data if `appdata.write` is declared
- apps may read user-selected content if `content.read` is declared
- paths are library-relative unless a capability explicitly returns a firmware-owned path
- paths must not contain `..`

library.list(libraryId, options)

Lists bounded entries in a target-defined library.

Requires the corresponding library capability, such as:

```text
library.books.read
library.flash.read
appdata.read
```

Example:

```squid
let files = library.list("books", { path: "/", extension: ".binbook" })
if (!files.ok) {
  service.display.text(files.error, { x: 20, y: 60, fontHeight: 24 })
}
```

Paginated example:

```squid
let page = library.list("books", {
  path: "/",
  extension: ".binbook",
  volume: "all",
  limit: 50,
  cursor: cursor
})
```

Suggested result fields:
- `ok`: bool
- `entries`: list of entry records
- `nextCursor`: string
- `complete`: bool
- `error`: string

Suggested entry fields:
- `name`: string
- `path`: string library-relative path
- `kind`: string such as `"file"` or `"directory"`
- `size`: int
- `extension`: string
- `library`: string logical library ID
- `volume`: string physical volume ID such as `"sd"` or `"flash"`

library.volumes(libraryId)

Returns bounded status records for the physical volumes backing a logical library.

Example:

```squid
let volumes = library.volumes("books")
if (!volumes.ok) {
  service.display.text(volumes.error, { x: 20, y: 60, fontHeight: 24 })
}
```

Suggested volume fields:
- `id`: string such as `"sd"` or `"flash"`
- `available`: bool
- `removable`: bool
- `readOnly`: bool
- `freeBytes`: int
- `totalBytes`: int
- `error`: string

`library.volumes(...)` should trigger a bounded refresh/probe when a removable volume has no card-detect signal and is currently marked unavailable. Firmware should debounce repeated probes to avoid blocking the UI.

library.mkdir(libraryId, path)

Creates a directory in a target-defined library.

Example:

```squid
let result = library.mkdir("books", "/manuals")
```

Returns a result record:

```squid
let result = library.mkdir("books", "/manuals")
if (!result.ok) {
  service.display.text(result.error, { x: 20, y: 60, fontHeight: 24 })
}
```

library.rename(libraryId, path, newName)

Renames a file or directory without moving it to a different parent directory.

Example:

```squid
library.rename("books", "/old.binbook", "new.binbook")
```

Returns a result record.

library.move(libraryId, fromPath, toPath)

Moves a file or directory inside the same library.

Example:

```squid
library.move("books", "/incoming/book.binbook", "/manuals/book.binbook")
```

Returns a result record. Same-volume moves should use filesystem rename when possible. Cross-volume moves are not atomic; firmware must implement them as copy, verify, then delete, or reject them with `unsupported`.

library.delete(libraryId, path)

Deletes a file or empty directory.

Example:

```squid
library.delete("books", "/bad.binbook")
```

Returns a result record. Deleting non-empty directories may be rejected unless the API call explicitly opts into recursive deletion in a future revision.

library.installUpload(uploadHandle, options)

Installs a firmware-staged upload into a target-defined library.

Example:

```squid
library.installUpload(upload, {
  library: "books",
  volume: "sd",
  folder: "/",
  name: "example",
  extension: ".binbook"
})
```

Returns a result record with at least:
- `ok`: bool
- `path`: string
- `library`: string
- `volume`: string
- `error`: string

BinBook upload example:

```squid
let checked = binbook.inspect(upload)

if (checked.ok) {
  let installed = library.installUpload(upload, {
    library: "books",
    volume: "sd",
    folder: "/",
    name: checked.title,
    extension: ".binbook"
  })
}
```

App package upload example:

```squid
let installed = library.installUpload(upload, {
  library: "apps-inbox",
  volume: "sd",
  folder: "/",
  extension: ".sqbc"
})
```

Uploaded `.sqbc` files and `.squid.zip` packages are staging artifacts until
the firmware app installer validates bytecode and target requirements, places
the artifact under the app ID derived from SQBC metadata, and publishes the
installed app where the filesystem permits it. Host tools may unpack and
validate `.squid.zip` before streaming normalized package files to constrained
firmware; production firmware is not required to parse ZIP archives directly.

Rules:
- firmware must sanitize names and reject path traversal
- firmware must enforce target storage quotas and maximum file sizes
- writes should be atomic where the backing filesystem permits it
- app uploads should land in `apps-inbox`; actual app installation remains firmware-owned
- app upload extensions should be `.sqbc` or `.squid.zip`
- large books should default to SD-backed `books`, not internal flash, unless the user or app explicitly selects `flash-library`
- if a removable volume disappears during an operation, firmware should return a structured storage error and mark the volume unavailable until remount/probe succeeds

---

## 35. Generic Data Format

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

## 36. BinBook Capability

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

`.uf2` is a firmware replacement image when the target bootloader supports UF2. It is not a SquidScript app format and must not be treated as a container for `.sqbc` or `.binbook` files.

Required runtime support:

```text
binbook.read
```

Typical usage:

```squid
let opened = binbook.open(file)
if (opened.ok) {
  let info = binbook.info(opened.book)
  let page = binbook.page(opened.book, pageIndex)
  let image = binbook.pageImage(page)
  service.display.draw(image, { x: 0, y: 0 })
}
```

The BinBook capability owns document-specific work. The display service owns final composition. Prefer this style of composition over BinBook-specific rendering syntax or all-in-one helpers that bypass `service.display.*`.

Built-ins:

```text
binbook.open(path)
binbook.inspect(uploadHandle)
binbook.info(book)
binbook.pageCount(book)
binbook.pageInfo(book, pageIndex)
binbook.page(book, pageIndex)
binbook.pageImage(page)
binbook.navCount(book)
binbook.navEntry(book, navIndex)
binbook.close(book)
service.display.draw(drawable, options)
```

Minimum API:

```text
binbook.open(path)
binbook.inspect(uploadHandle)
binbook.info(book)
binbook.page(book, pageIndex)
binbook.pageImage(page)
service.display.draw(drawable, options)
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

binbook.inspect(uploadHandle)

Validates a completed firmware-staged upload as a BinBook before it is published into a library. This is intended for upload flows where the app receives an opaque upload handle from `httpServer.upload(...)` or `bleTransfer.upload(...)`.

Example:

```squid
let checked = binbook.inspect(upload)

if (checked.ok) {
  library.installUpload(upload, {
    library: "books",
    volume: "sd",
    folder: "/",
    name: checked.title,
    extension: ".binbook"
  })
}
```

Suggested result fields:
- `ok`: bool
- `title`: string
- `author`: string
- `pageCount`: int
- `logicalWidth`: int
- `logicalHeight`: int
- `bpp`: int
- `error`: string

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
service.display.draw(image, { x: 0, y: 0 })
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
  file: string = ""
  pageIndex: int = 0
}
```

Invalid state:

```squid
state {
  book: string = null
}
```

---

## 36.1 App Registry And Launch Capability

App registry support is provided by firmware. SquidScript apps may request
installed app summaries and start another installed app, but firmware owns
SQBC metadata lookup, target requirement checks, bytecode validation, lifecycle
transitions, crash recovery, and returning to the previous installed return
target.

There is no public launcher app kind. A home screen, shell, or app picker is
just a SquidScript app. If it is installed as root `main.sqbc`, it is the first
app firmware starts.

Suggested app registry API identifiers:

```text
app.registry.list
app.registry.inspect
app.launch
```

Minimum app registry capability shape:

```text
app.registry()
app.registry.get(apps, index)
app.launch(appId)
app.processStack()
app.armedStack()
app.armedStack.get(armedApps, index)
```

`app.registry()`

Returns a bounded list handle or list-like firmware-owned value containing
installed app IDs. The runtime keeps this list compact; full summary records are
materialized one app at a time through `app.registry.get(apps, index)`.

Requires runtime support:

```text
app.registry.list
```

`app.registry.get(apps, index)`

Returns a read-only record with safe installed-app summary fields.

Example record:

```text
{
  id: "binbook-reader",
  name: "BinBook Reader",
  build: "source-or-build-id",
  description: "Read BinBook files"
}
```

Requires runtime support:

```text
app.registry.inspect
```

`app.processStack()`

Returns a bounded list-like firmware-owned value containing installed app IDs
that are waiting for the active foreground app to exit. The active app itself is
not included; use firmware lifecycle diagnostics for host-side full active app
reporting.

`app.armedStack()`

Returns a bounded list-like firmware-owned value containing records for armed
app trigger registrations. Each record has:

```text
{
  appId: "break-reminder",
  event: "timer.break"
}
```

`app.armedStack.get(armedApps, index)`

Returns the armed stack record at `index`.

`app.launch(appId)`

Requests that firmware launch an installed app by app ID. Firmware dispatches
the current app's `event.on("app.exit")`, records the current installed app id
as a return target, and dispatches `event.on("app.start")` in the launched app.
When the launched app exits, firmware starts the previous installed return
target fresh. If no return target exists, firmware restarts installed
`main.sqbc`.

Requires runtime support:

```text
app.launch
```

Example app-picker flow:

```squid
state {
  selected: int = 0
}

event.on("app.start") {
  state.load()
  screen.open("apps")
}

event.on("key.SELECT") {
  let apps = app.registry()
  let selectedApp = app.registry.get(apps, selected)
  app.launch(selectedApp.id)
}
```

---

## 36.2 Device Config Capability

Device config support is provided by firmware/runtime services. SquidScript
apps may load editable text SQDEVICE records, set draft values, transactionally
rebind one service binding, and explicitly persist the active configuration.

Suggested device config API identifiers:

```text
device.config.load
device.config.set
device.config.rebind
device.config.save
```

Minimum device config capability shape:

```text
device.config.load(source)
device.config.set(key, value)
device.config.rebind(binding)
device.config.save(destination)
```

All four calls return result records:

```text
{
  ok: bool,
  error: string,
  warning: string
}
```

`device.config.load(source)`

Loads a SQDEVICE text resource into draft device configuration.

Supported source forms:

- `package:device/foo.sqdevice` for read-only package resources
- `removable:/device/active.sqdevice` for editable removable text files

Package-relative SQDEVICE resource paths may live under any safe path as long
as they end with `.sqdevice`.

`device.config.set(key, value)`

Sets one key in the draft device configuration. `key` is a dotted string such
as `spi.sck` or `display.width`. `value` may be a supported SQDEVICE primitive
value: string, int, bool, or null.

`device.config.rebind(binding)`

Transactionally initializes the named active binding from draft configuration.
Examples:

```squid
device.config.rebind("display.default")
device.config.rebind("display.status")
```

If initialization fails, firmware keeps the old active binding. Known pin
conflicts may return a warning while still succeeding. Unknown GPIO names or
missing required fields fail.

`device.config.save(destination)`

Persists active device configuration. The current draft supports:

```squid
device.config.save("flash")
```

This writes firmware-owned binary SQDC as the global active config. Active
config is not app-scoped, and installed app resources are not modified.

---

## 37. Runtime APIs And Target Support

SquidScript does not use app-declared permissions. Built-ins are normal
language/runtime APIs. The compiler validates known APIs and call shapes,
firmware validates bytecode and target requirements, and runtime calls fail
with structured runtime or target errors when a supported API is unavailable on
the current device.

Suggested API and target feature identifiers:

service.display.draw
- Allows display drawing operations such as service.display.clear, service.display.text, service.display.line, service.display.rect, service.display.image, and service.display.draw.

service.indicator
- Allows the default logical indicator operations:
  service.indicator.write, service.indicator.toggle, service.indicator.read,
  and service.indicator.breathe.

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

input.text
- Allows opening firmware-owned text entry dialogs for non-credential app input.

service.wifi
- Allows foreground-owned firmware Wi-Fi service calls such as
  `service.wifi.startAP`, `service.wifi.stopAP`, `service.wifi.scan`,
  `service.wifi.status`, and `service.wifi.getAPIP`. Source may use `wifi.*`
  sugar for these calls.

service.wifi.scan
- Allows scanning for nearby Wi-Fi networks without exposing credentials.

service.wifi.accessPoint
- Allows starting and stopping foreground-only firmware-owned Wi-Fi access points.

service.wifi.configureIp
- Allows requesting station/AP IP and hostname configuration where the target supports it.

service.wifi.setup
- Allows opening firmware-owned Wi-Fi setup UI. This does not expose Wi-Fi credentials to the app.

httpServer.serve
- Allows starting foreground-only firmware-owned HTTP services for bounded app use cases such as uploads.

library.books.read
- Allows listing and reading entries in the user book library.

library.books.write
- Allows creating folders, installing uploads, renaming, moving, and deleting entries in the user book library.

library.flash.read
- Allows listing and reading entries in the internal flash library when the target provides one.

library.flash.write
- Allows creating folders, installing uploads, renaming, moving, and deleting entries in the internal flash library when the target provides one.

library.appsInbox.write
- Allows installing uploaded app packages into the apps inbox for later firmware/app-installer validation.

bluetoothHid.advertise
- Allows starting foreground-only Bluetooth HID peripheral advertising or reconnect behavior.

bluetoothHid.keys
- Allows sending allowlisted Bluetooth HID key events while this app owns the foreground HID session.

bleTransfer.receive
- Allows starting foreground-only firmware-owned BLE upload services that produce staged upload handles.

binbook.read
- Allows BinBook document APIs.

system.info
- Allows safe device info and resource-status queries such as
  `system.memory()` and `system.storage("apps")`.

app.registry.list
- Allows an app to request the installed app list.

app.registry.inspect
- Allows an app to read safe summaries for installed apps.

app.lifecycle.inspect
- Allows an app to inspect its foreground return stack and armed trigger
  registrations.

app.launch
- Allows an app to request that firmware launch another installed app.

device.config.load
- Allows importing a SQDEVICE text resource into the runtime draft device
  configuration. Supported sources are package resources such as
  `package:device/foo.sqdevice` and removable text files such as
  `removable:/device/active.sqdevice`.

device.config.set
- Allows setting one key in the runtime draft device configuration.

device.config.rebind
- Allows transactionally initializing a new active binding such as
  `display.default` or `display.status` from draft configuration. Failure keeps
  the old active binding.

device.config.save
- Allows persisting the active device configuration to firmware-owned storage.
  current draft defines `device.config.save("flash")` as binary SQDC persistence.

API availability checks happen during source compilation, bytecode validation,
and runtime execution. If bytecode calls an unknown built-in, firmware must
reject the app or stop execution with an error. If a known built-in is not
available on the current target, firmware must return a target/runtime error.

---

## 38. Bytecode Execution Model

## Developer Builds, REPL, And Debug Console

SquidScript has build profiles. Profiles are compiler and firmware build
settings, not source-declared privileges.

Initial profiles:

- `dev`: default for Zephyr firmware and `squidc repl`
- `release`: strips debug output and is intended for smaller, less debuggable
  app artifacts

Source may contain `debug.print(...)` calls freely:

```squid
debug.print("count", count)
```

In `dev`, `debug.print(expr, ...)` evaluates its arguments left-to-right and
writes one bounded output line to the active debug console. The concrete debug
transport is firmware-defined; the reference development firmware exposes it
through the developer REPL protocol.

In `release`, `debug.print(...)` calls are removed along with their argument
evaluation. Program behavior must not depend on debug argument evaluation.

Source may also contain dev-only diagnostic blocks:

```squid
debug {
  let x = 3
  let led = service.indicator.read()
  debug.print("here", x, led)
}
```

In `dev`, a `debug { ... }` block executes normally. Variables declared with
`let` inside the block are block-local and are not visible after the block.
Assignments inside the block are only valid for variables declared in that same
debug block. The block may contain debug-local setup, read-only expressions,
bounded control flow, and `debug.print(...)`.

In `release`, the whole `debug { ... }` block is removed. Expressions inside
the block are not evaluated, and no bytecode is emitted for the block.

Debug blocks must not mutate app state, navigate screens, change app lifecycle,
start timers, write or toggle GPIO, write storage or network state, draw to the
display, return from a function, or call user-defined functions. Screen blocks
may contain `debug { ... }`, but the contents still obey screen render-purity
and the debug-block mutation rules.

`hardware.gpio.*` is the initial target hardware namespace for reference
firmware. It is for raw target GPIO resources, while portable app-facing
devices such as LEDs use service APIs such as `service.indicator.*`.
GPIO resources may also be described by target definitions for target
capability
checks, simulator configuration, documentation, and autocomplete. Raw GPIO names
return the raw pin level.

```squid
service.indicator.write(true)
let led = service.indicator.read()

hardware.gpio.write("GPIO8", true)
let raw = hardware.gpio.read("GPIO8")
```

`hardware.gpio.write(name, value)` writes a boolean logical value.
`hardware.gpio.toggle(name)` toggles the named GPIO resource.
`hardware.gpio.read(name)` returns the current hardware output level for the
raw GPIO. GPIO access is allowed in event handlers and functions, but GPIO
mutation is not render-pure and is invalid inside screen blocks.

The REPL is developer tooling, not SquidScript syntax. Event snippets are
wrapped into generated handlers. Render snippets are wrapped into generated
screen blocks and must obey screen render-purity rules.

The developer protocol is documented in `docs/developer_repl_protocol.md`.

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

1. Load .sqbc.
2. Validate .sqbc header and sections.
3. Validate bytecode instruction stream.
4. Validate SQBC app metadata and target requirements.
5. Initialize runtime state.
6. Execute event handlers.
7. Render screens.
8. Record errors and crash diagnostics.

---

## 39. SQBC Bytecode File

.sqbc is the SquidScript bytecode format.

All multi-byte integer fields in .sqbc are little-endian.

Fixed-size binary records must use explicit integer widths such as u8, u16, u32, i32, and u64.

The bytecode format must not depend on host C struct padding or alignment.

If padding bytes are needed for alignment, they must be explicit and zero-filled.

Suggested sections:

```text
SQBC header
|-- magic
|-- build/source metadata
|-- app ID hash
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
  uint16_t header_length;
  uint16_t reserved;
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
  uint16_t min_display_width;
  uint16_t min_display_height;
  uint16_t required_key_count;
  uint16_t required_feature_count;
  uint16_t required_pixel_format_count;
  uint16_t compiled_target_string_id;       // 0xffff if not target-locked
  uint16_t runtime_profile_string_id;       // 0xffff if not fixed
};
```

The fixed header is followed by arrays of string-pool IDs for required logical keys, required feature names, and required pixel format names.

---

## 40. Bytecode Validation

Precompiled bytecode is an external app artifact and must be validated before execution.

Firmware must validate .sqbc before execution.

Validation checks:
- magic is correct
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
- target requirements are structurally valid
- required features are provided by the current target
- required logical keys are provided by the current input profile
- required pixel formats are provided by the current display profile
- stack depth is bounded
- call depth is bounded
- table sizes are within limits
- checksum/hash matches

If validation fails, the app is marked invalid and must not run.

---

## 41. Internal Bytecode Sketch

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

## 42. Screen Compilation

Screen blocks may be compiled into draw-command templates.

Example source:

```squid
screen("main", { render: "compose" }) {
  service.display.clear("white")
  service.display.text(title, { x: 20, y: 40, fontHeight: 32 })
  service.display.line(20, 96, 460, 96, { color: "gray15" })
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

The screen render policy should be encoded with or adjacent to the screen's draw-command template. Firmware uses that policy to select an appropriate renderer for the target. For example, an XTEINK X4 firmware may map `compose` to a single-buffer renderer when enough SRAM is available, and map `stream` to strip rendering for BinBook page screens.

---

## 43. Source Maps

Source maps are optional debug metadata.

The device does not need source maps to run an app.

If source-map.json is present and valid, firmware may use it for:
- friendlier runtime errors
- crash logs
- app registry/install diagnostics
- source-level function/handler names
- file and line references

If source-map.json is missing, corrupt, or mismatched, firmware must ignore it and continue using bytecode-level diagnostics.

source-map.json is non-authoritative.

source-map.json must not affect:
- bytecode validation
- API availability checks
- execution behavior
- app security

Example source-map.json:

```json
{
  "format": "squid-source-map",
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
      "name": "app.start",
      "source": 0,
      "lineStart": 12,
      "lineEnd": 24,
      "ipStart": 0,
      "ipEnd": 42
    },
    {
      "kind": "handler",
      "name": "key.RIGHT",
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

## 44. Runtime Diagnostics

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
Handler: event.on("key.RIGHT")
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
Event: key.RIGHT

Bytecode:
- function: key.RIGHT
- ip: 92

Call stack:
- key.RIGHT @ ip 92
- loadPage @ ip 178

Source:
- screens/reader.squid:38
```

---

## 45. Runtime Quotas

Suggested current draft limits:

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

## 46. Memory Model

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

## 47. Execution Model

The runtime is event-driven.

Launch flow:

1. firmware starts process 0 as the system/root host
2. firmware loads root `main.sqbc` as process 1
3. runtime validates .sqbc
4. runtime optionally loads and verifies source-map.json
5. runtime initializes state defaults
6. runtime applies top-level `device {}` bindings, if any
7. runtime runs event.on("app.start")
8. runtime renders current screen when requested
9. runtime waits for input, timer, service, or lifecycle events
10. runtime runs matching event handler
11. runtime renders if requested
12. runtime saves state if requested
13. if the app starts another app, firmware runs `event.on("app.exit")`, clears session-local timers, and records only the installed app id as a return target
14. when an app exits, firmware starts the previous installed return target fresh with `event.on("app.start")`
15. when no return target exists, firmware restarts installed `main.sqbc`

Only one app is active at a time. The active foreground app keeps in-memory
state across non-lifecycle foreground events. The runtime does not keep
suspended VMs for inactive apps. Returning to an app is a fresh `app.start`, so
apps must save and restore their own state across app-session boundaries.

Armed apps are not continuously executing background VMs. `app.arm(appId)`
reads an app's compiled `app.triggers` metadata and records
`service.timer.*(...)` registrations without dispatching foreground code or
keeping a VM resident. When a registered event fires, firmware starts the armed
app as the active app session and dispatches the event handler.

No multitasking in current draft.

---

## 48. Error Handling

Bytecode validation error:
- app is marked invalid
- app is not run
- firmware or the current app may show error details

Runtime error:
- execution stops
- error is recorded
- user is returned to the previous installed return target, or root `main.sqbc`
  is restarted

Recoverable API failure:
- the current app continues execution
- the API returns a result record with `ok: false`
- `error` contains a stable string code
- unavailable target support for a known fallible API returns `unsupported`

Repeated runtime errors:
- app may be disabled until user re-enables it
- app state may be reset by user
- app files are not deleted automatically

Example error report:

```text
App: binbook-reader
File: main.sqbc
Source: screens/reader.squid:38
Error: runtime support binbook.read unavailable for binbook.open()
```

If no valid source map exists:

```text
App: binbook-reader
File: main.sqbc
Function: fn#3
Instruction: 121
Error: runtime support binbook.read unavailable for binbook.open()
```

---

## 49. Crash Recovery

Before launching an app, firmware records:
- app ID
- launch file, if any
- status: starting

After first successful render:
- status: running

On clean exit:
- status: clean

On boot:
- if previous app status was starting or running, assume crash/reset
- do not auto-resume that app
- restart root `main.sqbc`
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

## 50. Runtime and Platform Safety Rules

SquidScript apps are first-class device apps. Firmware still validates app packages and `.sqbc` bytecode before execution, just as a device platform should validate any installable app artifact before running it.

These rules define the SquidScript runtime contract:

- no native code
- no arbitrary memory access
- no raw pointers
- filesystem access goes through target-defined libraries and platform APIs
- no path traversal
- no direct hardware register access
- no app-visible Wi-Fi credentials
- no raw sockets in current draft
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

## 51. Unsupported JavaScript Features

The current SquidScript draft does not support:
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

## 52. Current-Format Validation

SquidScript is pre-1.0 and has no compatibility contract. Current compiler,
runtime, firmware, and simulator artifacts are expected to agree on the current
format. If current bytecode does not run on the current runtime, treat that as a
bug to fix or an artifact to rebuild.

Validation behavior:
- if the magic is wrong: reject app
- if the current artifact is malformed: reject app
- if required target features are unavailable: reject app or fail launch with a
  clear target error
- do not add unsupported-version paths, backwards readers, or compatibility
  modes

Source maps:
- may be ignored without affecting execution
- must match bytecode hash to be used

---

## 53. Compiler: squidc

squidc is the off-device SquidScript compiler.

squidc responsibilities:
- resolve includes
- tokenize .squid source
- parse source
- validate language rules
- validate known API calls
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
- missing or unsupported API if relevant
- function/screen/handler context if relevant

Example:

```text
screens/reader.squid:38: binbook.page requires runtime support binbook.read
```

---

## 54. Example: Hello Menu App

Installed artifact:

```text
/sd/apps/hello-menu/main.sqbc
```

main.squid:

```squid
state {
  selected: int = 0
  view: string = "menu"
}

event.on("app.start") {
  state.load()
  view = "menu"
  screen.open("menu")
}

event.on("key.DOWN") {
  if (view == "menu") {
    if (selected < 2) {
      selected = selected + 1
      state.save()
      screen.refresh()
    }
  }
}

event.on("key.UP") {
  if (view == "menu") {
    if (selected > 0) {
      selected = selected - 1
      state.save()
      screen.refresh()
    }
  }
}

event.on("key.SELECT") {
  if (selected == 0) {
    view = "hello"
    screen.open("hello")
  } else {
    if (selected == 1) {
      view = "about"
      screen.open("about")
    } else {
      app.exit()
    }
  }
}

event.on("key.BACK") {
  if (view != "menu") {
    view = "menu"
    state.save()
    screen.open("menu")
  } else {
    state.save()
    app.exit()
  }
}

function drawMenuRow(index, label, y) {
  if (selected == index) {
    service.display.text(label, {
      x: 32,
      y: y,
      w: 416,
      h: 48,
      fontHeight: 22,
      align: "center",
      valign: "middle",
      textColor: "gray0",
      backgroundColor: "gray15"
    })
  } else {
    service.display.text(label, {
      x: 32,
      y: y,
      w: 416,
      h: 48,
      fontHeight: 22,
      align: "center",
      valign: "middle",
      textColor: "gray15",
      backgroundColor: "gray0"
    })
  }
}

screen("menu", { render: "compose" }) {
  service.display.clear("gray0")

  service.display.text("Hello Menu", {
    x: 20,
    y: 60,
    w: 440,
    h: 48,
    fontHeight: 32,
    align: "center",
    valign: "middle"
  })

  drawMenuRow(0, "Say Hello", 160)
  drawMenuRow(1, "About", 216)
  drawMenuRow(2, "Exit", 272)

  service.display.text("UP/DOWN select  SELECT open", {
    x: 20,
    y: 720,
    w: 440,
    h: 32,
    fontHeight: 18,
    align: "center",
    valign: "middle",
    textColor: "gray8"
  })
}

screen("hello") {
  service.display.clear("gray0")
  service.display.text("Hello, Squid!", {
    x: 20,
    y: 120,
    w: 440,
    h: 64,
    fontHeight: 32,
    align: "center",
    valign: "middle"
  })
  service.display.text("BACK returns to menu", {
    x: 20,
    y: 720,
    w: 440,
    h: 32,
    fontHeight: 18,
    align: "center",
    valign: "middle",
    textColor: "gray8"
  })
}

screen("about") {
  service.display.clear("gray0")
  service.display.text("Selection is state.", {
    x: 32,
    y: 120,
    w: 416,
    h: 48,
    fontHeight: 24,
    align: "center",
    valign: "middle"
  })
  service.display.text("Changing selected then calling screen.refresh redraws the menu from state. The old highlight is not manually erased.", {
    x: 32,
    y: 200,
    w: 416,
    h: 160,
    fontHeight: 18,
    wrap: true
  })
}
```

In this example, `event.on("key.UP")` and `event.on("key.DOWN")` update `selected` and call `screen.refresh()`. The runtime reruns `screen("menu")`, so the newly selected row is drawn highlighted and the previously selected row is drawn normally. The app never erases the old highlight directly.

Build:

```sh
squidc build /sd/apps/hello-menu --out /sd/apps/hello-menu/main.sqbc --source-map
```

---

## 55. Example: BinBook Reader App

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

The language specification does not duplicate the reader source. The reference document explains the design choices, and the files under `examples/binbook-reader/` are the source of truth for the example implementation.

Build:

```sh
squidc build /sd/apps/binbook-reader --out /sd/apps/binbook-reader/main.sqbc --source-map
```

---

## 56. Recommended MVP

The first implementation should support:

Firmware:
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
- service.display.clear
- service.display.text
- service.display.line
- service.display.rect
- service.display.image
- service.display.draw
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
- capability declaration checker
- diagnostics

Source language:
- state
- event.on
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
1. Hello Menu
2. BinBook Reader
3. Presentation Clicker

---

## 57. Summary

SquidScript is a JavaScript-like source language for first-class apps on low-RAM e-ink/display devices.

.squid is the authoring format.

.sqbc is the production executable format.

squidc compiles .squid to .sqbc off-device.

squidvm validates and executes .sqbc on the ESP32-C3.

Firmware images should be distributed as UF2 where the target bootloader supports it, so users can replace firmware through a USB mass-storage copy flow.

UF2 is only for firmware replacement. SquidScript apps, source maps, BinBook content, and user state remain normal storage files and must not be packaged into firmware UF2 images.

Production firmware should not need a source compiler.

source-map.json is optional debug metadata.

Source maps improve crash/error messages but must never affect execution or security.

The intended architecture is:

Firmware:
- process 0 system/root host
- root `main.sqbc` app loader
- bytecode VM
- display/input/storage/power
- API and target validation
- BinBook module
- Wi-Fi profile manager
- foreground HTTP server module
- Bluetooth HID module
- firmware services that emit generic events
- crash recovery
- optional source-map diagnostics

SD card:
- .sqbc bytecode
- optional .squid source
- optional source maps
- declarative app data
- user content

SquidScript apps:
- define behavior
- draw screens
- handle buttons
- manage state
- use standard firmware capabilities

SquidScript's core language is intentionally small. First-party device behavior is exposed through standard platform capabilities known to the compiler and VM. Domain-heavy capabilities such as BinBook are acceptable when they lift parsing, validation, decoding, memory management, or target-specific work that app authors should not perform in SquidScript.

SquidScript intentionally avoids:
- full JavaScript semantics
- native plugins
- SD-loaded native binaries
- arbitrary filesystem access
- unbounded loops
- complex object mutation
- continuously resident background VMs
- unrestricted binary parsing
