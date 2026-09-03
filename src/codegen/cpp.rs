use crate::ast::*;

pub struct CppGenerator {
    output: String,
    indent: usize,
}

impl CppGenerator {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    pub fn generate(mut self, program: &Program) -> String {
        self.line("#include <iostream>");
        self.line("#include <string>");
        self.line("");

        for item in &program.items {
            self.item(item);
            self.line("");
        }

        self.output
    }

    fn item(&mut self, item: &Item) {
        match item {
            Item::Function(function) => self.function(function),
            Item::Statement(statement) => self.statement(statement),
        }
    }

    fn function(&mut self, function: &Function) {
        let return_type = if function.name == "main" {
            "int".to_string()
        } else {
            function
                .return_type
                .as_ref()
                .map(|ty| self.type_name(ty))
                .unwrap_or_else(|| "void".to_string())
        };

        let params = function
            .params
            .iter()
            .map(|param| format!("{} {}", self.type_name(&param.ty), param.name))
            .collect::<Vec<_>>()
            .join(", ");

        self.line(&format!("{} {}({}) {{", return_type, function.name, params));

        self.indent += 1;

        for statement in &function.body.statements {
            self.statement(statement);
        }

        // Alaco's `fn main()` always maps to C++ `int main()`.
        if function.name == "main" {
            self.line("return 0;");
        }

        self.indent -= 1;
        self.line("}");
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let cpp_type = ty.as_ref().map(|ty| self.type_name(ty));

                let prefix = if *mutable { "" } else { "const " };

                match cpp_type {
                    Some(ty) => {
                        self.line(&format!(
                            "{}{} {} = {};",
                            prefix,
                            ty,
                            name,
                            self.expression(value)
                        ));
                    }

                    None => {
                        self.line(&format!(
                            "{}auto {} = {};",
                            prefix,
                            name,
                            self.expression(value)
                        ));
                    }
                }
            }

            Stmt::Return(value) => match value {
                Some(value) => {
                    self.line(&format!("return {};", self.expression(value)));
                }

                None => {
                    self.line("return;");
                }
            },

            Stmt::Skip => {
                self.line("continue;");
            }

            Stmt::Stop(value) => match value {
                Some(value) => {
                    self.line(&format!("return {};", self.expression(value)));
                }

                None => {
                    self.line("break;");
                }
            },

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

                for statement in &then_block.statements {
                    self.statement(statement);
                }

                self.indent -= 1;

                if let Some(else_block) = else_block {
                    self.line("} else {");

                    self.indent += 1;

                    for statement in &else_block.statements {
                        self.statement(statement);
                    }

                    self.indent -= 1;
                }

                self.line("}");
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

                self.indent += 1;

                self.block(body);

                self.indent -= 1;

                self.line("}");
            }

            LoopKind::Repeat(expression) => {
                let variable = binding.unwrap_or("_i");

                self.line(&format!(
                    "for (long long {} = 0; {} < {}; ++{}) {{",
                    variable,
                    variable,
                    self.expression(expression),
                    variable
                ));

                self.indent += 1;

                self.block(body);

                self.indent -= 1;

                self.line("}");
            }

            LoopKind::While(condition) => {
                self.line(&format!("while ({}) {{", self.expression(condition)));

                self.indent += 1;

                self.block(body);

                self.indent -= 1;

                self.line("}");
            }

            LoopKind::For { variable, iterable } => {
                self.line(&format!(
                    "for (auto&& {} : {}) {{",
                    variable,
                    self.expression(iterable)
                ));

                self.indent += 1;

                self.block(body);

                self.indent -= 1;

                self.line("}");
            }
        }
    }

    fn match_statement(&mut self, expression: &Expr, arms: &[MatchArm]) {
        let expression = self.expression(expression);

        for (index, arm) in arms.iter().enumerate() {
            match &arm.pattern {
                MatchPattern::Wildcard => {
                    self.line("else {");

                    self.indent += 1;
                    self.block(&arm.body);
                    self.indent -= 1;

                    self.line("}");

                    // A wildcard catches everything, so later arms
                    // can never be reached.
                    break;
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

                    // An identifier pattern catches everything.
                    break;
                }

                pattern => {
                    let condition = match pattern {
                        MatchPattern::Number(value) => {
                            format!("{} == {}", expression, value)
                        }

                        MatchPattern::String(value) => {
                            format!("{} == \"{}\"", expression, escape_cpp(value))
                        }

                        MatchPattern::Bool(value) => {
                            format!("{} == {}", expression, value)
                        }

                        MatchPattern::Wildcard | MatchPattern::Identifier(_) => unreachable!(),
                    };

                    if index == 0 {
                        self.line(&format!("if ({condition}) {{"));
                    } else {
                        self.line(&format!("else if ({condition}) {{"));
                    }

                    self.indent += 1;
                    self.block(&arm.body);
                    self.indent -= 1;

                    self.line("}");
                }
            }
        }
    }

    fn block(&mut self, block: &Block) {
        for statement in &block.statements {
            self.statement(statement);
        }
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

                format!("({}{})", operator, self.expression(operand))
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let operator = operator.as_cpp();

                format!(
                    "({} {} {})",
                    self.expression(left),
                    operator,
                    self.expression(right)
                )
            }

            Expr::Call { callee, arguments } => {
                if let Expr::Identifier(name) = callee.as_ref() {
                    if name == "print" {
                        return self.print_call(arguments);
                    }
                }

                let callee = self.expression(callee);

                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}({})", callee, arguments)
            }

            Expr::Member { object, member } => {
                format!("{}.{}", self.expression(object), member)
            }
        }
    }

    fn print_call(&self, arguments: &[Expr]) -> String {
        if arguments.is_empty() {
            return "std::cout << std::endl".to_string();
        }

        let mut output = String::from("std::cout");

        for argument in arguments {
            match argument {
                Expr::String(value) if value.contains('{') => {
                    output.push_str(" << ");
                    output.push_str(&self.interpolate_string(value));
                }

                _ => {
                    output.push_str(" << ");
                    output.push_str(&self.expression(argument));
                }
            }
        }

        output.push_str(" << std::endl");

        output
    }

    fn interpolate_string(&self, value: &str) -> String {
        let mut result = String::new();
        let mut text = String::new();
        let chars: Vec<char> = value.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '{' {
                if !text.is_empty() {
                    result.push_str(&format!("\"{}\" << ", escape_cpp(&text)));

                    text.clear();
                }

                let start = i + 1;
                let mut end = start;

                while end < chars.len() && chars[end] != '}' {
                    end += 1;
                }

                if end < chars.len() {
                    let expression: String = chars[start..end].iter().collect();

                    result.push_str(&expression);
                    result.push_str(" << ");

                    i = end + 1;
                    continue;
                }
            }

            text.push(chars[i]);
            i += 1;
        }

        if !text.is_empty() {
            result.push_str(&format!("\"{}\"", escape_cpp(&text)));
        } else if result.ends_with(" << ") {
            result.truncate(result.len() - 4);
        }

        result
    }

    fn type_name(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "long long".into(),
            Type::Float => "double".into(),
            Type::Bool => "bool".into(),
            Type::String => "std::string".into(),
            Type::Named(name) => name.clone(),
        }
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }

        self.output.push_str(text);
        self.output.push('\n');
    }
}

fn escape_cpp(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
