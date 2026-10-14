use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use miette::{IntoDiagnostic, NamedSource, Result};

use crate::{
    analyzer::Analyzer,
    ast::{Item, Program},
    codegen::cpp::CppCodegen,
    lexer::Lexer,
    parser::Parser,
    stdlib::{LoadedModule, Stdlib},
};

pub struct Compiler;

pub struct CompilationUnit {
    pub program: Program,
    pub modules: Vec<LoadedModule>,
}

impl Compiler {
    fn report(error: crate::error::AlacoError, filename: &str, source: &str) -> miette::Report {
        miette::Report::new(error)
            .with_source_code(NamedSource::new(filename.to_owned(), source.to_owned()))
    }

    pub fn parse(source: &str, filename: &str) -> Result<Program> {
        let mut lexer = Lexer::new(source);

        let tokens = lexer
            .tokenize()
            .map_err(|error| Self::report(error, filename, source))?;

        let mut parser = Parser::new(tokens);

        parser
            .parse()
            .map_err(|error| Self::report(error, filename, source))
    }

    /// Parse and analyze an Alaco source file.
    pub fn check(source: &str, filename: &str) -> Result<()> {
        let unit = Self::parse_unit(source, filename)?;

        Analyzer::new(&unit.modules)
            .analyze(&unit.program)
            .map_err(|error| Self::report(error, filename, source))?;

        Ok(())
    }

    /// Compile an Alaco source file into C++ source code.
    pub fn compile(source: &str, filename: &str) -> Result<String> {
        let unit = Self::parse_unit(source, filename)?;

        let ir = Analyzer::new(&unit.modules)
            .analyze(&unit.program)
            .map_err(|error| Self::report(error, filename, source))?;

        let generator = CppCodegen::new();

        Ok(generator.generate_with_modules(&ir, &unit.modules))
    }

