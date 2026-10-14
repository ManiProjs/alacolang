//! Defensive checks for invariants expected after semantic analysis and lowering.

use std::collections::HashSet;

use crate::{ast::Type, error::AlacoError, ir::*};

pub fn validate(program: &IrProgram) -> Result<(), AlacoError> {
    let mut item_functions = HashSet::new();
    let mut item_structs = HashSet::new();
    for item in &program.items {
        match item {
            IrItem::Function(name) => {
                if !item_functions.insert(name.as_str()) || program.function(name).is_none() {
                    return Err(invariant(format!(
                        "invalid or duplicate function item `{name}`"
                    )));
                }
            }
            IrItem::Struct(name) => {
                if !item_structs.insert(name.as_str())
                    || !program.structs.iter().any(|item| item.name == *name)
                {
                    return Err(invariant(format!(
                        "invalid or duplicate struct item `{name}`"
                    )));
                }
            }
        }
    }
    if item_functions.len() != program.functions.len()
        || item_structs.len() != program.structs.len()
    {
        return Err(invariant(
            "IR item list does not cover every function and struct".into(),
        ));
    }

    let mut function_names = HashSet::new();
    for function in &program.functions {
        if !function_names.insert(function.name.as_str()) {
            return Err(invariant(format!(
                "duplicate function `{}` in IR",
                function.name
            )));
        }
        let mut params = HashSet::new();
        if function
            .parameters
            .iter()
            .any(|p| !params.insert(p.name.as_str()))
        {
            return Err(invariant(format!(
                "duplicate parameter in function `{}`",
                function.name
            )));
        }
        validate_block(&function.body, &function.return_type)?;
    }

    let mut struct_names = HashSet::new();
    for structure in &program.structs {
        if !struct_names.insert(structure.name.as_str()) {
            return Err(invariant(format!(
                "duplicate struct `{}` in IR",
                structure.name
            )));
        }
        let mut fields = HashSet::new();
        if structure
            .fields
            .iter()
            .any(|f| !fields.insert(f.name.as_str()))
        {
            return Err(invariant(format!(
                "duplicate field in struct `{}`",
                structure.name
            )));
        }
    }
    Ok(())
}

fn validate_block(block: &IrBlock, return_type: &Type) -> Result<(), AlacoError> {
    for statement in &block.statements {
        match statement {
            IrStmt::Let { ty, value, .. } => {
                validate_expr(value)?;
                if *ty != value.ty || value.ty == Type::Void {
                    return Err(invariant(
                        "let binding type does not match its value".into(),
                    ));
                }
            }
            IrStmt::Destructure { value, .. } => validate_expr(value)?,
            IrStmt::Return(value, _) => {
                if let Some(value) = value {
                    validate_expr(value)?;
                    if *return_type == Type::Void || value.ty != *return_type {
                        return Err(invariant(
                            "return value type does not match function return type".into(),
                        ));
                    }
                } else if *return_type != Type::Void {
                    return Err(invariant(
                        "non-void function contains an empty return".into(),
                    ));
                }
            }
            IrStmt::Stop(value, _) => {
                if let Some(value) = value {
                    validate_expr(value)?;
                }
            }
            IrStmt::Skip(_) => {}
            IrStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                validate_expr(condition)?;
                if condition.ty != Type::Bool {
                    return Err(invariant("if condition is not Bool".into()));
                }
                validate_block(then_block, return_type)?;
                if let Some(block) = else_block {
                    validate_block(block, return_type)?;
                }
            }
            IrStmt::Loop { kind, body, .. } => {
                match kind {
                    IrLoopKind::Infinite => {}
                    IrLoopKind::Repeat(expr) => {
                        validate_expr(expr)?;
                        if expr.ty != Type::Int {
                            return Err(invariant("repeat count is not Int".into()));
                        }
                    }
                    IrLoopKind::While(expr) => {
                        validate_expr(expr)?;
                        if expr.ty != Type::Bool {
                            return Err(invariant("while condition is not Bool".into()));
                        }
                    }
                    IrLoopKind::For { iterable, .. } => validate_expr(iterable)?,
                }
                validate_block(body, return_type)?;
            }
            IrStmt::Expr(expr, _) => validate_expr(expr)?,
            IrStmt::Match {
                expression, arms, ..
            } => {
                validate_expr(expression)?;
                for arm in arms {
                    validate_block(&arm.body, return_type)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_expr(expr: &IrExpr) -> Result<(), AlacoError> {
    match &expr.kind {
        IrExprKind::Number(_)
        | IrExprKind::String(_)
        | IrExprKind::Interpolation(_)
        | IrExprKind::Bool(_)
        | IrExprKind::Identifier(_)
        | IrExprKind::Shell { .. } => {}
        IrExprKind::StructLiteral { fields, .. } => {
            for (_, value) in fields {
                validate_expr(value)?;
            }
        }
        IrExprKind::Binary { left, right, .. } => {
            validate_expr(left)?;
            validate_expr(right)?;
        }
        IrExprKind::Pipe { input, target } => {
            validate_expr(input)?;
            match target {
                IrPipeTarget::Call { callee, arguments } => {
                    validate_callee(callee)?;
                    for arg in arguments {
                        validate_expr(arg)?;
                    }
                }
                IrPipeTarget::Callee(callee) => validate_callee(callee)?,
                IrPipeTarget::Value(value) => validate_expr(value)?,
            }
        }
        IrExprKind::Unary { operand, .. } => validate_expr(operand)?,
        IrExprKind::Call { callee, arguments } => {
            validate_callee(callee)?;
            for arg in arguments {
                validate_expr(arg)?;
            }
        }
        IrExprKind::Member { object, .. } => validate_expr(object)?,
        IrExprKind::Intrinsic { arguments, .. } => {
            for arg in arguments {
                validate_expr(arg)?;
            }
        }
    }
    Ok(())
}

fn validate_callee(callee: &IrCallee) -> Result<(), AlacoError> {
    match callee {
        IrCallee::Identifier(_) => Ok(()),
        IrCallee::Member { object, .. } => validate_callee(object),
        IrCallee::Value(expr) => validate_expr(expr),
    }
}

fn invariant(message: String) -> AlacoError {
    AlacoError::InternalIrInvariant { message }
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::{
        ast::{Span, Type},
        ir::*,
    };

    #[test]
    fn rejects_ir_items_that_reference_missing_functions() {
        let ir = IrProgram {
            items: vec![IrItem::Function("missing".into())],
            functions: vec![],
            structs: vec![],
        };
        assert!(validate(&ir).is_err());
    }

    #[test]
    fn accepts_a_well_formed_minimal_program() {
        let ir = IrProgram {
            items: vec![IrItem::Function("main".into())],
            functions: vec![IrFunction {
                name: "main".into(),
                parameters: vec![],
                return_type: Type::Void,
                body: IrBlock::default(),
                span: Span { start: 0, end: 1 },
            }],
            structs: vec![],
        };
        assert!(validate(&ir).is_ok());
    }
}
