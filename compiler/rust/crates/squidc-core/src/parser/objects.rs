use crate::lexer::TokenKind;
use rowan::GreenNodeBuilder;

use super::Parser;

impl Parser<'_> {
    pub(super) fn parse_options_object(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> serde_json::Value {
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

    pub(super) fn parse_static_options_object(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> serde_json::Value {
        self.consume_ws(builder);
        if !self.at_kind(TokenKind::OpenBrace) {
            return serde_json::json!({});
        }
        self.parse_static_object_after_open(builder)
    }

    pub(super) fn parse_static_object_after_open(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        self.bump(builder);
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseBrace) {
                self.bump(builder);
                break;
            }
            let Some(key) = self.consume_ident(builder) else {
                self.bump(builder);
                continue;
            };
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Colon) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            let value = self
                .parse_static_value(builder)
                .unwrap_or(serde_json::Value::Null);
            map.insert(key, value);
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Comma) {
                self.bump(builder);
            }
        }
        serde_json::Value::Object(map)
    }

    pub(super) fn parse_static_array_after_open(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> serde_json::Value {
        let mut values = Vec::new();
        self.bump(builder);
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseBracket) {
                self.bump(builder);
                break;
            }
            values.push(
                self.parse_static_value(builder)
                    .unwrap_or(serde_json::Value::Null),
            );
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Comma) {
                self.bump(builder);
            }
        }
        serde_json::Value::Array(values)
    }

    pub(super) fn parse_static_value(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<serde_json::Value> {
        self.consume_ws(builder);
        if self.at_kind(TokenKind::String) {
            return self
                .consume_string(builder)
                .map(|value| serde_json::Value::String(value));
        }
        if self.at_kind(TokenKind::Number) {
            return self
                .consume_number(builder)
                .map(|value| serde_json::json!(value));
        }
        if self.at_kind(TokenKind::Ident) {
            let ident = self.consume_ident(builder)?;
            return Some(match ident.as_str() {
                "true" => serde_json::Value::Bool(true),
                "false" => serde_json::Value::Bool(false),
                "null" => serde_json::Value::Null,
                _ => serde_json::Value::String(ident),
            });
        }
        if self.at_kind(TokenKind::OpenBrace) {
            return Some(self.parse_static_object_after_open(builder));
        }
        if self.at_kind(TokenKind::OpenBracket) {
            return Some(self.parse_static_array_after_open(builder));
        }
        None
    }

    pub(super) fn consume_literal_value(
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
}
