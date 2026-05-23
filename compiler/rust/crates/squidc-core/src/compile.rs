use crate::{
    diagnostic::{error, Diagnostic},
    ir::{default_state_store, IrApp, IrFunction, IrHandler, IrProgram, IrScreen},
    parser::parse,
    profile::{BuildProfile, PORTABLE_TARGET_ID},
    semantic::validate_semantics,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileRequest {
    pub source: String,
    pub target_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileResponse {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub ir: Option<IrProgram>,
}

pub fn compile(request: CompileRequest) -> CompileResponse {
    compile_with_profile(request, BuildProfile::Dev)
}

pub fn compile_with_profile(request: CompileRequest, profile: BuildProfile) -> CompileResponse {
    let mut parsed = parse(&request.source);
    let ast = parsed.ast.clone();

    if ast.app.is_none() {
        parsed.diagnostics.push(error(
            "E_APP_REQUIRED",
            "expected app declaration",
            0,
            request.source.len().min(1),
        ));
    }
    if ast.screens.is_empty() {
        parsed.diagnostics.push(error(
            "E_SCREEN_REQUIRED",
            "expected at least one screen declaration",
            0,
            request.source.len().min(1),
        ));
    }
    if let Some(target) = ast.app.as_ref().and_then(|app| app.target.as_ref()) {
        if request.target_id != PORTABLE_TARGET_ID && target != &request.target_id {
            parsed.diagnostics.push(error(
                "E_TARGET_MISMATCH",
                "source target does not match selected compatibility target",
                0,
                request.source.len().min(1),
            ));
        }
    }
    validate_semantics(&ast, profile, &mut parsed.diagnostics);

    let ok = parsed.diagnostics.iter().all(|d| d.severity != "error");
    let ir = if ok {
        let app = ast.app.expect("app exists after validation");
        let mut trigger_statements = Vec::new();
        for trigger_block in ast.trigger_blocks {
            trigger_statements.extend(trigger_block.statements);
        }
        let handlers = ast
            .handlers
            .into_iter()
            .map(|handler| IrHandler {
                event: handler.event,
                preload: handler.preload,
                statements: handler.statements,
            })
            .chain((!trigger_statements.is_empty()).then_some(IrHandler {
                event: "app.arm".to_string(),
                preload: false,
                statements: trigger_statements,
            }))
            .collect();
        let functions = ast
            .functions
            .into_iter()
            .map(|function| IrFunction {
                name: function.name,
                params: function.params,
                statements: function.statements,
            })
            .collect();
        let state_store = ast
            .state
            .as_ref()
            .map(|state| state.store.clone())
            .unwrap_or_else(default_state_store);
        Some(IrProgram {
            format: "squidscript-ir".to_string(),
            app: IrApp {
                id: app.id,
                name: app.name,
                target: app.target.unwrap_or_else(|| request.target_id),
            },
            state_store,
            device_bindings: ast.device_bindings,
            state: ast.state.map(|state| state.values).unwrap_or_default(),
            functions,
            handlers,
            screens: ast
                .screens
                .into_iter()
                .map(|screen| IrScreen {
                    name: screen.name,
                    render: screen.render,
                    statements: screen.statements,
                })
                .collect(),
        })
    } else {
        None
    };

    CompileResponse {
        ok,
        diagnostics: parsed.diagnostics,
        ir,
    }
}
