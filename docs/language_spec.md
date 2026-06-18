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

SquidScript separates the core language from the standard platform capability set. Core language features define syntax, control flow, values, handlers, screens, state, and bytecode execution semantics. Standard platform capabilities are namespaced firmware/runtime APIs such as `service.display.*`, `state.*`, `file.*`, and device services.

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
- document and storage capabilities exposed by the current runtime

SquidScript apps own:
- app behavior
- screen definitions
- input handling
- persistent app state
- file, network, and device workflows through standard platform APIs
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
  -> squidc resolves local modules
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
10. If an app starts another app, firmware runs `event.on("app.exit")` and stores the current foreground app id as a return target.
11. When an app exits, firmware starts the previous foreground return target fresh with `event.on("app.start")`.
12. If no return target exists, firmware restarts installed `main.sqbc`.

See `docs/app_lifecycle_state_machine.md` for the foreground lifecycle state
machine, host launch handoff, fallback `main`, armed trigger activation,
planned sleep restore, and test isolation rules.

The active foreground app session preserves in-memory state across
non-lifecycle foreground event dispatches, such as key and foreground timer
handlers. App launch, app-exit returns, and armed trigger activations start
fresh VM sessions; apps must use explicit persistent state when they need data
to survive those session boundaries.

Production firmware must execute .sqbc.

Production firmware does not need to compile .squid.

---

## Concepts And Lifecycle Terms

SquidScript has one foreground app session at a time. Foreground lifecycle
events such as `app.start`, `app.exit`, key events, and foreground timer events
run against that active session. Launching another installed app or returning
from an app-exit boundary starts a fresh session; apps persist data across those
boundaries with explicit `state.*` calls.

An armed trigger registration is a firmware-owned event source declared in an
installed app's `app.triggers` metadata. `app.arm(appId)` reads that metadata
and records the trigger without running the app's foreground lifecycle code.
When the trigger fires, firmware launches the armed app as the foreground app
and dispatches the declared event handler. This is the declare → arm → fire →
launch model used by `app.triggers` and planned sleep restore.

Arming terms used throughout this spec:

- `armed app`: an installed app with at least one active armed trigger
  registration.
- `armed timer`: a timer event source registered from `app.triggers` rather
  than from a running foreground app session.
- `armed stack`: the bounded firmware list of app ids with active armed trigger
  registrations.
- `armed-app metadata`: the compiled `app.triggers` records read from an
  installed app's SQBC when `app.arm(appId)` runs.

BLE file transfer is not an armed trigger registration. It is an imperative
foreground service started by `service.ble.start("file-transfer", ...)`; a
completed transfer dispatches the configured completion event to the foreground
receiver.

See sections 21, 30, 32, and 47 for lifecycle handlers, BLE file transfer,
app registry/launch APIs, and runtime model details.

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
/sd/apps/reader/
|-- main.sqbc
|-- source-map.json
|-- main.squid
|-- device/
|   `-- display.sqdevice
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
|   `-- reader.json
|-- device-config.sqdc
|-- app-errors/
|   `-- reader.txt
|-- app-cache/
|-- app-registry.json
`-- crashlog.txt
```

Apps may read their own app directory and data directory through the normal
language/runtime storage APIs.

Apps may read external files only through explicit user selection or
firmware/app-registry-provided file association.

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
- import alias from "path"
- requires state { ... } in import-only files
- state { ... }
- device { ... }
- @preload before `event.on(...)`
- event.on("event.name") { ... }
- screen("name") { ... } unless the app entry is intentionally headless
- function name(...) { ... }
- export function name(...) { ... } in import-only files
- export screen("name") { ... } in import-only files

Top-level executable statements are not allowed.

Invalid:

```squid
state.count = state.count + 1
screen.refresh()
```

Valid:

```squid
event.on("key.RIGHT") {
  state.count = state.count + 1
  screen.refresh()
}
```

---

## 8. Modules And Imports

SquidScript supports compile-time local modules.

Modules are resolved by squidc from the app entry source file.

Production firmware does not resolve source modules.

Syntax:

```squid
import ui from "lib/ui.squid"
import reader from "screens/reader.squid"
```

The app entry file is the only file that becomes an app. Import-only files are
reusable modules. Imported files may export functions and screens, declare
their required app-state contract, and import other modules. They must not
declare `app`, `state`, `device`, `app.triggers`, or `event.on(...)`.

Module exports are explicit:

```squid
requires state {
  page: int
  title: string
}

export function openCurrent() {
  debug.print(state.title)
}

export screen("page") {
  display.text(state.title, { x: 0, y: 0 })
}
```

The importing source calls exported functions through its local alias:

```squid
import reader from "screens/reader.squid"

event.on("key.SELECT") {
  reader.openCurrent()
  screen.open(reader.page)
}
```

`screen.open(alias.screen)` is a symbolic module screen reference. squidc
validates that the alias is imported by the current file and that the screen is
exported by that module, then lowers it to the concrete screen table name in
SQBC. Local string screen references such as `screen.open("detail")` remain
valid within the source module that declares `screen("detail")`.

Import rules:
- imports are allowed only at top level
- every import must have an explicit alias
- import path must be a string literal
- import path is relative to the app directory
- import path must not contain `..`
- import path must not be absolute
- imported files must remain inside the app directory
- import cycles are rejected
- maximum import depth is enforced
- maximum number of imported files is enforced
- maximum combined source size is enforced
- duplicate aliases in one source file are compile-time errors
- aliases must not collide with local declarations or built-in namespaces
- duplicate declarations and duplicate exports are compile-time errors
- imports do not provide override behavior

Valid:

```squid
import common from "lib/common.squid"
import reader from "screens/reader.squid"
```

Invalid:

```squid
import other from "../other-app/main.squid"
import secret from "/sd/system/secret.squid"
import chosen from file.pickFile(".squid")
```

Recommended import limits:
- max imported files: 16
- max import depth: 4
- max combined source size: 32 KB to 64 KB
- max import path length: 96 bytes

Modules behave as source-level compilation units.

They are not runtime imports.

Package imports and import versioning are reserved for a future package manager
and are not accepted by the current compiler.

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

`@preload` is not valid before `function`, `screen`, `state`, `import`, or
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
state.count = state.count + 1
screen.refresh()
```

```squid
state.count = state.count + 1;
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
let lines = file.readLines("data/notes.txt", 8)
```

record:

A fixed-shape read-only object returned by built-ins.

Example:

```squid
let info = service.display.info()
state.title = info.driver
```

handle:

An opaque firmware-owned reference.

Examples:

```squid
let apps = app.registry()
let selected = app.registry.get(apps, state.selected)
```

Handles are not pointers.

Scripts can pass handles back to the firmware APIs that created or accept them, but scripts cannot inspect, serialize, forge, compare, or persist the underlying resource.

Handle lifetime is bounded to the current event or render turn unless a built-in explicitly says otherwise.

