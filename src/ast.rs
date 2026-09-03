use crate::language::BinaryOp;

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
    Function(Function),
    Statement(Stmt),
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
    },

    Return(Option<Expr>),

    Stop(Option<Expr>),

    Skip,

    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },

    Loop {
        kind: LoopKind,
        binding: Option<String>,
        body: Block,
    },

    Expr(Expr),

    Match {
        expression: Expr,
        arms: Vec<MatchArm>,
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
    Number(String),
    String(String),
    Bool(bool),

    Identifier(String),

    Binary {
        left: Box<Expr>,
        operator: BinaryOp,
        right: Box<Expr>,
    },

    Unary {
        operator: UnaryOp,
        operand: Box<Expr>,
    },

    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },

    Member {
        object: Box<Expr>,
        member: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Negate,
}

#[derive(Debug, Clone)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Named(String),
}
