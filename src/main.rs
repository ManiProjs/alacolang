mod analyzer;
mod ast;
mod codegen;
mod compiler;
mod lexer;
mod parser;
mod token;

use std::{env, fs, path::Path, process::Command};

use compiler::Compiler;

fn usage() -> ! {
    eprintln!(
        "usage:
    alaco check <file.aco>
    alaco build <file.aco>
    alaco run <file.aco>"
    );

    std::process::exit(1);
}

fn read_source(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(source) => source,

        Err(error) => {
            eprintln!("alaco: cannot read {}: {}", path.display(), error);

            std::process::exit(1);
        }
    }
}

fn main() {
    let mut args = env::args().skip(1);

    let Some(command) = args.next() else {
        usage();
    };

    let Some(filename) = args.next() else {
        usage();
    };

    let path = Path::new(&filename);

    if path.extension().and_then(|x| x.to_str()) != Some("aco") {
        eprintln!("alaco: '{}' is not an Alaco source file", path.display());

        std::process::exit(1);
    }

    let source = read_source(path);

    match command.as_str() {
        "check" => match Compiler::check(&source) {
            Ok(_) => {
                println!("✓ {}", path.display());
            }

            Err(error) => {
                eprintln!("alaco: {error}");
                std::process::exit(1);
            }
        },

        "build" => {
            let output = Compiler::output_path(path);

            match Compiler::build(&source, &output) {
                Ok(()) => {
                    println!("Built {}", output.display());
                }

                Err(error) => {
                    eprintln!("alaco: {error}");
                    std::process::exit(1);
                }
            }
        }

        "run" => {
            let output = Compiler::output_path(path);

            if let Err(error) = Compiler::build(&source, &output) {
                eprintln!("alaco: {error}");
                std::process::exit(1);
            }

            let executable = if output.components().count() == 1 {
                std::path::PathBuf::from(".").join(&output)
            } else {
                output.clone()
            };

            let status = match Command::new(&executable).status() {
                Ok(status) => status,

                Err(error) => {
                    eprintln!("alaco: failed to run {}: {}", executable.display(), error);

                    std::process::exit(1);
                }
            };

            if let Some(code) = status.code() {
                std::process::exit(code);
            }

            std::process::exit(1);
        }

        _ => usage(),
    }
}
