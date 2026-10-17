# Projects and runtime

## Package manifests

Alaco projects use `Alaco.toml` for package metadata. A minimal manifest has a `[package]` table with `name`, semantic `version`, `edition = "2026"`, and an optional `.aco` `entry` path (default `src/main.aco`). `[dependencies]` accepts version strings and inline tables with `path` or `version` fields. Run `alaco manifest` to validate and print this metadata. Dependency downloading and dependency-aware import resolution are not implemented yet; local imports remain relative to the importing file.

Use `alaco new my_app` to create a project scaffold, or `alaco init` to initialize the current directory. Inside a project, commands without an explicit source path use the configured entry point where applicable. Keep application code under `src/` and reusable local modules beside the source file that imports them, or in matching subdirectories for dotted module paths.

## Shell expressions

Alaco supports a shell block expression. It runs the block as a command and returns a `ShellResult` value, which can be printed:

```aco
fn main() {
    let result = shell {
        echo "Hello from a command"
    };
    print(result);
}
```

Shell commands are platform-dependent. Only run shell commands you trust.

The shell block is an operating-system integration, not a portable replacement for standard-library functions. Prefer `std.fs`, `std.path`, or other standard modules when they cover the task. A shell block's command syntax and available programs depend on the host environment.

## Compiler checks and diagnostics

The analyzer checks function argument types and catches common mistakes such as misspelled functions, module functions, and struct fields. Diagnostics use stable `alaco::analysis::*` codes; source labels target the expression or declaration responsible for the error. Unknown types are highlighted at the type name, type mismatches at the relevant expression, and duplicate declarations at the repeated declaration. Some whole-program errors, such as a missing `main`, have no source span to highlight. Use `alaco check file.aco` to parse and analyze without building or running the program.

### Recommended development loop

1. Write a small `.aco` file and keep one behavior in focus.
2. Run `alaco check path/to/file.aco` to catch syntax and type/name errors early.
3. Run `alaco run path/to/file.aco` to compile and execute it.
4. Use `alaco build path/to/file.aco` when you want a persistent native executable.
5. Add a focused program under `tests/` and run `alaco test` when the behavior should stay covered.

For command-line options, run `alaco --help`. For standard-library signatures and failure behavior, consult the [standard-library reference](/reference/standard-library). The compiler is evolving, so this guide documents the implemented subset rather than promising compatibility with another language.
