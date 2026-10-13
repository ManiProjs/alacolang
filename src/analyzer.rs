use std::collections::{HashMap, HashSet};

use crate::{
    ast::{
        Block, Expr, Function, Item, LoopKind, MatchPattern, Program, Span, Stmt, Struct, Type,
        UnaryOp,
    },
    error::AlacoError,
    language::BinaryOp,
    stdlib::LoadedModule,
};

#[derive(Debug, Clone)]
struct Variable {
    ty: Type,
    mutable: bool,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    params: Vec<Type>,
    return_type: Type,
    intrinsic: Option<String>,
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
        self.check_top_level(program)?;
        self.check_structs(program)?;

        // Standard-library implementations may call compiler intrinsics, but
        // user programs must only see the public functions exported by std.io.
        let implementation_functions = self.collect_functions(program, true, true)?;
        let public_functions = self.collect_functions(program, false, false)?;

        for module in self.modules {
            for item in &module.program.items {
                if let Item::Function(function) = item {
                    self.analyze_function(function, program, &implementation_functions)?;
                }
            }
        }

        for item in &program.items {
            if let Item::Function(function) = item {
                self.analyze_function(function, program, &public_functions)?;
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Top-level checks
    // ------------------------------------------------------------

    fn check_top_level(&self, program: &Program) -> Result<(), AlacoError> {
        let mut functions = HashSet::new();
        let mut structs = HashSet::new();
        let mut main_count = 0;

        for item in &program.items {
            if let Item::Struct(structure) = item {
                if matches!(
                    structure.name.as_str(),
                    "Int" | "Float" | "Bool" | "String" | "Shell" | "ShellResult" | "Void"
                ) {
                    return Err(self.error(
                        format!(
                            "struct name '{}' is reserved for a built-in type",
                            structure.name
                        ),
                        Some(structure.span),
                    ));
                }
                if !structs.insert(structure.name.as_str()) {
                    return Err(AlacoError::DuplicateStruct {
                        name: structure.name.clone(),
                        span: structure.span.into(),
                    });
                }
            }
        }

        for item in &program.items {
            match item {
                Item::Import(_) => {}

                Item::Struct(_) => {}

                Item::Function(function) => {
                    if function.name.starts_with("__builtin_") {
                        return Err(self.error(
                            format!(
                                "function name '{}' is reserved for compiler internals",
                                function.name
                            ),
                            Some(function.span),
                        ));
                    }

                    if function.name == "main" && main_count > 0 {
                        return Err(AlacoError::MultipleMain {
                            span: function.span.into(),
                        });
                    }

                    if !functions.insert(function.name.as_str()) {
                        return Err(self.error(
                            format!("duplicate function '{}'", function.name),
                            Some(function.span),
                        ));
                    }

                    if function.name == "main" {
                        main_count += 1;

                        if !function.params.is_empty() {
                            return Err(AlacoError::MainHasParameters {
                                span: function.span.into(),
                            });
                        }

                        if function.return_type.is_some() {
                            return Err(AlacoError::MainHasReturnType {
                                span: function.span.into(),
                            });
                        }
                    }

                    self.check_parameters(function, &structs)?;
                }

                Item::Statement(statement) => {
                    return Err(AlacoError::TopLevelStatement {
                        span: statement.span().into(),
                    });
                }
            }
        }

        if main_count == 0 {
            return Err(AlacoError::MissingMain);
        }

        Ok(())
    }

    fn check_structs(&self, program: &Program) -> Result<(), AlacoError> {
        let struct_names = self.struct_names(program);

        for item in &program.items {
            let structure = match item {
                Item::Struct(structure) => structure,
                _ => continue,
            };

            let mut fields = HashSet::new();

            for field in &structure.fields {
                if !fields.insert(field.name.as_str()) {
                    return Err(AlacoError::DuplicateField {
                        field: field.name.clone(),
                        struct_name: structure.name.clone(),
                        span: field.span.into(),
                    });
                }

                self.validate_value_type_at(&field.ty, &struct_names, field.type_span)?;
            }
        }

        Ok(())
    }

    fn check_parameters(
        &self,
        function: &Function,
        struct_names: &HashSet<&str>,
    ) -> Result<(), AlacoError> {
        let mut names = HashSet::new();

        for parameter in &function.params {
            if !names.insert(parameter.name.as_str()) {
                return Err(AlacoError::DuplicateParameter {
                    name: parameter.name.clone(),
                    function: function.name.clone(),
                    span: parameter.span.into(),
                });
            }

            self.validate_value_type_at(&parameter.ty, struct_names, parameter.type_span)?;
        }

        if let Some(return_type) = &function.return_type {
            self.validate_return_type_at(
                return_type,
                struct_names,
                function.return_type_span.unwrap_or(function.span),
            )?;
        }

        Ok(())
    }

    fn validate_type_at(
        &self,
        ty: &Type,
        struct_names: &HashSet<&str>,
        span: Span,
    ) -> Result<(), AlacoError> {
        match ty {
            Type::Named(name) if !struct_names.contains(name.as_str()) => {
                Err(AlacoError::UnknownType {
                    name: name.clone(),
                    span: span.into(),
                })
            }
            _ => Ok(()),
        }
    }

    fn validate_return_type_at(
        &self,
        ty: &Type,
        struct_names: &HashSet<&str>,
        span: Span,
    ) -> Result<(), AlacoError> {
        self.validate_type_at(ty, struct_names, span)
    }

    fn validate_value_type_at(
        &self,
        ty: &Type,
        struct_names: &HashSet<&str>,
        span: Span,
    ) -> Result<(), AlacoError> {
        if matches!(ty, Type::Void) {
            return Err(self.error(
                "Void cannot be used as a variable, parameter, or struct field type",
                Some(span),
            ));
        }
        self.validate_type_at(ty, struct_names, span)
    }

    fn struct_names<'b>(&self, program: &'b Program) -> HashSet<&'b str> {
        program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(structure) => Some(structure.name.as_str()),
                _ => None,
            })
            .collect()
    }

    // ------------------------------------------------------------
    // Functions
    // ------------------------------------------------------------

    fn collect_functions(
        &self,
        program: &Program,
        include_intrinsics: bool,
        include_namespaced_modules: bool,
    ) -> Result<HashMap<String, FunctionInfo>, AlacoError> {
        let mut functions = HashMap::new();

        // Compiler-provided implementation hooks are ordinary call targets.
        // Their public wrappers are defined in std.io.
        if include_intrinsics {
            for name in [
                "__builtin_print",
                "__builtin_println",
                "__builtin_eprint",
                "__builtin_eprintln",
            ] {
                functions.insert(
                    name.to_string(),
                    FunctionInfo {
                        params: vec![Type::String],
                        return_type: Type::Void,
                        intrinsic: Some(name.to_string()),
                    },
                );
            }

            // Runtime-backed standard-library hooks. They are visible while
            // analyzing std implementations, never directly to user code.
            for (name, params, return_type) in [
                ("__builtin_fs_read_text", vec![Type::String], Type::String),
                ("__builtin_fs_file_size", vec![Type::String], Type::Int),
                (
                    "__builtin_fs_copy",
                    vec![Type::String, Type::String],
                    Type::Bool,
                ),
                (
                    "__builtin_fs_write_text",
                    vec![Type::String, Type::String],
                    Type::Bool,
                ),
                ("__builtin_fs_exists", vec![Type::String], Type::Bool),
                ("__builtin_fs_remove", vec![Type::String], Type::Bool),
                ("__builtin_fs_create_dir", vec![Type::String], Type::Bool),
                ("__builtin_env_get", vec![Type::String], Type::String),
                ("__builtin_env_has", vec![Type::String], Type::Bool),
                (
                    "__builtin_env_set",
                    vec![Type::String, Type::String],
                    Type::Bool,
                ),
                ("__builtin_env_current_dir", vec![], Type::String),
                (
                    "__builtin_path_join",
                    vec![Type::String, Type::String],
                    Type::String,
                ),
                ("__builtin_path_basename", vec![Type::String], Type::String),
                ("__builtin_path_dirname", vec![Type::String], Type::String),
                ("__builtin_path_extension", vec![Type::String], Type::String),
                ("__builtin_path_stem", vec![Type::String], Type::String),
                ("__builtin_path_is_absolute", vec![Type::String], Type::Bool),
                ("__builtin_json_is_valid", vec![Type::String], Type::Bool),
                ("__builtin_json_escape", vec![Type::String], Type::String),
                (
                    "__builtin_json_has_key",
                    vec![Type::String, Type::String],
                    Type::Bool,
                ),
                (
                    "__builtin_json_get_int",
                    vec![Type::String, Type::String, Type::Int],
                    Type::Int,
                ),
                (
                    "__builtin_json_get_bool",
                    vec![Type::String, Type::String, Type::Bool],
                    Type::Bool,
                ),
                (
                    "__builtin_json_get_string",
                    vec![Type::String, Type::String, Type::String],
                    Type::String,
                ),
                ("__builtin_toml_is_valid", vec![Type::String], Type::Bool),
                (
                    "__builtin_toml_has_key",
                    vec![Type::String, Type::String],
                    Type::Bool,
                ),
                (
                    "__builtin_toml_get_string",
                    vec![Type::String, Type::String, Type::String],
                    Type::String,
                ),
                (
                    "__builtin_toml_get_int",
                    vec![Type::String, Type::String, Type::Int],
                    Type::Int,
                ),
                (
                    "__builtin_toml_get_bool",
                    vec![Type::String, Type::String, Type::Bool],
                    Type::Bool,
                ),
                ("__builtin_http_get", vec![Type::String], Type::String),
                (
                    "__builtin_http_post",
                    vec![Type::String, Type::String],
                    Type::String,
                ),
                ("__builtin_http_is_url", vec![Type::String], Type::Bool),
                ("__builtin_time_unix_seconds", vec![], Type::Int),
                ("__builtin_time_sleep_ms", vec![Type::Int], Type::Void),
            ] {
                functions.insert(
                    name.to_string(),
                    FunctionInfo {
                        params,
                        return_type,
                        intrinsic: Some(name.to_string()),
                    },
                );
            }
        }

        for module in self.modules {
            // std.io's functions are part of the default unqualified API.
            // Other modules are accessed through their import alias, e.g.
            // math.max(...), and must not leak into the global function scope.
            if !include_namespaced_modules && module.name != "std.io" {
                continue;
            }

            for item in &module.program.items {
                let Item::Function(function) = item else {
                    continue;
                };

                functions.insert(
                    function.name.clone(),
                    FunctionInfo {
                        params: function
                            .params
                            .iter()
                            .map(|parameter| parameter.ty.clone())
                            .collect(),
                        return_type: function.return_type.clone().unwrap_or(Type::Void),
                        intrinsic: function.intrinsic.clone(),
                    },
                );
            }
        }

        for item in &program.items {
            let function = match item {
                Item::Function(function) => function,
                _ => continue,
            };

            let return_type = function.return_type.clone().unwrap_or(Type::Void);

            functions.insert(
                function.name.clone(),
                FunctionInfo {
                    params: function
                        .params
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect(),
                    return_type,
                    intrinsic: function.intrinsic.clone(),
                },
            );
        }

        Ok(functions)
    }

    fn analyze_function(
        &self,
        function: &Function,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<(), AlacoError> {
        let mut scopes = Vec::new();

        scopes.push(HashMap::new());

        for parameter in &function.params {
            scopes
                .last_mut()
                .expect("analyzer scope stack is empty")
                .insert(
                    parameter.name.clone(),
                    Variable {
                        ty: parameter.ty.clone(),
                        mutable: true,
                    },
                );
        }

        let expected_return = function.return_type.clone().unwrap_or(Type::Void);

        self.analyze_block(
            &function.body,
            &mut scopes,
            0,
            &expected_return,
            program,
            functions,
        )?;

        if expected_return != Type::Void && !Self::block_guarantees_return(&function.body) {
            return Err(self.error(
                format!(
                    "function '{}' may finish without returning a value of type '{}'",
                    function.name,
                    self.type_name(&expected_return)
                ),
                Some(function.span),
            ));
        }

        Ok(())
    }

    fn block_guarantees_return(block: &Block) -> bool {
        for statement in &block.statements {
            let always_returns = match statement {
                Stmt::Return(..) => true,
                Stmt::If {
                    then_block,
                    else_block: Some(else_block),
                    ..
                } => {
                    Self::block_guarantees_return(then_block)
                        && Self::block_guarantees_return(else_block)
                }
                Stmt::Match { arms, .. } => {
                    arms.iter()
                        .any(|arm| matches!(arm.pattern, MatchPattern::Wildcard))
                        && arms
                            .iter()
                            .all(|arm| Self::block_guarantees_return(&arm.body))
                }
                _ => false,
            };

            if always_returns {
                return true;
            }
        }

        false
    }

    // ------------------------------------------------------------
    // Blocks
    // ------------------------------------------------------------

    fn analyze_block(
        &self,
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
        expected_return: &Type,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<(), AlacoError> {
        scopes.push(HashMap::new());

        for statement in &block.statements {
            self.analyze_statement(
                statement,
                scopes,
                loop_depth,
                expected_return,
                program,
                functions,
            )?;
        }

        scopes.pop();

        Ok(())
    }

    fn analyze_block_with_existing_scope(
        &self,
        block: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
        expected_return: &Type,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<(), AlacoError> {
        for statement in &block.statements {
            self.analyze_statement(
                statement,
                scopes,
                loop_depth,
                expected_return,
                program,
                functions,
            )?;
        }

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
        expected_return: &Type,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<(), AlacoError> {
        match statement {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
                span,
            } => {
                let value_type = self.type_of_expression(value, scopes, program, functions)?;

                if value_type == Type::Void {
                    return Err(self.error(
                        "a Void expression cannot initialize a variable",
                        Some(value.span()),
                    ));
                }

                if let Some(expected_type) = ty {
                    let struct_names = self.struct_names(program);

                    self.validate_value_type_at(expected_type, &struct_names, *span)?;

                    self.ensure_assignable_at(
                        expected_type,
                        &value_type,
                        &format!("variable '{}'", name),
                        value.span(),
                    )?;
                }

                let final_type = ty.clone().unwrap_or(value_type);

                let scope = scopes.last_mut().expect("analyzer scope stack is empty");

                if scope.contains_key(name) {
                    return Err(AlacoError::DuplicateVariable {
                        name: name.clone(),
                        span: (*span).into(),
                    });
                }

                scope.insert(
                    name.clone(),
                    Variable {
                        ty: final_type,
                        mutable: *mutable,
                    },
                );
            }

            Stmt::Destructure {
                struct_name,
                mutable,
                bindings,
                value,
                span,
            } => {
                let value_type = self.type_of_expression(value, scopes, program, functions)?;
                if value_type != Type::Named(struct_name.clone()) {
                    return Err(self.error(
                        format!(
                            "cannot destructure '{}' from type '{}'",
                            struct_name,
                            self.type_name(&value_type)
                        ),
                        Some(value.span()),
                    ));
                }
                let structure = self.find_struct(program, struct_name).ok_or_else(|| {
                    self.error(format!("unknown struct '{}'", struct_name), Some(*span))
                })?;
                let mut seen_fields = HashSet::new();
                let mut seen_bindings = HashSet::new();
                for (field_name, binding_name) in bindings {
                    if !seen_fields.insert(field_name.as_str()) {
                        return Err(self.error(
                            format!("field '{}' is destructured more than once", field_name),
                            Some(*span),
                        ));
                    }
                    if !seen_bindings.insert(binding_name.as_str()) {
                        return Err(self.error(
                            format!("duplicate destructuring binding '{}'", binding_name),
                            Some(*span),
                        ));
                    }
                    if !structure
                        .fields
                        .iter()
                        .any(|field| field.name == *field_name)
                    {
                        let suggestion = closest_name(
                            field_name,
                            structure.fields.iter().map(|field| field.name.as_str()),
                        );
                        let message = match suggestion {
                            Some(suggestion) => format!(
                                "struct '{}' has no field '{}'; did you mean '{}' ?",
                                struct_name, field_name, suggestion
                            ),
                            None => {
                                format!("struct '{}' has no field '{}'", struct_name, field_name)
                            }
                        };
                        return Err(self.error(message, Some(*span)));
                    }
                    if scopes
                        .last()
                        .expect("analyzer scope stack is empty")
                        .contains_key(binding_name)
                    {
                        return Err(AlacoError::DuplicateVariable {
                            name: binding_name.clone(),
                            span: (*span).into(),
                        });
                    }
                }
                let scope = scopes.last_mut().expect("analyzer scope stack is empty");
                for (field_name, binding_name) in bindings {
                    let field = structure
                        .fields
                        .iter()
                        .find(|field| field.name == *field_name)
                        .expect("field validated above");
                    scope.insert(
                        binding_name.clone(),
                        Variable {
                            ty: field.ty.clone(),
                            mutable: *mutable,
                        },
                    );
                }
            }

            Stmt::Return(value, span) => {
                let actual_type = match value {
                    Some(value) => self.type_of_expression(value, scopes, program, functions)?,

                    None => Type::Void,
                };

                self.ensure_assignable_at(
                    expected_return,
                    &actual_type,
                    "return value",
                    value.as_ref().map(Expr::span).unwrap_or(*span),
                )?;
            }

            Stmt::Stop(value, span) => {
                if loop_depth == 0 {
                    return Err(AlacoError::StopOutsideLoop {
                        span: (*span).into(),
                    });
                }

                if let Some(value) = value {
                    let actual_type = self.type_of_expression(value, scopes, program, functions)?;
                    self.ensure_assignable_at(
                        expected_return,
                        &actual_type,
                        "stop value",
                        value.span(),
                    )?;
                } else if *expected_return != Type::Void {
                    return Err(self.error(
                        format!(
                            "stop without a value cannot exit a function returning {}",
                            self.type_name(expected_return)
                        ),
                        Some(*span),
                    ));
                }
            }

            Stmt::Skip(span) => {
                if loop_depth == 0 {
                    return Err(AlacoError::SkipOutsideLoop {
                        span: (*span).into(),
                    });
                }
            }

            Stmt::If {
                condition,
                then_block,
                else_block,
                span: _,
            } => {
                let condition_type =
                    self.type_of_expression(condition, scopes, program, functions)?;

                self.require_type_at(
                    &Type::Bool,
                    &condition_type,
                    "if condition",
                    condition.span(),
                )?;

                self.analyze_block(
                    then_block,
                    scopes,
                    loop_depth,
                    expected_return,
                    program,
                    functions,
                )?;

                if let Some(else_block) = else_block {
                    self.analyze_block(
                        else_block,
                        scopes,
                        loop_depth,
                        expected_return,
                        program,
                        functions,
                    )?;
                }
            }

            Stmt::Loop {
                kind,
                binding,
                body,
                span,
            } => {
                self.analyze_loop(
                    kind,
                    binding.as_deref(),
                    *span,
                    body,
                    scopes,
                    loop_depth,
                    expected_return,
                    program,
                    functions,
                )?;
            }

            Stmt::Expr(expression, _) => {
                self.type_of_expression(expression, scopes, program, functions)?;
            }

            Stmt::Match {
                expression,
                arms,
                span: _,
            } => {
                let expression_type =
                    self.type_of_expression(expression, scopes, program, functions)?;

                for arm in arms {
                    self.check_match_pattern(&arm.pattern, &expression_type, expression.span())?;

                    self.analyze_block(
                        &arm.body,
                        scopes,
                        loop_depth,
                        expected_return,
                        program,
                        functions,
                    )?;
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
        span: Span,
        body: &Block,
        scopes: &mut Vec<HashMap<String, Variable>>,
        loop_depth: usize,
        expected_return: &Type,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<(), AlacoError> {
        match kind {
            LoopKind::Infinite => {}

            LoopKind::Repeat(expression) => {
                let ty = self.type_of_expression(expression, scopes, program, functions)?;

                self.require_type_at(&Type::Int, &ty, "repeat count", expression.span())?;
            }

            LoopKind::While(condition) => {
                let ty = self.type_of_expression(condition, scopes, program, functions)?;

                self.require_type_at(&Type::Bool, &ty, "while condition", condition.span())?;
            }

            LoopKind::For {
                variable: _,
                iterable,
            } => {
                let iterable_type =
                    self.type_of_expression(iterable, scopes, program, functions)?;

                return Err(self.error(
                    format!(
                        "'for' loops cannot iterate over '{}' yet",
                        self.type_name(&iterable_type)
                    ),
                    Some(span),
                ));
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
                        ty: Type::Int,
                        mutable: false,
                    },
                );
        }

        self.analyze_block_with_existing_scope(
            body,
            &mut loop_scopes,
            loop_depth + 1,
            expected_return,
            program,
            functions,
        )
    }

    // ------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------

    fn type_of_expression(
        &self,
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<Type, AlacoError> {
        match expression {
            Expr::Number(value, _) => {
                if value.contains('.') {
                    Ok(Type::Float)
                } else {
                    Ok(Type::Int)
                }
            }

            Expr::String(_, _) => Ok(Type::String),

            Expr::Bool(_, _) => Ok(Type::Bool),

            // Shell expressions are first-class values.
            //
            // Example:
            //
            // let say_hello = shell(
            //     echo "Hi!"
            // );
            //
            Expr::Shell { .. } => Ok(Type::Shell),

            Expr::Identifier(name, span) => {
                if let Some(variable) = self.find_variable(scopes, name) {
                    return Ok(variable.ty.clone());
                }

                if functions.contains_key(name) {
                    return Err(self.error(
                        format!("function '{}' cannot be used as a value", name),
                        Some(*span),
                    ));
                }

                Err(AlacoError::UndefinedVariable {
                    name: name.clone(),
                    span: (*span).into(),
                })
            }

            Expr::StructLiteral { name, fields, span } => {
                self.type_of_struct_literal(name, fields, *span, program, scopes, functions)
            }

            Expr::Unary {
                operator,
                operand,
                span,
            } => {
                let operand_type = self.type_of_expression(operand, scopes, program, functions)?;

                match operator {
                    UnaryOp::Negate => {
                        if !self.is_numeric(&operand_type) {
                            return Err(self.error(
                                format!("cannot negate {}", self.type_name(&operand_type)),
                                Some(*span),
                            ));
                        }

                        Ok(operand_type)
                    }
                    UnaryOp::Not => {
                        self.require_type_at(
                            &Type::Bool,
                            &operand_type,
                            "operand of logical not",
                            operand.span(),
                        )?;
                        Ok(Type::Bool)
                    }
                }
            }

            Expr::Binary {
                left,
                operator,
                right,
                span,
            } => self.type_of_binary(left, *operator, right, *span, scopes, program, functions),

            Expr::Pipe {
                input,
                target,
                span,
            } => {
                let mut arguments = vec![(**input).clone()];
                let (callee, additional_arguments): (&Expr, &[Expr]) = match target.as_ref() {
                    Expr::Call {
                        callee, arguments, ..
                    } => (callee, arguments),
                    other => (other, &[]),
                };
                arguments.extend(additional_arguments.iter().cloned());
                self.type_of_call(callee, &arguments, scopes, program, functions)
                    .map_err(|error| match error {
                        AlacoError::Analysis {
                            message,
                            span: None,
                        } => self.error(message, Some(*span)),
                        other => other,
                    })
            }

            Expr::Call {
                callee,
                arguments,
                span,
            } => self
                .type_of_call(callee, arguments, scopes, program, functions)
                .map_err(|error| match error {
                    AlacoError::Analysis {
                        message,
                        span: None,
                    } => self.error(message, Some(*span)),
                    other => other,
                }),

            Expr::Member {
                object,
                member,
                span,
            } => {
                let object_type = self.type_of_expression(object, scopes, program, functions)?;

                self.type_of_member(&object_type, member, program, *span)
            }

            Expr::Intrinsic {
                name,
                arguments,
                span,
            } => {
                let function = functions.get(name).ok_or_else(|| {
                    self.error(format!("unknown intrinsic '{}'", name), Some(*span))
                })?;

                if function.intrinsic.is_none() {
                    return Err(self.error(format!("'{}' is not an intrinsic", name), Some(*span)));
                }

                if arguments.len() != function.params.len() {
                    return Err(self.error(
                        format!(
                            "intrinsic '{}' expects {} argument(s), but {} were provided",
                            name,
                            function.params.len(),
                            arguments.len()
                        ),
                        Some(*span),
                    ));
                }

                for (index, argument) in arguments.iter().enumerate() {
                    let actual = self.type_of_expression(argument, scopes, program, functions)?;

                    self.ensure_assignable_at(
                        &function.params[index],
                        &actual,
                        &format!("argument {} of intrinsic '{}'", index + 1, name),
                        argument.span(),
                    )?;
                }

                Ok(function.return_type.clone())
            }
        }
    }

    // ------------------------------------------------------------
    // Structs
    // ------------------------------------------------------------

    fn type_of_struct_literal(
        &self,
        name: &str,
        fields: &[(String, Expr)],
        span: Span,
        program: &Program,
        scopes: &mut Vec<HashMap<String, Variable>>,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<Type, AlacoError> {
        let structure = self
            .find_struct(program, name)
            .ok_or_else(|| self.error(format!("unknown struct '{}'", name), Some(span)))?;

        let mut supplied = HashSet::new();

        for (field_name, value) in fields {
            if !supplied.insert(field_name.as_str()) {
                return Err(self.error(
                    format!("field '{}' is specified more than once", field_name),
                    Some(value.span()),
                ));
            }

            let field = structure
                .fields
                .iter()
                .find(|field| field.name == *field_name)
                .ok_or_else(|| {
                    let suggestion = closest_name(
                        field_name,
                        structure.fields.iter().map(|field| field.name.as_str()),
                    );
                    let message = match suggestion {
                        Some(suggestion) => format!(
                            "struct '{}' has no field '{}'; did you mean '{}'?",
                            name, field_name, suggestion
                        ),
                        None => format!("struct '{}' has no field '{}'", name, field_name),
                    };
                    self.error(message, Some(value.span()))
                })?;

            let actual_type = self.type_of_expression(value, scopes, program, functions)?;

            self.ensure_assignable_at(
                &field.ty,
                &actual_type,
                &format!("field '{}.{}'", name, field_name),
                value.span(),
            )?;
        }

        for field in &structure.fields {
            if !supplied.contains(field.name.as_str()) {
                return Err(self.error(
                    format!(
                        "missing field '{}' in struct literal '{}'",
                        field.name, name
                    ),
                    Some(span),
                ));
            }
        }

        Ok(Type::Named(name.to_string()))
    }

    fn type_of_member(
        &self,
        object_type: &Type,
        member: &str,
        program: &Program,
        span: Span,
    ) -> Result<Type, AlacoError> {
        match object_type {
            Type::Named(name) => {
                let structure = self
                    .find_struct(program, name)
                    .ok_or_else(|| self.error(format!("unknown type '{}'", name), Some(span)))?;

                let field = structure
                    .fields
                    .iter()
                    .find(|field| field.name == member)
                    .ok_or_else(|| {
                        let suggestion = closest_name(
                            member,
                            structure.fields.iter().map(|field| field.name.as_str()),
                        );
                        let message = match suggestion {
                            Some(suggestion) => format!(
                                "struct '{}' has no field '{}'; did you mean '{}'?",
                                name, member, suggestion
                            ),
                            None => format!("struct '{}' has no field '{}'", name, member),
                        };
                        self.error(message, Some(span))
                    })?;

                Ok(field.ty.clone())
            }

            Type::Shell => {
                if member == "get" {
                    Ok(Type::ShellResult)
                } else {
                    Err(self.error(
                        format!("type 'Shell' has no member '{}'", member),
                        Some(span),
                    ))
                }
            }

            Type::ShellResult => match member {
                "output" => Ok(Type::String),
                "status" => Ok(Type::Int),
                "success" => Ok(Type::Bool),
                _ => Err(self.error(
                    format!("type 'ShellResult' has no member '{}'", member),
                    Some(span),
                )),
            },

            _ => Err(self.error(
                format!("type '{}' has no members", self.type_name(object_type)),
                Some(span),
            )),
        }
    }

    fn find_struct<'b>(&self, program: &'b Program, name: &str) -> Option<&'b Struct> {
        program.items.iter().find_map(|item| match item {
            Item::Struct(structure) if structure.name == name => Some(structure),

            _ => None,
        })
    }

    // ------------------------------------------------------------
    // Function calls
    // ------------------------------------------------------------

    fn type_of_call(
        &self,
        callee: &Expr,
        arguments: &[Expr],
        scopes: &mut Vec<HashMap<String, Variable>>,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<Type, AlacoError> {
        if let Expr::Identifier(name, span) = callee {
            let function = functions.get(name).ok_or_else(|| {
                let suggestion = self.closest_function_name(name, functions);
                let message = match suggestion {
                    Some(suggestion) => {
                        format!("unknown function '{}'; did you mean '{}'", name, suggestion)
                    }
                    None => format!("unknown function '{}'", name),
                };
                self.error(message, Some(*span))
            })?;

            // Printing is intentionally polymorphic: the runtime provides
            // formatting for built-in and user-defined value types.
            if matches!(name.as_str(), "print" | "println" | "eprint" | "eprintln") {
                if arguments.len() != 1 {
                    return Err(self.error(
                        format!(
                            "function '{}' expects 1 argument(s), but {} were provided",
                            name,
                            arguments.len()
                        ),
                        Some(*span),
                    ));
                }
                let actual = self.type_of_expression(&arguments[0], scopes, program, functions)?;
                if actual == Type::Void {
                    return Err(self.error(
                        "cannot print a value of type 'Void'",
                        Some(arguments[0].span()),
                    ));
                }
                return Ok(Type::Void);
            }

            if arguments.len() != function.params.len() {
                return Err(self.error(
                    format!(
                        "function '{}' expects {} argument(s), but {} were provided",
                        name,
                        function.params.len(),
                        arguments.len()
                    ),
                    Some(*span),
                ));
            }

            for (index, argument) in arguments.iter().enumerate() {
                let actual = self.type_of_expression(argument, scopes, program, functions)?;

                self.ensure_assignable_at(
                    &function.params[index],
                    &actual,
                    &format!("argument {} of function '{}'", index + 1, name),
                    argument.span(),
                )?;
            }

            return Ok(function.return_type.clone());
        }

        if let Expr::Member {
            object,
            member,
            span,
        } = callee
        {
            if let Expr::Identifier(alias, _) = object.as_ref() {
                if let Some(module) = self.find_module(alias) {
                    let function = module
                        .program
                        .items
                        .iter()
                        .find_map(|item| match item {
                            Item::Function(function) if function.name == *member => Some(function),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            let suggestion = closest_name(
                                member,
                                module.program.items.iter().filter_map(|item| match item {
                                    Item::Function(function) => Some(function.name.as_str()),
                                    _ => None,
                                }),
                            );
                            let message = match suggestion {
                                Some(suggestion) => format!(
                                    "module '{}' has no function '{}'; did you mean '{}.{}'?",
                                    alias, member, alias, suggestion
                                ),
                                None => format!("module '{}' has no function '{}'", alias, member),
                            };
                            self.error(message, Some(callee.span()))
                        })?;

                    if arguments.len() != function.params.len() {
                        return Err(self.error(
                            format!(
                                "function '{}.{}' expects {} argument(s), but {} were provided",
                                alias,
                                member,
                                function.params.len(),
                                arguments.len()
                            ),
                            Some(callee.span()),
                        ));
                    }

                    for (index, (argument, parameter)) in
                        arguments.iter().zip(&function.params).enumerate()
                    {
                        let actual =
                            self.type_of_expression(argument, scopes, program, functions)?;
                        self.ensure_assignable_at(
                            &parameter.ty,
                            &actual,
                            &format!("argument {} of function '{}.{}'", index + 1, alias, member),
                            argument.span(),
                        )?;
                    }

                    return Ok(function.return_type.clone().unwrap_or(Type::Void));
                }
            }

            let object_type = self.type_of_expression(object, scopes, program, functions)?;
            // `get()` is the one built-in method exposed by shell futures.
            // Other members are values (struct fields / ShellResult fields),
            // not callable functions. Accepting them here would defer an
            // ordinary source error to the generated C++ compiler.
            if object_type == Type::Shell && member == "get" {
                if !arguments.is_empty() {
                    return Err(self.error(
                        format!(
                            "method '{}.{}' expects 0 argument(s), but {} were provided",
                            self.type_name(&object_type),
                            member,
                            arguments.len()
                        ),
                        Some(callee.span()),
                    ));
                }
                return Ok(Type::ShellResult);
            }

            self.type_of_member(&object_type, member, program, *span)?;
            return Err(self.error(
                format!(
                    "member '{}.{}' is a value, not a callable method",
                    self.type_name(&object_type),
                    member
                ),
                Some(callee.span()),
            ));
        }

        let callee_type = self.type_of_expression(callee, scopes, program, functions)?;

        Err(self.error(
            format!(
                "cannot call value of type '{}'",
                self.type_name(&callee_type)
            ),
            None,
        ))
    }

    // ------------------------------------------------------------
    // Binary operators
    // ------------------------------------------------------------

    fn type_of_binary(
        &self,
        left: &Expr,
        operator: BinaryOp,
        right: &Expr,
        span: Span,
        scopes: &mut Vec<HashMap<String, Variable>>,
        program: &Program,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<Type, AlacoError> {
        let left_type = self.type_of_expression(left, scopes, program, functions)?;

        let right_type = self.type_of_expression(right, scopes, program, functions)?;

        match operator {
            BinaryOp::Assign
            | BinaryOp::AddAssign
            | BinaryOp::SubtractAssign
            | BinaryOp::MultiplyAssign
            | BinaryOp::DivideAssign => {
                self.check_assignment_target(left, scopes, left.span())?;

                if matches!(
                    operator,
                    BinaryOp::AddAssign
                        | BinaryOp::SubtractAssign
                        | BinaryOp::MultiplyAssign
                        | BinaryOp::DivideAssign
                ) {
                    self.require_numeric_pair_at(&left_type, &right_type, span)?;
                }

                self.ensure_assignable_at(&left_type, &right_type, "assignment", right.span())?;

                Ok(left_type)
            }

            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo => {
                if operator == BinaryOp::Modulo
                    && (left_type != Type::Int || right_type != Type::Int)
                {
                    return Err(
                        self.error("remainder operator '%' requires Int operands", Some(span))
                    );
                }
                if operator == BinaryOp::Add
                    && left_type == Type::String
                    && right_type == Type::String
                {
                    return Ok(Type::String);
                }

                self.require_numeric_pair_at(&left_type, &right_type, span)?;

                if left_type == Type::Float || right_type == Type::Float {
                    Ok(Type::Float)
                } else {
                    Ok(Type::Int)
                }
            }

            BinaryOp::And | BinaryOp::Or => {
                self.require_type_at(&Type::Bool, &left_type, "left logical operand", left.span())?;
                self.require_type_at(
                    &Type::Bool,
                    &right_type,
                    "right logical operand",
                    right.span(),
                )?;
                Ok(Type::Bool)
            }

            BinaryOp::Equal | BinaryOp::NotEqual => {
                if !matches!(
                    left_type,
                    Type::Int | Type::Float | Type::Bool | Type::String
                ) || !matches!(
                    right_type,
                    Type::Int | Type::Float | Type::Bool | Type::String
                ) {
                    return Err(self.error(
                        format!(
                            "equality is not defined for {} and {}",
                            self.type_name(&left_type),
                            self.type_name(&right_type)
                        ),
                        Some(span),
                    ));
                }
                if !self.types_compatible(&left_type, &right_type) {
                    return Err(self.error(
                        format!(
                            "cannot compare {} and {}",
                            self.type_name(&left_type),
                            self.type_name(&right_type)
                        ),
                        Some(span),
                    ));
                }

                Ok(Type::Bool)
            }

            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                self.require_numeric_pair_at(&left_type, &right_type, span)?;

                Ok(Type::Bool)
            }
        }
    }

    // ------------------------------------------------------------
    // Assignment / mutability
    // ------------------------------------------------------------

    fn check_assignment_target(
        &self,
        expression: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
        span: Span,
    ) -> Result<(), AlacoError> {
        match expression {
            Expr::Identifier(name, _) => {
                let Some(variable) = self.find_variable(scopes, name) else {
                    return Err(AlacoError::UndefinedAssignment {
                        name: name.clone(),
                        span: span.into(),
                    });
                };

                if !variable.mutable {
                    return Err(AlacoError::ImmutableAssignment {
                        name: name.clone(),
                        span: span.into(),
                    });
                }
            }

            Expr::Member { object, .. } => {
                self.check_member_assignment_target(object, scopes, span)?;
            }

            _ => {
                return Err(AlacoError::InvalidAssignmentTarget { span: span.into() });
            }
        }

        Ok(())
    }

    fn check_member_assignment_target(
        &self,
        object: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
        span: Span,
    ) -> Result<(), AlacoError> {
        match object {
            Expr::Identifier(name, _) => {
                let Some(variable) = self.find_variable(scopes, name) else {
                    return Err(AlacoError::UndefinedAssignment {
                        name: name.clone(),
                        span: span.into(),
                    });
                };

                if !variable.mutable {
                    return Err(AlacoError::ImmutableAssignment {
                        name: name.clone(),
                        span: span.into(),
                    });
                }

                Ok(())
            }

            Expr::Member { object, .. } => {
                self.check_member_assignment_target(object, scopes, span)
            }

            _ => Err(AlacoError::InvalidAssignmentTarget { span: span.into() }),
        }
    }

    // ------------------------------------------------------------
    // Match
    // ------------------------------------------------------------

    fn check_match_pattern(
        &self,
        pattern: &MatchPattern,
        expression_type: &Type,
        span: Span,
    ) -> Result<(), AlacoError> {
        match pattern {
            MatchPattern::Number(_) => {
                if !self.is_numeric(expression_type) {
                    return Err(self.error(
                        format!(
                            "numeric match pattern cannot match {}",
                            self.type_name(expression_type)
                        ),
                        Some(span),
                    ));
                }
            }

            MatchPattern::String(_) => {
                self.require_type_at(&Type::String, expression_type, "string match pattern", span)?;
            }

            MatchPattern::Bool(_) => {
                self.require_type_at(&Type::Bool, expression_type, "boolean match pattern", span)?;
            }

            MatchPattern::Identifier(_) | MatchPattern::Wildcard => {}
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Type utilities
    // ------------------------------------------------------------

    fn ensure_assignable_at(
        &self,
        expected: &Type,
        actual: &Type,
        context: &str,
        span: Span,
    ) -> Result<(), AlacoError> {
        if self.types_assignable(expected, actual) {
            Ok(())
        } else {
            Err(self.error(
                format!(
                    "type mismatch for {}: expected {}, found {}",
                    context,
                    self.type_name(expected),
                    self.type_name(actual)
                ),
                Some(span),
            ))
        }
    }

    fn require_type_at(
        &self,
        expected: &Type,
        actual: &Type,
        context: &str,
        span: Span,
    ) -> Result<(), AlacoError> {
        self.ensure_assignable_at(expected, actual, context, span)
    }

    fn types_assignable(&self, expected: &Type, actual: &Type) -> bool {
        if expected == actual {
            return true;
        }

        // Numeric widening:
        //
        // Int -> Float
        matches!((expected, actual), (Type::Float, Type::Int))
    }

    fn types_compatible(&self, left: &Type, right: &Type) -> bool {
        left == right
            || matches!(
                (left, right),
                (Type::Float, Type::Int) | (Type::Int, Type::Float)
            )
    }

    fn require_numeric_pair_at(
        &self,
        left: &Type,
        right: &Type,
        span: Span,
    ) -> Result<(), AlacoError> {
        if self.is_numeric(left) && self.is_numeric(right) {
            Ok(())
        } else {
            Err(self.error(
                format!(
                    "numeric operation requires numeric values, found {} and {}",
                    self.type_name(left),
                    self.type_name(right)
                ),
                Some(span),
            ))
        }
    }

    fn is_numeric(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::Float)
    }

    fn type_name(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "Int".to_string(),
            Type::Float => "Float".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::String => "String".to_string(),
            Type::Shell => "Shell".to_string(),
            Type::ShellResult => "ShellResult".to_string(),
            Type::Void => "Void".to_string(),
            Type::Named(name) => name.clone(),
        }
    }

    // ------------------------------------------------------------
    // Variables / modules
    // ------------------------------------------------------------

    fn find_variable<'b>(
        &self,
        scopes: &'b [HashMap<String, Variable>],
        name: &str,
    ) -> Option<&'b Variable> {
        scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn find_module(&self, alias: &str) -> Option<&LoadedModule> {
        self.modules.iter().find(|module| module.alias == alias)
    }

    fn closest_function_name(
        &self,
        name: &str,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Option<String> {
        closest_name(
            name,
            functions
                .keys()
                .filter(|candidate| !candidate.starts_with("__builtin_"))
                .map(String::as_str),
        )
    }

    fn error(&self, message: impl Into<String>, span: Option<Span>) -> AlacoError {
        AlacoError::Analysis {
            message: message.into(),
            span: span.map(Into::into),
        }
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();

    for (left_index, left_char) in left.chars().enumerate() {
        let mut current = Vec::with_capacity(right.len() + 1);
        current.push(left_index + 1);

        for (right_index, right_char) in right.iter().enumerate() {
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            let substitution = previous[right_index] + usize::from(left_char != *right_char);
            current.push(insertion.min(deletion).min(substitution));
        }

        previous = current;
    }

    previous[right.len()]
}

fn closest_name<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let (distance, candidate) = candidates
        .map(|candidate| (edit_distance(name, candidate), candidate))
        .min_by_key(|(distance, candidate)| (*distance, *candidate))?;

    let threshold = (name.chars().count() / 3).max(2);
    (distance <= threshold).then(|| candidate.to_string())
}
