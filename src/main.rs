mod analyzer;
mod ast;
mod codegen;
mod compiler;
mod error;
mod features;
mod ir;
mod ir_validate;
mod language;
mod lexer;
mod lsp;
mod manifest;
mod optimizer;
mod parser;
mod stdlib;
mod token;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use miette::Result;
use owo_colors::OwoColorize;

use compiler::Compiler;

#[derive(Parser, Debug)]
#[command(
    name = "alaco",
    version,
    about = "The Alaco programming language toolchain",
    long_about = "The everything tool for Alaco: create projects, check code, build and run programs, execute test programs, clean build artifacts, and diagnose your development environment.",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CommandKind>,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    /// Create a new Alaco project.
    New {
        /// Name of the project and its new directory.
        name: String,
    },

    /// Initialize an Alaco project in an existing directory.
    Init {
        /// Directory to initialize (defaults to the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Check an Alaco source file.
    Check { file: Option<PathBuf> },

    /// Explain an error in an Alaco source file.
    Explain { file: Option<PathBuf> },

    /// Build an Alaco source file.
    Build {
        file: Option<PathBuf>,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Compile and run an Alaco program.
    Run { file: Option<PathBuf> },

    /// Compile and run test programs from tests/ (or a supplied file/directory).
    Test {
        /// A .aco test file or directory containing .aco test files (defaults to tests/).
        #[arg(default_value = "tests")]
        path: PathBuf,
    },

    /// Remove generated project build artifacts from target/.
    Clean,

    /// Check whether the Alaco build tools are installed correctly.
    Doctor,

    /// Validate and display the current package manifest.
    Manifest,

    /// Start the Alaco Language Server Protocol (LSP) server over stdio.
    Lsp,
}

fn main() -> ExitCode {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .unicode(true)
                .terminal_links(true)
                .build(),
        )
    }));

    match run_cli() {
        Ok(code) => code,

        Err(error) => {
            eprintln!("{error:?}");
            ExitCode::FAILURE
        }
    }
}

fn run_cli() -> Result<ExitCode> {
    let cli = Cli::parse();

    match cli.command {
        Some(CommandKind::New { name }) => new_project(&name),

        Some(CommandKind::Init { path }) => init_project(&path),

        Some(CommandKind::Check { file }) => check(&resolve_source(file)?),

        Some(CommandKind::Explain { file }) => explain(&resolve_source(file)?),

        Some(CommandKind::Build { file, output }) => {
            build(&resolve_source(file)?, output.as_deref())
        }

        Some(CommandKind::Run { file }) => run(&resolve_source(file)?),

        Some(CommandKind::Test { path }) => test(&path),

        Some(CommandKind::Clean) => clean(),

        Some(CommandKind::Doctor) => doctor(),

        Some(CommandKind::Manifest) => show_manifest(),

        Some(CommandKind::Lsp) => lsp::run()
            .map(|()| ExitCode::SUCCESS)
            .map_err(|error| miette::miette!("LSP transport error: {error}")),

        None => unreachable!("Clap requires a subcommand"),
    }
}

fn resolve_source(file: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(file) = file {
        return Ok(file);
    }

    let entry = if Path::new("Alaco.toml").is_file() {
        crate::manifest::Manifest::load(Path::new("Alaco.toml"))?.entry
    } else {
        PathBuf::from("src/main.aco")
    };
    if entry.is_file() {
        return Ok(entry);
    }

    Err(miette::miette!(
        "no source file was provided and the package entry '{}' was not found; check Alaco.toml or pass a source file",
        entry.display()
    ))
}

fn new_project(name: &str) -> Result<ExitCode> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
    {
        return Err(miette::miette!(
            "invalid project name '{name}'; use a simple directory name"
        ));
    }

    let root = PathBuf::from(name);
    if root.exists() {
        return Err(miette::miette!(
            "cannot create project '{}': the path already exists",
            root.display()
        ));
    }

    create_project_files(&root)?;
    println!("  {} Created project `{name}`", "✓".green());
    println!("\n  Next steps:\n    cd {name}\n    alaco run");

    Ok(ExitCode::SUCCESS)
}

fn init_project(root: &Path) -> Result<ExitCode> {
    if !root.is_dir() {
        return Err(miette::miette!(
            "project directory '{}' does not exist",
            root.display()
        ));
    }

    let entry = root.join("src/main.aco");
    if entry.exists() {
        return Err(miette::miette!(
            "'{}' already exists; refusing to overwrite it",
            entry.display()
        ));
    }

    create_project_files(root)?;
    println!(
        "  {} Initialized Alaco project in {}",
        "✓".green(),
        root.display()
    );

    Ok(ExitCode::SUCCESS)
}

