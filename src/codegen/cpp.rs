use crate::{
    ast::{
        Block, Expr, Function, Item, LoopKind, MatchArm, MatchPattern, Program, Stmt, Type, UnaryOp,
    },
    stdlib::LoadedModule,
};

pub struct CppGenerator {
    output: String,
    indent: usize,
    modules: Vec<LoadedModule>,
}

impl CppGenerator {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            modules: Vec::new(),
        }
    }

    pub fn generate(&mut self, program: &Program) -> String {
        self.output.clear();
        self.indent = 0;
        self.modules.clear();

        self.generate_header();

        for item in &program.items {
            self.item(item);
        }

        std::mem::take(&mut self.output)
    }

    pub fn generate_with_modules(&mut self, program: &Program, modules: &[LoadedModule]) -> String {
        self.output.clear();
        self.indent = 0;
        self.modules = modules.to_vec();

        self.generate_header();

        for module in modules {
            self.module(module);
        }

        for item in &program.items {
            self.item(item);
        }

        std::mem::take(&mut self.output)
    }

    fn generate_header(&mut self) {
        self.line("#include <iostream>");
        self.line("#include <string>");
        self.line("#include <vector>");
        self.line("");
    }

    fn module(&mut self, module: &LoadedModule) {
        self.line(&format!("namespace {} {{", module.alias));
        self.indent += 1;

        for item in &module.program.items {
            match item {
                Item::Function(function) => {
                    self.function(function);
                    self.line("");
                }

                Item::Struct(struct_def) => {
                    self.struct_definition(struct_def);
                    self.line("");
                }

                Item::Import(_) | Item::Statement(_) => {}
            }
        }

        self.indent -= 1;
        self.line("}");
        self.line("");
    }

    fn item(&mut self, item: &Item) {
        match item {
            Item::Import(_) => {}

            Item::Struct(struct_def) => {
                self.struct_definition(struct_def);
                self.line("");
            }

            Item::Function(function) => {
                self.function(function);
                self.line("");
            }

            Item::Statement(statement) => {
                self.statement(statement);
            }
        }
    }

    fn struct_definition(&mut self, struct_def: &crate::ast::Struct) {
        self.line(&format!("struct {} {{", struct_def.name));

        self.indent += 1;

        for field in &struct_def.fields {
            self.line(&format!("{} {};", Self::type_name(&field.ty), field.name));
        }

        self.indent -= 1;

        self.line("};");
    }

    fn function(&mut self, function: &Function) {
        let return_type = if function.name == "main" {
            "int".to_string()
        } else {
            function
                .return_type
                .as_ref()
                .map(Self::type_name)
                .unwrap_or_else(|| "void".to_string())
        };

        let params = function
            .params
            .iter()
            .map(|param| format!("{} {}", Self::type_name(&param.ty), param.name))
            .collect::<Vec<_>>()
            .join(", ");

        self.line(&format!("{} {}({}) {{", return_type, function.name, params));

        self.indent += 1;
        self.block(&function.body);
        self.indent -= 1;

        self.line("}");
    }

    fn block(&mut self, block: &Block) {
        for statement in &block.statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let value = self.expression(value);

                let type_name = ty
                    .as_ref()
                    .map(Self::type_name)
                    .unwrap_or_else(|| "auto".to_string());

                let qualifier = if *mutable { "" } else { "const " };

                self.line(&format!("{}{} {} = {};", qualifier, type_name, name, value));
            }

            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.line(&format!("return {};", self.expression(value)));
                } else {
                    self.line("return;");
                }
            }

            Stmt::Stop(value) => {
                if let Some(value) = value {
                    self.line(&format!("return {};", self.expression(value)));
                } else {
                    self.line("break;");
                }
            }

            Stmt::Skip => {
                self.line("continue;");
            }

            Stmt::Expr(expression) => {
                self.line(&format!("{};", self.expression(expression)));
            }

            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.line(&format!("if ({}) {{", self.expression(condition)));

                self.indent += 1;
                self.block(then_block);
                self.indent -= 1;

                if let Some(else_block) = else_block {
                    self.line("} else {");

                    self.indent += 1;
                    self.block(else_block);
                    self.indent -= 1;

                    self.line("}");
                } else {
                    self.line("}");
                }
            }

            Stmt::Loop {
                kind,
                binding,
                body,
            } => {
                self.loop_statement(kind, binding.as_deref(), body);
            }

            Stmt::Match { expression, arms } => {
                self.match_statement(expression, arms);
            }
        }
    }

    fn loop_statement(&mut self, kind: &LoopKind, binding: Option<&str>, body: &Block) {
        match kind {
            LoopKind::Infinite => {
                self.line("while (true) {");
            }

            LoopKind::Repeat(expression) => {
                let expression = self.expression(expression);

                self.line(&format!(
                    "for (long long _i = 0; _i < {}; ++_i) {{",
                    expression
                ));
            }

            LoopKind::While(condition) => {
                self.line(&format!("while ({}) {{", self.expression(condition)));
            }

            LoopKind::For { variable, iterable } => {
                self.line(&format!(
                    "for (auto {} : {}) {{",
                    variable,
                    self.expression(iterable)
                ));
            }
        }

        self.indent += 1;
        self.block(body);
        self.indent -= 1;

        self.line("}");
    }

    fn match_statement(&mut self, expression: &Expr, arms: &[MatchArm]) {
        let expression = self.expression(expression);

        for (index, arm) in arms.iter().enumerate() {
            match &arm.pattern {
                MatchPattern::Number(value) => {
                    self.match_condition(index, &format!("{} == {}", expression, value), &arm.body);
                }

                MatchPattern::String(value) => {
                    self.match_condition(
                        index,
                        &format!("{} == \"{}\"", expression, escape_cpp(value)),
                        &arm.body,
                    );
                }

                MatchPattern::Bool(value) => {
                    self.match_condition(index, &format!("{} == {}", expression, value), &arm.body);
                }

                MatchPattern::Identifier(name) => {
                    if index == 0 {
                        self.line("{");
                    } else {
                        self.line("else {");
                    }

                    self.indent += 1;

                    self.line(&format!("auto {} = {};", name, expression));

                    self.block(&arm.body);

                    self.indent -= 1;
                    self.line("}");

                    break;
                }

                MatchPattern::Wildcard => {
                    if index == 0 {
                        self.line("{");
                    } else {
                        self.line("else {");
                    }

                    self.indent += 1;
                    self.block(&arm.body);
                    self.indent -= 1;

                    self.line("}");

                    break;
                }
            }
        }
    }

    fn match_condition(&mut self, index: usize, condition: &str, body: &Block) {
        if index == 0 {
            self.line(&format!("if ({}) {{", condition));
        } else {
            self.line(&format!("else if ({}) {{", condition));
        }

        self.indent += 1;
        self.block(body);
        self.indent -= 1;

        self.line("}");
    }

    fn expression(&self, expression: &Expr) -> String {
        match expression {
            Expr::Number(value) => value.clone(),

            Expr::String(value) => {
                format!("\"{}\"", escape_cpp(value))
            }

            Expr::Bool(value) => value.to_string(),

            Expr::Identifier(name) => name.clone(),

            Expr::Unary { operator, operand } => {
                let operator = match operator {
                    UnaryOp::Negate => "-",
                };

                format!("{}{}", operator, self.expression(operand))
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                format!(
                    "({} {} {})",
                    self.expression(left),
                    operator.as_cpp(),
                    self.expression(right)
                )
            }

            Expr::Call { callee, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Vec<_>>()
                    .join(", ");

                if let Expr::Identifier(name) = callee.as_ref() {
                    if name == "print" {
                        return self.print_call(arguments);
                    }
                }

                if let Expr::Member { object, member } = callee.as_ref() {
                    if let Expr::Identifier(alias) = object.as_ref() {
                        if self.find_module(alias).is_some() {
                            return format!("{}::{}({})", alias, member, arguments);
                        }
                    }
                }

                format!("{}({})", self.expression(callee), arguments)
            }

            Expr::Member { object, member } => {
                format!("{}.{}", self.expression(object), member)
            }

            Expr::StructLiteral { name, fields } => {
                let values = fields
                    .iter()
                    .map(|(_, value)| self.expression(value))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}{{{}}}", name, values)
            }
        }
    }

    fn print_call(&self, arguments: String) -> String {
        format!("std::cout << {} << std::endl", arguments)
    }

    fn find_module(&self, alias: &str) -> Option<&LoadedModule> {
        self.modules.iter().find(|module| module.alias == alias)
    }

    fn line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }

        self.output.push_str(line);
        self.output.push('\n');
    }

    fn type_name(ty: &Type) -> String {
        match ty {
            Type::Int => "long long".to_string(),
            Type::Float => "double".to_string(),
            Type::Bool => "bool".to_string(),
            Type::String => "std::string".to_string(),
            Type::Named(name) => name.clone(),
        }
    }
}

fn escape_cpp(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
