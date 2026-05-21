use rowan::{GreenNode, GreenNodeBuilder, Language, SyntaxKind};
use serde::{Deserialize, Serialize};

pub mod sqbc_v2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SquidKind {
    Root = 0,
    AppDecl = 1,
    StateBlock = 2,
    Ident = 3,
    String = 4,
    Number = 5,
    Token = 6,
    Whitespace = 7,
    Error = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SquidLang {}

impl Language for SquidLang {
    type Kind = SquidKind;

    fn kind_from_raw(raw: SyntaxKind) -> Self::Kind {
        match raw.0 {
            0 => SquidKind::Root,
            1 => SquidKind::AppDecl,
            2 => SquidKind::StateBlock,
            3 => SquidKind::Ident,
            4 => SquidKind::String,
            5 => SquidKind::Number,
            6 => SquidKind::Token,
            7 => SquidKind::Whitespace,
            _ => SquidKind::Error,
        }
    }

    fn kind_to_raw(kind: Self::Kind) -> SyntaxKind {
        SyntaxKind(kind as u16)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileRequest {
    pub source: String,
    pub target_id: String,
}

pub const PORTABLE_TARGET_ID: &str = "portable";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BuildProfile {
    Dev,
    Release,
}

impl BuildProfile {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "dev" => Some(Self::Dev),
            "release" => Some(Self::Release),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileResponse {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub ir: Option<IrProgram>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrProgram {
    pub format: String,
    pub version: u32,
    pub app: IrApp,
    #[serde(default = "default_state_store")]
    pub state_store: String,
    pub state: Vec<IrStateValue>,
    pub functions: Vec<IrFunction>,
    pub handlers: Vec<IrHandler>,
    pub screens: Vec<IrScreen>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrApp {
    pub id: String,
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrStateValue {
    pub name: String,
    pub value_type: String,
    pub nullable: bool,
    pub value: serde_json::Value,
}

fn default_state_store() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrHandler {
    pub event: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub preload: bool,
    pub statements: Vec<IrStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<String>,
    pub statements: Vec<IrStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrScreen {
    pub name: String,
    pub render: String,
    pub statements: Vec<IrStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op")]
pub enum IrStatement {
    #[serde(rename = "state.load")]
    StateLoad,
    #[serde(rename = "state.save")]
    StateSave,
    #[serde(rename = "state.reset")]
    StateReset,
    #[serde(rename = "screen.open")]
    ScreenOpen { screen: String },
    #[serde(rename = "screen.refresh")]
    ScreenRefresh,
    #[serde(rename = "app.exit")]
    AppExit,
    #[serde(rename = "app.launch")]
    AppLaunch { app: String },
    #[serde(rename = "app.arm")]
    AppArm { app: String },
    #[serde(rename = "app.disarm")]
    AppDisarm { app: String },
    #[serde(rename = "service.timer.every")]
    ServiceTimerEvery { event: String, interval_ms: IrExpr },
    #[serde(rename = "service.timer.after")]
    ServiceTimerAfter { event: String, delay_ms: IrExpr },
    #[serde(rename = "assign")]
    Assign { name: String, expr: IrExpr },
    #[serde(rename = "let")]
    Let { name: String, expr: IrExpr },
    #[serde(rename = "if")]
    If {
        condition: IrExpr,
        then_statements: Vec<IrStatement>,
        else_statements: Vec<IrStatement>,
    },
    #[serde(rename = "repeat")]
    Repeat {
        count: IrExpr,
        statements: Vec<IrStatement>,
    },
    #[serde(rename = "for")]
    For {
        item: String,
        list: IrExpr,
        max: Option<IrExpr>,
        statements: Vec<IrStatement>,
    },
    #[serde(rename = "return")]
    Return { expr: Option<IrExpr> },
    #[serde(rename = "call")]
    Call { name: String, args: Vec<IrExpr> },
    #[serde(rename = "debug.print")]
    DebugPrint { args: Vec<IrExpr> },
    #[serde(rename = "hardware.gpio.write")]
    HardwareGpioWrite { name: String, value: IrExpr },
    #[serde(rename = "hardware.gpio.toggle")]
    HardwareGpioToggle { name: String },
    #[serde(rename = "display.clear")]
    DisplayClear { color: String },
    #[serde(rename = "display.text")]
    DisplayText {
        text: IrExpr,
        options: serde_json::Value,
    },
    #[serde(rename = "display.rect")]
    DisplayRect {
        x: i64,
        y: i64,
        w: i64,
        h: i64,
        options: serde_json::Value,
    },
    #[serde(rename = "display.line")]
    DisplayLine {
        x1: i64,
        y1: i64,
        x2: i64,
        y2: i64,
        options: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op")]
pub enum IrExpr {
    #[serde(rename = "literal")]
    Literal { value: serde_json::Value },
    #[serde(rename = "state")]
    State { name: String },
    #[serde(rename = "binary")]
    Binary {
        left: Box<IrExpr>,
        operator: String,
        right: Box<IrExpr>,
    },
    #[serde(rename = "unary")]
    Unary {
        operator: String,
        expr: Box<IrExpr>,
    },
    #[serde(rename = "field")]
    Field {
        target: Box<IrExpr>,
        field: String,
    },
    #[serde(rename = "hardware.gpio.read")]
    HardwareGpioRead { name: String },
    #[serde(rename = "system.memory")]
    SystemMemory,
    #[serde(rename = "system.storage")]
    SystemStorage { name: String },
    #[serde(rename = "call")]
    Call { name: String, args: Vec<IrExpr> },
}

pub const SQBC_MAGIC: &[u8; 4] = b"SQBC";
pub const SQBC_VERSION: u32 = 1;

pub fn encode_sqbc(ir: &IrProgram) -> Vec<u8> {
    let payload = serde_json::to_vec(ir).expect("IR must serialize for SQBC payload");
    let mut bytes = Vec::with_capacity(12 + payload.len());
    bytes.extend_from_slice(SQBC_MAGIC);
    bytes.extend_from_slice(&SQBC_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

#[derive(Debug, Clone)]
pub struct ParsedSource {
    pub green: Option<GreenNode>,
    pub ast: AstRoot,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AstRoot {
    pub app: Option<AstAppDecl>,
    pub state: Option<AstStateBlock>,
    pub functions: Vec<AstFunction>,
    pub handlers: Vec<AstHandler>,
    pub screens: Vec<AstScreen>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstAppDecl {
    pub id: String,
    pub name: String,
    pub target: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstStateBlock {
    pub selected_default: i64,
    pub store: String,
    pub values: Vec<IrStateValue>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstScreen {
    pub name: String,
    pub render: String,
    pub statements: Vec<IrStatement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstHandler {
    pub event: String,
    pub preload: bool,
    pub statements: Vec<IrStatement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstFunction {
    pub name: String,
    pub params: Vec<String>,
    pub statements: Vec<IrStatement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Ident,
    String,
    Number,
    OpenBrace,
    CloseBrace,
    OpenParen,
    CloseParen,
    Colon,
    Dot,
    Comma,
    Equals,
    Bang,
    Less,
    Greater,
    Plus,
    Minus,
    Question,
    At,
    Whitespace,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LexToken {
    kind: TokenKind,
    text: String,
    span: SourceSpan,
}

pub fn parse(source: &str) -> ParsedSource {
    let tokens = lex(source);
    let mut builder = GreenNodeBuilder::new();
    builder.start_node(SquidLang::kind_to_raw(SquidKind::Root));

    let mut parser = Parser {
        tokens: &tokens,
        cursor: 0,
        ast: AstRoot::default(),
        diagnostics: Vec::new(),
        pending_preload: None,
    };

    parser.parse_root(&mut builder);

    builder.finish_node();
    ParsedSource {
        green: Some(builder.finish()),
        ast: parser.ast,
        diagnostics: parser.diagnostics,
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

fn state_default_matches(value_type: &str, nullable: bool, value: &serde_json::Value) -> bool {
    if value.is_null() {
        return nullable;
    }
    match value_type {
        "int" => value.as_i64().is_some(),
        "bool" => value.as_bool().is_some(),
        "string" => value.as_str().is_some(),
        _ => false,
    }
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
        let handlers = ast
            .handlers
            .into_iter()
            .map(|handler| IrHandler {
                event: handler.event,
                preload: handler.preload,
                statements: handler.statements,
            })
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
            version: 1,
            app: IrApp {
                id: app.id,
                name: app.name,
                target: app.target.unwrap_or_else(|| request.target_id),
            },
            state_store,
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

struct Parser<'a> {
    tokens: &'a [LexToken],
    cursor: usize,
    ast: AstRoot,
    diagnostics: Vec<Diagnostic>,
    pending_preload: Option<SourceSpan>,
}

impl Parser<'_> {
    fn parse_root(&mut self, builder: &mut GreenNodeBuilder) {
        while !self.at_end() {
            self.consume_ws(builder);

            if self.at_kind(TokenKind::At) {
                self.parse_attribute(builder);
                continue;
            }

            if self.at_ident("app") {
                self.reject_pending_attribute();
                self.parse_app(builder);
            } else if self.at_ident("state") {
                self.reject_pending_attribute();
                self.parse_state(builder);
            } else if self.at_event_method("on") {
                let preload = self.pending_preload.take().is_some();
                self.parse_event_handler(builder, preload);
            } else if self.at_ident("function") {
                self.reject_pending_attribute();
                self.parse_function(builder);
            } else if self.at_ident("screen") {
                self.reject_pending_attribute();
                self.parse_screen(builder);
            } else if !self.at_end() {
                self.reject_pending_attribute();
                self.bump(builder);
            }
        }
    }

    fn parse_attribute(&mut self, builder: &mut GreenNodeBuilder) {
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder); // @
        self.consume_ws(builder);
        let Some(name) = self.consume_ident(builder) else {
            self.diagnostics.push(error(
                "E_ATTRIBUTE",
                "attribute must have a name",
                start,
                self.previous_end().unwrap_or(start),
            ));
            return;
        };
        let end = self.previous_end().unwrap_or(start);
        if name == "preload" {
            self.pending_preload = Some(SourceSpan { start, end });
        } else {
            self.diagnostics
                .push(error("E_ATTRIBUTE", "unsupported attribute", start, end));
        }
    }

    fn reject_pending_attribute(&mut self) {
        if let Some(span) = self.pending_preload.take() {
            self.diagnostics.push(error(
                "E_ATTRIBUTE_TARGET",
                "@preload is only valid before event.on handlers",
                span.start,
                span.end,
            ));
        }
    }

    fn parse_app(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::AppDecl));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder);
        self.consume_ws(builder);
        let id = self.consume_string(builder);
        self.consume_ws(builder);

        let mut target = None;
        if self.at_ident("target") {
            self.bump(builder);
            self.consume_ws(builder);
            target = self.consume_string(builder);
        }
        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();

        if let Some(id) = id {
            self.ast.app = Some(AstAppDecl {
                name: titleize(&id),
                id,
                target: target.clone(),
                span: SourceSpan { start, end },
            });
        }
    }

    fn parse_state(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::StateBlock));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        let mut selected_default = 0;
        let mut store = default_state_store();
        let mut values = Vec::new();

        self.bump(builder);
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
            self.consume_ws(builder);
            if self.at_kind(TokenKind::OpenBrace) {
                self.bump(builder);
                loop {
                    self.consume_ws(builder);
                    if self.at_kind(TokenKind::CloseBrace) {
                        self.bump(builder);
                        break;
                    }
                    let Some(option) = self.consume_ident(builder) else {
                        if self.at_end() {
                            break;
                        }
                        self.bump(builder);
                        continue;
                    };
                    self.consume_ws(builder);
                    if self.at_kind(TokenKind::Colon) {
                        self.bump(builder);
                    }
                    self.consume_ws(builder);
                    let value = self.consume_string(builder).unwrap_or_default();
                    if option == "store" {
                        if matches!(value.as_str(), "default" | "internal" | "removable") {
                            store = value;
                        } else {
                            self.diagnostics.push(error(
                                "E_STATE_STORE",
                                "unknown state store class",
                                start,
                                self.previous_end().unwrap_or(start),
                            ));
                        }
                    }
                    self.consume_ws(builder);
                    self.consume_comma(builder);
                }
            }
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseParen) {
                self.bump(builder);
            }
        }
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::OpenBrace) {
                self.bump(builder);
                continue;
            }
            if self.at_kind(TokenKind::CloseBrace) {
                self.bump(builder);
                break;
            }
            if self.at_kind(TokenKind::Ident) {
                let name = self
                    .peek()
                    .map(|token| token.text.clone())
                    .unwrap_or_default();
                self.bump(builder);
                self.consume_ws(builder);
                if self.at_kind(TokenKind::Colon) {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let Some(mut value_type) = self.consume_ident(builder) else {
                        self.diagnostics.push(error(
                            "E_STATE_TYPE",
                            "expected state slot type",
                            start,
                            self.previous_end().unwrap_or(start),
                        ));
                        continue;
                    };
                    let mut nullable = false;
                    self.consume_ws(builder);
                    if self.at_kind(TokenKind::Question) {
                        self.bump(builder);
                        nullable = true;
                        value_type.push('?');
                        self.consume_ws(builder);
                    }
                    if !matches!(value_type.as_str(), "int" | "bool" | "string" | "int?" | "bool?" | "string?") {
                        self.diagnostics.push(error(
                            "E_STATE_TYPE",
                            "unsupported state slot type",
                            start,
                            self.previous_end().unwrap_or(start),
                        ));
                    }
                    if self.at_kind(TokenKind::Equals) {
                        self.bump(builder);
                    } else {
                        self.diagnostics.push(error(
                            "E_STATE_DEFAULT",
                            "expected state slot default value",
                            start,
                            self.previous_end().unwrap_or(start),
                        ));
                    }
                    self.consume_ws(builder);
                    if let Some(value) = self.consume_literal_value(builder) {
                        let base_type = value_type.trim_end_matches('?').to_string();
                        if !state_default_matches(&base_type, nullable, &value) {
                            self.diagnostics.push(error(
                                "E_STATE_DEFAULT",
                                "state default does not match declared type",
                                start,
                                self.previous_end().unwrap_or(start),
                            ));
                        }
                        if name == "selected" {
                            if let Some(number) = value.as_i64() {
                                selected_default = number;
                            }
                        }
                        values.push(IrStateValue {
                            name,
                            value_type: base_type,
                            nullable,
                            value,
                        });
                    }
                }
                continue;
            }
            self.bump(builder);
        }

        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();
        self.ast.state = Some(AstStateBlock {
            selected_default,
            store,
            values,
            span: SourceSpan { start, end },
        });
    }

    fn parse_screen(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);

        self.bump(builder);
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        let name = self
            .consume_string(builder)
            .unwrap_or_else(|| "main".to_string());
        let mut render = "compose".to_string();

        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseParen) {
                self.bump(builder);
                break;
            }
            if self.at_ident("render") {
                self.bump(builder);
                self.consume_ws(builder);
                if self.at_kind(TokenKind::Colon) {
                    self.bump(builder);
                    self.consume_ws(builder);
                    if let Some(value) = self.consume_string(builder) {
                        render = value;
                    }
                }
                continue;
            }
            self.bump(builder);
        }

        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }
        let statements = self.parse_statements_until_close(builder);
        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();
        self.ast.screens.push(AstScreen {
            name,
            render,
            statements,
            span: SourceSpan { start, end },
        });
    }

    fn parse_event_handler(&mut self, builder: &mut GreenNodeBuilder, preload: bool) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder); // event
        self.consume_ws(builder);
        if self.at_kind(TokenKind::Dot) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        let _method = self.consume_ident(builder);
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        let event = self.consume_string(builder).unwrap_or_default();
        self.consume_call_tail(builder);
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }

        let statements = self.parse_statements_until_close(builder);
        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();
        self.ast.handlers.push(AstHandler {
            event,
            preload,
            statements,
            span: SourceSpan { start, end },
        });
    }

    fn parse_function(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder);
        self.consume_ws(builder);
        let name = self.consume_ident(builder).unwrap_or_default();
        let params = self.parse_param_list(builder);

        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }
        let statements = self.parse_statements_until_close(builder);
        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();
        self.ast.functions.push(AstFunction {
            name,
            params,
            statements,
            span: SourceSpan { start, end },
        });
    }

    fn parse_statements_until_close(&mut self, builder: &mut GreenNodeBuilder) -> Vec<IrStatement> {
        let mut statements = Vec::new();

        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseBrace) {
                self.bump(builder);
                break;
            }

            if let Some(statement) = self.parse_statement(builder) {
                statements.push(statement);
            } else {
                self.bump(builder);
            }
        }

