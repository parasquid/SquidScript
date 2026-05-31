# SquidScript Language Philosophy and Design Principles

Status: Draft
Audience: SquidScript spec authors, compiler/runtime implementers, and advanced app authors
Scope: SquidScript core language, standard platform capabilities, and language evolution

---

## 1. Purpose

This document describes the design philosophy that should guide SquidScript language and platform evolution.

The language specification defines what SquidScript is. This document explains
why it should stay that way, how changes should be evaluated, and where new
behavior should live.

SquidScript is intended for first-class device apps on low-RAM e-ink/display devices. Apps may be written in SquidScript, firmware-native C/C++, or scripts in another language; each app model needs clear platform contracts for display, storage, networking, hardware, lifecycle, and diagnostics.

SquidScript should make useful device apps possible without turning the device into a general-purpose JavaScript runtime, a native plugin host, or an undefined filesystem scripting environment.

Current draft details may change when they do not align with these principles.

---

## 2. Design Center

SquidScript is an imperative, event-driven DSL with simple procedural functions and capability-oriented platform APIs.

It is not object-oriented in current draft. Records are fixed-shape and read-only, handles are opaque, and the language does not provide classes, prototypes, `this`, `new`, object methods, arbitrary object mutation, or computed property assignment.

It is not functional in current draft. Functions are named procedures, not first-class values. The language does not provide closures, anonymous functions, callbacks, higher-order functions, algebraic data types, expression-first control flow, or a general immutable data model.

Render-pure screen blocks are a safety and replayability rule, not a commitment to a functional programming model.

SquidScript is designed around:

- low-RAM microcontroller-class devices
- e-ink and low-refresh display behavior
- off-device compilation
- prevalidated bytecode execution
- user-authored SD-card apps
- firmware-owned hardware and document capabilities
- bounded execution, memory use, storage access, and rendering work

SquidScript is not designed around:

- full JavaScript behavior
- browser behavior
- Node behavior
- dynamic package loading
- on-device source compilation as a production requirement
- native app binaries loaded from user storage
- raw framebuffer, hardware register, or memory access from apps
- unbounded general-purpose computation

The language should be useful because its device contracts are explicit, not because it is a second-class app model.

### 2.1 Learning From Embedded Scripting Projects

SquidScript should learn from mature embedded scripting projects such as
Espruino, MicroPython, and PikaPython without adopting their full runtime
contracts.

Useful concepts include clear hardware affordances, explicit input
configuration, edge-triggered device behavior, debounce controls, timers,
interactive hardware exploration, fast developer feedback, serial app download
workflows, memory diagnostics, board capability matrices, and host/firmware
binding declarations. These ideas should be evaluated through SquidScript's app
model rather than copied directly.

The constraint is deliberate: SquidScript is not trying to be a general
programming language. SquidScript apps are compiled off-device, validated before
execution, bounded by bytecode/runtime limits, and run through firmware-owned
capabilities. Event handlers are named declarations, not arbitrary JavaScript
callbacks or closures. Input should flow through device declarations, target
metadata, and logical events. Screens should stay render-pure so firmware can
redraw, recover, or refresh without changing app state.

SquidScript service APIs should be documented as explicit interfaces between
compiled apps and firmware hosts. A service capability should have a stable
source contract, bytecode lowering, VM callback shape, firmware implementation
contract, target capability metadata, diagnostics, and tests. PikaPython's
interface-oriented C-module declarations are useful inspiration for that
documentation style; SquidScript should express the pattern in its own
compiler/SQBC/runtime terms rather than adopting Python syntax or a general
on-device module system.

For users who want a mature general-purpose programming language on a
microcontroller, Espruino is often a better fit when they want JavaScript,
interactive hardware exploration, and its app/module ecosystem. MicroPython is
often a better fit when they want Python on microcontrollers and its embedded
ecosystem. SquidScript is for installable constrained-device apps where the
app/runtime contract, target validation, display behavior, and firmware
ownership boundaries matter more than general language flexibility.

---

## 3. Core Commitments

### 3.1 Small Core Language

The core language should stay small.

Core language features include syntax, control flow, expression semantics, type rules, event handlers, screen blocks, state declarations, functions, and bytecode execution behavior.

Examples:

