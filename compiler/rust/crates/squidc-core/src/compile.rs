use crate::{
    ast::{AstRoot, AstScreen},
    diagnostic::{error, Diagnostic},
    ir::{
        default_state_store, IrApp, IrBleProfileTrigger, IrExpr, IrFunction, IrHandler,
        IrProgram, IrScreen, IrStatement, IrTrigger,
    },
    parser::parse,
    profile::{BuildProfile, PORTABLE_TARGET_ID},
    semantic::validate_semantics,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

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
    let mut ast = parsed.ast.clone();

    if !ast.imports.is_empty() {
        parsed.diagnostics.push(error(
            "E_IMPORT_RESOLVER",
            "imports require compiling from an app entry path",
            0,
            request.source.len().min(1),
        ));
    }

    compile_ast(
        &mut ast,
        parsed.diagnostics,
        request.target_id,
        request.source.len(),
        profile,
    )
}

pub fn compile_path_with_profile(
    entry: &Path,
    target_id: &str,
    profile: BuildProfile,
) -> CompileResponse {
    let source_len = fs::metadata(entry)
        .map(|meta| meta.len() as usize)
        .unwrap_or(0);
    match ModuleGraph::load(entry) {
        Ok(mut graph) => {
            let source_len = graph.root_source_len.max(source_len);
            let linked = graph.link();
            match linked {
                Ok(mut ast) => compile_ast(
                    &mut ast,
                    graph.diagnostics,
                    target_id.to_string(),
                    source_len,
                    profile,
                ),
                Err(diagnostics) => CompileResponse {
                    ok: false,
                    diagnostics,
                    ir: None,
                },
            }
        }
        Err(diagnostic) => CompileResponse {
            ok: false,
            diagnostics: vec![diagnostic],
            ir: None,
        },
    }
}

fn compile_ast(
    ast: &mut AstRoot,
    mut diagnostics: Vec<Diagnostic>,
    target_id: String,
    source_len: usize,
    profile: BuildProfile,
) -> CompileResponse {
    if ast.app.is_none() {
        diagnostics.push(error(
            "E_APP_REQUIRED",
            "expected app declaration",
            0,
            source_len.min(1),
        ));
    }
    if ast.app.is_some() && ast.screens.is_empty() {
        let span = ast.app.as_ref().expect("app checked").span.clone();
        ast.screens.push(AstScreen {
            name: "main".to_string(),
            render: "compose".to_string(),
            statements: Vec::new(),
            exported: false,
            span,
        });
    }
    if let Some(target) = ast.app.as_ref().and_then(|app| app.target.as_ref()) {
        if target_id != PORTABLE_TARGET_ID && target != &target_id {
            diagnostics.push(error(
                "E_TARGET_MISMATCH",
                "source target does not match selected compatibility target",
                0,
                source_len.min(1),
            ));
        }
    }
    validate_semantics(ast, profile, &mut diagnostics);

    let ok = diagnostics.iter().all(|d| d.severity != "error");
    let ir = if ok {
        let app = ast.app.clone().expect("app exists after validation");
        let mut triggers = Vec::new();
        for trigger_block in ast.trigger_blocks.clone() {
            for statement in trigger_block.statements {
                if let Some(trigger) = trigger_from_statement(&statement) {
                    triggers.push(trigger);
                }
            }
        }
        let handlers = ast
            .handlers
            .clone()
            .into_iter()
            .map(|handler| IrHandler {
                event: handler.event,
                param: handler.param,
                preload: handler.preload,
                statements: handler.statements,
            })
            .collect();
        let functions = ast
            .functions
            .clone()
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
                target: app.target.unwrap_or_else(|| target_id.clone()),
            },
            state_store,
            device_bindings: ast.device_bindings.clone(),
            state: ast
                .state
                .clone()
                .map(|state| state.values)
                .unwrap_or_default(),
            functions,
            triggers,
            handlers,
            screens: ast
                .screens
                .clone()
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
        diagnostics,
        ir,
    }
}

