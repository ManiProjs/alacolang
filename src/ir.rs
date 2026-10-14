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

/// Unique identity for an expression during one compilation.
/// Unlike source spans, IDs remain distinct for nested or synthetic nodes
/// that happen to cover the same source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

pub type ExpressionTypes = HashMap<ExprId, Type>;
pub type ExpressionIds = HashMap<usize, ExprId>;

/// Check lowering inputs before the infallible AST-to-IR conversion runs.
/// This turns missing semantic annotations into a compiler diagnostic rather
/// than allowing `lower_expr` to panic on an analyzer regression.
pub fn validate_expression_coverage(
    program: &ast::Program,
    types: &ExpressionTypes,
    ids: &ExpressionIds,
) -> Result<(), crate::error::AlacoError> {
    fn expr(
        e: &ast::Expr,
        types: &ExpressionTypes,
        ids: &ExpressionIds,
    ) -> Result<(), crate::error::AlacoError> {
        let key = e as *const ast::Expr as usize;
        if ids.get(&key).and_then(|id| types.get(id)).is_none() {
            return Err(crate::error::AlacoError::InternalIrInvariant {
                message: format!(
                    "semantic analysis omitted a type annotation at {}..{}",
                    e.span().start,
                    e.span().end
                ),
            });
        }
        match e {
            ast::Expr::StructLiteral { fields, .. } => {
                for (_, e) in fields {
                    expr(e, types, ids)?;
                }
            }
            ast::Expr::Binary { left, right, .. } => {
                expr(left, types, ids)?;
                expr(right, types, ids)?;
            }
            ast::Expr::Pipe { input, target, .. } => {
                expr(input, types, ids)?;
                callee(target, types, ids)?;
            }
            ast::Expr::Unary { operand, .. } => expr(operand, types, ids)?,
            ast::Expr::Call {
                callee, arguments, ..
            } => {
                callee_expr(callee, types, ids)?;
                for arg in arguments {
                    expr(arg, types, ids)?;
                }
            }
            ast::Expr::Member { object, .. } => expr(object, types, ids)?,
            ast::Expr::Intrinsic { arguments, .. } => {
                for arg in arguments {
                    expr(arg, types, ids)?;
                }
            }
            ast::Expr::Number(..)
            | ast::Expr::String(..)
            | ast::Expr::Interpolation(..)
            | ast::Expr::Bool(..)
            | ast::Expr::Identifier(..)
            | ast::Expr::Previous(..)
            | ast::Expr::Shell { .. } => {}
        }
        Ok(())
    }
    fn callee_expr(
        e: &ast::Expr,
        types: &ExpressionTypes,
        ids: &ExpressionIds,
    ) -> Result<(), crate::error::AlacoError> {
        match e {
            ast::Expr::Identifier(..) => Ok(()),
            ast::Expr::Member { object, .. } => callee_expr(object, types, ids),
            _ => expr(e, types, ids),
        }
    }
    fn callee(
        e: &ast::Expr,
        types: &ExpressionTypes,
        ids: &ExpressionIds,
    ) -> Result<(), crate::error::AlacoError> {
        match e {
            ast::Expr::Identifier(..) | ast::Expr::Member { .. } => callee_expr(e, types, ids),
            ast::Expr::Call {
                callee, arguments, ..
            } => {
                callee_expr(callee, types, ids)?;
                for arg in arguments {
                    expr(arg, types, ids)?;
                }
                Ok(())
            }
            _ => expr(e, types, ids),
        }
    }
    fn block(
        b: &ast::Block,
        types: &ExpressionTypes,
        ids: &ExpressionIds,
    ) -> Result<(), crate::error::AlacoError> {
        for s in &b.statements {
            match s {
                ast::Stmt::Let { value, .. }
                | ast::Stmt::Destructure { value, .. }
                | ast::Stmt::Expr(value, _) => expr(value, types, ids)?,
                ast::Stmt::Return(value, _) | ast::Stmt::Stop(value, _) => {
                    if let Some(value) = value {
                        expr(value, types, ids)?;
                    }
                }
                ast::Stmt::Skip(_) => {}
                ast::Stmt::If {
                    condition,
                    then_block,
                    else_block,
                    ..
                } => {
                    expr(condition, types, ids)?;
                    block(then_block, types, ids)?;
                    if let Some(b) = else_block {
                        block(b, types, ids)?;
                    }
                }
                ast::Stmt::Loop { kind, body, .. } => {
                    match kind {
                        ast::LoopKind::Infinite => {}
                        ast::LoopKind::Repeat(e) | ast::LoopKind::While(e) => expr(e, types, ids)?,
                        ast::LoopKind::For { iterable, .. } => expr(iterable, types, ids)?,
                    };
                    block(body, types, ids)?;
                }
                ast::Stmt::Match {
                    expression, arms, ..
                } => {
                    expr(expression, types, ids)?;
                    for arm in arms {
                        block(&arm.body, types, ids)?;
                    }
                }
                ast::Stmt::When {
                    condition, body, ..
                } => {
                    expr(condition, types, ids)?;
                    block(body, types, ids)?;
                }
            }
        }
        Ok(())
    }
    for item in &program.items {
        if let ast::Item::Function(function) = item {
            block(&function.body, types, ids)?;
        }
    }
    Ok(())
}

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
#[allow(dead_code)] // Retained for diagnostics and future IR consumers/backends.
pub struct IrFunction {
    pub name: String,
    pub parameters: Vec<IrParameter>,
    pub return_type: Type,
    pub body: IrBlock,
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Parameter spans are useful for precise downstream diagnostics.
pub struct IrParameter {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Struct and field spans are retained for source-aware IR passes.
pub struct IrStruct {
    pub name: String,
    pub fields: Vec<IrStructField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // See IrStruct: field metadata remains part of the typed IR contract.
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
#[allow(dead_code)] // The current C++ backend does not consume all semantic/source metadata.
pub enum IrStmt {
    Let {
        name: String,
        mutable: bool,
        ty: Type,
        lifetime_ms: Option<u64>,
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
#[allow(dead_code)] // Source spans are preserved for future editor diagnostics and IR tooling.
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
    Previous(String),
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
        expression_types: &ExpressionTypes,
        expression_ids: &ExpressionIds,
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
                    body: lower_block(&function.body, expression_types, expression_ids),
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

fn lower_block(block: &ast::Block, types: &ExpressionTypes, ids: &ExpressionIds) -> IrBlock {
    IrBlock {
        statements: block
            .statements
            .iter()
            .map(|stmt| lower_stmt(stmt, types, ids))
            .collect(),
    }
}

fn lower_stmt(stmt: &ast::Stmt, types: &ExpressionTypes, ids: &ExpressionIds) -> IrStmt {
    match stmt {
        ast::Stmt::Let {
            name,
            mutable,
            ty,
            lifetime_ms,
            value,
            span,
        } => IrStmt::Let {
            name: name.clone(),
            mutable: *mutable,
            lifetime_ms: *lifetime_ms,
            ty: ty
                .clone()
                .unwrap_or_else(|| expression_type(value, types, ids)),
            value: lower_expr(value, types, ids),
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
            value: lower_expr(value, types, ids),
            span: *span,
        },
        ast::Stmt::Return(value, span) => {
            IrStmt::Return(value.as_ref().map(|e| lower_expr(e, types, ids)), *span)
        }
        ast::Stmt::Stop(value, span) => {
            IrStmt::Stop(value.as_ref().map(|e| lower_expr(e, types, ids)), *span)
        }
        ast::Stmt::Skip(span) => IrStmt::Skip(*span),
        ast::Stmt::If {
            condition,
            then_block,
            else_block,
            span,
        } => IrStmt::If {
            condition: lower_expr(condition, types, ids),
            then_block: lower_block(then_block, types, ids),
            else_block: else_block
                .as_ref()
                .map(|block| lower_block(block, types, ids)),
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
                ast::LoopKind::Repeat(expr) => IrLoopKind::Repeat(lower_expr(expr, types, ids)),
                ast::LoopKind::While(expr) => IrLoopKind::While(lower_expr(expr, types, ids)),
                ast::LoopKind::For { variable, iterable } => IrLoopKind::For {
                    variable: variable.clone(),
                    iterable: lower_expr(iterable, types, ids),
                },
            },
            binding: binding.clone(),
            body: lower_block(body, types, ids),
            span: *span,
        },
        ast::Stmt::Expr(expr, span) => IrStmt::Expr(lower_expr(expr, types, ids), *span),
        ast::Stmt::Match {
            expression,
            arms,
            span,
        } => IrStmt::Match {
            expression: lower_expr(expression, types, ids),
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
                    body: lower_block(&arm.body, types, ids),
                })
                .collect(),
            span: *span,
        },
        ast::Stmt::When {
            condition,
            body,
            span,
        } => IrStmt::If {
            condition: lower_expr(condition, types, ids),
            then_block: lower_block(body, types, ids),
            else_block: None,
            span: *span,
        },
    }
}

fn lower_expr(expr: &ast::Expr, types: &ExpressionTypes, ids: &ExpressionIds) -> IrExpr {
    let kind = match expr {
        ast::Expr::Number(value, _) => IrExprKind::Number(value.clone()),
        ast::Expr::String(value, _) => IrExprKind::String(value.clone()),
        ast::Expr::Interpolation(parts, _) => IrExprKind::Interpolation(parts.clone()),
        ast::Expr::Bool(value, _) => IrExprKind::Bool(*value),
        ast::Expr::Identifier(name, _) => IrExprKind::Identifier(name.clone()),
        ast::Expr::Previous(name, _) => IrExprKind::Previous(name.clone()),
        ast::Expr::StructLiteral { name, fields, .. } => IrExprKind::StructLiteral {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, expr)| (name.clone(), lower_expr(expr, types, ids)))
                .collect(),
        },
        ast::Expr::Binary {
            left,
            operator,
            right,
            ..
        } => IrExprKind::Binary {
            left: Box::new(lower_expr(left, types, ids)),
            operator: *operator,
            right: Box::new(lower_expr(right, types, ids)),
        },
        ast::Expr::Pipe { input, target, .. } => IrExprKind::Pipe {
            input: Box::new(lower_expr(input, types, ids)),
            target: match target.as_ref() {
                ast::Expr::Call {
                    callee, arguments, ..
                } => IrPipeTarget::Call {
                    callee: lower_callee(callee, types, ids),
                    arguments: arguments
                        .iter()
                        .map(|arg| lower_expr(arg, types, ids))
                        .collect(),
                },
                ast::Expr::Identifier(..) | ast::Expr::Member { .. } => {
                    IrPipeTarget::Callee(lower_callee(target, types, ids))
                }
                _ => IrPipeTarget::Value(Box::new(lower_expr(target, types, ids))),
            },
        },
        ast::Expr::Unary {
            operator, operand, ..
        } => IrExprKind::Unary {
            operator: *operator,
            operand: Box::new(lower_expr(operand, types, ids)),
        },
        ast::Expr::Call {
            callee, arguments, ..
        } => IrExprKind::Call {
            callee: lower_callee(callee, types, ids),
            arguments: arguments
                .iter()
                .map(|arg| lower_expr(arg, types, ids))
                .collect(),
        },
        ast::Expr::Member { object, member, .. } => IrExprKind::Member {
            object: Box::new(lower_expr(object, types, ids)),
            member: member.clone(),
        },
        ast::Expr::Shell { command, .. } => IrExprKind::Shell {
            command: command.clone(),
        },
        ast::Expr::Intrinsic {
            name, arguments, ..
        } => IrExprKind::Intrinsic {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|arg| lower_expr(arg, types, ids))
                .collect(),
        },
    };
    let span = expr.span();
    let id = ids.get(&(expr as *const ast::Expr as usize)).copied();
    IrExpr {
        kind,
        ty: id
            .and_then(|id| types.get(&id))
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

fn lower_callee(expr: &ast::Expr, types: &ExpressionTypes, ids: &ExpressionIds) -> IrCallee {
    match expr {
        ast::Expr::Identifier(name, _) => IrCallee::Identifier(name.clone()),
        ast::Expr::Member { object, member, .. } => IrCallee::Member {
            object: Box::new(lower_callee(object, types, ids)),
            member: member.clone(),
        },
        _ => IrCallee::Value(Box::new(lower_expr(expr, types, ids))),
    }
}

fn expression_type(expr: &ast::Expr, types: &ExpressionTypes, ids: &ExpressionIds) -> Type {
    let span = expr.span();
    ids.get(&(expr as *const ast::Expr as usize))
        .and_then(|id| types.get(id))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "semantic analysis omitted type for IR expression {expr:?} at {}..{}",
                span.start, span.end
            )
        })
}
