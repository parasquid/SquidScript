use crate::{
    ast::AstRoot,
    diagnostic::{error, warning, Diagnostic},
    ir::{IrExpr, IrStatement},
    profile::BuildProfile,
};
use std::collections::{BTreeMap, BTreeSet};

fn is_fallible_builtin(name: &str) -> bool {
    matches!(
        name,
        "file.pickFile"
            | "file.readText"
            | "file.readLines"
            | "service.wifi.connect"
            | "service.wifi.disconnect"
            | "service.wifi.scan"
            | "service.wifi.operation"
            | "service.wifi.result"
            | "service.wifi.cancel"
            | "service.wifi.scanNetwork"
            | "service.wifi.startAP"
            | "service.wifi.stopAP"
    )
}

pub(crate) fn validate_semantics(
    ast: &AstRoot,
    _profile: BuildProfile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut screen_names = BTreeSet::new();
    let state_names = ast
        .state
        .as_ref()
        .map(|state| {
            state
                .values
                .iter()
                .map(|value| value.name.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    validate_state_builtin_shadowing(ast, diagnostics);
    let function_map = ast
        .functions
        .iter()
        .map(|function| (function.name.clone(), function.statements.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let impure_functions = collect_render_impure_functions(&function_map);
    let mut function_names = BTreeSet::new();
    for function in &ast.functions {
        if !function_names.insert(function.name.clone()) {
            diagnostics.push(error(
                "E_DUPLICATE_FUNCTION",
                "function names must be unique",
                function.span.start,
                function.span.end,
            ));
        }
        validate_shadowing(
            &function.params,
            &state_names,
            function.span.start,
            function.span.end,
            diagnostics,
        );
    }
    for screen in &ast.screens {
        if !screen_names.insert(screen.name.clone()) {
            diagnostics.push(error(
                "E_DUPLICATE_SCREEN",
                "screen names must be unique",
                screen.span.start,
                screen.span.end,
            ));
        }
        if screen.render != "compose" && screen.render != "stream" {
            diagnostics.push(error(
                "E_RENDER_POLICY",
                "screen render policy must be compose or stream",
                screen.span.start,
                screen.span.end,
            ));
        }
        validate_screen_statements(
            &screen.statements,
            screen.span.start,
            screen.span.end,
            &impure_functions,
            diagnostics,
        );
        validate_ignored_fallible_results(
            &screen.statements,
            screen.span.start,
            screen.span.end,
            diagnostics,
        );
        validate_debug_blocks(
            &screen.statements,
            &state_names,
            &[],
            screen.span.start,
            screen.span.end,
            diagnostics,
        );
    }

    for handler in &ast.handlers {
        if handler.event == "app.arm" {
            diagnostics.push(error(
                "E_APP_ARM_HANDLER",
                "use app.triggers for trigger registration; event.on(\"app.arm\") is not supported",
                handler.span.start,
                handler.span.end,
            ));
        }
        validate_ignored_fallible_results(
            &handler.statements,
            handler.span.start,
            handler.span.end,
            diagnostics,
        );
        validate_handler_statements(
            &handler.statements,
            handler.span.start,
            handler.span.end,
            &screen_names,
            diagnostics,
        );
        validate_debug_blocks(
            &handler.statements,
            &state_names,
            &handler.param.iter().cloned().collect::<Vec<_>>(),
            handler.span.start,
            handler.span.end,
            diagnostics,
        );
    }
    for trigger_block in &ast.trigger_blocks {
        validate_trigger_statements(
            &trigger_block.statements,
            trigger_block.span.start,
            trigger_block.span.end,
            diagnostics,
        );
    }
    for function in &ast.functions {
        validate_ignored_fallible_results(
            &function.statements,
            function.span.start,
            function.span.end,
            diagnostics,
        );
        validate_screen_references(
            &function.statements,
            function.span.start,
            function.span.end,
            &screen_names,
            diagnostics,
        );
        validate_debug_blocks(
            &function.statements,
            &state_names,
            &function.params,
            function.span.start,
            function.span.end,
            diagnostics,
        );
    }

    validate_names(ast, &state_names, diagnostics);
}

fn validate_state_builtin_shadowing(ast: &AstRoot, diagnostics: &mut Vec<Diagnostic>) {
    let Some(state) = ast.state.as_ref() else {
        return;
    };
    for value in &state.values {
        if matches!(value.name.as_str(), "load" | "save" | "reset") {
            diagnostics.push(warning(
                "W_STATE_BUILTIN_SHADOW",
                "state field shares a name with a state service method; state.<field> reads the field and state.<field>() calls the service method",
                state.span.start,
                state.span.end,
            ));
        }
    }
}

fn validate_trigger_statements(
    statements: &[IrStatement],
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::ServiceTimerEvery { interval_ms, .. } => {
                validate_trigger_interval(interval_ms, start, end, diagnostics);
            }
            IrStatement::ServiceTimerAfter { delay_ms, .. } => {
                validate_trigger_interval(delay_ms, start, end, diagnostics);
            }
            _ => diagnostics.push(error(
                "E_APP_TRIGGER_STATEMENT",
                "app.triggers may only declare timer trigger registrations",
                start,
                end,
            )),
        }
    }
}

fn validate_ble_start(
    profile: &str,
    id: &str,
    accept: &[String],
    events: &BTreeMap<String, String>,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut invalid = false;
    if profile != "object-transfer" {
        invalid = true;
    }
    if id.is_empty() {
        invalid = true;
    }
    if accept.is_empty() || !accept.iter().all(|extension| extension.starts_with('.')) {
        invalid = true;
    }
    if events.is_empty() || events.values().any(|event| event.is_empty()) {
        invalid = true;
    }
    if invalid {
        diagnostics.push(error(
            "E_BLE_PROFILE",
            "service.ble.start requires profile object-transfer, a non-empty id, accepted extensions, and event routes",
            start,
            end,
        ));
    }
}

fn validate_trigger_interval(
    expr: &IrExpr,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let valid = match expr {
        IrExpr::Literal { value } => value
            .as_i64()
            .is_some_and(|value| value > 0 && i32::try_from(value).is_ok()),
        _ => false,
    };
    if !valid {
        diagnostics.push(error(
            "E_APP_TRIGGER_STATEMENT",
            "app.triggers timer intervals must be positive integer literals",
            start,
            end,
        ));
    }
}

fn validate_names(
    ast: &AstRoot,
    state_names: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for handler in &ast.handlers {
        let mut visible = handler.param.iter().cloned().collect::<BTreeSet<_>>();
        validate_statement_names(
            &handler.statements,
            state_names,
            &mut visible,
            handler.span.start,
            handler.span.end,
            diagnostics,
        );
    }
    for trigger_block in &ast.trigger_blocks {
        let mut visible = BTreeSet::new();
        validate_statement_names(
            &trigger_block.statements,
            state_names,
            &mut visible,
            trigger_block.span.start,
            trigger_block.span.end,
            diagnostics,
        );
    }
    for function in &ast.functions {
        let mut visible = function.params.iter().cloned().collect::<BTreeSet<_>>();
        validate_statement_names(
            &function.statements,
            state_names,
            &mut visible,
            function.span.start,
            function.span.end,
            diagnostics,
        );
    }
    for screen in &ast.screens {
        let mut visible = BTreeSet::new();
        validate_statement_names(
            &screen.statements,
            state_names,
            &mut visible,
            screen.span.start,
            screen.span.end,
            diagnostics,
        );
    }
}

fn validate_shadowing(
    names: &[String],
    state_names: &BTreeSet<String>,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for name in names {
        warn_if_state_shadow(name, state_names, start, end, diagnostics);
    }
}

fn warn_if_state_shadow(
    name: &str,
    state_names: &BTreeSet<String>,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if state_names.contains(name) {
        diagnostics.push(warning(
            "W_STATE_SHADOW",
            "local, parameter, or loop variable shadows a state field; use state.<field> or @field for persistent state",
            start,
            end,
        ));
    }
}

fn validate_statement_names(
    statements: &[IrStatement],
    state_names: &BTreeSet<String>,
    visible: &mut BTreeSet<String>,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::Let { name, expr } => {
                validate_expr_names(expr, state_names, visible, start, end, diagnostics);
                warn_if_state_shadow(name, state_names, start, end, diagnostics);
                visible.insert(name.clone());
            }
            IrStatement::Assign { name, expr } => {
                validate_expr_names(expr, state_names, visible, start, end, diagnostics);
                if !visible.contains(name) {
                    diagnostics.push(undeclared_variable(name, state_names, start, end));
                }
            }
            IrStatement::StateAssign { name, expr } => {
                validate_expr_names(expr, state_names, visible, start, end, diagnostics);
                if !state_names.contains(name) {
                    diagnostics.push(missing_state_field(name, start, end));
                }
            }
            IrStatement::If {
                condition,
                then_statements,
                else_statements,
            } => {
                validate_expr_names(condition, state_names, visible, start, end, diagnostics);
                let mut then_visible = visible.clone();
                validate_statement_names(
                    then_statements,
                    state_names,
                    &mut then_visible,
                    start,
                    end,
                    diagnostics,
                );
                let mut else_visible = visible.clone();
                validate_statement_names(
                    else_statements,
                    state_names,
                    &mut else_visible,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::Repeat { count, statements } => {
                validate_expr_names(count, state_names, visible, start, end, diagnostics);
                let mut nested_visible = visible.clone();
                validate_statement_names(
                    statements,
                    state_names,
                    &mut nested_visible,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::For {
                item,
                list,
                max,
                statements,
            } => {
                validate_expr_names(list, state_names, visible, start, end, diagnostics);
                if let Some(max) = max {
                    validate_expr_names(max, state_names, visible, start, end, diagnostics);
                }
                warn_if_state_shadow(item, state_names, start, end, diagnostics);
                let mut nested_visible = visible.clone();
                nested_visible.insert(item.clone());
                validate_statement_names(
                    statements,
                    state_names,
                    &mut nested_visible,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::Return { expr } => {
                if let Some(expr) = expr {
                    validate_expr_names(expr, state_names, visible, start, end, diagnostics);
                }
            }
            IrStatement::Call { args, .. } | IrStatement::DebugPrint { args } => {
                for arg in args {
                    validate_expr_names(arg, state_names, visible, start, end, diagnostics);
                }
            }
            IrStatement::DebugBlock { statements } => {
                let mut nested_visible = visible.clone();
                validate_statement_names(
                    statements,
                    state_names,
                    &mut nested_visible,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::ServiceTimerEvery { interval_ms, .. } => {
                validate_expr_names(interval_ms, state_names, visible, start, end, diagnostics);
            }
            IrStatement::ServiceTimerAfter { delay_ms, .. } => {
                validate_expr_names(delay_ms, state_names, visible, start, end, diagnostics);
            }
            IrStatement::AppInstall { file_ref, .. } => {
                validate_expr_names(file_ref, state_names, visible, start, end, diagnostics);
            }
            IrStatement::ServiceBleStart {
                profile,
                id,
                accept,
                events,
            } => {
                validate_ble_start(profile, id, accept, events, start, end, diagnostics);
            }
            IrStatement::ServiceBleStop => {}
            IrStatement::ServicePowerSleep { wake_after_ms } => {
                validate_expr_names(wake_after_ms, state_names, visible, start, end, diagnostics);
            }
            IrStatement::HardwareGpioWrite { value, .. } => {
                validate_expr_names(value, state_names, visible, start, end, diagnostics);
            }
            IrStatement::ServiceIndicatorWrite { value } => {
                validate_expr_names(value, state_names, visible, start, end, diagnostics);
            }
            IrStatement::ServiceIndicatorBlink { on_ms, off_ms } => {
                validate_expr_names(on_ms, state_names, visible, start, end, diagnostics);
                validate_expr_names(off_ms, state_names, visible, start, end, diagnostics);
            }
            IrStatement::DisplayText { text, .. } => {
                validate_expr_names(text, state_names, visible, start, end, diagnostics);
            }
            IrStatement::DisplayDraw { drawable, .. } => {
                validate_expr_names(drawable, state_names, visible, start, end, diagnostics);
            }
            IrStatement::StateLoad
            | IrStatement::StateSave
            | IrStatement::StateReset
            | IrStatement::ScreenOpen { .. }
            | IrStatement::ScreenRefresh
            | IrStatement::AppExit
            | IrStatement::AppLaunch { .. }
            | IrStatement::AppArm { .. }
            | IrStatement::AppDisarm { .. }
            | IrStatement::HardwareGpioToggle { .. }
            | IrStatement::ServiceIndicatorToggle
            | IrStatement::ServiceIndicatorBreathe
            | IrStatement::DisplayClear { .. }
            | IrStatement::DisplayRect { .. }
            | IrStatement::DisplayLine { .. }
            | IrStatement::DisplaySelect { .. }
            | IrStatement::DisplayImage { .. } => {}
        }
    }
}

fn validate_expr_names(
    expr: &IrExpr,
    state_names: &BTreeSet<String>,
    visible: &BTreeSet<String>,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::HardwareGpioRead { .. }
        | IrExpr::ServiceIndicatorRead
        | IrExpr::SystemMemory
        | IrExpr::SystemStartReason
        | IrExpr::SystemStorage { .. } => {}
        IrExpr::State { name } => {
            if !state_names.contains(name) {
                diagnostics.push(missing_state_field(name, start, end));
            }
        }
        IrExpr::Variable { name } => {
            if !visible.contains(name) {
                diagnostics.push(undeclared_variable(name, state_names, start, end));
            }
        }
        IrExpr::Binary { left, right, .. } => {
            validate_expr_names(left, state_names, visible, start, end, diagnostics);
            validate_expr_names(right, state_names, visible, start, end, diagnostics);
        }
        IrExpr::Unary { expr, .. } => {
            validate_expr_names(expr, state_names, visible, start, end, diagnostics);
        }
        IrExpr::Field { target, .. } => {
            validate_expr_names(target, state_names, visible, start, end, diagnostics);
        }
        IrExpr::Call { args, .. } => {
            for arg in args {
                validate_expr_names(arg, state_names, visible, start, end, diagnostics);
            }
        }
    }
}

fn undeclared_variable(
    name: &str,
    state_names: &BTreeSet<String>,
    start: usize,
    end: usize,
) -> Diagnostic {
    let message = if state_names.contains(name) {
        format!("undeclared local variable {name}; persistent state must be accessed as state.{name} or @{name}")
    } else {
        format!("undeclared local variable {name}; declare it with let before assignment or use")
    };
    error("E_UNDECLARED_VARIABLE", message, start, end)
}

fn missing_state_field(name: &str, start: usize, end: usize) -> Diagnostic {
    error(
        "E_MISSING_STATE_FIELD",
        format!("state field {name} is not declared in the state block"),
        start,
        end,
    )
}

fn validate_debug_blocks(
    statements: &[IrStatement],
    state_names: &std::collections::BTreeSet<String>,
    initial_locals: &[String],
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut visible_locals = initial_locals
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for statement in statements {
        match statement {
            IrStatement::Let { name, .. } => {
                visible_locals.insert(name.clone());
            }
            IrStatement::DebugBlock { statements } => {
                let mut debug_locals = std::collections::BTreeSet::new();
                collect_debug_local_names(statements, &mut debug_locals);
                validate_debug_block_statements(
                    statements,
                    state_names,
                    &visible_locals,
                    &debug_locals,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::If {
                then_statements,
                else_statements,
                ..
            } => {
                let locals = visible_locals.iter().cloned().collect::<Vec<_>>();
                validate_debug_blocks(
                    then_statements,
                    state_names,
                    &locals,
                    start,
                    end,
                    diagnostics,
                );
                validate_debug_blocks(
                    else_statements,
                    state_names,
                    &locals,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::Repeat { statements, .. } | IrStatement::For { statements, .. } => {
                let locals = visible_locals.iter().cloned().collect::<Vec<_>>();
                validate_debug_blocks(statements, state_names, &locals, start, end, diagnostics);
            }
            _ => {}
        }
    }

    for (index, statement) in statements.iter().enumerate() {
        if let IrStatement::DebugBlock {
            statements: debug_statements,
        } = statement
        {
            let mut debug_locals = std::collections::BTreeSet::new();
            collect_debug_local_names(debug_statements, &mut debug_locals);
            if statements[index + 1..]
                .iter()
                .any(|statement| statement_uses_any_name(statement, &debug_locals))
            {
                diagnostics.push(error(
                    "E_DEBUG_BLOCK",
                    "variables declared inside debug blocks are not visible after the block",
                    start,
                    end,
                ));
            }
        }
    }
}

fn collect_render_impure_functions(
    function_map: &BTreeMap<String, &[IrStatement]>,
) -> BTreeSet<String> {
    let mut impure = BTreeSet::new();
    loop {
        let before = impure.len();
        for (name, statements) in function_map {
            if statements_are_render_impure(statements, &impure) {
                impure.insert(name.clone());
            }
        }
        if impure.len() == before {
            break;
        }
    }
    impure
}

fn statements_are_render_impure(
    statements: &[IrStatement],
    impure_functions: &BTreeSet<String>,
) -> bool {
    statements
        .iter()
        .any(|statement| statement_is_render_impure(statement, impure_functions))
}

fn statement_is_render_impure(
    statement: &IrStatement,
    impure_functions: &BTreeSet<String>,
) -> bool {
    match statement {
        IrStatement::StateLoad
        | IrStatement::StateSave
        | IrStatement::StateReset
        | IrStatement::ScreenOpen { .. }
        | IrStatement::ScreenRefresh
        | IrStatement::AppExit
        | IrStatement::AppLaunch { .. }
        | IrStatement::AppArm { .. }
        | IrStatement::AppDisarm { .. }
        | IrStatement::AppInstall { .. }
        | IrStatement::ServiceTimerEvery { .. }
        | IrStatement::ServiceTimerAfter { .. }
        | IrStatement::ServiceBleStart { .. }
        | IrStatement::ServiceBleStop
        | IrStatement::ServicePowerSleep { .. }
        | IrStatement::HardwareGpioWrite { .. }
        | IrStatement::HardwareGpioToggle { .. }
        | IrStatement::ServiceIndicatorWrite { .. }
        | IrStatement::ServiceIndicatorToggle
        | IrStatement::ServiceIndicatorBreathe
        | IrStatement::ServiceIndicatorBlink { .. }
        | IrStatement::StateAssign { .. } => true,
        IrStatement::Call { name, .. } => {
            is_fallible_builtin(name) || impure_functions.contains(name)
        }
        IrStatement::If {
            then_statements,
            else_statements,
            ..
        } => {
            statements_are_render_impure(then_statements, impure_functions)
                || statements_are_render_impure(else_statements, impure_functions)
        }
        IrStatement::Repeat { statements, .. }
        | IrStatement::For { statements, .. }
        | IrStatement::DebugBlock { statements } => {
            statements_are_render_impure(statements, impure_functions)
        }
        IrStatement::Assign { .. }
        | IrStatement::Let { .. }
        | IrStatement::Return { .. }
        | IrStatement::DebugPrint { .. }
        | IrStatement::DisplayClear { .. }
        | IrStatement::DisplayText { .. }
        | IrStatement::DisplayRect { .. }
        | IrStatement::DisplayLine { .. }
        | IrStatement::DisplaySelect { .. }
        | IrStatement::DisplayImage { .. }
        | IrStatement::DisplayDraw { .. } => false,
    }
}

fn collect_debug_local_names(
    statements: &[IrStatement],
    debug_locals: &mut std::collections::BTreeSet<String>,
) {
    for statement in statements {
        match statement {
            IrStatement::Let { name, .. } => {
                debug_locals.insert(name.clone());
            }
            IrStatement::If {
                then_statements,
                else_statements,
                ..
            } => {
                collect_debug_local_names(then_statements, debug_locals);
                collect_debug_local_names(else_statements, debug_locals);
            }
            IrStatement::Repeat { statements, .. } => {
                collect_debug_local_names(statements, debug_locals);
            }
            IrStatement::For {
                item, statements, ..
            } => {
                debug_locals.insert(item.clone());
                collect_debug_local_names(statements, debug_locals);
            }
            IrStatement::DebugBlock { statements } => {
                collect_debug_local_names(statements, debug_locals);
            }
            _ => {}
        }
    }
}

fn validate_debug_block_statements(
    statements: &[IrStatement],
    state_names: &std::collections::BTreeSet<String>,
    outer_locals: &std::collections::BTreeSet<String>,
    debug_locals: &std::collections::BTreeSet<String>,
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::Let { expr, .. } => {
                validate_debug_expr(expr, start, end, diagnostics);
            }
            IrStatement::Assign { name, expr } => {
                validate_debug_expr(expr, start, end, diagnostics);
                if !debug_locals.contains(name) {
                    let message = if state_names.contains(name) {
                        "debug blocks must not assign to state"
                    } else if outer_locals.contains(name) {
                        "debug blocks must not assign to outer locals or parameters"
                    } else {
                        "debug blocks may only assign to variables declared inside the same debug block"
                    };
                    diagnostics.push(error("E_DEBUG_BLOCK", message, start, end));
                }
            }
            IrStatement::StateAssign { expr, .. } => {
                validate_debug_expr(expr, start, end, diagnostics);
                diagnostics.push(error(
                    "E_DEBUG_BLOCK",
                    "debug blocks must not assign to state",
                    start,
                    end,
                ));
            }
            IrStatement::DebugPrint { args } => {
                for arg in args {
                    validate_debug_expr(arg, start, end, diagnostics);
                }
            }
            IrStatement::If {
                condition,
                then_statements,
                else_statements,
            } => {
                validate_debug_expr(condition, start, end, diagnostics);
                validate_debug_block_statements(
                    then_statements,
                    state_names,
                    outer_locals,
                    debug_locals,
                    start,
                    end,
                    diagnostics,
                );
                validate_debug_block_statements(
                    else_statements,
                    state_names,
                    outer_locals,
                    debug_locals,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::Repeat { count, statements } => {
                validate_debug_expr(count, start, end, diagnostics);
                validate_debug_block_statements(
                    statements,
                    state_names,
                    outer_locals,
                    debug_locals,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::For {
                list,
                max,
                statements,
                ..
            } => {
                validate_debug_expr(list, start, end, diagnostics);
                if let Some(max) = max {
                    validate_debug_expr(max, start, end, diagnostics);
                }
                validate_debug_block_statements(
                    statements,
                    state_names,
                    outer_locals,
                    debug_locals,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::Call { name, args } => {
                for arg in args {
                    validate_debug_expr(arg, start, end, diagnostics);
                }
                diagnostics.push(error(
                    "E_DEBUG_BLOCK",
                    format!("debug blocks must not call user-defined function {name}"),
                    start,
                    end,
                ));
            }
            IrStatement::DebugBlock { statements } => {
                validate_debug_block_statements(
                    statements,
                    state_names,
                    outer_locals,
                    debug_locals,
                    start,
                    end,
                    diagnostics,
                );
            }
            IrStatement::StateLoad
            | IrStatement::StateSave
            | IrStatement::StateReset
            | IrStatement::ScreenOpen { .. }
            | IrStatement::ScreenRefresh
            | IrStatement::AppExit
            | IrStatement::AppLaunch { .. }
            | IrStatement::AppArm { .. }
            | IrStatement::AppDisarm { .. }
            | IrStatement::AppInstall { .. }
            | IrStatement::ServiceTimerEvery { .. }
            | IrStatement::ServiceTimerAfter { .. }
            | IrStatement::ServiceBleStart { .. }
            | IrStatement::ServiceBleStop
            | IrStatement::ServicePowerSleep { .. }
            | IrStatement::HardwareGpioWrite { .. }
            | IrStatement::HardwareGpioToggle { .. }
            | IrStatement::ServiceIndicatorWrite { .. }
            | IrStatement::ServiceIndicatorToggle
            | IrStatement::ServiceIndicatorBreathe
            | IrStatement::ServiceIndicatorBlink { .. }
            | IrStatement::Return { .. }
            | IrStatement::DisplayClear { .. }
            | IrStatement::DisplayText { .. }
            | IrStatement::DisplayRect { .. }
            | IrStatement::DisplayLine { .. }
            | IrStatement::DisplaySelect { .. }
            | IrStatement::DisplayImage { .. }
            | IrStatement::DisplayDraw { .. } => diagnostics.push(error(
                "E_DEBUG_BLOCK",
                "debug blocks may only contain debug-local setup, read-only expressions, bounded control flow, and debug.print",
                start,
                end,
            )),
        }
    }
}

fn validate_debug_expr(expr: &IrExpr, start: usize, end: usize, diagnostics: &mut Vec<Diagnostic>) {
    match expr {
        IrExpr::Literal { .. }
        | IrExpr::State { .. }
        | IrExpr::Variable { .. }
        | IrExpr::HardwareGpioRead { .. }
        | IrExpr::ServiceIndicatorRead
        | IrExpr::SystemMemory
        | IrExpr::SystemStartReason
        | IrExpr::SystemStorage { .. } => {}
        IrExpr::Binary { left, right, .. } => {
            validate_debug_expr(left, start, end, diagnostics);
            validate_debug_expr(right, start, end, diagnostics);
        }
        IrExpr::Unary { expr, .. } => validate_debug_expr(expr, start, end, diagnostics),
        IrExpr::Field { target, .. } => validate_debug_expr(target, start, end, diagnostics),
        IrExpr::Call { name, args } => {
            for arg in args {
                validate_debug_expr(arg, start, end, diagnostics);
            }
            diagnostics.push(error(
                "E_DEBUG_BLOCK",
                format!("debug blocks must not call user-defined function {name}"),
                start,
                end,
            ));
        }
    }
}

fn statement_uses_any_name(
    statement: &IrStatement,
    names: &std::collections::BTreeSet<String>,
) -> bool {
    match statement {
        IrStatement::Assign { name, expr }
        | IrStatement::StateAssign { name, expr }
        | IrStatement::Let { name, expr } => {
            names.contains(name) || expr_uses_any_name(expr, names)
        }
        IrStatement::If {
            condition,
            then_statements,
            else_statements,
        } => {
            expr_uses_any_name(condition, names)
                || then_statements
                    .iter()
                    .any(|s| statement_uses_any_name(s, names))
                || else_statements
                    .iter()
                    .any(|s| statement_uses_any_name(s, names))
        }
        IrStatement::Repeat { count, statements } => {
            expr_uses_any_name(count, names)
                || statements.iter().any(|s| statement_uses_any_name(s, names))
        }
        IrStatement::For {
            item,
            list,
            max,
            statements,
        } => {
            names.contains(item)
                || expr_uses_any_name(list, names)
                || max
                    .as_ref()
                    .is_some_and(|expr| expr_uses_any_name(expr, names))
                || statements.iter().any(|s| statement_uses_any_name(s, names))
        }
        IrStatement::Return { expr } => expr
            .as_ref()
            .is_some_and(|expr| expr_uses_any_name(expr, names)),
        IrStatement::Call { args, .. } | IrStatement::DebugPrint { args } => {
            args.iter().any(|arg| expr_uses_any_name(arg, names))
        }
        IrStatement::ServiceTimerEvery { interval_ms, .. } => {
            expr_uses_any_name(interval_ms, names)
        }
        IrStatement::ServiceTimerAfter { delay_ms, .. } => expr_uses_any_name(delay_ms, names),
        IrStatement::AppInstall { file_ref, .. } => expr_uses_any_name(file_ref, names),
        IrStatement::ServiceBleStart { .. } | IrStatement::ServiceBleStop => false,
        IrStatement::ServicePowerSleep { wake_after_ms } => {
            expr_uses_any_name(wake_after_ms, names)
        }
        IrStatement::HardwareGpioWrite { value, .. } => expr_uses_any_name(value, names),
        IrStatement::ServiceIndicatorWrite { value } => expr_uses_any_name(value, names),
        IrStatement::ServiceIndicatorBlink { on_ms, off_ms } => {
            expr_uses_any_name(on_ms, names) || expr_uses_any_name(off_ms, names)
        }
        IrStatement::DisplayText { text, .. } => expr_uses_any_name(text, names),
        IrStatement::DisplayDraw { drawable, .. } => expr_uses_any_name(drawable, names),
        IrStatement::DebugBlock { .. }
        | IrStatement::StateLoad
        | IrStatement::StateSave
        | IrStatement::StateReset
        | IrStatement::ScreenOpen { .. }
        | IrStatement::ScreenRefresh
        | IrStatement::AppExit
        | IrStatement::AppLaunch { .. }
        | IrStatement::AppArm { .. }
        | IrStatement::AppDisarm { .. }
        | IrStatement::HardwareGpioToggle { .. }
        | IrStatement::ServiceIndicatorToggle
        | IrStatement::ServiceIndicatorBreathe
        | IrStatement::DisplayClear { .. }
        | IrStatement::DisplayRect { .. }
        | IrStatement::DisplayLine { .. }
        | IrStatement::DisplaySelect { .. }
        | IrStatement::DisplayImage { .. } => false,
    }
}

fn expr_uses_any_name(expr: &IrExpr, names: &std::collections::BTreeSet<String>) -> bool {
    match expr {
        IrExpr::State { name } | IrExpr::Variable { name } => names.contains(name),
        IrExpr::Binary { left, right, .. } => {
            expr_uses_any_name(left, names) || expr_uses_any_name(right, names)
        }
        IrExpr::Unary { expr, .. } => expr_uses_any_name(expr, names),
        IrExpr::Field { target, .. } => expr_uses_any_name(target, names),
        IrExpr::Call { args, .. } => args.iter().any(|arg| expr_uses_any_name(arg, names)),
        IrExpr::Literal { .. }
        | IrExpr::HardwareGpioRead { .. }
        | IrExpr::ServiceIndicatorRead
        | IrExpr::SystemMemory
        | IrExpr::SystemStartReason
        | IrExpr::SystemStorage { .. } => false,
    }
}

fn validate_ignored_fallible_results(
    statements: &[IrStatement],
    start: usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::Call { name, .. } if is_fallible_builtin(name) => {
                diagnostics.push(warning(
                    "W_IGNORED_RESULT",
                    "fallible API result should be checked",
                    start,
                    end,
                ));
            }
            IrStatement::If {
                then_statements,
                else_statements,
                ..
            } => {
                validate_ignored_fallible_results(then_statements, start, end, diagnostics);
                validate_ignored_fallible_results(else_statements, start, end, diagnostics);
            }
            IrStatement::Repeat { statements, .. } | IrStatement::For { statements, .. } => {
                validate_ignored_fallible_results(statements, start, end, diagnostics);
            }
            IrStatement::DebugBlock { statements } => {
                validate_ignored_fallible_results(statements, start, end, diagnostics);
            }
            _ => {}
        }
    }
}

fn validate_screen_statements(
    statements: &[IrStatement],
    start: usize,
    end: usize,
    impure_functions: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::StateLoad
            | IrStatement::StateSave
            | IrStatement::ScreenOpen { .. }
            | IrStatement::ScreenRefresh
            | IrStatement::AppExit
            | IrStatement::AppLaunch { .. }
            | IrStatement::AppArm { .. }
            | IrStatement::AppDisarm { .. }
            | IrStatement::AppInstall { .. }
            | IrStatement::ServiceTimerEvery { .. }
            | IrStatement::ServiceTimerAfter { .. }
            | IrStatement::ServicePowerSleep { .. }
            | IrStatement::HardwareGpioWrite { .. }
            | IrStatement::HardwareGpioToggle { .. }
            | IrStatement::ServiceIndicatorWrite { .. }
            | IrStatement::ServiceIndicatorToggle
            | IrStatement::ServiceIndicatorBreathe
            | IrStatement::ServiceIndicatorBlink { .. }
            | IrStatement::StateAssign { .. } => {
                diagnostics.push(error(
                    "E_RENDER_PURITY",
                    "screen bodies must not directly mutate state or app lifecycle",
                    start,
                    end,
                ));
            }
            IrStatement::Call { name, .. } if is_fallible_builtin(name) => {
                diagnostics.push(error(
                    "E_RENDER_PURITY",
                    "screen bodies must not call fallible platform APIs",
                    start,
                    end,
                ));
            }
            IrStatement::Call { name, .. } if impure_functions.contains(name) => {
                diagnostics.push(error(
                    "E_RENDER_PURITY",
                    "screen bodies must not call functions that mutate state or app lifecycle",
                    start,
                    end,
                ));
            }
            IrStatement::If {
                then_statements,
                else_statements,
                ..
            } => {
                validate_screen_statements(
                    then_statements,
                    start,
                    end,
                    impure_functions,
                    diagnostics,
                );
                validate_screen_statements(
                    else_statements,
                    start,
                    end,
                    impure_functions,
                    diagnostics,
                );
            }
            IrStatement::Repeat { statements, .. } | IrStatement::For { statements, .. } => {
                validate_screen_statements(statements, start, end, impure_functions, diagnostics);
            }
            IrStatement::DebugBlock { statements } => {
                validate_screen_statements(statements, start, end, impure_functions, diagnostics);
            }
            _ => {}
        }
    }
}

fn validate_handler_statements(
    statements: &[IrStatement],
    start: usize,
    end: usize,
    screen_names: &std::collections::BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::DisplayClear { .. }
            | IrStatement::DisplayText { .. }
            | IrStatement::DisplayRect { .. }
            | IrStatement::DisplayLine { .. }
            | IrStatement::DisplaySelect { .. }
            | IrStatement::DisplayImage { .. }
            | IrStatement::DisplayDraw { .. } => {
                diagnostics.push(error(
                    "E_DISPLAY_OUTSIDE_SCREEN",
                    "display calls are only valid while rendering a screen",
                    start,
                    end,
                ));
            }
            IrStatement::If {
                then_statements,
                else_statements,
                ..
            } => {
                validate_handler_statements(then_statements, start, end, screen_names, diagnostics);
                validate_handler_statements(else_statements, start, end, screen_names, diagnostics);
            }
            IrStatement::Repeat { statements, .. } | IrStatement::For { statements, .. } => {
                validate_handler_statements(statements, start, end, screen_names, diagnostics);
            }
            IrStatement::DebugBlock { statements } => {
                validate_handler_statements(statements, start, end, screen_names, diagnostics);
            }
            _ => {}
        }
    }
    validate_screen_references(statements, start, end, screen_names, diagnostics);
}

fn validate_screen_references(
    statements: &[IrStatement],
    start: usize,
    end: usize,
    screen_names: &std::collections::BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::ScreenOpen { screen } if !screen_names.contains(screen) => {
                diagnostics.push(error(
                    "E_UNKNOWN_SCREEN",
                    "screen.open references an unknown screen",
                    start,
                    end,
                ));
            }
            IrStatement::If {
                then_statements,
                else_statements,
                ..
            } => {
                validate_screen_references(then_statements, start, end, screen_names, diagnostics);
                validate_screen_references(else_statements, start, end, screen_names, diagnostics);
            }
            IrStatement::Repeat { statements, .. } | IrStatement::For { statements, .. } => {
                validate_screen_references(statements, start, end, screen_names, diagnostics);
            }
            IrStatement::DebugBlock { statements } => {
                validate_screen_references(statements, start, end, screen_names, diagnostics);
            }
            _ => {}
        }
    }
}
