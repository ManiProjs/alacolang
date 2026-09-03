use std::collections::{HashMap, HashSet};

use crate::{
    ast::{Block, Expr, Function, Item, LoopKind, Program, Stmt},
    error::AlacoError,
    language::BinaryOp,
};

#[derive(Debug, Clone)]
struct Variable {
    mutable: bool,
}

pub struct Analyzer;

impl Analyzer {
    pub fn analyze(program: &Program) -> Result<(), AlacoError> {
        Self::check_functions(program)?;

        for item in &program.items {
            match item {
                Item::Function(function) => {
                    Self::analyze_function(function)?;
                }

                Item::Statement(_) => {
                    return Err(Self::error("top-level statements are not allowed"));
                }
            }
        }

        Ok(())
    }

    fn check_functions(program: &Program) -> Result<(), AlacoError> {
        let mut functions = HashSet::new();
        let mut main_count = 0;

        for item in &program.items {
            let function = match item {
                Item::Function(function) => function,

                Item::Statement(_) => {
                    return Err(Self::error(
                        "top-level statements are not allowed; expected a function",
                    ));
                }
            };

            if !functions.insert(function.name.as_str()) {
                return Err(Self::error(format!(
                    "duplicate function '{}'",
                    function.name
                )));
            }

            if function.name == "main" {
                main_count += 1;

                if !function.params.is_empty() {
                    return Err(Self::error("main function cannot have parameters"));
                }

                if function.return_type.is_some() {
                    return Err(Self::error(
                        "main function cannot have an explicit return type",
                    ));
                }
            }

            Self::check_parameters(function)?;
        }

        if main_count == 0 {
            return Err(Self::error("program must contain a 'main' function"));
        }

        if main_count > 1 {
            return Err(Self::error("program can only contain one 'main' function"));
        }

        Ok(())
    }

    fn check_parameters(function: &Function) -> Result<(), AlacoError> {
        let mut names = HashSet::new();

        for parameter in &function.params {
            if !names.insert(parameter.name.as_str()) {
                return Err(Self::error(format!(
                    "duplicate parameter '{}' in function '{}'",
                    parameter.name, function.name
                )));
            }
        }

        Ok(())
    }

    fn analyze_function(function: &Function) -> Result<(), AlacoError> {
        let mut scopes = Vec::new();

        scopes.push(HashMap::new());

        for parameter in &function.params {
            scopes
                .last_mut()
                .expect("analyzer scope stack is empty")
                .insert(parameter.name.clone(), Variable { mutable: true });
        }

        Self::analyze_block(&function.body, &mut scopes, 0)
    }

    fn analyze_block(
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
    ) -> Result<(), AlacoError> {
        scopes.push(HashMap::new());

        for statement in &block.statements {
            Self::analyze_statement(statement, scopes, loop_depth)?;
        }

        scopes.pop();

        Ok(())
    }

    fn analyze_statement(
        statement: &Stmt,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
    ) -> Result<(), AlacoError> {
        match statement {
            Stmt::Let {
                name,
                mutable,
                value,
                ..
            } => {
                Self::analyze_expression(value, scopes)?;

                let scope = scopes.last_mut().expect("analyzer scope stack is empty");

                if scope.contains_key(name) {
                    return Err(Self::error(format!(
                        "variable '{}' is already declared in this scope",
                        name
                    )));
                }

                scope.insert(name.clone(), Variable { mutable: *mutable });
            }

            Stmt::Return(value) => {
                if let Some(value) = value {
                    Self::analyze_expression(value, scopes)?;
                }
            }

            Stmt::Stop(value) => {
                if loop_depth == 0 {
                    return Err(Self::error("'stop' can only be used inside a loop"));
                }

                if let Some(value) = value {
                    Self::analyze_expression(value, scopes)?;
                }
            }

            Stmt::Skip => {
                if loop_depth == 0 {
                    return Err(Self::error("'skip' can only be used inside a loop"));
                }
            }

            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                Self::analyze_expression(condition, scopes)?;

                Self::analyze_block(then_block, scopes, loop_depth)?;

                if let Some(else_block) = else_block {
                    Self::analyze_block(else_block, scopes, loop_depth)?;
                }
            }

            Stmt::Loop {
                kind,
                binding,
                body,
            } => {
                Self::analyze_loop(kind, binding.as_deref(), body, scopes, loop_depth)?;
            }

            Stmt::Expr(expression) => {
                Self::analyze_expression(expression, scopes)?;
            }
        }

