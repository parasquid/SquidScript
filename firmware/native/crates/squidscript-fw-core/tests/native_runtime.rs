use squidc_core::compile::{compile, CompileRequest};
use squidscript_fw_core::{
    app_store::{AppStoreError, NativeAppStorage},
    native_runtime::{
        BoundedNativeFileBackend, NativeBinBookBackend, NativeDisplaySink, NativeFileBackend,
        NativeFileStorage, NativeFileStorageError, NativeRadioBackend, NativeRuntime,
        NativeRuntimeError, NativeUploadRouteError, NativeUploadTransport, NativeWifiApIp,
        NativeWifiBackendOperation, NativeWifiStatus, NoopBinBookBackend, NoopFileBackend,
        NoopRadioBackend,
    },
    radio_lifecycle::RadioKind,
};
use squidvm_core::{
    host::{
        BinBookChapterEntry, BinBookChapterListSummary, BinBookChapterListWriter,
        BinBookChapterResult, BinBookInfoResult, BinBookOpenResult, BinBookReadPageResult,
        ContentBinBookEntry, ContentBinBookListResult, FileReadLinesResult, FileReadTextResult,
        WifiAccessPoint,
    },
    value::{Handle, HandleKind},
};
use std::collections::HashMap;
use std::vec::Vec;

fn compile_sqbc(source: &str) -> Vec<u8> {
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap()
}

fn run_temp_app<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    F: NativeFileBackend,
    A: NativeAppStorage,
>(
    runtime: &mut NativeRuntime<B, D, C, F, A>,
    app_id: &str,
    sqbc: &[u8],
) {
    runtime.begin_temp_run(app_id, sqbc.len()).unwrap();
    runtime.write_temp_run_chunk(0, sqbc).unwrap();
    runtime.commit_temp_run().unwrap();
}

fn install_app<
    B: NativeRadioBackend,
    D: NativeDisplaySink,
    C: NativeBinBookBackend,
    F: NativeFileBackend,
    A: NativeAppStorage,
>(
    runtime: &mut NativeRuntime<B, D, C, F, A>,
    app_id: &str,
    sqbc: &[u8],
) {
    runtime.begin_app_install(app_id, sqbc.len()).unwrap();
    runtime.write_app_install_chunk(0, sqbc).unwrap();
    runtime.commit_app_install().unwrap();
}

#[derive(Default)]
struct MultiAppStorage {
    apps: HashMap<String, Vec<u8>>,
    pending_id: String,
    pending: Vec<u8>,
    states: HashMap<String, Vec<u8>>,
}

impl NativeAppStorage for MultiAppStorage {
    fn for_each_app(&mut self, visit: &mut dyn FnMut(&str, usize)) -> Result<(), AppStoreError> {
        for (id, bytes) in &self.apps {
            visit(id, bytes.len());
        }
        Ok(())
    }
    fn app_size(&mut self, app_id: &str) -> Result<usize, AppStoreError> {
        self.apps
            .get(app_id)
            .map(Vec::len)
            .ok_or(AppStoreError::NotFound)
    }
    fn read_app_at(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), AppStoreError> {
        read_test_bytes(&self.apps, app_id, offset, out)
    }
    fn begin_app_install(&mut self, app_id: &str, _: usize) -> Result<(), AppStoreError> {
        self.pending_id = app_id.into();
        self.pending.clear();
        Ok(())
    }
    fn write_app_install_chunk(
        &mut self,
        app_id: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), AppStoreError> {
        if self.pending_id != app_id || self.pending.len() != offset {
            return Err(AppStoreError::OutOfOrder);
        }
        self.pending.extend_from_slice(bytes);
        Ok(())
    }
    fn read_app_install_at(
        &mut self,
        app_id: &str,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), AppStoreError> {
        if self.pending_id != app_id {
            return Err(AppStoreError::NotFound);
        }
        out.copy_from_slice(
            self.pending
                .get(offset..offset + out.len())
                .ok_or(AppStoreError::TooLarge)?,
        );
        Ok(())
    }
    fn publish_app_install(&mut self, app_id: &str) -> Result<(), AppStoreError> {
        if self.pending_id != app_id {
            return Err(AppStoreError::NotFound);
        }
        self.apps.insert(app_id.into(), self.pending.clone());
        self.pending_id.clear();
        self.pending.clear();
        Ok(())
    }
    fn abort_app_install(&mut self, _: &str) -> Result<(), AppStoreError> {
        self.pending_id.clear();
        self.pending.clear();
        Ok(())
    }
    fn begin_resource_install(&mut self, _: &str, _: &str, _: usize) -> Result<(), AppStoreError> {
        Err(AppStoreError::Io)
    }
    fn write_resource_install_chunk(
        &mut self,
        _: &str,
        _: &str,
        _: usize,
        _: &[u8],
    ) -> Result<(), AppStoreError> {
        Err(AppStoreError::Io)
    }
    fn publish_resource_install(&mut self, _: &str, _: &str) -> Result<(), AppStoreError> {
        Err(AppStoreError::Io)
    }
    fn read_resource_at(
        &mut self,
        _: &str,
        _: &str,
        _: usize,
        _: &mut [u8],
    ) -> Result<(), AppStoreError> {
        Err(AppStoreError::NotFound)
    }
    fn resource_size(&mut self, _: &str, _: &str) -> Result<usize, AppStoreError> {
        Err(AppStoreError::NotFound)
    }
    fn load_state(&mut self, app_id: &str, out: &mut [u8]) -> Result<Option<usize>, AppStoreError> {
        let Some(bytes) = self.states.get(app_id) else {
            return Ok(None);
        };
        out[..bytes.len()].copy_from_slice(bytes);
        Ok(Some(bytes.len()))
    }
    fn save_state_atomic(&mut self, app_id: &str, bytes: &[u8]) -> Result<(), AppStoreError> {
        self.states.insert(app_id.into(), bytes.into());
        Ok(())
    }
    fn delete_state(&mut self, app_id: &str) -> Result<(), AppStoreError> {
        self.states.remove(app_id);
        Ok(())
    }
    fn format(&mut self) -> Result<(), AppStoreError> {
        self.apps.clear();
        self.pending.clear();
        self.states.clear();
        Ok(())
    }
    fn capacity(&mut self) -> Result<(usize, usize), AppStoreError> {
        Ok((1024 * 1024, 1024 * 1024))
    }
}

fn read_test_bytes(
    map: &HashMap<String, Vec<u8>>,
    key: &str,
    offset: usize,
    out: &mut [u8],
) -> Result<(), AppStoreError> {
    out.copy_from_slice(
        map.get(key)
            .and_then(|bytes| bytes.get(offset..offset + out.len()))
            .ok_or(AppStoreError::NotFound)?,
    );
    Ok(())
}

fn multi_app_runtime() -> NativeRuntime<
    NoopRadioBackend,
    CountingDisplaySink,
    NoopBinBookBackend,
    NoopFileBackend,
    MultiAppStorage,
> {
    NativeRuntime::with_radio_display_binbook_file_and_app_store(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        NoopFileBackend,
        MultiAppStorage::default(),
    )
}