    /// Build an Alaco source file into a native executable.
    pub fn build(source: &str, filename: &str, output: &Path) -> Result<()> {
        let cpp = Self::compile(source, filename)?;

        let cpp_path = output.with_extension("cpp");

        fs::write(&cpp_path, &cpp).into_diagnostic()?;

        let mut command = Command::new("clang++");
        command
            .arg("-std=c++20")
            .arg(&cpp_path)
            .arg("-o")
            .arg(output);
        #[cfg(target_os = "windows")]
        command.arg("-lwinhttp");
        #[cfg(target_os = "macos")]
        if cpp.contains("<CFNetwork/CFNetwork.h>") {
            command
                .arg("-framework")
                .arg("CoreFoundation")
                .arg("-framework")
                .arg("CFNetwork");
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        if cpp.contains("<openssl/ssl.h>") {
            command.arg("-lssl").arg("-lcrypto");
        }
        let result = command
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .into_diagnostic();

        let _ = fs::remove_file(&cpp_path);

        let status = result?;

        if !status.success() {
            return Err(miette::miette!(
                "clang++ exited with status {status}. This is surely a bug. Report it to Alaco developers."
            ));
        }

        Ok(())
    }

    fn parse_module_file(path: &Path) -> Result<Program> {
        let filename = path.to_string_lossy().into_owned();
        let source = fs::read_to_string(path)
            .map_err(|error| miette::miette!("cannot read module `{filename}`: {error}"))?;
        Self::parse(&source, &filename)
    }

    /// Resolve imported modules relative to the importing file, while keeping
    /// `std.*` imports rooted in the installed Alaco standard library.
    fn load_import_tree(
        program: &Program,
        importing_file: &Path,
        modules: &mut Vec<LoadedModule>,
        loaded: &mut HashSet<String>,
        active: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        let stdlib = Stdlib::new();
        let base_dir = importing_file.parent().unwrap_or_else(|| Path::new("."));

        for item in &program.items {
            let Item::Import(import) = item else {
                continue;
            };

            let is_std = import.path.first().map(String::as_str) == Some("std");
            let path = if is_std {
                stdlib.resolve(&import.path)
            } else {
                let mut path = base_dir.to_path_buf();
                for component in &import.path {
                    path.push(component);
                }
                path.set_extension("aco");
                path
            };

            let resolved = path.canonicalize().map_err(|error| {
                miette::miette!(
                    "cannot resolve imported module `{}` at `{}`: {error}",
                    import.path.join("."),
                    path.display()
                )
            })?;
            let alias = import
                .alias
                .clone()
                .or_else(|| import.path.last().cloned())
                .unwrap_or_else(|| import.path.join("."));
            if is_std
                && modules
                    .iter()
                    .any(|module| module.name == import.path.join(".") && module.alias == alias)
            {
                continue;
            }
            let identity = format!("{} as {}", resolved.display(), alias);

            if active.contains(&resolved) {
                return Err(miette::miette!(
                    "cyclic module import detected while loading `{}`",
                    import.path.join(".")
                ));
            }
            if loaded.contains(&identity) {
                continue;
            }
            if modules.iter().any(|module| module.alias == alias) {
                return Err(miette::miette!(
                    "module alias `{alias}` is already in use; import `{}` with a different alias",
                    import.path.join(".")
                ));
            }

            active.insert(resolved.clone());
            let module_program = if is_std {
                stdlib.load(&import.path)?.program
            } else {
                Self::parse_module_file(&resolved)?
            };

            // Dependencies are emitted before the module that uses them.
            Self::load_import_tree(&module_program, &resolved, modules, loaded, active)?;
            active.remove(&resolved);
            loaded.insert(identity);

            modules.push(LoadedModule {
                name: if is_std {
                    import.path.join(".")
                } else {
                    resolved.to_string_lossy().into_owned()
                },
                alias,
                program: module_program,
                ir: None,
                imported_symbols: import.symbols.clone(),
            });
        }

        Ok(())
    }

    /// Parse the source and load all standard libraries.
    ///
    /// `std.io`, `std.shell`, and `std.sql` are automatically available.
    /// Other modules must be explicitly imported.
    fn parse_unit(source: &str, filename: &str) -> Result<CompilationUnit> {
        let program = Self::parse(source, filename)?;

        let stdlib = Stdlib::new();

        let mut modules = Vec::new();

        // --------------------------------------------------------
        // Automatic standard libraries
        // --------------------------------------------------------

        for module_path in Stdlib::default_modules() {
            let module_path = module_path
                .iter()
                .map(|component| (*component).to_string())
                .collect::<Vec<_>>();

            let module = if module_path == ["std", "io"] {
                // Public print/println functions are part of the language's
                // default environment, so missing std.io must be reported as
                // a standard-library installation error instead of making
                // every call fail later as an unknown function.
                Some(stdlib.load(&module_path)?)
            } else {
                stdlib.load_if_exists(&module_path)?
            };

            if let Some(module) = module {
                modules.push(module);
            }
        }

        // --------------------------------------------------------
        // Explicit imports, including local modules and their dependencies.
        // --------------------------------------------------------
        let source_path = PathBuf::from(filename);
        let mut loaded = modules.iter().map(|module| module.name.clone()).collect();
        Self::load_import_tree(
            &program,
            &source_path,
            &mut modules,
            &mut loaded,
            &mut HashSet::new(),
        )?;

        // Lower each module with its own expression-type table. Source spans
        // are local to a file, so sharing the main program's table would let
        // unrelated expressions overwrite one another.
        for index in 0..modules.len() {
            let module_program = modules[index].program.clone();
            let module_ir = Analyzer::new(&modules)
                .analyze_module(&module_program)
                .map_err(|error| Self::report(error, &modules[index].name, ""))?;
            modules[index].ir = Some(module_ir);
        }

        Ok(CompilationUnit { program, modules })
    }
}

#[cfg(test)]
mod tests {
    use crate::analyzer::Analyzer;

    use super::Compiler;

    #[test]
    fn public_io_functions_accept_multiple_value_types() {
        let cpp = Compiler::compile(
            r#"
            fn main() {
                println("hello");
                print(42);
                println(3.5);
                println(true);
            }
            "#,
            "io_test.aco",
        )
        .expect("print functions should accept strings, integers, floats, and booleans");

        assert!(cpp.contains("template <typename T> static void println(const T& value)"));
        assert!(cpp.contains("static void println(bool value)"));
        assert!(cpp.contains("print(42);"));
        assert!(cpp.contains("println(true);"));
    }

    #[test]
    fn struct_destructuring_supports_shorthand_renaming_and_omitted_fields() {
        let cpp = Compiler::compile(
            r#"
            struct Point { x: Int, y: Int, label: String }
            fn main() {
                let point = Point { x: 7, y: 9, label: "ignored" };
                let Point { x, y: vertical } = point;
                println(x + vertical);
            }
            "#,
            "struct_destructuring_test.aco",
        )
        .expect("valid struct destructuring should compile");

        assert!(cpp.contains("const auto _alaco_destructure_"));
        assert!(cpp.contains("auto x = _alaco_destructure_"));
        assert!(cpp.contains("auto vertical = _alaco_destructure_"));
    }

    #[test]
    fn local_modules_support_relative_imports_and_aliases() {
        let source = include_str!("../examples/modules/main.aco");
        let cpp = Compiler::compile(source, "examples/modules/main.aco")
            .expect("local modules and aliases should compile");

        assert!(cpp.contains("namespace greetings {"));
        assert!(cpp.contains("namespace maths {"));
        assert!(cpp.contains("greetings::hello(\"Alaco\")"));
        assert!(cpp.contains("maths::square(9)"));
    }

