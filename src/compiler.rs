use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    analyzer::Analyzer, ast::Program, codegen::cpp::CppGenerator, lexer::Lexer, parser::Parser,
};

pub struct Compiler;

impl Compiler {
    /// Lex and parse an Alaco source file.
    pub fn parse(source: &str) -> Result<Program, String> {
        let tokens = Lexer::new(source).tokenize()?;
        Parser::new(tokens).parse()
    }

    /// Run semantic analysis on an already-parsed program.
    pub fn analyze(program: &Program) -> Result<(), String> {
        Analyzer::analyze(program)
    }

    /// Parse and analyze Alaco source.
    ///
    /// This does not invoke Clang and does not generate any files.
    pub fn check(source: &str) -> Result<Program, String> {
        let program = Self::parse(source)?;
        Self::analyze(&program)?;
        Ok(program)
    }

    /// Parse, analyze, and generate C++ entirely in memory.
    pub fn generate(source: &str) -> Result<String, String> {
        let program = Self::check(source)?;
        Ok(CppGenerator::new().generate(&program))
    }

    /// Compile Alaco source directly to a native executable.
    ///
    /// The generated C++ is passed to clang++ through stdin.
    /// No intermediate `.cpp` file is created.
    pub fn build(source: &str, output: &Path) -> Result<(), String> {
        let cpp = Self::generate(source)?;

        let mut child = Command::new("clang++")
            .args(["-x", "c++", "-std=c++20", "-O2", "-pipe", "-", "-o"])
            .arg(output)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("failed to start clang++: {error}"))?;

        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| "failed to open clang++ stdin".to_string())?;

            stdin
                .write_all(cpp.as_bytes())
                .map_err(|error| format!("failed to send generated C++ to clang++: {error}"))?;
        }

        let status = child
            .wait()
            .map_err(|error| format!("failed to wait for clang++: {error}"))?;

        if !status.success() {
            return Err(format!("clang++ exited with {}", status));
        }

        Ok(())
    }

    /// Determine the default executable path for an Alaco source file.
    ///
    /// `examples/hello.aco` → `examples/hello`
    pub fn output_path(source: &Path) -> PathBuf {
        source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(source.file_stem().unwrap_or_default())
    }
}
