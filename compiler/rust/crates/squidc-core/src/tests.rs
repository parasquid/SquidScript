use crate::{
    compile::{compile, compile_with_profile, CompileRequest},
    ir::{encode_sqbc, IrDeviceBinding, IrExpr, IrStatement, SQBC_MAGIC, SQBC_VERSION},
    parser::parse,
    profile::{BuildProfile, PORTABLE_TARGET_ID},
    sqbc,
};
use std::path::{Path, PathBuf};

fn repo_path(parts: &[&str]) -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("squidc-core crate should live under compiler/rust/crates");
    parts
        .iter()
        .fold(repo_root.to_path_buf(), |path, part| path.join(part))
}

fn read_repo_fixture(parts: &[&str]) -> String {
    std::fs::read_to_string(repo_path(parts)).expect("fixture should be readable")
}

#[test]
fn parses_preload_attribute_on_event_handler() {
    let source = r#"app "preload-demo"
@preload
event.on("key.SELECT") {
  debug.print("select")
}
screen("main") {}
"#;
    let parsed = parse(source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.ast.handlers.len(), 1);
    assert_eq!(parsed.ast.handlers[0].event, "key.SELECT");
    assert!(parsed.ast.handlers[0].preload);

    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    let ir = compiled.ir.unwrap();
    assert_eq!(ir.handlers.len(), 1);
    assert!(ir.handlers[0].preload);
}