The runtime must release any remaining handles at the end of the current event or render turn.

Built-ins that return handles may also provide explicit release calls when the
runtime exposes a longer-lived handle type.

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
When a loaded string value exactly matches an existing SQBC string-pool literal,
or a firmware static string, the runtime may reuse that reference instead of
copying the value into dynamic string storage. Loaded string values that are
not available as SQBC or static strings are retained in the VM string interner
because persisted state can contain app data that was not compiled into the
app.

Example:

```squid
event.on("app.start") {
  state.load()
  if (state.stateVersion != 2) {
    state.reset()
    state.stateVersion = 2
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
  apps: app.registry()
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
  let info = service.display.info()
  state.title = info.driver
}
```

Local variables are function-scoped inside functions and render-turn scoped inside screen blocks in current draft.

Local variables are not persisted.

Local variables, parameters, and for-loop variables may share names with state fields because state access is explicit. squidc should warn on this shadowing. Local names must not shadow other locals in the same function, built-in namespaces, or function names.

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
state.count = state.count + 1
@count = @count + 1
```

`state.<field>` is the canonical persistent state form. `@field` is source sugar for the same state field read or write.

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

Bare identifiers resolve only initialized locals, parameters, and for-loop variables. Persistent state must be read or written through `state.<field>` or `@field`. If a bare name matches a declared state field, squidc should report an undeclared-variable error with a suggestion to use the explicit state form.

---

## 15. Objects and Records

The current SquidScript draft supports read-only fixed-shape records returned by built-ins.

Example:

```squid
let info = service.display.info()

state.title = info.driver
state.pageCount = info.height
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
let result = file.pickFile(".txt")
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

state.count
@count
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

Parentheses determine invocation everywhere:

```squid
foo.bar     // field or reference expression
foo.bar()   // call expression or call statement
```

This also applies to `state`: `state.load` is a state field read if `load` is declared in the app state block, while `state.load()` calls the state service method.

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
  state.count = state.count + 1
}
```

String concatenation with + is supported only when both operands are strings.

SquidScript does not perform automatic string conversion for +.

Example:

```squid
state.title = "Page " + state.suffix
```

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
class ...
async function ...
await ...
```

---

## 17.1 Built-in Namespaces

Capability APIs are namespaced.

These namespaces form the standard platform capability set for the current draft. They are not user-imported libraries, and they are not core language syntax. `squidc` validates calls against known capability signatures and emits builtin IDs into `.sqbc`; `squidvm` validates and dispatches those IDs to firmware/runtime modules.

`service.*` is reserved for target- or firmware-backed runtime capability
endpoints that app code invokes and that may vary by target availability,
binding, configuration, or runtime state. A service may be a physical or
logical device endpoint such as display or indicator, a target subsystem such
as Wi-Fi or power, or a firmware scheduler/radio facility such as timers or BLE
profiles. The common rule is that the target/runtime owns the capability
boundary and apps interact with it through bounded calls, result records,
service bindings, or service state.

Namespaces that are VM/app concepts rather than target runtime endpoints should
stay outside `service.*`. Examples include `app.*`, `screen.*`, `state.*`,
`string.*`, and `system.*`. Raw target access remains under
`hardware.*` so portable services do not imply direct register or pin control.
Content/document APIs such as `file.*` may depend on storage-backed firmware
services internally, but their app-facing namespace should reflect the
authoring concept unless they are explicitly exposing a runtime service
endpoint.

The current draft uses these built-in namespaces:

- `app.*` for app-level actions such as exit, launch, arming, and registry inspection
- `screen.*` for current-screen navigation and refresh
- `service.*` for target/firmware-backed runtime capability endpoints such as
  `service.display.*`, `service.indicator.*`, `service.wifi.*`, timers, power,
  and BLE profiles
- `device.config.*` for loading, editing, rebinding, and saving active device service configuration
- `hardware.*` for target-defined hardware capabilities such as GPIO
- `state.*` for firmware-managed persistent state
- `service.wifi.*` for firmware-owned Wi-Fi services; `wifi.*` is source sugar for the same calls
- `file.*` for user-selected files and bounded reads
- `app.registry.*` for installed app listing and inspection
- `system.*` for safe target/firmware information

Global built-ins should not be added when a capability namespace is available.

New device or document behavior should normally be added as a namespaced capability rather than as new syntax. New core syntax should be reserved for behavior that cannot be expressed clearly, safely, or efficiently through capability calls and existing value types.

---

## 17.2 Device Binding Blocks

`device {}` is a top-level service binding declaration. It binds abstract
runtime services such as display, indicator, input, and storage to concrete
device resources.

Example:

```squid
device {
  indicator { use "device/indicator.sqdevice" }
  indicator "external" { use "gpio:GPIO10" }
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
  indicator { use "gpio:GPIO8" }
  input { use "gpio-button:GPIO9:key.SELECT:activeLow" }
}
```

Rules:

- `device {}` is allowed only at top level.
- Service names are identifiers such as `indicator`, `display`, `input`, or `storage`.
- Binding names are string literals. Omitted binding name means `default`.
- Each binding body must contain exactly one `use` statement in current draft.
- The `use` target must be a string literal.
- A `.sqdevice` target is a package-relative path. It must end with
  `.sqdevice` and be safe: no absolute paths, empty segments, parent traversal
  with `..`, backslash separators, or installer/system roots such as `sd/...`
  or `system/...`.
- An inline indicator GPIO target has the form `gpio:GPIO<n>`. Current compiler
  validation accepts only the literal `GPIO` prefix plus one or two decimal
  digits.
- An inline GPIO-button input target has the form
  `gpio-button:GPIO<n>:key.<KEY>:activeLow` or
  `gpio-button:GPIO<n>:key.<KEY>:activeHigh`. `<KEY>` must be one of the
  standard logical short-key names: `UP`, `DOWN`, `LEFT`, `RIGHT`, `SELECT`,
  `BACK`, `MENU`, `HOME`, or `POWER`.
- Target-specific pin availability and pin safety remain runtime/target
  responsibilities. Firmware must reject inline GPIO and GPIO-button bindings
  whose pin is not present as GPIO-capable in the selected target metadata.

Runtime applies top-level device bindings before `event.on("app.start")`.
Failure to load, validate, or initialize a binding stops app launch with a
structured runtime error. Package install stores `.sqdevice` resources but does
not activate them by itself. Inline GPIO bindings do not require a package
resource.

Use `device {}` for static app-owned bindings. This is the default authoring
model for an app that always needs the same indicator, display, input, or other
service binding: firmware applies the binding before app code runs, and the app
does not need to manually load, edit, or rebind device configuration in
`event.on("app.start")`.

Target-defined default bindings are initialized through the same active binding
model before app code runs. Active bindings are global until changed or reboot.
A temp run may edit or rebind configuration in RAM, but those changes remain
volatile unless app code explicitly calls `device.config.save("flash")`.

