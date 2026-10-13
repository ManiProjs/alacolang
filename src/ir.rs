//! Typed intermediate representation produced only after semantic analysis.
//!
//! Unlike the parser AST, every IR expression carries its inferred type and
//! the IR has its own statement/expression node types. Source spans are kept
//! for diagnostics, and item order is preserved for module code generation.

use std::collections::HashMap;

use crate::{
    ast::{self, Span, Type, UnaryOp},
    language::BinaryOp,
};

#[derive(Debug, Clone)]
pub struct IrProgram {
    pub items: Vec<IrItem>,
    pub functions: Vec<IrFunction>,
    pub structs: Vec<IrStruct>,
}

#[derive(Debug, Clone)]
pub enum IrItem {
    Function(String),
    Struct(String),
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub parameters: Vec<IrParameter>,
    pub return_type: Type,
    pub body: IrBlock,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrParameter {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrStruct {
    pub name: String,
    pub fields: Vec<IrStructField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IrStructField {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct IrBlock {
    pub statements: Vec<IrStmt>,
}

#[derive(Debug, Clone)]
pub enum IrStmt {
    Let {
        name: String,
        mutable: bool,
        ty: Type,
        value: IrExpr,
        span: Span,
    },
    Destructure {
        struct_name: String,
        mutable: bool,
        bindings: Vec<(String, String)>,
        value: IrExpr,
        span: Span,
    },
    Return(Option<IrExpr>, Span),
    Stop(Option<IrExpr>, Span),
    Skip(Span),
    If {
        condition: IrExpr,
        then_block: IrBlock,
        else_block: Option<IrBlock>,
        span: Span,
    },
    Loop {
        kind: IrLoopKind,
        binding: Option<String>,
        body: IrBlock,
        span: Span,
    },
    Expr(IrExpr, Span),
    Match {
        expression: IrExpr,
        arms: Vec<IrMatchArm>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum IrLoopKind {
    Infinite,
    Repeat(IrExpr),
    While(IrExpr),
    For { variable: String, iterable: IrExpr },
}

#[derive(Debug, Clone)]
pub struct IrMatchArm {
    pub pattern: IrMatchPattern,
    pub body: IrBlock,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrMatchPattern {
    Number(String),
    String(String),
    Bool(bool),
    Identifier(String),
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct IrExpr {
    pub kind: IrExprKind,
    pub ty: Type,
    pub span: Span,
}

/// A callable name/member is not a value expression, so it has no inferred
/// value type of its own. Keeping it separate prevents fake `Void` types in IR.
#[derive(Debug, Clone)]
pub enum IrCallee {
    Identifier(String),
    Member {
        object: Box<IrCallee>,
        member: String,
    },
    Value(Box<IrExpr>),
}

#[derive(Debug, Clone)]
pub enum IrPipeTarget {
    Call {
        callee: IrCallee,
        arguments: Vec<IrExpr>,
    },
    Callee(IrCallee),
    Value(Box<IrExpr>),
}

#[derive(Debug, Clone)]
pub enum IrExprKind {
    Number(String),
    String(String),
    Interpolation(Vec<String>),
    Bool(bool),
    Identifier(String),
    StructLiteral {
        name: String,
        fields: Vec<(String, IrExpr)>,
    },
    Binary {
        left: Box<IrExpr>,
        operator: BinaryOp,
        right: Box<IrExpr>,
    },
    Pipe {
        input: Box<IrExpr>,
        target: IrPipeTarget,
    },
    Unary {
        operator: UnaryOp,
        operand: Box<IrExpr>,
    },
    Call {
        callee: IrCallee,
        arguments: Vec<IrExpr>,
    },
    Member {
        object: Box<IrExpr>,
        member: String,
    },
    Shell {
        command: String,
    },
    Intrinsic {
        name: String,
        arguments: Vec<IrExpr>,
    },
}

impl IrProgram {
    pub fn from_validated(
        program: &ast::Program,
        expression_types: &HashMap<(usize, usize), Type>,
    ) -> Self {
        let items = program
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Function(function) => Some(IrItem::Function(function.name.clone())),
                ast::Item::Struct(structure) => Some(IrItem::Struct(structure.name.clone())),
                ast::Item::Import(_) | ast::Item::Statement(_) => None,
            })
            .collect();
        let functions = program
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Function(function) => Some(IrFunction {
                    name: function.name.clone(),
                    parameters: function
                        .params
                        .iter()
                        .map(|param| IrParameter {
                            name: param.name.clone(),
                            ty: param.ty.clone(),
                            span: param.span,
                        })
                        .collect(),
                    return_type: function.return_type.clone().unwrap_or(Type::Void),
                    body: lower_block(&function.body, expression_types),
                    span: function.span,
                }),
                _ => None,
            })
            .collect();
        let structs = program
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Struct(structure) => Some(IrStruct {
                    name: structure.name.clone(),
                    fields: structure
                        .fields
                        .iter()
                        .map(|field| IrStructField {
                            name: field.name.clone(),
                            ty: field.ty.clone(),
                            span: field.span,
                        })
                        .collect(),
                    span: structure.span,
                }),
                _ => None,
            })
            .collect();
        Self {
            items,
            functions,
            structs,
        }
    }

    pub fn function(&self, name: &str) -> Option<&IrFunction> {
        self.functions.iter().find(|function| function.name == name)
    }
}

fn lower_block(block: &ast::Block, types: &HashMap<(usize, usize), Type>) -> IrBlock {
    IrBlock {
        statements: block
            .statements
            .iter()
            .map(|stmt| lower_stmt(stmt, types))
            .collect(),
    }
}

fn lower_stmt(stmt: &ast::Stmt, types: &HashMap<(usize, usize), Type>) -> IrStmt {
    match stmt {
        ast::Stmt::Let {
            name,
            mutable,
            ty,
            value,
            span,
        } => IrStmt::Let {
            name: name.clone(),
            mutable: *mutable,
            ty: ty.clone().unwrap_or_else(|| expression_type(value, types)),
            value: lower_expr(value, types),
            span: *span,
        },
        ast::Stmt::Destructure {
            struct_name,
            mutable,
            bindings,
            value,
            span,
        } => IrStmt::Destructure {
            struct_name: struct_name.clone(),
            mutable: *mutable,
            bindings: bindings.clone(),
            value: lower_expr(value, types),
            span: *span,
        },
        ast::Stmt::Return(value, span) => {
            IrStmt::Return(value.as_ref().map(|e| lower_expr(e, types)), *span)
        }
        ast::Stmt::Stop(value, span) => {
            IrStmt::Stop(value.as_ref().map(|e| lower_expr(e, types)), *span)
        }
        ast::Stmt::Skip(span) => IrStmt::Skip(*span),
        ast::Stmt::If {
            condition,
            then_block,
            else_block,
            span,
        } => IrStmt::If {
            condition: lower_expr(condition, types),
            then_block: lower_block(then_block, types),
            else_block: else_block.as_ref().map(|block| lower_block(block, types)),
            span: *span,
        },
        ast::Stmt::Loop {
            kind,
            binding,
            body,
            span,
        } => IrStmt::Loop {
            kind: match kind {
                ast::LoopKind::Infinite => IrLoopKind::Infinite,
                ast::LoopKind::Repeat(expr) => IrLoopKind::Repeat(lower_expr(expr, types)),
                ast::LoopKind::While(expr) => IrLoopKind::While(lower_expr(expr, types)),
                ast::LoopKind::For { variable, iterable } => IrLoopKind::For {
                    variable: variable.clone(),
                    iterable: lower_expr(iterable, types),
                },
            },
            binding: binding.clone(),
            body: lower_block(body, types),
            span: *span,
        },
        ast::Stmt::Expr(expr, span) => IrStmt::Expr(lower_expr(expr, types), *span),
        ast::Stmt::Match {
            expression,
            arms,
            span,
        } => IrStmt::Match {
            expression: lower_expr(expression, types),
            arms: arms
                .iter()
                .map(|arm| IrMatchArm {
                    pattern: match &arm.pattern {
                        ast::MatchPattern::Number(value) => IrMatchPattern::Number(value.clone()),
                        ast::MatchPattern::String(value) => IrMatchPattern::String(value.clone()),
                        ast::MatchPattern::Bool(value) => IrMatchPattern::Bool(*value),
                        ast::MatchPattern::Identifier(value) => {
                            IrMatchPattern::Identifier(value.clone())
                        }
                        ast::MatchPattern::Wildcard => IrMatchPattern::Wildcard,
                    },
                    body: lower_block(&arm.body, types),
                })
                .collect(),
            span: *span,
        },
    }
}

fn lower_expr(expr: &ast::Expr, types: &HashMap<(usize, usize), Type>) -> IrExpr {
    let kind = match expr {
        ast::Expr::Number(value, _) => IrExprKind::Number(value.clone()),
        ast::Expr::String(value, _) => IrExprKind::String(value.clone()),
        ast::Expr::Interpolation(parts, _) => IrExprKind::Interpolation(parts.clone()),
        ast::Expr::Bool(value, _) => IrExprKind::Bool(*value),
        ast::Expr::Identifier(name, _) => IrExprKind::Identifier(name.clone()),
        ast::Expr::StructLiteral { name, fields, .. } => IrExprKind::StructLiteral {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, expr)| (name.clone(), lower_expr(expr, types)))
                .collect(),
        },
        ast::Expr::Binary {
            left,
            operator,
            right,
            ..
        } => IrExprKind::Binary {
            left: Box::new(lower_expr(left, types)),
            operator: *operator,
            right: Box::new(lower_expr(right, types)),
        },
        ast::Expr::Pipe { input, target, .. } => IrExprKind::Pipe {
            input: Box::new(lower_expr(input, types)),
            target: match target.as_ref() {
                ast::Expr::Call {
                    callee, arguments, ..
                } => IrPipeTarget::Call {
                    callee: lower_callee(callee, types),
                    arguments: arguments.iter().map(|arg| lower_expr(arg, types)).collect(),
                },
                ast::Expr::Identifier(..) | ast::Expr::Member { .. } => {
                    IrPipeTarget::Callee(lower_callee(target, types))
                }
                _ => IrPipeTarget::Value(Box::new(lower_expr(target, types))),
            },
        },
        ast::Expr::Unary {
            operator, operand, ..
        } => IrExprKind::Unary {
            operator: *operator,
            operand: Box::new(lower_expr(operand, types)),
        },
        ast::Expr::Call {
            callee, arguments, ..
        } => IrExprKind::Call {
            callee: lower_callee(callee, types),
            arguments: arguments.iter().map(|arg| lower_expr(arg, types)).collect(),
        },
        ast::Expr::Member { object, member, .. } => IrExprKind::Member {
            object: Box::new(lower_expr(object, types)),
            member: member.clone(),
        },
        ast::Expr::Shell { command, .. } => IrExprKind::Shell {
            command: command.clone(),
        },
        ast::Expr::Intrinsic {
            name, arguments, ..
        } => IrExprKind::Intrinsic {
            name: name.clone(),
            arguments: arguments.iter().map(|arg| lower_expr(arg, types)).collect(),
        },
    };
    let span = expr.span();
    IrExpr {
        kind,
        ty: types
            .get(&(span.start, span.end))
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "semantic analysis omitted type for IR expression {expr:?} at {}..{}",
                    span.start, span.end
                )
            }),
        span,
    }
}

fn lower_callee(expr: &ast::Expr, types: &HashMap<(usize, usize), Type>) -> IrCallee {
    match expr {
        ast::Expr::Identifier(name, _) => IrCallee::Identifier(name.clone()),
        ast::Expr::Member { object, member, .. } => IrCallee::Member {
            object: Box::new(lower_callee(object, types)),
            member: member.clone(),
        },
        _ => IrCallee::Value(Box::new(lower_expr(expr, types))),
    }
}

fn expression_type(expr: &ast::Expr, types: &HashMap<(usize, usize), Type>) -> Type {
    let span = expr.span();
    types
        .get(&(span.start, span.end))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "semantic analysis omitted type for IR expression {expr:?} at {}..{}",
                span.start, span.end
            )
        })
}