#[test]
fn temp_run_dispatches_app_start_and_records_diagnostics() {
    let sqbc = compile_sqbc(
        r#"app "native-temp"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  debug.print("native", state.count)
  state.save()
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-temp", &sqbc);

    let output = runtime.output_lines();
    assert_eq!(output.as_slice(), &["native 1"]);
    let trace = runtime.trace_lines();
    assert_eq!(trace.as_slice(), &["app.start", "state.load", "state.save"]);
    assert!(!runtime.state_bytes().is_empty());
    assert_eq!(runtime.active_app(), Some("native-temp"));
    assert_eq!(runtime.lifecycle_phase(), "idle");
    assert_eq!(runtime.app_storage_write_calls(), 0);
}

#[test]
fn reset_clears_temp_app_and_diagnostics() {
    let sqbc = compile_sqbc(
        r#"app "native-temp"
event.on("app.start") { debug.print("hello") }
"#,
    );
    let mut runtime = NativeRuntime::new();
    run_temp_app(&mut runtime, "native-temp", &sqbc);

    runtime.reset();

    assert_eq!(runtime.active_app(), None);
    assert!(runtime.output_lines().as_slice().is_empty());
    assert!(runtime.trace_lines().as_slice().is_empty());
    assert!(runtime.state_bytes().is_empty());
}

#[test]
fn installed_app_launches_from_dedicated_slot() {
    let sqbc = compile_sqbc(
        r#"app "installed"
event.on("app.start") { debug.print("installed start") }
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed", &sqbc);
    runtime.launch_app("installed").unwrap();

    assert_eq!(runtime.output_lines().as_slice(), &["installed start"]);
    assert_eq!(runtime.active_app(), Some("installed"));
    assert_eq!(runtime.installed_app(), Some(("installed", sqbc.len())));
}

#[test]
fn installed_lifecycle_launches_exits_and_returns_fresh() {
    let main = compile_sqbc(
        r#"app "main"
event.on("app.start") { debug.print("main-start") }
event.on("app.exit") { debug.print("main-exit") }
"#,
    );
    let reader = compile_sqbc(
        r#"app "reader"
event.on("app.start") { debug.print("reader-start", system.startReason()) }
event.on("key.BACK") { app.exit() }
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "main", &main);
    install_app(&mut runtime, "reader", &reader);

    runtime.boot_app("main").unwrap();
    runtime.launch_app("reader").unwrap();
    assert_eq!(runtime.active_app(), Some("reader"));
    assert_eq!(runtime.lifecycle_process_at(0), Some("main"));
    assert_eq!(runtime.lifecycle_start_reason(), "launch");
    assert_eq!(runtime.output_lines().as_slice(), &["reader-start launch"]);

    runtime.dispatch_event("key.BACK").unwrap();
    assert_eq!(runtime.active_app(), Some("main"));
    assert_eq!(runtime.lifecycle_start_reason(), "return");
    assert_eq!(
        runtime.output_lines().as_slice().last(),
        Some(&"main-start")
    );
}

#[test]
fn installed_lifecycle_rejects_missing_target_and_return_stack_overflow() {
    let mut runtime = multi_app_runtime();
    for id in ["main", "one", "two", "three"] {
        install_app(
            &mut runtime,
            id,
            &compile_sqbc(&format!("app \"{id}\"\nevent.on(\"app.start\") {{}}\n")),
        );
    }
    runtime.boot_app("main").unwrap();
    assert_eq!(
        runtime.launch_app("missing"),
        Err(NativeRuntimeError::AppNotInstalled)
    );
    runtime.launch_app("one").unwrap();
    runtime.launch_app("two").unwrap();
    assert_eq!(
        runtime.launch_app("three"),
        Err(NativeRuntimeError::TooLarge)
    );
    assert_eq!(runtime.active_app(), Some("two"));
    assert_eq!(runtime.lifecycle_phase(), "idle");
}

#[test]
fn installed_sleep_request_defers_cleanup_and_builds_checkpoint() {
    let sqbc = compile_sqbc(
        r#"app "sleeper"
event.on("app.start") {}
event.on("key.POWER") {
  debug.print("request")
  service.power.sleep({ wakeAfterMs: 3000 })
  debug.print("returned")
}
event.on("power.sleep") { debug.print("cleanup") }
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "sleeper", &sqbc);
    runtime.boot_app("sleeper").unwrap();

    runtime.dispatch_event("key.POWER").unwrap();

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["request", "returned", "cleanup"]
    );
    let checkpoint = runtime.take_prepared_sleep_checkpoint().unwrap().unwrap();
    assert_eq!(checkpoint.active_app.as_str(), "sleeper");
    assert_eq!(checkpoint.wake_after_ms, 3_000);
    assert!(runtime.take_prepared_sleep_checkpoint().unwrap().is_none());
}

#[test]
fn temp_app_cannot_request_planned_sleep() {
    let sqbc = compile_sqbc(
        r#"app "temp-sleeper"
event.on("app.start") { service.power.sleep({ wakeAfterMs: 3000 }) }
"#,
    );
    let mut runtime = NativeRuntime::new();
    runtime.begin_temp_run("temp-sleeper", sqbc.len()).unwrap();
    runtime.write_temp_run_chunk(0, &sqbc).unwrap();

    assert_eq!(
        runtime.commit_temp_run(),
        Err(NativeRuntimeError::Vm(
            squidvm_core::error::VmError::InvalidOperand
        ))
    );
    assert!(runtime.take_prepared_sleep_checkpoint().unwrap().is_none());
}

#[test]
fn failed_sleep_cleanup_aborts_prepared_checkpoint() {
    let sqbc = compile_sqbc(
        r#"app "bad-cleanup"
event.on("app.start") {}
event.on("key.POWER") { service.power.sleep({ wakeAfterMs: 3000 }) }
event.on("power.sleep") { app.launch("missing") }
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "bad-cleanup", &sqbc);
    runtime.boot_app("bad-cleanup").unwrap();

    assert_eq!(
        runtime.dispatch_event("key.POWER"),
        Err(NativeRuntimeError::AppNotInstalled)
    );
    assert!(runtime.take_prepared_sleep_checkpoint().unwrap().is_none());
}

#[test]
fn sleep_checkpoint_restores_wake_reason_return_stack_and_armed_apps() {
    let main = compile_sqbc(
        r#"app "main"
event.on("app.start") { app.arm("wake-helper") }
"#,
    );
    let helper = compile_sqbc(
        r#"app "wake-helper"
app.triggers { service.input.on("key.POWER.longTap") }
event.on("key.POWER.longTap") {}
"#,
    );
    let reader = compile_sqbc(
        r#"app "reader"
event.on("app.start") { debug.print("reader", system.startReason()) }
event.on("key.POWER") { service.power.sleep({ wakeAfterMs: 0 }) }
event.on("power.sleep") { state.save() }
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "main", &main);
    install_app(&mut runtime, "wake-helper", &helper);
    install_app(&mut runtime, "reader", &reader);
    runtime.boot_app("main").unwrap();
    runtime.launch_app("reader").unwrap();
    runtime.dispatch_event("key.POWER").unwrap();
    let checkpoint = runtime.take_prepared_sleep_checkpoint().unwrap().unwrap();

    runtime.reset();
    runtime.rebuild_app_registry().unwrap();
    runtime.restore_power_checkpoint(&checkpoint).unwrap();

    assert_eq!(runtime.active_app(), Some("reader"));
    assert_eq!(runtime.lifecycle_start_reason(), "wake");
    assert_eq!(runtime.lifecycle_process_at(0), Some("main"));
    assert_eq!(
        runtime.lifecycle_armed_at(0),
        Some(("wake-helper", "key.POWER.longTap"))
    );
    assert_eq!(runtime.output_lines().as_slice(), &["reader wake"]);
}

#[test]
fn armed_input_launches_owner_and_unmatched_input_stays_foreground() {
    let root = compile_sqbc(
        r#"app "main"
event.on("app.start") { app.arm("armed-input") }
event.on("key.UP") { debug.print("root-up") }
"#,
    );
    let armed = compile_sqbc(
        r#"app "armed-input"
app.triggers { service.input.on("key.POWER") }
event.on("app.start") { debug.print("armed-start") }
event.on("key.POWER") { debug.print("armed-power") }
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "armed-input", &armed);
    install_app(&mut runtime, "main", &root);
    runtime.boot_app("main").unwrap();

    runtime.enqueue_input_event("key.UP").unwrap();
    assert_eq!(runtime.active_app(), Some("main"));
    assert_eq!(runtime.output_lines().as_slice(), &["root-up"]);

    runtime.enqueue_input_event("key.POWER").unwrap();
    assert_eq!(runtime.active_app(), Some("armed-input"));
    assert_eq!(
        runtime.output_lines().as_slice(),
        &["armed-start", "armed-power"]
    );
    assert_eq!(runtime.lifecycle_process_at(0), Some("main"));
}

#[test]
fn armed_timer_launches_owner_with_exact_declared_event() {
    let root = compile_sqbc(
        r#"app "main"
event.on("app.start") { app.arm("armed-timer") }
"#,
    );
    let armed = compile_sqbc(
        r#"app "armed-timer"
app.triggers { service.timer.after("timer.due", 10) }
event.on("app.start") { debug.print("timer-start") }
event.on("timer.due") { debug.print("timer-due") }
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "armed-timer", &armed);
    install_app(&mut runtime, "main", &root);
    runtime.boot_app("main").unwrap();

    runtime.tick_timers(10).unwrap();

    assert_eq!(runtime.active_app(), Some("armed-timer"));
    assert_eq!(
        runtime.output_lines().as_slice(),
        &["timer-start", "timer-due"]
    );
}

#[test]
fn duplicate_armed_input_keeps_original_owner_and_records_error() {
    let root = compile_sqbc(
        r#"app "main"
event.on("app.start") {
  app.arm("first-owner")
  app.arm("second-owner")
}
"#,
    );
    let first = compile_sqbc(
        r#"app "first-owner"
app.triggers { service.input.on("key.POWER") }
event.on("key.POWER") { debug.print("first") }
"#,
    );
    let second = compile_sqbc(
        r#"app "second-owner"
app.triggers { service.input.on("key.POWER") }
event.on("key.POWER") { debug.print("second") }
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "first-owner", &first);
    install_app(&mut runtime, "second-owner", &second);
    install_app(&mut runtime, "main", &root);
    assert!(runtime.boot_app("main").is_err());
    assert!(runtime
        .error_lines()
        .as_slice()
        .contains(&"armed_input_owner_conflict"));

    runtime.enqueue_input_event("key.POWER").unwrap();
    assert_eq!(runtime.active_app(), Some("first-owner"));
    assert_eq!(runtime.output_lines().as_slice(), &["first"]);
}

#[test]
fn replacing_app_removes_its_armed_routes() {
    let root = compile_sqbc(
        r#"app "main"
event.on("app.start") { app.arm("replace-owner") }
"#,
    );
    let armed = compile_sqbc(
        r#"app "replace-owner"
app.triggers { service.input.on("key.POWER") }
event.on("key.POWER") {}
"#,
    );
    let replacement = compile_sqbc(
        r#"app "replace-owner"
event.on("app.start") {}
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "replace-owner", &armed);
    install_app(&mut runtime, "main", &root);
    runtime.boot_app("main").unwrap();
    assert_eq!(runtime.lifecycle_armed_len(), 1);

    install_app(&mut runtime, "replace-owner", &replacement);

    assert_eq!(runtime.lifecycle_armed_len(), 0);
}

#[test]
fn squidscript_can_inspect_registry_process_and_armed_stacks() {
    let root = compile_sqbc(
        r#"app "main"
event.on("app.start") { app.arm("armed-view") }
"#,
    );
    let armed = compile_sqbc(
        r#"app "armed-view"
app.triggers { service.input.on("key.POWER") }
event.on("app.start") {
  let apps = app.registry()
  let selected = app.registry.get(apps, 1)
  debug.print("registry", selected.id)
  let process = app.processStack()
  for appId in process max 2 { debug.print("process", appId) }
  let armedApps = app.armedStack()
  for entry in armedApps max 8 { debug.print("armed", entry.appId, entry.event) }
}
event.on("key.POWER") {}
"#,
    );
    let mut runtime = multi_app_runtime();
    install_app(&mut runtime, "armed-view", &armed);
    install_app(&mut runtime, "main", &root);
    runtime.boot_app("main").unwrap();

    runtime.enqueue_input_event("key.POWER").unwrap();

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "registry main",
            "process main",
            "armed armed-view key.POWER"
        ]
    );
}

#[test]
fn fallback_runs_from_read_only_sqbc_without_entering_registry_or_persisting_state() {
    let sqbc = compile_sqbc(
        r#"app "main"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  state.save()
  debug.print("count", state.count)
  debug.print(system.memory())
  debug.print(system.storage("apps"))
}
"#,
    );
    let fallback = Box::leak(sqbc.into_boxed_slice());
    let mut runtime = NativeRuntime::new();
    runtime.set_system_memory_metrics(400 * 1024, 1024, 2048);

    runtime.launch_fallback(fallback).unwrap();

    assert_eq!(runtime.active_app(), Some("main"));
    assert!(runtime.app_registry().iter().all(Option::is_none));
    assert_eq!(runtime.installed_app(), None);
    assert!(!runtime.state_bytes().is_empty());
    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "count 1",
            "RAM 400 KiB heap 1024 used 2048 free",
            "Apps 8 KiB"
        ]
    );

    runtime.reset();
    runtime.launch_fallback(fallback).unwrap();
    assert!(runtime.app_registry().iter().all(Option::is_none));
    assert_eq!(runtime.output_lines().as_slice()[0], "count 1");
}

#[test]
fn installed_app_state_survives_reset_and_relaunch() {
    let sqbc = compile_sqbc(
        r#"app "installed-state"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  debug.print("count", state.count)
  state.save()
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed-state", &sqbc);
    runtime.launch_app("installed-state").unwrap();
    assert_eq!(runtime.output_lines().as_slice(), &["count 1"]);

    runtime.reset();
    runtime.launch_app("installed-state").unwrap();

    assert_eq!(runtime.output_lines().as_slice(), &["count 2"]);
    assert_eq!(runtime.active_app(), Some("installed-state"));
}

#[test]
fn installed_app_dispatches_key_and_named_events() {
    let sqbc = compile_sqbc(
        r#"app "installed-events"
event.on("app.start") { debug.print("start") }
event.on("key.SELECT") { debug.print("select") }
event.on("repl") { debug.print("repl") }
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed-events", &sqbc);
    runtime.launch_app("installed-events").unwrap();
    runtime.dispatch_event("key.SELECT").unwrap();
    runtime
        .dispatch_app_event("installed-events", "repl")
        .unwrap();

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["start", "select", "repl"]
    );
}

#[test]
fn imported_state_is_visible_to_next_installed_launch() {
    let sqbc = compile_sqbc(
        r#"app "installed-import"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  debug.print("count", state.count)
  state.save()
}
"#,
    );
    let mut source_runtime = NativeRuntime::new();
    install_app(&mut source_runtime, "installed-import", &sqbc);
    source_runtime.launch_app("installed-import").unwrap();
    let saved = source_runtime.state_bytes().to_vec();

    let mut runtime = NativeRuntime::new();
    install_app(&mut runtime, "installed-import", &sqbc);
    runtime.import_state(&saved).unwrap();
    runtime.launch_app("installed-import").unwrap();

    assert_eq!(runtime.output_lines().as_slice(), &["count 2"]);
}

#[test]
fn fresh_runtime_reports_inactive_lifecycle() {
    let runtime = NativeRuntime::new();

    assert_eq!(runtime.active_app(), None);
    assert_eq!(runtime.lifecycle_process_len(), 0);
    assert_eq!(runtime.lifecycle_armed_len(), 0);
    assert_eq!(runtime.lifecycle_phase(), "idle");
    assert_eq!(runtime.lifecycle_start_reason(), "boot");
}

#[test]
fn runtime_retains_bounded_errors_until_explicitly_cleared() {
    let mut runtime = NativeRuntime::new();
    runtime.record_error("storage: io-error");
    runtime.record_error("transfer: invalid-offset");

    assert_eq!(
        runtime.error_lines().iter().collect::<Vec<_>>(),
        vec!["storage: io-error", "transfer: invalid-offset"]
    );

    runtime.clear_errors();
    assert_eq!(runtime.error_lines().iter().count(), 0);
}

