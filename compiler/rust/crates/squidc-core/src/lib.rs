use rowan::{GreenNode, GreenNodeBuilder, Language, SyntaxKind};
use serde::{Deserialize, Serialize};

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
    pub state: Vec<IrStateValue>,
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
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrHandler {
    pub event: String,
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
    #[serde(rename = "screen.open")]
    ScreenOpen { screen: String },
    #[serde(rename = "screen.refresh")]
    ScreenRefresh,
    #[serde(rename = "app.exit")]
    AppExit,
    #[serde(rename = "assign")]
    Assign { name: String, expr: IrExpr },
    #[serde(rename = "display.clear")]
    DisplayClear { color: String },
    #[serde(rename = "display.text")]
    DisplayText { text: String, options: serde_json::Value },
    #[serde(rename = "display.rect")]
    DisplayRect { x: i64, y: i64, w: i64, h: i64, options: serde_json::Value },
    #[serde(rename = "display.line")]
    DisplayLine { x1: i64, y1: i64, x2: i64, y2: i64, options: serde_json::Value },
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
    Plus,
    Minus,
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
    };

    parser.parse_root(&mut builder);

    builder.finish_node();
    ParsedSource {
        green: Some(builder.finish()),
        ast: parser.ast,
        diagnostics: parser.diagnostics,
    }
}

pub fn compile(request: CompileRequest) -> CompileResponse {
    let mut parsed = parse(&request.source);
    let ast = parsed.ast.clone();

    if ast.app.is_none() {
        parsed.diagnostics.push(error("E_APP_REQUIRED", "expected app declaration", 0, request.source.len().min(1)));
    }
    if ast.screens.is_empty() {
        parsed.diagnostics.push(error("E_SCREEN_REQUIRED", "expected at least one screen declaration", 0, request.source.len().min(1)));
    }
    if let Some(target) = ast.app.as_ref().and_then(|app| app.target.as_ref()) {
        if target != &request.target_id {
            parsed.diagnostics.push(error("E_TARGET_MISMATCH", "source target does not match selected target", 0, request.source.len().min(1)));
        }
    }

    let ok = parsed.diagnostics.iter().all(|d| d.severity != "error");
    let ir = if ok {
        let app = ast.app.expect("app exists after validation");
        let handlers = ast.handlers.into_iter().map(|handler| IrHandler {
            event: handler.event,
            statements: handler.statements,
        }).collect();
        Some(IrProgram {
            format: "squidscript-ir".to_string(),
            version: 1,
            app: IrApp {
                id: app.id,
                name: app.name,
                target: request.target_id,
            },
            state: ast.state.map(|state| state.values).unwrap_or_default(),
            handlers,
            screens: ast.screens.into_iter().map(|screen| IrScreen {
                name: screen.name,
                render: screen.render,
                statements: screen.statements,
            }).collect(),
        })
    } else {
        None
    };

    CompileResponse { ok, diagnostics: parsed.diagnostics, ir }
}