fn create_project_files(root: &Path) -> Result<()> {
    let source_dir = root.join("src");
    fs::create_dir_all(&source_dir)
        .map_err(|error| miette::miette!("failed to create '{}': {error}", source_dir.display()))?;

    let project_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("alaco-project");
    let package_name = project_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let manifest = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\nentry = \"src/main.aco\"\n\n[dependencies]\n",
        package_name
    );
    write_if_missing(&root.join("Alaco.toml"), &manifest)?;

    fs::write(
        source_dir.join("main.aco"),
        "fn main() {\n    println(\"Hello, Alaco!\");\n}\n",
    )
    .map_err(|error| miette::miette!("failed to write project entry point: {error}"))?;

    write_if_missing(&root.join(".gitignore"), "/target/\n")?;
    write_if_missing(
        &root.join("README.md"),
        "# Alaco project\n\nRun the program with `alaco run`.\n\nCheck it with `alaco check`, or build it with `alaco build`.\n",
    )?;

    Ok(())
}

fn write_if_missing(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    fs::write(path, contents)
        .map_err(|error| miette::miette!("failed to write '{}': {error}", path.display()))
}

fn doctor() -> Result<ExitCode> {
    let mut failed = false;
    println!("Alaco {} environment check\n", env!("CARGO_PKG_VERSION"));

    match Command::new("clang++").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("clang++ found");
            println!("  {} C++ compiler: {first_line}", "✓".green());
        }
        Ok(_) => {
            failed = true;
            println!("  {} clang++ is installed but failed to run", "✗".red());
        }
        Err(error) => {
            failed = true;
            println!("  {} clang++ not found ({error})", "✗".red());
            println!("    Install LLVM/Clang and ensure `clang++` is on PATH.");
        }
    }

    let stdlib = crate::stdlib::Stdlib::new();
    let stdlib_root = stdlib.root();
    if stdlib_root.is_dir() {
        println!(
            "  {} Standard library: {}",
            "✓".green(),
            stdlib_root.display()
        );
    } else {
        failed = true;
        println!(
            "  {} Standard library not found: {}",
            "✗".red(),
            stdlib_root.display()
        );
    }

    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn show_manifest() -> Result<ExitCode> {
    let path = PathBuf::from("Alaco.toml");
    let manifest = crate::manifest::Manifest::load(&path)?;
    println!("{} {}", manifest.name.bold(), manifest.version);
    println!("  Edition: {}", manifest.edition);
    println!("  Entry:   {}", manifest.entry.display());
    println!("  Dependencies: {}", manifest.dependencies.len());
    for (name, dependency) in &manifest.dependencies {
        println!("    {name} = {dependency}");
    }
    Ok(ExitCode::SUCCESS)
}