#[test]
fn runtime_diagnostics_do_not_pollute_retained_errors() {
    let mut runtime = NativeRuntime::new();
    runtime.record_trace("diag.content-check.start");

    assert_eq!(
        runtime.trace_lines().iter().collect::<Vec<_>>(),
        vec!["diag.content-check.start"]
    );
    assert_eq!(runtime.error_lines().iter().count(), 0);
}

#[test]
fn temp_run_rejects_oversize_payloads() {
    let mut runtime = NativeRuntime::new();

    let error = runtime
        .begin_temp_run(
            "too-large",
            squidscript_fw_core::native_runtime::MAX_TEMP_SQBC_BYTES + 1,
        )
        .unwrap_err();

    assert_eq!(error, NativeRuntimeError::TooLarge);
}

#[test]
fn resources_report_vm_and_temp_run_state() {
    let sqbc = compile_sqbc(
        r#"app "native-temp"
event.on("app.start") { debug.print("hello") }
"#,
    );
    let mut runtime = NativeRuntime::new();
    run_temp_app(&mut runtime, "native-temp", &sqbc);

    let resources = runtime.resource_metrics();

    assert!(resources
        .iter()
        .any(|metric| metric.key == "runtime_current_app_present" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "vm_sqbc_chunk_bytes" && metric.value > 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "runtime_static_bytes" && metric.value > 0));
}

#[test]
fn screen_open_records_native_display_drawlog() {
    let sqbc = compile_sqbc(
        r#"app "native-display"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
  service.display.text("hello", {
    x: 4
    y: 8
    w: 120
    h: 24
    fontHeight: 16
    textColor: color.BLACK
    backgroundColor: color.WHITE
  })
  service.display.rect(1, 2, 30, 40, {
    fillColor: color.GRAY8
    strokeColor: color.BLACK
  })
  service.display.line(5, 6, 7, 8, {
    color: color.GRAY4
  })
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-display", &sqbc);

    assert_eq!(
        runtime.drawlog_lines().as_slice(),
        &[
            "clear 0",
            "text hello x=4 y=8 w=120 h=24 font=16 fg=15 bg=0",
            "rect x=1 y=2 w=30 h=40 fill=8 stroke=15",
            "line x1=5 y1=6 x2=7 y2=8 color=4",
        ]
    );
}

#[test]
fn display_info_reports_native_x4_display_profile() {
    let sqbc = compile_sqbc(
        r#"app "native-display-info"
event.on("app.start") {
  let info = display.info()
  debug.print(info.ok, info.available, info.status, info.driver, info.width, info.height)
  debug.print(info.physicalWidth, info.physicalHeight, info.rotation, info.nativePixelFormat)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-display-info", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "true true ready xteink-x4-display 480 800",
            "800 480 270 GRAY2_PACKED",
        ]
    );
}

#[test]
fn display_resource_operations_record_native_drawlog() {
    let sqbc = compile_sqbc(
        r#"app "native-display-resources"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.select("status")
  service.display.image("data/icon.bmp", {
    x: 20
    y: 24
  })
  service.display.draw("drawable/page", {
    x: 0
    y: 0
  })
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-display-resources", &sqbc);

    assert_eq!(
        runtime.drawlog_lines().as_slice(),
        &[
            "select status",
            "image data/icon.bmp x=20 y=24 w=0 h=0",
            "draw drawable/page x=0 y=0 w=0 h=0",
        ]
    );
}

#[derive(Default)]
struct FakeFileBackend {
    read_text_calls: usize,
    read_lines_calls: usize,
}

const TEST_NOTE_LINES: [&str; 2] = ["alpha", "beta"];

impl NativeFileBackend for FakeFileBackend {
    fn file_read_text<'a>(
        &'a mut self,
        path: &str,
    ) -> Result<FileReadTextResult<'a>, squidvm_core::error::VmError> {
        self.read_text_calls += 1;
        assert_eq!(path, "notes/status.txt");
        Ok(FileReadTextResult {
            ok: true,
            error: None,
            text: Some("ready"),
        })
    }

    fn file_read_lines<'a>(
        &'a mut self,
        path: &str,
        max_lines: i32,
    ) -> Result<FileReadLinesResult<'a>, squidvm_core::error::VmError> {
        self.read_lines_calls += 1;
        assert_eq!(path, "notes/list.txt");
        assert_eq!(max_lines, 2);
        Ok(FileReadLinesResult {
            ok: true,
            error: None,
            lines: &TEST_NOTE_LINES,
        })
    }
}

#[test]
fn native_file_backend_drives_file_read_text_and_lines() {
    let sqbc = compile_sqbc(
        r#"app "native-file"
event.on("app.start") {
  let text = file.readText("notes/status.txt")
  debug.print("text", text.ok, text.text)
  let lines = file.readLines("notes/list.txt", 2)
  debug.print("lines", lines.ok)
  for line in lines.lines max 2 {
    debug.print("line", line)
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FakeFileBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["text true ready", "lines true", "line alpha", "line beta",]
    );
    let backend = runtime.file_backend();
    assert_eq!(backend.read_text_calls, 1);
    assert_eq!(backend.read_lines_calls, 1);
}

#[derive(Default)]
struct StaticFileStorage {
    reads: usize,
    copied: Vec<u8>,
    published: Vec<u8>,
    published_path: Option<String>,
    tmp: Vec<u8>,
    tmp_path: Option<String>,
    deleted: Vec<String>,
    formatted: bool,
}

impl NativeFileStorage for StaticFileStorage {
    fn for_each_file(
        &mut self,
        visit: &mut dyn FnMut(&str, u64),
    ) -> Result<(), NativeFileStorageError> {
        visit("books/readme.binbook", 4096);
        visit("notes/status.txt", 5);
        visit("notes/list.txt", 17);
        Ok(())
    }

    fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError> {
        match path {
            "notes/status.txt" => Ok(5),
            "notes/list.txt" => Ok(17),
            "books/copied.txt" if !self.copied.is_empty() => Ok(self.copied.len() as u64),
            path if Some(path) == self.published_path.as_deref() => Ok(self.published.len() as u64),
            path if Some(path) == self.tmp_path.as_deref() => Ok(self.tmp.len() as u64),
            _ => Err(NativeFileStorageError::NotFound),
        }
    }

    fn read_at(
        &mut self,
        path: &str,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), NativeFileStorageError> {
        self.reads += 1;
        let source: &[u8] = match path {
            "notes/status.txt" => b"ready",
            "notes/list.txt" => b"alpha\nbeta\ngamma\n",
            "books/copied.txt" if !self.copied.is_empty() => &self.copied,
            path if Some(path) == self.published_path.as_deref() => &self.published,
            path if Some(path) == self.tmp_path.as_deref() => &self.tmp,
            _ => return Err(NativeFileStorageError::NotFound),
        };
        let offset = offset as usize;
        let available = source.len().saturating_sub(offset);
        let read_len = available.min(out.len());
        out[..read_len].copy_from_slice(&source[offset..offset + read_len]);
        for byte in out.iter_mut().skip(read_len) {
            *byte = 0;
        }
        Ok(())
    }

    fn create_or_truncate(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if path == "books/copied.txt" {
            self.copied.clear();
            return Ok(());
        }
        if path.starts_with("books/") && path.ends_with(".binbook") {
            self.published_path = Some(path.to_string());
            self.published.clear();
            return Ok(());
        }
        if path.starts_with("tmp/") {
            self.tmp_path = Some(path.to_string());
            self.tmp.clear();
            return Ok(());
        }
        {
            return Err(NativeFileStorageError::InvalidName);
        }
    }

    fn write_at(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<(), NativeFileStorageError> {
        if path == "books/copied.txt" && offset as usize == self.copied.len() {
            self.copied.extend_from_slice(data);
            return Ok(());
        }
        if Some(path) == self.published_path.as_deref() && offset as usize == self.published.len() {
            self.published.extend_from_slice(data);
            return Ok(());
        }
        if Some(path) == self.tmp_path.as_deref() && offset as usize == self.tmp.len() {
            self.tmp.extend_from_slice(data);
            return Ok(());
        }
        {
            return Err(NativeFileStorageError::InvalidName);
        }
    }

    fn flush(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if path == "books/copied.txt"
            || Some(path) == self.published_path.as_deref()
            || Some(path) == self.tmp_path.as_deref()
        {
            Ok(())
        } else {
            Err(NativeFileStorageError::InvalidName)
        }
    }

    fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if Some(path) == self.published_path.as_deref() {
            self.deleted.push(path.to_string());
            self.published_path = None;
            self.published.clear();
            return Ok(());
        }
        if Some(path) == self.tmp_path.as_deref() {
            self.deleted.push(path.to_string());
            self.tmp_path = None;
            self.tmp.clear();
            return Ok(());
        }
        Err(NativeFileStorageError::NotFound)
    }

    fn format(&mut self) -> Result<(), NativeFileStorageError> {
        self.formatted = true;
        self.copied.clear();
        self.published.clear();
        self.published_path = None;
        self.tmp.clear();
        self.tmp_path = None;
        Ok(())
    }
}

#[derive(Default)]
struct SequentialUploadOnlyStorage {
    expected_size: u64,
    bytes: Vec<u8>,
    tmp_path: Option<String>,
    committed: bool,
    calls: Vec<&'static str>,
}

impl NativeFileStorage for SequentialUploadOnlyStorage {
    fn for_each_file(
        &mut self,
        _visit: &mut dyn FnMut(&str, u64),
    ) -> Result<(), NativeFileStorageError> {
        Ok(())
    }

    fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError> {
        if self.committed && Some(path) == self.tmp_path.as_deref() {
            Ok(self.bytes.len() as u64)
        } else {
            Err(NativeFileStorageError::NotFound)
        }
    }

    fn read_at(
        &mut self,
        path: &str,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), NativeFileStorageError> {
        if !self.committed || Some(path) != self.tmp_path.as_deref() {
            return Err(NativeFileStorageError::NotFound);
        }
        let offset = usize::try_from(offset).map_err(|_| NativeFileStorageError::Io)?;
        let end = offset
            .checked_add(out.len())
            .ok_or(NativeFileStorageError::Io)?;
        out.copy_from_slice(
            self.bytes
                .get(offset..end)
                .ok_or(NativeFileStorageError::Io)?,
        );
        Ok(())
    }

    fn create_or_truncate(&mut self, _path: &str) -> Result<(), NativeFileStorageError> {
        self.calls.push("legacy-create");
        Err(NativeFileStorageError::Io)
    }

    fn begin_write(
        &mut self,
        path: &str,
        expected_size: u64,
    ) -> Result<(), NativeFileStorageError> {
        self.calls.push("begin");
        self.tmp_path = Some(path.to_string());
        self.expected_size = expected_size;
        self.bytes.clear();
        self.committed = false;
        Ok(())
    }

    fn write_at(
        &mut self,
        _path: &str,
        _offset: u64,
        _data: &[u8],
    ) -> Result<(), NativeFileStorageError> {
        self.calls.push("legacy-write");
        Err(NativeFileStorageError::Io)
    }

    fn write_chunk(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<(), NativeFileStorageError> {
        self.calls.push("chunk");
        if Some(path) != self.tmp_path.as_deref() || offset as usize != self.bytes.len() {
            return Err(NativeFileStorageError::InvalidName);
        }
        self.bytes.extend_from_slice(data);
        Ok(())
    }

    fn flush(&mut self, _path: &str) -> Result<(), NativeFileStorageError> {
        self.calls.push("legacy-flush");
        Err(NativeFileStorageError::Io)
    }

    fn commit_write(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        self.calls.push("commit");
        if Some(path) != self.tmp_path.as_deref() || self.bytes.len() as u64 != self.expected_size {
            return Err(NativeFileStorageError::Io);
        }
        self.committed = true;
        Ok(())
    }

    fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if Some(path) == self.tmp_path.as_deref() {
            self.tmp_path = None;
            self.bytes.clear();
            self.committed = false;
            Ok(())
        } else {
            Err(NativeFileStorageError::NotFound)
        }
    }

    fn format(&mut self) -> Result<(), NativeFileStorageError> {
        self.tmp_path = None;
        self.bytes.clear();
        self.committed = false;
        Ok(())
    }
}

#[derive(Default)]
struct ContentTrackingFileStorage {
    files: HashMap<String, Vec<u8>>,
    reads: usize,
    file_size_calls: usize,
    deleted: Vec<String>,
}

impl NativeFileStorage for ContentTrackingFileStorage {
    fn for_each_file(
        &mut self,
        visit: &mut dyn FnMut(&str, u64),
    ) -> Result<(), NativeFileStorageError> {
        for (path, data) in &self.files {
            visit(path.as_str(), data.len() as u64);
        }
        Ok(())
    }

    fn file_size(&mut self, path: &str) -> Result<u64, NativeFileStorageError> {
        self.file_size_calls += 1;
        self.files
            .get(path)
            .map(|data| data.len() as u64)
            .ok_or(NativeFileStorageError::NotFound)
    }

    fn read_at(
        &mut self,
        path: &str,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), NativeFileStorageError> {
        self.reads += 1;
        let Some(source) = self.files.get(path) else {
            return Err(NativeFileStorageError::NotFound);
        };
        let offset = offset as usize;
        if offset > source.len() {
            return Err(NativeFileStorageError::InvalidName);
        }
        let available = source.len().saturating_sub(offset);
        let read_len = available.min(out.len());
        out[..read_len].copy_from_slice(&source[offset..offset + read_len]);
        for byte in out.iter_mut().skip(read_len) {
            *byte = 0;
        }
        Ok(())
    }

    fn create_or_truncate(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        self.files.insert(path.to_string(), Vec::new());
        Ok(())
    }

    fn write_at(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<(), NativeFileStorageError> {
        let file = self
            .files
            .get_mut(path)
            .ok_or(NativeFileStorageError::NotFound)?;
        let offset = offset as usize;
        if offset > file.len() {
            return Err(NativeFileStorageError::InvalidName);
        }
        if offset == file.len() {
            file.extend_from_slice(data);
            return Ok(());
        }
        let end = offset + data.len();
        if end > file.len() {
            return Err(NativeFileStorageError::InvalidName);
        }
        file[offset..end].copy_from_slice(data);
        Ok(())
    }

    fn flush(&mut self, _path: &str) -> Result<(), NativeFileStorageError> {
        Ok(())
    }

    fn delete(&mut self, path: &str) -> Result<(), NativeFileStorageError> {
        if self.files.remove(path).is_none() {
            return Err(NativeFileStorageError::NotFound);
        }
        self.deleted.push(path.to_string());
        Ok(())
    }

    fn format(&mut self) -> Result<(), NativeFileStorageError> {
        self.files.clear();
        Ok(())
    }
}

fn deterministic_payload(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| (index as u8).wrapping_mul(73).wrapping_add(17))
        .collect()
}

fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut hash = 0xFFFF_FFFFu32;
    for byte in bytes {
        hash ^= *byte as u32;
        for _ in 0..8 {
            let mask = hash & 1;
            hash >>= 1;
            if mask != 0 {
                hash ^= 0xEDB8_8320;
            }
        }
    }
    !hash
}

#[test]
fn bounded_native_file_backend_reads_text_and_lines_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-file"
event.on("app.start") {
  let text = file.readText("notes/status.txt")
  debug.print("text", text.ok, text.error, text.text)
  let lines = file.readLines("notes/list.txt", 2)
  debug.print("lines", lines.ok, lines.error)
  for line in lines.lines max 2 {
    debug.print("line", line)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-file", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "text true null ready",
            "lines true null",
            "line alpha",
            "line beta",
        ]
    );
    assert_eq!(runtime.file_backend().storage().reads, 2);
}

