use std::collections::{HashMap, HashSet};

use crate::{
    ast::{Block, Expr, Function, Item, LoopKind, Program, Stmt},
    error::AlacoError,
    language::BinaryOp,
    stdlib::LoadedModule,
};

#[derive(Debug, Clone)]
struct Variable {
    mutable: bool,
}

#[derive(Debug, Clone)]
pub struct Analyzer<'a> {
    modules: &'a [LoadedModule],
}

impl<'a> Analyzer<'a> {
    pub fn new(modules: &'a [LoadedModule]) -> Self {
        Self { modules }
    }

    pub fn analyze(&self, program: &Program) -> Result<(), AlacoError> {
        self.check_functions(program)?;

        for item in &program.items {
            match item {
                Item::Import(_) => {}

                Item::Function(function) => {
                    self.analyze_function(function)?;
                }

                Item::Statement(_) => {
                    return Err(self.error("top-level statements are not allowed"));
                }
            }
        }

        Ok(())
    }

    fn find_module(&self, alias: &str) -> Option<&LoadedModule> {
        self.modules.iter().find(|module| module.alias == alias)
    }

    fn module_has_function(module: &LoadedModule, name: &str) -> bool {
        module.program.items.iter().any(|item| {
            matches!(
                item,
                Item::Function(function) if function.name == name
            )
        })
    }

    fn check_functions(&self, program: &Program) -> Result<(), AlacoError> {
        let mut functions = HashSet::new();
        let mut main_count = 0;

        for item in &program.items {
            let function = match item {
                Item::Function(function) => function,

                Item::Import(_) => continue,

                Item::Statement(_) => {
                    return Err(
                        self.error("top-level statements are not allowed; expected a function")
                    );
                }
            };

            if !functions.insert(function.name.as_str()) {
                return Err(self.error(format!("duplicate function '{}'", function.name)));
            }

            if function.name == "main" {
                main_count += 1;

                if !function.params.is_empty() {
                    return Err(self.error("main function cannot have parameters"));
                }

                if function.return_type.is_some() {
                    return Err(self.error("main function cannot have an explicit return type"));
                }
            }

            self.check_parameters(function)?;
        }

        if main_count == 0 {
            return Err(self.error("program must contain a 'main' function"));
        }

        if main_count > 1 {
            return Err(self.error("program can only contain one 'main' function"));
        }

        Ok(())
    }

    fn check_parameters(&self, function: &Function) -> Result<(), AlacoError> {
        let mut names = HashSet::new();

        for parameter in &function.params {
            if !names.insert(parameter.name.as_str()) {
                return Err(self.error(format!(
                    "duplicate parameter '{}' in function '{}'",
                    parameter.name, function.name
                )));
            }
        }

        Ok(())
    }

    fn analyze_function(&self, function: &Function) -> Result<(), AlacoError> {
        let mut scopes = Vec::new();

        scopes.push(HashMap::new());

        for parameter in &function.params {
            scopes
                .last_mut()
                .expect("analyzer scope stack is empty")
                .insert(parameter.name.clone(), Variable { mutable: true });
        }

        self.analyze_block(&function.body, &mut scopes, 0)
    }

    fn analyze_block(
        &self,
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
    ) -> Result<(), AlacoError> {
        scopes.push(HashMap::new());

        for statement in &block.statements {
            self.analyze_statement(statement, scopes, loop_depth)?;
        }

        scopes.pop();

        Ok(())
    }

