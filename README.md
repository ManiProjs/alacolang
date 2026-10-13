# Alaco

Alaco (also called **A**) is a hobby programming language implemented in Rust. The compiler parses and analyzes `.aco` source files, generates C++, and invokes a system C++ compiler to build a native executable.

## Quick start

Install Rust and a C++ compiler supported by your platform, then build the compiler from this repository:

```sh
cargo build
```

Create `hello.aco`:

```alaco
fn main() {
    println("Hello, Alaco!");
}
```

Run or check it with Cargo:

```sh
cargo run -- run hello.aco
cargo run -- check hello.aco
```

When using an installed `alaco` executable, use the same commands directly. The `alaco` command is the entry point for the full toolchain.

## CLI reference

| Command | Description |
| --- | --- |
| `alaco new <name>` | Create a new project with a working entry point. |
| `alaco init [directory]` | Initialize a project in an existing directory without overwriting an entry point. |
| `alaco check [file.aco]` | Parse and analyze a source file. Defaults to `src/main.aco` in a project. |
| `alaco explain [file.aco]` | Check a file and show diagnostics when errors are found. |
| `alaco build [file.aco]` | Build a native executable. Project builds go in `target/`. |
| `alaco build [file.aco] --output <path>` | Build to a specified output path. |
| `alaco run [file.aco]` | Compile and run a program, cleaning up the temporary executable. |
| `alaco test [path]` | Build and run each `.aco` test program in `tests/`, or in a supplied file/directory. |
| `alaco clean` | Remove generated artifacts in the current project's `target/` directory. |
| `alaco doctor` | Check for the C++ compiler and installed standard library. |
| `alaco manifest` | Validate and display `Alaco.toml` package metadata. |
| `alaco --help` | Show command-line help. |

For a quick start:

```sh
alaco new hello
cd hello
alaco run
alaco check
alaco build
alaco test
alaco doctor
```

Inside a project, `Alaco.toml` defines package metadata and the default entry point. `alaco new` and `alaco init` create a manifest. Pass a `.aco` path to run a standalone file instead.

### Package manifests

Every Alaco project uses `Alaco.toml`:

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2026"
entry = "src/main.aco"

[dependencies]
# Dependencies can be declared by version:
# colors = "^1.0"
# Or as a local package:
# my_utils = { path = "../my_utils" }
```

`name` and a semantic `version` are required. `edition` currently supports `2026`; `entry` defaults to `src/main.aco` and must be a relative `.aco` path inside the package. Run `alaco manifest` to validate and inspect the manifest. Dependency declarations are parsed and displayed, but dependency fetching, version resolution, and exposing installed packages as import namespaces are not implemented yet; local `import` statements continue to resolve relative to their importing source file.

### Tests

Put test programs in `tests/` with one `.aco` file per test. Each file is compiled and run as a separate program; a successful process exit means the test passed. A test can also be run directly with `alaco test tests/example.aco`. Tests in the supplied directory are discovered one level deep and run in filename order. Use `alaco clean` to remove generated build artifacts.

## Documentation

- [Language guide](docs/language-guide.md) — syntax, types, functions, control flow, structs, imports, and shell expressions.
- [Standard library reference](docs/standard-library.md) — module APIs, return values, and runtime limitations.
- [Editor support](editors/README.md) — VS Code, Neovim, and Vim syntax highlighting and compiler commands.

## Standard library example

```alaco
import std.math;
import std.string;

fn main() {
    println(math.clamp(math.abs(-12), 0, 10));
    println(string.concat("Ala", "co"));
    println(string.repeat_string("ha", 3));
}
```

`std.io` is available by default. Other modules must be imported explicitly. See the full [standard-library reference](docs/standard-library.md) for all currently documented modules.

The standard library includes filesystem, environment, path, JSON text, basic TOML syntax checking, HTTP GET/JSON POST, and timing helpers. The JSON/TOML/HTTP APIs are intentionally small today; see the reference for their limitations.

## Compiler diagnostics

The analyzer checks types before C++ generation and can suggest corrections for misspelled function names, module functions, and struct fields. Semantic diagnostics use stable `alaco::analysis::*` codes and highlight the relevant expression, declaration, parameter, field, or type name where source information is available. Missing-entry-point errors cannot point to a specific source span.

## Examples and tests

The [`examples/`](examples/) directory contains small programs and standard-library integration examples. Run the compiler's test suite with:

```sh
cargo test
```