struct Parser<'a> {
    tokens: &'a [LexToken],
    cursor: usize,
    ast: AstRoot,
    diagnostics: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn parse_root(&mut self, builder: &mut GreenNodeBuilder) {
        while !self.at_end() {
            self.consume_ws(builder);

            if self.at_ident("app") {
                self.parse_app(builder);
            } else if self.at_ident("state") {
                self.parse_state(builder);
            } else if self.peek().map(|token| token.kind == TokenKind::Ident && token.text.starts_with("on")).unwrap_or(false) {
                self.parse_handler(builder);
            } else if self.at_ident("screen") {
                self.parse_screen(builder);
            } else if !self.at_end() {
                self.bump(builder);
            }
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
        if target.is_none() {
            self.diagnostics.push(error("E_TARGET_REQUIRED", "app declaration must include target", start, end));
        }
    }

    fn parse_state(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::StateBlock));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        let mut selected_default = 0;
        let mut values = Vec::new();

        self.bump(builder);
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
                let name = self.peek().map(|token| token.text.clone()).unwrap_or_default();
                self.bump(builder);
                self.consume_ws(builder);
                if self.at_kind(TokenKind::Colon) {
                    self.bump(builder);
                    self.consume_ws(builder);
                    if let Some(value) = self.consume_literal_value(builder) {
                        if name == "selected" {
                            if let Some(number) = value.as_i64() {
                                selected_default = number;
                            }
                        }
                        values.push(IrStateValue { name, value });
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
        let name = self.consume_string(builder).unwrap_or_else(|| "main".to_string());
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

    fn parse_handler(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        let handler_name = self.peek().map(|token| token.text.clone()).unwrap_or_default();
        self.bump(builder);

        let mut key_arg = None;
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::String) {
                key_arg = self.consume_string(builder);
                continue;
            }
            if self.at_kind(TokenKind::OpenBrace) {
                self.bump(builder);
                break;
            }
            self.bump(builder);
        }

        let statements = self.parse_statements_until_close(builder);

        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();

        let event = if handler_name == "onKey" {
            key_arg.map(|key| format!("onKey.{key}")).unwrap_or_else(|| "onKey".to_string())
        } else {
            handler_name
        };
        self.ast.handlers.push(AstHandler {
            event,
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

        if self.at_kind(TokenKind::Equals) {
            self.bump(builder);
            self.consume_ws(builder);
            let expr = self.parse_expr(builder)?;
            return Some(IrStatement::Assign { name: first, expr });
        }

        if !self.at_kind(TokenKind::Dot) {
            return None;
        }
        self.bump(builder);
        self.consume_ws(builder);
        let method = self.peek().filter(|token| token.kind == TokenKind::Ident)?.text.clone();
        self.bump(builder);
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
        }
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
            ("display", "clear") => {
                let color = self.consume_string(builder).unwrap_or_else(|| "white".to_string());
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayClear { color })
            }
            ("display", "text") => {
                let text = self.consume_string(builder).unwrap_or_default();
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
                Some(IrStatement::DisplayRect { x, y, w, h, options })
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
                Some(IrStatement::DisplayLine { x1, y1, x2, y2, options })
            }
            _ => {
                self.consume_call_tail(builder);
                None
            }
        }
    }

    fn parse_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        let left = if self.at_kind(TokenKind::Number) {
            IrExpr::Literal { value: serde_json::json!(self.consume_number(builder)?) }
        } else if self.at_kind(TokenKind::String) {
            IrExpr::Literal { value: serde_json::json!(self.consume_string(builder)?) }
        } else if self.at_kind(TokenKind::Ident) {
            let name = self.peek()?.text.clone();
            self.bump(builder);
            IrExpr::State { name }
        } else {
            return None;
        };

        self.consume_ws(builder);
        if self.at_kind(TokenKind::Plus) || self.at_kind(TokenKind::Minus) {
            let operator = self.peek()?.text.clone();
            self.bump(builder);
            self.consume_ws(builder);
            let right = self.parse_expr(builder)?;
            Some(IrExpr::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            })
        } else {
            Some(left)
        }
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
            let Some(key) = self.peek().filter(|token| token.kind == TokenKind::Ident).map(|token| token.text.clone()) else {
                self.bump(builder);
                continue;
            };
            self.bump(builder);
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Colon) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            if let Some(value) = self.consume_literal_value(builder) {
                map.insert(key, value);
            }
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Comma) {
                self.bump(builder);
            }
        }
        serde_json::Value::Object(map)
    }

    fn consume_literal_value(&mut self, builder: &mut GreenNodeBuilder) -> Option<serde_json::Value> {
        if self.at_kind(TokenKind::Number) {
            return self.consume_number(builder).map(|number| serde_json::json!(number));
        }
        if self.at_kind(TokenKind::String) {
            return self.consume_string(builder).map(|text| serde_json::json!(text));
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

    fn consume_number(&mut self, builder: &mut GreenNodeBuilder) -> Option<i64> {
        if !self.at_kind(TokenKind::Number) {
            return None;
        }
        let value = self.peek()?.text.parse().ok();
        self.bump(builder);
        value
    }

    fn at_ident(&self, ident: &str) -> bool {
        self.peek().map(|token| token.kind == TokenKind::Ident && token.text == ident).unwrap_or(false)
    }

    fn at_kind(&self, kind: TokenKind) -> bool {
        self.peek().map(|token| token.kind == kind).unwrap_or(false)
    }

    fn at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    fn peek(&self) -> Option<&LexToken> {
        self.tokens.get(self.cursor)
    }

    fn previous_end(&self) -> Option<usize> {
        self.cursor.checked_sub(1).and_then(|index| self.tokens.get(index)).map(|token| token.span.end)
    }

    fn bump(&mut self, builder: &mut GreenNodeBuilder) {
        if let Some(token) = self.tokens.get(self.cursor) {
            builder.token(SquidLang::kind_to_raw(syntax_kind_for(token.kind)), &token.text);
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
        let ch = source[cursor..].chars().next().expect("cursor is at char boundary");

        if ch.is_whitespace() {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..].chars().next().expect("cursor is at char boundary");
                if !next.is_whitespace() {
                    break;
                }
                cursor += next.len_utf8();
            }
            tokens.push(token(TokenKind::Whitespace, source, start, cursor));
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..].chars().next().expect("cursor is at char boundary");
                if !(next.is_ascii_alphanumeric() || next == '_' || next == '-') {
                    break;
                }
                cursor += next.len_utf8();
            }
            tokens.push(token(TokenKind::Ident, source, start, cursor));
        } else if ch.is_ascii_digit() {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..].chars().next().expect("cursor is at char boundary");
                if !next.is_ascii_digit() {
                    break;
                }
                cursor += next.len_utf8();
            }
            tokens.push(token(TokenKind::Number, source, start, cursor));
        } else if ch == '"' {
            cursor += ch.len_utf8();
            while cursor < bytes.len() {
                let next = source[cursor..].chars().next().expect("cursor is at char boundary");
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
                '+' => TokenKind::Plus,
                '-' => TokenKind::Minus,
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
        | TokenKind::Plus
        | TokenKind::Minus => SquidKind::Token,
    }
}

