use crate::{
    ast::{
        Block, Expr, Function, Import, Item, LoopKind, MatchArm, MatchPattern, Param, Program,
        Stmt, Struct, Type, UnaryOp,
    },
    language::BinaryOp,
};

pub struct CppCodegen {
    output: String,
    indent: usize,
}

impl CppCodegen {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    pub fn generate(mut self, program: &Program) -> String {
        self.generate_header();
        self.generate_runtime();

        for item in &program.items {
            self.generate_item(item);
        }

        self.output
    }

    pub fn generate_with_modules(
        mut self,
        program: &Program,
        modules: &[crate::stdlib::LoadedModule],
    ) -> String {
        self.generate_header();

        self.generate_runtime();

        for module in modules {
            for item in &module.program.items {
                self.generate_item(item);
            }
        }

        for item in &program.items {
            self.generate_item(item);
        }

        self.output
    }

    fn generate_header(&mut self) {
        self.push_line("#include <algorithm>");
        self.push_line("#include <chrono>");
        self.push_line("#include <cstdio>");
        self.push_line("#include <cstdlib>");
        self.push_line("#include <future>");
        self.push_line("#include <iostream>");
        self.push_line("#include <sstream>");
        self.push_line("#include <string>");
        self.push_line("#include <thread>");
        self.push_line("#include <utility>");
        self.push_line("#include <vector>");
        self.push_line("");
    }

    fn generate_runtime(&mut self) {
        self.push_line("// Alaco runtime");
        self.push_line("");

        self.push_line("static void print(const std::string& value) {");
        self.indent += 1;
        self.push_line("std::cout << value;");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line("struct ShellResult {");
        self.indent += 1;
        self.push_line("int status;");
        self.push_line("std::string output;");
        self.push_line("bool success;");
        self.indent -= 1;
        self.push_line("};");
        self.push_line("");

        self.push_line("struct Shell {");
        self.indent += 1;

        self.push_line("std::future<ShellResult> task;");
        self.push_line("");

        self.push_line("explicit Shell(std::future<ShellResult>&& task)");
        self.indent += 1;
        self.push_line(": task(std::move(task)) {}");
        self.indent -= 1;
        self.push_line("");

        self.push_line("ShellResult get() {");
        self.indent += 1;
        self.push_line("return task.get();");
        self.indent -= 1;
        self.push_line("}");

        self.indent -= 1;
        self.push_line("};");
        self.push_line("");

        self.push_line("static ShellResult shell_run(const std::string& command) {");
        self.indent += 1;

        self.push_line("std::string output;");
        self.push_line("std::string full_command = command + \" 2>&1\";");
        self.push_line("");

        self.push_line("FILE* pipe = popen(full_command.c_str(), \"r\");");
        self.push_line("");

        self.push_line("if (!pipe) {");
        self.indent += 1;
        self.push_line("return {-1, \"failed to start shell command\", false};");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line("char buffer[4096];");
        self.push_line("");

        self.push_line("while (fgets(buffer, sizeof(buffer), pipe)) {");
        self.indent += 1;
        self.push_line("output += buffer;");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line("int status = pclose(pipe);");
        self.push_line("");

        self.push_line("return {status, output, status == 0};");

        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        /*
         * IMPORTANT:
         *
         * Creating a Shell starts the asynchronous operation.
         * Calling `.get()` waits for it and retrieves the result.
         *
         * This means:
         *
         *     let result = `ls`.get()
         *
         * becomes:
         *
         *     auto result = shell_async("ls").get();
         */
        self.push_line("static Shell shell_async(const std::string& command) {");
        self.indent += 1;
        self.push_line("return Shell(std::async(std::launch::async, shell_run, command));");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line("// End Alaco runtime");
        self.push_line("");
    }

    fn generate_item(&mut self, item: &Item) {
        match item {
            Item::Import(import) => self.generate_import(import),
            Item::Struct(struct_) => self.generate_struct(struct_),
            Item::Function(function) => self.generate_function(function),
            Item::Statement(statement) => {
                self.generate_statement(statement);
            }
        }
    }

    fn generate_import(&mut self, _import: &Import) {
        // Imports are resolved by the Alaco compiler before C++ generation.
    }

    fn generate_struct(&mut self, struct_: &Struct) {
        self.push_line(&format!("struct {} {{", self.cpp_identifier(&struct_.name)));

        self.indent += 1;

        for field in &struct_.fields {
            self.push_line(&format!(
                "{} {};",
                self.type_name(&field.ty),
                self.cpp_identifier(&field.name)
            ));
        }

        self.indent -= 1;

        self.push_line("};");
        self.push_line("");
    }

