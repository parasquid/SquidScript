use crate::{
    ast::{AstFunction, AstHandler, AstScreen, AstTriggerBlock},
    diagnostic::SourceSpan,
    lexer::TokenKind,
    syntax::{SquidKind, SquidLang},
};
use rowan::{GreenNodeBuilder, Language};

use super::Parser;

impl Parser<'_> {
    pub(super) fn parse_screen(&mut self, builder: &mut GreenNodeBuilder, exported: bool) {
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
            exported,
            span: SourceSpan { start, end },
        });
    }

    pub(super) fn parse_event_handler(&mut self, builder: &mut GreenNodeBuilder, preload: bool) {
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
        self.consume_ws(builder);
        let param = if self.at_kind(TokenKind::Comma) {
            self.bump(builder);
            self.consume_ws(builder);
            self.consume_ident(builder)
        } else {
            None
        };
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
            param,
            preload,
            statements,
            span: SourceSpan { start, end },
        });
    }

    pub(super) fn parse_app_triggers(&mut self, builder: &mut GreenNodeBuilder) {
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

    pub(super) fn parse_function(&mut self, builder: &mut GreenNodeBuilder, exported: bool) {
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
            exported,
            span: SourceSpan { start, end },
        });
    }

    pub(super) fn parse_param_list(&mut self, builder: &mut GreenNodeBuilder) -> Vec<String> {
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
}
