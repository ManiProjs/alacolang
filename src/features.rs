//! Source-level lowering for language features that expand into ordinary AST.

use crate::ast::{Block, Expr, Item, Program, Span, Stmt};

/// Expand `when` into condition checks after each statement in its lexical
/// block. The generated AST is then analyzed and lowered like ordinary code.
pub fn expand_when(program: &mut Program) {
    for item in &mut program.items {
        if let Item::Function(function) = item {
            expand_block(&mut function.body);
        }
    }
}

fn expand_block(block: &mut Block) {
    for statement in &mut block.statements {
        match statement {
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                expand_block(then_block);
                if let Some(else_block) = else_block {
                    expand_block(else_block);
                }
            }
            Stmt::Loop { body, .. } => expand_block(body),
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    expand_block(&mut arm.body);
                }
            }
            Stmt::When { body, .. } => expand_block(body),
            _ => {}
        }
    }

    let source = std::mem::take(&mut block.statements);
    let mut output = Vec::new();
    let mut watchers: Vec<(Expr, Block, Span)> = Vec::new();

    for statement in source {
        if let Stmt::When {
            condition,
            body,
            span,
        } = statement
        {
            watchers.push((condition, body, span));
            append_checks(&mut output, &watchers);
        } else {
            output.push(statement);
            append_checks(&mut output, &watchers);
        }
    }
    block.statements = output;
}

fn append_checks(output: &mut Vec<Stmt>, watchers: &[(Expr, Block, Span)]) {
    for (condition, body, span) in watchers {
        output.push(Stmt::If {
            condition: condition.clone(),
            then_block: body.clone(),
            else_block: None,
            span: *span,
        });
    }
}