#[test]
fn bounded_native_file_backend_picks_file_by_extension_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-picker"
event.on("app.start") {
  let picked = file.pickFile(".txt")
  debug.print("pick", picked.ok, picked.error, picked.path)
  if picked.ok {
    let text = file.readText(picked.path)
    debug.print("text", text.ok, text.error, text.text)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-picker", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["pick true null notes/status.txt", "text true null ready",]
    );
}

#[test]
fn bounded_native_file_backend_copies_file_with_bounded_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-copy"
event.on("app.start") {
  let copied = file.copy("notes/status.txt", {
    library: "books"
    name: "copied.txt"
  })
  debug.print("copy", copied.ok, copied.error, copied.ref, copied.bytesWritten)
  if copied.ok {
    let text = file.readText(copied.ref)
    debug.print("text", text.ok, text.error, text.text)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-copy", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["copy true null books/copied.txt 5", "text true null ready",]
    );
}

#[test]
fn bounded_native_file_backend_lists_binbook_content_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-content"
event.on("app.start") {
  let listing = content.binbook.list("books", { offset: 0, limit: 2 })
  debug.print("list", listing.ok, listing.error, listing.count, listing.hasMore)
  for item in listing.items max 2 {
    debug.print("item", item.name, item.ref, item.size)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-content", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "list true null 1 false",
            "item readme.binbook books/readme.binbook 4096",
        ]
    );
}

#[test]
fn bounded_native_file_backend_lists_generic_library_files_from_storage() {
    let sqbc = compile_sqbc(
        r#"app "native-storage-file-list"
event.on("app.start") {
  let listing = file.list("books", { offset: 0, limit: 4 })
  debug.print("list", listing.ok, listing.error, listing.count, listing.hasMore)
  for item in listing.items max 4 {
    debug.print("item", item.name, item.ref, item.size)
  }
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-storage-file-list", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "list true null 1 false",
            "item readme.binbook books/readme.binbook 4096",
        ]
    );
}

#[test]
fn bounded_native_file_backend_publishes_content_file_with_bounded_chunks() {
    let mut file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());

    let path = file_backend
        .content_install_begin("proof.binbook", 8)
        .unwrap();
    assert_eq!(path, "books/proof.binbook");
    file_backend
        .content_install_chunk("books/proof.binbook", 0, b"ABCD")
        .unwrap();
    file_backend
        .content_install_chunk("books/proof.binbook", 4, b"EFGH")
        .unwrap();
    file_backend
        .content_install_commit("books/proof.binbook")
        .unwrap();

    assert_eq!(
        file_backend.storage().published_path.as_deref(),
        Some("books/proof.binbook")
    );
    assert_eq!(file_backend.storage().published.as_slice(), b"ABCDEFGH");
}

#[test]
fn bounded_native_file_backend_enforces_portable_ascii_content_names() {
    let mut file_backend = BoundedNativeFileBackend::<StaticFileStorage, 128, 4, 16>::new(
        StaticFileStorage::default(),
    );
    let longest_name = std::format!("{}.binbook", "a".repeat(113));
    let too_long_name = std::format!("{}.binbook", "a".repeat(114));

    let path = file_backend
        .content_install_begin(&longest_name, 1)
        .expect("121-byte ASCII filename");
    assert_eq!(path.len(), squid_device_protocol::MAX_PATH_LEN - 1);
    assert_eq!(
        file_backend.content_install_begin(&too_long_name, 1),
        Err("invalid-name")
    );
    assert_eq!(
        file_backend.content_install_begin("cafe\u{301}.binbook", 1),
        Err("invalid-name")
    );
}

#[test]
fn bounded_native_file_backend_checks_published_content_size_and_crc32() {
    let mut file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());

    let path = file_backend
        .content_install_begin("proof.binbook", 8)
        .unwrap()
        .to_string();
    file_backend
        .content_install_chunk(&path, 0, b"ABCD")
        .unwrap();
    file_backend
        .content_install_chunk(&path, 4, b"EFGH")
        .unwrap();
    file_backend.content_install_commit(&path).unwrap();

    let checked = file_backend.content_check("proof.binbook").unwrap();

    assert_eq!(checked.name, "proof.binbook");
    assert_eq!(checked.size, 8);
    assert_eq!(checked.crc32, 0x68dc_b61c);
}

#[test]
fn bounded_native_file_backend_checks_copied_content_with_uncached_readback() {
    let mut file_backend = BoundedNativeFileBackend::<ContentTrackingFileStorage, 64, 4, 16>::new(
        ContentTrackingFileStorage::default(),
    );
    let source_path = "books/source.binbook";
    let source_data = deterministic_payload(8982);
    file_backend
        .storage_mut()
        .create_or_truncate(source_path)
        .unwrap();
    file_backend
        .storage_mut()
        .write_at(source_path, 0, &source_data)
        .unwrap();
    let destination = file_backend
        .file_copy(source_path, "books", "copied.binbook")
        .unwrap();
    assert!(destination.ok);
    assert_eq!(destination.bytes_written, 8982);
    assert_eq!(
        file_backend.storage().files.get(source_path),
        Some(&source_data)
    );

    let (before_size_calls, before_reads) = {
        let storage = file_backend.storage();
        (storage.file_size_calls, storage.reads)
    };

    let (checked_name, checked_size, checked_crc) = {
        let checked = file_backend.content_check("copied.binbook").unwrap();
        (checked.name.to_string(), checked.size, checked.crc32)
    };

    let (after_size_calls, after_reads) = {
        let storage = file_backend.storage();
        (storage.file_size_calls, storage.reads)
    };

    assert_eq!(checked_name, "copied.binbook");
    assert_eq!(checked_size, 8982);
    assert_eq!(checked_crc, crc32_ieee(&source_data));
    assert_eq!(after_size_calls, before_size_calls + 1);
    assert!(after_reads > before_reads);
}

#[test]
fn bounded_native_file_backend_content_check_reads_mutated_storage_data() {
    let mut file_backend = BoundedNativeFileBackend::<ContentTrackingFileStorage, 32, 4, 16>::new(
        ContentTrackingFileStorage::default(),
    );
    let source_path = "books/source.binbook";
    let source_data = b"immutable-copy-seed".to_vec();
    file_backend
        .storage_mut()
        .create_or_truncate(source_path)
        .unwrap();
    file_backend
        .storage_mut()
        .write_at(source_path, 0, &source_data)
        .unwrap();
    file_backend
        .file_copy(source_path, "books", "copied.binbook")
        .unwrap();

    let destination_path = "books/copied.binbook";
    let before_crc = file_backend.content_check("copied.binbook").unwrap().crc32;
    assert_eq!(before_crc, crc32_ieee(&source_data));

    {
        let storage = file_backend.storage_mut();
        storage.write_at(destination_path, 0, b"X").unwrap();
    }
    let after_crc = file_backend.content_check("copied.binbook").unwrap().crc32;
    let mut mutated = source_data.clone();
    mutated[0] = b'X';

    assert_eq!(after_crc, crc32_ieee(&mutated));
}

#[test]
fn bounded_native_file_backend_deletes_published_content_by_simple_name() {
    let mut file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());

    let path = file_backend
        .content_install_begin("proof.binbook", 4)
        .unwrap()
        .to_string();
    file_backend
        .content_install_chunk(&path, 0, b"ABCD")
        .unwrap();
    file_backend.content_install_commit(&path).unwrap();

    let deleted = file_backend.content_delete("proof.binbook").unwrap();

    assert_eq!(deleted, "proof.binbook");
    assert_eq!(file_backend.storage().deleted, ["books/proof.binbook"]);
    assert_eq!(
        file_backend.content_check("proof.binbook"),
        Err("not-found")
    );
}

