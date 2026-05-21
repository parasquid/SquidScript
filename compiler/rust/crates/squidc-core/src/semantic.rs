use crate::{
    ast::AstRoot,
    diagnostic::{error, warning, Diagnostic},
    ir::{IrExpr, IrStatement},
    profile::BuildProfile,
};

fn is_fallible_builtin(name: &str) -> bool {
    matches!(
        name,
        "content.pickFile"
            | "content.readText"
            | "content.readLines"
            | "data.read"
            | "library.list"
            | "library.volumes"
            | "library.mkdir"
            | "library.rename"
            | "library.move"
            | "library.delete"
            | "library.installUpload"
            | "binbook.open"
            | "binbook.inspect"
            | "wifi.connect"
            | "wifi.disconnect"
            | "wifi.scan"
            | "wifi.startAP"
            | "wifi.stopAP"
            | "wifi.setIP"
            | "wifi.setAPIP"
            | "wifi.setHostname"
            | "wifi.openSetup"
            | "httpServer.start"
            | "httpServer.stop"
            | "httpServer.poll"
            | "bleTransfer.start"
            | "bleTransfer.stop"
            | "bleTransfer.poll"
    )
}

pub(crate) fn validate_semantics(
    ast: &AstRoot,
    _profile: BuildProfile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut screen_names = std::collections::BTreeSet::new();
    let state_names = ast
        .state
        .as_ref()
        .map(|state| {
            state
                .values
                .iter()
                .map(|value| value.name.clone())
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
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

    let mut function_names = std::collections::BTreeSet::new();
    for function in &ast.functions {
        if !function_names.insert(function.name.clone()) {
            diagnostics.push(error(
                "E_DUPLICATE_FUNCTION",
                "function names must be unique",
                function.span.start,
                function.span.end,
            ));
        }
    }

    for handler in &ast.handlers {
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
            &[],
            handler.span.start,
            handler.span.end,
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
            | IrStatement::ServiceTimerEvery { .. }
            | IrStatement::ServiceTimerAfter { .. }
            | IrStatement::HardwareGpioWrite { .. }
            | IrStatement::HardwareGpioToggle { .. }
            | IrStatement::ServiceIndicatorWrite { .. }
            | IrStatement::ServiceIndicatorToggle
            | IrStatement::Return { .. }
            | IrStatement::DisplayClear { .. }
            | IrStatement::DisplayText { .. }
            | IrStatement::DisplayRect { .. }
            | IrStatement::DisplayLine { .. } => diagnostics.push(error(
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
        | IrExpr::HardwareGpioRead { .. }
        | IrExpr::ServiceIndicatorRead
        | IrExpr::SystemMemory
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
        IrStatement::Assign { name, expr } | IrStatement::Let { name, expr } => {
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
        IrStatement::HardwareGpioWrite { value, .. } => expr_uses_any_name(value, names),
        IrStatement::ServiceIndicatorWrite { value } => expr_uses_any_name(value, names),
        IrStatement::DisplayText { text, .. } => expr_uses_any_name(text, names),
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
        | IrStatement::DisplayClear { .. }
        | IrStatement::DisplayRect { .. }
        | IrStatement::DisplayLine { .. } => false,
    }
}

fn expr_uses_any_name(expr: &IrExpr, names: &std::collections::BTreeSet<String>) -> bool {
    match expr {
        IrExpr::State { name } => names.contains(name),
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
            | IrStatement::ServiceTimerEvery { .. }
            | IrStatement::ServiceTimerAfter { .. }
            | IrStatement::HardwareGpioWrite { .. }
            | IrStatement::HardwareGpioToggle { .. }
            | IrStatement::ServiceIndicatorWrite { .. }
            | IrStatement::ServiceIndicatorToggle
            | IrStatement::Assign { .. } => {
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
            IrStatement::If {
                then_statements,
                else_statements,
                ..
            } => {
                validate_screen_statements(then_statements, start, end, diagnostics);
                validate_screen_statements(else_statements, start, end, diagnostics);
            }
            IrStatement::Repeat { statements, .. } | IrStatement::For { statements, .. } => {
                validate_screen_statements(statements, start, end, diagnostics);
            }
            IrStatement::DebugBlock { statements } => {
                validate_screen_statements(statements, start, end, diagnostics);
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
            | IrStatement::DisplayLine { .. } => {
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
