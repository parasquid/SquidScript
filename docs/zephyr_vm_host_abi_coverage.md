# Zephyr VM Host ABI Coverage

This note tracks current evidence for the implemented Zephyr VM host callback
ABI. It is an in-progress coverage map, not a replacement for `ROADMAP.md`.

## Scope

The ABI under test is the `SqvmCallbacks` boundary used by Zephyr C firmware to
host the Rust VM through `squidvm-ffi`. Coverage should prove that implemented
callbacks have Rust FFI tests and Zephyr ztests for success, boundary,
unsupported, and error/status behavior where those states apply.

## Current Evidence

- Rust FFI dispatch tests cover success paths for debug output, display draw
  log, indicator, GPIO, app lifecycle, app registry, process and armed stacks,
  timers, Wi-Fi records, device configuration records, content result records,
  system resource strings, state storage, and resumable SQBC reads.
- Rust FFI dispatch tests cover callback error conversion to
  `SqvmStatus::VmError` for all status-returning callback families that are
  reachable through current SQBC fixtures.
- Rust FFI dispatch tests cover absent required callbacks as VM errors, and
  absent optional service callbacks as honest unsupported records for Wi-Fi
  actions, device configuration, and content APIs.
- Rust FFI dispatch tests cover optional no-op callbacks as intentionally
  non-failing when absent, including debug output and passive display draw
  sinks.
- Zephyr protocol ztests cover status-label and errno mapping for
  `SQVM_STATUS_INVALID_ARGUMENT` and `SQVM_STATUS_VM_ERROR`.
- Zephyr ztests cover runtime callback boundary errors for indicator, GPIO, and
  timer helper paths, including invalid arguments and timer-capacity exhaustion.
- Zephyr ztests cover VM-dispatch success and error behavior for system
  resource callbacks, including `system.memory()` and
  `system.storage("apps")`.
- Zephyr ztests cover VM-dispatch success and error behavior for app registry
  callbacks, including `app.registry()` and `app.registry.get(...)`.
- Zephyr ztests cover VM-dispatch success behavior for process stack and armed
  stack callbacks, including `app.processStack()`, `app.armedStack()`, and
  `app.armedStack.get(...)`.
- Zephyr ztests cover VM-dispatch success behavior for display draw-log
  callbacks, including `service.display.clear`, `text`, `rect`, `line`,
  `select`, `image`, and `draw`; protocol ztests also cover draw-log response
  serialization for records already captured by the runtime.
- Zephyr ztests cover unsupported content result records and Wi-Fi action
  stubs through VM dispatch.
- Zephyr ztests cover storage adapter behavior for SQBC reads and state
  load/save/reset requests, including buffer-capacity errors.
- Zephyr ztests cover device configuration load, set, rebind, save, packaged
  resources, inline GPIO bindings, and unsupported GPIO binding records.

## Current Interpretation

- Required status-returning callbacks are expected to surface callback failures
  as VM errors.
- Optional service callbacks are expected to return unsupported result records
  when their API has a result shape that can represent unsupported behavior.
- Optional no-op callbacks are expected to preserve dispatch success when
  absent.
- Void draw-log callbacks have no callback-level status/error channel; Zephyr
  ztests cover their success behavior through VM dispatch and draw-log storage.
- No current gap is known for the implemented Zephyr VM host callback set. When
  new SQBC builtins or host callbacks are added, add Rust FFI equivalence tests
  and Zephyr ztests in the same slice.
- Future callbacks should keep the same caller-owned-buffer pattern used by
  `system.memory()` and `system.storage("apps")` unless a documented API
  requirement makes another ownership model necessary.
