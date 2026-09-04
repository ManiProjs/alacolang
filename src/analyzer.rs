use std::collections::{HashMap, HashSet};

use crate::{
    ast::{
        Block, Expr, Function, Item, LoopKind, MatchPattern, Program, Stmt, Struct, Type, UnaryOp,
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

        let functions = self.collect_functions(program)?;

        for item in &program.items {
            if let Item::Function(function) = item {
                self.analyze_function(function, program, &functions)?;
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

        // Collect struct names first so functions can reference them.
        for item in &program.items {
            if let Item::Struct(structure) = item {
                if !structs.insert(structure.name.as_str()) {
                    return Err(self.error(format!("duplicate struct '{}'", structure.name)));
                }
            }
        }

        for item in &program.items {
            match item {
                Item::Import(_) => {}

                Item::Struct(_) => {}

                Item::Function(function) => {
                    if !functions.insert(function.name.as_str()) {
                        return Err(self.error(format!("duplicate function '{}'", function.name)));
                    }

                    if function.name == "main" {
                        main_count += 1;

                        if !function.params.is_empty() {
                            return Err(self.error("main function cannot have parameters"));
                        }

                        if function.return_type.is_some() {
                            return Err(
                                self.error("main function cannot have an explicit return type")
                            );
                        }
                    }

                    self.check_parameters(function, &structs)?;
                }

                Item::Statement(_) => {
                    return Err(
                        self.error("top-level statements are not allowed; expected a function")
                    );
                }
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
                    return Err(self.error(format!(
                        "duplicate field '{}' in struct '{}'",
                        field.name, structure.name
                    )));
                }

                // Struct fields cannot be Void.
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
                return Err(self.error(format!(
                    "duplicate parameter '{}' in function '{}'",
                    parameter.name, function.name
                )));
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
            Type::Int | Type::Float | Type::Bool | Type::String | Type::Void => Ok(()),

            Type::Void => {
                Err(self.error("Void cannot be used as a variable, field, or parameter type"))
            }

            Type::Named(name) => {
                if struct_names.contains(name.as_str()) {
                    Ok(())
                } else {
                    Err(self.error(format!("unknown type '{}'", name)))
                }
            }
        }
    }

    fn validate_return_type(
        &self,
        ty: &Type,
        struct_names: &HashSet<&str>,
    ) -> Result<(), AlacoError> {
        match ty {
            Type::Void | Type::Int | Type::Float | Type::Bool | Type::String => Ok(()),

            Type::Named(name) => {
                if struct_names.contains(name.as_str()) {
                    Ok(())
                } else {
                    Err(self.error(format!("unknown type '{}'", name)))
                }
            }
        }
    }

    fn validate_value_type(
        &self,
        ty: &Type,
        struct_names: &HashSet<&str>,
    ) -> Result<(), AlacoError> {
        match ty {
            Type::Void => {
                Err(self
                    .error("Void cannot be used as a variable, parameter, or struct field type"))
            }

            _ => self.validate_type(ty, struct_names),
        }
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
    ) -> Result<HashMap<String, FunctionInfo>, AlacoError> {
        let mut functions = HashMap::new();

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
                    return Err(self.error(format!(
                        "variable '{}' is already declared in this scope",
                        name
                    )));
                }

                scope.insert(
                    name.clone(),
                    Variable {
                        ty: final_type,
                        mutable: *mutable,
                    },
                );
            }

            Stmt::Return(value) => {
                let actual_type = match value {
                    Some(value) => self.type_of_expression(value, scopes, program, functions)?,

                    None => Type::Void,
                };

                self.ensure_assignable(expected_return, &actual_type, "return value")?;
            }

            Stmt::Stop(value) => {
                if loop_depth == 0 {
                    return Err(self.error("'stop' can only be used inside a loop"));
                }

                if let Some(value) = value {
                    self.type_of_expression(value, scopes, program, functions)?;
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

            Stmt::Expr(expression) => {
                self.type_of_expression(expression, scopes, program, functions)?;
            }

            Stmt::Match { expression, arms } => {
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

                return Err(self.error(format!(
                    "'for' loops cannot iterate over '{}' yet",
                    self.type_name(&iterable_type)
                )));
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
            Expr::Number(value) => {
                if value.contains('.') {
                    Ok(Type::Float)
                } else {
                    Ok(Type::Int)
                }
            }

            Expr::String(_) => Ok(Type::String),

            Expr::Bool(_) => Ok(Type::Bool),

            Expr::Identifier(name) => {
                if self.is_builtin(name) {
                    return Err(
                        self.error(format!("'{}' is a builtin function, not a value", name))
                    );
                }

                if let Some(variable) = self.find_variable(scopes, name) {
                    return Ok(variable.ty.clone());
                }

                if functions.contains_key(name) {
                    return Err(
                        self.error(format!("function '{}' cannot be used as a value", name))
                    );
                }

                Err(self.error(format!("use of undefined variable '{}'", name)))
            }

            Expr::StructLiteral { name, fields } => {
                self.type_of_struct_literal(name, fields, program, scopes, functions)
            }

            Expr::Unary { operator, operand } => {
                let operand_type = self.type_of_expression(operand, scopes, program, functions)?;

                match operator {
                    UnaryOp::Negate => {
                        if !self.is_numeric(&operand_type) {
                            return Err(self.error(format!(
                                "cannot negate {}",
                                self.type_name(&operand_type)
                            )));
                        }

                        Ok(operand_type)
                    }
                }
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => self.type_of_binary(left, *operator, right, scopes, program, functions),

            Expr::Call { callee, arguments } => {
                self.type_of_call(callee, arguments, scopes, program, functions)
            }

            Expr::Member { object, member } => {
                let object_type = self.type_of_expression(object, scopes, program, functions)?;

                self.type_of_member(&object_type, member, program)
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
            .ok_or_else(|| self.error(format!("unknown struct '{}'", name)))?;

        let mut supplied = HashSet::new();

        for (field_name, value) in fields {
            if !supplied.insert(field_name.as_str()) {
                return Err(self.error(format!(
                    "field '{}' is specified more than once",
                    field_name
                )));
            }

            let field = structure
                .fields
                .iter()
                .find(|field| field.name == *field_name)
                .ok_or_else(|| {
                    self.error(format!("struct '{}' has no field '{}'", name, field_name))
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
                return Err(self.error(format!(
                    "missing field '{}' in struct literal '{}'",
                    field.name, name
                )));
            }
        }

        Ok(Type::Named(name.to_string()))
    }

    fn type_of_member(
        &self,
        object_type: &Type,
        member: &str,
        program: &Program,
    ) -> Result<Type, AlacoError> {
        match object_type {
            Type::Named(name) => {
                let structure = self
                    .find_struct(program, name)
                    .ok_or_else(|| self.error(format!("unknown type '{}'", name)))?;

                let field = structure
                    .fields
                    .iter()
                    .find(|field| field.name == member)
                    .ok_or_else(|| {
                        self.error(format!("struct '{}' has no field '{}'", name, member))
                    })?;

                Ok(field.ty.clone())
            }

            _ => Err(self.error(format!(
                "type '{}' has no members",
                self.type_name(object_type)
            ))),
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
        if let Expr::Identifier(name) = callee {
            if name == "print" {
                for argument in arguments {
                    self.type_of_expression(argument, scopes, program, functions)?;
                }

                return Ok(Type::Void);
            }

            let function = functions
                .get(name)
                .ok_or_else(|| self.error(format!("unknown function '{}'", name)))?;

            if arguments.len() != function.params.len() {
                return Err(self.error(format!(
                    "function '{}' expects {} argument(s), but {} were provided",
                    name,
                    function.params.len(),
                    arguments.len()
                )));
            }

            for (index, argument) in arguments.iter().enumerate() {
                let actual = self.type_of_expression(argument, scopes, program, functions)?;

                self.ensure_assignable(
                    &function.params[index],
                    &actual,
                    &format!("argument {} of function '{}'", index + 1, name),
                )?;
            }

            return Ok(function.return_type.clone());
        }

        if let Expr::Member { object, member } = callee {
            if let Expr::Identifier(alias) = object.as_ref() {
                if let Some(module) = self.find_module(alias) {
                    if !Self::module_has_function(module, member) {
                        return Err(
                            self.error(format!("module '{}' has no function '{}'", alias, member))
                        );
                    }

                    for argument in arguments {
                        self.type_of_expression(argument, scopes, program, functions)?;
                    }

                    return Ok(Type::Void);
                }
            }
        }

        let callee_type = self.type_of_expression(callee, scopes, program, functions)?;

        Err(self.error(format!(
            "cannot call value of type '{}'",
            self.type_name(&callee_type)
        )))
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
            | BinaryOp::Modulo => {
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
                    return Err(self.error(format!(
                        "cannot compare {} and {}",
                        self.type_name(&left_type),
                        self.type_name(&right_type)
                    )));
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
                self.check_member_assignment_target(object, scopes)?;
            }

            _ => {
                return Err(self.error("invalid assignment target"));
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
            Expr::Identifier(name) => {
                let Some(variable) = self.find_variable(scopes, name) else {
                    return Err(self.error(format!(
                        "cannot assign through undefined variable '{}'",
                        name
                    )));
                };

                if !variable.mutable {
                    return Err(self.error(format!(
                        "cannot modify field through immutable variable '{}'",
                        name
                    )));
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
                    return Err(self.error(format!(
                        "numeric match pattern cannot match {}",
                        self.type_name(expression_type)
                    )));
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
            Err(self.error(format!(
                "type mismatch for {}: expected {}, found {}",
                context,
                self.type_name(expected),
                self.type_name(actual)
            )))
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
            Err(self.error(format!(
                "numeric operation requires numeric values, found {} and {}",
                self.type_name(left),
                self.type_name(right)
            )))
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

    fn error(&self, message: impl Into<String>) -> AlacoError {
        AlacoError::Analysis {
            message: message.into(),
            span: None,
        }
    }
}