Display bindings:

- `service.display.*` commands use `display default` unless a render block calls
  `service.display.select("name")`.
- Multiple display bindings are allowed only when code uses
  `service.display.select(...)` to route draw commands.
- Each new screen or render block starts on `display default`.

Indicator bindings:

- `indicator { ... }` binds `indicator.default`.
- `service.indicator.write(value)`, `service.indicator.toggle()`,
  `service.indicator.read()`, `service.indicator.breathe()`, and
  `service.indicator.blink(onMs?, offMs?)` operate on `indicator.default` in
  current draft. `breathe()` returns the default indicator to the target-defined
  breathing pattern. `blink(...)` starts a non-blocking blink pattern; omitted
  durations default to 500 ms on and 500 ms off. App-driven writes/toggles and
  automatic patterns replace each other.
- `indicator { use "gpio:GPIO<n>" }` normalizes to the same
  `indicator.default` device-binding model as a package `.sqdevice` resource.
  The simple inline form is active-high; use `.sqdevice` / SQDC when a binding
  needs explicit polarity or richer electrical configuration.
- Named indicator bindings are deferred until a target has a real second
  app-facing indicator.

Input bindings:

- Multiple input bindings may feed the same logical key event stream.
- Binding-specific electrical details remain in SQDEVICE/SQDC and firmware
  runtime code, not in compiler core.
- Inline GPIO-button input bindings normalize into SQDC metadata with
  `mode = "gpio-button"`, a `pinName`, an `event` such as `key.SELECT`, and an
  `activeLow` polarity flag. Firmware activates them as physical GPIO inputs
  and dispatches the configured logical key event when it observes a pressed
  edge.

---

## 18. If Statements

Example:

```squid
if (count > 0) {
  state.count = state.count - 1
  state.save()
  screen.refresh()
}
```

With else:

```squid
if (state.pageIndex < state.pageCount - 1) {
  state.pageIndex = state.pageIndex + 1
} else {
  debug.print("end")
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
  state.count = state.count + 1
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
  state.count = state.count + 1
}

for (;;) {
  state.count = state.count + 1
}
```

The runtime also enforces a global instruction limit per event.

---

## 20. Functions

Functions are declared with function.

Example:

```squid
function loadSlide() {
  let picked = file.pickFile(".txt")
  if (picked.ok) {
    state.file = picked.path
  }
}
```

Functions may return values:

