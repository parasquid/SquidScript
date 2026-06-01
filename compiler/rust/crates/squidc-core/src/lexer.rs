use crate::{diagnostic::SourceSpan, syntax::SquidKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident,
    String,
    Number,
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
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
pub(crate) struct LexToken {
    pub(crate) kind: TokenKind,
    pub(crate) text: String,
    pub(crate) span: SourceSpan,
}

pub(crate) fn lex(source: &str) -> Vec<LexToken> {
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
                '[' => TokenKind::OpenBracket,
                ']' => TokenKind::CloseBracket,
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

pub(crate) fn syntax_kind_for(kind: TokenKind) -> SquidKind {
    match kind {
        TokenKind::Ident => SquidKind::Ident,
        TokenKind::String => SquidKind::String,
        TokenKind::Number => SquidKind::Number,
        TokenKind::Whitespace => SquidKind::Whitespace,
        TokenKind::Unknown => SquidKind::Error,
        TokenKind::OpenBrace
        | TokenKind::CloseBrace
        | TokenKind::OpenBracket
        | TokenKind::CloseBracket
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