- `if`
- `repeat`
- `for item in list max N`
- `function`
- `state`
- `screen`
- `event.on`

A feature belongs in the core language only when it changes how all SquidScript programs are parsed, checked, represented, or executed.

New syntax should have the highest admission bar.

### 3.2 Rich Standard Capabilities

SquidScript has no user package or library system in the current draft.

Useful device behavior therefore enters the platform through standard capability namespaces known to the compiler, bytecode validator, and VM.

Examples:

- `service.display.*`
- `screen.*`
- `input.*`
- `state.*`
- `file.*`
- `data.*`
- `string.*`
- `wifi.*`
- `httpServer.*`
- `bluetoothHid.*`
- `binbook.*`

These are built in from an app author's perspective, but they are not core language syntax. They are namespaced, declared, bounded firmware/runtime APIs.

The compiler should know each standard capability's:

- name
- argument rules
- return type
- required capability declaration
- target feature requirements
- render-safety behavior
- handle behavior
- bytecode builtin ID
- diagnostic behavior

The VM should validate and dispatch standard capabilities through firmware-owned implementations.

### 3.3 No Undefined Behavior

SquidScript should not have undefined behavior.

Invalid source should be rejected by `squidc` when statically detectable.

Invalid dynamic behavior should stop the current app with a structured `squidvm` runtime error.

The firmware must not continue normal app execution after type errors, invalid handles, API availability failures, arithmetic faults, out-of-bounds access, unsupported bytecode, malformed capability data, or validation failures.

Recoverable platform failures should be explicit values. Fallible APIs should
return read-only result records with `ok: bool` and `error: string` instead of
using exceptions, hidden control flow, or multiple returns.

### 3.4 Boundedness Is A Feature

Bounded behavior is not only an implementation constraint. It is part of the language model.

SquidScript should preserve explicit limits for:

- bytecode size
- source size
- include depth
- loop iterations
- instruction count per event
- call depth
- string length
- file reads
- list sizes
- handle count
- state size
- draw command count
- rendering work

When a feature cannot be bounded clearly, it should not be added until its bounds can be specified and validated.

### 3.5 Firmware Owns Scarce Or Shared Resources

Apps should not directly own hardware registers, decoded page buffers, native pointers, display framebuffers, or memory-managed resources.

This is not because SquidScript apps are second-class. It is because on a small device these resources are shared platform services with lifecycle, recovery, and power-management rules. C/C++ app modules and Ruby scripts should also use equivalent platform contracts when they participate in the managed app environment.

The firmware/runtime should own these resources and expose them through:

- opaque handles
- read-only records
- bounded lists
- display-ready drawables
- target-profile requirements
- structured diagnostics

Handles are not pointers. Apps may pass handles to approved APIs, but they should not inspect, forge, persist, serialize, or compare them unless a capability explicitly permits it.

### 3.6 Rendering Should Be Replayable

Screen blocks should be render-pure.

Rendering may be repeated because of e-ink refresh behavior, partial refresh policies, screen restoration, or crash recovery. Re-running a screen block should not mutate persistent state, navigate the app, write files, or create non-render-safe side effects.

State changes belong in lifecycle handlers, key handlers, timers, or other event handlers.

Screen blocks describe what should appear. They should not decide what permanent app state becomes.

### 3.7 Capabilities Beat Devices

Apps should target capabilities where possible, not specific boards.

The target profile system should let a program ask for logical features such as:

- minimum logical display size
- supported pixel formats
- logical keys
- storage/content access
- `service.display.draw`
- `binbook.read`

Exact device targeting should be rare and reserved for cases where portable feature requirements cannot express the target requirement.

### 3.8 App Pickers Are Ordinary Apps

SquidScript app pickers, shells, and home screens should be written in
SquidScript when possible.

The platform should support multiple app-selection designs: grids, lists,
document-first shells, kid-mode shells, kiosk shells, or other user-facing
models.

There is no public app `kind` for launcher, foreground, background, or service
behavior in current draft. A home screen is an ordinary app, commonly installed as root
`main.sqbc`. Apps can start other installed apps through the standard app
registry/lifecycle API, while firmware owns validation, stack transitions, and
return-to-previous-app behavior.