#[test]
fn native_upload_completion_exposes_ephemeral_tmp_file_only_during_handler() {
    let sqbc = compile_sqbc(
        r#"app "native-upload"
event.on("app.start") {}

event.on("ble.file.complete", ev) {
  let text = file.readText(ev.upload)
  debug.print("upload", ev.upload, ev.name, ev.bytesReceived, ev.totalBytes, ev.id, ev.transport)
  debug.print("text", text.ok, text.error, text.text)
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-upload", &sqbc);
    let upload_path = runtime
        .stage_ephemeral_upload(
            "unsafe/../proof.txt",
            b"ready",
            "rx",
            NativeUploadTransport::Ble,
        )
        .unwrap()
        .to_string();

    assert_eq!(upload_path, "tmp/proof.txt");
    runtime
        .dispatch_upload_complete("native-upload", "ble.file.complete", upload_path.as_str())
        .unwrap();

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "upload tmp/proof.txt proof.txt 5 5 rx ble",
            "text true null ready",
        ]
    );
    assert_eq!(runtime.file_backend().storage().tmp_path, None);
    assert_eq!(runtime.file_backend().storage().deleted, ["tmp/proof.txt"]);
}

#[test]
fn native_incremental_upload_staging_streams_chunks_into_ephemeral_tmp_file() {
    let sqbc = compile_sqbc(
        r#"app "native-upload-stream"
event.on("app.start") {}

event.on("ble.file.complete", ev) {
  let text = file.readText(ev.upload)
  debug.print("upload", ev.upload, ev.name, ev.bytesReceived, ev.totalBytes, ev.id, ev.transport)
  debug.print("text", text.ok, text.error, text.text)
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-upload-stream", &sqbc);
    let upload_path = runtime
        .begin_ephemeral_upload("unsafe/../proof.txt", 5, "rx", NativeUploadTransport::Ble)
        .unwrap()
        .to_string();

    runtime
        .write_ephemeral_upload_chunk(upload_path.as_str(), 0, b"re")
        .unwrap();
    runtime
        .write_ephemeral_upload_chunk(upload_path.as_str(), 2, b"ady")
        .unwrap();
    runtime
        .commit_ephemeral_upload(upload_path.as_str(), 5)
        .unwrap();
    runtime
        .dispatch_upload_complete(
            "native-upload-stream",
            "ble.file.complete",
            upload_path.as_str(),
        )
        .unwrap();

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "upload tmp/proof.txt proof.txt 5 5 rx ble",
            "text true null ready",
        ]
    );
    assert_eq!(runtime.file_backend().storage().deleted, ["tmp/proof.txt"]);
}

#[test]
fn native_incremental_upload_staging_uses_sequential_storage_stream() {
    let sqbc = compile_sqbc(
        r#"app "native-upload-sequential"
event.on("app.start") {}

event.on("ble.file.complete", ev) {
  let text = file.readText(ev.upload)
  debug.print("text", text.ok, text.error, text.text)
}
"#,
    );
    let file_backend = BoundedNativeFileBackend::<SequentialUploadOnlyStorage, 32, 4, 16>::new(
        SequentialUploadOnlyStorage::default(),
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-upload-sequential", &sqbc);
    let upload_path = runtime
        .begin_ephemeral_upload("unsafe/../proof.txt", 5, "rx", NativeUploadTransport::Ble)
        .unwrap()
        .to_string();
    runtime
        .write_ephemeral_upload_chunk(upload_path.as_str(), 0, b"re")
        .unwrap();
    runtime
        .write_ephemeral_upload_chunk(upload_path.as_str(), 2, b"ady")
        .unwrap();
    runtime
        .commit_ephemeral_upload(upload_path.as_str(), 5)
        .unwrap();
    runtime
        .dispatch_upload_complete(
            "native-upload-sequential",
            "ble.file.complete",
            upload_path.as_str(),
        )
        .unwrap();

    assert_eq!(runtime.output_lines().as_slice(), &["text true null ready"]);
    assert_eq!(
        runtime.file_backend().storage().calls,
        ["begin", "chunk", "chunk", "commit"]
    );
}

#[test]
fn native_upload_completion_can_install_and_defer_launch_from_tmp_file_ref() {
    let receiver_sqbc = compile_sqbc(
        r#"app "native-upload-installer"
event.on("app.start") {}

event.on("ble.file.complete", ev) {
  let installed = app.install(ev.upload)
  app.launch(installed.id)
}
"#,
    );
    let uploaded_sqbc = compile_sqbc(
        r#"app "uploaded-from-ble"
event.on("app.start") {
  debug.print("uploaded started")
}
"#,
    );
    let file_backend = BoundedNativeFileBackend::<StaticFileStorage, 128, 4, 32>::new(
        StaticFileStorage::default(),
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-upload-installer", &receiver_sqbc);
    let upload_path = runtime
        .stage_ephemeral_upload(
            "uploaded.sqbc",
            &uploaded_sqbc,
            "rx",
            NativeUploadTransport::Ble,
        )
        .unwrap()
        .to_string();

    runtime
        .dispatch_upload_complete(
            "native-upload-installer",
            "ble.file.complete",
            upload_path.as_str(),
        )
        .unwrap();

    assert_eq!(runtime.active_app(), Some("uploaded-from-ble"));
    assert_eq!(
        runtime.installed_app(),
        Some(("uploaded-from-ble", uploaded_sqbc.len()))
    );
    assert_eq!(runtime.output_lines().as_slice(), &["uploaded started"]);
    assert_eq!(runtime.file_backend().storage().tmp_path, None);
    assert_eq!(
        runtime.file_backend().storage().deleted,
        ["tmp/uploaded.sqbc"]
    );
}

#[test]
fn replacing_app_discards_staged_ephemeral_upload() {
    let sqbc = compile_sqbc(
        r#"app "native-upload-cleanup"
event.on("app.start") {}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-upload-cleanup", &sqbc);
    assert_eq!(
        runtime
            .stage_ephemeral_upload("proof.txt", b"ready", "rx", NativeUploadTransport::Ble,)
            .unwrap(),
        "tmp/proof.txt"
    );

    run_temp_app(&mut runtime, "native-upload-cleanup", &sqbc);

    assert_eq!(runtime.file_backend().storage().tmp_path, None);
    assert_eq!(runtime.file_backend().storage().deleted, ["tmp/proof.txt"]);
}

#[derive(Default)]
struct FileBackedBinBookBackend {
    reset_calls: usize,
    open_calls: usize,
    info_calls: usize,
    read_page_calls: usize,
    chapters_calls: usize,
    chapter_calls: usize,
}

impl NativeFileBackend for FileBackedBinBookBackend {
    fn reset_runtime_state(&mut self) {
        self.reset_calls += 1;
    }

    fn binbook_open<'a>(
        &'a mut self,
        path: &str,
    ) -> Result<BinBookOpenResult<'a>, squidvm_core::error::VmError> {
        self.open_calls += 1;
        assert_eq!(path, "books/readme.binbook");
        Ok(BinBookOpenResult {
            ok: true,
            error: None,
            book: Some(Handle::new(HandleKind::BinBook, 4)),
        })
    }

    fn binbook_info<'a>(
        &'a mut self,
        book: Handle,
    ) -> Result<BinBookInfoResult<'a>, squidvm_core::error::VmError> {
        self.info_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        Ok(BinBookInfoResult {
            ok: true,
            error: None,
            title: Some("Storage Book"),
            page_count: 12,
            chapter_count: 3,
        })
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, squidvm_core::error::VmError> {
        self.read_page_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        assert_eq!(page_index, 0);
        Ok(BinBookReadPageResult {
            ok: true,
            error: None,
            drawable: Some(Handle::new(HandleKind::Drawable, 2)),
        })
    }

    fn binbook_chapters_into<'a>(
        &'a mut self,
        book: Handle,
        offset: i32,
        limit: i32,
        writer: &mut dyn BinBookChapterListWriter,
    ) -> Result<BinBookChapterListSummary<'a>, squidvm_core::error::VmError> {
        self.chapters_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        assert_eq!(offset, 0);
        assert_eq!(limit, 8);
        writer.push_entry(BinBookChapterEntry {
            index: 0,
            title: "Start",
            page_index: 0,
            level: 0,
            entry_type: 3,
        })?;
        writer.push_entry(BinBookChapterEntry {
            index: 1,
            title: "Next",
            page_index: 4,
            level: 0,
            entry_type: 3,
        })?;
        Ok(BinBookChapterListSummary {
            ok: true,
            error: None,
            count: 3,
            has_more: true,
        })
    }

    fn binbook_chapter<'a>(
        &'a mut self,
        book: Handle,
        index: i32,
    ) -> Result<BinBookChapterResult<'a>, squidvm_core::error::VmError> {
        self.chapter_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 4));
        assert_eq!(index, 1);
        Ok(BinBookChapterResult {
            ok: true,
            error: None,
            chapter: Some(BinBookChapterEntry {
                index: 1,
                title: "Next",
                page_index: 4,
                level: 0,
                entry_type: 3,
            }),
        })
    }
}

#[test]
fn runtime_reset_releases_file_backend_runtime_state() {
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FileBackedBinBookBackend::default(),
    );

    runtime.reset();

    assert_eq!(runtime.file_backend().reset_calls, 1);
}

#[test]
fn native_file_backend_can_drive_binbook_open_and_info() {
    let sqbc = compile_sqbc(
        r#"app "native-file-binbook"
event.on("app.start") {
  let opened = binbook.open("books/readme.binbook")
  debug.print("open", opened.ok, opened.error)
  if opened.ok {
    let info = binbook.info(opened.book)
    debug.print("info", info.ok, info.title, info.pageCount, info.chapterCount)
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FileBackedBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file-binbook", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["open true null", "info true Storage Book 12 3"]
    );
    assert_eq!(runtime.file_backend().open_calls, 1);
    assert_eq!(runtime.file_backend().info_calls, 1);
}

#[test]
fn native_file_backend_can_drive_binbook_read_page_and_drawable_draw() {
    let sqbc = compile_sqbc(
        r#"app "native-file-binbook-page"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  let opened = binbook.open("books/readme.binbook")
  if opened.ok {
    let page = binbook.readPage(opened.book, 0)
    debug.print("page", page.ok, page.error)
    if page.ok {
      service.display.draw(page.drawable)
    }
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FileBackedBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file-binbook-page", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["page true null"]);
    assert_eq!(
        runtime.drawlog_lines().as_slice(),
        &["draw Drawable:2 x=0 y=0 w=0 h=0"]
    );
    assert_eq!(runtime.file_backend().read_page_calls, 1);
}

#[test]
fn native_file_backend_can_drive_binbook_chapters_and_chapter() {
    let sqbc = compile_sqbc(
        r#"app "native-file-binbook-chapters"
event.on("app.start") {
  let opened = binbook.open("books/readme.binbook")
  if opened.ok {
    let chapters = binbook.chapters(opened.book, { offset: 0, limit: 8 })
    debug.print("chapters", chapters.ok, chapters.count, chapters.hasMore)
    for chapter in chapters.items max 8 {
      debug.print(chapter.index, chapter.title, chapter.pageIndex, chapter.level, chapter.type)
    }
    let chapter = binbook.chapter(opened.book, 1)
    debug.print("chapter", chapter.ok, chapter.index, chapter.title, chapter.pageIndex)
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        FileBackedBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-file-binbook-chapters", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "chapters true 3 true",
            "0 Start 0 0 3",
            "1 Next 4 0 3",
            "chapter true 1 Next 4"
        ]
    );
    assert_eq!(runtime.file_backend().open_calls, 1);
    assert_eq!(runtime.file_backend().chapters_calls, 1);
    assert_eq!(runtime.file_backend().chapter_calls, 1);
}

const TEST_CONTENT_BINBOOKS: [ContentBinBookEntry; 1] = [ContentBinBookEntry {
    name: "proof.binbook",
    reference: "content:books/r/proof.binbook",
    size: 4096,
}];

#[derive(Default)]
struct FakeBinBookBackend {
    list_calls: usize,
    open_calls: usize,
    read_page_calls: usize,
}

impl NativeBinBookBackend for FakeBinBookBackend {
    fn content_binbook_list<'a>(
        &'a mut self,
        library: &str,
        offset: i32,
        limit: i32,
    ) -> Result<ContentBinBookListResult<'a>, squidvm_core::error::VmError> {
        self.list_calls += 1;
        assert_eq!(library, "books");
        assert_eq!(offset, 0);
        assert_eq!(limit, 1);
        Ok(ContentBinBookListResult {
            ok: true,
            error: None,
            warning: None,
            items: &TEST_CONTENT_BINBOOKS,
            count: TEST_CONTENT_BINBOOKS.len() as i32,
            has_more: false,
        })
    }

    fn binbook_open<'a>(
        &'a mut self,
        path: &str,
    ) -> Result<BinBookOpenResult<'a>, squidvm_core::error::VmError> {
        self.open_calls += 1;
        assert_eq!(path, "content:books/r/proof.binbook");
        Ok(BinBookOpenResult {
            ok: true,
            error: None,
            book: Some(Handle::new(HandleKind::BinBook, 3)),
        })
    }

    fn binbook_read_page<'a>(
        &'a mut self,
        book: Handle,
        page_index: i32,
    ) -> Result<BinBookReadPageResult<'a>, squidvm_core::error::VmError> {
        self.read_page_calls += 1;
        assert_eq!(book, Handle::new(HandleKind::BinBook, 3));
        assert_eq!(page_index, 0);
        Ok(BinBookReadPageResult {
            ok: true,
            error: None,
            drawable: Some(Handle::new(HandleKind::Drawable, 9)),
        })
    }
}