```squid
function nextPageIndex() {
  return state.pageIndex + 1
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

See `docs/app_lifecycle_state_machine.md` for the lifecycle transition model
that determines when these handlers run and which start reason is reported.

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
  state.count = state.count + 1
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

An app entry source with no explicit `screen(...)` declarations is a headless
app. The compiler synthesizes an empty `screen("main") {}` for the app so the
SQBC screen table remains well formed. This does not make unknown screen names
valid: `screen.open("missing")` is still rejected unless a real or synthesized
screen with that name exists.

The `screen.*` namespace controls app-level view selection and refresh.

The `service.display.*` namespace draws into the current render pass using the target's logical display coordinate system. Display calls are valid only while rendering a `screen(...)` body. Event handlers and helper functions reached from event handlers should update app state, then call `screen.open(...)` or `screen.refresh()`; the screen body should read that state and issue `service.display.*` calls during render. Screen bodies must not mutate app state or app lifecycle.

The `service.display.*` namespace is canonical. Source may use the shorter
`display.*` form as sugar for the same calls. `display.clear(...)`,
`display.text(...)`, `display.line(...)`, `display.rect(...)`, and
`display.info()` compile to the
same IR and bytecode operations as `service.display.clear(...)`,
`service.display.text(...)`, `service.display.line(...)`, and
`service.display.rect(...)`, and `service.display.info()`. The shorter form
does not create a separate runtime capability or a different display binding
model.

In other words, `screen.open(...)` and `screen.refresh()` decide which view is active and when it is re-rendered; `service.display.clear(...)`, `service.display.text(...)`, and `service.display.draw(...)` describe what appears during that render. The sugar form may be used when writing source:

Example:

```squid
screen("main") {
  display.clear("white")
  display.text("Count", { x: 20, y: 40, fontHeight: 32 })
  display.text(state.count, { x: 20, y: 120, fontHeight: 48 })
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

Example stream-oriented reader screen:

```squid
screen("reader", { render: "stream" }) {
  service.display.image("data/page.bmp", { x: 0, y: 0 })
  service.display.text(state.title, { x: 12, y: 740, fontHeight: 18 })
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
- service.display.info()
- local let bindings for display-only calculations
- safe read-only value access
- render-safe handle creation for drawing APIs

The equivalent `display.clear(...)`, `display.text(...)`,
`display.line(...)`, `display.rect(...)`, and `display.info()` sugar forms are
also allowed in screen blocks.

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
  @count = @count + 1
}
```

Rationale:

Screen blocks may be re-rendered at any time.

Rendering should not change persistent app state.

Handles created during screen rendering are transient and must be released automatically by the runtime after the render turn.

If a screen block calls a user-defined function, that function must also be render-pure.

squidc should reject calls from screen blocks to functions that perform state writes, app navigation, file writes, app exit, or other non-render-safe operations.

State changes should happen in event handlers, followed by a screen refresh or navigation. Screen bodies should render from current state:

```squid
event.on("key.SELECT") {
  @count = @count + 1
  screen.refresh()
}

screen("main") {
  service.display.text(@count, { x: 4, y: 8 })
}
```

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

service.display.info()

Source sugar:

```squid
display.info()
```

Returns a read-only result record describing the active `display.default`
binding. This is a portable service query, not a `hardware.*` API. The record is
a cached firmware/runtime snapshot and must be safe to call from render-pure
screen code.

Fields:

- `ok`: bool
- `error`: string or null
- `warning`: string or null
- `available`: bool
- `status`: string
- `binding`: string
- `driver`: string
- `transport`: string
- `width`: int
- `height`: int
- `physicalWidth`: int
- `physicalHeight`: int
- `rotation`: int
- `colorModel`: string
- `logicalGrayLevels`: int
- `nativeBpp`: int
- `nativePixelFormat`: string
- `defaultFontHeight`: int
- `supportsPartialRefresh`: bool
- `supportsFastRefresh`: bool

`width` and `height` are logical drawing dimensions. Firmware owns physical
rotation, packed-pixel conversion, bus access, and driver execution. Apps should
use `display.info()` when they need to adapt layout to the installed firmware's
active display binding.

`device.config.rebind("display.default")` is the explicit operation that
validates, initializes, probes, and refreshes the active display binding after a
display SQDEVICE draft changes. A valid binding may still report
`available: false` when the configured physical display is absent or not
responding.

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

Draws a display-ready resource handle, such as a BinBook page drawable.
`drawable` must be a firmware-owned drawable handle. String paths are not
drawables.

Example:

```squid
service.display.draw(drawable, { x: 0, y: 0 })
```

`options` is optional. Omitted coordinates default to `{ x: 0, y: 0 }`.
Omitted width and height default to the drawable's natural size.

The runtime may clip drawing outside the logical screen.

The runtime may reject excessive draw commands.

service.display.refreshMode(mode)

Sets the display refresh policy for the current render flush. Valid modes are:

- `auto`: target default. On SSD1677 BinBook page turns this uses ordered-dither
  differential partial refreshes between full cleanup refreshes.
- `fast1bpp`: force a black/white fast refresh when the target supports it.
- `full`: force the cleanest full refresh path available for the target.

The override applies only to the current render pass.

Example:

```squid
service.display.refreshMode("full")
service.display.draw(page.drawable)
```

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
clears session-local timers, then starts the next foreground return target with
`event.on("app.start")`. If no return target exists, firmware restarts logical
`main`.

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

Runtime event-name storage is bounded. Event names must be shorter than the
firmware event slot, which is currently 24 bytes including the terminating NUL;
the portable authoring limit is therefore 23 UTF-8 bytes. This applies to
foreground timers, armed timers, and other runtime-dispatched event names.

Runtime resources are bounded; the full table of caps (foreground timer slots,
armed timer slots, active device-binding slots, input button slots, output
line slots, drawlog record slots, app store limits, and wire-format limits)
lives in `docs/runtime_limits.md`. Zephyr target JSON selects a build-time
runtime-limits profile under `targets/runtime-limits/`, and
`firmware/zephyr/src/runtime_limits.h` bridges generated Kconfig symbols to C
macros. Registering a foreground timer beyond the active cap returns `-ENOSPC`
to the VM.

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
diagnostics, including heap fragmentation probe availability, should use the
device protocol or CLI resource command rather than parsing this text.

system.startReason()

Returns a short string describing why the current foreground app's `app.start`
handler is running:

- `"boot"`: ordinary root app startup after firmware boot.
- `"wake"`: planned sleep resume restored this foreground app.
- `"launch"`: this foreground app was launched through app lifecycle handoff.
- `"return"`: this foreground app was restarted after another foreground app
  exited through `app.exit()`.

`system.startReason()` is a startup/lifecycle hint, not persistent app data.
Apps that need to reopen content, restore page position, or redraw a prior UI
after sleep should store that data explicitly with `state.save()` before sleep
and reload it from `app.start`.

system.storage(name)

Returns a display-oriented string for a firmware storage area. Zephyr firmware
supports:

```squid
system.storage("apps")
```

`"apps"` means firmware-managed writable SquidScript app storage. The physical
Zephyr flash-map, NVS, and LittleFS layout is target-specific firmware detail.

service.power.sleep({ wakeAfterMs })

Requests planned firmware sleep after the current VM event completes.
`wakeAfterMs` is a positive duration in milliseconds. Firmware then dispatches
`event.on("power.sleep")` to the current foreground app so the app can perform
bounded cleanup, such as `state.save()`. If the sleep-prep handler and firmware
checkpoint succeed, firmware stores lifecycle routing metadata, configures the
target wake source, and enters sleep. If sleep prep, checkpointing, or wake
configuration fails, firmware remains awake and reports diagnostics.

Planned sleep persists lifecycle routing only: the active foreground app id,
the foreground return stack app ids, and armed app ids. It does not snapshot the
VM stack, current screen, foreground timers, or service handles. On timer wake,
firmware restores the foreground app by dispatching `app.start` with
`system.startReason() == "wake"`, re-registers armed app triggers from current
installed app metadata, and preserves app-exit return behavior through the
restored return stack.
Temp foreground apps are not eligible for planned resume because their staged
SQBC slot is replaceable.

```squid
event.on("key.POWER") {
  service.power.sleep({ wakeAfterMs: 60000 })
}

event.on("power.sleep") {
  state.save()
}

event.on("app.start") {
  state.load()
  if system.startReason() == "wake" {
    debug.print("resumed")
  }
}
```

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

Each trigger event declared by `app.triggers` must be unique and must have a
matching `event.on("<event>")` handler in the same app.

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

## 28. State Built-ins

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
state.count = state.count + 1
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

## 29. Wi-Fi Service Built-ins

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
- `service.wifi.operation()`
- `service.wifi.result()`
- `service.wifi.cancel()`
- `service.wifi.scanNetwork(index)`
- `service.wifi.status()`
- `service.wifi.getAPIP()`

`startAP`, `stopAP`, `connect`, `disconnect`, and `scan` start or replace the
single foreground Wi-Fi operation and return an operation record immediately:

- `active`: bool
- `kind`: string or null; currently `startAP`, `stopAP`, `connect`,
  `disconnect`, or `scan`
- `state`: string; `idle`, `running`, `done`, `cancelled`, or `error`
- `done`: bool
- `cancelled`: bool
- `ok`: bool
- `error`: string or null

`operation()` returns the current operation record without starting a new
operation. `cancel()` cancels the current foreground Wi-Fi operation when the
target can stop it, or records the operation as cancelled when the underlying
driver cannot cancel an in-flight request.

`result()` returns the latest operation result summary:

- `ready`: bool
- `kind`: string or null
- `state`: string
- `ok`: bool
- `error`: string or null
- `cancelled`: bool
- `count`: int; for scan results, the number of bounded AP records currently
  available

The operation record describes the foreground command's progress. It is
separate from the Wi-Fi service state reported by `status()`, so a completed
scan can have `operation().state == "done"` while `status().state == "idle"`.

`scanNetwork(index)` returns one AP record from the latest completed scan:

- `ok`: bool
- `error`: string or null
- `ssid`: string, empty for hidden or undecodable SSIDs
- `ssidLength`: int
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
- `backend`: string, currently `zephyr`, `sim`, or `unavailable`
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

`status().state` is a service-level lifecycle value. Firmware maps target
driver states into the portable set `unavailable`, `idle`, `configuring`,
`starting`, `started`, `stopping`, `stopped`, or `error`; raw target driver
states are not exposed through this field.

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
reports the AP state. Target hardware tests that exercise external AP clients
must prove client association and DHCP lease assignment separately from AP
start/stop. AP client reachability to HTTP services is a separate networking
feature. Password/security policy, richer `startAP` options, profile setup UI,
hostnames, and configurable IP are deferred.

Station mode uses named profiles. SquidScript source passes only a profile name
such as `service.wifi.connect("dev")`; credentials are provisioned by firmware,
host tooling, or target setup outside SquidScript. Firmware must not expose
configured station SSIDs or passwords in SquidScript source, state, records,
logs, diagnostics, or source maps. Current ESP32-C3 development firmware
supports Wi-Fi status, bounded redacted scan/list snapshots, AP start/stop,
volatile station profiles, and station connect/disconnect through Zephyr. When
the station interface has a preferred DHCP IPv4 address,
`service.wifi.status().ipAddress` reports it.

Rules:
- Apps may start a foreground-owned access point when the target exposes the Wi-Fi service.
- Wi-Fi operations are foreground-owned and nonblocking. Apps start an
  operation, poll `wifi.operation()` or `wifi.result()` from a timer/event, and
  keep serial/runtime polling responsive while the driver progresses.
- Starting a second operation while one is running returns an operation record
  with `ok == false` and `error == "wifi busy"`.
- Apps call `wifi.scan()` again to refresh scan results, then use
  `wifi.result().count` and `wifi.scanNetwork(index)` to inspect bounded rows.
- If Wi-Fi AP or station mode is active, `wifi.scan()` returns an operation
  record with `ok == false` and `error == "wifi busy"` instead of interrupting
  radio state.
- If the target has no Wi-Fi or scanning is unsupported, Wi-Fi operation/result
  records use `ok == false` and `error == "unsupported"`.
- Scan results may expose nearby SSID names according to the target's bounded
  SSID policy, SSID byte length, channels, RSSI values, auth names, and hidden
  flags. They must not expose raw BSSID/MAC values, create, update, select, or
  reveal saved station profiles or credential values.
- Wi-Fi activity requested by a normal app is foreground-only in current draft.
- Firmware must stop or release app-owned Wi-Fi requests when the app exits, crashes, or loses foreground.
- Wi-Fi credentials must never be exposed to SquidScript source, state, records, logs, diagnostics, or source maps.
- Optional mDNS/captive-portal behavior is firmware-owned and target-dependent.

---

## 30. BLE File Transfer

BLE file transfer is an imperative `service.ble.*` capability the app drives
itself, consistent with `service.wifi.*` and `service.timer.*`. An app turns
receive on with `service.ble.start` and off with `service.ble.stop`; it is not
declared in `app.triggers`.

```squid
app "ble-install"

event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: {
      complete: "ble.file.complete"
    }
  })
}

event.on("ble.file.complete", ev) {
  let installed = app.install(ev.upload)
  app.launch(installed.id)
}
```

`service.ble.start(profile, config)` sets the calling app's BLE receive to the
given profile: it registers the profile in the routing table and begins
advertising the transfer service UUID. `profile` is `"file-transfer"`.

`config` options:
- `id`: required string. The app-local profile instance identifier, exposed to
  handlers as `ev.id` and used by firmware diagnostics.
- `accept`: required non-empty list of file-extension strings such as
  `".sqbc"`.
- `events`: required object mapping transfer event kinds to SquidScript event
  names. Current event kinds: `complete`.

`service.ble.stop()` clears the calling app's profile, aborts any in-flight
transfer, and stops advertising once no profiles remain. It takes no arguments.

Semantics:
- **One receive per app.** `start` is idempotent set/replace: calling it again
  re-applies the config (same config is a no-op; a changed config replaces the
  prior one). It never errors on a second call, so placing `start` in
  `app.start` is safe across re-launches.
- **Foreground service.** `start` activates receive for the current foreground
  app. Launching another foreground app clears the previous app's active BLE
  receive profile.
- **Installed apps and target fallback only.** Temp-run apps cannot start BLE
  receive because completed transfers route through a stable app slot. Installed
  apps use their registry slot; the target fallback app uses a reserved fallback
  slot.
- **Advertising is gated on active profiles.** The radio advertises the transfer
  service UUID only while at least one profile is registered. A target fallback
  app may start receive at boot, but receive otherwise becomes active only after
  the foreground app runs `start`.
- **Activation requires running the app once.** The profile is created by
  running `service.ble.start`, not by reading compiled metadata. After a device
  reset, BLE receive is inactive until the owning app runs `start` again.

Handler payload parameters are declared as the second argument to `event.on`.
The parameter is a read-only event record whose fields are provided by the
firmware dispatch path for that event. The `ble.file.complete` event carries:

```text
upload          file reference to the received staging file
name            uploaded safe file name
bytesReceived   string
totalBytes      string
id              the profile instance id
```

`upload` is a `file.*` reference to the staging file. The reference is valid
only inside the `ble.file.complete` handler — the firmware `fs_unlink`s the
staging file after the handler returns. Failed, aborted, or rejected transfers
do not dispatch a SquidScript event in the current firmware.

Rules:
- BLE file transfer does not grant raw GATT access to SquidScript apps.
- Firmware must stream BLE chunks to staging storage rather than app RAM.
- Firmware delivers the file as-is to the receiving app. Validation is the
  app's responsibility (e.g., via `app.install(ev.upload)` which validates the
  SQBC magic header).
