use std::collections::HashSet;

use crate::{
    ast::{Type, UnaryOp},
    ir::{
        IrCallee, IrExpr, IrExprKind, IrFunction, IrItem, IrLoopKind, IrMatchPattern, IrPipeTarget,
        IrProgram, IrStmt, IrStruct,
    },
    language::BinaryOp,
};

#[derive(Debug, Default)]
struct RuntimeRequirements {
    print: bool,
    println: bool,
    eprint: bool,
    eprintln: bool,
    shell: bool,
    string: bool,
    fs: bool,
    env: bool,
    path: bool,
    json: bool,
    toml: bool,
    http: bool,
    time: bool,
    lifetime: bool,
}

impl RuntimeRequirements {
    fn needs_io(&self) -> bool {
        self.print || self.println || self.eprint || self.eprintln
    }

    fn needs_any_runtime(&self) -> bool {
        self.needs_io()
            || self.shell
            || self.fs
            || self.env
            || self.path
            || self.json
            || self.toml
            || self.http
            || self.time
            || self.lifetime
    }
}

pub struct CppCodegen {
    output: String,
    indent: usize,
    module_aliases: HashSet<String>,
    lifetime_variables: HashSet<String>,
}

impl CppCodegen {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            module_aliases: HashSet::new(),
            lifetime_variables: HashSet::new(),
        }
    }

    pub fn generate_with_modules(
        mut self,
        ir: &IrProgram,
        modules: &[crate::stdlib::LoadedModule],
    ) -> String {
        self.module_aliases = modules.iter().map(|module| module.alias.clone()).collect();
        let requirements = self.runtime_requirements(ir, modules);

        self.generate_header(&requirements);
        self.generate_runtime(&requirements);

        for structure in &ir.structs {
            self.generate_ir_struct(structure);
        }

        for module in modules {
            if module.name == "std.io" {
                // std.io is the default public API, so functions such as
                // print(...) remain available without a module qualifier.
                // The four print functions are emitted as C++ templates in
                // the runtime so they can accept any streamable value.
                let module_ir = module
                    .ir
                    .as_ref()
                    .expect("every loaded module must have validated IR");
                for item in &module_ir.items {
                    match item {
                        IrItem::Function(name)
                            if matches!(
                                name.as_str(),
                                "print" | "println" | "eprint" | "eprintln"
                            ) =>
                        {
                            continue;
                        }
                        IrItem::Function(name) => self.generate_ir_function(
                            module_ir
                                .function(name)
                                .expect("IR item must reference a function"),
                        ),
                        IrItem::Struct(name) => self.generate_ir_struct(
                            module_ir
                                .structs
                                .iter()
                                .find(|item| &item.name == name)
                                .expect("IR item must reference a struct"),
                        ),
                    }
                }
            } else {
                // Explicitly imported modules keep their namespace, matching
                // Alaco calls such as math.max(...).
                self.push_line(&format!(
                    "namespace {} {{",
                    self.cpp_identifier(&module.alias)
                ));
                self.indent += 1;

                let module_ir = module
                    .ir
                    .as_ref()
                    .expect("every loaded module must have validated IR");
                for item in &module_ir.items {
                    match item {
                        IrItem::Function(name) => self.generate_ir_function(
                            module_ir
                                .function(name)
                                .expect("IR item must reference a function"),
                        ),
                        IrItem::Struct(name) => self.generate_ir_struct(
                            module_ir
                                .structs
                                .iter()
                                .find(|item| &item.name == name)
                                .expect("IR item must reference a struct"),
                        ),
                    }
                }

                self.indent -= 1;
                self.push_line("}");
                self.push_line("");

                for symbol in &module.imported_symbols {
                    self.push_line(&format!(
                        "using {}::{};",
                        self.cpp_identifier(&module.alias),
                        self.cpp_identifier(symbol)
                    ));
                }
                if !module.imported_symbols.is_empty() {
                    self.push_line("");
                }
            }
        }

        for function in &ir.functions {
            self.generate_ir_function(function);
        }

        self.output
    }

    fn collect_lifetime_names(block: &crate::ir::IrBlock, names: &mut HashSet<String>) {
        for statement in &block.statements {
            match statement {
                IrStmt::Let {
                    name,
                    lifetime_ms: Some(_),
                    ..
                } => {
                    names.insert(name.clone());
                }
                IrStmt::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    Self::collect_lifetime_names(then_block, names);
                    if let Some(block) = else_block {
                        Self::collect_lifetime_names(block, names);
                    }
                }
                IrStmt::Loop { body, .. } => Self::collect_lifetime_names(body, names),
                IrStmt::Match { arms, .. } => {
                    for arm in arms {
                        Self::collect_lifetime_names(&arm.body, names);
                    }
                }
                _ => {}
            }
        }
    }

    // ============================================================
    // Runtime dependency detection
    // ============================================================

    fn runtime_requirements(
        &self,
        program: &IrProgram,
        modules: &[crate::stdlib::LoadedModule],
    ) -> RuntimeRequirements {
        let mut requirements = RuntimeRequirements::default();

        for module in modules {
            match module.name.as_str() {
                "std.fs" => requirements.fs = true,
                "std.env" => requirements.env = true,
                "std.path" => requirements.path = true,
                "std.json" => requirements.json = true,
                "std.toml" => requirements.toml = true,
                "std.http" => requirements.http = true,
                "std.timing" => requirements.time = true,
                _ => {}
            }
            if let Some(ir) = &module.ir {
                self.scan_ir_for_runtime(ir, &mut requirements);
            }
        }

        self.scan_ir_for_runtime(program, &mut requirements);

        requirements
    }

    fn scan_ir_for_runtime(&self, program: &IrProgram, requirements: &mut RuntimeRequirements) {
        for structure in &program.structs {
            for field in &structure.fields {
                self.scan_type_for_runtime(&field.ty, requirements);
            }
        }

        for function in &program.functions {
            self.scan_type_for_runtime(&function.return_type, requirements);
            for parameter in &function.parameters {
                self.scan_type_for_runtime(&parameter.ty, requirements);
            }
            self.scan_ir_block_for_runtime(&function.body, requirements);
        }
    }

    fn scan_type_for_runtime(&self, ty: &Type, requirements: &mut RuntimeRequirements) {
        match ty {
            Type::String => {
                requirements.string = true;
            }

            Type::Shell | Type::ShellResult => {
                requirements.shell = true;
            }

            _ => {}
        }
    }

    fn scan_ir_block_for_runtime(
        &self,
        block: &crate::ir::IrBlock,
        requirements: &mut RuntimeRequirements,
    ) {
        for statement in &block.statements {
            self.scan_ir_statement_for_runtime(statement, requirements);
        }
    }

    fn scan_ir_statement_for_runtime(
        &self,
        statement: &IrStmt,
        requirements: &mut RuntimeRequirements,
    ) {
        match statement {
            IrStmt::Let {
                ty,
                value,
                lifetime_ms,
                ..
            } => {
                if lifetime_ms.is_some() {
                    requirements.lifetime = true;
                }
                self.scan_type_for_runtime(ty, requirements);
                self.scan_expr_for_runtime(value, requirements);
            }

            IrStmt::Destructure { value, .. } => self.scan_expr_for_runtime(value, requirements),

            IrStmt::Return(value, ..) | IrStmt::Stop(value, ..) => {
                if let Some(value) = value {
                    self.scan_expr_for_runtime(value, requirements);
                }
            }

            IrStmt::Skip(..) => {}

            IrStmt::Expr(expression, ..) => {
                self.scan_expr_for_runtime(expression, requirements);
            }

            IrStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.scan_expr_for_runtime(condition, requirements);

                self.scan_ir_block_for_runtime(then_block, requirements);

                if let Some(else_block) = else_block {
                    self.scan_ir_block_for_runtime(else_block, requirements);
                }
            }

            IrStmt::Loop { kind, body, .. } => {
                match kind {
                    IrLoopKind::Infinite => {}

                    IrLoopKind::Repeat(expression) => {
                        self.scan_expr_for_runtime(expression, requirements);
                    }

                    IrLoopKind::While(expression) => {
                        self.scan_expr_for_runtime(expression, requirements);
                    }

                    IrLoopKind::For { iterable, .. } => {
                        self.scan_expr_for_runtime(iterable, requirements);
                    }
                }

                self.scan_ir_block_for_runtime(body, requirements);
            }

            IrStmt::Match {
                expression, arms, ..
            } => {
                self.scan_expr_for_runtime(expression, requirements);

                for arm in arms {
                    self.scan_ir_block_for_runtime(&arm.body, requirements);
                }
            }
        }
    }

    fn scan_expr_for_runtime(&self, expression: &IrExpr, requirements: &mut RuntimeRequirements) {
        self.scan_type_for_runtime(&expression.ty, requirements);
        match &expression.kind {
            IrExprKind::Shell { .. } => {
                requirements.shell = true;
            }

            IrExprKind::Identifier(..)
            | IrExprKind::Previous(..)
            | IrExprKind::Number(..)
            | IrExprKind::String(..)
            | IrExprKind::Interpolation(..)
            | IrExprKind::Bool(..) => {}

            IrExprKind::StructLiteral { fields, .. } => {
                for (_, value) in fields {
                    self.scan_expr_for_runtime(value, requirements);
                }
            }

            IrExprKind::Binary { left, right, .. } => {
                self.scan_expr_for_runtime(left, requirements);

                self.scan_expr_for_runtime(right, requirements);
            }

            IrExprKind::Pipe { input, target, .. } => {
                self.scan_expr_for_runtime(input, requirements);
                match target {
                    IrPipeTarget::Call { callee, arguments } => {
                        self.scan_callee_for_runtime(callee, requirements);
                        for argument in arguments {
                            self.scan_expr_for_runtime(argument, requirements);
                        }
                    }
                    IrPipeTarget::Callee(callee) => {
                        self.scan_callee_for_runtime(callee, requirements);
                    }
                    IrPipeTarget::Value(value) => self.scan_expr_for_runtime(value, requirements),
                }
            }

            IrExprKind::Unary { operand, .. } => {
                self.scan_expr_for_runtime(operand, requirements);
            }

            IrExprKind::Call {
                callee, arguments, ..
            } => {
                if let IrCallee::Identifier(name) = callee {
                    match name.as_str() {
                        "__builtin_print" | "print" => requirements.print = true,
                        "__builtin_println" | "println" => requirements.println = true,
                        "__builtin_eprint" | "eprint" => requirements.eprint = true,
                        "__builtin_eprintln" | "eprintln" => requirements.eprintln = true,
                        "__builtin_json_is_valid"
                        | "__builtin_json_escape"
                        | "__builtin_json_has_key"
                        | "__builtin_json_get_string"
                        | "__builtin_json_get_int"
                        | "__builtin_json_get_bool" => requirements.json = true,
                        "__builtin_toml_is_valid"
                        | "__builtin_toml_has_key"
                        | "__builtin_toml_get_string"
                        | "__builtin_toml_get_int"
                        | "__builtin_toml_get_bool" => requirements.toml = true,
                        "__builtin_http_get" | "__builtin_http_post" | "__builtin_http_is_url" => {
                            requirements.http = true
                        }
                        "__builtin_time_unix_seconds" | "__builtin_time_sleep_ms" => {
                            requirements.time = true
                        }
                        _ => {}
                    }
                }

                self.scan_callee_for_runtime(callee, requirements);

                for argument in arguments {
                    self.scan_expr_for_runtime(argument, requirements);
                }
            }

            IrExprKind::Member { object, .. } => {
                self.scan_expr_for_runtime(object, requirements);
            }

            IrExprKind::Intrinsic {
                name, arguments, ..
            } => {
                match name.as_str() {
                    "__builtin_print" => {
                        requirements.print = true;
                    }

                    "__builtin_println" => {
                        requirements.println = true;
                    }

                    "__builtin_eprint" => {
                        requirements.eprint = true;
                    }

                    "__builtin_eprintln" => {
                        requirements.eprintln = true;
                    }

                    _ => {}
                }

                for argument in arguments {
                    self.scan_expr_for_runtime(argument, requirements);
                }
            }
        }
    }

    fn scan_callee_for_runtime(&self, callee: &IrCallee, requirements: &mut RuntimeRequirements) {
        match callee {
            IrCallee::Identifier(_) => {}
            IrCallee::Member { object, .. } => self.scan_callee_for_runtime(object, requirements),
            IrCallee::Value(value) => self.scan_expr_for_runtime(value, requirements),
        }
    }

    // ============================================================
    // C++ Header
    // ============================================================

    fn generate_header(&mut self, requirements: &RuntimeRequirements) {
        self.push_line("#include <algorithm>");
        self.push_line("#include <cstdint>");
        self.push_line("#include <cstdlib>");

        if requirements.string {
            self.push_line("#include <string>");
        }

        self.push_line("#include <sstream>");

        if requirements.needs_io() {
            self.push_line("#include <iostream>");
            self.push_line("#include <string>");
        }

        if requirements.shell {
            self.push_line("#include <cstdio>");
            self.push_line("#include <future>");
            self.push_line("#include <string>");
            self.push_line("#include <utility>");
        }

        if requirements.fs || requirements.env || requirements.path {
            self.push_line("#include <filesystem>");
            self.push_line("#include <fstream>");
            self.push_line("#include <string>");
        }

        if requirements.env {
            self.push_line("#include <cstdlib>");
            self.push_line("#include <string>");
        }

        if requirements.json || requirements.toml || requirements.http {
            self.push_line("#include <cctype>");
            self.push_line("#include <cstdio>");
            self.push_line("#include <string>");
        }

        if requirements.http {
            self.push_line("#if defined(_WIN32)");
            self.push_line("#include <windows.h>");
            self.push_line("#include <winhttp.h>");
            self.push_line("#elif defined(__APPLE__)");
            self.push_line("#include <CoreFoundation/CoreFoundation.h>");
            self.push_line("#include <CFNetwork/CFNetwork.h>");
            self.push_line("#include <sys/types.h>");
            self.push_line("#include <sys/socket.h>");
            self.push_line("#include <netdb.h>");
            self.push_line("#include <sys/time.h>");
            self.push_line("#include <unistd.h>");
            self.push_line("#else");
            self.push_line("#include <sys/types.h>");
            self.push_line("#include <sys/socket.h>");
            self.push_line("#include <netdb.h>");
            self.push_line("#include <sys/time.h>");
            self.push_line("#include <unistd.h>");
            self.push_line("#include <openssl/ssl.h>");
            self.push_line("#include <openssl/err.h>");
            self.push_line("#endif");
        }

        if requirements.time {
            self.push_line("#include <chrono>");
            self.push_line("#include <thread>");
        }

        if requirements.lifetime {
            self.push_line("#include <chrono>");
            self.push_line("#include <memory>");
            self.push_line("#include <mutex>");
            self.push_line("#include <optional>");
            self.push_line("#include <stdexcept>");
            self.push_line("#include <thread>");
            self.push_line("#include <utility>");
        }

        self.push_line("");
    }

    // ============================================================
    // Runtime
    // ============================================================

    fn generate_runtime(&mut self, requirements: &RuntimeRequirements) {
        if !requirements.needs_any_runtime() {
            return;
        }

        self.push_line("// Alaco runtime");
        self.push_line("");

        if requirements.lifetime {
            self.push_line("template <typename T> class __AlacoLifetime {");
            self.push_line("    struct State { std::mutex mutex; std::optional<T> value; std::optional<T> previous; };");
            self.push_line("    std::shared_ptr<State> state;");
            self.push_line("public:");
            self.push_line("    __AlacoLifetime(T initial, long long milliseconds) : state(std::make_shared<State>()) {");
            self.push_line("        state->value = std::move(initial);");
            self.push_line("        auto shared = state;");
            self.push_line("        std::thread([shared, milliseconds] { std::this_thread::sleep_for(std::chrono::milliseconds(milliseconds)); std::lock_guard<std::mutex> lock(shared->mutex); shared->value.reset(); shared->previous.reset(); }).detach();");
            self.push_line("    }");
            self.push_line("    T get() const { std::lock_guard<std::mutex> lock(state->mutex); if (!state->value) throw std::runtime_error(\"Alaco variable lifetime expired\"); return *state->value; }");
            self.push_line("    T previous() const { std::lock_guard<std::mutex> lock(state->mutex); if (!state->value) throw std::runtime_error(\"Alaco variable lifetime expired\"); return state->previous ? *state->previous : *state->value; }");
            self.push_line("    void set(T next) { std::lock_guard<std::mutex> lock(state->mutex); if (!state->value) throw std::runtime_error(\"Alaco variable lifetime expired\"); state->previous = *state->value; state->value = std::move(next); }");
            self.push_line("};");
            self.push_line("");
        }

        if requirements.needs_io() {
            self.generate_io_runtime(requirements);
        }

        if requirements.shell {
            self.generate_shell_runtime(requirements);
        }

        if requirements.fs {
            self.generate_fs_runtime();
        }

        if requirements.env {
            self.generate_env_runtime();
        }

        if requirements.path {
            self.generate_path_runtime();
        }

        if requirements.json {
            self.generate_json_runtime();
        }
        if requirements.toml {
            self.generate_toml_runtime();
        }
        if requirements.http {
            self.generate_http_runtime();
        }
        if requirements.time {
            self.generate_time_runtime();
        }

        self.push_line("// End Alaco runtime");
        self.push_line("");
    }

    fn generate_io_runtime(&mut self, requirements: &RuntimeRequirements) {
        if requirements.print {
            self.push_line("template <typename T> static void print(const T& value) {");

            self.indent += 1;
            self.push_line("std::cout << value;");
            self.indent -= 1;

            self.push_line("}");
            self.push_line(
                "static void print(bool value) { std::cout << (value ? \"true\" : \"false\"); }",
            );
            self.push_line("");
        }

        if requirements.println {
            self.push_line("template <typename T> static void println(const T& value) {");

            self.indent += 1;
            self.push_line("std::cout << value << '\\n';");
            self.indent -= 1;

            self.push_line("}");
            self.push_line("static void println(bool value) { std::cout << (value ? \"true\" : \"false\") << '\\n'; }");
            self.push_line("");
        }

        if requirements.eprint {
            self.push_line("template <typename T> static void eprint(const T& value) {");

            self.indent += 1;
            self.push_line("std::cerr << value;");
            self.indent -= 1;

            self.push_line("}");
            self.push_line(
                "static void eprint(bool value) { std::cerr << (value ? \"true\" : \"false\"); }",
            );
            self.push_line("");
        }

        if requirements.eprintln {
            self.push_line("template <typename T> static void eprintln(const T& value) {");

            self.indent += 1;
            self.push_line("std::cerr << value << '\\n';");
            self.indent -= 1;

            self.push_line("}");
            self.push_line("static void eprintln(bool value) { std::cerr << (value ? \"true\" : \"false\") << '\\n'; }");
            self.push_line("");
        }
    }

    fn generate_fs_runtime(&mut self) {
        self.push_line("static std::string __builtin_fs_read_text(const std::string& path) {");
        self.indent += 1;
        self.push_line("std::ifstream file(path, std::ios::binary);");
        self.push_line("if (!file) return std::string{};");
        self.push_line("return std::string(std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>());");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("static long long __builtin_fs_file_size(const std::string& path) {");
        self.indent += 1;
        self.push_line("std::error_code ec;");
        self.push_line("auto size = std::filesystem::file_size(path, ec);");
        self.push_line("if (ec) return -1;");
        self.push_line("return static_cast<long long>(size);");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("static bool __builtin_fs_copy(const std::string& source, const std::string& destination) {");
        self.indent += 1;
        self.push_line("std::error_code ec;");
        self.push_line("bool copied = std::filesystem::copy_file(source, destination, std::filesystem::copy_options::overwrite_existing, ec);");
        self.push_line("return copied && !ec;");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("static bool __builtin_fs_write_text(const std::string& path, const std::string& contents) {");
        self.indent += 1;
        self.push_line("std::ofstream file(path, std::ios::binary | std::ios::trunc);");
        self.push_line("if (!file) return false;");
        self.push_line("file << contents;");
        self.push_line("return file.good();");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("static bool __builtin_fs_exists(const std::string& path) { std::error_code ec; return std::filesystem::exists(path, ec) && !ec; }");
        self.push_line("static bool __builtin_fs_remove(const std::string& path) { std::error_code ec; return std::filesystem::remove(path, ec) && !ec; }");
        self.push_line("static bool __builtin_fs_create_dir(const std::string& path) { std::error_code ec; return std::filesystem::create_directories(path, ec) || (!ec && std::filesystem::is_directory(path, ec)); }");
        self.push_line("");
    }

    fn generate_env_runtime(&mut self) {
        self.push_line("static std::string __builtin_env_get(const std::string& name) { const char* value = std::getenv(name.c_str()); return value ? std::string(value) : std::string{}; }");
        self.push_line("static bool __builtin_env_has(const std::string& name) { return std::getenv(name.c_str()) != nullptr; }");
        self.push_line(
            "static bool __builtin_env_set(const std::string& name, const std::string& value) {",
        );
        self.indent += 1;
        self.push_line("if (name.empty() || name.find('=') != std::string::npos) return false;");
        self.push_line("#if defined(_WIN32)");
        self.push_line("return _putenv_s(name.c_str(), value.c_str()) == 0;");
        self.push_line("#else");
        self.push_line("return setenv(name.c_str(), value.c_str(), 1) == 0;");
        self.push_line("#endif");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("static std::string __builtin_env_current_dir() { std::error_code ec; auto path = std::filesystem::current_path(ec); return ec ? std::string{} : path.string(); }");
        self.push_line("");
    }

    fn generate_path_runtime(&mut self) {
        self.push_line("static std::string __builtin_path_join(const std::string& left, const std::string& right) { return (std::filesystem::path(left) / std::filesystem::path(right)).lexically_normal().string(); }");
        self.push_line("static std::string __builtin_path_basename(const std::string& value) { return std::filesystem::path(value).filename().string(); }");
        self.push_line("static std::string __builtin_path_dirname(const std::string& value) { auto parent = std::filesystem::path(value).parent_path().string(); return parent.empty() ? std::string(\".\") : parent; }");
        self.push_line("static std::string __builtin_path_extension(const std::string& value) { auto ext = std::filesystem::path(value).extension().string(); return !ext.empty() && ext.front() == '.' ? ext.substr(1) : ext; }");
        self.push_line("static std::string __builtin_path_stem(const std::string& value) { return std::filesystem::path(value).stem().string(); }");
        self.push_line("static bool __builtin_path_is_absolute(const std::string& value) { return std::filesystem::path(value).is_absolute(); }");
        self.push_line("");
    }

    fn generate_json_runtime(&mut self) {
        self.push_line("class __AlacoJsonParser {");
        self.push_line("    const std::string& text; std::size_t pos = 0;");
        // JSON permits exactly four whitespace bytes; std::isspace also accepts
        // vertical tab and form feed, which must not make invalid JSON pass.
        self.push_line("    void ws() { while (pos < text.size() && (text[pos] == ' ' || text[pos] == '\\t' || text[pos] == '\\n' || text[pos] == '\\r')) ++pos; }");
        self.push_line("    bool take(char c) { ws(); if (pos < text.size() && text[pos] == c) { ++pos; return true; } return false; }");
        self.push_line("    bool string_value() { if (!take('\\\"')) return false; while (pos < text.size()) { unsigned char c = static_cast<unsigned char>(text[pos++]); if (c == '\\\"') return true; if (c < 0x20) return false; if (c == '\\\\') { if (pos >= text.size()) return false; char e = text[pos++]; if (e == 'u') { for (int n = 0; n < 4; ++n) { if (pos >= text.size() || !std::isxdigit(static_cast<unsigned char>(text[pos++]))) return false; } } else if (e != '\\\"' && e != '\\\\' && e != '/' && e != 'b' && e != 'f' && e != 'n' && e != 'r' && e != 't') return false; } } return false; }");
        self.push_line("    bool number_value() { std::size_t start = pos; if (pos < text.size() && text[pos] == '-') ++pos; if (pos >= text.size()) return false; if (text[pos] == '0') ++pos; else { if (!std::isdigit(static_cast<unsigned char>(text[pos]))) return false; while (pos < text.size() && std::isdigit(static_cast<unsigned char>(text[pos]))) ++pos; } if (pos < text.size() && text[pos] == '.') { ++pos; std::size_t digits = pos; while (pos < text.size() && std::isdigit(static_cast<unsigned char>(text[pos]))) ++pos; if (digits == pos) return false; } if (pos < text.size() && (text[pos] == 'e' || text[pos] == 'E')) { ++pos; if (pos < text.size() && (text[pos] == '+' || text[pos] == '-')) ++pos; std::size_t digits = pos; while (pos < text.size() && std::isdigit(static_cast<unsigned char>(text[pos]))) ++pos; if (digits == pos) return false; } return pos > start; }");
        self.push_line("    bool value() { ws(); if (pos >= text.size()) return false; char c = text[pos]; if (c == '\\\"') return string_value(); if (c == '{') { ++pos; ws(); if (take('}')) return true; do { if (!string_value() || !take(':') || !value()) return false; ws(); if (take('}')) return true; } while (take(',')); return false; } if (c == '[') { ++pos; ws(); if (take(']')) return true; do { if (!value()) return false; ws(); if (take(']')) return true; } while (take(',')); return false; } for (const char* word : {\"true\", \"false\", \"null\"}) { std::string candidate(word); if (text.compare(pos, candidate.size(), candidate) == 0) { pos += candidate.size(); return true; } } return number_value(); }");
        self.push_line("    std::string raw_for_key(const std::string& wanted) { pos = 0; if (!take('{')) return {}; ws(); if (take('}')) return {}; while (pos < text.size()) { ws(); std::size_t key_start = pos; if (!string_value()) return {}; std::size_t key_end = pos; std::string key = text.substr(key_start, key_end - key_start); if (!take(':')) return {}; std::size_t value_start = pos; if (!value()) return {}; std::size_t value_end = pos; if (key == std::string(\"\\\"\") + wanted + \"\\\"\") return text.substr(value_start, value_end - value_start); if (!take(',')) break; } return {}; }");
        self.push_line("public: explicit __AlacoJsonParser(const std::string& source) : text(source) {} bool valid() { if (!value()) return false; ws(); return pos == text.size(); } std::string get_raw(const std::string& key) { if (!valid()) return {}; return raw_for_key(key); }");
        self.push_line("};");
        self.push_line("static bool __builtin_json_is_valid(const std::string& text) { return __AlacoJsonParser(text).valid(); }");
        self.push_line("static std::string __builtin_json_get_raw(const std::string& text, const std::string& key) { return __AlacoJsonParser(text).get_raw(key); }");
        self.push_line("static bool __builtin_json_has_key(const std::string& text, const std::string& key) { return !__builtin_json_get_raw(text, key).empty(); }");
        self.push_line("static long long __builtin_json_get_int(const std::string& text, const std::string& key, long long fallback) { auto raw = __builtin_json_get_raw(text, key); if (raw.empty() || raw.find_first_of(\".eE\") != std::string::npos) return fallback; try { std::size_t n = 0; auto v = std::stoll(raw, &n); return n == raw.size() ? v : fallback; } catch (...) { return fallback; } }");
        self.push_line("static bool __builtin_json_get_bool(const std::string& text, const std::string& key, bool fallback) { auto raw = __builtin_json_get_raw(text, key); return raw == \"true\" ? true : raw == \"false\" ? false : fallback; }");
        self.push_line("static std::string __builtin_json_get_string(const std::string& text, const std::string& key, const std::string& fallback) { auto raw = __builtin_json_get_raw(text, key); if (raw.size() < 2 || raw.front() != 34 || raw.back() != 34) return fallback; std::string out; for (std::size_t i = 1; i + 1 < raw.size(); ++i) { char c = raw[i]; if (c == 92 && i + 2 < raw.size()) { char e = raw[++i]; switch (e) { case 'n': out += char(10); break; case 'r': out += char(13); break; case 't': out += char(9); break; case 'b': out += char(8); break; case 'f': out += char(12); break; default: out += e; break; } } else out += c; } return out; }");
        self.push_line("static std::string __builtin_json_escape(const std::string& text) { std::string out; const char* hex = \"0123456789abcdef\"; for (unsigned char c : text) { switch (c) { case '\\\"': out += \"\\\\\\\"\"; break; case '\\\\': out += \"\\\\\\\\\"; break; case '\\b': out += \"\\\\b\"; break; case '\\f': out += \"\\\\f\"; break; case '\\n': out += \"\\\\n\"; break; case '\\r': out += \"\\\\r\"; break; case '\\t': out += \"\\\\t\"; break; default: if (c < 0x20) { out += \"\\\\u00\"; out += hex[c >> 4]; out += hex[c & 15]; } else out += static_cast<char>(c); } } return out; }");
        self.push_line("");
    }

    fn generate_toml_runtime(&mut self) {
        self.push_line("static bool __builtin_toml_is_valid(const std::string& text) {");
        self.push_line("    std::size_t start = 0; while (start <= text.size()) { std::size_t end = text.find('\\n', start); if (end == std::string::npos) end = text.size(); std::string line = text.substr(start, end - start); start = end + 1;");
        self.push_line("        auto first = line.find_first_not_of(\" \\t\\r\"); if (first == std::string::npos) { if (end == text.size()) break; continue; } line = line.substr(first); bool quoted = false, escaped = false; std::size_t comment = std::string::npos; for (std::size_t i = 0; i < line.size(); ++i) { char c = line[i]; if (escaped) { escaped = false; continue; } if (quoted && c == '\\\\') { escaped = true; continue; } if (c == '\\\"') quoted = !quoted; if (c == '#' && !quoted) { comment = i; break; } } if (comment != std::string::npos) line = line.substr(0, comment); auto last = line.find_last_not_of(\" \\t\\r\"); if (last == std::string::npos) continue; line.resize(last + 1); if (quoted) return false;");
        self.push_line("        if (line.front() == '[') { if (line.size() < 3 || line.back() != ']') return false; continue; } auto eq = line.find('='); if (eq == std::string::npos || eq == 0 || eq + 1 >= line.size()) return false; std::string key = line.substr(0, eq), value = line.substr(eq + 1); if (key.find_first_not_of(\"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_- .\\\"\") != std::string::npos) return false; auto vf = value.find_first_not_of(\" \\t\"); if (vf == std::string::npos) return false; value = value.substr(vf); if (value == \"true\" || value == \"false\") continue; if (value.front() == 34) { if (value.size() < 2 || value.back() != 34) return false; continue; } if (value.front() == '[') { if (value.back() != ']') return false; continue; } try { std::size_t n = 0; (void)std::stod(value, &n); if (n != value.size()) return false; } catch (...) { return false; } }");
        self.push_line("    return true;");
        self.push_line("}");
        self.push_line("static std::string __builtin_toml_raw(const std::string& text, const std::string& wanted) { std::size_t start = 0; while (start < text.size()) { auto end = text.find('\\n', start); if (end == std::string::npos) end = text.size(); std::string line = text.substr(start, end - start); start = end + 1; auto first = line.find_first_not_of(\" \\t\\r\"); if (first == std::string::npos) continue; line = line.substr(first); if (line.front() == '[') return {}; auto hash = line.find('#'); if (hash != std::string::npos) line = line.substr(0, hash); auto eq = line.find('='); if (eq == std::string::npos) continue; auto trim = [](std::string s) { auto a = s.find_first_not_of(\" \\t\\r\"); auto b = s.find_last_not_of(\" \\t\\r\"); return a == std::string::npos ? std::string() : s.substr(a, b - a + 1); }; if (trim(line.substr(0, eq)) == wanted) return trim(line.substr(eq + 1)); } return {}; }");
        self.push_line("static bool __builtin_toml_has_key(const std::string& text, const std::string& key) { return !__builtin_toml_raw(text, key).empty(); }");
        self.push_line("static long long __builtin_toml_get_int(const std::string& text, const std::string& key, long long fallback) { auto raw = __builtin_toml_raw(text, key); try { std::size_t n = 0; auto v = std::stoll(raw, &n); return !raw.empty() && n == raw.size() ? v : fallback; } catch (...) { return fallback; } }");
        self.push_line("static bool __builtin_toml_get_bool(const std::string& text, const std::string& key, bool fallback) { auto raw = __builtin_toml_raw(text, key); return raw == \"true\" ? true : raw == \"false\" ? false : fallback; }");
        self.push_line("static std::string __builtin_toml_get_string(const std::string& text, const std::string& key, const std::string& fallback) { auto raw = __builtin_toml_raw(text, key); if (raw.size() < 2 || raw.front() != 34 || raw.back() != 34) return fallback; return raw.substr(1, raw.size() - 2); }");
        self.push_line("");
    }

    fn generate_http_runtime(&mut self) {
        self.output.push_str(r#"static bool __builtin_http_is_url(const std::string& url) {
    return url.rfind("https://", 0) == 0 || url.rfind("http://", 0) == 0;
}
static std::string __alaco_http_request(const std::string& url, const std::string* body) {
#if defined(_WIN32)
    std::wstring wide(url.begin(), url.end());
    URL_COMPONENTS parts{}; parts.dwStructSize = sizeof(parts);
    wchar_t host[256]{}, path[4096]{};
    parts.lpszHostName = host; parts.dwHostNameLength = 256;
    parts.lpszUrlPath = path; parts.dwUrlPathLength = 4096;
    if (!__builtin_http_is_url(url) || !WinHttpCrackUrl(wide.c_str(), 0, 0, &parts)) return {};
    HINTERNET session = WinHttpOpen(L"Alaco/0.1", WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0);
    if (!session) return {};
    WinHttpSetTimeouts(session, 20000, 20000, 20000, 20000);
    HINTERNET connection = WinHttpConnect(session, host, parts.nPort, 0);
    if (!connection) { WinHttpCloseHandle(session); return {}; }
    bool secure = parts.nScheme == INTERNET_SCHEME_HTTPS;
    HINTERNET request = WinHttpOpenRequest(connection, body ? L"POST" : L"GET", path, nullptr, WINHTTP_NO_REFERER, WINHTTP_DEFAULT_ACCEPT_TYPES, secure ? WINHTTP_FLAG_SECURE : 0);
    std::string result;
    if (request) {
        const wchar_t* headers = body ? L"Content-Type: application/json\r\n" : WINHTTP_NO_ADDITIONAL_HEADERS;
        BOOL sent = WinHttpSendRequest(request, headers, body ? (DWORD)-1L : 0, body ? (LPVOID)body->data() : WINHTTP_NO_REQUEST_DATA, body ? (DWORD)body->size() : 0, body ? (DWORD)body->size() : 0, 0);
        DWORD status = 0, status_size = sizeof(status);
        if (sent && WinHttpReceiveResponse(request, nullptr) && WinHttpQueryHeaders(request, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_HEADER_NAME_BY_INDEX, &status, &status_size, WINHTTP_NO_HEADER_INDEX) && status >= 200 && status < 300) {
            DWORD available = 0;
            while (WinHttpQueryDataAvailable(request, &available) && available) {
                std::string chunk(available, '\0'); DWORD read = 0;
                if (!WinHttpReadData(request, chunk.data(), available, &read) || !read) break;
                chunk.resize(read); result += chunk;
            }
        }
        WinHttpCloseHandle(request);
    }
    WinHttpCloseHandle(connection); WinHttpCloseHandle(session); return result;
#elif defined(__APPLE__)
    if (!__builtin_http_is_url(url)) return {};
    CFStringRef url_string = CFStringCreateWithCString(kCFAllocatorDefault, url.c_str(), kCFStringEncodingUTF8);
    if (!url_string) return {};
    CFURLRef cf_url = CFURLCreateWithString(kCFAllocatorDefault, url_string, nullptr);
    CFRelease(url_string);
    if (!cf_url) return {};
    CFStringRef method = CFStringCreateWithCString(kCFAllocatorDefault, body ? "POST" : "GET", kCFStringEncodingASCII);
    CFHTTPMessageRef message = CFHTTPMessageCreateRequest(kCFAllocatorDefault, method, cf_url, kCFHTTPVersion1_1);
    CFRelease(method);
    if (!message) { CFRelease(cf_url); return {}; }
    if (body) {
        CFStringRef header = CFStringCreateWithCString(kCFAllocatorDefault, "Content-Type", kCFStringEncodingASCII);
        CFStringRef value = CFStringCreateWithCString(kCFAllocatorDefault, "application/json", kCFStringEncodingASCII);
        CFHTTPMessageSetHeaderFieldValue(message, header, value);
        CFRelease(header); CFRelease(value);
        CFDataRef data = CFDataCreate(kCFAllocatorDefault, reinterpret_cast<const UInt8*>(body->data()), static_cast<CFIndex>(body->size()));
        CFHTTPMessageSetBody(message, data); CFRelease(data);
    }
    CFReadStreamRef stream = CFReadStreamCreateForHTTPRequest(kCFAllocatorDefault, message);
    CFRelease(message);
    if (!stream) { CFRelease(cf_url); return {}; }
    CFStringRef scheme = CFURLCopyScheme(cf_url);
    bool secure = scheme && CFEqual(scheme, CFSTR("https"));
    if (scheme) CFRelease(scheme);
    if (secure) {
        const void* keys[] = { kCFStreamSSLLevel };
        const void* values[] = { kCFStreamSocketSecurityLevelNegotiatedSSL };
        CFDictionaryRef settings = CFDictionaryCreate(kCFAllocatorDefault, keys, values, 1, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CFReadStreamSetProperty(stream, kCFStreamPropertySSLSettings, settings);
        CFRelease(settings);
    }
    std::string result;
    if (CFReadStreamOpen(stream)) {
        UInt8 buffer[4096]; CFIndex count;
        while ((count = CFReadStreamRead(stream, buffer, sizeof(buffer))) > 0) result.append(reinterpret_cast<const char*>(buffer), static_cast<std::size_t>(count));
        CFTypeRef response = CFReadStreamCopyProperty(stream, kCFStreamPropertyHTTPResponseHeader);
        if (response && CFGetTypeID(response) == CFHTTPMessageGetTypeID()) {
            int status = static_cast<int>(CFHTTPMessageGetResponseStatusCode((CFHTTPMessageRef)response));
            if (status < 200 || status >= 300) result.clear();
        } else result.clear();
        if (response) CFRelease(response);
        CFReadStreamClose(stream);
    }
    CFRelease(stream); CFRelease(cf_url); return result;
#else
    if (!__builtin_http_is_url(url)) return {};
    const bool secure = url.rfind("https://", 0) == 0;
    std::string rest = url.substr(secure ? 8 : 7);
    auto slash = rest.find_first_of("/?#");
    std::string authority = rest.substr(0, slash);
    std::string path = slash == std::string::npos ? "/" : rest.substr(slash);
    if (path.front() == '?' || path.front() == '#') path = "/" + path;
    auto colon = authority.rfind(':');
    std::string host = colon == std::string::npos ? authority : authority.substr(0, colon);
    std::string port = colon == std::string::npos ? (secure ? "443" : "80") : authority.substr(colon + 1);
    if (host.empty() || host.find('@') != std::string::npos) return {};
    addrinfo hints{}; hints.ai_socktype = SOCK_STREAM; hints.ai_family = AF_UNSPEC;
    addrinfo* addresses = nullptr;
    if (getaddrinfo(host.c_str(), port.c_str(), &hints, &addresses) != 0) return {};
    int fd = -1;
    for (auto* a = addresses; a; a = a->ai_next) {
        fd = static_cast<int>(socket(a->ai_family, a->ai_socktype, a->ai_protocol));
        if (fd >= 0 && connect(fd, a->ai_addr, a->ai_addrlen) == 0) break;
        if (fd >= 0) close(fd);
        fd = -1;
    }
    freeaddrinfo(addresses);
    if (fd < 0) return {};
    timeval timeout{}; timeout.tv_sec = 20;
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
    SSL_CTX* context = nullptr; SSL* tls = nullptr;
    if (secure) {
        context = SSL_CTX_new(TLS_client_method());
        if (!context) { close(fd); return {}; }
        SSL_CTX_set_verify(context, SSL_VERIFY_PEER, nullptr);
        if (SSL_CTX_set_default_verify_paths(context) != 1) { SSL_CTX_free(context); close(fd); return {}; }
        tls = SSL_new(context);
        if (!tls || SSL_set_tlsext_host_name(tls, host.c_str()) != 1 || SSL_set1_host(tls, host.c_str()) != 1 || SSL_set_fd(tls, fd) != 1 || SSL_connect(tls) != 1) {
            if (tls) SSL_free(tls); SSL_CTX_free(context); close(fd); return {};
        }
    }
    auto write_all = [&](const char* data, std::size_t size) {
        std::size_t sent = 0;
        while (sent < size) {
            std::size_t remaining = size - sent;
            int n = secure ? SSL_write(tls, data + sent, static_cast<int>(remaining > 1048576 ? 1048576 : remaining)) : static_cast<int>(send(fd, data + sent, remaining, 0));
            if (n <= 0) return false;
            sent += static_cast<std::size_t>(n);
        }
        return true;
    };
    std::string request = body ? "POST " : "GET ";
    request += path + " HTTP/1.1\r\nHost: " + host + "\r\nConnection: close\r\n";
    if (body) request += "Content-Type: application/json\r\nContent-Length: " + std::to_string(body->size()) + "\r\n";
    request += "\r\n"; if (body) request += *body;
    if (!write_all(request.data(), request.size())) { if (tls) SSL_free(tls); if (context) SSL_CTX_free(context); close(fd); return {}; }
    std::string response; char buffer[4096];
    for (;;) {
        int n = secure ? SSL_read(tls, buffer, sizeof(buffer)) : static_cast<int>(recv(fd, buffer, sizeof(buffer), 0));
        if (n <= 0) break;
        response.append(buffer, static_cast<std::size_t>(n));
    }
    if (tls) { SSL_shutdown(tls); SSL_free(tls); }
    if (context) SSL_CTX_free(context);
    close(fd);
    auto split = response.find("\r\n\r\n");
    auto status = response.find("HTTP/");
    auto status_end = response.find("\r\n", status);
    bool ok = status != std::string::npos && status_end != std::string::npos && response.size() > status + 9 && response[status + 9] == '2';
    if (split == std::string::npos || !ok) return {};
    return response.substr(split + 4);
#endif
}
static std::string __builtin_http_get(const std::string& url) { return __alaco_http_request(url, nullptr); }
static std::string __builtin_http_post(const std::string& url, const std::string& body) { return __alaco_http_request(url, &body); }

"#);
    }

    fn generate_time_runtime(&mut self) {
        self.push_line("static long long __builtin_time_unix_seconds() { return std::chrono::duration_cast<std::chrono::seconds>(std::chrono::system_clock::now().time_since_epoch()).count(); }");
        self.push_line("static void __builtin_time_sleep_ms(long long ms) { if (ms > 0) std::this_thread::sleep_for(std::chrono::milliseconds(ms)); }");
        self.push_line("");
    }

    fn generate_shell_runtime(&mut self, requirements: &RuntimeRequirements) {
        self.push_line("struct ShellResult {");

        self.indent += 1;

        self.push_line("int status;");
        self.push_line("std::string output;");

        self.indent -= 1;

        self.push_line("};");
        self.push_line("");

        self.push_line(
            "static std::ostream& operator<<(std::ostream& stream, const ShellResult& value) {",
        );
        self.indent += 1;
        self.push_line("stream << \"ShellResult { status: \" << value.status << \", output: \" << value.output << \" }\";");
        self.push_line("return stream;");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Shell expressions return futures. Printing one resolves it and
        // formats the resulting ShellResult instead of streaming std::future.
        for (name, enabled, stream) in [
            ("print", requirements.print, "std::cout"),
            ("println", requirements.println, "std::cout"),
            ("eprint", requirements.eprint, "std::cerr"),
            ("eprintln", requirements.eprintln, "std::cerr"),
        ] {
            if enabled {
                let newline = if name.ends_with("ln") {
                    " << '\\n'"
                } else {
                    ""
                };
                self.push_line(&format!(
                    "static void {}(std::future<ShellResult>& value) {{ {} << value.get(){}; }}",
                    name, stream, newline
                ));
                self.push_line(&format!(
                    "static void {}(std::future<ShellResult>&& value) {{ {} << value.get(){}; }}",
                    name, stream, newline
                ));
            }
        }
        self.push_line("");

        self.push_line("struct Shell {");

        self.indent += 1;

        self.push_line("std::string command;");

        self.indent -= 1;

        self.push_line("};");
        self.push_line("");

        self.push_line("static Shell shell_create(const std::string& command) {");

        self.indent += 1;

        self.push_line("return Shell{command};");

        self.indent -= 1;

        self.push_line("}");
        self.push_line("");

        self.push_line("static ShellResult shell_run(const Shell& shell) {");

        self.indent += 1;

        self.push_line("std::string output;");

        self.push_line("FILE* pipe = popen(shell.command.c_str(), \"r\");");

        self.push_line("if (!pipe) {");

        self.indent += 1;

        self.push_line("return ShellResult{-1, \"\"};");

        self.indent -= 1;

        self.push_line("}");

        self.push_line("char buffer[4096];");

        self.push_line("while (fgets(buffer, sizeof(buffer), pipe)) {");

        self.indent += 1;

        self.push_line("output += buffer;");

        self.indent -= 1;

        self.push_line("}");

        self.push_line("int status = pclose(pipe);");

        self.push_line("return ShellResult{status, std::move(output)};");

        self.indent -= 1;

        self.push_line("}");
        self.push_line("");

        self.push_line("static std::future<ShellResult> shell_async(");

        self.indent += 1;

        self.push_line("const std::string& command");

        self.indent -= 1;

        self.push_line(") {");

        self.indent += 1;

        self.push_line("return std::async(std::launch::async, [command]() {");

        self.indent += 1;

        self.push_line("return shell_run(Shell{command});");

        self.indent -= 1;

        self.push_line("});");

        self.indent -= 1;

        self.push_line("}");
        self.push_line("");
    }

    // ============================================================
    // Items
    // ============================================================

    fn generate_ir_struct(&mut self, structure: &IrStruct) {
        let name = self.cpp_identifier(&structure.name);
        self.push_line(&format!("struct {} {{", name));
        self.indent += 1;

        for field in &structure.fields {
            self.push_line(&format!(
                "{} {};",
                self.type_name(&field.ty),
                self.cpp_identifier(&field.name)
            ));
        }

        self.indent -= 1;
        self.push_line("};");
        self.push_line("");

        self.push_line(&format!(
            "static std::ostream& operator<<(std::ostream& stream, const {}& value) {{",
            name
        ));
        self.indent += 1;
        self.push_line(&format!("stream << \"{} {{\";", structure.name));
        for (index, field) in structure.fields.iter().enumerate() {
            let separator = if index == 0 { " " } else { ", " };
            self.push_line(&format!("stream << \"{}{}: \";", separator, field.name));
            if field.ty == Type::Bool {
                self.push_line(&format!(
                    "stream << (value.{} ? \"true\" : \"false\");",
                    self.cpp_identifier(&field.name)
                ));
            } else {
                self.push_line(&format!(
                    "stream << value.{};",
                    self.cpp_identifier(&field.name)
                ));
            }
        }
        self.push_line("stream << \" }\";");
        self.push_line("return stream;");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");
    }

    fn generate_ir_function(&mut self, function: &IrFunction) {
        let outer_lifetimes = std::mem::take(&mut self.lifetime_variables);
        Self::collect_lifetime_names(&function.body, &mut self.lifetime_variables);
        let return_type = if function.name == "main" {
            "int".to_string()
        } else {
            self.type_name(&function.return_type)
        };
        let name = self.cpp_identifier(&function.name);
        let params = function
            .parameters
            .iter()
            .map(|param| {
                format!(
                    "{} {}",
                    self.type_name(&param.ty),
                    self.cpp_identifier(&param.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        self.push_line(&format!("{} {}({}) {{", return_type, name, params));
        self.indent += 1;
        for parameter in &function.parameters {
            self.push_line(&format!(
                "{} {} = {};",
                self.type_name(&parameter.ty),
                self.previous_identifier(&parameter.name),
                self.cpp_identifier(&parameter.name)
            ));
        }
        self.generate_ir_block_contents(&function.body);
        if function.name == "main" {
            self.push_line("return 0;");
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");
        self.lifetime_variables = outer_lifetimes;
    }

    fn generate_ir_block_contents(&mut self, block: &crate::ir::IrBlock) {
        for statement in &block.statements {
            self.generate_ir_statement(statement);
        }
    }

    fn generate_ir_statement(&mut self, statement: &IrStmt) {
        match statement {
            IrStmt::Let {
                name,
                ty,
                value,
                lifetime_ms,
                ..
            } => {
                if let Some(milliseconds) = lifetime_ms {
                    self.push_line(&format!(
                        "__AlacoLifetime<{}> {}({}, {}LL);",
                        self.type_name(ty),
                        self.cpp_identifier(name),
                        self.ir_expression(value),
                        milliseconds
                    ));
                    return;
                }
                self.push_line(&format!(
                    "{} {} = {};",
                    self.type_name(ty),
                    self.cpp_identifier(name),
                    self.ir_expression(value)
                ));
                self.push_line(&format!(
                    "{} {} = {};",
                    self.type_name(ty),
                    self.previous_identifier(name),
                    self.cpp_identifier(name)
                ));
            }
            IrStmt::Destructure {
                bindings,
                value,
                span,
                ..
            } => {
                let temporary = format!("_alaco_destructure_{}", span.start);
                self.push_line(&format!(
                    "const auto {} = {};",
                    temporary,
                    self.ir_expression(value)
                ));
                for (field, binding) in bindings {
                    self.push_line(&format!(
                        "auto {} = {}.{};",
                        self.cpp_identifier(binding),
                        temporary,
                        self.cpp_identifier(field)
                    ));
                    self.push_line(&format!(
                        "auto {} = {};",
                        self.previous_identifier(binding),
                        self.cpp_identifier(binding)
                    ));
                }
            }
            IrStmt::Return(value, _) | IrStmt::Stop(value, _) => {
                self.push_line(
                    &value
                        .as_ref()
                        .map(|v| format!("return {};", self.ir_expression(v)))
                        .unwrap_or_else(|| "return;".to_string()),
                );
            }
            IrStmt::Skip(_) => self.push_line("continue;"),
            IrStmt::Expr(expr, _) => self.push_line(&format!("{};", self.ir_expression(expr))),
            IrStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.push_line(&format!("if ({}) {{", self.ir_expression(condition)));
                self.indent += 1;
                self.generate_ir_block_contents(then_block);
                self.indent -= 1;
                if let Some(block) = else_block {
                    self.push_line("} else {");
                    self.indent += 1;
                    self.generate_ir_block_contents(block);
                    self.indent -= 1;
                }
                self.push_line("}");
            }
            IrStmt::Loop {
                kind,
                binding,
                body,
                ..
            } => {
                match kind {
                    IrLoopKind::Infinite => self.push_line("while (true) {"),
                    IrLoopKind::Repeat(count) => self.push_line(&format!(
                        "for (long long _i = 0; _i < {}; ++_i) {{",
                        self.ir_expression(count)
                    )),
                    IrLoopKind::While(condition) => {
                        self.push_line(&format!("while ({}) {{", self.ir_expression(condition)))
                    }
                    IrLoopKind::For { variable, iterable } => self.push_line(&format!(
                        "for (auto {} : {}) {{",
                        self.cpp_identifier(binding.as_deref().unwrap_or(variable)),
                        self.ir_expression(iterable)
                    )),
                }
                self.indent += 1;
                self.generate_ir_block_contents(body);
                self.indent -= 1;
                self.push_line("}");
            }
            IrStmt::Match {
                expression, arms, ..
            } => {
                if arms.is_empty() {
                    return;
                }
                let temp = "_match_value";
                self.push_line(&format!(
                    "auto {} = {};",
                    temp,
                    self.ir_expression(expression)
                ));
                for (index, arm) in arms.iter().enumerate() {
                    let condition = self.match_condition(temp, &arm.pattern);
                    if index == 0 {
                        self.push_line(
                            &condition
                                .map(|c| format!("if ({}) {{", c))
                                .unwrap_or_else(|| "{".to_string()),
                        );
                    } else {
                        self.push_line(
                            &condition
                                .map(|c| format!("}} else if ({}) {{", c))
                                .unwrap_or_else(|| "} else {".to_string()),
                        );
                    }
                    self.indent += 1;
                    self.generate_ir_block_contents(&arm.body);
                    self.indent -= 1;
                }
                self.push_line("}");
            }
        }
    }

    fn ir_expression(&self, expression: &IrExpr) -> String {
        match &expression.kind {
            IrExprKind::Number(value) => value.clone(),
            IrExprKind::String(value) => self.cpp_string_literal(value),
            IrExprKind::Interpolation(parts) => {
                let mut output = String::from("([&]() { std::ostringstream _alaco_interp; ");
                for (index, part) in parts.iter().enumerate() {
                    if index % 2 == 0 {
                        output.push_str(&format!(
                            "_alaco_interp << {}; ",
                            self.cpp_string_literal(part)
                        ));
                    } else {
                        output.push_str(&format!(
                            "_alaco_interp << {}; ",
                            self.expression_identifier(part)
                        ));
                    }
                }
                output.push_str("return _alaco_interp.str(); }())");
                output
            }
            IrExprKind::Bool(value) => value.to_string(),
            IrExprKind::Identifier(name) => self.expression_identifier(name),
            IrExprKind::Previous(name) if self.lifetime_variables.contains(name) => {
                format!("{}.previous()", self.cpp_identifier(name))
            }
            IrExprKind::Previous(name) => self.previous_identifier(name),
            IrExprKind::StructLiteral { name, fields } => format!(
                "{}{{{}}}",
                self.cpp_identifier(name),
                fields
                    .iter()
                    .map(|(_, v)| self.ir_expression(v))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            IrExprKind::Binary {
                left,
                operator,
                right,
            } => {
                if matches!(
                    operator,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubtractAssign
                        | BinaryOp::MultiplyAssign
                        | BinaryOp::DivideAssign
                ) {
                    if let IrExprKind::Identifier(name) = &left.kind {
                        if self.lifetime_variables.contains(name) {
                            let right = self.ir_expression(right);
                            let value = if *operator == BinaryOp::Assign {
                                right
                            } else {
                                let base_operator = match operator {
                                    BinaryOp::AddAssign => "+",
                                    BinaryOp::SubtractAssign => "-",
                                    BinaryOp::MultiplyAssign => "*",
                                    BinaryOp::DivideAssign => "/",
                                    _ => "=",
                                };
                                format!(
                                    "({}.get() {} {})",
                                    self.cpp_identifier(name),
                                    base_operator,
                                    right
                                )
                            };
                            return format!("{}.set({})", self.cpp_identifier(name), value);
                        }
                        return format!(
                            "(({} = {}), ({} {} {}))",
                            self.previous_identifier(name),
                            self.cpp_identifier(name),
                            self.cpp_identifier(name),
                            self.binary_operator(*operator),
                            self.ir_expression(right)
                        );
                    }
                }
                format!(
                    "({} {} {})",
                    self.ir_expression(left),
                    self.binary_operator(*operator),
                    self.ir_expression(right)
                )
            }
            IrExprKind::Pipe { input, target } => {
                let input = self.ir_expression(input);
                match target {
                    IrPipeTarget::Call { callee, arguments } => {
                        let mut values = vec![input];
                        values.extend(arguments.iter().map(|a| self.ir_expression(a)));
                        format!("{}({})", self.ir_callee(callee), values.join(", "))
                    }
                    IrPipeTarget::Callee(callee) => {
                        format!("{}({})", self.ir_callee(callee), input)
                    }
                    IrPipeTarget::Value(value) => {
                        format!("{}({})", self.ir_expression(value), input)
                    }
                }
            }
            IrExprKind::Unary { operator, operand } => match operator {
                UnaryOp::Negate => format!("(-{})", self.ir_expression(operand)),
                UnaryOp::Not => format!("(!{})", self.ir_expression(operand)),
            },
            IrExprKind::Call { callee, arguments } => {
                let args = arguments
                    .iter()
                    .map(|a| self.ir_expression(a))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", self.ir_callee(callee), args)
            }
            IrExprKind::Member { object, member } => format!(
                "{}.{}",
                self.ir_expression(object),
                self.cpp_identifier(member)
            ),
            IrExprKind::Shell { command } => {
                format!("shell_async({})", self.cpp_string_literal(command))
            }
            IrExprKind::Intrinsic { name, arguments } => {
                let args = arguments
                    .iter()
                    .map(|a| self.ir_expression(a))
                    .collect::<Vec<_>>()
                    .join(", ");
                match name.as_str() {
                    "__builtin_print" | "__builtin_println" | "__builtin_eprint"
                    | "__builtin_eprintln" => format!("{}({})", name, args),
                    _ => format!("{}({})", self.cpp_identifier(name), args),
                }
            }
        }
    }

    fn ir_callee(&self, callee: &IrCallee) -> String {
        match callee {
            IrCallee::Identifier(name) => self.cpp_identifier(name),
            IrCallee::Member { object, member } => {
                if let IrCallee::Identifier(alias) = object.as_ref() {
                    if self.module_aliases.contains(alias) {
                        if alias == "io" {
                            return self.cpp_identifier(member);
                        }
                        return format!(
                            "{}::{}",
                            self.cpp_identifier(alias),
                            self.cpp_identifier(member)
                        );
                    }
                }
                format!("{}.{}", self.ir_callee(object), self.cpp_identifier(member))
            }
            IrCallee::Value(value) => self.ir_expression(value),
        }
    }

    fn match_condition(&self, value: &str, pattern: &IrMatchPattern) -> Option<String> {
        match pattern {
            IrMatchPattern::Number(number) => Some(format!("{} == {}", value, number)),

            IrMatchPattern::String(string) => {
                Some(format!("{} == {}", value, self.cpp_string_literal(string)))
            }

            IrMatchPattern::Bool(value_bool) => Some(format!(
                "{} == {}",
                value,
                if *value_bool { "true" } else { "false" }
            )),

            IrMatchPattern::Identifier(identifier) => {
                Some(format!("{} == {}", value, self.cpp_identifier(identifier)))
            }

            IrMatchPattern::Wildcard => None,
        }
    }

    // ============================================================
    // Operators
    // ============================================================

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

    // ============================================================
    // Types
    // ============================================================

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

    // ============================================================
    // C++ String Escaping
    // ============================================================

    fn cpp_string_literal(&self, value: &str) -> String {
        let mut result = String::with_capacity(value.len() + 2);

        result.push('"');

        for character in value.chars() {
            match character {
                '\\' => {
                    result.push_str("\\\\");
                }

                '"' => {
                    result.push_str("\\\"");
                }

                '\n' => {
                    result.push_str("\\n");
                }

                '\r' => {
                    result.push_str("\\r");
                }

                '\t' => {
                    result.push_str("\\t");
                }

                '\0' => {
                    result.push_str("\\0");
                }

                character => {
                    result.push(character);
                }
            }
        }

        result.push('"');

        result
    }

    // ============================================================
    // C++ Identifiers
    // ============================================================

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
            | "static_assert" | "static_cast" | "struct" | "synchronized" | "template" | "this"
            | "thread_local" | "throw" | "true" | "try" | "typedef" | "typeid" | "typename"
            | "union" | "unsigned" | "using" | "virtual" | "void" | "volatile" | "wchar_t"
            | "while" | "xor" | "xor_eq" => {
                format!("{}_alaco", name)
            }

            _ => name.to_string(),
        }
    }

    fn previous_identifier(&self, name: &str) -> String {
        format!("_alaco_previous_{}", self.cpp_identifier(name))
    }

    fn expression_identifier(&self, name: &str) -> String {
        if self.lifetime_variables.contains(name) {
            format!("{}.get()", self.cpp_identifier(name))
        } else {
            self.cpp_identifier(name)
        }
    }

    // ============================================================
    // Output
    // ============================================================

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