fn test(path: &Path) -> Result<ExitCode> {
    let tests = discover_tests(path)?;
    if tests.is_empty() {
        return Err(miette::miette!(
            "no .aco tests found in '{}'; add test programs to tests/ or pass a test file",
            path.display()
        ));
    }

    println!("  {} {} test program(s)", "Testing".bold(), tests.len());
    let started = Instant::now();
    let mut passed = 0usize;
    let mut failed = 0usize;

    for test_file in &tests {
        let label = test_file.display().to_string();
        let output = test_output_path(test_file);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                miette::miette!(
                    "failed to create test output directory '{}': {error}",
                    parent.display()
                )
            })?;
        }

        let result = (|| -> Result<bool> {
            let source = read_source(test_file)?;
            Compiler::build(&source, &label, &output)?;
            let status = Command::new(executable_path(&output))
                .status()
                .map_err(|error| {
                    miette::miette!("failed to run test '{}': {error}", test_file.display())
                })?;
            Ok(status.success())
        })();
        cleanup(&output);

        match result {
            Ok(true) => {
                passed += 1;
                println!("  {} {label}", "PASS".green());
            }
            Ok(false) => {
                failed += 1;
                println!(
                    "  {} {label} (test program exited unsuccessfully)",
                    "FAIL".red()
                );
            }
            Err(error) => {
                failed += 1;
                println!("  {} {label}", "FAIL".red());
                eprintln!("    {error:?}");
            }
        }
    }

    println!("\n  {passed} passed; {failed} failed");
    print_finished(started.elapsed());

    Ok(if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn discover_tests(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        if path.extension().and_then(|extension| extension.to_str()) != Some("aco") {
            return Err(miette::miette!(
                "test file must use the .aco extension: '{}'",
                path.display()
            ));
        }
        return Ok(vec![path.to_path_buf()]);
    }

    if !path.is_dir() {
        return Err(miette::miette!(
            "test path '{}' does not exist or is not a directory",
            path.display()
        ));
    }

    let mut tests = fs::read_dir(path)
        .map_err(|error| {
            miette::miette!(
                "failed to read test directory '{}': {error}",
                path.display()
            )
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|entry| {
            entry.is_file()
                && entry.extension().and_then(|extension| extension.to_str()) == Some("aco")
        })
        .collect::<Vec<_>>();
    tests.sort();
    Ok(tests)
}

fn test_output_path(file: &Path) -> PathBuf {
    let root = Path::new("target");
    let stem = file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("test");
    root.join(format!(".alaco-test-{stem}-{}", std::process::id()))
}

fn clean() -> Result<ExitCode> {
    clean_target(Path::new("target"))
}

fn clean_target(target: &Path) -> Result<ExitCode> {
    if target.exists() {
        fs::remove_dir_all(target).map_err(|error| {
            miette::miette!(
                "failed to remove build directory '{}': {error}",
                target.display()
            )
        })?;
        println!("  {} Removed {}", "✓".green(), target.display());
    } else {
        println!("  {} Nothing to clean", "✓".green());
    }
    Ok(ExitCode::SUCCESS)
}

fn check(file: &Path) -> Result<ExitCode> {
    let source = read_source(file)?;
    let filename = file.display().to_string();

    let start = Instant::now();

    println!("  {} {}", "Checking".bold(), file.display());

    Compiler::check(&source, &filename)?;

    println!("  {} Parsed", "✓".green());
    println!("  {} Analyzed", "✓".green());

    print_finished(start.elapsed());

    Ok(ExitCode::SUCCESS)
}

fn explain(file: &Path) -> Result<ExitCode> {
    let source = read_source(file)?;
    let filename = file.display().to_string();

    match Compiler::check(&source, &filename) {
        Ok(()) => {
            println!("  {} No errors found.", "✓".green());

            Ok(ExitCode::SUCCESS)
        }

        Err(error) => {
            if let Some(alaco_error) = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<crate::error::AlacoError>())
            {
                if let Some((why, fix)) = alaco_error.explanation() {
                    eprintln!("\nWhy: {why}\nSuggestion: {fix}");
                }
            }

            Err(error)
        }
    }
}

fn build(file: &Path, requested_output: Option<&Path>) -> Result<ExitCode> {
    let source = read_source(file)?;
    let filename = file.display().to_string();

    let output = requested_output
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(file));

    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            miette::miette!(
                "failed to create output directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let start = Instant::now();

    println!("  {} {}", "Building".bold(), file.display());

    println!("  {} Compiling", "→".dimmed());

    Compiler::build(&source, &filename, &output)?;

    println!("  {} Built", "✓".green());

    print_finished(start.elapsed());

    println!("  {} {}", "→".dimmed(), output.display());

    Ok(ExitCode::SUCCESS)
}

fn run(file: &Path) -> Result<ExitCode> {
    let source = read_source(file)?;
    let filename = file.display().to_string();

    let output = temporary_output_path(file);

    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            miette::miette!(
                "failed to create output directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let start = Instant::now();

    println!("  {} {}", "Running".bold(), file.display());

    println!("  {} Compiling", "→".dimmed());

    if let Err(error) = Compiler::build(&source, &filename, &output) {
        cleanup(&output);
        return Err(error);
    }

    println!("  {} Built", "✓".green());
    println!();

    let executable = executable_path(&output);

    let status = Command::new(&executable)
        .status()
        .map_err(|error| miette::miette!("failed to run '{}': {error}", executable.display()));

    cleanup(&output);

    let status = status?;

    println!();

    if status.success() {
        print_finished(start.elapsed());
        return Ok(ExitCode::SUCCESS);
    }

    let code = status.code().unwrap_or(1);

    Ok(ExitCode::from(code.clamp(1, 255) as u8))
}

fn read_source(file: &Path) -> Result<String> {
    if file.extension().and_then(|extension| extension.to_str()) != Some("aco") {
        return Err(miette::miette!(
            "expected an Alaco source file (.aco), got '{}'",
            file.display()
        ));
    }

    fs::read_to_string(file)
        .map_err(|error| miette::miette!("failed to read '{}': {error}", file.display()))
}

fn default_output_path(file: &Path) -> PathBuf {
    if is_project_entry(file) {
        let root = file
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        let project_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("alaco");
        return root.join("target").join(project_name);
    }

    let parent = file.parent().unwrap_or_else(|| Path::new("."));

    let name = file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("a.out");

    parent.join(name)
}

fn temporary_output_path(file: &Path) -> PathBuf {
    if is_project_entry(file) {
        let root = file
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        return root.join(format!("target/.alaco-run-{}", std::process::id()));
    }

    let parent = file.parent().unwrap_or_else(|| Path::new("."));

    let name = file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("alaco_program");

    parent.join(format!(".{name}.alaco-run"))
}

fn is_project_entry(file: &Path) -> bool {
    file.file_name().is_some_and(|name| name == "main.aco")
        && file
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "src")
}

