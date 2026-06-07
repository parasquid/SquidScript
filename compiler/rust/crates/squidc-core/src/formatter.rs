use crate::{
    lexer::{lex, LexToken, TokenKind},
    parser,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    pub message: String,
}

pub fn format_source(source: &str) -> Result<String, FormatError> {
    let parsed = parser::parse(source);
    if !parsed.diagnostics.is_empty() {
        return Err(FormatError {
            message: "cannot format source with parser diagnostics".to_string(),
        });
    }
    Ok(Formatter::new(lex(source)).format())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    Brace,
    Bracket,
    Paren,
}

struct Formatter {
    tokens: Vec<LexToken>,
    cursor: usize,
    out: String,
    indent: usize,
    contexts: Vec<Context>,
    line_start: bool,
    previous_kind: Option<TokenKind>,
    previous_text: Option<String>,
}

impl Formatter {
    fn new(tokens: Vec<LexToken>) -> Self {
        Self {
            tokens: tokens
                .into_iter()
                .filter(|token| token.kind != TokenKind::Whitespace)
                .collect(),
            cursor: 0,
            out: String::new(),
            indent: 0,
            contexts: Vec::new(),
            line_start: true,
            previous_kind: None,
            previous_text: None,
        }
    }

    fn format(mut self) -> String {
        while self.cursor < self.tokens.len() {
            let token = self.tokens[self.cursor].clone();
            match token.kind {
                TokenKind::OpenBrace => self.open_brace(),
                TokenKind::CloseBrace => self.close_brace(),
                TokenKind::OpenBracket => {
                    self.write("[");
                    self.contexts.push(Context::Bracket);
                }
                TokenKind::CloseBracket => {
                    self.write("]");
                    self.contexts.pop();
                }
                TokenKind::OpenParen => {
                    if self.previous_text.as_deref() == Some("if") {
                        self.space();
                    }
                    self.write("(");
                    self.contexts.push(Context::Paren);
                }
                TokenKind::CloseParen => {
                    self.write(")");
                    self.contexts.pop();
                }
                TokenKind::Comma => self.comma(),
                TokenKind::Colon => {
                    self.write(":");
                    self.space();
                }
                TokenKind::Dot => self.write("."),
                TokenKind::Equals => {
                    if self.next_kind() == Some(TokenKind::Equals) {
                        self.binary_operator("==");
                        self.cursor += 1;
                    } else {
                        self.binary_operator("=");
                    }
                }
                TokenKind::Bang if self.next_kind() == Some(TokenKind::Equals) => {
                    self.binary_operator("!=");
                    self.cursor += 1;
                }
                TokenKind::Less => {
                    if self.next_kind() == Some(TokenKind::Equals) {
                        self.binary_operator("<=");
                        self.cursor += 1;
                    } else {
                        self.binary_operator("<");
                    }
                }
                TokenKind::Greater => {
                    if self.next_kind() == Some(TokenKind::Equals) {
                        self.binary_operator(">=");
                        self.cursor += 1;
                    } else {
                        self.binary_operator(">");
                    }
                }
                TokenKind::Plus => self.binary_operator("+"),
                TokenKind::Minus => self.binary_operator("-"),
                TokenKind::Ident | TokenKind::String | TokenKind::Number | TokenKind::At => {
                    self.word(&token.text)
                }
                TokenKind::Bang | TokenKind::Question | TokenKind::Unknown => {
                    self.write(&token.text)
                }
                TokenKind::Whitespace => {}
            }
            self.previous_kind = Some(token.kind);
            self.previous_text = Some(token.text);
            self.cursor += 1;
        }
        self.finish()
    }

    fn open_brace(&mut self) {
        if !self.line_start {
            self.space();
        }
        self.write("{");
        self.contexts.push(Context::Brace);
        self.indent += 1;
        self.newline();
    }

    fn close_brace(&mut self) {
        if !self.line_start {
            self.newline();
        }
        self.indent = self.indent.saturating_sub(1);
        self.write("}");
        self.contexts.pop();
        if self.contexts.is_empty() {
            self.blank_line();
            if self.has_next_non_trailing_comma() {
                self.blank_line();
            }
        }
    }

    fn comma(&mut self) {
        if matches!(
            self.next_kind(),
            Some(TokenKind::CloseBrace | TokenKind::CloseBracket | TokenKind::CloseParen)
        ) {
            return;
        }
        match self.contexts.last().copied() {
            Some(Context::Paren | Context::Bracket) => {
                self.write(",");
                self.space();
            }
            Some(Context::Brace) => self.newline(),
            None => self.newline(),
        }
    }

    fn binary_operator(&mut self, op: &str) {
        self.space();
        self.write(op);
        self.space();
    }

    fn word(&mut self, text: &str) {
        self.break_before_word_if_needed(text);
        if self.needs_space_before_word() {
            self.space();
        }
        self.write(text);
    }

    fn break_before_word_if_needed(&mut self, text: &str) {
        if self.line_start || self.out.is_empty() || self.previous_kind == Some(TokenKind::Dot) {
            return;
        }
        if self.previous_text.as_deref() == Some("let") {
            return;
        }
        if self.contexts.is_empty() && is_top_level_keyword(text) {
            self.blank_line();
            return;
        }
        if self.contexts.last() == Some(&Context::Brace) {
            let previous_ends_statement = matches!(
                self.previous_kind,
                Some(
                    TokenKind::CloseParen
                        | TokenKind::CloseBrace
                        | TokenKind::CloseBracket
                        | TokenKind::Ident
                        | TokenKind::String
                        | TokenKind::Number
                )
            );
            if previous_ends_statement && is_statement_or_field_start(text) {
                self.newline();
            }
        }
    }

