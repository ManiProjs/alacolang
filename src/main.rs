mod analyzer;
mod ast;
mod codegen;
mod compiler;
mod error;
mod lexer;
mod parser;
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
    about = "The Alaco programming language compiler"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CommandKind>,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    /// Check an Alaco source file.
    Check { file: PathBuf },

    /// Explain an error in an Alaco source file.
    Explain { file: PathBuf },

    /// Build an Alaco source file.
    Build {
        file: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Compile and run an Alaco program.
    Run { file: PathBuf },
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
        Some(CommandKind::Check { file }) => check(&file),

        Some(CommandKind::Explain { file }) => explain(&file),

        Some(CommandKind::Build { file, output }) => build(&file, output.as_deref()),

        Some(CommandKind::Run { file }) => run(&file),

        None => {
            println!("alaco {}", env!("CARGO_PKG_VERSION"));
            println!("Try {} for more information.", "alaco --help".cyan());

            Ok(ExitCode::SUCCESS)
        }
    }
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
            eprintln!("{error:?}");

            if let Some(alaco_error) = error.downcast_ref::<crate::error::AlacoError>() {
                if let Some((why, fix)) = alaco_error.explanation() {
                    eprintln!();
                    eprintln!("{}", "Why this happened:".bold());
                    eprintln!("  {why}");

                    eprintln!();
                    eprintln!("{}", "How to fix it:".bold());
                    eprintln!("  {fix}");
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

    let start = Instant::now();

    println!("  {} {}", "Running".bold(), file.display());

    println!("  {} Compiling", "→".dimmed());

    Compiler::build(&source, &filename, &output)?;

    println!("  {} Built", "✓".green());
    println!();

    let executable = executable_path(&output);

    let status = Command::new(&executable)
        .status()
        .map_err(|error| miette::miette!("failed to run '{}': {error}", executable.display()))?;

    cleanup(&output);

    println!();

    if status.success() {
        print_finished(start.elapsed());
        return Ok(ExitCode::SUCCESS);
    }

    let code = status.code().unwrap_or(1);

    eprintln!(
        "{} program exited with status {}",
        "error:".red().bold(),
        code
    );

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
    let parent = file.parent().unwrap_or_else(|| Path::new("."));

    let name = file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("a.out");

    parent.join(name)
}

fn temporary_output_path(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new("."));

    let name = file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("alaco_program");

    parent.join(format!(".{name}.alaco-run"))
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
