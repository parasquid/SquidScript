use rowan::{Language, SyntaxKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SquidKind {
    Root = 0,
    AppDecl = 1,
    StateBlock = 2,
    Ident = 3,
    String = 4,
    Number = 5,
    Token = 6,
    Whitespace = 7,
    Error = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SquidLang {}

impl Language for SquidLang {
    type Kind = SquidKind;

    fn kind_from_raw(raw: SyntaxKind) -> Self::Kind {
        match raw.0 {
            0 => SquidKind::Root,
            1 => SquidKind::AppDecl,
            2 => SquidKind::StateBlock,
            3 => SquidKind::Ident,
            4 => SquidKind::String,
            5 => SquidKind::Number,
            6 => SquidKind::Token,
            7 => SquidKind::Whitespace,
            _ => SquidKind::Error,
        }
    }

    fn kind_to_raw(kind: Self::Kind) -> SyntaxKind {
        SyntaxKind(kind as u16)
    }
}
