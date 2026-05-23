use crate::{
    diagnostic::SourceSpan,
    ir::{IrDeviceBinding, IrStateValue, IrStatement},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AstRoot {
    pub app: Option<AstAppDecl>,
    pub state: Option<AstStateBlock>,
    pub device_bindings: Vec<IrDeviceBinding>,
    pub trigger_blocks: Vec<AstTriggerBlock>,
    pub functions: Vec<AstFunction>,
    pub handlers: Vec<AstHandler>,
    pub screens: Vec<AstScreen>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstAppDecl {
    pub id: String,
    pub name: String,
    pub target: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstStateBlock {
    pub selected_default: i64,
    pub store: String,
    pub values: Vec<IrStateValue>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstScreen {
    pub name: String,
    pub render: String,
    pub statements: Vec<IrStatement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstHandler {
    pub event: String,
    pub preload: bool,
    pub statements: Vec<IrStatement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstTriggerBlock {
    pub statements: Vec<IrStatement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstFunction {
    pub name: String,
    pub params: Vec<String>,
    pub statements: Vec<IrStatement>,
    pub span: SourceSpan,
}