Firmware services are not SquidScript app kinds. They are firmware modules that
may emit generic events such as `service.pageTurn.forward` for active apps to
handle.

### 3.9 Specifications And Fixtures Outrank Implementation Accidents

The written specification and golden fixtures should define SquidScript behavior.

Prototype compilers may discover good behavior, but implementation accidents should not become language semantics by default.

Accepted behavior should be moved into:

- language specification text
- capability specification text
- diagnostics contracts
- IR fixtures
- bytecode fixtures
- source map fixtures
- invalid-source fixtures

The Ruby compiler prototype may help explore behavior. The Rust compiler should reproduce the accepted specification and fixtures, not accidental Ruby quirks.

---

## 4. Language vs Platform

SquidScript should distinguish core language constructs from standard platform capabilities.

### 4.1 Core Language Example: `if`

`if` is core language.

```squid
if (state.pageIndex > 0) {
  state.pageIndex = state.pageIndex - 1
}
```

The parser, validator, bytecode encoder, and VM must understand `if` as control flow. It cannot be omitted by a target profile. Every runtime supporting the same language implementation must implement it consistently.

### 4.2 General Capability Example: `service.display.*`

`service.display.*` is a standard platform capability.

```squid
service.display.text("Hello", { x: 20, y: 40 })
service.display.draw(image, { x: 0, y: 0 })
```

The parser sees namespaced calls. The compiler validates the calls against known capability signatures, target features, and render-safety rules. The firmware display module owns composition, clipping, physical display mapping, and refresh behavior.

### 4.3 Domain Capability Example: `binbook.*`

`binbook.*` is a standard domain capability.

```squid
let book = binbook.open(state.file)
let page = binbook.page(book, state.pageIndex)
let image = binbook.pageImage(page)
service.display.draw(image, { x: 0, y: 0 })
```

The BinBook capability owns document parsing, validation, page lookup, decoding, conversion, memory management, and safe handles. Final drawing still composes through `service.display.draw`.

For recoverable failures, capability APIs return result records:

```squid
let opened = binbook.open(state.file)
if (!opened.ok) {
  service.display.text(opened.error, { x: 20, y: 60 })
}
```

This keeps BinBook first-party and built in without adding BinBook-specific syntax to the core language.

The draft BinBook capability contract is the reference example of this extensibility model:

```text
capabilities/binbook.cap.json
```

That contract describes what `squidc`, `.sqbc` validation, `squidvm`, and firmware must agree on: function names, signatures, return types, handle types, target features, render-safety rules, diagnostics, and symbolic builtin IDs. It deliberately does not include BinBook parser or decoder source code.

---

## 5. Feature Admission Rules

SquidScript should use case-by-case feature review rather than a single absolute priority such as "always maximize ergonomics" or "always minimize runtime complexity."

A future feature should be accepted only when its value is proportionate to the complexity it adds to the language, compiler, bytecode format, VM, target profiles, and diagnostics.

### 5.1 Prefer Composable Capabilities

Prefer capability APIs that return composable SquidScript values:

- `int`
- `bool`
- `string`
- `null`
- read-only records
- bounded lists
- opaque handles
- display-ready drawables

Good:

```squid
let page = binbook.page(book, state.pageIndex)
let image = binbook.pageImage(page)
service.display.draw(image, { x: 0, y: 0 })
```

Riskier:

```squid
binbook.showPage(state.file, state.pageIndex)
```

The second form may be convenient, but it combines file handling, document validation, page selection, rendering, layout, clipping, and display composition into one special path. It should be added only if the lower-level composition cannot express the behavior safely or efficiently.

### 5.2 Add Domain Capabilities When They Lift Heavy Work

Domain capabilities are appropriate when they lift work that app authors should not do in SquidScript.

Examples of heavy work:

- parsing binary document formats
- validating untrusted structured data
- streaming large files
- decoding page/image data
- tiling for low-memory rendering
- converting bit depths or pixel formats
- enforcing handle lifetimes
- applying target-specific resource limits
- hiding firmware recovery behavior

BinBook is a good standard domain capability because a low-RAM script should not parse `.binbook` bytes, manage page buffers, or know display tiling rules.

### 5.3 Add Syntax Only As A Last Resort

New syntax should be considered only when namespaced capability calls cannot express the behavior clearly, safely, or efficiently.

