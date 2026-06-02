use crate::{
    ir::{IrExpr, IrStatement},
    lexer::TokenKind,
};
use rowan::GreenNodeBuilder;
use std::collections::BTreeMap;

use super::Parser;

impl Parser<'_> {
    pub(super) fn parse_statements_until_close(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Vec<IrStatement> {
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

    pub(super) fn parse_statement(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<IrStatement> {
        if self.at_kind(TokenKind::At) {
            self.bump(builder);
            self.consume_ws(builder);
            let name = self.consume_ident(builder)?;
            self.consume_ws(builder);
            if self.at_kind(TokenKind::Equals) {
                self.bump(builder);
                self.consume_ws(builder);
                let expr = self.parse_expr(builder)?;
                return Some(IrStatement::StateAssign { name, expr });
            }
            return None;
        }

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

        if first == "state" && self.at_kind(TokenKind::Equals) {
            self.bump(builder);
            self.consume_ws(builder);
            let expr = self.parse_expr(builder)?;
            return Some(IrStatement::StateAssign { name: method, expr });
        }

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
            && matches!(
                method.as_str(),
                "timer" | "display" | "indicator" | "wifi" | "power" | "ble"
            )
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
            if method == "ble" {
                return match action.as_str() {
                    "profile" => {
                        let profile = self.consume_string(builder).unwrap_or_default();
                        self.consume_comma(builder);
                        let options = self.parse_static_options_object(builder);
                        self.consume_call_tail(builder);
                        let id = options
                            .get("id")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let role = options
                            .get("role")
                            .and_then(|value| value.as_str())
                            .unwrap_or("server")
                            .to_string();
                        let accept = options
                            .get("accept")
                            .and_then(|value| value.as_array())
                            .map(|values| {
                                values
                                    .iter()
                                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let events = options
                            .get("events")
                            .and_then(|value| value.as_object())
                            .map(|events| {
                                events
                                    .iter()
                                    .filter_map(|(key, value)| {
                                        value.as_str().map(|value| (key.clone(), value.to_string()))
                                    })
                                    .collect::<BTreeMap<_, _>>()
                            })
                            .unwrap_or_default();
                        Some(IrStatement::ServiceBleProfile {
                            profile,
                            id,
                            role,
                            accept,
                            events,
                        })
                    }
                    _ => {
                        self.consume_call_tail(builder);
                        None
                    }
                };
            }
            if method == "power" {
                return match action.as_str() {
                    "sleep" => {
                        let options = self.parse_options_object(builder);
                        let wake_after_ms = options
                            .get("wakeAfterMs")
                            .and_then(|value| serde_json::from_value(value.clone()).ok())
                            .unwrap_or(IrExpr::Literal {
                                value: serde_json::json!(0),
                            });
                        self.consume_call_tail(builder);
                        Some(IrStatement::ServicePowerSleep { wake_after_ms })
                    }
                    _ => {
                        self.consume_call_tail(builder);
                        None
                    }
                };
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
                ("indicator", "blink") => {
                    let default_ms = || IrExpr::Literal {
                        value: serde_json::json!(500),
                    };
                    let on_ms = self.parse_expr(builder).unwrap_or_else(default_ms);
                    self.consume_comma(builder);
                    let off_ms = self.parse_expr(builder).unwrap_or_else(default_ms);
                    self.consume_call_tail(builder);
                    Some(IrStatement::ServiceIndicatorBlink { on_ms, off_ms })
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
                if !has_call_args {
                    return None;
                }
                self.consume_call_tail(builder);
                Some(IrStatement::StateLoad)
            }
            ("state", "save") => {
                if !has_call_args {
                    return None;
                }
                self.consume_call_tail(builder);
                Some(IrStatement::StateSave)
            }
            ("state", "reset") => {
                if !has_call_args {
                    return None;
                }
                self.consume_call_tail(builder);
                Some(IrStatement::StateReset)
            }
            ("screen", "refresh") => {
                self.consume_call_tail(builder);
                Some(IrStatement::ScreenRefresh)
            }
            ("screen", "open") => {
                let screen = self
                    .consume_string(builder)
                    .or_else(|| self.consume_symbolic_screen_ref(builder))
                    .unwrap_or_default();
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

    pub(super) fn parse_display_statement(
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
            "select" => {
                let name = self
                    .consume_string(builder)
                    .unwrap_or_else(|| "default".to_string());
                self.consume_call_tail(builder);
                Some(IrStatement::DisplaySelect { name })
            }
            "image" => {
                let path = self.consume_string(builder).unwrap_or_default();
                self.consume_ws(builder);
                if self.at_kind(TokenKind::Comma) {
                    self.bump(builder);
                }
                self.consume_ws(builder);
                let options = self.parse_options_object(builder);
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayImage { path, options })
            }
            "draw" => {
                let drawable = self.parse_expr(builder).unwrap_or(IrExpr::Literal {
                    value: serde_json::json!(null),
                });
                self.consume_ws(builder);
                if self.at_kind(TokenKind::Comma) {
                    self.bump(builder);
                }
                self.consume_ws(builder);
                let options = self.parse_options_object(builder);
                self.consume_call_tail(builder);
                Some(IrStatement::DisplayDraw { drawable, options })
            }
            _ => {
                self.consume_call_tail(builder);
                None
            }
        }
    }

    pub(super) fn parse_if_statement(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<IrStatement> {
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

    pub(super) fn parse_repeat_statement(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<IrStatement> {
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

    pub(super) fn parse_for_statement(
        &mut self,
        builder: &mut GreenNodeBuilder,
    ) -> Option<IrStatement> {
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
}
