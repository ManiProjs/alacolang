use std::{
    env, fs,
    path::{Path, PathBuf},
};

use miette::{NamedSource, Report, Result};

use crate::{ast::Program, error::AlacoError, lexer::Lexer, parser::Parser};

const STDLIB_ENV: &str = "ALACO_STDLIB_DIR";

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub name: String,
    pub alias: String,
    pub program: Program,
}

#[derive(Debug, Clone)]
pub struct Stdlib {
    root: PathBuf,
}

impl Stdlib {
    pub fn new() -> Self {
        let root = env::var_os(STDLIB_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_root);

        Self { root }
    }

    fn default_root() -> PathBuf {
        // When running a compiler built from this checkout, use the
        // repository's standard library automatically. Installed builds can
        // continue using the user's installed standard library directory.
        let development_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("std");
        if development_root.join("std").join("io.aco").is_file() {
            return development_root;
        }

        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".alaco").join("stdlib"))
            .unwrap_or_else(|| PathBuf::from(".alaco/stdlib"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Standard libraries that are automatically available to every Alaco
    /// program.
    ///
    /// These modules do not need an explicit `import`.
    pub fn default_modules() -> &'static [&'static [&'static str]] {
        &[&["std", "io"], &["std", "shell"], &["std", "sql"]]
    }

    pub fn resolve(&self, module: &[String]) -> PathBuf {
        let mut path = self.root.clone();

        for component in module {
            path.push(component);
        }

        path.set_extension("aco");
        path
    }

    pub fn exists(&self, module: &[String]) -> bool {
        self.resolve(module).is_file()
    }

    pub fn load_if_exists(&self, module: &[String]) -> Result<Option<LoadedModule>> {
        if !self.exists(module) {
            return Ok(None);
        }

        self.load(module).map(Some)
    }

    pub fn load(&self, module: &[String]) -> Result<LoadedModule> {
        let name = module.join(".");
        let alias = module.last().cloned().unwrap_or_else(|| name.clone());

        let path = self.resolve(module);
        let filename = path.to_string_lossy().into_owned();

        let source = fs::read_to_string(&path).map_err(|source| {
            Report::new(AlacoError::StdlibLoadError {
                module: name.clone(),
                source,
            })
        })?;

        let source_for_report = source.clone();

        let mut lexer = Lexer::new(&source);

        let tokens = lexer.tokenize().map_err(|error| {
            Report::new(error).with_source_code(NamedSource::new(
                filename.clone(),
                source_for_report.clone(),
            ))
        })?;

        let mut parser = Parser::new(tokens);

        let program = parser.parse().map_err(|error| {
            Report::new(error)
                .with_source_code(NamedSource::new(filename.clone(), source_for_report))
        })?;

        Ok(LoadedModule {
            name,
            alias,
            program,
        })
    }
}