    fn analyze_statement(
        &self,
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
                self.analyze_expression(value, scopes)?;

                let scope = scopes.last_mut().expect("analyzer scope stack is empty");

                if scope.contains_key(name) {
                    return Err(self.error(format!(
                        "variable '{}' is already declared in this scope",
                        name
                    )));
                }

                scope.insert(name.clone(), Variable { mutable: *mutable });
            }

            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.analyze_expression(value, scopes)?;
                }
            }

            Stmt::Stop(value) => {
                if loop_depth == 0 {
                    return Err(self.error("'stop' can only be used inside a loop"));
                }

                if let Some(value) = value {
                    self.analyze_expression(value, scopes)?;
                }
            }

            Stmt::Skip => {
                if loop_depth == 0 {
                    return Err(self.error("'skip' can only be used inside a loop"));
                }
            }

            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.analyze_expression(condition, scopes)?;

                self.analyze_block(then_block, scopes, loop_depth)?;

                if let Some(else_block) = else_block {
                    self.analyze_block(else_block, scopes, loop_depth)?;
                }
            }

            Stmt::Loop {
                kind,
                binding,
                body,
            } => {
                self.analyze_loop(kind, binding.as_deref(), body, scopes, loop_depth)?;
            }

            Stmt::Expr(expression) => {
                self.analyze_expression(expression, scopes)?;
            }

            Stmt::Match { expression, arms } => {
                self.analyze_expression(expression, scopes)?;

                for arm in arms {
                    self.analyze_block(&arm.body, scopes, loop_depth)?;
                }
            }
        }

        Ok(())
    }

    fn analyze_loop(
        &self,
        kind: &LoopKind,
        binding: Option<&str>,
        body: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
    ) -> Result<(), AlacoError> {
        match kind {
            LoopKind::Infinite => {}

            LoopKind::Repeat(expression) => {
                self.analyze_expression(expression, scopes)?;
            }

            LoopKind::While(condition) => {
                self.analyze_expression(condition, scopes)?;
            }

            LoopKind::For { variable, iterable } => {
                self.analyze_expression(iterable, scopes)?;

                if binding.is_some() {
                    return Err(self.error(
                        "a 'for' loop cannot currently have an additional iteration binding",
                    ));
                }

                let mut loop_scopes = scopes.clone();

                loop_scopes.push(HashMap::new());

                loop_scopes
                    .last_mut()
                    .expect("analyzer scope stack is empty")
                    .insert(variable.clone(), Variable { mutable: false });

                self.analyze_block_with_existing_scope(body, &mut loop_scopes, loop_depth + 1)?;

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

        self.analyze_block_with_existing_scope(body, &mut loop_scopes, loop_depth + 1)
    }

    fn analyze_block_with_existing_scope(
        &self,
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
    ) -> Result<(), AlacoError> {
        for statement in &block.statements {
            self.analyze_statement(statement, scopes, loop_depth)?;
        }

        Ok(())
    }

    fn analyze_expression(
        &self,
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
    ) -> Result<(), AlacoError> {
        match expression {
            Expr::Number(_) | Expr::String(_) | Expr::Bool(_) => {}

            Expr::Identifier(name) => {
                if !self.is_builtin(name) && self.find_variable(scopes, name).is_none() {
                    return Err(self.error(format!("use of undefined variable '{}'", name)));
                }
            }

            Expr::Unary { operand, .. } => {
                self.analyze_expression(operand, scopes)?;
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                self.analyze_expression(left, scopes)?;
                self.analyze_expression(right, scopes)?;

                if matches!(
                    operator,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubtractAssign
                        | BinaryOp::MultiplyAssign
                        | BinaryOp::DivideAssign
                ) {
                    self.check_assignment_target(left, scopes)?;
                }
            }

            Expr::Call { callee, arguments } => {
                self.analyze_call(callee, arguments, scopes)?;
            }

            Expr::Member { object, .. } => {
                self.analyze_expression(object, scopes)?;
            }
        }

        Ok(())
    }

    fn analyze_call(
        &self,
        callee: &Expr,
        arguments: &[Expr],
        scopes: &mut Vec<HashMap<String, Variable>>,
    ) -> Result<(), AlacoError> {
        // std.math.max(...)
        //
        // The `math` identifier is a module alias, not a variable.
        if let Expr::Member { object, member } = callee {
            if let Expr::Identifier(alias) = object.as_ref() {
                if let Some(module) = self.find_module(alias) {
                    if !Self::module_has_function(module, member) {
                        return Err(
                            self.error(format!("module '{}' has no function '{}'", alias, member))
                        );
                    }

                    for argument in arguments {
                        self.analyze_expression(argument, scopes)?;
                    }

                    return Ok(());
                }
            }
        }

        // Builtin calls such as print(...)
        if let Expr::Identifier(name) = callee {
            if self.is_builtin(name) {
                for argument in arguments {
                    self.analyze_expression(argument, scopes)?;
                }

                return Ok(());
            }
        }

        self.analyze_expression(callee, scopes)?;

        for argument in arguments {
            self.analyze_expression(argument, scopes)?;
        }

        Ok(())
    }

    fn is_builtin(&self, name: &str) -> bool {
        matches!(name, "print")
    }

    fn check_assignment_target(
        &self,
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
    ) -> Result<(), AlacoError> {
        match expression {
            Expr::Identifier(name) => {
                let Some(variable) = self.find_variable(scopes, name) else {
                    return Err(
                        self.error(format!("cannot assign to undefined variable '{}'", name))
                    );
                };

                if !variable.mutable {
                    return Err(
                        self.error(format!("cannot assign to immutable variable '{}'", name))
                    );
                }
            }

            Expr::Member { object, .. } => {
                self.analyze_expression(object, scopes)?;
            }

            _ => {
                return Err(self.error("invalid assignment target"));
            }
        }

        Ok(())
    }

    fn find_variable<'b>(
        &self,
        scopes: &'b [HashMap<String, Variable>],
        name: &str,
    ) -> Option<&'b Variable> {
        scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn error(&self, message: impl Into<String>) -> AlacoError {
        AlacoError::Analysis {
            message: message.into(),
            span: None,
        }
    }
}