    #[test]
    fn simple_local_import_resolves_to_a_sibling_aco_file() {
        let source = include_str!("../examples/modules/simple_import.aco");
        let cpp = Compiler::compile(source, "examples/modules/simple_import.aco")
            .expect("a simple import should resolve helpers to helpers.aco");

        assert!(cpp.contains("namespace helpers {"));
        assert!(cpp.contains("helpers::hello(\"Alaco\")"));
    }

    #[test]
    fn named_imports_expose_selected_functions_without_module_prefix() {
        let cpp = Compiler::compile(
            "import hello from helpers; fn main() { println(hello(\"Alaco\")); }",
            "examples/modules/named_import.aco",
        )
        .expect("named imports should resolve and type-check");

        assert!(cpp.contains("namespace helpers {"), "{cpp}");
        assert!(cpp.contains("using helpers::hello;"), "{cpp}");
        assert!(cpp.contains("hello(\"Alaco\")"), "{cpp}");
    }

    #[test]
    fn string_interpolation_emits_literal_and_variable_parts() {
        let cpp = Compiler::compile(
            "fn main() { let name = \"Alaco\"; let count = 3; println(\"Hello, ${name}! Count: ${count}\"); }",
            "string_interpolation.aco",
        )
        .expect("string interpolation should compile");

        assert!(cpp.contains("#include <sstream>"), "{cpp}");
        assert!(cpp.contains("_alaco_interp << name"), "{cpp}");
        assert!(cpp.contains("_alaco_interp << count"), "{cpp}");
    }

    #[test]
    fn struct_destructuring_rejects_unknown_fields_and_wrong_types() {
        let unknown_field = Compiler::check(
            "struct Point { x: Int } fn main() { let point = Point { x: 1 }; let Point { y } = point; }",
            "destructure_unknown_field.aco",
        );
        assert!(unknown_field.is_err(), "unknown fields must be rejected");

        let wrong_type = Compiler::check(
            "struct Point { x: Int } struct Other { x: Int } fn main() { let other = Other { x: 1 }; let Point { x } = other; }",
            "destructure_wrong_type.aco",
        );
        assert!(
            wrong_type.is_err(),
            "the pattern must match the value's struct type"
        );
    }

    #[test]
    fn print_formats_shell_futures_and_user_structs() {
        let cpp = Compiler::compile(
            r#"fn main() {
    let person = Person { name: "Ada", active: true };
    println(person);
    let result = shell { echo "Hi!" };
    print(result);
}
struct Person {
    name: String,
    active: Bool,
}
"#,
            "print_all_types_test.aco",
        )
        .expect("print should support user structs and shell futures");

        assert!(cpp.contains("operator<<(std::ostream& stream, const Person& value)"));
        assert!(cpp.contains("static void print(std::future<ShellResult>& value)"));
        assert!(cpp.contains("stream << (value.active ? \"true\" : \"false\")"));
    }

    #[test]
    fn compiler_intrinsics_are_not_public_functions() {
        let result = Compiler::check(
            r#"
            fn main() {
                __builtin_println("private");
            }
            "#,
            "private_intrinsic_test.aco",
        );

        assert!(
            result.is_err(),
            "private compiler hooks must not be callable"
        );
    }

    #[test]
    fn namespaced_module_calls_are_typed_and_lowered() {
        let cpp = Compiler::compile(
            r#"
            import std.math;

            fn main() {
                let result = math.max(2, 3);
                println("math works");
            }
            "#,
            "module_test.aco",
        )
        .expect("a valid namespaced std.math call should compile");

        assert!(cpp.contains("namespace math {"));
        assert!(cpp.contains("math::max(2, 3)"));

        let invalid = Compiler::check(
            r#"
            import std.math;

            fn main() {
                let result = math.max("wrong", 3);
            }
            "#,
            "invalid_module_test.aco",
        );

        assert!(invalid.is_err(), "module arguments must be type checked");
    }

