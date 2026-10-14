//! Conservative optimization passes over typed IR.

use crate::{
    ast::{Type, UnaryOp},
    ir::{IrBlock, IrExpr, IrExprKind, IrLoopKind, IrProgram, IrStmt},
    language::BinaryOp,
};

/// Run optimization passes that preserve source spans and inferred types.
pub fn optimize(program: &mut IrProgram) {
    for function in &mut program.functions {
        optimize_block(&mut function.body);
    }
}

fn optimize_block(block: &mut IrBlock) {
    for statement in &mut block.statements {
        match statement {
            IrStmt::Let { value, .. } | IrStmt::Destructure { value, .. } => optimize_expr(value),
            IrStmt::Return(value, _) | IrStmt::Stop(value, _) => {
                if let Some(value) = value {
                    optimize_expr(value);
                }
            }
            IrStmt::Skip(_) => {}
            IrStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                optimize_expr(condition);
                optimize_block(then_block);
                if let Some(block) = else_block {
                    optimize_block(block);
                }
            }
            IrStmt::Loop { kind, body, .. } => {
                match kind {
                    IrLoopKind::Infinite => {}
                    IrLoopKind::Repeat(expr) | IrLoopKind::While(expr) => optimize_expr(expr),
                    IrLoopKind::For { iterable, .. } => optimize_expr(iterable),
                }
                optimize_block(body);
            }
            IrStmt::Expr(expr, _) => optimize_expr(expr),
            IrStmt::Match {
                expression, arms, ..
            } => {
                optimize_expr(expression);
                for arm in arms {
                    optimize_block(&mut arm.body);
                }
            }
        }
    }
}

fn optimize_expr(expr: &mut IrExpr) {
    match &mut expr.kind {
        IrExprKind::StructLiteral { fields, .. } => {
            for (_, value) in fields {
                optimize_expr(value);
            }
        }
        IrExprKind::Binary { left, right, .. } => {
            optimize_expr(left);
            optimize_expr(right);
        }
        IrExprKind::Pipe { input, target } => {
            optimize_expr(input);
            match target {
                crate::ir::IrPipeTarget::Call { arguments, .. } => {
                    for argument in arguments {
                        optimize_expr(argument);
                    }
                }
                crate::ir::IrPipeTarget::Callee(_) => {}
                crate::ir::IrPipeTarget::Value(value) => optimize_expr(value),
            }
        }
        IrExprKind::Unary { operand, .. } => optimize_expr(operand),
        IrExprKind::Call { arguments, .. } | IrExprKind::Intrinsic { arguments, .. } => {
            for argument in arguments {
                optimize_expr(argument);
            }
        }
        IrExprKind::Member { object, .. } => optimize_expr(object),
        IrExprKind::Number(_)
        | IrExprKind::String(_)
        | IrExprKind::Interpolation(_)
        | IrExprKind::Bool(_)
        | IrExprKind::Identifier(_)
        | IrExprKind::Shell { .. } => {}
    }

    let replacement = match &expr.kind {
        IrExprKind::Unary {
            operator: UnaryOp::Negate,
            operand,
        } if expr.ty == Type::Int => match &operand.kind {
            IrExprKind::Number(value) => value
                .parse::<i64>()
                .ok()
                .and_then(i64::checked_neg)
                .map(|value| IrExprKind::Number(value.to_string())),
            _ => None,
        },
        IrExprKind::Unary {
            operator: UnaryOp::Not,
            operand,
        } => match operand.kind {
            IrExprKind::Bool(value) => Some(IrExprKind::Bool(!value)),
            _ => None,
        },
        IrExprKind::Binary {
            left,
            operator,
            right,
        } if expr.ty == Type::Int && left.ty == Type::Int && right.ty == Type::Int => {
            let (IrExprKind::Number(left), IrExprKind::Number(right)) = (&left.kind, &right.kind)
            else {
                return;
            };
            let (Ok(left), Ok(right)) = (left.parse::<i64>(), right.parse::<i64>()) else {
                return;
            };
            let folded = match operator {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Subtract => left.checked_sub(right),
                BinaryOp::Multiply => left.checked_mul(right),
                BinaryOp::Divide => left.checked_div(right),
                BinaryOp::Modulo => left.checked_rem(right),
                _ => None,
            };
            folded.map(|value| IrExprKind::Number(value.to_string()))
        }
        _ => None,
    };

    if let Some(kind) = replacement {
        expr.kind = kind;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Span;

    fn int(value: &str) -> IrExpr {
        IrExpr {
            kind: IrExprKind::Number(value.into()),
            ty: Type::Int,
            span: Span { start: 0, end: 1 },
        }
    }

    #[test]
    fn folds_integer_arithmetic_and_keeps_the_outer_span() {
        let span = Span { start: 10, end: 15 };
        let mut expression = IrExpr {
            kind: IrExprKind::Binary {
                left: Box::new(int("6")),
                operator: BinaryOp::Multiply,
                right: Box::new(int("7")),
            },
            ty: Type::Int,
            span,
        };

        optimize_expr(&mut expression);

        assert!(matches!(expression.kind, IrExprKind::Number(ref value) if value == "42"));
        assert_eq!(expression.span.start, span.start);
        assert_eq!(expression.span.end, span.end);
    }

    #[test]
    fn leaves_division_by_zero_and_overflow_untouched() {
        for (left, operator, right) in [
            ("8", BinaryOp::Divide, "0"),
            ("9223372036854775807", BinaryOp::Add, "1"),
        ] {
            let mut expression = IrExpr {
                kind: IrExprKind::Binary {
                    left: Box::new(int(left)),
                    operator,
                    right: Box::new(int(right)),
                },
                ty: Type::Int,
                span: Span { start: 0, end: 1 },
            };
            optimize_expr(&mut expression);
            assert!(matches!(expression.kind, IrExprKind::Binary { .. }));
        }
    }
}
