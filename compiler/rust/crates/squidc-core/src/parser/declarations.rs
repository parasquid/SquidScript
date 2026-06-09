use crate::{
    ast::{AstAppDecl, AstImport, AstStateBlock, AstStateRequirement},
    device_config,
    diagnostic::{error, SourceSpan},
    ir::{default_state_store, IrDeviceBinding, IrStateValue},
    lexer::TokenKind,
    syntax::{SquidKind, SquidLang},
};
use rowan::{GreenNodeBuilder, Language};
use std::collections::BTreeSet;

use super::{state_default_matches, titleize, Parser};

impl Parser<'_> {
    pub(super) fn parse_root(&mut self, builder: &mut GreenNodeBuilder) {
        while !self.at_end() {
            self.consume_ws(builder);

            if self.at_kind(TokenKind::At) {
                self.parse_attribute(builder);
                continue;
            }

            if self.at_ident("import") {
                self.reject_pending_attribute();
                self.parse_import(builder);
            } else if self.at_ident("requires") {
                self.reject_pending_attribute();
                self.parse_required_state(builder);
            } else if self.at_ident("export") {
                self.reject_pending_attribute();
                self.parse_exported_decl(builder);
            } else if self.at_qualified_ident("app", "triggers") {
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
                self.parse_function(builder, false);
            } else if self.at_ident("screen") {
                self.reject_pending_attribute();
                self.parse_screen(builder, false);
            } else if !self.at_end() {
                self.reject_pending_attribute();
                self.parse_unexpected_top_level(builder);
            }
        }
    }

    pub(super) fn parse_unexpected_top_level(&mut self, builder: &mut GreenNodeBuilder) {
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder);
        let end = self.previous_end().unwrap_or(start);
        self.diagnostics.push(error(
            "E_UNEXPECTED_TOP_LEVEL",
            "unexpected top-level declaration",
            start,
            end,
        ));
    }

    pub(super) fn parse_import(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder);
        self.consume_ws(builder);
        let alias = self.consume_ident(builder).unwrap_or_default();
        self.consume_ws(builder);
        if self.at_ident("from") {
            self.bump(builder);
        } else {
            self.diagnostics.push(error(
                "E_IMPORT_SYNTAX",
                "import must use: import alias from \"path\"",
                start,
                self.previous_end().unwrap_or(start),
            ));
        }
        self.consume_ws(builder);
        let path = self.consume_string(builder).unwrap_or_default();
        let end = self.previous_end().unwrap_or(start);
        builder.finish_node();
        if !alias.is_empty() && !path.is_empty() {
            self.ast.imports.push(AstImport {
                alias,
                path,
                span: SourceSpan { start, end },
            });
        }
    }

    pub(super) fn parse_exported_decl(&mut self, builder: &mut GreenNodeBuilder) {
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder);
        self.consume_ws(builder);
        if self.at_ident("function") {
            self.parse_function(builder, true);
        } else if self.at_ident("screen") {
            self.parse_screen(builder, true);
        } else {
            self.diagnostics.push(error(
                "E_EXPORT_TARGET",
                "export is only valid before function or screen",
                start,
                self.previous_end().unwrap_or(start),
            ));
        }
    }

    pub(super) fn parse_required_state(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::Token));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        self.bump(builder); // requires
        self.consume_ws(builder);
        if self.at_ident("state") {
            self.bump(builder);
        }
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
            let field_start = self.peek().map(|token| token.span.start).unwrap_or(start);
            let Some(name) = self.consume_ident(builder) else {
                self.bump(builder);
                continue;
            };
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Colon) {
                self.bump(builder);
            }
            self.consume_ws(builder);
            let value_type = self.consume_ident(builder).unwrap_or_default();
            self.consume_ws(builder);
            let nullable = if self.at_kind(TokenKind::Question) {
                self.bump(builder);
                true
            } else {
                false
            };
            let field_end = self.previous_end().unwrap_or(field_start);
            if !value_type.is_empty() {
                self.ast.required_state.push(AstStateRequirement {
                    name,
                    value_type,
                    nullable,
                    span: SourceSpan {
                        start: field_start,
                        end: field_end,
                    },
                });
            }
            self.consume_ws(builder);
            self.consume_comma(builder);
        }
        builder.finish_node();
    }

    pub(super) fn parse_attribute(&mut self, builder: &mut GreenNodeBuilder) {
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

    pub(super) fn reject_pending_attribute(&mut self) {
        if let Some(span) = self.pending_preload.take() {
            self.diagnostics.push(error(
                "E_ATTRIBUTE_TARGET",
                "@preload is only valid before event.on handlers",
                span.start,
                span.end,
            ));
        }
    }

    pub(super) fn parse_app(&mut self, builder: &mut GreenNodeBuilder) {
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
            if self.ast.app.is_some() {
                self.diagnostics.push(error(
                    "E_DUPLICATE_APP",
                    "app declaration must be unique",
                    start,
                    end,
                ));
            }
            self.ast.app = Some(AstAppDecl {
                name: titleize(&id),
                id,
                target: target.clone(),
                span: SourceSpan { start, end },
            });
        }
    }

    pub(super) fn parse_state(&mut self, builder: &mut GreenNodeBuilder) {
        builder.start_node(SquidLang::kind_to_raw(SquidKind::StateBlock));
        let start = self.peek().map(|token| token.span.start).unwrap_or(0);
        let mut selected_default = 0;
        let mut store = default_state_store();
        let mut values = Vec::new();
        let mut field_names = BTreeSet::new();

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
                        if !field_names.insert(name.clone()) {
                            self.diagnostics.push(error(
                                "E_DUPLICATE_STATE_FIELD",
                                "state field names must be unique",
                                start,
                                self.previous_end().unwrap_or(start),
                            ));
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
        if self.ast.state.is_some() {
            self.diagnostics.push(error(
                "E_DUPLICATE_STATE_BLOCK",
                "state block must be unique",
                start,
                end,
            ));
        }
        self.ast.state = Some(AstStateBlock {
            selected_default,
            store,
            values,
            span: SourceSpan { start, end },
        });
    }

    pub(super) fn parse_device(&mut self, builder: &mut GreenNodeBuilder) {
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
            if !device_config::is_safe_device_binding_resource(&resource) {
                self.diagnostics.push(error(
                    "E_DEVICE_PATH",
                    "device binding must use a safe package-relative .sqdevice path, gpio:GPIO pin, or gpio-button binding",
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
}
