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
    fn public_io_functions_are_wrappers_around_private_runtime_hooks() {
        let cpp = Compiler::compile(
            r#"
            fn main() {
                println("hello");
            }
            "#,
            "io_test.aco",
        )
        .expect("a program using the public println function should compile");

        assert!(cpp.contains("void println(std::string value)"));
        assert!(cpp.contains("__builtin_println(value);"));
        assert!(cpp.contains("std::cout << value <<"));
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
    fn function_argument_type_errors_highlight_the_argument() {
        let error = Compiler::check(
            "fn main() {\n    println(42);\n}\n",
            "argument_type_test.aco",
        )
        .expect_err("println should reject an integer argument");

        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("expected String, found Int"));
        assert!(diagnostic.contains("argument_type_test.aco:2:13"));
    }
}
