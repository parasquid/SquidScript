use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub span: SourceSpan,
}

pub(crate) fn error(
    code: &str,
    message: impl Into<String>,
    start: usize,
    end: usize,
) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: "error".to_string(),
        message: message.into(),
        span: SourceSpan { start, end },
    }
}

pub(crate) fn warning(code: &str, message: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: "warning".to_string(),
        message: message.to_string(),
        span: SourceSpan { start, end },
    }
}
