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
                if !structs.insert(structure.name.as_str()) {
                    return Err(self.error(format!("duplicate struct '{}'", structure.name), None));
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
                            None,
                        ));
                    }

                    if !functions.insert(function.name.as_str()) {
                        return Err(
                            self.error(format!("duplicate function '{}'", function.name), None)
                        );
                    }

                    if function.name == "main" {
                        main_count += 1;

                        if !function.params.is_empty() {
                            return Err(self.error("main function cannot have parameters", None));
                        }

                        if function.return_type.is_some() {
                            return Err(self
                                .error("main function cannot have an explicit return type", None));
                        }
                    }

                    self.check_parameters(function, &structs)?;
                }

                Item::Statement(_) => {
                    return Err(self.error(
                        "top-level statements are not allowed; expected a function",
                        None,
                    ));
                }
            }
        }

        if main_count == 0 {
            return Err(self.error("program must contain a 'main' function", None));
        }

        if main_count > 1 {
            return Err(self.error("program can only contain one 'main' function", None));
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
                    return Err(self.error(
                        format!(
                            "duplicate field '{}' in struct '{}'",
                            field.name, structure.name
                        ),
                        None,
                    ));
                }

                self.validate_value_type(&field.ty, &struct_names)?;
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
                return Err(self.error(
                    format!(
                        "duplicate parameter '{}' in function '{}'",
                        parameter.name, function.name
                    ),
                    None,
                ));
            }

            self.validate_value_type(&parameter.ty, struct_names)?;
        }

        if let Some(return_type) = &function.return_type {
            self.validate_return_type(return_type, struct_names)?;
        }

        Ok(())
    }

    fn validate_type(&self, ty: &Type, struct_names: &HashSet<&str>) -> Result<(), AlacoError> {
        match ty {
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Void
            | Type::Shell
            | Type::ShellResult => Ok(()),

            Type::Named(name) => {
                if struct_names.contains(name.as_str()) {
                    Ok(())
                } else {
                    Err(self.error(format!("unknown type '{}'", name), None))
                }
            }
        }
    }

    fn validate_return_type(
        &self,
        ty: &Type,
        struct_names: &HashSet<&str>,
    ) -> Result<(), AlacoError> {
        self.validate_type(ty, struct_names)
    }

    fn validate_value_type(
        &self,
        ty: &Type,
        struct_names: &HashSet<&str>,
    ) -> Result<(), AlacoError> {
        if matches!(ty, Type::Void) {
            return Err(self.error(
                "Void cannot be used as a variable, parameter, or struct field type",
                None,
            ));
        }

        self.validate_type(ty, struct_names)
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
        )
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
                span: _,
            } => {
                let value_type = self.type_of_expression(value, scopes, program, functions)?;

                if let Some(expected_type) = ty {
                    let struct_names = self.struct_names(program);

                    self.validate_value_type(expected_type, &struct_names)?;

                    self.ensure_assignable(
                        expected_type,
                        &value_type,
                        &format!("variable '{}'", name),
                    )?;
                }

                let final_type = ty.clone().unwrap_or(value_type);

                let scope = scopes.last_mut().expect("analyzer scope stack is empty");

                if scope.contains_key(name) {
                    return Err(self.error(
                        format!("variable '{}' is already declared in this scope", name),
                        None,
                    ));
                }

                scope.insert(
                    name.clone(),
                    Variable {
                        ty: final_type,
                        mutable: *mutable,
                    },
                );
            }

            Stmt::Return(value, _) => {
                let actual_type = match value {
                    Some(value) => self.type_of_expression(value, scopes, program, functions)?,

                    None => Type::Void,
                };

                self.ensure_assignable(expected_return, &actual_type, "return value")?;
            }

            Stmt::Stop(value, _) => {
                if loop_depth == 0 {
                    return Err(self.error("'stop' can only be used inside a loop", None));
                }

                if let Some(value) = value {
                    self.type_of_expression(value, scopes, program, functions)?;
                }
            }

            Stmt::Skip(_) => {
                if loop_depth == 0 {
                    return Err(self.error("'skip' can only be used inside a loop", None));
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

                self.require_type(&Type::Bool, &condition_type, "if condition")?;

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
                span: _,
            } => {
                self.analyze_loop(
                    kind,
                    binding.as_deref(),
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
                    self.check_match_pattern(&arm.pattern, &expression_type)?;

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

                self.require_type(&Type::Int, &ty, "repeat count")?;
            }

            LoopKind::While(condition) => {
                let ty = self.type_of_expression(condition, scopes, program, functions)?;

                self.require_type(&Type::Bool, &ty, "while condition")?;
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
                    None,
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
                        None,
                    ));
                }

                Err(self.error(format!("use of undefined variable '{}'", name), Some(*span)))
            }

            Expr::StructLiteral {
                name,
                fields,
                span: _,
            } => self.type_of_struct_literal(name, fields, program, scopes, functions),

            Expr::Unary {
                operator,
                operand,
                span: _,
            } => {
                let operand_type = self.type_of_expression(operand, scopes, program, functions)?;

                match operator {
                    UnaryOp::Negate => {
                        if !self.is_numeric(&operand_type) {
                            return Err(self.error(
                                format!("cannot negate {}", self.type_name(&operand_type)),
                                None,
                            ));
                        }

                        Ok(operand_type)
                    }
                }
            }

            Expr::Binary {
                left,
                operator,
                right,
                span: _,
            } => self.type_of_binary(left, *operator, right, scopes, program, functions),

            Expr::Call {
                callee,
                arguments,
                span: _,
            } => self.type_of_call(callee, arguments, scopes, program, functions),

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
                span: _,
            } => {
                let function = functions
                    .get(name)
                    .ok_or_else(|| self.error(format!("unknown intrinsic '{}'", name), None))?;

                if function.intrinsic.is_none() {
                    return Err(self.error(format!("'{}' is not an intrinsic", name), None));
                }

                if arguments.len() != function.params.len() {
                    return Err(self.error(
                        format!(
                            "intrinsic '{}' expects {} argument(s), but {} were provided",
                            name,
                            function.params.len(),
                            arguments.len()
                        ),
                        None,
                    ));
                }

                for (index, argument) in arguments.iter().enumerate() {
                    let actual = self.type_of_expression(argument, scopes, program, functions)?;

                    self.ensure_assignable(
                        &function.params[index],
                        &actual,
                        &format!("argument {} of intrinsic '{}'", index + 1, name),
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
        program: &Program,
        scopes: &mut Vec<HashMap<String, Variable>>,
        functions: &HashMap<String, FunctionInfo>,
    ) -> Result<Type, AlacoError> {
        let structure = self
            .find_struct(program, name)
            .ok_or_else(|| self.error(format!("unknown struct '{}'", name), None))?;

        let mut supplied = HashSet::new();

        for (field_name, value) in fields {
            if !supplied.insert(field_name.as_str()) {
                return Err(self.error(
                    format!("field '{}' is specified more than once", field_name),
                    None,
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
                    self.error(message, None)
                })?;

            let actual_type = self.type_of_expression(value, scopes, program, functions)?;

            self.ensure_assignable(
                &field.ty,
                &actual_type,
                &format!("field '{}.{}'", name, field_name),
            )?;
        }

        for field in &structure.fields {
            if !supplied.contains(field.name.as_str()) {
                return Err(self.error(
                    format!(
                        "missing field '{}' in struct literal '{}'",
                        field.name, name
                    ),
                    None,
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
                    .ok_or_else(|| self.error(format!("unknown type '{}'", name), None))?;

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
                    Err(self.error(format!("type 'Shell' has no member '{}'", member), None))
                }
            }

            Type::ShellResult => match member {
                "output" => Ok(Type::String),
                "status" => Ok(Type::Int),
                "success" => Ok(Type::Bool),
                _ => Err(self.error(
                    format!("type 'ShellResult' has no member '{}'", member),
                    None,
                )),
            },

            _ => Err(self.error(
                format!("type '{}' has no members", self.type_name(object_type)),
                None,
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
            let member_type = self.type_of_member(&object_type, member, program, *span)?;

            for argument in arguments {
                self.type_of_expression(argument, scopes, program, functions)?;
            }

            return Ok(member_type);
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
                self.check_assignment_target(left, scopes)?;

                if matches!(
                    operator,
                    BinaryOp::AddAssign
                        | BinaryOp::SubtractAssign
                        | BinaryOp::MultiplyAssign
                        | BinaryOp::DivideAssign
                ) {
                    self.require_numeric_pair(&left_type, &right_type)?;
                }

                self.ensure_assignable(&left_type, &right_type, "assignment")?;

                Ok(left_type)
            }

            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo
            | BinaryOp::And
            | BinaryOp::Or => {
                if operator == BinaryOp::Add
                    && left_type == Type::String
                    && right_type == Type::String
                {
                    return Ok(Type::String);
                }

                self.require_numeric_pair(&left_type, &right_type)?;

                if left_type == Type::Float || right_type == Type::Float {
                    Ok(Type::Float)
                } else {
                    Ok(Type::Int)
                }
            }

            BinaryOp::Equal | BinaryOp::NotEqual => {
                if !self.types_compatible(&left_type, &right_type) {
                    return Err(self.error(
                        format!(
                            "cannot compare {} and {}",
                            self.type_name(&left_type),
                            self.type_name(&right_type)
                        ),
                        None,
                    ));
                }

                Ok(Type::Bool)
            }

            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                self.require_numeric_pair(&left_type, &right_type)?;

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
    ) -> Result<(), AlacoError> {
        match expression {
            Expr::Identifier(name, _) => {
                let Some(variable) = self.find_variable(scopes, name) else {
                    return Err(self.error(
                        format!("cannot assign to undefined variable '{}'", name),
                        None,
                    ));
                };

                if !variable.mutable {
                    return Err(self.error(
                        format!("cannot assign to immutable variable '{}'", name),
                        None,
                    ));
                }
            }

            Expr::Member { object, .. } => {
                self.check_member_assignment_target(object, scopes)?;
            }

            _ => {
                return Err(self.error("invalid assignment target", None));
            }
        }

        Ok(())
    }

    fn check_member_assignment_target(
        &self,
        object: &Expr,
        scopes: &mut Vec<HashMap<String, Variable>>,
    ) -> Result<(), AlacoError> {
        match object {
            Expr::Identifier(name, _) => {
                let Some(variable) = self.find_variable(scopes, name) else {
                    return Err(self.error(
                        format!("cannot assign through undefined variable '{}'", name),
                        None,
                    ));
                };

                if !variable.mutable {
                    return Err(self.error(
                        format!("cannot modify field through immutable variable '{}'", name),
                        None,
                    ));
                }

                Ok(())
            }

            Expr::Member { object, .. } => self.check_member_assignment_target(object, scopes),

            _ => Ok(()),
        }
    }

    // ------------------------------------------------------------
    // Match
    // ------------------------------------------------------------

    fn check_match_pattern(
        &self,
        pattern: &MatchPattern,
        expression_type: &Type,
    ) -> Result<(), AlacoError> {
        match pattern {
            MatchPattern::Number(_) => {
                if !self.is_numeric(expression_type) {
                    return Err(self.error(
                        format!(
                            "numeric match pattern cannot match {}",
                            self.type_name(expression_type)
                        ),
                        None,
                    ));
                }
            }

            MatchPattern::String(_) => {
                self.require_type(&Type::String, expression_type, "string match pattern")?;
            }

            MatchPattern::Bool(_) => {
                self.require_type(&Type::Bool, expression_type, "boolean match pattern")?;
            }

            MatchPattern::Identifier(_) | MatchPattern::Wildcard => {}
        }

        Ok(())
    }

    // ------------------------------------------------------------
    // Type utilities
    // ------------------------------------------------------------

    fn ensure_assignable(
        &self,
        expected: &Type,
        actual: &Type,
        context: &str,
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
                None,
            ))
        }
    }

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

    fn require_type(
        &self,
        expected: &Type,
        actual: &Type,
        context: &str,
    ) -> Result<(), AlacoError> {
        self.ensure_assignable(expected, actual, context)
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

    fn require_numeric_pair(&self, left: &Type, right: &Type) -> Result<(), AlacoError> {
        if self.is_numeric(left) && self.is_numeric(right) {
            Ok(())
        } else {
            Err(self.error(
                format!(
                    "numeric operation requires numeric values, found {} and {}",
                    self.type_name(left),
                    self.type_name(right)
                ),
                None,
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