- The staging file is ephemeral: it is `fs_unlink`d after the
  `ble.file.complete` event handler returns. The app must consume the file
  (copy, install, log) before returning from the handler.
- A completed transfer is delivered to the foreground app profile whose
  `accept` list contains the uploaded file-name extension, such as `.sqbc` or
  `.binbook`. The event exposes the full safe file name as `ev.name`.
- The active route table must not contain multiple receivers for the same
  uploaded extension. Ambiguous or stale route-table state is a firmware
  invariant violation: firmware records an `invariant.ble.*` diagnostic and
  rejects the transfer without dispatching an app event.
- App artifacts uploaded through BLE should use `.sqbc` until a resource
  package format is specified, and they should follow the same installer rules
  as HTTP uploads.
- A target may expose BLE radio hardware metadata without implementing
  `service.ble.start("file-transfer", ...)` runtime support.

---

## 31. File Built-ins

`file.*` is the app-facing API for user/device-visible files currently backed
by SQBC and VM runtime calls. File references abstract over backing location: a
picked or associated file may live on SD, internal flash, simulator storage, USB
mass storage, or another target-defined backend. Normal app code should pass the
returned file reference back to `file.*` calls rather than constructing physical
volume paths.

Storage location remains metadata, not part of the default read contract. The
firmware/runtime owns physical volume selection, mount state, path validation,
quota checks, and I/O errors. Apps may not directly write to `/sd/system` or
construct physical system paths.

Common storage error codes:

- `not-found`
- `read-only`
- `volume-missing`
- `volume-changed`
- `no-space`
- `invalid-name`
- `unsupported`
- `io-error`

file.pickFile(extension)

Opens a firmware-controlled file picker.

Requires runtime support:

```text
file.pick
```

Example:

```squid
let picked = file.pickFile(".txt")
if (picked.ok) {
  state.file = picked.path
}
```

On runtimes without a firmware-controlled picker, including the current
ESP32-C3 Zephyr canonical firmware, this API returns a result record rather than
crashing the app:

```text
{ ok: false, error: "unsupported", path: null }
```

file.readText(path)

Reads a bounded text file.

Requires runtime support:

```text
file.read or appdata.read, depending on path
```

Example:

```squid
let result = file.readText(state.file)
if (result.ok) {
  debug.print(result.text)
}
```