        statements
    }

    fn parse_statement(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrStatement> {
        if !self.at_kind(TokenKind::Ident) {
            return None;
        }

        let first = self.peek()?.text.clone();
        self.bump(builder);
        self.consume_ws(builder);

        if first == "let" {
            let name = self.consume_ident(builder)?;
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Colon) {
                self.bump(builder);
                self.consume_ws(builder);
                let _type_name = self.consume_ident(builder);
                self.consume_ws(builder);
            }
            if self.at_kind(TokenKind::Equals) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            let expr = self.parse_expr(builder)?;
            return Some(IrStatement::Let { name, expr });
        }

        if first == "if" {
            return self.parse_if_statement(builder);
        }

        if first == "repeat" {
            return self.parse_repeat_statement(builder);
        }

        if first == "for" {
            return self.parse_for_statement(builder);
        }

        if first == "return" {
            if self.at_kind(TokenKind::CloseBrace) {
                return Some(IrStatement::Return { expr: None });
            }
            let expr = self.parse_expr(builder);
            return Some(IrStatement::Return { expr });
        }

        if self.at_kind(TokenKind::Equals) {
            self.bump(builder);
            self.consume_ws(builder);
            let expr = self.parse_expr(builder)?;
            return Some(IrStatement::Assign { name: first, expr });
        }

        if self.at_kind(TokenKind::OpenParen) {
            let args = self.parse_call_args(builder);
            return Some(IrStatement::Call { name: first, args });
        }

        if !self.at_kind(TokenKind::Dot) {
            return None;
        }
        self.bump(builder);
        self.consume_ws(builder);
        let method = self
            .peek()
            .filter(|token| token.kind == TokenKind::Ident)?
            .text
            .clone();
        self.bump(builder);
        self.consume_ws(builder);

        if first == "hardware" && method == "gpio" {
            if self.at_kind(TokenKind::Dot) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            let action = self.consume_ident(builder)?;
            self.consume_ws(builder);
            if self.at_kind(TokenKind::OpenParen) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            return match action.as_str() {
                "write" => {
                    let name = self.consume_string(builder).unwrap_or_default();
                    self.consume_comma(builder);
                    let value = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                        value: serde_json::json!(false),
                    });
                    self.consume_call_tail(builder);
                    Some(IrStatement::HardwareGpioWrite { name, value })
                }
                "toggle" => {
                    let name = self.consume_string(builder).unwrap_or_default();
                    self.consume_call_tail(builder);
                    Some(IrStatement::HardwareGpioToggle { name })
                }
                _ => {
                    self.consume_call_tail(builder);
                    None
                }
            };
        }

        if first == "service" && method == "timer" {
            if self.at_kind(TokenKind::Dot) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            let action = self.consume_ident(builder)?;
            self.consume_ws(builder);
            if self.at_kind(TokenKind::OpenParen) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            return match action.as_str() {
                "every" => {
                    let event = self.consume_string(builder).unwrap_or_default();
                    self.consume_comma(builder);
                    let interval_ms = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                        value: serde_json::json!(0),
                    });
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceTimerEvery { event, interval_ms })
                }
                "after" => {
                    let event = self.consume_string(builder).unwrap_or_default();
                    self.consume_comma(builder);
                    let delay_ms = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                        value: serde_json::json!(0),
                    });
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceTimerAfter { event, delay_ms })
                }
                _ => {
                    self.consume_call_tail(builder);
                    None
                }
            };
        }

        let has_call_args = if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
            true
        } else {
            false
        };
        self.consume_ws(builder);

        match (first.as_str(), method.as_str()) {
            ("state", "load") => {
                self.consume_call_tail(builder);
                Some(IrStatement::StateLoad)
            }
            ("state", "save") => {
                self.consume_call_tail(builder);
                Some(IrStatement::StateSave)
            }
            ("state", "reset") => {
                self.consume_call_tail(builder);
                Some(IrStatement::StateReset)
            }
            ("screen", "refresh") => {
                self.consume_call_tail(builder);
                Some(IrStatement::ScreenRefresh)
            }
            ("screen", "open") => {
                let screen = self.consume_string(builder).unwrap_or_default();
                self.consume_call_tail(builder);
                Some(IrStatement::ScreenOpen { screen })
            }
            ("app", "exit") => {
                self.consume_call_tail(builder);
                Some(IrStatement::AppExit)
            }
            ("app", "launch") => {
                let app = self.consume_string(builder).unwrap_or_default();
                self.consume_call_tail(builder);
                Some(IrStatement::AppLaunch { app })
            }
            ("app", "arm") => {
                let app = self.consume_string(builder).unwrap_or_default();
                self.consume_call_tail(builder);
                Some(IrStatement::AppArm { app })
            }
            ("app", "disarm") => {
                let app = self.consume_string(builder).unwrap_or_default();
                self.consume_call_tail(builder);
                Some(IrStatement::AppDisarm { app })
            }
            ("debug", "print") => {
                let args = self.parse_call_args_after_open(builder);
                Some(IrStatement::DebugPrint { args })
            }
            ("display", "clear") => {
                let color = self
                    .consume_string(builder)
                    .unwrap_or_else(|| "white".to_string());
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayClear { color })
            }
            ("display", "text") => {
                let text = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                    value: serde_json::json!(""),
                });
                self.consume_ws(builder);
                if self.at_kind(TokenKind::Comma) {
                    self.bump(builder);
                }
                self.consume_ws(builder);
                let options = self.parse_options_object(builder);
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayText { text, options })
            }
            ("display", "rect") => {
                let x = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let y = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let w = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let h = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let options = self.parse_options_object(builder);
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayRect {
                    x,
                    y,
                    w,
                    h,
                    options,
                })
            }
            ("display", "line") => {
                let x1 = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let y1 = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let x2 = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let y2 = self.consume_number(builder).unwrap_or(0);
                self.consume_comma(builder);
                let options = self.parse_options_object(builder);
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayLine {
                    x1,
                    y1,
                    x2,
                    y2,
                    options,
                })
            }
            _ => {
                if has_call_args {
                    Some(IrStatement::Call {
                        name: format!("{first}.{method}"),
                        args: self.parse_call_args_after_open(builder),
                    })
                } else {
                    self.consume_call_tail(builder);
                    None
                }
            }
        }
    }

    fn parse_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        self.parse_comparison_expr(builder)
    }

    fn parse_comparison_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        let mut expr = self.parse_additive_expr(builder)?;
        loop {
            self.consume_ws(builder);
            if !(self.at_kind(TokenKind::Less)
                || self.at_kind(TokenKind::Greater)
                || self.at_kind(TokenKind::Equals)
                || (self.at_kind(TokenKind::Bang) && self.next_kind() == Some(TokenKind::Equals)))
            {
                break;
            }
            let operator = self.consume_binary_operator(builder)?;
            self.consume_ws(builder);
            let right = self.parse_additive_expr(builder)?;
            expr = IrExpr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }
        Some(expr)
    }

    fn parse_additive_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        let mut expr = self.parse_unary_expr(builder)?;
        loop {
            self.consume_ws(builder);
            if !(self.at_kind(TokenKind::Plus) || self.at_kind(TokenKind::Minus)) {
                break;
            }
            let operator = self.consume_binary_operator(builder)?;
            self.consume_ws(builder);
            let right = self.parse_unary_expr(builder)?;
            expr = IrExpr::Binary {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
            };
        }
        Some(expr)
    }

    fn parse_unary_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        self.consume_ws(builder);
        if self.at_kind(TokenKind::Bang) {
            self.bump(builder);
            self.consume_ws(builder);
            let expr = self.parse_unary_expr(builder)?;
            return Some(IrExpr::Unary {
                operator: "!".to_string(),
                expr: Box::new(expr),
            });
        }
        self.parse_postfix_expr(builder)
    }

    fn parse_postfix_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        let mut expr = self.parse_primary_expr(builder)?;
        loop {
            self.consume_ws(builder);
            if !self.at_kind(TokenKind::Dot) {
                break;
            }
            self.bump(builder);
            self.consume_ws(builder);
            let Some(field) = self.consume_ident(builder) else {
                break;
            };
            expr = IrExpr::Field {
                target: Box::new(expr),
                field,
            };
        }
        Some(expr)
    }

    fn parse_primary_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        let expr = if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
            self.consume_ws(builder);
            let expr = self.parse_expr(builder)?;
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseParen) {
                self.bump(builder);
            }
            expr
        } else if self.at_kind(TokenKind::Number) {
            IrExpr::Literal {
                value: serde_json::json!(self.consume_number(builder)?),
            }
        } else if self.at_kind(TokenKind::String) {
            IrExpr::Literal {
                value: serde_json::json!(self.consume_string(builder)?),
            }
        } else if self.at_kind(TokenKind::Ident) {
            let name = self.peek()?.text.clone();
            self.bump(builder);
            if name == "true" {
                IrExpr::Literal {
                    value: serde_json::json!(true),
                }
            } else if name == "false" {
                IrExpr::Literal {
                    value: serde_json::json!(false),
                }
            } else if name == "null" {
                IrExpr::Literal {
                    value: serde_json::Value::Null,
                }
            } else if self.at_kind(TokenKind::Dot) {
                self.bump(builder);
                self.consume_ws(builder);
                let namespace = self.consume_ident(builder).unwrap_or_default();
                self.consume_ws(builder);
                if name == "system" && namespace == "memory" {
                    self.parse_call_args(builder);
                    IrExpr::SystemMemory
                } else if name == "system" && namespace == "storage" {
                    let args = self.parse_call_args(builder);
                    let name = args
                        .first()
                        .and_then(|arg| match arg {
                            IrExpr::Literal { value } => value.as_str().map(ToOwned::to_owned),
                            _ => None,
                        })
                        .unwrap_or_default();
                    IrExpr::SystemStorage { name }
                } else if name == "hardware" && namespace == "gpio" && self.at_kind(TokenKind::Dot)
                {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let action = self.consume_ident(builder).unwrap_or_default();
                    self.consume_ws(builder);
                    if action == "read" {
                        let args = self.parse_call_args(builder);
                        let name = args
                            .first()
                            .and_then(|arg| match arg {
                                IrExpr::Literal { value } => value.as_str().map(ToOwned::to_owned),
                                _ => None,
                            })
                            .unwrap_or_default();
                        IrExpr::HardwareGpioRead { name }
                    } else {
                        IrExpr::State { name }
                    }
                } else if self.at_kind(TokenKind::OpenParen) {
                    IrExpr::Call {
                        name: format!("{name}.{namespace}"),
                        args: self.parse_call_args(builder),
                    }
                } else {
                    IrExpr::Field {
                        target: Box::new(IrExpr::State { name }),
                        field: namespace,
                    }
                }
            } else if self.at_kind(TokenKind::OpenParen) {
                IrExpr::Call {
                    name,
                    args: self.parse_call_args(builder),
                }
            } else {
                IrExpr::State { name }
            }
        } else {
            return None;
        };
        Some(expr)
    }

    fn parse_if_statement(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrStatement> {
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        let condition = self.parse_expr(builder)?;
        self.consume_ws(builder);
        if self.at_kind(TokenKind::CloseParen) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }
        let then_statements = self.parse_statements_until_close(builder);
        self.consume_ws(builder);
        let else_statements = if self.at_ident("else") {
            self.bump(builder);
            self.consume_ws(builder);
            if self.at_kind(TokenKind::OpenBrace) {
                self.bump(builder);
            }
            self.parse_statements_until_close(builder)
        } else {
            Vec::new()
        };
        Some(IrStatement::If {
            condition,
            then_statements,
            else_statements,
        })
    }

    fn parse_repeat_statement(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrStatement> {
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        let count = self.parse_expr(builder)?;
        self.consume_ws(builder);
        if self.at_kind(TokenKind::CloseParen) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }
        let statements = self.parse_statements_until_close(builder);
        Some(IrStatement::Repeat { count, statements })
    }

    fn parse_for_statement(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrStatement> {
        self.consume_ws(builder);
        let item = self.consume_ident(builder)?;
        self.consume_ws(builder);
        if self.at_ident("in") {
            self.bump(builder);
        }
        self.consume_ws(builder);
        let list = self.parse_expr(builder)?;
        self.consume_ws(builder);
        let max = if self.at_ident("max") {
            self.bump(builder);
            self.consume_ws(builder);
            self.parse_expr(builder)
        } else {
            None
        };
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }
        let statements = self.parse_statements_until_close(builder);
        Some(IrStatement::For {
            item,
            list,
            max,
            statements,
        })
    }

    fn parse_param_list(&mut self, builder: &mut GreenNodeBuilder) -> Vec<String> {
        let mut params = Vec::new();
        self.consume_ws(builder);
        if !self.at_kind(TokenKind::OpenParen) {
            return params;
        }
        self.bump(builder);
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseParen) {
                self.bump(builder);
                break;
            }
            if let Some(param) = self.consume_ident(builder) {
                params.push(param);
            }
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Comma) {
                self.bump(builder);
            }
        }
        params
    }

    fn parse_call_args(&mut self, builder: &mut GreenNodeBuilder) -> Vec<IrExpr> {
        if !self.at_kind(TokenKind::OpenParen) {
            return Vec::new();
        }
        self.bump(builder);
        self.parse_call_args_after_open(builder)
    }

    fn parse_call_args_after_open(&mut self, builder: &mut GreenNodeBuilder) -> Vec<IrExpr> {
        let mut args = Vec::new();
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseParen) {
                self.bump(builder);
                break;
            }
            if let Some(arg) = self.parse_expr(builder) {
                args.push(arg);
            } else {
                self.bump(builder);
            }
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Comma) {
                self.bump(builder);
            }
        }
        args
    }

    fn consume_binary_operator(&mut self, builder: &mut GreenNodeBuilder) -> Option<String> {
        let first = self.peek()?.text.clone();
        if matches!(
            self.peek()?.kind,
            TokenKind::Equals | TokenKind::Bang | TokenKind::Less | TokenKind::Greater
        ) {
            self.bump(builder);
            if self.at_kind(TokenKind::Equals) {
                let second = self.peek()?.text.clone();
                self.bump(builder);
                return Some(format!("{first}{second}"));
            }
            return Some(first);
        }
        self.bump(builder);
        Some(first)
    }

    fn parse_options_object(&mut self, builder: &mut GreenNodeBuilder) -> serde_json::Value {
        self.consume_ws(builder);
        if !self.at_kind(TokenKind::OpenBrace) {
            return serde_json::json!({});
        }

        let mut map = serde_json::Map::new();
        self.bump(builder);
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseBrace) {
                self.bump(builder);
                break;
            }
            let Some(key) = self
                .peek()
                .filter(|token| token.kind == TokenKind::Ident)
                .map(|token| token.text.clone())
            else {
                self.bump(builder);
                continue;
            };
            self.bump(builder);
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Colon) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            if let Some(value) = self.parse_expr(builder) {
                map.insert(
                    key,
                    serde_json::to_value(value).expect("IR expressions serialize"),
                );
            }
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Comma) {
                self.bump(builder);
            }
        }
        serde_json::Value::Object(map)
    }

    fn consume_literal_value(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<serde_json::Value> {
        if self.at_kind(TokenKind::Number) {
            return self
                .consume_number(builder)
                .map(|number| serde_json::json!(number));
        }
        if self.at_kind(TokenKind::String) {
            return self
                .consume_string(builder)
                .map(|text| serde_json::json!(text));
        }
        if self.at_ident("true") {
            self.bump(builder);
            return Some(serde_json::json!(true));
        }
        if self.at_ident("false") {
            self.bump(builder);
            return Some(serde_json::json!(false));
        }
        if self.at_ident("null") {
            self.bump(builder);
            return Some(serde_json::Value::Null);
        }
        None
    }

    fn consume_comma(&mut self, builder: &mut GreenNodeBuilder) {
        self.consume_ws(builder);
        if self.at_kind(TokenKind::Comma) {
            self.bump(builder);
        }
        self.consume_ws(builder);
    }

    fn consume_call_tail(&mut self, builder: &mut GreenNodeBuilder) {
        let mut depth = 0usize;
        while !self.at_end() {
            if self.at_kind(TokenKind::OpenParen) {
                depth += 1;
            }
            if self.at_kind(TokenKind::CloseParen) {
                self.bump(builder);
                if depth == 0 {
                    break;
                }
                depth -= 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            if depth == 0 && self.at_kind(TokenKind::CloseBrace) {
                break;
            }
            self.bump(builder);
        }
    }

    fn consume_ws(&mut self, builder: &mut GreenNodeBuilder) {
        while self.at_kind(TokenKind::Whitespace) {
            self.bump(builder);
        }
    }

    fn consume_string(&mut self, builder: &mut GreenNodeBuilder) -> Option<String> {
        if !self.at_kind(TokenKind::String) {
            return None;
        }
        let text = self.peek()?.text.clone();
        self.bump(builder);
        Some(text.trim_matches('"').to_string())
    }

    fn consume_ident(&mut self, builder: &mut GreenNodeBuilder) -> Option<String> {
        if !self.at_kind(TokenKind::Ident) {
            return None;
        }
        let text = self.peek()?.text.clone();
        self.bump(builder);
        Some(text)
    }

    fn consume_number(&mut self, builder: &mut GreenNodeBuilder) -> Option<i64> {
        if !self.at_kind(TokenKind::Number) {
            return None;
        }
        let value = self.peek()?.text.parse().ok();
        self.bump(builder);
        value
    }

    fn at_ident(&self, ident: &str) -> bool {
        self.peek()
            .map(|token| token.kind == TokenKind::Ident && token.text == ident)
            .unwrap_or(false)
    }

    fn at_event_method(&self, method: &str) -> bool {
        let mut index = self.cursor;
        while self
            .tokens
            .get(index)
            .map(|token| token.kind == TokenKind::Whitespace)
            .unwrap_or(false)
        {
            index += 1;
        }
        matches!(
            (
                self.tokens.get(index),
                self.tokens.get(index + 1),
                self.tokens.get(index + 2)
            ),
            (
                Some(LexToken {
                    kind: TokenKind::Ident,
                    text,
                    ..
                }),
                Some(LexToken {
                    kind: TokenKind::Dot,
                    ..
                }),
                Some(LexToken {
                    kind: TokenKind::Ident,
                    text: method_text,
                    ..
                })
            ) if text == "event" && method_text == method
        )
    }

    fn at_kind(&self, kind: TokenKind) -> bool {
        self.peek().map(|token| token.kind == kind).unwrap_or(false)
    }

    fn next_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.cursor + 1).map(|token| token.kind)
    }

    fn at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    fn peek(&self) -> Option<&LexToken> {
        self.tokens.get(self.cursor)
    }

    fn previous_end(&self) -> Option<usize> {
        self.cursor
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map(|token| token.span.end)
    }

    fn bump(&mut self, builder: &mut GreenNodeBuilder) {
        if let Some(token) = self.tokens.get(self.cursor) {
            builder.token(
                SquidLang::kind_to_raw(syntax_kind_for(token.kind)),
                &token.text,
            );
            self.cursor += 1;
        }
    }
}

