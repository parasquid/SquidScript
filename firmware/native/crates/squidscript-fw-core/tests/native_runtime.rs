use squidc_core::compile::{compile, CompileRequest};
use squidscript_fw_core::native_runtime::{NativeRuntime, NativeRuntimeError};

fn compile_sqbc(source: &str) -> Vec<u8> {
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    squidc_core::sqbc::encode_sqbc(&compiled.ir.unwrap()).unwrap()
}

fn run_temp_app(runtime: &mut NativeRuntime, app_id: &str, sqbc: &[u8]) {
    runtime.begin_temp_run(app_id, sqbc.len()).unwrap();
    runtime.write_temp_run_chunk(0, sqbc).unwrap();
    runtime.commit_temp_run().unwrap();
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
    assert_eq!(
        runtime.lifecycle_lines().as_slice()[0],
        "active=native-temp"
    );
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
fn fresh_runtime_reports_inactive_lifecycle() {
    let runtime = NativeRuntime::new();

    assert_eq!(
        runtime.lifecycle_lines().as_slice(),
        &["active=", "armed_stack="]
    );
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