On runtimes without bounded external file reads, including the current
ESP32-C3 Zephyr canonical firmware, this API returns:

```text
{ ok: false, error: "unsupported", text: null }
```

file.readLines(path, maxLines)

Reads bounded lines from a text file.

Example:

```squid
let result = file.readLines("data/notes.txt", 100)
if (result.ok) {
  debug.print(result.lines)
}
```

On runtimes without bounded external file reads, including the current
ESP32-C3 Zephyr canonical firmware, this API returns:

```text
{ ok: false, error: "unsupported", lines: [] }
```

file.copy(source, { library, name })

Publishes a firmware-owned file reference into a logical content library.
The current Zephyr firmware supports copying a valid `.binbook` file into the
`"books"` library with a safe `.binbook` `name`.

Example:

```squid
event.on("http.file.complete", ev) {
  let copied = file.copy(ev.upload, { library: "books", name: ev.name })
  if (copied.ok) {
    debug.print(copied.ref)
  }
}
```

Result:

```text
{ ok: bool, error: string?, ref: string?, bytesWritten: int }
```

Normal error strings include `invalid-name`, `volume-missing`,
`invalid-content`, `no-space`, `unsupported`, and `io-error`.

Rules:

- file APIs return result records for normal storage/runtime unavailability
- paths must not contain `..`
- returned file paths are firmware-owned references, not raw physical paths
- direct writes and directory management are not part of the current
  language/runtime API
- larger storage, upload, data parsing, and document capabilities must
  be added as real compiler, SQBC, VM, firmware, docs, and tests slices before
  they appear in this canonical spec

---

## 31A. HTTP File Upload

HTTP upload is an imperative `service.http.*` capability for device-local
content ingress over the target network service. An app starts the route when
it wants to accept files and handles completed uploads as ordinary events.

```squid
app "content-uploader"

event.on("app.start") {
  service.wifi.startAP("SquidScript-X4")
  service.http.start("file-upload", {
    id: "binbook-upload",
    accept: [".binbook"],
    events: {
      complete: "http.file.complete"
    }
  })
}

event.on("http.file.complete", ev) {
  let copied = file.copy(ev.upload, { library: "books", name: ev.name })
  debug.print(copied.ok, copied.error, copied.ref, copied.bytesWritten)
}
```

`service.http.start(profile, config)` supports `profile` `"file-upload"`.
`config.id` is the app-local profile id, `config.accept` is a non-empty list
of accepted file extensions, and `config.events.complete` is the event
dispatched after a successful upload.

`service.http.stop()` clears the calling app's HTTP upload route, aborts any
in-flight upload, and discards any retained partial upload for that route.

On Zephyr firmware, `PUT /upload/<safe-name>` streams the request body into a
firmware staging file. A client may resume an interrupted upload by sending
`HEAD /upload/<safe-name>` and reading `X-Squid-Upload-Offset` and
`X-Squid-Upload-Total`. A resumed `PUT` sends `Content-Range: bytes
<offset>-<end>/<total>` and a body that starts at the reported offset. The
retained partial upload is process-local firmware state; rebooting the device
or stopping the HTTP service discards the resume state. A completed upload
dispatches the configured event with:

```text
upload          file reference to the received staging file
name            uploaded safe file name
bytesReceived   string
totalBytes      string
id              profile instance id
```

The `upload` reference is ephemeral. The app should consume it inside the
handler, normally by calling `file.copy(...)` for content or `app.install(...)`
for SQBC app payloads.

---

## 31B. BinBook Built-ins

`binbook.*` is the app-facing API for compiled `.binbook` raster-book
resources. The firmware owns BinBook validation, page-index lookup, page data
streaming, display conversion, and handle lifetime. App code receives handles
and result records; it does not parse BinBook bytes.

`binbook.open(path)`

Opens and validates a `.binbook` resource for the current app. The current
Zephyr firmware supports safe package resource paths such as
`"books/sample.binbook"` and opaque content refs returned by
`content.binbook.list`.

Result:

```text
{ ok: bool, error: string?, book: handle? }
```

`binbook.info(book)`

Returns metadata for an opened book handle.

Result:

```text
{ ok: bool, error: string?, title: string?, pageCount: int, chapterCount: int }
```

`binbook.readPage(book, pageIndex)`

Resolves a page from an opened book handle into a display-ready drawable
handle. The page index is zero-based. The current SSD1677 Zephyr backend
accepts target-native full-panel GRAY2 BinBook page data and streams it from
the resource file into the controller's two display planes without allocating a
full-screen framebuffer.

Result:

```text
{ ok: bool, error: string?, drawable: handle? }
```

`binbook.chapters(book)`

`binbook.chapters(book, { offset: int, limit: int })`

Lists chapter/navigation entries from an opened book handle. The runtime
materializes at most one bounded page of entries per call. The BinBook backend
reads the fixed-size `CHAPTER_INDEX` table and title `StringRef`s from the
resource file on demand; it does not load the full navigation table or string
table into RAM.

Result:

```text
{ ok: bool, error: string?, items: list, count: int, hasMore: bool }
```

Each item has:

```text
{ index: int, title: string, pageIndex: int, level: int, type: int }
```

`binbook.chapter(book, index)`

Reads one chapter entry by zero-based chapter index from an opened book handle.
Use this for actions such as jumping to the selected chapter after a menu
selection has already been tracked in app state.

Result:

```text
{ ok: bool, error: string?, index: int, title: string, pageIndex: int, level: int, type: int }
```

Example:

```squid
let opened = binbook.open("books/sample.binbook")
if (opened.ok) {
  let info = binbook.info(opened.book)
  let chapters = binbook.chapters(opened.book, { offset: 0, limit: 8 })
  let page = binbook.readPage(opened.book, state.pageIndex)
  if (page.ok) {
    service.display.draw(page.drawable)
    debug.print("pages", info.pageCount, chapters.count)
  }
}
```

Rules:

- BinBook book and drawable handles are transient firmware-owned handles.
- App state may store page indexes and package-relative paths, not handles.
- `service.display.draw(...)` is the composition API for returned drawables.
- Unsupported or invalid books/pages return result records; scripts should
  check `ok` before using returned handles.

---

## 31C. Content Library Built-ins

`content.*` APIs expose logical content libraries. They return opaque refs for
portable app code; refs are app-facing identifiers, not physical filesystem
paths.

`content.binbook.list("books")`

`content.binbook.list("books", { offset: int, limit: int })`

Lists BinBook documents in the logical `books` library. The current Zephyr
firmware merges package resources under `resources/books` with removable
storage content under the target's `books` library. The VM materializes at most
the runtime list item cap in `items`; `count` reports the total matching rows
seen by the runtime, and `hasMore` indicates that another page is available.

Result:

```text
{ ok: bool, error: string?, warning: string?, items: list, count: int, hasMore: bool }
```