struct ModuleGraph {
    root: PathBuf,
    app_dir: PathBuf,
    modules: BTreeMap<PathBuf, ParsedModule>,
    import_targets: BTreeMap<(PathBuf, String), PathBuf>,
    order: Vec<PathBuf>,
    diagnostics: Vec<Diagnostic>,
    root_source_len: usize,
}

struct ParsedModule {
    ast: AstRoot,
    label: String,
    is_root: bool,
}

impl ModuleGraph {
    fn load(entry: &Path) -> Result<Self, Diagnostic> {
        let root = entry.canonicalize().map_err(|io_error| {
            error(
                "E_IMPORT_IO",
                format!("failed to read {}: {io_error}", entry.display()),
                0,
                1,
            )
        })?;
        let app_dir = root
            .parent()
            .ok_or_else(|| {
                error(
                    "E_IMPORT_PATH",
                    "app entry must have a parent directory",
                    0,
                    1,
                )
            })?
            .to_path_buf();
        let root_source_len = fs::read_to_string(&root)
            .map(|source| source.len())
            .unwrap_or(0);
        let mut graph = Self {
            root: root.clone(),
            app_dir,
            modules: BTreeMap::new(),
            import_targets: BTreeMap::new(),
            order: Vec::new(),
            diagnostics: Vec::new(),
            root_source_len,
        };
        graph.load_module(&root, "root", true, &mut Vec::new())?;
        Ok(graph)
    }

    fn load_module(
        &mut self,
        path: &Path,
        label: &str,
        is_root: bool,
        stack: &mut Vec<PathBuf>,
    ) -> Result<(), Diagnostic> {
        let canonical = path.canonicalize().map_err(|io_error| {
            error(
                "E_IMPORT_IO",
                format!("failed to read {}: {io_error}", path.display()),
                0,
                1,
            )
        })?;
        if !canonical.starts_with(&self.app_dir) {
            return Err(error(
                "E_IMPORT_PATH",
                format!("import escapes app directory: {}", path.display()),
                0,
                1,
            ));
        }
        if stack.contains(&canonical) {
            return Err(error(
                "E_IMPORT_CYCLE",
                format!("import cycle at {}", path.display()),
                0,
                1,
            ));
        }
        if self.modules.contains_key(&canonical) {
            return Ok(());
        }
        if self.modules.len() >= 16 {
            return Err(error("E_IMPORT_LIMIT", "import file limit exceeded", 0, 1));
        }
        stack.push(canonical.clone());
        let source = fs::read_to_string(&canonical).map_err(|io_error| {
            error(
                "E_IMPORT_IO",
                format!("failed to read {}: {io_error}", path.display()),
                0,
                1,
            )
        })?;
        let parsed = parse(&source);
        self.diagnostics.extend(parsed.diagnostics);
        let ast = parsed.ast;
        for import in &ast.imports {
            if import.path.contains("..")
                || import.path.starts_with('/')
                || import.path.contains('\\')
                || import.path.is_empty()
            {
                self.diagnostics.push(error(
                    "E_IMPORT_PATH",
                    "import path must be a local path inside the app directory",
                    import.span.start,
                    import.span.end,
                ));
                continue;
            }
            let next = self.app_dir.join(&import.path);
            let target = next.canonicalize().map_err(|io_error| {
                error(
                    "E_IMPORT_IO",
                    format!("failed to read {}: {io_error}", next.display()),
                    0,
                    1,
                )
            })?;
            self.import_targets
                .insert((canonical.clone(), import.alias.clone()), target);
            self.load_module(&next, &import.alias, false, stack)?;
        }
        stack.pop();
        self.order.push(canonical.clone());
        self.modules.insert(
            canonical,
            ParsedModule {
                ast,
                label: label.to_string(),
                is_root,
            },
        );
        Ok(())
    }