    fn generate_function(&mut self, function: &Function) {
        let return_type = if function.name == "main" {
            "int".to_string()
        } else {
            function
                .return_type
                .as_ref()
                .map(|ty| self.type_name(ty))
                .unwrap_or_else(|| "void".to_string())
        };

        let name = self.cpp_identifier(&function.name);

        let params = function
            .params
            .iter()
            .map(|param| self.generate_param(param))
            .collect::<Vec<_>>()
            .join(", ");

        self.push_line(&format!("{} {}({}) {{", return_type, name, params));

        self.indent += 1;

        self.generate_block_contents(&function.body);

        self.indent -= 1;

        self.push_line("}");
        self.push_line("");
    }

    fn generate_param(&self, param: &Param) -> String {
        format!(
            "{} {}",
            self.type_name(&param.ty),
            self.cpp_identifier(&param.name)
        )
    }

    fn generate_block_contents(&mut self, block: &Block) {
        for statement in &block.statements {
            self.generate_statement(statement);
        }
    }

    fn generate_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let {
                name,
                mutable: _,
                ty,
                value,
                ..
            } => {
                let cpp_name = self.cpp_identifier(name);
                let expression = self.expression(value);

                if let Some(ty) = ty {
                    self.push_line(&format!(
                        "{} {} = {};",
                        self.type_name(ty),
                        cpp_name,
                        expression
                    ));
                } else {
                    self.push_line(&format!("auto {} = {};", cpp_name, expression));
                }
            }

            Stmt::Return(value, ..) => {
                if let Some(value) = value {
                    self.push_line(&format!("return {};", self.expression(value)));
                } else {
                    self.push_line("return;");
                }
            }

            Stmt::Stop(value, ..) => {
                if let Some(value) = value {
                    self.push_line(&format!("return {};", self.expression(value)));
                } else {
                    self.push_line("return;");
                }
            }

            Stmt::Skip(..) => {
                self.push_line("continue;");
            }

            Stmt::Expr(expression, ..) => {
                self.push_line(&format!("{};", self.expression(expression)));
            }

            Stmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.push_line(&format!("if ({}) {{", self.expression(condition)));

                self.indent += 1;
                self.generate_block_contents(then_block);
                self.indent -= 1;

                if let Some(else_block) = else_block {
                    self.push_line("} else {");

                    self.indent += 1;
                    self.generate_block_contents(else_block);
                    self.indent -= 1;
                }