fn executable_path(path: &Path) -> PathBuf {
    if path.components().count() == 1 {
        PathBuf::from(".").join(path)
    } else {
        path.to_path_buf()
    }
}

fn cleanup(path: &Path) {
    let _ = fs::remove_file(path);
}

fn print_finished(duration: Duration) {
    println!();

    println!(
        "  {} in {}",
        "Finished".bold(),
        format_duration(duration).dimmed()
    );
}

fn format_duration(duration: Duration) -> String {
    let milliseconds = duration.as_secs_f64() * 1000.0;

    if milliseconds < 1.0 {
        format!("{milliseconds:.2}ms")
    } else if milliseconds < 1000.0 {
        format!("{milliseconds:.0}ms")
    } else {
        format!("{:.2}s", milliseconds / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clean_target, create_project_files, default_output_path, discover_tests, format_duration,
        init_project, temporary_output_path,
    };
    use std::{fs, path::Path, time::Duration};

    #[test]
    fn formats_short_durations_in_milliseconds() {
        assert_eq!(format_duration(Duration::from_micros(250)), "0.25ms");
        assert_eq!(format_duration(Duration::from_millis(42)), "42ms");
    }

    #[test]
    fn formats_long_durations_in_seconds() {
        assert_eq!(format_duration(Duration::from_millis(1250)), "1.25s");
    }

    #[test]
    fn project_scaffolding_creates_a_runnable_entry_point() {
        let root = test_directory("new-project");
        fs::create_dir_all(&root).unwrap();

        create_project_files(&root).unwrap();

        let source = fs::read_to_string(root.join("src/main.aco")).unwrap();
        assert!(source.contains("fn main()"));
        assert!(source.contains("Hello, Alaco!"));
        assert!(root.join("README.md").is_file());
        assert!(root.join(".gitignore").is_file());
        let manifest = fs::read_to_string(root.join("Alaco.toml")).unwrap();
        assert!(manifest.contains("name = \""));
        assert!(manifest.contains("entry = \"src/main.aco\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn init_never_overwrites_an_existing_entry_point() {
        let root = test_directory("init-project");
        let source_dir = root.join("src");
        fs::create_dir_all(&source_dir).unwrap();
        let entry = source_dir.join("main.aco");
        fs::write(&entry, "// keep me\n").unwrap();

        assert!(init_project(&root).is_err());
        assert_eq!(fs::read_to_string(entry).unwrap(), "// keep me\n");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_initialization_preserves_existing_support_files() {
        let root = test_directory("init-preserve");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("README.md"), "my existing readme\n").unwrap();
        fs::write(root.join(".gitignore"), "custom ignore rules\n").unwrap();

        create_project_files(&root).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("README.md")).unwrap(),
            "my existing readme\n"
        );
        assert_eq!(
            fs::read_to_string(root.join(".gitignore")).unwrap(),
            "custom ignore rules\n"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_build_outputs_live_in_target() {
        let entry = Path::new("demo/src/main.aco");
        assert_eq!(default_output_path(entry), Path::new("demo/target/demo"));
        assert!(
            temporary_output_path(entry)
                .to_string_lossy()
                .starts_with("demo/target/.alaco-run-")
        );
    }

    #[test]
    fn test_discovery_finds_only_top_level_aco_files_in_sorted_order() {
        let root = test_directory("test-discovery");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("z.aco"), "").unwrap();
        fs::write(root.join("a.aco"), "").unwrap();
        fs::write(root.join("notes.md"), "").unwrap();
        fs::write(root.join("nested/ignored.aco"), "").unwrap();

        assert_eq!(
            discover_tests(&root).unwrap(),
            vec![root.join("a.aco"), root.join("z.aco")]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clean_removes_only_the_given_build_directory() {
        let root = test_directory("clean");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("artifact"), "generated").unwrap();
        fs::write(root.join("keep.txt"), "keep").unwrap();

        clean_target(&target).unwrap();

        assert!(!target.exists());
        assert_eq!(fs::read_to_string(root.join("keep.txt")).unwrap(), "keep");
        fs::remove_dir_all(root).unwrap();
    }

    fn test_directory(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "alaco-cli-{label}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }
}