    fn link(&mut self) -> Result<AstRoot, Vec<Diagnostic>> {
        let mut diagnostics = std::mem::take(&mut self.diagnostics);
        let mut linked = self
            .modules
            .get(&self.root)
            .map(|module| module.ast.clone())
            .unwrap_or_default();
        validate_import_only_modules(&self.modules, &mut diagnostics);
        validate_import_aliases(&self.modules, &mut diagnostics);
        validate_state_contracts(&linked, &self.modules, &mut diagnostics);

        let mut function_exports_by_path = BTreeMap::new();
        let mut screen_exports_by_path = BTreeMap::new();
        let mut module_functions = BTreeMap::new();
        let mut module_screens = BTreeMap::new();
        for (path, module) in &self.modules {
            let functions = module
                .ast
                .functions
                .iter()
                .map(|function| function.name.clone())
                .collect::<BTreeSet<_>>();
            let screens = module
                .ast
                .screens
                .iter()
                .map(|screen| screen.name.clone())
                .collect::<BTreeSet<_>>();
            module_functions.insert(path.clone(), functions);
            module_screens.insert(path.clone(), screens);
            if !module.is_root {
                let mut function_exports = BTreeMap::new();
                for function in &module.ast.functions {
                    if function.exported {
                        function_exports.insert(
                            function.name.clone(),
                            format!("{}.{}", module.label, function.name),
                        );
                    }
                }
                function_exports_by_path.insert(path.clone(), function_exports);
                let mut screen_exports = BTreeMap::new();
                for screen in &module.ast.screens {
                    if screen.exported {
                        screen_exports.insert(
                            screen.name.clone(),
                            format!("{}.{}", module.label, screen.name),
                        );
                    }
                }
                screen_exports_by_path.insert(path.clone(), screen_exports);
            }
        }
        let root_function_imports =
            local_import_exports(&self.root, &self.import_targets, &function_exports_by_path);
        let root_screen_imports =
            local_import_exports(&self.root, &self.import_targets, &screen_exports_by_path);
        let root_import_aliases = local_import_aliases(&self.root, &self.import_targets);

        rewrite_ast_references(
            &mut linked,
            "",
            &module_functions[&self.root],
            &module_screens[&self.root],
            &root_import_aliases,
            &root_function_imports,
            &root_screen_imports,
            &mut diagnostics,
        );

        for path in self.order.clone() {
            let Some(module) = self.modules.get(&path) else {
                continue;
            };
            if module.is_root {
                continue;
            }
            let mut ast = module.ast.clone();
            let function_imports =
                local_import_exports(&path, &self.import_targets, &function_exports_by_path);
            let screen_imports =
                local_import_exports(&path, &self.import_targets, &screen_exports_by_path);
            let import_aliases = local_import_aliases(&path, &self.import_targets);
            qualify_module_declarations(&mut ast, &module.label);
            rewrite_ast_references(
                &mut ast,
                &module.label,
                &module_functions[&path],
                &module_screens[&path],
                &import_aliases,
                &function_imports,
                &screen_imports,
                &mut diagnostics,
            );
            linked.functions.extend(ast.functions);
            linked.screens.extend(ast.screens);
        }

        linked.imports.clear();
        linked.required_state.clear();
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "error")
        {
            Err(diagnostics)
        } else {
            self.diagnostics = diagnostics.clone();
            Ok(linked)
        }
    }
}

fn validate_import_only_modules(
    modules: &BTreeMap<PathBuf, ParsedModule>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for module in modules.values().filter(|module| !module.is_root) {
        if module.ast.app.is_some()
            || module.ast.state.is_some()
            || !module.ast.device_bindings.is_empty()
            || !module.ast.trigger_blocks.is_empty()
            || !module.ast.handlers.is_empty()
        {
            diagnostics.push(error(
                "E_IMPORT_ONLY_DECL",
                "import-only files may declare imports, required state, functions, and screens only",
                0,
                1,
            ));
        }
    }
}