fn lex(source: &str) -> Vec<LexToken> {
    let mut tokens = Vec::new();
    let mut cursor = 0;
    let bytes = source.as_bytes();

    while cursor < bytes.len() {
        let start = cursor;
        let ch = source[cursor..]
            .chars()
            .next()
            .expect("cursor is at char boundary");

        if ch.is_whitespace() {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is at char boundary");
                if !next.is_whitespace() {
                    break;
                }
                cursor += next.len_utf8();
            }
            tokens.push(token(TokenKind::Whitespace, source, start, cursor));
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is at char boundary");
                if !(next.is_ascii_alphanumeric() || next == '_' || next == '-') {
                    break;
                }
                cursor += next.len_utf8();
            }
            tokens.push(token(TokenKind::Ident, source, start, cursor));
        } else if ch.is_ascii_digit() {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is at char boundary");
                if !next.is_ascii_digit() {
                    break;
                }
                cursor += next.len_utf8();
            }
            tokens.push(token(TokenKind::Number, source, start, cursor));
        } else if ch == '"' {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is at char boundary");
                cursor += next.len_utf8();
                if next == '"' {
                    break;
                }
            }
            tokens.push(token(TokenKind::String, source, start, cursor));
        } else {
            cursor += ch.len_utf8();
            let kind = match ch {
                '{' => TokenKind::OpenBrace,
                '}' => TokenKind::CloseBrace,
                '(' => TokenKind::OpenParen,
                ')' => TokenKind::CloseParen,
                ':' => TokenKind::Colon,
                '.' => TokenKind::Dot,
                ',' => TokenKind::Comma,
                '=' => TokenKind::Equals,
                '!' => TokenKind::Bang,
                '<' => TokenKind::Less,
                '>' => TokenKind::Greater,
                '+' => TokenKind::Plus,
                '-' => TokenKind::Minus,
                '?' => TokenKind::Question,
                '@' => TokenKind::At,
                _ => TokenKind::Unknown,
            };
            tokens.push(token(kind, source, start, cursor));
        }
    }

    tokens
}