                self.push_line("}");
            }

            Stmt::Loop {
                kind,
                binding,
                body,
                ..
            } => {
                self.generate_loop(kind, binding.as_deref(), body);
            }

            Stmt::Match {
                expression, arms, ..
            } => {
                self.generate_match(expression, arms);
            }
        }
    }

    fn generate_loop(&mut self, kind: &LoopKind, _binding: Option<&str>, body: &Block) {
        match kind {
            LoopKind::Infinite { .. } => {
                self.push_line("while (true) {");

                self.indent += 1;
                self.generate_block_contents(body);
                self.indent -= 1;

                self.push_line("}");
            }

            LoopKind::Repeat(count, ..) => {
                let count = self.expression(count);

                self.push_line(&format!("for (long long _i = 0; _i < {}; ++_i) {{", count));

                self.indent += 1;
                self.generate_block_contents(body);
                self.indent -= 1;

                self.push_line("}");
            }

            LoopKind::While(condition, ..) => {
                self.push_line(&format!("while ({}) {{", self.expression(condition)));

                self.indent += 1;
                self.generate_block_contents(body);
                self.indent -= 1;

                self.push_line("}");
            }

            LoopKind::For {
                variable, iterable, ..
            } => {
                let variable = self.cpp_identifier(variable);
                let iterable = self.expression(iterable);

                self.push_line(&format!("for (auto {} : {}) {{", variable, iterable));

                self.indent += 1;
                self.generate_block_contents(body);
                self.indent -= 1;

                self.push_line("}");
            }
        }
    }

    fn generate_match(&mut self, expression: &Expr, arms: &[MatchArm]) {
        let expression = self.expression(expression);
        let temp = "_match_value";

        self.push_line(&format!("auto {} = {};", temp, expression));

        for (index, arm) in arms.iter().enumerate() {
            let condition = self.match_condition(temp, &arm.pattern);

            if index == 0 {
                if let Some(condition) = condition {
                    self.push_line(&format!("if ({}) {{", condition));
                } else {
                    self.push_line("{");
                }
            } else if let Some(condition) = condition {
                self.push_line(&format!("}} else if ({}) {{", condition));
            } else {
                self.push_line("} else {");
            }

            self.indent += 1;
            self.generate_block_contents(&arm.body);
            self.indent -= 1;
        }

        if !arms.is_empty() {
            self.push_line("}");
        }
    }

    fn match_condition(&self, value: &str, pattern: &MatchPattern) -> Option<String> {
        match pattern {
            MatchPattern::Number(number, ..) => Some(format!("{} == {}", value, number)),

            MatchPattern::String(string, ..) => {
                Some(format!("{} == {}", value, self.cpp_string_literal(string)))
            }

            MatchPattern::Bool(value_bool, ..) => Some(format!(
                "{} == {}",
                value,
                if *value_bool { "true" } else { "false" }
            )),

            MatchPattern::Identifier(identifier, ..) => {
                Some(format!("{} == {}", value, self.cpp_identifier(identifier)))
            }

            MatchPattern::Wildcard => None,
        }
    }

    fn expression(&self, expression: &Expr) -> String {
        match expression {
            Expr::Number(value, ..) => value.clone(),

            Expr::String(value, ..) => self.cpp_string_literal(value),

            Expr::Bool(value, ..) => {
                if *value {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }

            Expr::Identifier(name, ..) => self.cpp_identifier(name),

            Expr::StructLiteral { name, fields, .. } => {
                let values = fields
                    .iter()
                    .map(|(_, value)| self.expression(value))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}{{{}}}", self.cpp_identifier(name), values)
            }

            Expr::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let left = self.expression(left);
                let right = self.expression(right);

                format!("({} {} {})", left, self.binary_operator(*operator), right)
            }

            Expr::Unary {
                operator, operand, ..
            } => {
                let operand = self.expression(operand);

                match operator {
                    UnaryOp::Negate => {
                        format!("(-{})", operand)
                    }
                }
            }

            Expr::Call {
                callee, arguments, ..
            } => {
                let callee = self.expression(callee);

                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}({})", callee, arguments)
            }

            Expr::Member { object, member, .. } => {
                let object = self.expression(object);

                format!("{}.{}", object, self.cpp_identifier(member))
            }

            /*
             * Shell expressions create a Shell.
             *
             * They DO NOT call `.get()` automatically.
             *
             * Alaco:
             *
             *     `ls`
             *
             * C++:
             *
             *     shell_async("ls")
             *
             * Alaco:
             *
             *     `ls`.get()
             *
             * should ultimately become:
             *
             *     shell_async("ls").get()
             */
            Expr::Shell { command, .. } => {
                format!("shell_async({})", self.cpp_string_literal(command))
            }
        }
    }

    fn binary_operator(&self, operator: BinaryOp) -> &'static str {
        match operator {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Modulo => "%",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::Assign => "=",
            BinaryOp::AddAssign => "+=",
            BinaryOp::SubtractAssign => "-=",
            BinaryOp::MultiplyAssign => "*=",
            BinaryOp::DivideAssign => "/=",
        }
    }

    fn type_name(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "long long".to_string(),
            Type::Float => "double".to_string(),
            Type::Bool => "bool".to_string(),
            Type::String => "std::string".to_string(),
            Type::Shell => "Shell".to_string(),
            Type::ShellResult => "ShellResult".to_string(),
            Type::Void => "void".to_string(),

            Type::Named(name) => self.cpp_identifier(name),
        }
    }

    fn cpp_string_literal(&self, value: &str) -> String {
        let mut result = String::with_capacity(value.len() + 2);

        result.push('"');

        for character in value.chars() {
            match character {
                '\\' => result.push_str("\\\\"),
                '"' => result.push_str("\\\""),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                '\0' => result.push_str("\\0"),
                character => result.push(character),
            }
        }

        result.push('"');

        result
    }

    fn cpp_identifier(&self, name: &str) -> String {
        match name {
            "alignas" | "alignof" | "and" | "and_eq" | "asm" | "atomic_cancel"
            | "atomic_commit" | "atomic_noexcept" | "auto" | "bitand" | "bitor" | "bool"
            | "break" | "case" | "catch" | "char" | "char8_t" | "char16_t" | "char32_t"
            | "class" | "compl" | "concept" | "const" | "consteval" | "constexpr" | "constinit"
            | "const_cast" | "continue" | "co_await" | "co_return" | "co_yield" | "decltype"
            | "default" | "delete" | "do" | "double" | "dynamic_cast" | "else" | "enum"
            | "explicit" | "export" | "extern" | "false" | "float" | "for" | "friend" | "goto"
            | "if" | "inline" | "int" | "long" | "mutable" | "namespace" | "new" | "noexcept"
            | "not" | "not_eq" | "nullptr" | "operator" | "or" | "or_eq" | "private"
            | "protected" | "public" | "reflexpr" | "register" | "reinterpret_cast"
            | "requires" | "return" | "short" | "signed" | "sizeof" | "static"
            | "static_assert" | "static_cast" | "struct" | "switch" | "synchronized"
            | "template" | "this" | "thread_local" | "throw" | "true" | "try" | "typedef"
            | "typeid" | "typename" | "union" | "unsigned" | "using" | "virtual" | "void"
            | "volatile" | "wchar_t" | "while" | "xor" | "xor_eq" => {
                format!("{}_alaco", name)
            }

            _ => name.to_string(),
        }
    }

    fn push_line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }

        self.output.push_str(line);
        self.output.push('\n');
    }
}

impl Default for CppCodegen {
    fn default() -> Self {
        Self::new()
    }
}
