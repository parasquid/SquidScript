# Zephyr VM Host ABI Coverage

This note tracks current evidence for the implemented Zephyr VM host callback
ABI. It is an in-progress coverage map, not a replacement for `ROADMAP.md`.

## Scope

The ABI under test is the `SqvmCallbacks` boundary used by Zephyr C firmware to
host the Rust VM through `squidvm-ffi`. Coverage should prove that implemented
callbacks have Rust FFI tests and Zephyr ztests for success, boundary,
unsupported, and error/status behavior where those states apply.

## Service Interface Rule

Future SquidScript service interfaces should move as complete host contracts,
not as firmware-only conveniences. For a new VM-visible service callback, add
or update the source API contract, compiler lowering, SQBC builtin ID, Rust VM
callback shape, C FFI boundary, Zephyr runtime wiring, target capability
metadata when applicable, diagnostics, Rust FFI equivalence tests, and Zephyr
ztests in the same slice. Interface-declaration systems such as PikaPython's
C-module `.pyi` files are useful inspiration for keeping host bindings explicit;
SquidScript expresses that contract through its language docs, compiler tables,
SQBC ABI, FFI headers, and firmware tests rather than through Python module
stubs.

## Current Evidence

- Rust FFI dispatch tests cover success paths for debug output, display draw
  log, indicator, GPIO, app lifecycle, app registry, process and armed stacks,
  timers, power sleep, Wi-Fi records, device configuration records, content
  result records, system resource/start-reason strings, state storage, and
  resumable SQBC reads.
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
  `system.storage("apps")`; Rust FFI tests cover `system.startReason()`.
- Zephyr ztests cover VM-dispatch success and error behavior for app registry
  callbacks, including `app.registry()` and `app.registry.get(...)`.
- Zephyr ztests cover VM-dispatch success behavior for process stack and armed
  stack callbacks, including `app.processStack()`, `app.armedStack()`, and
  `app.armedStack.get(...)`.
- Zephyr ztests cover VM-dispatch success behavior for display draw-log
  callbacks, including `service.display.clear`, `text`, `rect`, `line`,
  `select`, `image`, and `draw`; protocol ztests also cover draw-log response
  serialization for records already captured by the runtime. The display info
  callback exposes the active display service descriptor for
  `service.display.info()` / `display.info()`.
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

<!-- BEGIN SQUIDVM_FFI_ABI_MANIFEST -->
## Manifest-Checked ABI Inventory

This section is generated from `compiler/rust/crates/squidvm-ffi/abi/manifest.json`.
Run `python3 scripts/check-squidvm-ffi-abi.py --write-header --write-doc --write-generated` after changing the FFI ABI.
The checker validates Rust exports, the generated Zephyr C header, generated
Rust callback/test artifacts, generated result-default helpers, runtime
callback wiring, and this generated documentation section against the
manifest.

- Exports: 58
- Callback fields: 51
- Generated result-default records: 9
- Public ABI types: 97
- Public ABI constants: 12

### Export Families

| Family | Direction | Symbols |
| --- | --- | --- |
| device-bindings | rust_to_c | 2 |
| device-config | rust_to_c | 10 |
| device-protocol | rust_to_c | 25 |
| dispatch | rust_to_c | 4 |
| events | rust_to_c | 1 |
| panic | c_to_rust | 1 |
| triggers | rust_to_c | 8 |
| vm-context | rust_to_c | 7 |

### Callback Coverage Expectations

| Family | Callbacks | Rust coverage | Zephyr coverage | Evidence checks |
| --- | --- | --- | --- | ---: |
| core | trace, read_exact_at, debug_output | Rust FFI dispatch and reader tests | protocol dispatch and storage adapter ztests | 5 |
| display | display_clear, display_text, display_rect, display_line, display_select, display_image, display_draw, display_info | Rust FFI dispatch display tests | draw-log and display-info ztests | 2 |
| indicator | indicator_write, indicator_toggle, indicator_read, indicator_breathe, indicator_blink | Rust FFI dispatch indicator tests | runtime callback boundary ztests | 6 |
| hardware-gpio | hardware_gpio_write, hardware_gpio_toggle, hardware_gpio_read | Rust FFI dispatch GPIO tests | runtime callback boundary ztests | 3 |
| app-lifecycle | app_launch, app_arm, app_disarm, app_registry_list, app_registry_get, app_process_stack, app_armed_stack | Rust FFI dispatch lifecycle tests | app lifecycle and registry ztests | 6 |
| timer | timer_every, timer_after | Rust FFI dispatch timer tests | timer helper boundary ztests | 4 |
| wifi | wifi_start_ap, wifi_stop_ap, wifi_connect, wifi_disconnect, wifi_get_ap_ip, wifi_status, wifi_scan, wifi_operation, wifi_result, wifi_cancel, wifi_scan_network | Rust FFI dispatch Wi-Fi tests | unsupported/action stub ztests | 5 |
| device-config | device_config_load, device_config_set, device_config_rebind, device_config_save | Rust FFI dispatch device-config tests | device configuration ztests | 5 |
| file | file_pick_file, file_read_text, file_read_lines | Rust FFI dispatch file result tests | unsupported content/file result ztests | 5 |
| system | system_memory_text, system_storage_text, system_start_reason_text | Rust FFI dispatch system text tests | system resource/start reason ztests | 3 |
| power | power_sleep | Rust FFI dispatch power tests | power sleep dispatch ztests | 2 |
<!-- END SQUIDVM_FFI_ABI_MANIFEST -->