fn token(kind: TokenKind, source: &str, start: usize, end: usize) -> LexToken {
    LexToken {
        kind,
        text: source[start..end].to_string(),
        span: SourceSpan { start, end },
    }
}

fn syntax_kind_for(kind: TokenKind) -> SquidKind {
    match kind {
        TokenKind::Ident => SquidKind::Ident,
        TokenKind::String => SquidKind::String,
        TokenKind::Number => SquidKind::Number,
        TokenKind::Whitespace => SquidKind::Whitespace,
        TokenKind::Unknown => SquidKind::Error,
        TokenKind::OpenBrace
        | TokenKind::CloseBrace
        | TokenKind::OpenParen
        | TokenKind::CloseParen
        | TokenKind::Colon
        | TokenKind::Dot
        | TokenKind::Comma
        | TokenKind::Equals
        | TokenKind::Bang
        | TokenKind::Less
        | TokenKind::Greater
        | TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Question
        | TokenKind::At => SquidKind::Token,
    }
}

fn titleize(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn error(code: &str, message: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: "error".to_string(),
        message: message.to_string(),
        span: SourceSpan { start, end },
    }
}

fn warning(code: &str, message: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: "warning".to_string(),
        message: message.to_string(),
        span: SourceSpan { start, end },
    }
}

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