#[test]
fn rejects_preload_attribute_before_non_handler() {
    let source = r#"app "bad-preload"
@preload
function helper() {
  debug.print("no")
}
event.on("app.start") {}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(!compiled.ok);
    assert!(compiled
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_ATTRIBUTE_TARGET"));
}

#[test]
fn parses_spec_hello_menu_into_typed_ast_and_cst() {
    let source = read_repo_fixture(&["compiler", "fixtures", "valid", "hello_menu.squid"]);
    let parsed = parse(&source);

    assert!(parsed.green.is_some());
    assert_eq!(
        parsed.ast.app.as_ref().map(|app| app.id.as_str()),
        Some("hello-menu")
    );
    assert_eq!(
        parsed
            .ast
            .app
            .as_ref()
            .and_then(|app| app.target.as_deref()),
        Some("xteink-x4")
    );
    assert_eq!(
        parsed
            .ast
            .state
            .as_ref()
            .map(|state| state.selected_default),
        Some(0)
    );
    assert_eq!(
        parsed.ast.state.as_ref().map(|state| state
            .values
            .iter()
            .map(|value| value.name.as_str())
            .collect::<Vec<_>>()),
        Some(vec!["selected", "view"])
    );
    assert_eq!(
        parsed
            .ast
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>(),
        vec!["drawMenuRow"]
    );
    assert_eq!(parsed.ast.handlers.len(), 5);
    assert_eq!(
        parsed
            .ast
            .screens
            .iter()
            .map(|screen| screen.name.as_str())
            .collect::<Vec<_>>(),
        vec!["menu", "hello", "about"]
    );
}

#[test]
fn compiles_spec_hello_menu_to_screen_ir() {
    let source = read_repo_fixture(&["compiler", "fixtures", "valid", "hello_menu.squid"]);
    let output = compile(CompileRequest {
        source,
        target_id: "xteink-x4".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.format, "squidscript-ir");
    assert_eq!(ir.version, 1);
    assert_eq!(
        ir.state
            .iter()
            .map(|state| state.name.as_str())
            .collect::<Vec<_>>(),
        vec!["selected", "view"]
    );
    assert_eq!(
        ir.functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>(),
        vec!["drawMenuRow"]
    );
    assert_eq!(
        ir.screens
            .iter()
            .map(|screen| screen.name.as_str())
            .collect::<Vec<_>>(),
        vec!["menu", "hello", "about"]
    );
    assert_eq!(
        ir.handlers
            .iter()
            .map(|handler| handler.event.as_str())
            .collect::<Vec<_>>(),
        vec!["app.start", "key.DOWN", "key.UP", "key.SELECT", "key.BACK"]
    );
    assert!(matches!(
        ir.handlers[1].statements[0],
        IrStatement::If { .. }
    ));
    assert!(matches!(
        ir.handlers[4].statements[0],
        IrStatement::If { .. }
    ));
    let menu = ir
        .screens
        .iter()
        .find(|screen| screen.name == "menu")
        .expect("menu screen");
    assert!(menu
        .statements
        .iter()
        .any(|statement| matches!(statement, IrStatement::DisplayText { .. })));
    assert_eq!(
        menu.statements
            .iter()
            .filter(|statement| matches!(statement, IrStatement::Call { name, .. } if name == "drawMenuRow"))
            .count(),
        3
    );
}

#[test]
fn parses_and_lowers_simple_handlers() {
    let source = r#"app "hello-menu" target "xteink-x4"
state { selected: int = 0 }
event.on("app.start") {
  screen.open("main")
}
event.on("key.DOWN") {
  selected = selected + 1
  screen.refresh()
}
screen("main") {
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.handlers.len(), 2);
    assert_eq!(ir.handlers[0].event, "app.start");
    assert_eq!(ir.handlers[1].event, "key.DOWN");
}

#[test]
fn parses_and_lowers_functions_locals_conditionals_and_returns() {
    let source = r#"app "control-flow" target "xteink-x4"
state { selected: int = 0 }

function chooseScreen(value) {
  if (value == 0) {
return "main"
  } else {
return "detail"
  }
}

event.on("key.SELECT") {
  if (selected == 0) {
screen.open("detail")
  } else {
app.exit()
  }
}

screen("main") {
  let label = "Hello"
  service.display.clear("gray0")
  drawLabel(label)
}

screen("detail") {
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.functions.len(), 1);
    assert_eq!(ir.functions[0].name, "chooseScreen");
    assert_eq!(ir.functions[0].params, vec!["value"]);
    assert!(matches!(
        ir.functions[0].statements[0],
        IrStatement::If { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::If { .. }
    ));
    assert!(ir.screens[0]
        .statements
        .iter()
        .any(|statement| matches!(statement, IrStatement::Let { name, .. } if name == "label")));
    assert!(ir.screens[0].statements.iter().any(
        |statement| matches!(statement, IrStatement::Call { name, .. } if name == "drawLabel")
    ));
}

#[test]
fn parses_and_lowers_bounded_loops() {
    let source = r#"app "loops" target "xteink-x4"
state { selected: int = 0 }
event.on("app.start") {
  repeat (3) {
selected = selected + 1
  }
  screen.open("main")
}
screen("main") {
  let rows = visibleRows()
  for row in rows max 5 {
drawRow(row)
  }
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::Repeat { .. }
    ));
    assert!(ir.screens[0].statements.iter().any(|statement| matches!(statement, IrStatement::For { item, max: Some(_), .. } if item == "row")));
}

#[test]
fn parses_typed_locals_and_comparison_precedence() {
    let source = r#"app "precedence" target "xteink-x4"
state { count: int = 0 }
event.on("key.SELECT") {
  let next: int = count + 1
  if (count + 1 < 10) {
screen.open("main")
  }
}
screen("main") {
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(
        matches!(ir.handlers[0].statements[0], IrStatement::Let { ref name, .. } if name == "next")
    );
    let IrStatement::If { condition, .. } = &ir.handlers[0].statements[1] else {
        panic!("expected if statement");
    };
    let IrExpr::Binary {
        left,
        operator,
        right,
    } = condition
    else {
        panic!("expected comparison expression");
    };
    assert_eq!(operator, "<");
    assert!(matches!(left.as_ref(), IrExpr::Binary { operator, .. } if operator == "+"));
    assert!(matches!(right.as_ref(), IrExpr::Literal { .. }));
}

#[test]
fn matches_hello_menu_ir_fixture() {
    let source = read_repo_fixture(&["compiler", "fixtures", "valid", "hello_menu.squid"]);
    let expected = read_repo_fixture(&["compiler", "fixtures", "expected", "hello_menu.ir.json"]);
    let output = compile(CompileRequest {
        source,
        target_id: "xteink-x4".to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    let actual_json = serde_json::to_value(&ir).unwrap();
    let expected_json: serde_json::Value = serde_json::from_str(&expected).unwrap();
    assert_eq!(actual_json["app"], expected_json["app"]);
    assert_eq!(
        ir.functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>(),
        expected_json["functions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ir.handlers
            .iter()
            .map(|handler| handler.event.as_str())
            .collect::<Vec<_>>(),
        expected_json["handlers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ir.screens
            .iter()
            .map(|screen| screen.name.as_str())
            .collect::<Vec<_>>(),
        expected_json["screens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        ir.functions[0].statements[0],
        IrStatement::If { .. }
    ));
}

#[test]
fn compiles_browser_sim_binbook_reader_fixture() {
    let source = read_repo_fixture(&[
        "compiler",
        "fixtures",
        "valid",
        "binbook_reader_browser_sim.squid",
    ]);
    let output = compile(CompileRequest {
        source,
        target_id: "xteink-x4".to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.app.id, "binbook-reader");
    assert!(ir.screens.iter().any(|screen| screen.name == "main"));
}

#[test]
fn encodes_minimal_sqbc_container() {
    let source = read_repo_fixture(&["compiler", "fixtures", "valid", "hello_menu.squid"]);
    let output = compile(CompileRequest {
        source,
        target_id: "xteink-x4".to_string(),
    });
    let ir = output.ir.unwrap();
    let sqbc = encode_sqbc(&ir);

    assert_eq!(&sqbc[0..4], SQBC_MAGIC);
    assert_eq!(
        u32::from_le_bytes(sqbc[4..8].try_into().unwrap()),
        SQBC_VERSION
    );
    assert_eq!(
        u32::from_le_bytes(sqbc[8..12].try_into().unwrap()) as usize,
        sqbc.len() - 12
    );
}

#[test]
fn encodes_reference_sqbc_for_headless_counter() {
    let source = read_repo_fixture(&[
        "compiler",
        "rust",
        "fixtures",
        "conformance",
        "headless_counter.squid",
    ]);
    let output = compile(CompileRequest {
        source,
        target_id: "esp32c3-super-mini".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let sqbc = sqbc::encode_sqbc(&output.ir.unwrap()).expect("reference subset should encode");

    assert_eq!(&sqbc[0..4], sqbc::SQBC_MAGIC);
    assert_eq!(
        u32::from_le_bytes(sqbc[6..10].try_into().unwrap()) as usize,
        sqbc.len()
    );
    assert_eq!(u32::from_le_bytes(sqbc[10..14].try_into().unwrap()), 8);
    assert_eq!(
        sqbc::read_app_id(&sqbc).unwrap().as_deref(),
        Some("headless-counter")
    );
}

#[test]
fn parses_debug_print_and_release_sqbc_strips_it() {
    let source = r#"app "debug-counter"
state { count: int = 1 }
event.on("app.start") {
  debug.print("count", count)
}
screen("main") {}
"#;
    let output = compile_with_profile(
        CompileRequest {
            source: source.to_string(),
            target_id: PORTABLE_TARGET_ID.to_string(),
        },
        BuildProfile::Dev,
    );
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::DebugPrint { .. }
    ));

    let dev = sqbc::encode_sqbc_with_profile(&ir, BuildProfile::Dev).unwrap();
    let release = sqbc::encode_sqbc_with_profile(&ir, BuildProfile::Release).unwrap();
    assert!(dev.len() > release.len());
}

#[test]
fn parses_debug_blocks_in_handlers_functions_and_screens() {
    let source = r#"app "debug-blocks"
state { count: int = 1 }
function inspect(value) {
  debug {
let x = value + 1
debug.print("fn", x)
  }
  return value
}
event.on("app.start") {
  debug {
let led = service.indicator.read()
debug.print("event", count, led)
  }
}
screen("main") {
  debug {
let x = count
debug.print("screen", x)
  }
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(matches!(
        ir.functions[0].statements[0],
        IrStatement::DebugBlock { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::DebugBlock { .. }
    ));
    assert!(matches!(
        ir.screens[0].statements[0],
        IrStatement::DebugBlock { .. }
    ));
}

#[test]
fn release_sqbc_strips_debug_block_without_evaluating_expressions() {
    let source = r#"app "debug-release-strip"
state { count: int = 1 }
event.on("app.start") {
  debug {
let x = releaseOnly
debug.print("hidden", x)
  }
  count = count + 1
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::DebugBlock { .. }
    ));
    assert!(sqbc::encode_sqbc_with_profile(&ir, BuildProfile::Dev).is_err());
    let release = sqbc::encode_sqbc_with_profile(&ir, BuildProfile::Release)
        .expect("release should strip the debug block before expression compilation");
    assert!(!release.is_empty());
    assert!(!String::from_utf8_lossy(&release).contains("hidden"));
}

#[test]
fn rejects_debug_block_mutations_and_escaping_locals() {
    let source = r#"app "bad-debug"
state { count: int = 1 }
function inspect(value) {
  let outer = 1
  debug {
let x = 2
count = 3
outer = 4
value = 5
screen.open("main")
service.indicator.toggle()
service.display.clear("gray0")
return x
  }
  debug.print(x)
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!output.ok);
    let debug_errors = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "E_DEBUG_BLOCK")
        .count();
    assert!(debug_errors >= 7, "{:?}", output.diagnostics);
}

#[test]
fn allows_assignment_to_debug_local_only() {
    let source = r#"app "debug-local-assign"
state { count: int = 1 }
event.on("app.start") {
  debug {
let x = count
x = x + 1
debug.print("x", x)
  }
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    let dev = sqbc::encode_sqbc_with_profile(&ir, BuildProfile::Dev)
        .expect("debug-local assignment should encode as a local write");
    let release = sqbc::encode_sqbc_with_profile(&ir, BuildProfile::Release)
        .expect("release should strip the debug block");
    assert!(dev.len() > release.len());
}

#[test]
fn parses_render_screen_for_headless_drawlog() {
    let source = r#"app "drawlog"
state { count: int = 0 }
event.on("app.start") {
  screen.open("main")
}
screen("main") {
  service.display.clear("gray0")
  service.display.text("Hello", { x: 10, y: 20 })
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let sqbc = sqbc::encode_sqbc(&output.ir.unwrap()).unwrap();
    assert_eq!(u32::from_le_bytes(sqbc[10..14].try_into().unwrap()), 8);
}

#[test]
fn parses_display_namespace_as_display_service_sugar() {
    let source = r#"app "display-sugar"
screen("main") {
  display.clear("white")
  display.text("Hello", { x: 10, y: 20 })
  display.rect(0, 0, 100, 40, { fillColor: "gray4" })
  display.line(0, 40, 100, 40, { color: "black" })
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    let screen = ir
        .screens
        .iter()
        .find(|screen| screen.name == "main")
        .unwrap();
    assert!(matches!(
        screen.statements[0],
        IrStatement::DisplayClear { ref color } if color == "white"
    ));
    assert!(matches!(
        screen.statements[1],
        IrStatement::DisplayText { .. }
    ));
    assert!(matches!(
        screen.statements[2],
        IrStatement::DisplayRect { .. }
    ));
    assert!(matches!(
        screen.statements[3],
        IrStatement::DisplayLine { .. }
    ));
    sqbc::encode_sqbc(&ir).expect("display sugar should lower to display bytecode");
}

#[test]
fn parses_device_bindings_and_rejects_unsafe_paths() {
    let source = r#"app "device-test"
device {
  indicator { use "device/indicator.sqdevice" }
  display "status" { use "device/status-display.sqdevice" }
}
screen("main") {
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(
        ir.device_bindings,
        vec![
            IrDeviceBinding {
                service: "indicator".to_string(),
                binding: "default".to_string(),
                resource: "device/indicator.sqdevice".to_string(),
            },
            IrDeviceBinding {
                service: "display".to_string(),
                binding: "status".to_string(),
                resource: "device/status-display.sqdevice".to_string(),
            },
        ]
    );

    let bad = compile(CompileRequest {
        source: r#"app "bad-device"
device { indicator { use "../indicator.sqdevice" } }
screen("main") {}
"#
        .to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!bad.ok);
    assert!(bad
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_DEVICE_PATH"));
}

#[test]
fn parses_hardware_gpio_calls() {
    let source = r#"app "gpio"
state { led: bool = false }
event.on("app.start") {
  hardware.gpio.write("GPIO8", true)
  led = hardware.gpio.read("GPIO8")
  hardware.gpio.toggle("GPIO10")
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.app.target, PORTABLE_TARGET_ID);
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::HardwareGpioWrite { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[1],
        IrStatement::Assign {
            expr: IrExpr::HardwareGpioRead { .. },
            ..
        }
    ));
    assert!(matches!(
        ir.handlers[0].statements[2],
        IrStatement::HardwareGpioToggle { .. }
    ));
}

#[test]
fn parses_system_resource_string_calls() {
    let source = r#"app "resources"
state { ready: bool = false }
event.on("app.start") {
  debug.print(system.memory())
  debug.print(system.storage("apps"))
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    let IrStatement::DebugPrint { args } = &ir.handlers[0].statements[0] else {
        panic!("expected debug.print");
    };
    assert_eq!(args, &vec![IrExpr::SystemMemory]);
    let IrStatement::DebugPrint { args } = &ir.handlers[0].statements[1] else {
        panic!("expected debug.print");
    };
    assert_eq!(
        args,
        &vec![IrExpr::SystemStorage {
            name: "apps".to_string()
        }]
    );
}

#[test]
fn parses_typed_nullable_state_and_reset_call() {
    let source = r#"app "typed-state"
state({ store: "internal" }) {
  stateVersion: int = 2
  retryAt: int? = null
  title: string = ""
}
event.on("app.start") {
  state.reset()
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.state_store, "internal");
    assert_eq!(ir.state[1].name, "retryAt");
    assert_eq!(ir.state[1].value_type, "int");
    assert!(ir.state[1].nullable);
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::StateReset
    ));
}

#[test]
fn rejects_state_default_that_does_not_match_declared_type() {
    let source = r#"app "bad-state"
state {
  retryAt: int = "hello"
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!output.ok);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_STATE_DEFAULT"));
}

#[test]
fn parses_timer_handlers_app_launch_and_timer_service() {
    let source = r#"app "timer-demo"
state { count: int = 0 }
event.on("app.start") {
  app.launch("worker")
  service.timer.every("timer.debug", 1000)
}
event.on("timer.debug") {
  debug.print("tick", count)
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.handlers[1].event, "timer.debug");
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::AppLaunch { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[1],
        IrStatement::ServiceTimerEvery { .. }
    ));
}

#[test]
fn parses_generic_events_and_trigger_lifecycle_calls() {
    let source = r#"app "event-demo"
state { ticks: int = 0 }
event.on("app.start") {
  app.arm("reminder")
  app.launch("reader")
  service.timer.every("timer.clock", 60000)
}
event.on("app.arm") {
  service.timer.after("timer.break", 1500000)
}
event.on("app.exit") {
  state.save()
}
event.on("timer.break") {
  ticks = ticks + 1
  app.disarm("reminder")
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.handlers[0].event, "app.start");
    assert_eq!(ir.handlers[1].event, "app.arm");
    assert_eq!(ir.handlers[2].event, "app.exit");
    assert_eq!(ir.handlers[3].event, "timer.break");
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::AppArm { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[1],
        IrStatement::AppLaunch { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[2],
        IrStatement::ServiceTimerEvery { .. }
    ));
    assert!(matches!(
        ir.handlers[1].statements[0],
        IrStatement::ServiceTimerAfter { .. }
    ));
    assert!(matches!(
        ir.handlers[3].statements[1],
        IrStatement::AppDisarm { .. }
    ));
}

#[test]
fn rejects_hardware_gpio_mutation_in_screen_render() {
    let source = r#"app "gpio"
state {}
event.on("app.start") {}
screen("main") {
  hardware.gpio.toggle("GPIO8")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!output.ok);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_RENDER_PURITY"));
}

#[test]
fn reports_diagnostics_with_spans() {
    let output = compile(CompileRequest {
        source: "screen(\"main\") {}\n".to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(!output.ok);
    assert!(output
        .diagnostics
        .iter()
        .any(|d| d.code == "E_APP_REQUIRED" && d.span.end >= d.span.start));
}

#[test]
fn reports_real_semantic_diagnostics() {
    let source = r#"app "bad" target "xteink-x4"
state { selected: int = 0 }
event.on("app.start") {
  screen.open("missing")
  service.display.clear("gray0")
}
screen("main", { render: "invalid" }) {
  selected = selected + 1
}
screen("main") {
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(!output.ok);
    let codes = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"E_UNKNOWN_SCREEN"));
    assert!(codes.contains(&"E_DISPLAY_OUTSIDE_SCREEN"));
    assert!(codes.contains(&"E_RENDER_POLICY"));
    assert!(codes.contains(&"E_RENDER_PURITY"));
    assert!(codes.contains(&"E_DUPLICATE_SCREEN"));
    assert!(output
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.span.end >= diagnostic.span.start));
}

#[test]
fn parses_result_record_field_access_and_unary_not() {
    let source = r#"app "result-records" target "xteink-x4"
state { failed: bool = false }
event.on("app.start") {
  let result = library.mkdir("books", "/manuals")
  if (!result.ok) {
failed = true
debug.print(result.error)
  }
  screen.open("main")
}
screen("main") {
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.expect("expected IR");
    let IrStatement::Let { expr, .. } = &ir.handlers[0].statements[0] else {
        panic!("expected result let");
    };
    assert!(matches!(expr, IrExpr::Call { name, .. } if name == "library.mkdir"));
    let IrStatement::If {
        condition,
        then_statements,
        ..
    } = &ir.handlers[0].statements[1]
    else {
        panic!("expected result guard");
    };
    assert!(matches!(condition, IrExpr::Unary { operator, .. } if operator == "!"));
    assert!(then_statements.iter().any(|statement| matches!(
        statement,
        IrStatement::DebugPrint { args } if matches!(args.first(), Some(IrExpr::Field { field, .. }) if field == "error")
    )));
}

#[test]
fn compiles_service_wifi_ap_records_and_wifi_sugar() {
    let source = r#"app "wifi-ap"
state {}

event.on("app.start") {
  let ap = service.wifi.startAP("SquidScript")
  let status = wifi.status()
  debug.print(ap.ok, status.active, status.ipAddress)
  wifi.stopAP()
}

screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    let handler = ir
        .handlers
        .iter()
        .find(|handler| handler.event == "app.start")
        .unwrap();
    assert!(matches!(
        &handler.statements[0],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.startAP"
    ));
    assert!(matches!(
        &handler.statements[1],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.status"
    ));
    sqbc::encode_sqbc(&ir).unwrap();
}

#[test]
fn warns_when_fallible_result_is_ignored() {
    let source = r#"app "ignored-result" target "xteink-x4"
event.on("app.start") {
  library.mkdir("books", "/manuals")
  screen.open("main")
}
screen("main") {
  service.display.clear("gray0")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: "xteink-x4".to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "W_IGNORED_RESULT" && diagnostic.severity == "warning"
    }));
}
