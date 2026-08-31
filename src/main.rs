mod analyzer;
mod ast;
mod codegen;
mod compiler;
mod lexer;
mod parser;
mod token;

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::Instant,
};

use clap::{Parser, Subcommand};
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
    /// Check an Alaco source file without compiling it
    Check {
        /// Alaco source file
        file: PathBuf,
    },

    /// Compile an Alaco source file
    Build {
        /// Alaco source file
        file: PathBuf,

        /// Output executable
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Compile and run an Alaco program
    Run {
        /// Alaco source file
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(CommandKind::Check { file }) => check(&file),
        Some(CommandKind::Build { file, output }) => build(&file, output.as_deref()),
        Some(CommandKind::Run { file }) => run(&file),

        None => {
            println!("alaco {}", env!("CARGO_PKG_VERSION"));

            println!("Try 'alaco --help' for more information.");

            ExitCode::SUCCESS
        }
    }
}

fn check(file: &Path) -> ExitCode {
    let source = match read_source(file) {
        Ok(source) => source,
        Err(()) => return ExitCode::FAILURE,
    };

    let start = Instant::now();

    println!("  {} {}", "Checking".bold(), file.display());

    match Compiler::check(&source) {
        Ok(_) => {
            println!("  {} Parsed", "✓".green());
            println!("  {} Analyzed", "✓".green());

            println!();
            println!(
                "  {} Finished in {}",
                "Finished".bold(),
                format_duration(start.elapsed()).dimmed()
            );

            ExitCode::SUCCESS
        }

        Err(error) => {
            print_error(&error);
            ExitCode::FAILURE
        }
    }
}

fn build(file: &Path, requested_output: Option<&Path>) -> ExitCode {
    let source = match read_source(file) {
        Ok(source) => source,
        Err(()) => return ExitCode::FAILURE,
    };

    let output = requested_output
        .map(PathBuf::from)
        .unwrap_or_else(|| Compiler::output_path(file));

    let start = Instant::now();

    println!("  {} {}", "Building".bold(), file.display());

    println!("  {} Checking", "→".dimmed());

    if let Err(error) = Compiler::check(&source) {
        print_error(&error);
        return ExitCode::FAILURE;
    }

    println!("  {} Parsed", "✓".green());
    println!("  {} Analyzed", "✓".green());

    println!("  {} Compiling", "→".dimmed());

    if let Err(error) = Compiler::build(&source, &output) {
        print_error(&error);
        return ExitCode::FAILURE;
    }

    println!("  {} Compiled", "✓".green());
    println!("  {} Linked", "✓".green());

    println!();
    println!(
        "  {} Finished in {}",
        "Finished".bold(),
        format_duration(start.elapsed()).dimmed()
    );

    println!("  {} {}", "→".dimmed(), output.display().bold());

    ExitCode::SUCCESS
}

fn run(file: &Path) -> ExitCode {
    let source = match read_source(file) {
        Ok(source) => source,
        Err(()) => return ExitCode::FAILURE,
    };

    let start = Instant::now();

    println!("  {} {}", "Compiling".bold(), file.display());

    println!("  {} Checking", "→".dimmed());

    if let Err(error) = Compiler::check(&source) {
        print_error(&error);
        return ExitCode::FAILURE;
    }

    println!("  {} Parsed", "✓".green());
    println!("  {} Analyzed", "✓".green());

    let output = Compiler::output_path(file);

    println!("  {} Compiling", "→".dimmed());

    if let Err(error) = Compiler::build(&source, &output) {
        print_error(&error);
        return ExitCode::FAILURE;
    }

    println!("  {} Compiled", "✓".green());
    println!("  {} Linked", "✓".green());

    println!();

    let executable = executable_path(&output);

    let status = match Command::new(&executable).status() {
        Ok(status) => status,

        Err(error) => {
            print_error(&format!(
                "failed to run '{}': {}",
                executable.display(),
                error
            ));

            cleanup(&output);

            return ExitCode::FAILURE;
        }
    };

    cleanup(&output);

    println!();

    if let Some(code) = status.code() {
        if code == 0 {
            println!(
                "  {} Finished in {}",
                "Finished".bold(),
                format_duration(start.elapsed()).dimmed()
            );

            return ExitCode::SUCCESS;
        }

        println!(
            "  {} process exited with status {}",
            "error:".red().bold(),
            code
        );

        return ExitCode::from(code.clamp(1, 255) as u8);
    }

    println!(
        "  {} process terminated unexpectedly",
        "error:".red().bold()
    );

    ExitCode::FAILURE
}

fn read_source(file: &Path) -> Result<String, ()> {
    if !file.exists() {
        print_error(&format!("file not found: {}", file.display()));

        return Err(());
    }

    match file.extension().and_then(|x| x.to_str()) {
        Some("aco") => {}

        _ => {
            print_error(&format!(
                "expected an Alaco source file (.aco), got '{}'",
                file.display()
            ));

            return Err(());
        }
    }

    std::fs::read_to_string(file).map_err(|error| {
        print_error(&format!("could not read '{}': {}", file.display(), error));
    })
}

fn executable_path(path: &Path) -> PathBuf {
    if path.components().count() == 1 {
        PathBuf::from(".").join(path)
    } else {
        path.to_path_buf()
    }
}

fn cleanup(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        eprintln!(
            "{} could not remove temporary executable '{}': {}",
            "warning:".yellow().bold(),
            path.display(),
            error
        );
    }
}

fn print_error(error: &str) {
    eprintln!();
    eprintln!("{} {}", "error:".red().bold(), error);
}

fn format_duration(duration: std::time::Duration) -> String {
    let millis = duration.as_secs_f64() * 1000.0;

    if millis < 1.0 {
        format!("{:.2}ms", millis)
    } else if millis < 1000.0 {
        format!("{:.0}ms", millis)
    } else {
        format!("{:.2}s", millis / 1000.0)
    }
}
