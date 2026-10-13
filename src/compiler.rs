use std::{
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

        let mut parser = Parser::new(tokens, filename.to_owned(), source.to_owned());

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

        Analyzer::new(&unit.modules)
            .analyze(&unit.program)
            .map_err(|error| Self::report(error, filename, source))?;

        let generator = CppCodegen::new();

        Ok(generator.generate_with_modules(&unit.program, &unit.modules))
    }

    /// Build an Alaco source file into a native executable.
    pub fn build(source: &str, filename: &str, output: &Path) -> Result<()> {
        let cpp = Self::compile(source, filename)?;

        let cpp_path = output.with_extension("cpp");

        fs::write(&cpp_path, cpp).into_diagnostic()?;

        let result = Command::new("clang++")
            .arg("-std=c++20")
            .arg(&cpp_path)
            .arg("-o")
            .arg(output)
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

    /// Get the default executable path for an Alaco source file.
    ///
    /// `hello.aco` -> `hello`
    pub fn output_path(file: &Path) -> PathBuf {
        let stem = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("alaco_program");

        file.parent().unwrap_or_else(|| Path::new(".")).join(stem)
    }

    /// Load explicitly imported standard-library modules.
    fn load_modules(program: &Program) -> Result<Vec<LoadedModule>> {
        let stdlib = Stdlib::new();
        let mut modules = Vec::new();

        for item in &program.items {
            let Item::Import(import) = item else {
                continue;
            };

            // Currently only the standard library is supported.
            if import.path.first().map(String::as_str) != Some("std") {
                continue;
            }

            let module = stdlib.load(&import.path)?;

            modules.push(module);
        }

        Ok(modules)
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
        // Explicit imports
        // --------------------------------------------------------

        let explicit_modules = Self::load_modules(&program)?;

        for module in explicit_modules {
            // Don't load a module twice if it is both automatic
            // and explicitly imported.
            if !modules.iter().any(|existing| existing.name == module.name) {
                modules.push(module);
            }
        }

        Ok(CompilationUnit { program, modules })
    }
}

#[cfg(test)]
mod tests {
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
}