    #[test]
    fn unknown_function_diagnostic_suggests_names_and_highlights_the_call() {
        let error = Compiler::check("fn main() {\n    pritnln(\"hello\");\n}\n", "typo_test.aco")
            .expect_err("a misspelled function should fail analysis");

        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("did you mean 'println'"));
        assert!(diagnostic.contains("typo_test.aco:2:5"));
        assert!(diagnostic.contains("pritnln"));
    }

    #[test]
    fn ordinary_function_argument_type_errors_highlight_the_argument() {
        let error = Compiler::check(
            "fn greet(name: String) {}\nfn main() {\n    greet(true);\n}\n",
            "argument_type_test.aco",
        )
        .expect_err("ordinary functions should still reject mismatched argument types");

        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("expected"));
        assert!(diagnostic.contains("String, found Bool"));
        assert!(diagnostic.contains("argument_type_test.aco:3:11"));
    }

    #[test]
    fn analyzer_diagnostics_highlight_the_relevant_source_expression() {
        let cases = [
            (
                "fn main() {\n    if 1 { }\n}\n",
                "condition_type_test.aco",
                "condition_type_test.aco:2:8",
                "if condition",
            ),
            (
                "fn main() {\n    let value: String = true;\n}\n",
                "initializer_type_test.aco",
                "initializer_type_test.aco:2:25",
                "type mismatch for variable 'value'",
            ),
            (
                "fn main() {\n    let value = 1;\n    let value = 2;\n}\n",
                "duplicate_variable_test.aco",
                "duplicate_variable_test.aco:3:5",
                "already declared in this scope",
            ),
            (
                "fn greet(name: Unknown) {}\nfn main() {}\n",
                "unknown_type_test.aco",
                "unknown_type_test.aco:1:16",
                "unknown type `Unknown`",
            ),
        ];

        for (source, filename, location, message) in cases {
            let error = Compiler::check(source, filename)
                .expect_err("invalid source should fail semantic analysis");
            let diagnostic = format!("{error:?}");
            assert!(
                diagnostic.contains(location),
                "expected {location}: {diagnostic}"
            );
            assert!(
                diagnostic.contains(message),
                "expected {message}: {diagnostic}"
            );
        }
    }

    #[test]
    fn common_semantic_errors_have_stable_codes_and_source_labels() {
        let error = Compiler::check(
            "fn main() {\n    println(missing);\n}\n",
            "undefined_variable_code_test.aco",
        )
        .expect_err("an undefined variable must fail analysis");

        let diagnostic = format!("{error:?}");
        assert!(
            diagnostic.contains("alaco::analysis::undefined_variable"),
            "missing stable diagnostic code: {diagnostic}"
        );
        assert!(
            diagnostic.contains("undefined_variable_code_test.aco:2:13"),
            "missing precise source label: {diagnostic}"
        );
    }

    #[test]
    fn logical_operators_accept_booleans_and_preserve_precedence() {
        let cpp = Compiler::compile(
            "fn main() { let result = true or false and false; println(result); }",
            "logical_operators_test.aco",
        )
        .expect("logical operators should accept boolean operands");

        assert!(cpp.contains("(true || (false && false))"));
    }

    #[test]
    fn logical_operators_reject_non_boolean_operands() {
        for source in [
            "fn main() { let result = 1 and 2; }",
            "fn main() { let result = true or 0; }",
        ] {
            assert!(
                Compiler::check(source, "invalid_logical_operator_test.aco").is_err(),
                "expected logical operator operands to be checked: {source}"
            );
        }
    }

    #[test]
    fn logical_not_compiles_and_requires_a_boolean_operand() {
        let source = "fn main() { let ready = true; println(!ready); println(!(2 < 1)); }";
        let cpp = Compiler::compile(source, "logical_not_test.aco")
            .expect("logical not should compile for boolean expressions");

        assert!(cpp.contains("(!ready)"));
        assert!(cpp.contains("(!(2 < 1))"));

        // Exercise the whole pipeline, not only the generated C++ text.
        let executable =
            std::env::temp_dir().join(format!("alaco_logical_not_test_{}", std::process::id()));
        Compiler::build(source, "logical_not_test.aco", &executable)
            .expect("generated C++ should compile successfully");
        let output = std::process::Command::new(&executable)
            .output()
            .expect("the compiled test program should run");
        let _ = std::fs::remove_file(&executable);
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "false\ntrue\n");

        let invalid = Compiler::check(
            "fn main() { let result = !42; }",
            "invalid_logical_not_test.aco",
        );
        assert!(
            invalid.is_err(),
            "logical not must reject non-boolean values"
        );
        assert!(
            format!("{:?}", invalid.unwrap_err()).contains("operand of logical not"),
            "the error should explain which operand is invalid"
        );
    }

    #[test]
    fn binary_type_diagnostics_highlight_the_whole_expression() {
        let error = Compiler::check(
            "fn main() {\n    let result = 1 + true;\n}\n",
            "binary_span_test.aco",
        )
        .expect_err("adding an integer and boolean must fail analysis");

        let diagnostic = format!("{error:?}");
        assert!(
            diagnostic.contains("binary_span_test.aco:2:18"),
            "the diagnostic should begin at the left operand, not the final token: {diagnostic}"
        );
        assert!(diagnostic.contains("1 + true"), "{diagnostic}");
    }

    #[test]
    fn calling_a_struct_field_is_rejected_during_analysis() {
        let result = Compiler::check(
            "struct User { name: String } fn main() { let user = User { name: \"Ada\" }; user.name(); }",
            "field_call_test.aco",
        );

        assert!(result.is_err(), "struct fields must not be callable");
        let diagnostic = format!("{:?}", result.unwrap_err());
        assert!(diagnostic.contains("not a callable method"), "{diagnostic}");
    }

    #[test]
    fn shell_future_get_is_callable_without_arguments() {
        let valid = Compiler::check(
            "fn main() { let result = shell { echo hi }.get(); println(result.success); }",
            "shell_get_test.aco",
        );
        assert!(
            valid.is_ok(),
            "shell future get() should remain supported: {valid:?}"
        );

        let invalid = Compiler::check(
            "fn main() { let result = shell { echo hi }.get(1); }",
            "shell_get_arguments_test.aco",
        );
        assert!(
            invalid.is_err(),
            "shell future get() must not accept arguments"
        );
    }

    #[test]
    fn non_void_functions_must_return_on_every_path() {
        let missing_return = Compiler::check(
            "fn choose(flag: Bool) -> Int { if flag { return 1; } } fn main() {}",
            "missing_return_test.aco",
        );
        assert!(missing_return.is_err(), "a missing return path must fail");
        assert!(
            format!("{:?}", missing_return.unwrap_err()).contains("may finish without returning")
        );

        let complete_return = Compiler::check(
            "fn choose(flag: Bool) -> Int { if flag { return 1; } else { return 2; } } fn main() {}",
            "complete_return_test.aco",
        );
        assert!(
            complete_return.is_ok(),
            "both branches return: {complete_return:?}"
        );
    }

    #[test]
    fn standard_math_module_exposes_common_integer_helpers() {
        let cpp = Compiler::compile(
            r#"
            import std.math;

            fn main() {
                println(math.abs(-5));
                println(math.square(4));
                println(math.clamp(12, 0, 10));
                println(math.is_even(8));
                println(math.is_odd(7));
            }
            "#,
            "math_helpers_test.aco",
        )
        .expect("standard math helpers should type-check and compile");

        assert!(cpp.contains("math::abs("));
        assert!(cpp.contains("math::square(4)"));
        assert!(cpp.contains("math::clamp(12, 0, 10)"));
        assert!(cpp.contains("math::is_even(8)"));
        assert!(cpp.contains("math::is_odd(7)"));
    }

    #[test]
    fn standard_string_module_exposes_string_helpers() {
        let cpp = Compiler::compile(
            r#"
            import std.string;

            fn main() {
                println(string.concat("Al", "aco"));
                println(string.is_empty(""));
                println(string.repeat_string("ha", 3));
            }
            "#,
            "string_helpers_test.aco",
        )
        .expect("standard string helpers should type-check and compile");

        assert!(cpp.contains("string::concat(\"Al\", \"aco\")"));
        assert!(cpp.contains("string::is_empty(\"\")"));
        assert!(cpp.contains("string::repeat_string(\"ha\", 3)"));
    }

    #[test]
    fn lowercase_builtin_type_names_generate_valid_cpp_types() {
        let cpp = Compiler::compile(
            "fn hello(name: string) { println(name); } fn main() { hello(\"Alaco\"); }",
            "lowercase_builtin_types.aco",
        )
        .expect("lowercase primitive type aliases should compile");

        assert!(cpp.contains("void hello(std::string name)"), "{cpp}");
    }

    #[test]
    fn additional_standard_modules_expose_geometry_stats_units_and_validation() {
        let cpp = Compiler::compile(
            r#"
            import std.geometry;
            import std.stats;
            import std.units;
            import std.validation;

            fn main() {
                println(geometry.rectangle_area(4, 5));
                println(geometry.manhattan_distance(0, 0, 3, 4));
                println(stats.average3(3, 6, 9));
                println(units.celsius_to_fahrenheit(100.0));
                println(validation.is_non_empty("Alaco"));
            }
            "#,
            "additional_stdlib_test.aco",
        )
        .expect("additional standard modules should type-check and compile");

        assert!(cpp.contains("geometry::rectangle_area(4, 5)"));
        assert!(cpp.contains("stats::average3(3, 6, 9)"));
        assert!(cpp.contains("units::celsius_to_fahrenheit("));
        assert!(cpp.contains("validation::is_non_empty(\"Alaco\")"));
    }

    #[test]
    fn runtime_standard_modules_emit_native_filesystem_environment_and_path_support() {
        let cpp = Compiler::compile(
            r#"
            import std.fs;
            import std.env;
            import std.path;

            fn main() {
                println(fs.read_text("notes.txt"));
                println(fs.file_size("notes.txt"));
                println(fs.copy("notes.txt", "backup.txt"));
                println(fs.write_text("notes.txt", "hello"));
                println(env.get("HOME"));
                println(env.current_dir());
                println(path.join("folder", "file.txt"));
                println(path.basename("folder/file.txt"));
            }
            "#,
            "runtime_stdlib_test.aco",
        )
        .expect("runtime standard modules should type-check and compile");

        assert!(cpp.contains("#include <filesystem>"));
        assert!(cpp.contains("#include <fstream>"));
        assert!(cpp.contains("static std::string __builtin_fs_read_text"));
        assert!(cpp.contains("static long long __builtin_fs_file_size"));
        assert!(cpp.contains("static bool __builtin_fs_copy"));
        assert!(cpp.contains("static bool __builtin_env_set"));
        assert!(cpp.contains("static std::string __builtin_path_join"));
        assert!(cpp.contains("fs::read_text(\"notes.txt\")"));
        assert!(cpp.contains("env::current_dir()"));
        assert!(cpp.contains("path::basename(\"folder/file.txt\")"));

        let private_hook = Compiler::check(
            "fn main() { __builtin_fs_exists(\"notes.txt\"); }",
            "private_fs_hook_test.aco",
        );
        assert!(private_hook.is_err(), "runtime hooks must remain private");
    }

    #[test]
    fn misspelled_namespaced_function_suggests_the_correct_name() {
        let error = Compiler::check(
            "import std.math;\nfn main() {\n    math.sqaure(4);\n}\n",
            "module_typo_test.aco",
        )
        .expect_err("unknown module functions should produce a diagnostic");

        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("math.square"), "{diagnostic}");
        assert!(
            diagnostic.contains("module_typo_test.aco:3:10"),
            "{diagnostic}"
        );
    }

    #[test]
    fn misspelled_struct_literal_field_suggests_the_correct_name() {
        let error = Compiler::check(
            "struct User { name: String }\nfn main() {\n    let user = User { naem: \"Ada\" };\n}\n",
            "struct_field_typo_test.aco",
        )
        .expect_err("unknown struct fields should produce a diagnostic");

        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("did you mean 'name'?"), "{diagnostic}");
    }

    #[test]
    fn misspelled_struct_member_suggests_the_correct_name_and_highlights_it() {
        let error = Compiler::check(
            "struct User { name: String }\nfn main() {\n    let user = User { name: \"Ada\" };\n    println(user.naem);\n}\n",
            "struct_member_typo_test.aco",
        )
        .expect_err("unknown struct members should produce a diagnostic");

        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("did you mean 'name'?"), "{diagnostic}");
        assert!(
            diagnostic.contains("struct_member_typo_test.aco:4:18"),
            "{diagnostic}"
        );
    }

    #[test]
    fn pipes_pass_the_left_value_as_the_first_argument() {
        let cpp = Compiler::compile(
            "fn double(value: Int) -> Int { return value * 2; } fn add(left: Int, right: Int) -> Int { return left + right; } fn main() { let result = 4 |> double |> add(3); println(result); }",
            "pipe_test.aco",
        )
        .expect("valid pipe expressions should compile");

        assert!(cpp.contains("double_alaco(4)"), "{cpp}");
        assert!(cpp.contains("add(double_alaco(4), 3)"), "{cpp}");
    }

    #[test]
    fn pipe_input_expression_is_evaluated_once() {
        let source = r#"
            fn source() -> Int { println("once"); return 4; }
            fn double(value: Int) -> Int { return value * 2; }
            fn main() { println(source() |> double); }
        "#;
        let output_path =
            std::env::temp_dir().join(format!("alaco_pipe_once_{}", std::process::id()));
        Compiler::build(source, "pipe_once_test.aco", &output_path)
            .expect("side-effecting pipe input should compile");

        let output = std::process::Command::new(&output_path)
            .output()
            .expect("generated test executable should run");
        let _ = std::fs::remove_file(&output_path);

        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "once\n8\n");
    }

    #[test]
    fn pipes_reject_type_mismatches_at_the_pipe_site() {
        let error = Compiler::check(
            "fn takes_text(value: String) -> Int { return 1; } fn main() { let result = 42 |> takes_text; }",
            "pipe_type_test.aco",
        )
        .expect_err("pipe input types must be checked against function parameters");

        assert!(format!("{error:?}").contains("expected String, found Int"));
    }

    #[test]
    fn undefined_variable_diagnostics_suggest_close_names() {
        let error = Compiler::check(
            "fn main() { let message = \"hello\"; println(mesage); }",
            "variable_suggestion.aco",
        )
        .expect_err("misspelled variables should be rejected");

        let rendered = format!("{error:?}");
        assert!(rendered.contains("did you mean 'message'?"), "{rendered}");
    }

    #[test]
    fn match_identifier_patterns_must_resolve_and_match_types() {
        let unknown = Compiler::check(
            "fn main() { let value = 1; match value { missing => { } } }",
            "unknown_match_pattern.aco",
        )
        .expect_err("unknown match pattern identifiers must not reach C++ generation");
        assert!(
            format!("{unknown:?}").contains("undefined variable `missing`"),
            "{unknown:?}"
        );

        let mismatched = Compiler::check(
            "fn main() { let value = 1; let text = \"one\"; match value { text => { } } }",
            "mismatched_match_pattern.aco",
        )
        .expect_err("match patterns must be type-compatible with the matched expression");
        assert!(format!("{mismatched:?}").contains("matched expression has type Int"));
    }

    #[test]
    fn semantic_analysis_produces_typed_ir_signatures() {
        let program = Compiler::parse(
            "fn greet(name: String) -> String { return name; } fn main() { let value = greet(\"Alaco\"); }",
            "typed_ir.aco",
        )
        .expect("source should parse");
        let ir = Analyzer::new(&[])
            .analyze(&program)
            .expect("valid source should lower to IR");

        let greet = ir.function("greet").expect("IR should contain greet");
        assert_eq!(greet.parameters.len(), 1);
        assert_eq!(greet.parameters[0].name, "name");
        assert_eq!(greet.parameters[0].ty, crate::ast::Type::String);
        assert_eq!(greet.return_type, crate::ast::Type::String);
        let crate::ir::IrStmt::Return(Some(return_value), _) = &greet.body.statements[0] else {
            panic!("IR should contain the function's return statement");
        };
        assert_eq!(return_value.ty, crate::ast::Type::String);
    }

    #[test]
    fn compilation_folds_constant_integer_arithmetic() {
        let cpp = Compiler::compile(
            "fn calculate() -> Int { return (6 * 7) + 1; } fn main() { println(calculate()); }",
            "constant_folding.aco",
        )
        .expect("constant integer arithmetic should compile");

        assert!(cpp.contains("return 43;"), "{cpp}");
        assert!(!cpp.contains("(6 * 7)"), "{cpp}");
    }

    #[test]
    fn remainder_requires_integer_operands() {
        let error = Compiler::check(
            "fn main() { let result = 4.5 % 2; }",
            "remainder_type_test.aco",
        )
        .expect_err("floating-point remainder must be rejected until supported explicitly");

        assert!(format!("{error:?}").contains("requires Int operands"));
    }

    #[test]
    fn equality_rejects_void_and_struct_values() {
        let struct_error = Compiler::check(
            "struct Point { x: Int } fn main() { let left = Point { x: 1 }; let right = Point { x: 1 }; let same = left == right; }",
            "struct_equality_test.aco",
        );
        assert!(
            struct_error.is_err(),
            "struct equality needs a defined policy"
        );

        let void_error = Compiler::check(
            "fn noop() {} fn main() { let same = noop() == noop(); }",
            "void_equality_test.aco",
        );
        assert!(void_error.is_err(), "Void is not a comparable value type");
    }

    #[test]
    fn type_safety_rejects_invalid_return_paths_and_void_values() {
        let missing_return = Compiler::check(
            "fn choose(flag: Bool) -> Int { if flag { return 1; } } fn main() {}",
            "missing_return_path.aco",
        );
        assert!(
            missing_return.is_err(),
            "non-Void functions must return on every path"
        );

        let void_return_value = Compiler::check(
            "fn nothing() {} fn main() { let value = nothing(); }",
            "void_value.aco",
        );
        assert!(
            void_return_value.is_err(),
            "Void calls cannot be stored as values"
        );

        let wrong_struct_field = Compiler::check(
            "struct Config { enabled: Bool } fn main() { let config = Config { enabled: 1 }; }",
            "wrong_struct_field.aco",
        );
        assert!(
            wrong_struct_field.is_err(),
            "struct fields must enforce their declared types"
        );
    }

    #[test]
    fn inferred_variables_are_immutable_by_default_and_keep_their_inferred_type() {
        let inferred = Compiler::check(
            "fn main() { let count = 1; let title = \"Alaco\"; let enabled = true; println(count); println(title); println(enabled); }",
            "inferred_variables.aco",
        );
        assert!(
            inferred.is_ok(),
            "types should be inferred from initializers"
        );

        let immutable_assignment = Compiler::check(
            "fn main() { let count = 1; count = 2; }",
            "immutable_by_default.aco",
        );
        assert!(
            immutable_assignment.is_err(),
            "let bindings must be immutable unless explicitly marked mut"
        );

        let type_change = Compiler::check(
            "fn main() { let mut value = 1; value = \"now a string\"; }",
            "inferred_type_is_stable.aco",
        );
        assert!(
            type_change.is_err(),
            "mut permits reassignment but must not disable static type checking"
        );

        let mutable_same_type = Compiler::check(
            "fn main() { let mut count = 1; count = count + 1; }",
            "mutable_inferred_variable.aco",
        );
        assert!(
            mutable_same_type.is_ok(),
            "mutable inferred variables should allow same-type reassignment"
        );
    }

    #[test]
    fn struct_fields_stay_typed_and_mutation_requires_a_mutable_binding() {
        let immutable_field = Compiler::check(
            "struct Config { port: Int } fn main() { let config = Config { port: 80 }; config.port = 443; }",
            "immutable_struct_field.aco",
        );
        assert!(
            immutable_field.is_err(),
            "fields cannot be mutated through immutable bindings"
        );

        let mutable_field = Compiler::check(
            "struct Config { port: Int } fn main() { let mut config = Config { port: 80 }; config.port = 443; }",
            "mutable_struct_field.aco",
        );
        assert!(
            mutable_field.is_ok(),
            "fields may be mutated through mutable bindings"
        );

        let wrong_field_assignment = Compiler::check(
            "struct Config { port: Int } fn main() { let mut config = Config { port: 80 }; config.port = \"443\"; }",
            "wrong_struct_field_assignment.aco",
        );
        assert!(
            wrong_field_assignment.is_err(),
            "field assignments must match the field's declared type"
        );
    }

    #[test]
    fn void_values_cannot_be_bound_or_assigned_through_temporaries() {
        let void_binding = Compiler::check(
            "fn noop() {} fn main() { let value = noop(); }",
            "void_binding_test.aco",
        );
        assert!(void_binding.is_err(), "Void cannot initialize a local");

        let temporary_member = Compiler::check(
            "struct Point { x: Int } fn make_point() -> Point { return Point { x: 1 }; } fn main() { make_point().x = 2; }",
            "temporary_member_assignment_test.aco",
        );
        assert!(
            temporary_member.is_err(),
            "a temporary is not an assignment target"
        );
    }

    #[test]
    fn batteries_included_modules_compile_and_run() {
        let source = r#"
            import std.json;
            import std.toml;
            import std.http;
            import std.timing;

            fn double(value: Int) -> Int { return value * 2; }

            fn main() {
                println(json.is_valid("{\"ok\":true,\"n\":3}"));
                println(json.is_valid("{bad}"));
                println(json.quote("a\nb"));
                println(json.has_key("{\"n\":3}", "n"));
                println(json.get_int("{\"n\":3}", "n", 99));
                println(json.get_string("{\"name\":\"Alaco\"}", "name", "missing"));
                println(json.get_bool("{\"ok\":true}", "ok", false));
                println(toml.is_valid("[app]\nname = \"Alaco\""));
                println(toml.is_valid("count = nonsense"));
                println(toml.get_int("count = 4", "count", 99));
                println(http.is_url("https://example.com"));
                println(21 |> double);
                println(timing.unix_seconds() > 0);
            }
        "#;

        let output_path = std::env::temp_dir().join(format!(
            "alaco_batteries_integration_{}",
            std::process::id()
        ));
        Compiler::build(source, "batteries_integration_test.aco", &output_path)
            .expect("the standard library runtime should compile to C++");

        let output = std::process::Command::new(&output_path)
            .output()
            .expect("the generated test executable should run");
        let _ = std::fs::remove_file(&output_path);

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(
                "true\nfalse\n\"a\\nb\"\ntrue\n3\nAlaco\ntrue\ntrue\nfalse\n4\ntrue\n42\ntrue"
            ),
            "{stdout}"
        );
    }

    #[test]
    fn json_rejects_non_json_whitespace() {
        let source = format!(
            "import std.json; fn main() {{ println(json.is_valid(\"{}\")); }}",
            '\u{000b}'
        );
        let output_path =
            std::env::temp_dir().join(format!("alaco_json_whitespace_{}", std::process::id()));
        Compiler::build(&source, "json_whitespace_test.aco", &output_path)
            .expect("JSON whitespace test should compile");

        let output = std::process::Command::new(&output_path)
            .output()
            .expect("generated test executable should run");
        let _ = std::fs::remove_file(&output_path);

        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "false");
    }
}
