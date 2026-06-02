use crate::{
    ast::AstRoot,
    diagnostic::{Diagnostic, SourceSpan},
    lexer::{lex, LexToken},
    syntax::{SquidKind, SquidLang},
};
use rowan::{GreenNode, GreenNodeBuilder, Language};

mod declarations;
mod expressions;
mod handlers;
mod objects;
mod statements;
mod tokens;

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

pub(super) fn state_default_matches(
    value_type: &str,
    nullable: bool,
    value: &serde_json::Value,
) -> bool {
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

pub(super) struct Parser<'a> {
    tokens: &'a [LexToken],
    cursor: usize,
    ast: AstRoot,
    diagnostics: Vec<Diagnostic>,
    pending_preload: Option<SourceSpan>,
}

pub(super) fn titleize(id: &str) -> String {
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