#[test]
fn native_binbook_backend_drives_content_list_open_and_read_page() {
    let sqbc = compile_sqbc(
        r#"app "native-binbook"
event.on("app.start") {
  let listing = content.binbook.list("books", { offset: 0, limit: 1 })
  debug.print("list", listing.ok, listing.count, listing.hasMore)
  if listing.ok {
    for item in listing.items max 1 {
      let opened = binbook.open(item.ref)
      debug.print("open", opened.ok)
      if opened.ok {
        let page = binbook.readPage(opened.book, 0)
        debug.print("page", page.ok)
      }
    }
  }
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_display_and_binbook(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        FakeBinBookBackend::default(),
    );

    run_temp_app(&mut runtime, "native-binbook", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["list true 1 false", "open true", "page true"]
    );
    let backend = runtime.binbook_backend();
    assert_eq!(backend.list_calls, 1);
    assert_eq!(backend.open_calls, 1);
    assert_eq!(backend.read_page_calls, 1);
}

#[derive(Default)]
struct CountingDisplaySink {
    events: Vec<String>,
    dropped_draws: u32,
}

impl NativeDisplaySink for CountingDisplaySink {
    fn draw_clear(&mut self, color: u8) {
        self.events.push(format!("clear {color}"));
    }

    fn screen_rendered(&mut self, name: &str) {
        self.events.push(format!("rendered {name}"));
    }

    fn pending_refreshes(&self) -> u32 {
        self.events
            .iter()
            .filter(|event| event.starts_with("rendered "))
            .count() as u32
    }

    fn recorded_draws(&self) -> u32 {
        self.events
            .iter()
            .filter(|event| !event.starts_with("rendered "))
            .count() as u32
    }

    fn dropped_draws(&self) -> u32 {
        self.dropped_draws
    }
}

#[test]
fn screen_render_completion_notifies_native_display_sink() {
    let sqbc = compile_sqbc(
        r#"app "native-display-sink"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
}
"#,
    );
    let mut runtime =
        NativeRuntime::with_radio_and_display(NoopRadioBackend, CountingDisplaySink::default());

    run_temp_app(&mut runtime, "native-display-sink", &sqbc);

    assert_eq!(runtime.drawlog_lines().as_slice(), &["clear 0"]);
    assert_eq!(
        runtime.display_sink().events.as_slice(),
        &["clear 0", "rendered main"]
    );
}

#[test]
fn resources_report_display_sink_refresh_state() {
    let sqbc = compile_sqbc(
        r#"app "native-display-resources"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
}
"#,
    );
    let mut runtime =
        NativeRuntime::with_radio_and_display(NoopRadioBackend, CountingDisplaySink::default());

    run_temp_app(&mut runtime, "native-display-resources", &sqbc);

    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "display_pending_refreshes" && metric.value == 1));
}

#[test]
fn resources_report_display_sink_flush_queue_state() {
    let sqbc = compile_sqbc(
        r#"app "native-display-queue"
event.on("app.start") {
  screen.open("main")
}

screen("main") {
  service.display.clear(color.WHITE)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_and_display(
        NoopRadioBackend,
        CountingDisplaySink {
            events: Vec::new(),
            dropped_draws: 2,
        },
    );

    run_temp_app(&mut runtime, "native-display-queue", &sqbc);

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "display_recorded_draws" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "display_dropped_draws" && metric.value == 2));
}

#[test]
fn temp_run_reports_capability_demand_from_sqbc_builtins() {
    let sqbc = compile_sqbc(
        r#"app "native-demand"
event.on("app.start") {
  let status = service.wifi.status()
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: { complete: "ble.done" }
  })
  let info = display.info()
  let files = file.list("books", { offset: 0, limit: 8 })
  debug.print(status.active, info.available, files.count)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-demand", &sqbc);

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_wifi" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_ble" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_display" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "demand_storage" && metric.value == 1));
}

#[test]
fn installed_launch_replaces_capability_demand_metadata() {
    let radio_sqbc = compile_sqbc(
        r#"app "installed-radio"
event.on("app.start") {
  let status = service.wifi.status()
  debug.print(status.active)
}
"#,
    );
    let plain_sqbc = compile_sqbc(
        r#"app "installed-plain"
event.on("app.start") {
  debug.print("plain")
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    install_app(&mut runtime, "installed-radio", &radio_sqbc);
    runtime.launch_app("installed-radio").unwrap();
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "demand_wifi" && metric.value == 1));

    install_app(&mut runtime, "installed-plain", &plain_sqbc);
    runtime.launch_app("installed-plain").unwrap();

    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "demand_wifi" && metric.value == 0));
}

#[test]
fn wifi_service_calls_update_native_radio_lease_metrics() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  debug.print("wifi", ap.ok, ap.error)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-wifi", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["wifi true null"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 1));
}

#[test]
fn wifi_stop_releases_native_radio_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-stop"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let stop = service.wifi.stopAP()
  debug.print("wifi", ap.ok, stop.ok)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-wifi-stop", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["wifi true true"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
}

#[test]
fn wifi_status_reports_native_ap_configuration_and_events() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-status"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let started = service.wifi.status()
  let stop = service.wifi.stopAP()
  let stopped = service.wifi.status()
  debug.print("started", ap.ok, started.active, started.mode, started.ssid)
  debug.print("started-state", started.state, started.backend, started.driverStarted, started.configured, started.driverMode)
  debug.print("started-events", started.channel, started.apStartEvents, started.apStopEvents)
  debug.print("stopped", stop.ok, stopped.active, stopped.mode, stopped.ssid)
  debug.print("stopped-state", stopped.state, stopped.backend, stopped.driverStarted, stopped.configured, stopped.driverMode)
  debug.print("stopped-events", stopped.channel, stopped.apStartEvents, stopped.apStopEvents)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-status", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "started true true ap SquidNative",
            "started-state started native-x4 true true ap",
            "started-events 1 1 0",
            "stopped true false null null",
            "stopped-state stopped native-x4 false false null",
            "stopped-events 0 1 1",
        ]
    );
}

#[test]
fn wifi_scan_without_native_scan_support_reports_result_and_releases_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-scan"
event.on("app.start") {
  let scan = service.wifi.scan()
  let op = service.wifi.operation()
  let result = service.wifi.result()
  debug.print("scan", scan.ok, scan.error, scan.active, scan.kind, scan.state, scan.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-scan", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "scan false unsupported false scan error true",
            "operation false unsupported false scan error true",
            "result true false unsupported scan error 0",
        ]
    );
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
}

#[test]
fn wifi_scan_reports_backend_networks_and_releases_temporary_scan_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-scan-real"
event.on("app.start") {
  let scan = service.wifi.scan()
  let result = service.wifi.result()
  let row0 = service.wifi.scanNetwork(0)
  let row1 = service.wifi.scanNetwork(1)
  debug.print("scan", scan.ok, scan.error, scan.active, scan.kind, scan.state, scan.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("row0", row0.ok, row0.error, row0.ssid, row0.ssidLength, row0.channel, row0.rssi, row0.auth, row0.bssid, row0.hidden)
  debug.print("row1", row1.ok, row1.error, row1.ssidLength)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        scan_supported: true,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-scan-real", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "scan true null false scan done true",
            "result true true null scan done 1",
            "row0 true null SquidLab 8 6 -42 WPA2_PSK 02:04:06:08:0a:0c false",
            "row1 false not-found 0",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 1);
    assert_eq!(backend.wifi_scan_count, 1);
    assert_eq!(backend.wifi_release_count, 1);
}

#[test]
fn wifi_scan_starts_running_without_calling_backend_until_completion_step() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-scan-pending"
event.on("app.start") {
  let scan = service.wifi.scan()
  let result = service.wifi.result()
  debug.print("scan", scan.ok, scan.error, scan.active, scan.kind, scan.state, scan.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        scan_supported: true,
        wifi_operations_deferred: true,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-scan-pending", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "scan true null true scan running false",
            "result false true null scan running 0",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_scan_count, 0);
    assert_eq!(backend.pending_wifi_scan_count, 1);
}

#[test]
fn wifi_scan_completion_records_count_and_releases_temporary_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-scan-complete"
event.on("app.start") {
  service.wifi.scan()
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        scan_supported: true,
        wifi_operations_deferred: true,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-scan-complete", &sqbc);
    runtime.complete_wifi_scan(3).unwrap();

    let result = runtime.wifi_operation_result();
    assert!(result.ready);
    assert!(result.ok);
    assert_eq!(result.kind, Some("scan"));
    assert_eq!(result.state, "done");
    assert_eq!(result.count, 3);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
}

#[test]
fn wifi_scan_while_ap_active_reports_busy_and_keeps_ap_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-busy-scan"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let scan = service.wifi.scan()
  let op = service.wifi.operation()
  let result = service.wifi.result()
  let status = service.wifi.status()
  debug.print("scan", ap.ok, scan.ok, scan.error, scan.active, scan.kind, scan.state, scan.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("status", status.active, status.mode, status.ssid)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-busy-scan", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "scan true false wifi busy false scan error true",
            "operation false wifi busy false scan error true",
            "result true false wifi busy scan error 0",
            "status true ap SquidNative",
        ]
    );
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 1));
}

#[test]
fn wifi_status_and_ap_ip_report_backend_provided_network_details() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-real-status"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  let ip = service.wifi.getAPIP()
  let status = service.wifi.status()
  debug.print("ap", ap.ok, ip.error, ip.ip, ip.gw, ip.netmask)
  debug.print("status", status.active, status.mode, status.ipAddress, status.ssid, status.clients, status.channel)
  debug.print("events", status.apStartEvents, status.apStopEvents, status.probeEvents)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        ap_ip_supported: true,
        connected_clients: 2,
        probe_events: 3,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-real-status", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "ap true null 192.0.2.1 192.0.2.1 255.255.255.0",
            "status true ap 192.0.2.1 SquidNative 2 6",
            "events 1 0 3",
        ]
    );
}