fn local_import_exports(
    source: &PathBuf,
    import_targets: &BTreeMap<(PathBuf, String), PathBuf>,
    exports_by_path: &BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> BTreeMap<(String, String), String> {
    let mut exports = BTreeMap::new();
    for ((importer, alias), target) in import_targets {
        if importer != source {
            continue;
        }
        if let Some(target_exports) = exports_by_path.get(target) {
            for (symbol, canonical) in target_exports {
                exports.insert((alias.clone(), symbol.clone()), canonical.clone());
            }
        }
    }
    exports
}

fn local_import_aliases(
    source: &PathBuf,
    import_targets: &BTreeMap<(PathBuf, String), PathBuf>,
) -> BTreeSet<String> {
    import_targets
        .keys()
        .filter(|(importer, _)| importer == source)
        .map(|(_, alias)| alias.clone())
        .collect()
}

fn validate_import_aliases(
    modules: &BTreeMap<PathBuf, ParsedModule>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    const BUILTINS: &[&str] = &[
        "app",
        "screen",
        "service",
        "device",
        "hardware",
        "input",
        "state",
        "stateMachine",
        "wifi",
        "display",
        "file",
        "data",
        "string",
        "debug",
    ];
    for module in modules.values() {
        let mut aliases = BTreeSet::new();
        let local_functions = module
            .ast
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<BTreeSet<_>>();
        let local_screens = module
            .ast
            .screens
            .iter()
            .map(|screen| screen.name.as_str())
            .collect::<BTreeSet<_>>();
        for import in &module.ast.imports {
            if !aliases.insert(import.alias.clone())
                || BUILTINS.contains(&import.alias.as_str())
                || local_functions.contains(import.alias.as_str())
                || local_screens.contains(import.alias.as_str())
            {
                diagnostics.push(error(
                    "E_DUPLICATE_IMPORT_ALIAS",
                    "import aliases must be unique in a source file and must not collide with local or built-in names",
                    import.span.start,
                    import.span.end,
                ));
            }
        }
    }
}

fn validate_state_contracts(
    root: &AstRoot,
    modules: &BTreeMap<PathBuf, ParsedModule>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let state = root
        .state
        .as_ref()
        .map(|state| {
            state
                .values
                .iter()
                .map(|value| (value.name.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    for module in modules.values().filter(|module| !module.is_root) {
        for required in &module.ast.required_state {
            let Some(actual) = state.get(&required.name) else {
                diagnostics.push(error(
                    "E_STATE_CONTRACT",
                    format!(
                        "imported module requires missing state field `{}`",
                        required.name
                    ),
                    required.span.start,
                    required.span.end,
                ));
                continue;
            };
            if actual.value_type != required.value_type || actual.nullable != required.nullable {
                diagnostics.push(error(
                    "E_STATE_CONTRACT",
                    format!(
                        "imported module state requirement `{}` does not match app state",
                        required.name
                    ),
                    required.span.start,
                    required.span.end,
                ));
            }
        }
    }
}

fn qualify_module_declarations(ast: &mut AstRoot, label: &str) {
    for function in &mut ast.functions {
        function.name = format!("{label}.{}", function.name);
    }
    for screen in &mut ast.screens {
        screen.name = format!("{label}.{}", screen.name);
    }
}

fn rewrite_ast_references(
    ast: &mut AstRoot,
    label: &str,
    local_functions: &BTreeSet<String>,
    local_screens: &BTreeSet<String>,
    import_aliases: &BTreeSet<String>,
    exported_functions: &BTreeMap<(String, String), String>,
    exported_screens: &BTreeMap<(String, String), String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for function in &mut ast.functions {
        rewrite_statements(
            &mut function.statements,
            label,
            local_functions,
            local_screens,
            import_aliases,
            exported_functions,
            exported_screens,
            diagnostics,
        );
    }
    for handler in &mut ast.handlers {
        rewrite_statements(
            &mut handler.statements,
            label,
            local_functions,
            local_screens,
            import_aliases,
            exported_functions,
            exported_screens,
            diagnostics,
        );
    }
    for trigger in &mut ast.trigger_blocks {
        rewrite_statements(
            &mut trigger.statements,
            label,
            local_functions,
            local_screens,
            import_aliases,
            exported_functions,
            exported_screens,
            diagnostics,
        );
    }
    for screen in &mut ast.screens {
        rewrite_statements(
            &mut screen.statements,
            label,
            local_functions,
            local_screens,
            import_aliases,
            exported_functions,
            exported_screens,
            diagnostics,
        );
    }
}

fn rewrite_statements(
    statements: &mut [IrStatement],
    label: &str,
    local_functions: &BTreeSet<String>,
    local_screens: &BTreeSet<String>,
    import_aliases: &BTreeSet<String>,
    exported_functions: &BTreeMap<(String, String), String>,
    exported_screens: &BTreeMap<(String, String), String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            IrStatement::ScreenOpen { screen } => {
                if let Some((alias, name)) = screen.split_once('.') {
                    if let Some(canonical) =
                        exported_screens.get(&(alias.to_string(), name.to_string()))
                    {
                        *screen = canonical.clone();
                    } else {
                        diagnostics.push(error(
                            "E_UNKNOWN_MODULE_SYMBOL",
                            "screen.open references an unknown exported module screen",
                            0,
                            1,
                        ));
                    }
                } else if !label.is_empty() && local_screens.contains(screen) {
                    *screen = format!("{label}.{screen}");
                } else if !import_aliases.is_empty() && screen.contains('.') {
                    diagnostics.push(error(
                        "E_UNKNOWN_MODULE_SYMBOL",
                        "screen.open references an unknown exported module screen",
                        0,
                        1,
                    ));
                }
            }
            IrStatement::Call { name, args } => {
                rewrite_call_name(
                    name,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
                rewrite_exprs(
                    args,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
            }
            IrStatement::Let { expr, .. }
            | IrStatement::Assign { expr, .. }
            | IrStatement::StateAssign { expr, .. }
            | IrStatement::ServiceTimerEvery {
                interval_ms: expr, ..
            }
            | IrStatement::ServiceTimerAfter { delay_ms: expr, .. }
            | IrStatement::ServicePowerSleep {
                wake_after_ms: expr,
            }
            | IrStatement::HardwareGpioWrite { value: expr, .. }
            | IrStatement::ServiceIndicatorWrite { value: expr }
            | IrStatement::DisplayText { text: expr, .. }
            | IrStatement::DisplayDraw { drawable: expr, .. } => {
                rewrite_expr(
                    expr,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
            }
            IrStatement::ServiceIndicatorBlink { on_ms, off_ms } => {
                rewrite_expr(
                    on_ms,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
                rewrite_expr(
                    off_ms,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
            }
            IrStatement::If {
                condition,
                then_statements,
                else_statements,
            } => {
                rewrite_expr(
                    condition,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
                rewrite_statements(
                    then_statements,
                    label,
                    local_functions,
                    local_screens,
                    import_aliases,
                    exported_functions,
                    exported_screens,
                    diagnostics,
                );
                rewrite_statements(
                    else_statements,
                    label,
                    local_functions,
                    local_screens,
                    import_aliases,
                    exported_functions,
                    exported_screens,
                    diagnostics,
                );
            }
            IrStatement::Repeat { count, statements } => {
                rewrite_expr(
                    count,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
                rewrite_statements(
                    statements,
                    label,
                    local_functions,
                    local_screens,
                    import_aliases,
                    exported_functions,
                    exported_screens,
                    diagnostics,
                );
            }
            IrStatement::For {
                list,
                max,
                statements,
                ..
            } => {
                rewrite_expr(
                    list,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
                if let Some(max) = max {
                    rewrite_expr(
                        max,
                        label,
                        local_functions,
                        import_aliases,
                        exported_functions,
                        diagnostics,
                    );
                }
                rewrite_statements(
                    statements,
                    label,
                    local_functions,
                    local_screens,
                    import_aliases,
                    exported_functions,
                    exported_screens,
                    diagnostics,
                );
            }
            IrStatement::Return { expr: Some(expr) } => {
                rewrite_expr(
                    expr,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
            }
            IrStatement::DebugPrint { args } => {
                rewrite_exprs(
                    args,
                    label,
                    local_functions,
                    import_aliases,
                    exported_functions,
                    diagnostics,
                );
            }
            IrStatement::DebugBlock { statements } => {
                rewrite_statements(
                    statements,
                    label,
                    local_functions,
                    local_screens,
                    import_aliases,
                    exported_functions,
                    exported_screens,
                    diagnostics,
                );
            }
            _ => {}
        }
    }
}

fn rewrite_exprs(
    exprs: &mut [IrExpr],
    label: &str,
    local_functions: &BTreeSet<String>,
    import_aliases: &BTreeSet<String>,
    exported_functions: &BTreeMap<(String, String), String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for expr in exprs {
        rewrite_expr(
            expr,
            label,
            local_functions,
            import_aliases,
            exported_functions,
            diagnostics,
        );
    }
}

fn rewrite_expr(
    expr: &mut IrExpr,
    label: &str,
    local_functions: &BTreeSet<String>,
    import_aliases: &BTreeSet<String>,
    exported_functions: &BTreeMap<(String, String), String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        IrExpr::Call { name, args } => {
            rewrite_call_name(
                name,
                label,
                local_functions,
                import_aliases,
                exported_functions,
                diagnostics,
            );
            rewrite_exprs(
                args,
                label,
                local_functions,
                import_aliases,
                exported_functions,
                diagnostics,
            );
        }
        IrExpr::Binary { left, right, .. } => {
            rewrite_expr(
                left,
                label,
                local_functions,
                import_aliases,
                exported_functions,
                diagnostics,
            );
            rewrite_expr(
                right,
                label,
                local_functions,
                import_aliases,
                exported_functions,
                diagnostics,
            );
        }
        IrExpr::Unary { expr, .. } => {
            rewrite_expr(
                expr,
                label,
                local_functions,
                import_aliases,
                exported_functions,
                diagnostics,
            );
        }
        IrExpr::Field { target, .. } => {
            rewrite_expr(
                target,
                label,
                local_functions,
                import_aliases,
                exported_functions,
                diagnostics,
            );
        }
        _ => {}
    }
}

fn rewrite_call_name(
    name: &mut String,
    label: &str,
    local_functions: &BTreeSet<String>,
    import_aliases: &BTreeSet<String>,
    exported_functions: &BTreeMap<(String, String), String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some((alias, symbol)) = name.split_once('.') {
        if alias == label && local_functions.contains(symbol) {
            return;
        } else if let Some(canonical) =
            exported_functions.get(&(alias.to_string(), symbol.to_string()))
        {
            *name = canonical.clone();
        } else if import_aliases.contains(alias) {
            diagnostics.push(error(
                "E_UNKNOWN_MODULE_SYMBOL",
                format!("call references an unknown exported module function `{name}`"),
                0,
                1,
            ));
        }
    } else if !label.is_empty() && local_functions.contains(name) {
        *name = format!("{label}.{name}");
    }
}

fn trigger_from_statement(statement: &IrStatement) -> Option<IrTrigger> {
    match statement {
        IrStatement::ServiceTimerEvery { event, interval_ms } => {
            literal_i32(interval_ms).map(|interval_ms| IrTrigger {
                event: event.clone(),
                repeating: true,
                interval_ms,
                ble: None,
            })
        }
        IrStatement::ServiceTimerAfter { event, delay_ms } => {
            literal_i32(delay_ms).map(|interval_ms| IrTrigger {
                event: event.clone(),
                repeating: false,
                interval_ms,
                ble: None,
            })
        }
        IrStatement::ServiceBleProfile {
            profile,
            id,
            role,
            accept,
            events,
        } => Some(IrTrigger {
            event: String::new(),
            repeating: false,
            interval_ms: 0,
            ble: Some(IrBleProfileTrigger {
                profile: profile.clone(),
                id: id.clone(),
                role: role.clone(),
                accept: accept.clone(),
                events: events.clone(),
            }),
        }),
        _ => None,
    }
}

fn literal_i32(expr: &IrExpr) -> Option<i32> {
    match expr {
        IrExpr::Literal { value } => value.as_i64().and_then(|value| i32::try_from(value).ok()),
        _ => None,
    }
}