fn titleize(id: &str) -> String {
    id.split(['-', '_']).filter(|part| !part.is_empty()).map(|part| {
        let mut chars = part.chars();
        match chars.next() {
            Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
            None => String::new(),
        }
    }).collect::<Vec<_>>().join(" ")
}

fn error(code: &str, message: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: "error".to_string(),
        message: message.to_string(),
        span: SourceSpan { start, end },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_hello_menu_into_typed_ast_and_cst() {
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let parsed = parse(source);

        assert!(parsed.green.is_some());
        assert_eq!(parsed.ast.app.as_ref().map(|app| app.id.as_str()), Some("hello-menu"));
        assert_eq!(parsed.ast.app.as_ref().and_then(|app| app.target.as_deref()), Some("xteink-x4"));
        assert_eq!(parsed.ast.state.as_ref().map(|state| state.selected_default), Some(0));
        assert_eq!(parsed.ast.handlers.len(), 5);
        assert_eq!(parsed.ast.screens.iter().map(|screen| screen.name.as_str()).collect::<Vec<_>>(), vec!["main", "detail"]);
    }

    #[test]
    fn compiles_spec_hello_menu_to_screen_ir() {
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let output = compile(CompileRequest { source: source.to_string(), target_id: "xteink-x4".to_string() });
        assert!(output.ok, "{:?}", output.diagnostics);
        let ir = output.ir.unwrap();
        assert_eq!(ir.format, "squidscript-ir");
        assert_eq!(ir.version, 1);
        assert_eq!(ir.screens.iter().map(|screen| screen.name.as_str()).collect::<Vec<_>>(), vec!["main", "detail"]);
        assert_eq!(ir.handlers.iter().map(|handler| handler.event.as_str()).collect::<Vec<_>>(), vec!["onStart", "onKey.DOWN", "onKey.UP", "onKey.SELECT", "onKey.BACK"]);
    }

    #[test]
    fn parses_and_lowers_simple_handlers() {
        let source = r#"app "hello-menu" target "xteink-x4"
state { selected: 0 }
onStart() {
  screen.open("main")
}
onKey("DOWN") {
  selected = selected + 1
  screen.refresh()
}
screen("main") {
  display.clear("gray0")
}
"#;
        let output = compile(CompileRequest { source: source.to_string(), target_id: "xteink-x4".to_string() });
        assert!(output.ok, "{:?}", output.diagnostics);
        let ir = output.ir.unwrap();
        assert_eq!(ir.handlers.len(), 2);
        assert_eq!(ir.handlers[0].event, "onStart");
        assert_eq!(ir.handlers[1].event, "onKey.DOWN");
    }

    #[test]
    fn matches_hello_menu_ir_fixture() {
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let expected = include_str!("../../../../fixtures/expected/hello_menu.ir.json");
        let output = compile(CompileRequest { source: source.to_string(), target_id: "xteink-x4".to_string() });

        assert!(output.ok, "{:?}", output.diagnostics);
        let actual_json = serde_json::to_value(output.ir.unwrap()).unwrap();
        let expected_json: serde_json::Value = serde_json::from_str(expected).unwrap();
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn compiles_browser_sim_binbook_reader_fixture() {
        let source = include_str!("../../../../fixtures/valid/binbook_reader_browser_sim.squid");
        let output = compile(CompileRequest { source: source.to_string(), target_id: "xteink-x4".to_string() });

        assert!(output.ok, "{:?}", output.diagnostics);
        let ir = output.ir.unwrap();
        assert_eq!(ir.app.id, "binbook-reader");
        assert!(ir.screens.iter().any(|screen| screen.name == "main"));
    }

    #[test]
    fn encodes_minimal_sqbc_container() {
        let source = include_str!("../../../../fixtures/valid/hello_menu.squid");
        let output = compile(CompileRequest { source: source.to_string(), target_id: "xteink-x4".to_string() });
        let ir = output.ir.unwrap();
        let sqbc = encode_sqbc(&ir);

        assert_eq!(&sqbc[0..4], SQBC_MAGIC);
        assert_eq!(u32::from_le_bytes(sqbc[4..8].try_into().unwrap()), SQBC_VERSION);
        assert_eq!(u32::from_le_bytes(sqbc[8..12].try_into().unwrap()) as usize, sqbc.len() - 12);
    }

    #[test]
    fn reports_diagnostics_with_spans() {
        let output = compile(CompileRequest { source: "screen(\"main\") {}\n".to_string(), target_id: "xteink-x4".to_string() });
        assert!(!output.ok);
        assert!(output.diagnostics.iter().any(|d| d.code == "E_APP_REQUIRED" && d.span.end >= d.span.start));
    }
}
