use crate::{
    lexer::{syntax_kind_for, LexToken, TokenKind},
    syntax::SquidLang,
};
use rowan::{GreenNodeBuilder, Language};

use super::Parser;

impl Parser<'_> {
    pub(super) fn consume_comma(&mut self, builder: &mut GreenNodeBuilder) {
        self.consume_ws(builder);
        if self.at_kind(TokenKind::Comma) {
            self.bump(builder);
        }
        self.consume_ws(builder);
    }

    pub(super) fn consume_call_tail(&mut self, builder: &mut GreenNodeBuilder) {
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

    pub(super) fn consume_ws(&mut self, builder: &mut GreenNodeBuilder) {
        while self.at_kind(TokenKind::Whitespace) {
            self.bump(builder);
        }
    }

    pub(super) fn consume_string(&mut self, builder: &mut GreenNodeBuilder) -> Option<String> {
        if !self.at_kind(TokenKind::String) {
            return None;
        }
        let text = self.peek()?.text.clone();
        self.bump(builder);
        Some(text.trim_matches('"').to_string())
    }

    pub(super) fn consume_symbolic_screen_ref(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<String> {
        self.consume_ws(builder);
        let first = self.consume_ident(builder)?;
        self.consume_ws(builder);
        if !self.at_kind(TokenKind::Dot) {
            return Some(first);
        }
        self.bump(builder);
        self.consume_ws(builder);
        let second = self.consume_ident(builder)?;
        Some(format!("{first}.{second}"))
    }

    pub(super) fn consume_ident(&mut self, builder: &mut GreenNodeBuilder) -> Option<String> {
        if !self.at_kind(TokenKind::Ident) {
            return None;
        }
        let text = self.peek()?.text.clone();
        self.bump(builder);
        Some(text)
    }

    pub(super) fn consume_number(&mut self, builder: &mut GreenNodeBuilder) -> Option<i64> {
        if !self.at_kind(TokenKind::Number) {
            return None;
        }
        let value = self.peek()?.text.parse().ok();
        self.bump(builder);
        value
    }

    pub(super) fn at_ident(&self, ident: &str) -> bool {
        self.peek()
            .map(|token| token.kind == TokenKind::Ident && token.text == ident)
            .unwrap_or(false)
    }

    pub(super) fn at_event_method(&self, method: &str) -> bool {
        self.at_qualified_ident("event", method)
    }

    pub(super) fn at_qualified_ident(&self, namespace: &str, method: &str) -> bool {
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

    pub(super) fn at_kind(&self, kind: TokenKind) -> bool {
        self.peek().map(|token| token.kind == kind).unwrap_or(false)
    }

    pub(super) fn next_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.cursor + 1).map(|token| token.kind)
    }

    pub(super) fn at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    pub(super) fn peek(&self) -> Option<&LexToken> {
        self.tokens.get(self.cursor)
    }

    pub(super) fn previous_end(&self) -> Option<usize> {
        self.cursor
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map(|token| token.span.end)
    }

    pub(super) fn bump(&mut self, builder: &mut GreenNodeBuilder) {
        if let Some(token) = self.tokens.get(self.cursor) {
            builder.token(
                SquidLang::kind_to_raw(syntax_kind_for(token.kind)),
                &token.text,
            );
            self.cursor += 1;
        }
    }
}