Before adding syntax, ask whether the behavior can be expressed as:

- a capability function
- an option object
- a read-only record
- an opaque handle
- a drawable resource
- a target-profile requirement
- a compiler diagnostic rule

If the answer is yes, prefer the non-syntax form.

### 5.4 Avoid Global Built-Ins

Global built-ins should be avoided when a capability namespace is available.

Prefer:

```squid
string.format("{}/{}", state.pageIndex + 1, state.pageCount)
screen.refresh()
state.save()
```

Avoid:

```squid
format("{}/{}", pageIndex + 1, pageCount)
refresh()
save()
```

Namespaces make diagnostics, documentation, target checks, and bytecode validation clearer.

---

## 6. Extension Tests

Every proposed feature should pass these tests or document why it is an exception.

### 6.1 Boundedness Test

Can the feature's memory, time, bytecode, file, handle, and rendering costs be bounded by the spec and target profiles?

### 6.2 Heavy-Lifting Test

Does the feature remove work that is unsafe, too expensive, too target-specific, or too error-prone for app authors?

### 6.3 Composition Test

Can the feature compose through existing values and capabilities instead of creating a special control path?

### 6.4 Target Availability Test

Can the feature's availability be validated through known APIs, SQBC metadata,
target profiles, and structured runtime errors?

### 6.5 Target-Profile Test

Can the feature's availability and limits be expressed through target profiles and target checks?

### 6.6 Diagnostics Test

Can `squidc` and `squidvm` produce clear structured diagnostics for invalid use?

### 6.7 Render-Safety Test

If the feature can be used from a screen block, is its render behavior pure, bounded, and replayable?

### 6.8 Fixture Test

Can the behavior be captured in valid and invalid fixtures, expected diagnostics, expected IR, expected bytecode, and source maps where relevant?

### 6.9 Spec Test

Can the behavior be specified without relying on "whatever the firmware happens to do"?

---

## 7. Compatibility And Evolution

SquidScript should evolve deliberately.

The current draft may change when the language is still draft, but every accepted change should clarify:

- whether it changes core language semantics
- whether it adds or changes a standard capability
- whether it changes bytecode validation
- whether it changes target-profile requirements
- whether old bytecode remains valid
- whether old source remains valid
- what diagnostics should be produced for unavailable or invalid use

When implementation clarity and improvement conflict, the spec should state the update path explicitly.

---

## 8. Non-Goals

SquidScript should continue to avoid:

- full JavaScript behavior
- JavaScript object/prototype semantics
- closures in the current draft
- async/await in the current draft
- dynamic evaluation
- user package imports
- runtime source imports
- SD-loaded native binaries
- unrestricted filesystem access
- raw framebuffer mutation
- arbitrary binary parsing in app code
- unbounded loops
- recursion in the current draft
- broad mutable object graphs
- hidden target-specific behavior in app code

Avoiding these is not a lack of ambition. It is how SquidScript remains suitable for constrained, user-extensible firmware.

---

## 9. Practical Decision Guide

When deciding where a new feature belongs:

1. If it changes parsing, control flow, evaluation, or type semantics for all programs, consider it core language.
2. If it exposes reusable device behavior, make it a standard platform capability.
3. If it exposes a first-party document or media workflow that lifts heavy firmware-native work, make it a standard domain capability.
4. If it is only a convenience wrapper over existing capabilities, prefer leaving it out unless it removes common error-prone boilerplate.
5. If it requires target-specific resources, express that through target profiles and SQBC metadata.
6. If it cannot be bounded, diagnosed, validated, or fixture-tested, do not add it yet.

Examples:

| Proposal | Preferred Category | Rationale |
|---|---|---|
| `if` | Core language | Changes control flow and bytecode execution |
| `service.display.draw(drawable, options)` | Standard platform capability | Exposes display composition through firmware |
| `binbook.pageImage(page)` | Standard domain capability | Converts document content into a composable drawable |
| BinBook-specific syntax | Usually reject | Special syntax is not needed when capability calls compose |
| `binbook.showPage(file, index)` | Require review | Convenient, but may bypass composition and combine too many responsibilities |
| User package imports | Defer/reject for the current draft | Adds dependency, validation, and runtime model complexity |

