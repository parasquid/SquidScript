use crate::{
    compile::{compile, compile_path_with_profile, compile_with_profile, CompileRequest},
    ir::{encode_sqbc, IrDeviceBinding, IrExpr, IrStatement, SQBC_MAGIC},
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

fn sqbc_sections(bytes: &[u8]) -> Vec<(u16, usize, usize)> {
    assert_eq!(&bytes[0..4], b"SQBC");
    let section_count = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    (0..section_count)
        .map(|index| {
            let base = 14 + index * 12;
            let kind = u16::from_le_bytes(bytes[base..base + 2].try_into().unwrap());
            let offset = u32::from_le_bytes(bytes[base + 4..base + 8].try_into().unwrap()) as usize;
            let len = u32::from_le_bytes(bytes[base + 8..base + 12].try_into().unwrap()) as usize;
            (kind, offset, len)
        })
        .collect()
}

fn unique_test_dir(prefix: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{}-{}-{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
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
  state.selected = state.selected + 1
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
fn explicit_state_access_and_at_sugar_compile_to_state_ir() {
    let canonical = r#"app "explicit-state"
state { index: int = 0 }
event.on("key.DOWN") {
  state.index = state.index + 1
}
screen("main") {}
"#;
    let sugar = r#"app "explicit-state"
state { index: int = 0 }
event.on("key.DOWN") {
  @index = @index + 1
}
screen("main") {}
"#;

    let canonical = compile(CompileRequest {
        source: canonical.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    let sugar = compile(CompileRequest {
        source: sugar.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(canonical.ok, "{:?}", canonical.diagnostics);
    assert!(sugar.ok, "{:?}", sugar.diagnostics);
    assert_eq!(
        canonical.ir.as_ref().unwrap().handlers[0].statements,
        sugar.ir.as_ref().unwrap().handlers[0].statements
    );
    assert!(matches!(
        &canonical.ir.as_ref().unwrap().handlers[0].statements[0],
        IrStatement::StateAssign { name, expr: IrExpr::Binary { left, .. } }
            if name == "index" && matches!(left.as_ref(), IrExpr::State { name } if name == "index")
    ));
    sqbc::encode_sqbc(canonical.ir.as_ref().unwrap()).expect("state get/set should encode");
}

#[test]
fn local_assignment_requires_declared_local_and_lowers_to_local_ir() {
    let source = r#"app "local-assign"
state { next: int = 0 }
function bump(value) {
  let next = value
  next = next + 1
  return next
}
event.on("key.SELECT") {
  debug.print(bump(1))
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    let function = &output.ir.as_ref().unwrap().functions[0];
    assert!(matches!(
        &function.statements[1],
        IrStatement::Assign { name, expr: IrExpr::Binary { left, .. } }
            if name == "next" && matches!(left.as_ref(), IrExpr::Variable { name } if name == "next")
    ));
    sqbc::encode_sqbc(output.ir.as_ref().unwrap()).expect("local get/set should encode");
}

#[test]
fn bare_state_name_read_and_assignment_are_undeclared_with_state_suggestion() {
    let source = r#"app "bare-state"
state { index: int = 0 }
event.on("key.DOWN") {
  index = index + 1
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(!output.ok);
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "E_UNDECLARED_VARIABLE"
            && diagnostic.message.contains("state.index")
            && diagnostic.message.contains("@index")
    }));
}

#[test]
fn missing_explicit_state_field_is_rejected() {
    for source in [
        r#"app "missing-state"
state { count: int = 0 }
event.on("key.SELECT") {
  state.missing = 1
}
screen("main") {}
"#,
        r#"app "missing-state"
state { count: int = 0 }
event.on("key.SELECT") {
  debug.print(@missing)
}
screen("main") {}
"#,
    ] {
        let output = compile(CompileRequest {
            source: source.to_string(),
            target_id: PORTABLE_TARGET_ID.to_string(),
        });

        assert!(!output.ok);
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E_MISSING_STATE_FIELD"));
    }
}

#[test]
fn state_field_can_share_state_builtin_name_when_not_called() {
    let source = r#"app "state-load-field"
state {
  load: int = 0
  save: int = 0
  reset: int = 0
}
event.on("key.SELECT") {
  state.load = state.load + 1
  state.save = state.save + 1
  state.reset = state.reset + 1
  state.load()
  state.save()
  state.reset()
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "W_STATE_BUILTIN_SHADOW")
            .count(),
        3,
        "{:?}",
        output.diagnostics
    );
    assert!(matches!(
        &output.ir.as_ref().unwrap().handlers[0].statements[0],
        IrStatement::StateAssign { name, expr: IrExpr::Binary { left, .. } }
            if name == "load" && matches!(left.as_ref(), IrExpr::State { name } if name == "load")
    ));
    assert!(matches!(
        output.ir.as_ref().unwrap().handlers[0].statements[3],
        IrStatement::StateLoad
    ));
}

#[test]
fn state_shadowing_by_local_param_and_for_item_warns_but_compiles() {
    let source = r#"app "shadowing"
state { count: int = 0 }
function render(count) {
  let value = count
  for item in rows() max 3 {
    debug.print(item)
  }
  return value
}
event.on("key.SELECT") {
  let count = 1
  debug.print(render(count))
}
event.on("key.BACK") {
  for count in rows() max 3 {
    debug.print(count)
  }
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "W_STATE_SHADOW"));
}

#[test]
fn state_assignment_is_rejected_in_screens_and_screen_reachable_functions() {
    let direct = r#"app "render-purity-direct"
state { count: int = 0 }
screen("main") {
  @count = @count + 1
}
"#;
    let transitive = r#"app "render-purity-call"
state { count: int = 0 }
function mutate() {
  @count = @count + 1
}
event.on("key.SELECT") {
  mutate()
}
screen("main") {
  mutate()
}
"#;

    for source in [direct, transitive] {
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
}

#[test]
fn local_assignment_is_allowed_in_screen_reachable_code() {
    let source = r#"app "render-local"
function label(value) {
  let current = value
  current = current + 1
  return current
}
screen("main") {
  let count = 1
  count = count + 1
  service.display.text(label(count), { x: 1, y: 1 })
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
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
  if (state.selected == 0) {
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
state.selected = state.selected + 1
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
  let next: int = state.count + 1
  if (state.count + 1 < 10) {
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
fn compiles_namespaced_module_function_screen_and_state_contract() {
    let root = unique_test_dir("squidc-modules");
    let app_dir = root.join("app");
    std::fs::create_dir_all(app_dir.join("screens")).unwrap();
    std::fs::write(
        app_dir.join("main.squid"),
        r#"app "module-demo"
import reader from "screens/reader.squid"
state {
  page: int = 1
}

event.on("app.start") {
  reader.openCurrent()
  screen.open(reader.page)
}
"#,
    )
    .unwrap();
    std::fs::write(
        app_dir.join("screens").join("reader.squid"),
        r#"requires state {
  page: int
}

export function openCurrent() {
  debug.print(@page)
}

export screen("page") {
  debug.print(@page)
}
"#,
    )
    .unwrap();

    let output =
        compile_path_with_profile(&app_dir.join("main.squid"), "portable", BuildProfile::Dev);

    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(ir
        .functions
        .iter()
        .any(|function| function.name == "reader.openCurrent"));
    assert!(ir.screens.iter().any(|screen| screen.name == "reader.page"));
    assert!(matches!(
        ir.handlers[0].statements[1],
        IrStatement::ScreenOpen { ref screen } if screen == "reader.page"
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_import_module_lifecycle_and_state_declarations() {
    let root = unique_test_dir("squidc-modules");
    let app_dir = root.join("app");
    std::fs::create_dir_all(app_dir.join("lib")).unwrap();
    std::fs::write(
        app_dir.join("main.squid"),
        r#"app "module-demo"
import bad from "lib/bad.squid"
state {}
screen("main") {}
"#,
    )
    .unwrap();
    std::fs::write(
        app_dir.join("lib").join("bad.squid"),
        r#"state {
  count: int = 0
}

event.on("app.start") {}
"#,
    )
    .unwrap();

    let output =
        compile_path_with_profile(&app_dir.join("main.squid"), "portable", BuildProfile::Dev);

    assert!(!output.ok);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_IMPORT_ONLY_DECL"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_missing_module_state_contract_field() {
    let root = unique_test_dir("squidc-modules");
    let app_dir = root.join("app");
    std::fs::create_dir_all(app_dir.join("lib")).unwrap();
    std::fs::write(
        app_dir.join("main.squid"),
        r#"app "module-demo"
import reader from "lib/reader.squid"
state {}
screen("main") {}
"#,
    )
    .unwrap();
    std::fs::write(
        app_dir.join("lib").join("reader.squid"),
        r#"requires state {
  page: int
}

export function show() {
  debug.print(@page)
}
"#,
    )
    .unwrap();

    let output =
        compile_path_with_profile(&app_dir.join("main.squid"), "portable", BuildProfile::Dev);

    assert!(!output.ok);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_STATE_CONTRACT"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_private_module_exports_and_duplicate_aliases() {
    let root = unique_test_dir("squidc-modules");
    let app_dir = root.join("app");
    std::fs::create_dir_all(app_dir.join("lib")).unwrap();
    std::fs::write(
        app_dir.join("main.squid"),
        r#"app "module-demo"
import reader from "lib/reader.squid"
import reader from "lib/other.squid"
state {}
event.on("app.start") {
  reader.hidden()
}
"#,
    )
    .unwrap();
    std::fs::write(
        app_dir.join("lib").join("reader.squid"),
        r#"function hidden() {}
"#,
    )
    .unwrap();
    std::fs::write(
        app_dir.join("lib").join("other.squid"),
        r#"export function visible() {}
"#,
    )
    .unwrap();

    let output =
        compile_path_with_profile(&app_dir.join("main.squid"), "portable", BuildProfile::Dev);

    assert!(!output.ok);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_DUPLICATE_IMPORT_ALIAS"));
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_UNKNOWN_MODULE_SYMBOL"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn module_call_resolution_only_uses_current_file_import_aliases() {
    let root = unique_test_dir("squidc-modules");
    let app_dir = root.join("app");
    std::fs::create_dir_all(app_dir.join("lib")).unwrap();
    std::fs::write(
        app_dir.join("main.squid"),
        r#"app "module-demo"
import helper from "lib/helper.squid"
state {}
event.on("app.start") {
  futureCapability.doThing()
  helper.visible()
}
"#,
    )
    .unwrap();
    std::fs::write(
        app_dir.join("lib").join("helper.squid"),
        r#"export function visible() {}
"#,
    )
    .unwrap();

    let output =
        compile_path_with_profile(&app_dir.join("main.squid"), "portable", BuildProfile::Dev);

    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(matches!(
        ir.handlers[0].statements[0],
        IrStatement::Call { ref name, .. } if name == "futureCapability.doThing"
    ));
    assert!(matches!(
        ir.handlers[0].statements[1],
        IrStatement::Call { ref name, .. } if name == "helper.visible"
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_import_alias_that_collides_with_reserved_namespace() {
    let root = unique_test_dir("squidc-modules");
    let app_dir = root.join("app");
    std::fs::create_dir_all(app_dir.join("lib")).unwrap();
    std::fs::write(
        app_dir.join("main.squid"),
        r#"app "module-demo"
import file from "lib/helper.squid"
state {}
screen("main") {}
"#,
    )
    .unwrap();
    std::fs::write(
        app_dir.join("lib").join("helper.squid"),
        r#"export function visible() {}
"#,
    )
    .unwrap();

    let output =
        compile_path_with_profile(&app_dir.join("main.squid"), "portable", BuildProfile::Dev);

    assert!(!output.ok);
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_DUPLICATE_IMPORT_ALIAS"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_unknown_top_level_declaration() {
    let source = r#"app "unknown-top-level"
capability "demo"
state {}
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
        .any(|diagnostic| diagnostic.code == "E_UNEXPECTED_TOP_LEVEL"));
}

#[test]
fn compiles_app_entry_without_screen_to_empty_main_screen() {
    let source = r#"app "headless"

event.on("app.start") {
  debug.print("hi")
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.screens.len(), 1);
    assert_eq!(ir.screens[0].name, "main");
    assert_eq!(ir.screens[0].render, "compose");
    assert!(ir.screens[0].statements.is_empty());
    sqbc::encode_sqbc(&ir).expect("synthetic empty main screen should encode");
}

#[test]
fn no_screen_app_still_rejects_unknown_screen_open() {
    let source = r#"app "headless"

event.on("app.start") {
  screen.open("missing")
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
        .any(|diagnostic| diagnostic.code == "E_UNKNOWN_SCREEN"));
    assert!(output
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code != "E_SCREEN_REQUIRED"));
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
        u32::from_le_bytes(sqbc[4..8].try_into().unwrap()) as usize,
        sqbc.len() - 8
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
    assert_eq!(u32::from_le_bytes(sqbc[10..14].try_into().unwrap()), 11);
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
  debug.print("count", state.count)
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
debug.print("event", state.count, led)
  }
}
screen("main") {
  debug {
let x = state.count
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
let x = !false
debug.print("hidden", x)
  }
  state.count = state.count + 1
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
fn parses_indicator_blink_with_default_and_explicit_durations() {
    let source = r#"app "indicator-blink"
event.on("app.start") {
  service.indicator.blink()
  service.indicator.blink(120)
  service.indicator.blink(120, 80)
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert_eq!(ir.handlers[0].statements.len(), 3);
    assert!(matches!(
        &ir.handlers[0].statements[0],
        IrStatement::ServiceIndicatorBlink { on_ms, off_ms }
            if matches!(on_ms, IrExpr::Literal { value } if value == &serde_json::json!(500))
                && matches!(off_ms, IrExpr::Literal { value } if value == &serde_json::json!(500))
    ));
    assert!(matches!(
        &ir.handlers[0].statements[1],
        IrStatement::ServiceIndicatorBlink { on_ms, off_ms }
            if matches!(on_ms, IrExpr::Literal { value } if value == &serde_json::json!(120))
                && matches!(off_ms, IrExpr::Literal { value } if value == &serde_json::json!(500))
    ));
    assert!(matches!(
        &ir.handlers[0].statements[2],
        IrStatement::ServiceIndicatorBlink { on_ms, off_ms }
            if matches!(on_ms, IrExpr::Literal { value } if value == &serde_json::json!(120))
                && matches!(off_ms, IrExpr::Literal { value } if value == &serde_json::json!(80))
    ));
}

#[test]
fn rejects_debug_block_mutations_and_escaping_locals() {
    let source = r#"app "bad-debug"
state { count: int = 1 }
function inspect(value) {
  let outer = 1
  debug {
let x = 2
state.count = 3
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
let x = state.count
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
    assert_eq!(u32::from_le_bytes(sqbc[10..14].try_into().unwrap()), 11);
}

#[test]
fn parses_display_namespace_as_display_service_sugar() {
    let source = r#"app "display-sugar"
screen("main") {
  display.clear("white")
  display.text("Hello", { x: 10, y: 20 })
  display.rect(0, 0, 100, 40, { fillColor: "gray4" })
  display.line(0, 40, 100, 40, { color: "black" })
  service.display.select("status")
  service.display.image("data/icon.bmp", { x: 20, y: 24 })
  service.display.draw("drawable/page", { x: 0, y: 0 })
  service.display.draw("drawable/full-page")
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
    assert!(matches!(
        screen.statements[4],
        IrStatement::DisplaySelect { ref name } if name == "status"
    ));
    assert!(matches!(
        screen.statements[5],
        IrStatement::DisplayImage { ref path, .. } if path == "data/icon.bmp"
    ));
    assert!(matches!(
        screen.statements[6],
        IrStatement::DisplayDraw { .. }
    ));
    assert!(matches!(
        screen.statements[7],
        IrStatement::DisplayDraw { .. }
    ));
    let sqbc = sqbc::encode_sqbc(&ir).expect("display sugar should lower to display bytecode");
    assert!(
        sqbc.windows(2).any(|window| window == [50, 0x36]),
        "expected service.display.draw builtin in SQBC"
    );
}

#[test]
fn compiles_binbook_handle_api_and_default_draw_options() {
    let source = r#"app "binbook-smoke"
state { pageIndex: int = 0 }
screen("main", { render: "stream" }) {
  let opened = binbook.open("books/sample.binbook")
  if (opened.ok) {
    let info = binbook.info(opened.book)
    let page = binbook.readPage(opened.book, state.pageIndex)
    if (page.ok) {
      service.display.draw(page.drawable)
      debug.print("pages", info.pageCount)
    }
  }
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let sqbc = sqbc::encode_sqbc(&output.ir.unwrap()).unwrap();
    assert!(
        sqbc.windows(2).any(|window| window == [50, 0x80]),
        "expected binbook.open builtin in SQBC"
    );
    assert!(
        sqbc.windows(2).any(|window| window == [50, 0x81]),
        "expected binbook.info builtin in SQBC"
    );
    assert!(
        sqbc.windows(2).any(|window| window == [50, 0x82]),
        "expected binbook.readPage builtin in SQBC"
    );
}

#[test]
fn compiles_content_binbook_list_result_call_to_sqbc() {
    let source = r#"app "content-library"

event.on("app.start") {
  let page = content.binbook.list("books", { offset: 0, limit: 8 })
  debug.print(page.ok, page.error, page.warning, page.count, page.hasMore)
  for item in page.items max 8 {
    debug.print(item.name, item.ref, item.size)
  }
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let sqbc = sqbc::encode_sqbc(&output.ir.unwrap()).unwrap();
    assert!(
        sqbc.windows(2).any(|window| window == [50, 0x83]),
        "expected content.binbook.list builtin in SQBC"
    );
}

#[test]
fn parses_display_info_as_display_service_sugar() {
    let source = r#"app "display-info"
event.on("app.start") {
  let info = display.info()
  debug.print(info.ok, info.available, info.width, info.height, info.colorModel)
}
screen("main") {
  let info = service.display.info()
  display.text(info.height, { x: 0, y: 0 })
}
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
        handler.statements[0],
        IrStatement::Let { ref expr, .. }
            if matches!(expr, IrExpr::Call { name, args } if name == "service.display.info" && args.is_empty())
    ));
    let screen = ir
        .screens
        .iter()
        .find(|screen| screen.name == "main")
        .unwrap();
    assert!(matches!(
        screen.statements[0],
        IrStatement::Let { ref expr, .. }
            if matches!(expr, IrExpr::Call { name, args } if name == "service.display.info" && args.is_empty())
    ));
}

#[test]
fn parses_device_bindings_and_rejects_unsafe_paths() {
    let source = r#"app "device-test"
device {
  indicator { use "device/indicator.sqdevice" }
  indicator "external" { use "gpio:GPIO10" }
  display "status" { use "device/status-display.sqdevice" }
  input { use "gpio-button:GPIO9:key.SELECT:activeLow" }
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
                service: "indicator".to_string(),
                binding: "external".to_string(),
                resource: "gpio:GPIO10".to_string(),
            },
            IrDeviceBinding {
                service: "display".to_string(),
                binding: "status".to_string(),
                resource: "device/status-display.sqdevice".to_string(),
            },
            IrDeviceBinding {
                service: "input".to_string(),
                binding: "default".to_string(),
                resource: "gpio-button:GPIO9:key.SELECT:activeLow".to_string(),
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
fn compiles_device_config_result_calls_to_sqbc() {
    let source = r#"app "device-config"

event.on("app.start") {
  let loaded = device.config.load("package:device/indicator.sqdevice")
  let set = device.config.set("mode", "gpio")
  let rebound = device.config.rebind("indicator.default")
  let saved = device.config.save("flash")
  debug.print(loaded.ok, loaded.error, set.ok, rebound.warning, saved.ok)
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    sqbc::encode_sqbc(&output.ir.unwrap()).expect("device config calls should encode");
}

#[test]
fn compiles_power_sleep_and_start_reason_to_sqbc() {
    let source = r#"app "planned-sleep"

event.on("app.start") {
  debug.print(system.startReason())
  service.power.sleep({ wakeAfterMs: 30000 })
}

event.on("power.sleep") {
  state.save()
}

screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.as_ref().unwrap();
    assert!(matches!(
        &ir.handlers[0].statements[0],
        IrStatement::DebugPrint { args }
            if matches!(args.first(), Some(IrExpr::SystemStartReason))
    ));
    assert!(matches!(
        &ir.handlers[0].statements[1],
        IrStatement::ServicePowerSleep { wake_after_ms }
            if matches!(wake_after_ms, IrExpr::Literal { value } if value.as_i64() == Some(30000))
    ));
    sqbc::encode_sqbc(ir).expect("power sleep and start reason should encode");
}

#[test]
fn compiles_file_pick_file_result_call_to_sqbc() {
    let source = r#"app "file-picker"

event.on("app.start") {
  let picked = file.pickFile(".binbook")
  debug.print(picked.ok, picked.error, picked.path)
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    sqbc::encode_sqbc(&output.ir.unwrap()).expect("file pickFile should encode");
}

#[test]
fn compiles_file_read_result_calls_to_sqbc() {
    let source = r#"app "file-read"

event.on("app.start") {
  let text = file.readText("notes.txt")
  let lines = file.readLines("notes.txt", 4)
  debug.print(text.ok, text.error, text.text, lines.ok, lines.error, lines.lines)
}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });

    assert!(output.ok, "{:?}", output.diagnostics);
    sqbc::encode_sqbc(&output.ir.unwrap()).expect("file read calls should encode");
}

#[test]
fn parses_hardware_gpio_calls() {
    let source = r#"app "gpio"
state { led: bool = false }
event.on("app.start") {
  hardware.gpio.write("GPIO8", true)
  state.led = hardware.gpio.read("GPIO8")
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
        IrStatement::StateAssign {
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
  debug.print("tick", state.count)
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
fn parses_dynamic_lifecycle_ids_and_app_install_result() {
    let source = r#"app "ble-bootstrap"
event.on("ble.file.complete", ev) {
  let installed = app.install(ev.upload)
  app.launch(installed.id)
  app.arm(installed.id)
  app.disarm(installed.id)
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
        IrStatement::Let { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[1],
        IrStatement::AppLaunch { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[2],
        IrStatement::AppArm { .. }
    ));
    assert!(matches!(
        ir.handlers[0].statements[3],
        IrStatement::AppDisarm { .. }
    ));
    sqbc::encode_sqbc(&ir).expect("SQBC should encode dynamic lifecycle ids");
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
app.triggers {
  service.timer.after("timer.break", 1500000)
}
event.on("app.exit") {
  state.save()
}
event.on("timer.break") {
  state.ticks = state.ticks + 1
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
    assert_eq!(ir.handlers[1].event, "app.exit");
    assert_eq!(ir.handlers[2].event, "timer.break");
    assert_eq!(ir.triggers.len(), 1);
    assert_eq!(ir.triggers[0].event, "timer.break");
    assert_eq!(ir.triggers[0].interval_ms, 1500000);
    assert!(!ir.triggers[0].repeating);
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
        ir.handlers[2].statements[1],
        IrStatement::AppDisarm { .. }
    ));
}

#[test]
fn parses_app_triggers_as_registration_declarations() {
    let source = r#"app "trigger-demo"
event.on("app.start") {
  app.arm("reminder")
}
app.triggers {
  service.timer.after("timer.break", 1500000)
}
event.on("timer.break") {
  debug.print("break")
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
    assert_eq!(ir.handlers[1].event, "timer.break");
    assert_eq!(ir.triggers.len(), 1);
    assert_eq!(ir.triggers[0].event, "timer.break");
    assert_eq!(ir.triggers[0].interval_ms, 1500000);
    assert!(!ir.triggers[0].repeating);
}

#[test]
fn rejects_handler_code_block_over_vm_chunk_limit() {
    // The VM loads a whole handler/function/screen frame into one 640-byte code
    // chunk, so the compiler must reject a frame whose compiled code exceeds it
    // (otherwise it fails on-device with an opaque error).
    let mut source =
        String::from("app \"big\"\nstate { acc: int = 0 }\nevent.on(\"app.start\") {\n");
    for _ in 0..200 {
        source.push_str("  state.acc = state.acc + 1\n");
    }
    source.push_str("}\nscreen(\"main\") {}\n");
    let output = compile(CompileRequest {
        source,
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let err =
        sqbc::encode_sqbc(&output.ir.unwrap()).expect_err("over-limit app must fail to encode");
    assert!(
        err.message.contains("per-block limit"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn parses_service_ble_start_and_stop_statements() {
    let source = r#"app "ble-install"
event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: {
      complete: "ble.file.complete",
      error: "ble.file.error"
    }
  })
}
event.on("app.exit") {
  service.ble.stop()
}
event.on("ble.file.complete", ev) {
  debug.print(ev.id)
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    // service.ble.start/stop are statements, not triggers.
    assert!(ir.triggers.is_empty());
    assert_eq!(ir.handlers[0].event, "app.start");
    match &ir.handlers[0].statements[0] {
        IrStatement::ServiceBleStart {
            profile,
            id,
            accept,
            events,
        } => {
            assert_eq!(profile, "file-transfer");
            assert_eq!(id, "sqbc-install");
            assert_eq!(accept, &vec![".sqbc".to_string()]);
            assert_eq!(
                events.get("complete").map(String::as_str),
                Some("ble.file.complete")
            );
        }
        other => panic!("expected ServiceBleStart, got {other:?}"),
    }
    assert_eq!(ir.handlers[1].event, "app.exit");
    assert!(matches!(
        ir.handlers[1].statements[0],
        IrStatement::ServiceBleStop
    ));
}

#[test]
fn parses_service_http_start_and_stop_statements() {
    let source = r#"app "http-upload"
event.on("app.start") {
  service.http.start("file-upload", {
    id: "binbook-upload",
    accept: [".binbook"],
    events: {
      complete: "http.file.complete",
      error: "http.file.error"
    }
  })
}
event.on("app.exit") {
  service.http.stop()
}
event.on("http.file.complete", ev) {
  let copied = file.copy(ev.upload, { library: "books", name: ev.name })
  debug.print(copied.ref)
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let ir = output.ir.unwrap();
    assert!(ir.triggers.is_empty());
    assert_eq!(ir.handlers[0].event, "app.start");
    match &ir.handlers[0].statements[0] {
        IrStatement::ServiceHttpStart {
            profile,
            id,
            accept,
            events,
        } => {
            assert_eq!(profile, "file-upload");
            assert_eq!(id, "binbook-upload");
            assert_eq!(accept, &vec![".binbook".to_string()]);
            assert_eq!(
                events.get("complete").map(String::as_str),
                Some("http.file.complete")
            );
        }
        other => panic!("expected ServiceHttpStart, got {other:?}"),
    }
    assert_eq!(ir.handlers[1].event, "app.exit");
    assert!(matches!(
        ir.handlers[1].statements[0],
        IrStatement::ServiceHttpStop
    ));
}

#[test]
fn rejects_service_ble_start_without_id() {
    let source = r#"app "ble-install"
event.on("app.start") {
  service.ble.start("file-transfer", {
    accept: [".sqbc"],
    events: { complete: "ble.file.complete" }
  })
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
        .any(|diagnostic| diagnostic.code == "E_BLE_PROFILE"));
}

#[test]
fn rejects_service_ble_start_without_complete_event_route() {
    let source = r#"app "ble-install"
event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: { error: "ble.file.error" }
  })
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
        .any(|diagnostic| diagnostic.code == "E_BLE_PROFILE"));
}

#[test]
fn rejects_service_ble_start_in_app_triggers() {
    let source = r#"app "ble-install"
app.triggers {
  service.ble.start("file-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: { complete: "ble.file.complete" }
  })
}
event.on("app.start") {}
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
        .any(|diagnostic| diagnostic.code == "E_APP_TRIGGER_STATEMENT"));
}

#[test]
fn encodes_ble_profile_metadata_from_start_statement() {
    let source = r#"app "ble-install"
event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: {
      complete: "ble.file.complete",
      error: "ble.file.error"
    }
  })
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(output.ok, "{:?}", output.diagnostics);
    let bytes = sqbc::encode_sqbc(&output.ir.unwrap()).expect("SQBC should encode");
    let sections = sqbc_sections(&bytes);
    let ble = sections
        .iter()
        .find(|(kind, _, _)| *kind == 10)
        .expect("BLE profile section should be present");
    // count (u16) == 1 profile encoded from the start statement.
    assert!(ble.2 > 2);
}

#[test]
fn rejects_authored_app_arm_handlers_and_foreground_trigger_statements() {
    let old_handler = r#"app "old-arm"
event.on("app.arm") {
  service.timer.after("timer.break", 1500000)
}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: old_handler.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!compiled.ok);
    assert!(compiled
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_APP_ARM_HANDLER"));

    let foreground_statement = r#"app "bad-trigger"
app.triggers {
  app.launch("reader")
}
event.on("app.start") {}
screen("main") {}
"#;
    let compiled = compile(CompileRequest {
        source: foreground_statement.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!compiled.ok);
    assert!(compiled
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "E_APP_TRIGGER_STATEMENT"));
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
  state.selected = state.selected + 1
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
fn reports_duplicate_source_invariants_as_errors() {
    let source = r#"app "first"
app "second"
state {
  count: int = 0
  count: int = 1
}
state {
  other: int = 0
}
device {
  indicator "default" { use "gpio:GPIO8" }
  indicator "default" { use "gpio:GPIO10" }
}
event.on("app.start") {}
event.on("app.start") {}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!output.ok);
    let codes = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"E_DUPLICATE_APP"), "{codes:?}");
    assert!(codes.contains(&"E_DUPLICATE_STATE_BLOCK"), "{codes:?}");
    assert!(codes.contains(&"E_DUPLICATE_STATE_FIELD"), "{codes:?}");
    assert!(codes.contains(&"E_DUPLICATE_DEVICE_BINDING"), "{codes:?}");
    assert!(codes.contains(&"E_DUPLICATE_HANDLER"), "{codes:?}");
}

#[test]
fn reports_duplicate_params_locals_and_trigger_events() {
    let source = r#"app "duplicates"
app.triggers {
  service.timer.every("timer.tick", 1000)
  service.timer.after("timer.tick", 2000)
}
event.on("app.start") {
  let value = 1
  let value = 2
}
event.on("timer.tick") {}
function draw(label, label) {
  debug.print(label)
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!output.ok);
    let codes = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"E_DUPLICATE_TRIGGER_EVENT"), "{codes:?}");
    assert!(codes.contains(&"E_DUPLICATE_LOCAL"), "{codes:?}");
    assert!(codes.contains(&"E_DUPLICATE_PARAM"), "{codes:?}");
}

#[test]
fn reports_trigger_without_handler_and_duplicate_ble_profile_ids() {
    let source = r#"app "routes"
app.triggers {
  service.timer.every("timer.missing", 1000)
}
event.on("app.start") {
  service.ble.start({
    profile: "file-transfer",
    id: "inbox",
    accept: [".txt"],
    events: { complete: "ble.file.complete" }
  })
  service.ble.start({
    profile: "file-transfer",
    id: "inbox",
    accept: [".bin"],
    events: { complete: "ble.file.complete" }
  })
}
event.on("ble.file.complete", ev) {
  debug.print(ev.path)
}
screen("main") {}
"#;
    let output = compile(CompileRequest {
        source: source.to_string(),
        target_id: PORTABLE_TARGET_ID.to_string(),
    });
    assert!(!output.ok);
    let codes = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"E_TRIGGER_HANDLER"), "{codes:?}");
    assert!(codes.contains(&"E_DUPLICATE_BLE_PROFILE_ID"), "{codes:?}");
}

#[test]
fn parses_result_record_field_access_and_unary_not() {
    let source = r#"app "result-records" target "xteink-x4"
state { failed: bool = false }
event.on("app.start") {
  let result = file.pickFile(".txt")
  if (!result.ok) {
state.failed = true
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
    assert!(matches!(expr, IrExpr::Call { name, .. } if name == "file.pickFile"));
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
fn compiles_service_wifi_station_profile_calls_and_wifi_sugar() {
    let source = r#"app "wifi-station"
state {}

event.on("app.start") {
  let connect = service.wifi.connect("dev")
  let status = wifi.status()
  debug.print(connect.ok, status.profile, status.connected, status.disconnectReason)
  wifi.disconnect()
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
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.connect"
    ));
    assert!(matches!(
        &handler.statements[1],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.status"
    ));
    sqbc::encode_sqbc(&ir).unwrap();
}

#[test]
fn compiles_service_wifi_scan_and_wifi_sugar_to_same_builtin() {
    let source = r#"app "wifi-scan"
state {}

event.on("app.start") {
  let serviceScan = service.wifi.scan()
  let sugarScan = wifi.scan()
  debug.print(serviceScan.ok, sugarScan.count)
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
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.scan"
    ));
    assert!(matches!(
        &handler.statements[1],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.scan"
    ));
    sqbc::encode_sqbc(&ir).expect("wifi scan should lower to bytecode");
}

#[test]
fn compiles_async_wifi_operation_calls_and_wifi_sugar() {
    let source = r#"app "wifi-async"
state {}

event.on("app.start") {
  let scan = wifi.scan()
  let op = service.wifi.operation()
  let row = wifi.scanNetwork(0)
  let result = service.wifi.result()
  let cancel = wifi.cancel()
  debug.print(scan.state, op.active, row.ok, result.ready, cancel.cancelled)
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
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.scan"
    ));
    assert!(matches!(
        &handler.statements[1],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.operation"
    ));
    assert!(matches!(
        &handler.statements[2],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.scanNetwork"
    ));
    assert!(matches!(
        &handler.statements[3],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.result"
    ));
    assert!(matches!(
        &handler.statements[4],
        IrStatement::Let { expr: IrExpr::Call { name, .. }, .. } if name == "service.wifi.cancel"
    ));
    sqbc::encode_sqbc(&ir).expect("async wifi calls should lower to bytecode");
}

#[test]
fn warns_when_fallible_result_is_ignored() {
    let source = r#"app "ignored-result" target "xteink-x4"
event.on("app.start") {
  file.pickFile(".txt")
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