Each item has:

```text
{ name: string, ref: string, size: int }
```

Example:

```squid
let page = content.binbook.list("books", { offset: 0, limit: 8 })
if (page.ok) {
  for item in page.items max 8 {
    let opened = binbook.open(item.ref)
    if (opened.ok) {
      debug.print(item.name)
    }
  }
}
```

---

## 32. App Registry And Launch Capability

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

`app.install(fileRef)`

Installs a SquidScript app from a file reference to a staged SQBC payload. The
firmware reads the app id embedded in the SQBC metadata, validates it, moves the
payload to `<store_mount_point>/apps/<id>/main.sqbc`, and registers the app in
the app registry. The call returns a record with the installed app id:

```squid
let installed = app.install(ev.upload)
app.launch(installed.id)
```

`app.install(fileRef, appId)`

Installs the same file using an explicit destination app id. This is an
override for tooling and tests; the one-argument form is the normal BLE
installer flow.

Both forms return a record with at least `id`. Runtime failures surface through
the VM error path (`-EINVAL` for malformed args, unsafe app ids, or invalid SQBC;
`-EIO` for filesystem errors).

Requires runtime support:

```text
app.install
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
  let selectedApp = app.registry.get(apps, state.selected)
  app.launch(selectedApp.id)
}
```

---

## 33. Device Config Capability

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
conflicts may return a warning while still succeeding. Unknown GPIO names,
unknown display drivers, unsafe bus/pin choices, or missing required fields
fail.

For `display.default`, SQDEVICE may describe the active display binding with a
firmware-supported driver name, a transport such as `spi` or `i2c`, transport
fields such as bus/address/pins, logical and physical dimensions, rotation,
color model, native pixel format, and text defaults. The executable display
driver must already be present in the firmware image. SQDEVICE can select and
describe a display binding; it does not load new driver code.

Example display SQDEVICE:

```text
SQDEVICE
service string 15:display.default
driver string 7:ssd1306
transport string 3:i2c
i2c.bus string 4:i2c0
i2c.address int 60
width int 78
height int 40
physicalWidth int 78
physicalHeight int 40
rotation int 0
colorModel string 4:mono
nativeBpp int 1
nativePixelFormat string 12:MONO1_PACKED
defaultFontHeight int 8
```

`device.config.save(destination)`

Persists active device configuration. The current draft supports:

```squid
device.config.save("flash")
```

This writes firmware-owned binary SQDC as the global active config. Active
config is not app-scoped, and installed app resources are not modified.

The current Zephyr canonical firmware exposes these calls through compiler
lowering, SQBC builtins, the Rust VM host, FFI, and the Zephyr runtime callback
table. The current Zephyr runtime supports package resource
`device.config.load("package:...")` into a bounded draft and
`device.config.set(...)` edits on that draft. It also validates and activates
the current `indicator.default` GPIO binding through
`device.config.rebind(...)`. Firmware-defined target defaults are exposed
through the same SQDC draft shape, but a firmware backend may apply trusted
generated defaults through a direct target-specific path when the generated
metadata has already been validated against target metadata and hardware
configuration. Author-provided, package-provided, and saved global device
config still use the normal draft/rebind path. The runtime applies target
defaults, applies saved global SQDC defaults before `app.start`, and then
applies installed app top-level `device { indicator { use ... } }` package
`.sqdevice` and inline `gpio:GPIO<n>` bindings so app-local declarations
override saved defaults.
`device.config.save("flash")` writes firmware-owned binary SQDC to the current
target's active device-config storage.

For normal static app bindings, prefer a top-level `device { ... }`
declaration instead of explicit `device.config.load(...)` calls. Use
`device.config.load(...)` when app code intentionally needs runtime device
configuration control: conditional loading, editing values with
`device.config.set(...)`, rebinding a service during execution, persisting a
device-level hardware configuration with `device.config.save("flash")`, or
running diagnostics that need to exercise the runtime configuration API.
`device.config.save("flash")` persists active device configuration, not app
state; use `state.save()` for app-owned counters, preferences, and other
per-app data.

---

## 34. Runtime APIs And Target Support

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
  service.indicator.breathe, and service.indicator.blink.

state.read
- Allows state.load.

state.write
- Allows state.save and state.reset.

appdata.read
- Allows reading files under the app's own data directory.

appdata.write
- Allows writing files under the app's own data directory.

file.pick
- Allows file.pickFile.

file.read
- Allows reading a user-selected external file.

service.wifi
- Allows foreground-owned firmware Wi-Fi service calls such as
  `service.wifi.startAP`, `service.wifi.stopAP`, `service.wifi.scan`,
  `service.wifi.operation`, `service.wifi.result`,
  `service.wifi.scanNetwork`, `service.wifi.status`, and
  `service.wifi.getAPIP`. Source may use `wifi.*` sugar for these calls.

service.wifi.scan
- Allows starting and polling nearby Wi-Fi scans without exposing credentials.

service.wifi.accessPoint
- Allows starting and stopping foreground-only firmware-owned Wi-Fi access points.

service.ble.file-transfer
- Allows declaring firmware-owned BLE file-transfer trigger profiles that
  are encoded in installed SQBC metadata. Runtime chunk receive and install
  support is target/firmware-specific and may still be unavailable.

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

## 35. Bytecode Execution Model

## Developer Builds, REPL, And Debug Console

SquidScript has build profiles. Profiles are compiler and firmware build
settings, not source-declared privileges.

Initial profiles:

- `dev`: default for Zephyr firmware and `squidc repl`
- `release`: strips debug output and is intended for smaller, less debuggable
  app artifacts

Source may contain `debug.print(...)` calls freely:

```squid
debug.print("count", state.count)
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
.squid source + local modules
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

## 36. SQBC Bytecode File

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

The loader rejects duplicate section kinds and duplicate lookup keys in parsed
tables, including state names, function names, handler event names, trigger
event names, and screen names. Table records that reference missing string-pool
entries are invalid bytecode.

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

## 37. Bytecode Validation

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

## 38. Internal Bytecode Sketch

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

## 39. Screen Compilation

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

## 40. Source Maps

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

## 41. Runtime Diagnostics

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
App: counter
Error: integer overflow
Event: key.RIGHT
Handler: event.on("key.RIGHT")
Function: increment()
Source: main.squid:18
```

Fallback without source map:

```text
App: counter
Error: integer overflow
Event: key.RIGHT
Function: fn#4
Instruction: 182
```

Crash log example:

```text
App: counter
Error: integer overflow
Event: key.RIGHT

Bytecode:
- function: key.RIGHT
- ip: 92

Call stack:
- key.RIGHT @ ip 92
- increment @ ip 178

Source:
- main.squid:18
```

---

## 42. Runtime Quotas

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
- max list items returned by a built-in: 256
- max handle count: 16 to 32

The exact values may be tuned per firmware target.