The concrete reference for capability-based platform extensibility is `capabilities/binbook.cap.json`. Future standard domain capabilities should be comparable: contract-first, namespaced, target-profile-aware, compiler-visible, VM-validatable, and implemented by firmware/runtime code outside the compiler.

---

## Appendix A. Philosophy Essay

SquidScript should be small in its language and generous in its safe platform.

The device is the center of the design. It has little RAM, slow storage, a display that rewards deliberate rendering, and firmware that must remain recoverable even when user-authored apps are broken. SquidScript exists to let people extend that device without letting extension become a second firmware.

That means SquidScript should not try to be JavaScript. JavaScript-like syntax is useful because it lowers the cost of reading and writing small apps. But JavaScript behavior would bring the wrong obligations: dynamic objects, prototypes, closures, async execution, dynamic evaluation, broad libraries, and a runtime model shaped for browsers and servers rather than low-RAM e-ink firmware.

The core language should therefore remain compact. It should contain the constructs needed to describe event-driven app behavior: state, handlers, screens, functions, expressions, conditionals, and bounded iteration. Each core feature should justify itself as part of the execution model shared by every SquidScript program.

At the same time, the platform should not be austere for its own sake. A device that is meant to read BinBooks should make BinBook reading straightforward. A device that owns a display should expose drawing. A device that persists app state should expose safe state operations. Because early SquidScript has no package system, these functions belong in standard capability namespaces known to the compiler and VM.

This is the balance: the language stays small, while the platform carries first-party capabilities that lift real work.

BinBook is the model case. App authors should not parse BinBook bytes, validate indexes, decode page data, allocate page buffers, convert pixel formats, or tile output for a constrained display. The firmware should own that. But the BinBook capability should still compose with the rest of SquidScript. It should return handles, records, and display-ready resources. Final composition should still go through `service.display.draw`. The platform may be batteries included, but the batteries should have clean terminals.

The BinBook capability contract makes this extensibility concrete. It gives the compiler an interface to check without making the compiler a BinBook implementation. It gives the VM stable builtin IDs and validation rules without putting JSON metadata into `.sqbc`. It gives firmware a clear responsibility boundary: implement the native document capability according to the BinBook file-format spec and the SquidScript capability contract.

This distinction matters for future design. Adding `binbook.pageImage(page)` expands the standard platform. Adding BinBook-specific syntax expands the language. The first can be declared, profiled, validated, and implemented as a firmware module. The second changes how the language is parsed and taught. SquidScript should choose the first path unless the second is clearly necessary.

The same principle applies beyond BinBook. New features should be reviewed by what they cost the whole system, not just by whether they make one example shorter. A good feature is bounded, diagnosable, fixture-testable, target-aware, and composable. It should fail predictably. It should not require the firmware to guess. It should not hide unbounded work behind friendly syntax.

SquidScript should prefer explicit failure over surprising continuation. It should prefer handles over pointers, records over mutable objects, capability calls over global magic, and off-device compilation over on-device cleverness. It should make the safe path obvious and the unsafe path unavailable.

The result should feel practical rather than minimalistic. A small app should be easy to write. A broken app should be easy to diagnose. A firmware implementation should be able to validate bytecode before trusting it. A future spec author should be able to decide where a feature belongs without asking whether SquidScript is trying to become a general-purpose language.

SquidScript is not a sandboxed JavaScript. It is a small, compiled, capability-oriented language for first-class apps on constrained display devices.

---

## Appendix B. External Design References

These references are non-normative. They are useful background for how other
languages explain tradeoffs, omissions, and design center.

- Go's "Language Design in the Service of Software Engineering" frames a language around the real engineering environment it serves: https://go.dev/talks/2012/splash.article
- The Go FAQ explains feature omission in terms of fit, clarity, compilation speed, and system model complexity: https://go.dev/doc/faq
- Ruby's philosophy discussion emphasizes human experience, reduced frustration, and the limits of abstract perfection: https://www.artima.com/articles/the-philosophy-of-ruby
- TC39's FAQ discusses pressure, modes, and process discipline for evolving a
  widely deployed language: https://tc39wiki.calculist.org/about/faq/
- Rust's public positioning emphasizes reliability, performance, productivity, and embedded suitability: https://www.rust-lang.org/
