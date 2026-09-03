use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use miette::{IntoDiagnostic, NamedSource, Result};

use crate::{
    analyzer::Analyzer,
    ast::{Item, Program},
    codegen::cpp::CppGenerator,
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

    /// Lex and parse an Alaco source file.
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

        let mut generator = CppGenerator::new();

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
            return Err(miette::miette!("clang++ exited with status {status}"));
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

    fn parse_unit(source: &str, filename: &str) -> Result<CompilationUnit> {
        let program = Self::parse(source, filename)?;
        let modules = Self::load_modules(&program)?;

        Ok(CompilationUnit { program, modules })
    }
}