        Ok(())
    }

    fn analyze_loop(
        kind: &LoopKind,
        binding: Option<&str>,
        body: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
    ) -> Result<(), AlacoError> {
        match kind {
            LoopKind::Infinite => {}

            LoopKind::Repeat(expression) => {
                Self::analyze_expression(expression, scopes)?;
            }

            LoopKind::While(condition) => {
                Self::analyze_expression(condition, scopes)?;
            }

            LoopKind::For { variable, iterable } => {
                Self::analyze_expression(iterable, scopes)?;

                if binding.is_some() {
                    return Err(Self::error(
                        "a 'for' loop cannot currently have an additional iteration binding",
                    ));
                }

                let mut loop_scopes = scopes.clone();

                loop_scopes.push(HashMap::new());

                loop_scopes
                    .last_mut()
                    .expect("analyzer scope stack is empty")
                    .insert(variable.clone(), Variable { mutable: false });

                Self::analyze_block_with_existing_scope(body, &mut loop_scopes, loop_depth + 1)?;

                return Ok(());
            }
        }

        let mut loop_scopes = scopes.clone();

        loop_scopes.push(HashMap::new());

        if let Some(binding) = binding {
            loop_scopes
                .last_mut()
                .expect("analyzer scope stack is empty")
                .insert(binding.to_string(), Variable { mutable: false });
        }

        Self::analyze_block_with_existing_scope(body, &mut loop_scopes, loop_depth + 1)
    }

    fn analyze_block_with_existing_scope(
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
    ) -> Result<(), AlacoError> {
        for statement in &block.statements {
            Self::analyze_statement(statement, scopes, loop_depth)?;
        }

        Ok(())
    }

    fn analyze_expression(
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
    ) -> Result<(), AlacoError> {
        match expression {
            Expr::Number(_) | Expr::String(_) | Expr::Bool(_) => {}

            Expr::Identifier(name) => {
                if !Self::is_builtin(name) && Self::find_variable(scopes, name).is_none() {
                    return Err(Self::error(format!("use of undefined variable '{}'", name)));
                }
            }

            Expr::Unary { operand, .. } => {
                Self::analyze_expression(operand, scopes)?;
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                Self::analyze_expression(left, scopes)?;
                Self::analyze_expression(right, scopes)?;

                if matches!(
                    operator,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubtractAssign
                        | BinaryOp::MultiplyAssign
                        | BinaryOp::DivideAssign
                ) {
                    Self::check_assignment_target(left, scopes)?;
                }
            }

            Expr::Call { callee, arguments } => {
                Self::analyze_call(callee, arguments, scopes)?;
            }

            Expr::Member { object, .. } => {
                Self::analyze_expression(object, scopes)?;
            }
        }

        Ok(())
    }

    fn analyze_call(
        callee: &Expr,
        arguments: &[Expr],
        scopes: &mut Vec<HashMap<String, Variable>>,
    ) -> Result<(), AlacoError> {
        if let Expr::Identifier(name) = callee {
            if Self::is_builtin(name) {
                for argument in arguments {
                    Self::analyze_expression(argument, scopes)?;
                }

                return Ok(());
            }
        }

        Self::analyze_expression(callee, scopes)?;

        for argument in arguments {
            Self::analyze_expression(argument, scopes)?;
        }

        Ok(())
    }

    fn is_builtin(name: &str) -> bool {
        matches!(name, "print")
    }

    fn check_assignment_target(
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
    ) -> Result<(), AlacoError> {
        match expression {
            Expr::Identifier(name) => {
                let Some(variable) = Self::find_variable(scopes, name) else {
                    return Err(Self::error(format!(
                        "cannot assign to undefined variable '{}'",
                        name
                    )));
                };

                if !variable.mutable {
                    return Err(Self::error(format!(
                        "cannot assign to immutable variable '{}'",
                        name
                    )));
                }
            }

            Expr::Member { object, .. } => {
                Self::analyze_expression(object, scopes)?;
            }

            _ => {
                return Err(Self::error("invalid assignment target"));
            }
        }

        Ok(())
    }

    fn find_variable<'a>(
        scopes: &'a [HashMap<String, Variable>],
        name: &str,
    ) -> Option<&'a Variable> {
        scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn error(message: impl Into<String>) -> AlacoError {
        AlacoError::Analysis {
            message: message.into(),
            span: None,
        }
    }
}