#[test]
fn wifi_connect_missing_profile_reports_error_without_acquiring_radio() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-missing-profile"
event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let op = service.wifi.operation()
  let result = service.wifi.result()
  let status = service.wifi.status()
  debug.print("connect", connect.ok, connect.error, connect.active, connect.kind, connect.state, connect.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("status", status.active, status.mode, status.profile, status.connected)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-wifi-missing-profile", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "connect false profile missing false connect error true",
            "operation false profile missing false connect error true",
            "result true false profile missing connect error 0",
            "status false null null false",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 0);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
}

#[test]
fn wifi_connect_configures_matching_profile_as_station_operation() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-profile"
event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let op = service.wifi.operation()
  let result = service.wifi.result()
  let status = service.wifi.status()
  debug.print("connect", connect.ok, connect.error, connect.active, connect.kind, connect.state, connect.done)
  debug.print("operation", op.ok, op.error, op.active, op.kind, op.state, op.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
  debug.print("status", status.active, status.mode, status.profile, status.connected)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());
    runtime
        .set_wifi_profile("dev", "SquidNet", "password")
        .unwrap();

    run_temp_app(&mut runtime, "native-wifi-profile", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "connect true null true connect running false",
            "operation true null true connect running false",
            "result false true null connect running 0",
            "status true sta dev true",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 1);
    assert_eq!(backend.wifi_connect_count, 1);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
}

#[test]
fn wifi_connect_can_defer_backend_station_work() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-profile-deferred"
event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let result = service.wifi.result()
  debug.print("connect", connect.ok, connect.error, connect.active, connect.kind, connect.state, connect.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        wifi_operations_deferred: true,
        ..CountingRadioBackend::default()
    });
    runtime
        .set_wifi_profile("dev", "SquidNet", "password")
        .unwrap();

    run_temp_app(&mut runtime, "native-wifi-profile-deferred", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "connect true null true connect running false",
            "result false true null connect running 0",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_connect_count, 0);
    assert_eq!(backend.pending_wifi_connect_count, 1);
    assert_eq!(runtime.wifi_operation_active_kind(), Some("connect"));
}

#[test]
fn wifi_connect_completion_marks_operation_done_and_keeps_station_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-profile-complete"
event.on("app.start") {
  service.wifi.connect("dev")
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        wifi_operations_deferred: true,
        ..CountingRadioBackend::default()
    });
    runtime
        .set_wifi_profile("dev", "SquidNet", "password")
        .unwrap();

    run_temp_app(&mut runtime, "native-wifi-profile-complete", &sqbc);
    runtime.complete_wifi_connect().unwrap();

    let result = runtime.wifi_operation_result();
    assert!(result.ready);
    assert!(result.ok);
    assert_eq!(result.kind, Some("connect"));
    assert_eq!(result.state, "done");
    assert_eq!(runtime.wifi_operation_active_kind(), None);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 1));
}

#[test]
fn wifi_station_status_reports_backend_provided_ip_address() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-sta-ip"
event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let status = service.wifi.status()
  debug.print("station", connect.ok, status.connected, status.mode, status.ipAddress)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        sta_ip_supported: true,
        ..CountingRadioBackend::default()
    });
    runtime
        .set_wifi_profile("dev", "SquidNet", "password")
        .unwrap();

    run_temp_app(&mut runtime, "native-wifi-sta-ip", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["station true true sta 198.51.100.23"]
    );
}

#[test]
fn wifi_start_ap_can_defer_backend_work() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-ap-deferred"
event.on("app.start") {
  let start = service.wifi.startAP("SquidNative")
  let result = service.wifi.result()
  debug.print("ap", start.ok, start.error, start.active, start.kind, start.state, start.done)
  debug.print("result", result.ready, result.ok, result.error, result.kind, result.state, result.count)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        wifi_operations_deferred: true,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-ap-deferred", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &[
            "ap true null true startAP running false",
            "result false true null startAP running 0",
        ]
    );
    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 1);
    assert_eq!(backend.wifi_start_ap_count, 0);
    assert_eq!(backend.pending_wifi_start_ap_count, 1);
    assert_eq!(backend.wifi_release_count, 0);
}

#[test]
fn wifi_start_ap_completion_marks_done_and_keeps_ap_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-wifi-ap-complete"
event.on("app.start") {
  service.wifi.startAP("SquidNative")
}

event.on("ap.poll") {
  let result = service.wifi.result()
  let status = service.wifi.status()
  debug.print("ap poll", result.ready, result.ok, result.kind, result.state, status.mode, status.state)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend {
        wifi_operations_deferred: true,
        ..CountingRadioBackend::default()
    });

    run_temp_app(&mut runtime, "native-wifi-ap-complete", &sqbc);
    {
        let backend = runtime.radio_backend_mut();
        backend.ap_mode = true;
        backend.ap_ssid.clear();
        backend.ap_ssid.push_str("SquidNative");
    }
    runtime.complete_wifi_start_ap().unwrap();

    let result = runtime.wifi_operation_result();
    assert!(result.ready);
    assert!(result.ok);
    assert_eq!(result.kind, Some("startAP"));
    assert_eq!(result.state, "done");
    let backend = runtime.radio_backend();
    assert_eq!(backend.pending_wifi_start_ap_count, 1);
    assert_eq!(backend.wifi_release_count, 0);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 1));

    runtime.dispatch_event("ap.poll").unwrap();

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["ap poll true true startAP done ap started"]
    );
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 1));
}

#[test]
fn ble_service_start_and_reset_release_native_radio_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-ble"
event.on("app.start") {
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("ble ready")
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-ble", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["ble ready"]);
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 1));

    runtime.reset();

    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
}

#[test]
fn native_runtime_upload_start_records_profile_and_transport_state() {
    let sqbc = compile_sqbc(
        r#"app "native-ble-profile"
event.on("app.start") {
  let started = service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("upload ready", started.ok, started.id)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-ble-profile", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["upload ready true rx"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_id_len" && metric.value == 2));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_start_events" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_stop_events" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_transport_ble_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_transport_http_active" && metric.value == 0));

    let backend = runtime.radio_backend();
    assert_eq!(backend.ble_profile_start_count, 1);
    assert_eq!(backend.ble_profile_stop_count, 0);
    assert_eq!(backend.ble_profile_id, "rx");
}

#[test]
fn native_runtime_upload_status_and_stop_clear_profile_state() {
    let sqbc = compile_sqbc(
        r#"app "native-ble-profile-stop"
event.on("app.start") {
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["http", "ble"]
    events: {
      complete: "ble.done"
    }
  })
  let active = service.upload.status()
  debug.print("active", active.active, active.httpPath)
  service.upload.stop()
  let stopped = service.upload.status()
  debug.print("stopped", stopped.active)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-ble-profile-stop", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["active true /upload/<safe-name>", "stopped false"]
    );
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_id_len" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_start_events" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "upload_profile_stop_events" && metric.value == 1));

    let backend = runtime.radio_backend();
    assert_eq!(backend.ble_profile_start_count, 1);
    assert_eq!(backend.ble_profile_stop_count, 1);
    assert!(backend.ble_profile_id.is_empty());
}

#[test]
fn native_runtime_upload_route_uses_transport_accept_and_complete_event() {
    let sqbc = compile_sqbc(
        r#"app "native-ble-route"
event.on("app.start") {
  service.upload.start({
    id: "rx"
    accept: [".txt", ".binbook"]
    transports: ["http", "ble"]
    events: {
      complete: "ble.file.complete"
    }
  })
  debug.print("ble ready")
}
event.on("ble.file.complete", ev) {
  debug.print("upload", ev.id, ev.name)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-ble-route", &sqbc);

    let route = runtime
        .resolve_upload_route("proof.txt", NativeUploadTransport::Ble)
        .unwrap();
    assert_eq!(route.profile_id, "rx");
    assert_eq!(route.complete_event, "ble.file.complete");
    assert_eq!(
        runtime.resolve_upload_route("proof.jpg", NativeUploadTransport::Http),
        Err(NativeUploadRouteError::RouteMismatch)
    );
}

#[test]
fn native_runtime_upload_http_start_does_not_acquire_wifi() {
    let sqbc = compile_sqbc(
        r#"app "native-http-upload"
event.on("app.start") {
  let started = service.upload.start({
    id: "rx"
    accept: [".txt"]
    transports: ["http"]
    events: { complete: "upload.complete" }
  })
  let status = service.upload.status()
  debug.print(started.ok, started.httpPath, status.active)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-http-upload", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["true /upload/<safe-name> true"]
    );
    assert_eq!(runtime.radio_backend().wifi_acquire_count, 0);
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "upload_transport_http_active" && metric.value == 1));
}

#[test]
fn native_runtime_upload_start_reports_unsupported_transport() {
    let sqbc = compile_sqbc(
        r#"app "native-http-unsupported"
event.on("app.start") {
  let started = service.upload.start({
    id: "rx"
    accept: [".txt"]
    transports: ["http"]
    events: { complete: "upload.complete" }
  })
  let status = service.upload.status()
  debug.print(started.ok, started.error, status.active, status.error)
}
"#,
    );
    let mut backend = CountingRadioBackend::default();
    backend.http_upload_unsupported = true;
    let mut runtime = NativeRuntime::with_radio_backend(backend);

    run_temp_app(&mut runtime, "native-http-unsupported", &sqbc);

    assert_eq!(
        runtime.output_lines().as_slice(),
        &["false unsupported false unsupported"]
    );
}

#[test]
fn native_runtime_upload_resume_requires_exact_active_session() {
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );
    let path = runtime
        .begin_ephemeral_upload("proof.txt", 5, "rx", NativeUploadTransport::Http)
        .unwrap()
        .to_string();
    runtime
        .write_ephemeral_upload_chunk(path.as_str(), 0, b"re")
        .unwrap();
    let progress = runtime.active_upload_progress().unwrap();
    assert_eq!(progress.path, path);
    assert_eq!(progress.name, "proof.txt");
    assert_eq!(progress.id, "rx");
    assert_eq!(progress.transport, NativeUploadTransport::Http);
    assert_eq!(progress.bytes_received, 2);
    assert_eq!(progress.total_bytes, 5);

    assert_eq!(
        runtime
            .begin_ephemeral_upload("proof.txt", 5, "rx", NativeUploadTransport::Http)
            .unwrap(),
        path
    );
    assert_eq!(
        runtime.begin_ephemeral_upload("other.txt", 5, "rx", NativeUploadTransport::Http),
        Err(NativeRuntimeError::UploadSessionActive)
    );
    assert_eq!(
        runtime.write_ephemeral_upload_chunk(path.as_str(), 0, b"xx"),
        Err(NativeRuntimeError::InvalidOffset)
    );
    runtime
        .write_ephemeral_upload_chunk(path.as_str(), 2, b"ady")
        .unwrap();
    runtime.commit_ephemeral_upload(path.as_str(), 5).unwrap();
}

#[test]
fn native_runtime_upload_stop_deletes_in_flight_stage() {
    let sqbc = compile_sqbc(
        r#"app "native-upload-stop-cleanup"
event.on("app.start") {
  service.upload.start({
    id: "rx"
    accept: [".txt"]
    transports: ["http"]
    events: { complete: "upload.complete" }
  })
}
event.on("stop", ev) { service.upload.stop() }
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );
    run_temp_app(&mut runtime, "native-upload-stop-cleanup", &sqbc);
    let path = runtime
        .begin_ephemeral_upload("proof.txt", 5, "rx", NativeUploadTransport::Http)
        .unwrap()
        .to_string();
    runtime
        .write_ephemeral_upload_chunk(path.as_str(), 0, b"re")
        .unwrap();

    runtime.dispatch_event("stop").unwrap();

    assert!(runtime
        .begin_ephemeral_upload("other.txt", 3, "other", NativeUploadTransport::Http)
        .is_ok());
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "upload_profile_active" && metric.value == 0));
}