fn validate_semantics(ast: &AstRoot, _profile: BuildProfile, diagnostics: &mut Vec<Diagnostic>) {
    let mut screen_names = std::collections::BTreeSet::new();
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
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let parsed = parse(source);

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
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let output = compile(CompileRequest {
            source: source.to_string(),
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
  display.clear("gray0")
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
  display.clear("gray0")
  drawLabel(label)
}

screen("detail") {
  display.clear("gray0")
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
        assert!(ir.screens[0].statements.iter().any(
            |statement| matches!(statement, IrStatement::Let { name, .. } if name == "label")
        ));
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
  display.clear("gray0")
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
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let expected = include_str!("../../../../fixtures/expected/hello_menu.ir.json");
        let output = compile(CompileRequest {
            source: source.to_string(),
            target_id: "xteink-x4".to_string(),
        });

        assert!(output.ok, "{:?}", output.diagnostics);
        let ir = output.ir.unwrap();
        let actual_json = serde_json::to_value(&ir).unwrap();
        let expected_json: serde_json::Value = serde_json::from_str(expected).unwrap();
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
        let source = include_str!("../../../../fixtures/valid/binbook_reader_browser_sim.squid");
        let output = compile(CompileRequest {
            source: source.to_string(),
            target_id: "xteink-x4".to_string(),
        });

        assert!(output.ok, "{:?}", output.diagnostics);
        let ir = output.ir.unwrap();
        assert_eq!(ir.app.id, "binbook-reader");
        assert!(ir.screens.iter().any(|screen| screen.name == "main"));
    }

    #[test]
    fn encodes_minimal_sqbc_container() {
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let output = compile(CompileRequest {
            source: source.to_string(),
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
    fn encodes_reference_sqbc_v3_for_headless_counter() {
        let source = include_str!("../../../fixtures/conformance/headless_counter.squid");
        let output = compile(CompileRequest {
            source: source.to_string(),
            target_id: "esp32c3-super-mini".to_string(),
        });
        assert!(output.ok, "{:?}", output.diagnostics);
        let sqbc =
            sqbc_v2::encode_sqbc_v2(&output.ir.unwrap()).expect("reference subset should encode");

        assert_eq!(&sqbc[0..4], sqbc_v2::SQBC_V2_MAGIC);
        assert_eq!(
            u16::from_le_bytes(sqbc[4..6].try_into().unwrap()),
            sqbc_v2::SQBC_V3_VERSION
        );
        assert_eq!(
            u32::from_le_bytes(sqbc[8..12].try_into().unwrap()) as usize,
            sqbc.len()
        );
        assert_eq!(u32::from_le_bytes(sqbc[12..16].try_into().unwrap()), 7);
        assert_eq!(
            sqbc_v2::read_app_id(&sqbc).unwrap().as_deref(),
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

        let dev = sqbc_v2::encode_sqbc_v2_with_profile(&ir, BuildProfile::Dev).unwrap();
        let release = sqbc_v2::encode_sqbc_v2_with_profile(&ir, BuildProfile::Release).unwrap();
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
  display.clear("gray0")
  display.text("Hello", { x: 10, y: 20 })
}
"#;
        let output = compile(CompileRequest {
            source: source.to_string(),
            target_id: PORTABLE_TARGET_ID.to_string(),
        });
        assert!(output.ok, "{:?}", output.diagnostics);
        let sqbc = sqbc_v2::encode_sqbc_v2(&output.ir.unwrap()).unwrap();
        assert_eq!(u32::from_le_bytes(sqbc[12..16].try_into().unwrap()), 7);
    }

    #[test]
    fn parses_hardware_gpio_calls() {
        let source = r#"app "gpio"
state { led: bool = false }
event.on("app.start") {
  hardware.gpio.write("indicator.primary", true)
  led = hardware.gpio.read("indicator.primary")
  hardware.gpio.toggle("pin.raw0")
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
        assert!(matches!(ir.handlers[0].statements[0], IrStatement::StateReset));
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
  hardware.gpio.toggle("indicator.primary")
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
  display.clear("gray0")
}
screen("main", { render: "invalid" }) {
  selected = selected + 1
}
screen("main") {
  display.clear("gray0")
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
  display.clear("gray0")
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
        let IrStatement::If { condition, then_statements, .. } = &ir.handlers[0].statements[1] else {
            panic!("expected result guard");
        };
        assert!(matches!(condition, IrExpr::Unary { operator, .. } if operator == "!"));
        assert!(then_statements.iter().any(|statement| matches!(
            statement,
            IrStatement::DebugPrint { args } if matches!(args.first(), Some(IrExpr::Field { field, .. }) if field == "error")
        )));
    }

    #[test]
    fn warns_when_fallible_result_is_ignored() {
        let source = r#"app "ignored-result" target "xteink-x4"
event.on("app.start") {
  library.mkdir("books", "/manuals")
  screen.open("main")
}
screen("main") {
  display.clear("gray0")
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
}
