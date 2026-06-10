use crate::{ir::IrExpr, lexer::TokenKind};
use rowan::GreenNodeBuilder;

use super::Parser;

impl Parser<'_> {
    pub(super) fn parse_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
        self.parse_comparison_expr(builder)
    }

    pub(super) fn parse_comparison_expr(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<IrExpr> {
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

    pub(super) fn parse_additive_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
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

    pub(super) fn parse_unary_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
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

    pub(super) fn parse_postfix_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
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

    pub(super) fn parse_primary_expr(&mut self, builder: &mut GreenNodeBuilder) -> Option<IrExpr> {
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
        } else if self.at_kind(TokenKind::At) {
            self.bump(builder);
            self.consume_ws(builder);
            IrExpr::State {
                name: self.consume_ident(builder)?,
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
                } else if name == "system" && namespace == "startReason" {
                    self.parse_call_args(builder);
                    IrExpr::SystemStartReason
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
                } else if name == "service"
                    && namespace == "display"
                    && self.at_kind(TokenKind::Dot)
                {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let action = self.consume_ident(builder).unwrap_or_default();
                    self.consume_ws(builder);
                    IrExpr::Call {
                        name: format!("service.display.{action}"),
                        args: self.parse_call_args(builder),
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
                } else if name == "device" && namespace == "config" && self.at_kind(TokenKind::Dot)
                {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let action = self.consume_ident(builder).unwrap_or_default();
                    self.consume_ws(builder);
                    IrExpr::Call {
                        name: format!("device.config.{action}"),
                        args: self.parse_call_args(builder),
                    }
                } else if name == "app"
                    && matches!(namespace.as_str(), "registry" | "armedStack")
                    && self.at_kind(TokenKind::Dot)
                {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let action = self.consume_ident(builder).unwrap_or_default();
                    self.consume_ws(builder);
                    IrExpr::Call {
                        name: format!("app.{namespace}.{action}"),
                        args: self.parse_call_args(builder),
                    }
                } else if name == "content"
                    && namespace == "binbook"
                    && self.at_kind(TokenKind::Dot)
                {
                    self.bump(builder);
                    self.consume_ws(builder);
                    let action = self.consume_ident(builder).unwrap_or_default();
                    self.consume_ws(builder);
                    IrExpr::Call {
                        name: format!("content.binbook.{action}"),
                        args: self.parse_call_args(builder),
                    }
                } else if name == "state" {
                    if self.at_kind(TokenKind::OpenParen) {
                        IrExpr::Call {
                            name: format!("state.{namespace}"),
                            args: self.parse_call_args(builder),
                        }
                    } else {
                        IrExpr::State { name: namespace }
                    }
                } else if self.at_kind(TokenKind::OpenParen) {
                    let call_name = if name == "wifi" {
                        format!("service.wifi.{namespace}")
                    } else if name == "display" {
                        format!("service.display.{namespace}")
                    } else {
                        format!("{name}.{namespace}")
                    };
                    IrExpr::Call {
                        name: call_name,
                        args: self.parse_call_args(builder),
                    }
                } else {
                    IrExpr::Field {
                        target: Box::new(IrExpr::Variable { name }),
                        field: namespace,
                    }
                }
            } else if self.at_kind(TokenKind::OpenParen) {
                IrExpr::Call {
                    name,
                    args: self.parse_call_args(builder),
                }
            } else {
                IrExpr::Variable { name }
            }
        } else {
            return None;
        };
        Some(expr)
    }

    pub(super) fn parse_call_args(&mut self, builder: &mut GreenNodeBuilder) -> Vec<IrExpr> {
        if !self.at_kind(TokenKind::OpenParen) {
            return Vec::new();
        }
        self.bump(builder);
        self.parse_call_args_after_open(builder)
    }

    pub(super) fn parse_call_args_after_open(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Vec<IrExpr> {
        let mut args = Vec::new();
        while !self.at_end() {
            self.consume_ws(builder);
            if self.at_kind(TokenKind::CloseParen) {
                self.bump(builder);
                break;
            }
            if self.at_kind(TokenKind::OpenBrace) {
                args.push(IrExpr::Literal {
                    value: self.parse_options_object(builder),
                });
            } else if let Some(arg) = self.parse_expr(builder) {
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

    pub(super) fn consume_binary_operator(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<String> {
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
}