#[test]
fn wifi_and_ble_service_calls_can_hold_native_leases_together() {
    let sqbc = compile_sqbc(
        r#"app "native-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("radios", ap.ok)
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-radios", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["radios true"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 1));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 2));
}

#[test]
fn app_exit_releases_all_native_radio_leases() {
    let sqbc = compile_sqbc(
        r#"app "native-exit-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("before-exit", ap.ok)
  app.exit()
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-exit-radios", &sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["before-exit true"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}

#[test]
fn runtime_error_releases_all_native_radio_leases() {
    let sqbc = compile_sqbc(
        r#"app "native-error-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print(ap.missing)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());
    runtime
        .begin_temp_run("native-error-radios", sqbc.len())
        .unwrap();
    runtime.write_temp_run_chunk(0, &sqbc).unwrap();

    assert_eq!(
        runtime.commit_temp_run(),
        Err(NativeRuntimeError::Vm(
            squidvm_core::error::VmError::InvalidOperand
        ))
    );

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}

#[test]
fn storage_format_releases_radio_leases_and_preserves_content_files() {
    let sqbc = compile_sqbc(
        r#"app "native-format-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("format-ready", ap.ok)
}

"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        CountingRadioBackend::default(),
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );

    run_temp_app(&mut runtime, "native-format-radios", &sqbc);
    runtime.storage_format().unwrap();

    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
    assert!(!runtime.file_backend().storage().formatted);

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}

#[test]
fn storage_format_clears_installed_app_and_saved_state() {
    let sqbc = compile_sqbc(
        r#"app "native-format-app"
state { count: int = 0 }
event.on("app.start") {
  state.load()
  state.count = state.count + 1
  state.save()
}
"#,
    );
    let file_backend =
        BoundedNativeFileBackend::<StaticFileStorage, 32, 4, 16>::new(StaticFileStorage::default());
    let mut runtime = NativeRuntime::with_radio_display_binbook_and_file(
        NoopRadioBackend,
        CountingDisplaySink::default(),
        NoopBinBookBackend,
        file_backend,
    );
    install_app(&mut runtime, "native-format-app", &sqbc);
    runtime.launch_app("native-format-app").unwrap();
    assert!(runtime.installed_app().is_some());
    assert!(!runtime.state_bytes().is_empty());

    runtime.storage_format().unwrap();

    assert_eq!(runtime.installed_app(), None);
    assert!(runtime.state_bytes().is_empty());
}

#[test]
fn replacing_temp_app_releases_previous_native_radio_leases() {
    let radio_sqbc = compile_sqbc(
        r#"app "native-radio"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("radio", ap.ok)
}
"#,
    );
    let plain_sqbc = compile_sqbc(
        r#"app "native-plain"
event.on("app.start") {
  debug.print("plain")
}
"#,
    );
    let mut runtime = NativeRuntime::new();

    run_temp_app(&mut runtime, "native-radio", &radio_sqbc);
    assert!(runtime
        .resource_metrics()
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 2));

    run_temp_app(&mut runtime, "native-plain", &plain_sqbc);

    assert_eq!(runtime.output_lines().as_slice(), &["plain"]);
    let resources = runtime.resource_metrics();
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_wifi_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_ble_active" && metric.value == 0));
    assert!(resources
        .iter()
        .any(|metric| metric.key == "radio_active_leases" && metric.value == 0));
}

#[derive(Default)]
struct CountingRadioBackend {
    http_upload_unsupported: bool,
    wifi_acquire_count: usize,
    wifi_release_count: usize,
    wifi_start_ap_count: usize,
    wifi_stop_ap_count: usize,
    wifi_connect_count: usize,
    wifi_scan_count: usize,
    ble_acquire_count: usize,
    ble_release_count: usize,
    ble_profile_start_count: usize,
    ble_profile_stop_count: usize,
    ble_profile_id: String,
    ap_mode: bool,
    sta_mode: bool,
    ap_ssid: String,
    scan_supported: bool,
    wifi_operations_deferred: bool,
    pending_wifi_scan_count: usize,
    pending_wifi_connect_count: usize,
    pending_wifi_start_ap_count: usize,
    ap_ip_supported: bool,
    sta_ip_supported: bool,
    connected_clients: i32,
    probe_events: i32,
}

impl NativeRadioBackend for CountingRadioBackend {
    fn supports_upload_transport(&self, transport: NativeUploadTransport) -> bool {
        transport != NativeUploadTransport::Http || !self.http_upload_unsupported
    }

    fn acquire(&mut self, radio: RadioKind) -> Result<(), ()> {
        match radio {
            RadioKind::Wifi => self.wifi_acquire_count += 1,
            RadioKind::Ble => self.ble_acquire_count += 1,
        }
        Ok(())
    }

    fn release(&mut self, radio: RadioKind) {
        match radio {
            RadioKind::Wifi => {
                self.wifi_release_count += 1;
                if self.ap_mode {
                    self.wifi_stop_ap_count += 1;
                }
                self.ap_mode = false;
                self.sta_mode = false;
                self.ap_ssid.clear();
            }
            RadioKind::Ble => {
                self.ble_release_count += 1;
                self.ble_profile_id.clear();
            }
        }
    }

    fn start_wifi_ap(&mut self, ssid: &str) -> Result<(), ()> {
        assert_eq!(ssid, "SquidNative");
        self.wifi_start_ap_count += 1;
        self.ap_mode = true;
        self.ap_ssid.clear();
        self.ap_ssid.push_str(ssid);
        Ok(())
    }

    fn begin_start_wifi_ap(&mut self, ssid: &str) -> NativeWifiBackendOperation {
        if self.wifi_operations_deferred {
            assert_eq!(ssid, "SquidNative");
            self.pending_wifi_start_ap_count += 1;
            NativeWifiBackendOperation::Pending
        } else {
            match self.start_wifi_ap(ssid) {
                Ok(()) => NativeWifiBackendOperation::Done { count: 0 },
                Err(()) => NativeWifiBackendOperation::Error {
                    error: "unavailable",
                },
            }
        }
    }

    fn start_ble_profile(&mut self, id: &str) -> Result<(), ()> {
        self.ble_profile_start_count += 1;
        self.ble_profile_id.clear();
        self.ble_profile_id.push_str(id);
        Ok(())
    }

    fn stop_ble_profile(&mut self) {
        self.ble_profile_stop_count += 1;
        self.ble_profile_id.clear();
    }

    fn wifi_mode(&self) -> Option<&'static str> {
        if self.ap_mode {
            Some("ap")
        } else if self.sta_mode {
            Some("sta")
        } else {
            None
        }
    }

    fn connect_wifi_station(&mut self, ssid: &str, password: &str) -> Result<(), ()> {
        assert_eq!(ssid, "SquidNet");
        assert_eq!(password, "password");
        self.wifi_connect_count += 1;
        self.sta_mode = true;
        Ok(())
    }

    fn begin_connect_wifi_station(
        &mut self,
        ssid: &str,
        password: &str,
    ) -> NativeWifiBackendOperation {
        if self.wifi_operations_deferred {
            assert_eq!(ssid, "SquidNet");
            assert_eq!(password, "password");
            self.pending_wifi_connect_count += 1;
            NativeWifiBackendOperation::Pending
        } else {
            match self.connect_wifi_station(ssid, password) {
                Ok(()) => NativeWifiBackendOperation::Done { count: 0 },
                Err(()) => NativeWifiBackendOperation::Error {
                    error: "unavailable",
                },
            }
        }
    }

    fn wifi_status(&self) -> NativeWifiStatus<'_> {
        NativeWifiStatus {
            mode: self.wifi_mode(),
            ssid: self.ap_mode.then_some(self.ap_ssid.as_str()),
            ip_address: if self.ap_mode && self.ap_ip_supported {
                Some("192.0.2.1")
            } else if self.sta_mode && self.sta_ip_supported {
                Some("198.51.100.23")
            } else {
                None
            },
            state: if self.ap_mode {
                "started"
            } else if self.sta_mode {
                "starting"
            } else {
                "stopped"
            },
            driver_started: self.ap_mode || self.sta_mode,
            configured: self.ap_mode || self.sta_mode,
            channel: if self.ap_mode {
                if self.ap_ip_supported {
                    6
                } else {
                    1
                }
            } else {
                0
            },
            clients: if self.ap_mode {
                self.connected_clients
            } else {
                0
            },
            ap_start_events: self.wifi_start_ap_count as i32,
            ap_stop_events: self.wifi_stop_ap_count as i32,
            probe_events: self.probe_events,
            sta_connected_events: if self.sta_mode { 1 } else { 0 },
            sta_disconnected_events: 0,
            last_backend_code: None,
            connected: self.sta_mode,
            scan_matches: if self.scan_supported { 1 } else { 0 },
            rssi: 0,
            auth: None,
            bssid: None,
            disconnect_reason: None,
            disconnect_reason_code: 0,
        }
    }

    fn wifi_ap_ip(&self) -> NativeWifiApIp<'_> {
        if self.ap_mode && self.ap_ip_supported {
            NativeWifiApIp {
                ip: Some("192.0.2.1"),
                gw: Some("192.0.2.1"),
                netmask: Some("255.255.255.0"),
                error: None,
            }
        } else {
            NativeWifiApIp::unavailable()
        }
    }

    fn scan_wifi(&mut self) -> Result<i32, &'static str> {
        self.wifi_scan_count += 1;
        if self.scan_supported {
            Ok(1)
        } else {
            Err("unsupported")
        }
    }

    fn begin_scan_wifi(&mut self) -> NativeWifiBackendOperation {
        if self.wifi_operations_deferred {
            self.pending_wifi_scan_count += 1;
            NativeWifiBackendOperation::Pending
        } else {
            match self.scan_wifi() {
                Ok(count) => NativeWifiBackendOperation::Done { count },
                Err(error) => NativeWifiBackendOperation::Error { error },
            }
        }
    }

    fn wifi_scan_network(&self, index: i32) -> Result<Option<WifiAccessPoint>, &'static str> {
        if !self.scan_supported {
            return Err("unsupported");
        }
        if index != 0 {
            return Ok(None);
        }
        Ok(Some(
            WifiAccessPoint::new(
                b"SquidLab",
                Some([0x02, 0x04, 0x06, 0x08, 0x0a, 0x0c]),
                6,
                -42,
                Some("WPA2_PSK"),
                false,
            )
            .unwrap(),
        ))
    }
}

#[test]
fn service_calls_drive_physical_radio_backend_once_per_active_lease() {
    let sqbc = compile_sqbc(
        r#"app "native-radios"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  let status = service.wifi.status()
  debug.print("radios", ap.ok, status.active)
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-radios", &sqbc);

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_acquire_count, 1);
    assert_eq!(backend.wifi_start_ap_count, 1);
    assert_eq!(backend.ble_acquire_count, 1);
    assert_eq!(backend.wifi_release_count, 0);
    assert_eq!(backend.ble_release_count, 0);
    assert_eq!(runtime.output_lines().as_slice(), &["radios true true"]);
}

#[test]
fn app_replacement_drives_physical_radio_backend_release() {
    let radio_sqbc = compile_sqbc(
        r#"app "native-radio"
event.on("app.start") {
  let ap = service.wifi.startAP("SquidNative")
  service.upload.start({
    id: "rx"
    accept: [".sqbc"]
    transports: ["ble"]
    events: {
      complete: "ble.done"
    }
  })
  debug.print("radio", ap.ok)
}
"#,
    );
    let plain_sqbc = compile_sqbc(
        r#"app "native-plain"
event.on("app.start") {
  debug.print("plain")
}
"#,
    );
    let mut runtime = NativeRuntime::with_radio_backend(CountingRadioBackend::default());

    run_temp_app(&mut runtime, "native-radio", &radio_sqbc);
    run_temp_app(&mut runtime, "native-plain", &plain_sqbc);

    let backend = runtime.radio_backend();
    assert_eq!(backend.wifi_release_count, 1);
    assert_eq!(backend.ble_release_count, 1);
}