The runtime may reject apps or stop execution if limits are exceeded.

### Reference VM String References

The current Rust VM and Zephyr firmware use one string value model. A string
value references one of three sources:

- an SQBC string-pool literal compiled into the app
- a firmware static string for stable target/runtime vocabulary
- a dynamic VM-interned string for service results, loaded state values,
  concatenation results, and other runtime-created text

SQBC and firmware static strings do not consume dynamic string storage. Dynamic
strings are interned values: an exact duplicate reuses the existing dynamic
reference instead of consuming another slot or copying bytes again. When a
runtime-created string is a contiguous substring of an existing dynamic or
firmware static string, the runtime may store a substring reference instead of
copying the bytes into the dynamic string arena.

Each dispatched event starts by retaining only dynamic string values stored in
persistent app state, then clears all other dynamic strings, records, and lists
from the previous event. A value stored in `state {}` can survive into later
events; ordinary locals, service-result record fields, list items, diagnostic
strings, and intermediate concatenation results do not.

Current reference VM limits:

- operand stack values per event frame: 16
- dynamic string references per event after state retention: 32
- dynamic string byte arena per event after state retention: 512 bytes
- maximum bytes in one dynamic string: 128 bytes

Direct string-returning built-ins and string concatenation produce dynamic
interned strings unless the result exactly matches an SQBC literal or firmware
static string, or can be represented as a substring of an existing dynamic or
firmware static string. Assigning a dynamic string into `state {}` marks that
reference as retained so the next event cleanup preserves its text.
`state.load()` uses the same interner and reuses exact SQBC/static matches when
possible.

Exceeding the dynamic reference or byte budget stops the current event with a
runtime error instead of wrapping, truncating, or leaking into the next event.
Portable apps should keep very large string-returning workflows bounded, and
cursor-style APIs may be added for workloads such as large Wi-Fi scans.

---

## 43. Memory Model

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

## 44. Execution Model

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
13. if the app starts another app, firmware runs `event.on("app.exit")`, clears session-local timers, and records the current foreground app id as a return target
14. when an app exits, firmware starts the previous foreground return target fresh with `event.on("app.start")`
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

## 45. Error Handling

Bytecode validation error:
- app is marked invalid
- app is not run
- firmware or the current app may show error details

Runtime error:
- execution stops
- error is recorded
- user is returned to the previous foreground return target, or logical `main`
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
App: notes
File: main.sqbc
Source: main.squid:12
Error: runtime support file.read unavailable for file.readText()
```

If no valid source map exists:

```text
App: notes
File: main.sqbc
Function: fn#3
Instruction: 121
Error: runtime support file.read unavailable for file.readText()
```

---

## 46. Crash Recovery

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

## 47. Runtime and Platform Safety Rules

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

## 48. Unsupported JavaScript Features

The current SquidScript draft does not support:
- var
- const
- class
- new
- this
- prototype
- constructor
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

## 49. Current-Format Validation

SquidScript is pre-1.0 and has no compatibility contract. Current compiler,
runtime, firmware, and simulator artifacts are expected to agree on the current
format. If current bytecode does not run on the current runtime, treat that as a
bug to fix or an artifact to rebuild.

The canonical specification describes only current accepted forms. Removed
forms are omitted rather than documented as unsupported. Test fixtures should
not preserve removed syntax or removed API names by name, and tooling should
not add migration diagnostics, aliases, compatibility modes, or special
fallbacks unless that bridge is explicitly requested.

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

## 50. Compiler: squidc

squidc is the off-device SquidScript compiler.

squidc responsibilities:
- resolve local modules
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
squidc app build /path/to/app --out /path/to/app/main.sqbc
```

Compiler diagnostics should include:
- file path
- line number
- column number if available
- error message
- missing or unsupported API if relevant
- function/screen/handler context if relevant
- duplicate app declarations, state blocks, state fields, device bindings,
  function names, function parameters, local variables in a visible scope,
  handler events, trigger events, BLE profile ids, and screen names
- `app.triggers` events that do not have matching `event.on(...)` handlers

Example:

```text
main.squid:12: state field count must be accessed as state.count or @count
```

---

## 51. Example: Hello Menu App

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
  state.view = "menu"
  screen.open("menu")
}

event.on("key.DOWN") {
  if (state.view == "menu") {
    if (state.selected < 2) {
      state.selected = state.selected + 1
      state.save()
      screen.refresh()
    }
  }
}

event.on("key.UP") {
  if (state.view == "menu") {
    if (state.selected > 0) {
      state.selected = state.selected - 1
      state.save()
      screen.refresh()
    }
  }
}

event.on("key.SELECT") {
  if (state.selected == 0) {
    state.view = "hello"
    screen.open("hello")
  } else {
    if (state.selected == 1) {
      state.view = "about"
      screen.open("about")
    } else {
      app.exit()
    }
  }
}

event.on("key.BACK") {
  if (state.view != "menu") {
    state.view = "menu"
    state.save()
    screen.open("menu")
  } else {
    state.save()
    app.exit()
  }
}

function drawMenuRow(index, label, y) {
  if (state.selected == index) {
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
squidc app build /sd/apps/hello-menu --out /sd/apps/hello-menu/main.sqbc
```

---

## 52. Current Reference Baseline

The current reference implementation supports:

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
- file.pickFile
- file.readText
- file.readLines
- service.indicator.*
- hardware.gpio.*
- service.timer.*
- service.power.sleep
- service.wifi status, AP, station profile, and scan calls
- app registry, launch, arm, disarm, process stack, and armed stack calls
- device.config load, set, rebind, and save calls
- BLE file-transfer trigger metadata
- optional source-map loader
- error/crash diagnostics

Compiler:
- local module resolver
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
2. hardware and runtime target fixtures
3. browser simulator fixtures

---

## 53. Summary

SquidScript is a JavaScript-like source language for first-class apps on low-RAM e-ink/display devices.

.squid is the authoring format.

.sqbc is the production executable format.

squidc compiles .squid to .sqbc off-device.

squidvm validates and executes .sqbc on the ESP32-C3.

Firmware images should be distributed as UF2 where the target bootloader supports it, so users can replace firmware through a USB mass-storage copy flow.

UF2 is only for firmware replacement. SquidScript apps, source maps, app data,
and user state remain normal storage files and must not be packaged into
firmware UF2 images.

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
- Wi-Fi profile manager
- BLE trigger metadata handling where enabled by the target
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

SquidScript's core language is intentionally small. First-party device behavior
is exposed through standard platform capabilities known to the compiler and VM.
Domain-heavy capabilities should be promoted into this spec only when the
compiler, SQBC, VM, firmware, docs, and tests all support the current API.

SquidScript intentionally avoids:
- full JavaScript semantics
- native plugins
- SD-loaded native binaries
- arbitrary filesystem access
- unbounded loops
- complex object mutation
- continuously resident background VMs
- unrestricted binary parsing