    fn needs_space_before_word(&self) -> bool {
        if self.line_start || self.out.is_empty() {
            return false;
        }
        let Some(last) = self.out.as_bytes().last().copied() else {
            return false;
        };
        last.is_ascii_alphanumeric() || last == b'"' || last == b']'
    }

    fn write(&mut self, text: &str) {
        if self.line_start {
            for _ in 0..self.indent {
                self.out.push_str("  ");
            }
            self.line_start = false;
        }
        self.out.push_str(text);
    }

    fn space(&mut self) {
        if !self.line_start && !self.out.ends_with([' ', '\n', '(', '[', '.', '@']) {
            self.out.push(' ');
        }
    }

    fn newline(&mut self) {
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.line_start = true;
    }

    fn blank_line(&mut self) {
        self.newline();
        if !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
        self.line_start = true;
    }

    fn finish(mut self) -> String {
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }

    fn next_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.cursor + 1).map(|token| token.kind)
    }

    fn has_next_non_trailing_comma(&self) -> bool {
        self.tokens
            .iter()
            .skip(self.cursor + 1)
            .any(|token| token.kind != TokenKind::Comma)
    }
}

fn is_top_level_keyword(text: &str) -> bool {
    matches!(
        text,
        "app"
            | "import"
            | "requires"
            | "state"
            | "device"
            | "event"
            | "function"
            | "export"
            | "screen"
    )
}

fn is_statement_or_field_start(text: &str) -> bool {
    matches!(
        text,
        "if" | "let"
            | "return"
            | "repeat"
            | "for"
            | "debug"
            | "state"
            | "screen"
            | "app"
            | "service"
            | "hardware"
            | "device"
            | "system"
    ) || text
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::format_source;

    #[test]
    fn formats_app_state_and_handler_canonically() {
        let source = r#"app   "demo"   target   "portable"
state{count:int=0,label:string="ready",}
event.on("app.start"){state.load()if(state.count!=0){state.reset()}debug.print("ready",state.count)}
"#;

        let formatted = format_source(source).unwrap();

        assert_eq!(
            formatted,
            r#"app "demo" target "portable"

state {
  count: int = 0
  label: string = "ready"
}

event.on("app.start") {
  state.load()
  if (state.count != 0) {
    state.reset()
  }
  debug.print("ready", state.count)
}
"#
        );
    }

    #[test]
    fn formats_nested_objects_lists_and_ble_install_syntax() {
        let source = r#"app "ble-install"
event.on("app.start"){service.ble.start("file-transfer",{id:"sqbc-install",accept:[".sqbc",],events:{complete:"ble.file.complete",},})}
event.on("ble.file.complete",ev){let installed=app.install(ev.upload)app.launch(installed.id)}
"#;

        let formatted = format_source(source).unwrap();

        assert_eq!(
            formatted,
            r#"app "ble-install"

event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "sqbc-install"
    accept: [".sqbc"]
    events: {
      complete: "ble.file.complete"
    }
  })
}

event.on("ble.file.complete", ev) {
  let installed = app.install(ev.upload)
  app.launch(installed.id)
}
"#
        );
    }

    #[test]
    fn formatter_is_idempotent() {
        let source = r#"app "demo"

state {
  count: int = 0
}

event.on("app.start") {
  state.load()
}
"#;

        let once = format_source(source).unwrap();
        let twice = format_source(&once).unwrap();

        assert_eq!(twice, once);
    }

    #[test]
    fn preserves_comparison_operators_and_splits_identifier_ended_statements() {
        let source = r#"app "wifi"
event.on("timer.debug"){state.clients=status.clients if(state.clients<=0){state.led=false service.indicator.breathe()}if(state.ticks>=8){state.led=true service.indicator.write(state.led)}}
"#;

        let formatted = format_source(source).unwrap();

        assert_eq!(
            formatted,
            r#"app "wifi"

event.on("timer.debug") {
  state.clients = status.clients
  if (state.clients <= 0) {
    state.led = false
    service.indicator.breathe()
  }
  if (state.ticks >= 8) {
    state.led = true
    service.indicator.write(state.led)
  }
}
"#
        );
        assert_eq!(format_source(&formatted).unwrap(), formatted);
    }

    #[test]
    fn keeps_let_binding_name_on_same_line() {
        let source = r#"app "demo"
event.on("app.start"){let installed=app.install(ev.upload)app.launch(installed.id)}
"#;

        let formatted = format_source(source).unwrap();

        assert_eq!(
            formatted,
            r#"app "demo"

event.on("app.start") {
  let installed = app.install(ev.upload)
  app.launch(installed.id)
}
"#
        );
        assert_eq!(format_source(&formatted).unwrap(), formatted);
    }

    #[test]
    fn keeps_object_field_after_list_on_own_line() {
        let source = r#"app "demo"
event.on("app.start"){service.ble.start("file-transfer",{id:"sqbc-install",accept:[".sqbc"],events:{complete:"ble.file.complete"}})}
"#;

        let formatted = format_source(source).unwrap();

        assert_eq!(
            formatted,
            r#"app "demo"

event.on("app.start") {
  service.ble.start("file-transfer", {
    id: "sqbc-install"
    accept: [".sqbc"]
    events: {
      complete: "ble.file.complete"
    }
  })
}
"#
        );
        assert_eq!(format_source(&formatted).unwrap(), formatted);
    }

    #[test]
    fn rejects_invalid_source() {
        let error = format_source("capability \"demo\"").unwrap_err();

        assert!(error
            .message
            .contains("cannot format source with parser diagnostics"));
    }
}
