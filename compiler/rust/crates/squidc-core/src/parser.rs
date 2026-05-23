use crate::{
    ast::{
        AstAppDecl, AstFunction, AstHandler, AstRoot, AstScreen, AstStateBlock, AstTriggerBlock,
    },
    device_config,
    diagnostic::{error, Diagnostic, SourceSpan},
    ir::{default_state_store, IrDeviceBinding, IrExpr, IrStateValue, IrStatement},
    lexer::{lex, syntax_kind_for, LexToken, TokenKind},
    syntax::{SquidKind, SquidLang},
};
use rowan::{GreenNode, GreenNodeBuilder, Language};

#[derive(Debug, Clone)]
pub struct ParsedSource {
    pub green: Option<GreenNode>,
    pub ast: AstRoot,
    pub diagnostics: Vec<Diagnostic>,
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

            if self.at_qualified_ident("app", "triggers") {
                self.reject_pending_attribute();
                self.parse_app_triggers(builder);
            } else if self.at_ident("app") {
                self.reject_pending_attribute();
                self.parse_app(builder);
            } else if self.at_ident("state") {
                self.reject_pending_attribute();
                self.parse_state(builder);
            } else if self.at_ident("device") {
                self.reject_pending_attribute();
                self.parse_device(builder);
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
                    if !matches!(
                        value_type.as_str(),
                        "int" | "bool" | "string" | "int?" | "bool?" | "string?"
                    ) {
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

    fn parse_device(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder);
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseBrace) {
                self.bump(builder);
                break;
            }
            let Some(service) = self.consume_ident(builder) else {
                self.bump(builder);
                continue;
            };
            self.consume_ws(builder);
            let binding = if self.at_kind(TokenKind::String) {
                self.consume_string(builder)
                    .unwrap_or_else(|| "default".to_string())
            } else {
                "default".to_string()
            };
            self.consume_ws(builder);
            if self.at_kind(TokenKind::OpenBrace) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            let mut resource = String::new();
            if self.at_ident("use") {
                self.bump(builder);
                self.consume_ws(builder);
                resource = self.consume_string(builder).unwrap_or_default();
            }
            while !self.at_end() {
                self.consume_ws(builder);
                if self.at_kind(TokenKind::CloseBrace) {
                    self.bump(builder);
                    break;
                }
                self.bump(builder);
            }
            if !device_config::is_safe_sqdevice_path(&resource) {
                self.diagnostics.push(error(
                    "E_DEVICE_PATH",
                    "device binding must use a safe package-relative .sqdevice path",
                    start,
                    self.previous_end().unwrap_or(start),
                ));
            } else {
                self.ast.device_bindings.push(IrDeviceBinding {
                    service,
                    binding,
                    resource,
                });
            }
        }
        builder.finish_node();
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

    fn parse_app_triggers(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder); // app
        self.consume_ws(builder);
        if self.at_kind(TokenKind::Dot) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        let _method = self.consume_ident(builder);
        self.consume_ws(builder);
        if self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
        }
        let statements = self.parse_statements_until_close(builder);
        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();
        self.ast.trigger_blocks.push(AstTriggerBlock {
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

        if first == "debug" && self.at_kind(TokenKind::OpenBrace) {
            self.bump(builder);
            let statements = self.parse_statements_until_close(builder);
            return Some(IrStatement::DebugBlock { statements });
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

        if first == "display" {
            return self.parse_display_statement(builder, &method);
        }

        if first == "service"
            && matches!(method.as_str(), "timer" | "display" | "indicator" | "wifi")
        {
            if self.at_kind(TokenKind::Dot) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            let action = self.consume_ident(builder)?;
            self.consume_ws(builder);
            if method == "display" {
                return self.parse_display_statement(builder, &action);
            }
            if self.at_kind(TokenKind::OpenParen) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            if method == "wifi" {
                return Some(IrStatement::Call {
                    name: format!("service.wifi.{action}"),
                    args: self.parse_call_args_after_open(builder),
                });
            }
            return match (method.as_str(), action.as_str()) {
                ("timer", "every") => {
                    let event = self.consume_string(builder).unwrap_or_default();
                    self.consume_comma(builder);
                    let interval_ms = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                        value: serde_json::json!(0),
                    });
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceTimerEvery { event, interval_ms })
                }
                ("timer", "after") => {
                    let event = self.consume_string(builder).unwrap_or_default();
                    self.consume_comma(builder);
                    let delay_ms = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                        value: serde_json::json!(0),
                    });
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceTimerAfter { event, delay_ms })
                }
                ("indicator", "write") => {
                    let value = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                        value: serde_json::json!(false),
                    });
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceIndicatorWrite { value })
                }
                ("indicator", "toggle") => {
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceIndicatorToggle)
                }
                ("indicator", "breathe") => {
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceIndicatorBreathe)
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
            _ => {
                if has_call_args {
                    let name = if first == "wifi" {
                        format!("service.wifi.{method}")
                    } else {
                        format!("{first}.{method}")
                    };
                    Some(IrStatement::Call {
                        name,
                        args: self.parse_call_args_after_open(builder),
                    })
                } else {
                    self.consume_call_tail(builder);
                    None
                }
            }
        }
    }

    fn parse_display_statement(
        &mut self,
        builder: &mut GreenNodeBuilder,
        action: &str,
    ) -> Option<IrStatement> {
        if self.at_kind(TokenKind::OpenParen) {
            self.bump(builder);
        }
        self.consume_ws(builder);
        match action {
            "clear" => {
                let color = self
                    .consume_string(builder)
                    .unwrap_or_else(|| "white".to_string());
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayClear { color })
            }
            "text" => {
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
            "rect" => {
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
            "line" => {
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
                self.consume_call_tail(builder);
                None
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
                } else if name == "service"
                    && namespace == "indicator"
                    && self.at_kind(TokenKind::Dot)
                {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let action = self.consume_ident(builder).unwrap_or_default();
                    self.consume_ws(builder);
                    if action == "read" {
                        self.parse_call_args(builder);
                        IrExpr::ServiceIndicatorRead
                    } else {
                        IrExpr::State { name }
                    }
                } else if name == "service" && namespace == "wifi" && self.at_kind(TokenKind::Dot) {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let action = self.consume_ident(builder).unwrap_or_default();
                    self.consume_ws(builder);
                    IrExpr::Call {
                        name: format!("service.wifi.{action}"),
                        args: self.parse_call_args(builder),
                    }
                } else if self.at_kind(TokenKind::OpenParen) {
                    let call_name = if name == "wifi" {
                        format!("service.wifi.{namespace}")
                    } else {
                        format!("{name}.{namespace}")
                    };
                    IrExpr::Call {
                        name: call_name,
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
        self.at_qualified_ident("event", method)
    }

    fn at_qualified_ident(&self, namespace: &str, method: &str) -> bool {
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
            ) if text == namespace && method_text == method
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
