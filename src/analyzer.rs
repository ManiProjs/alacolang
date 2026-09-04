use std::collections::{HashMap, HashSet};

use crate::{
    ast::{Block, Expr, Function, Item, LoopKind, Program, Stmt, Struct, Type},
    error::AlacoError,
    language::BinaryOp,
    stdlib::LoadedModule,
};

#[derive(Debug, Clone)]
struct Variable {
    mutable: bool,
    ty: Option<Type>,
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
        self.check_structs(program)?;
        self.check_functions(program)?;

        for item in &program.items {
            match item {
                Item::Import(_) | Item::Struct(_) => {}

                Item::Function(function) => {
                    self.analyze_function(function, program)?;
                }

                Item::Statement(_) => {
                    return Err(self.error("top-level statements are not allowed"));
                }
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Struct validation
    // ------------------------------------------------------------

    fn check_structs(&self, program: &Program) -> Result<(), AlacoError> {
        let mut structs = HashSet::new();

        for item in &program.items {
            let Item::Struct(struct_def) = item else {
                continue;
            };

            if !structs.insert(struct_def.name.as_str()) {
                return Err(self.error(format!("duplicate struct '{}'", struct_def.name)));
            }

            let mut fields = HashSet::new();

            for field in &struct_def.fields {
                if !fields.insert(field.name.as_str()) {
                    return Err(self.error(format!(
                        "duplicate field '{}' in struct '{}'",
                        field.name, struct_def.name
                    )));
                }

                self.validate_type(program, &field.ty)?;
            }
        }

        Ok(())
    }

    fn validate_type(&self, program: &Program, ty: &Type) -> Result<(), AlacoError> {
        if let Type::Named(name) = ty {
            if self.find_struct(program, name).is_none() {
                return Err(self.error(format!("unknown type '{}'", name)));
            }
        }

        Ok(())
    }

    fn find_struct<'b>(&self, program: &'b Program, name: &str) -> Option<&'b Struct> {
        program.items.iter().find_map(|item| match item {
            Item::Struct(struct_def) if struct_def.name == name => Some(struct_def),

            _ => None,
        })
    }

