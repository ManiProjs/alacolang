use crate::language::BinaryOp;
use miette::SourceSpan;

impl From<Span> for SourceSpan {
    fn from(span: Span) -> Self {
        SourceSpan::new(span.start.into(), (span.end - span.start).into())
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Import(Import),
    Struct(Struct),
    Function(Function),
    Statement(Stmt),
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum MatchPattern {
    Number(String),
    String(String),
    Bool(bool),
    Identifier(String),
    Wildcard,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },

    Return(Option<Expr>, Span),

    Stop(Option<Expr>, Span),

    Skip(Span),

    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
        span: Span,
    },

    Loop {
        kind: LoopKind,
        binding: Option<String>,
        body: Block,
        span: Span,
    },

    Expr(Expr, Span),

    Match {
        expression: Expr,
        arms: Vec<MatchArm>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum LoopKind {
    Infinite,
    Repeat(Expr),
    While(Expr),

    For { variable: String, iterable: Expr },
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(String, Span),
    String(String, Span),
    Bool(bool, Span),

    Identifier(String, Span),

    StructLiteral {
        name: String,
        fields: Vec<(String, Expr)>,
        span: Span,
    },

    Binary {
        left: Box<Expr>,
        operator: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },

    Unary {
        operator: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },

    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
        span: Span,
    },

    Member {
        object: Box<Expr>,
        member: String,
        span: Span,
    },

    Shell {
        command: String,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Number(_, span) => *span,
            Expr::String(_, span) => *span,
            Expr::Bool(_, span) => *span,
            Expr::Identifier(_, span) => *span,
            Expr::StructLiteral { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Member { span, .. } => *span,
            Expr::Shell { span, .. } => *span,
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. } => *span,
            Stmt::Return(_, span) => *span,
            Stmt::Stop(_, span) => *span,
            Stmt::Skip(span) => *span,
            Stmt::If { span, .. } => *span,
            Stmt::Loop { span, .. } => *span,
            Stmt::Expr(_, span) => *span,
            Stmt::Match { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Negate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Shell,
    ShellResult,
    Void,
    Named(String),
}