    fn find_struct_field<'b>(
        &self,
        program: &'b Program,
        struct_name: &str,
        field_name: &str,
    ) -> Option<&'b Type> {
        self.find_struct(program, struct_name)?
            .fields
            .iter()
            .find(|field| field.name == field_name)
            .map(|field| &field.ty)
    }

    // ------------------------------------------------------------
    // Functions
    // ------------------------------------------------------------

    fn check_functions(&self, program: &Program) -> Result<(), AlacoError> {
        let mut functions = HashSet::new();
        let mut main_count = 0;

        for item in &program.items {
            let function = match item {
                Item::Function(function) => function,

                Item::Import(_) | Item::Struct(_) => continue,

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

            for parameter in &function.params {
                self.validate_type(program, &parameter.ty)?;
            }

            if let Some(return_type) = &function.return_type {
                self.validate_type(program, return_type)?;
            }
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

    // ------------------------------------------------------------
    // Function analysis
    // ------------------------------------------------------------

    fn analyze_function(&self, function: &Function, program: &Program) -> Result<(), AlacoError> {
        let mut scopes = Vec::new();

        scopes.push(HashMap::new());

        for parameter in &function.params {
            scopes
                .last_mut()
                .expect("analyzer scope stack is empty")
                .insert(
                    parameter.name.clone(),
                    Variable {
                        mutable: true,
                        ty: Some(parameter.ty.clone()),
                    },
                );
        }

        self.analyze_block(&function.body, &mut scopes, 0, program)
    }

    fn analyze_block(
        &self,
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
        program: &Program,
    ) -> Result<(), AlacoError> {
        scopes.push(HashMap::new());

        for statement in &block.statements {
            self.analyze_statement(statement, scopes, loop_depth, program)?;
        }

        scopes.pop();

        Ok(())
    }

    // ------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------

    fn analyze_statement(
        &self,
        statement: &Stmt,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
        program: &Program,
    ) -> Result<(), AlacoError> {
        match statement {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let value_type = self.analyze_expression(value, scopes, program)?;

                if let Some(declared_type) = ty {
                    self.validate_type(program, declared_type)?;

                    if let Some(actual_type) = &value_type {
                        if !self.types_compatible(declared_type, &actual_type) {
                            return Err(self.error(format!(
                                "type mismatch: variable '{}' expects '{}', found '{}'",
                                name,
                                self.type_display(declared_type),
                                self.type_display(&actual_type),
                            )));
                        }
                    }
                }

                let scope = scopes.last_mut().expect("analyzer scope stack is empty");

                if scope.contains_key(name) {
                    return Err(self.error(format!(
                        "variable '{}' is already declared in this scope",
                        name
                    )));
                }

                scope.insert(
                    name.clone(),
                    Variable {
                        mutable: *mutable,
                        ty: ty.clone().or(value_type),
                    },
                );
            }

            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.analyze_expression(value, scopes, program)?;
                }
            }

            Stmt::Stop(value) => {
                if loop_depth == 0 {
                    return Err(self.error("'stop' can only be used inside a loop"));
                }

                if let Some(value) = value {
                    self.analyze_expression(value, scopes, program)?;
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
                let condition_type = self.analyze_expression(condition, scopes, program)?;

                if let Some(ty) = condition_type {
                    if ty != Type::Bool {
                        return Err(self.error("'if' condition must be a bool"));
                    }
                }

                self.analyze_block(then_block, scopes, loop_depth, program)?;

                if let Some(else_block) = else_block {
                    self.analyze_block(else_block, scopes, loop_depth, program)?;
                }
            }

            Stmt::Loop {
                kind,
                binding,
                body,
            } => {
                self.analyze_loop(kind, binding.as_deref(), body, scopes, loop_depth, program)?;
            }

            Stmt::Expr(expression) => {
                self.analyze_expression(expression, scopes, program)?;
            }

            Stmt::Match { expression, arms } => {
                self.analyze_expression(expression, scopes, program)?;

                for arm in arms {
                    self.analyze_block(&arm.body, scopes, loop_depth, program)?;
                }
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Loops
    // ------------------------------------------------------------

    fn analyze_loop(
        &self,
        kind: &LoopKind,
        binding: Option<&str>,
        body: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
        program: &Program,
    ) -> Result<(), AlacoError> {
        match kind {
            LoopKind::Infinite => {}

            LoopKind::Repeat(expression) => {
                self.analyze_expression(expression, scopes, program)?;
            }

            LoopKind::While(condition) => {
                self.analyze_expression(condition, scopes, program)?;
            }

            LoopKind::For { variable, iterable } => {
                self.analyze_expression(iterable, scopes, program)?;

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
                    .insert(
                        variable.clone(),
                        Variable {
                            mutable: false,
                            ty: None,
                        },
                    );

                self.analyze_block_with_existing_scope(
                    body,
                    &mut loop_scopes,
                    loop_depth + 1,
                    program,
                )?;

                return Ok(());
            }
        }

        let mut loop_scopes = scopes.clone();

        loop_scopes.push(HashMap::new());

        if let Some(binding) = binding {
            loop_scopes
                .last_mut()
                .expect("analyzer scope stack is empty")
                .insert(
                    binding.to_string(),
                    Variable {
                        mutable: false,
                        ty: None,
                    },
                );
        }

        self.analyze_block_with_existing_scope(body, &mut loop_scopes, loop_depth + 1, program)
    }

    fn analyze_block_with_existing_scope(
        &self,
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
        program: &Program,
    ) -> Result<(), AlacoError> {
        for statement in &block.statements {
            self.analyze_statement(statement, scopes, loop_depth, program)?;
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Expressions + type inference
    // ------------------------------------------------------------

    fn analyze_expression(
        &self,
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
        program: &Program,
    ) -> Result<Option<Type>, AlacoError> {
        match expression {
            Expr::Number(value) => {
                if value.contains('.') {
                    Ok(Some(Type::Float))
                } else {
                    Ok(Some(Type::Int))
                }
            }

            Expr::String(_) => Ok(Some(Type::String)),

            Expr::Bool(_) => Ok(Some(Type::Bool)),

            Expr::Identifier(name) => {
                if self.is_builtin(name) {
                    return Ok(None);
                }

                let Some(variable) = self.find_variable(scopes, name) else {
                    return Err(self.error(format!("use of undefined variable '{}'", name)));
                };

                Ok(variable.ty.clone())
            }

            Expr::Unary { operand, .. } => self.analyze_expression(operand, scopes, program),

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left_type = self.analyze_expression(left, scopes, program)?;

                let right_type = self.analyze_expression(right, scopes, program)?;

                if matches!(
                    operator,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubtractAssign
                        | BinaryOp::MultiplyAssign
                        | BinaryOp::DivideAssign
                ) {
                    self.check_assignment_target(left, scopes, program)?;

                    if let (Some(left_type), Some(right_type)) = (&left_type, &right_type) {
                        if !self.types_compatible(left_type, right_type) {
                            return Err(self.error(format!(
                                "cannot assign '{}' to '{}'",
                                self.type_display(right_type),
                                self.type_display(left_type),
                            )));
                        }
                    }

                    return Ok(left_type);
                }

                match operator {
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual => Ok(Some(Type::Bool)),

                    _ => Ok(left_type.or(right_type)),
                }
            }

            Expr::Call { callee, arguments } => {
                self.analyze_call(callee, arguments, scopes, program)?;

                Ok(None)
            }

            Expr::Member { object, member } => {
                let object_type = self.analyze_expression(object, scopes, program)?;

                let Some(object_type) = object_type else {
                    return Ok(None);
                };

                if let Type::Named(struct_name) = &object_type {
                    let Some(field_type) = self.find_struct_field(program, struct_name, member)
                    else {
                        return Err(self.error(format!(
                            "struct '{}' has no field '{}'",
                            struct_name, member
                        )));
                    };

                    return Ok(Some(field_type.clone()));
                }

                Err(self.error(format!(
                    "type '{}' has no fields",
                    self.type_display(&object_type)
                )))
            }

            Expr::StructLiteral { name, fields } => {
                self.analyze_struct_literal(name, fields, scopes, program)
            }
        }
    }

    fn analyze_struct_literal(
        &self,
        name: &str,
        fields: &[(String, Expr)],
        scopes: &mut Vec<HashMap<String, Variable>>,
        program: &Program,
    ) -> Result<Option<Type>, AlacoError> {
        let Some(struct_def) = self.find_struct(program, name) else {
            return Err(self.error(format!("unknown struct '{}'", name)));
        };

        let mut initialized = HashSet::new();

        for (field_name, value) in fields {
            if !initialized.insert(field_name.as_str()) {
                return Err(self.error(format!(
                    "field '{}' is initialized more than once",
                    field_name
                )));
            }

            let Some(field_type) = struct_def
                .fields
                .iter()
                .find(|field| field.name == *field_name)
                .map(|field| &field.ty)
            else {
                return Err(self.error(format!("struct '{}' has no field '{}'", name, field_name)));
            };

            let value_type = self.analyze_expression(value, scopes, program)?;

            if let Some(value_type) = value_type {
                if !self.types_compatible(field_type, &value_type) {
                    return Err(self.error(format!(
                        "field '{}.{}' expects '{}', found '{}'",
                        name,
                        field_name,
                        self.type_display(field_type),
                        self.type_display(&value_type),
                    )));
                }
            }
        }

        // Every declared field must be initialized.
        for field in &struct_def.fields {
            if !initialized.contains(field.name.as_str()) {
                return Err(self.error(format!("missing field '{}.{}'", name, field.name)));
            }
        }

        Ok(Some(Type::Named(name.to_string())))
    }

    // ------------------------------------------------------------
    // Calls
    // ------------------------------------------------------------

    fn analyze_call(
        &self,
        callee: &Expr,
        arguments: &[Expr],
        scopes: &mut Vec<HashMap<String, Variable>>,
        program: &Program,
    ) -> Result<(), AlacoError> {
        if let Expr::Member { object, member } = callee {
            if let Expr::Identifier(alias) = object.as_ref() {
                if let Some(module) = self.find_module(alias) {
                    if !Self::module_has_function(module, member) {
                        return Err(
                            self.error(format!("module '{}' has no function '{}'", alias, member))
                        );
                    }

                    for argument in arguments {
                        self.analyze_expression(argument, scopes, program)?;
                    }

                    return Ok(());
                }
            }
        }

        if let Expr::Identifier(name) = callee {
            if self.is_builtin(name) {
                for argument in arguments {
                    self.analyze_expression(argument, scopes, program)?;
                }

                return Ok(());
            }
        }

        self.analyze_expression(callee, scopes, program)?;

        for argument in arguments {
            self.analyze_expression(argument, scopes, program)?;
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Assignment safety
    // ------------------------------------------------------------

    fn check_assignment_target(
        &self,
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
        program: &Program,
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

            Expr::Member { object, member } => {
                let object_type = self.analyze_expression(object, scopes, program)?;

                let Some(object_type) = object_type else {
                    return Err(self.error("cannot assign to field of unknown type"));
                };

                if let Type::Named(struct_name) = object_type {
                    if self
                        .find_struct_field(program, &struct_name, member)
                        .is_none()
                    {
                        return Err(self.error(format!(
                            "struct '{}' has no field '{}'",
                            struct_name, member
                        )));
                    }
                } else {
                    return Err(self.error("only struct fields can be assigned"));
                }
            }

            _ => {
                return Err(self.error("invalid assignment target"));
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------

    fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        expected == actual
    }

    fn type_display(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "Int".to_string(),
            Type::Float => "Float".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::String => "String".to_string(),
            Type::Named(name) => name.clone(),
        }
    }

    fn find_variable<'b>(
        &self,
        scopes: &'b [HashMap<String, Variable>],
        name: &str,
    ) -> Option<&'b Variable> {
        scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn module_has_function(module: &LoadedModule, name: &str) -> bool {
        module.program.items.iter().any(|item| {
            matches!(
                item,
                Item::Function(function)
                    if function.name == name
            )
        })
    }

    fn is_builtin(&self, name: &str) -> bool {
        matches!(name, "print")
    }

    fn find_module(&self, alias: &str) -> Option<&LoadedModule> {
        self.modules.iter().find(|module| module.alias == alias)
    }

    fn error(&self, message: impl Into<String>) -> AlacoError {
        AlacoError::Analysis {
            message: message.into(),
            span: None,
        }
    }
}
