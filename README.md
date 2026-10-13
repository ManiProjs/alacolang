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

When using an installed `alaco` executable, the equivalent commands are `alaco run hello.aco` and `alaco check hello.aco`. `run` compiles and executes the program; `check` parses and analyzes it without building an executable.

## CLI reference

| Command | Description |
| --- | --- |
| `alaco check <file.aco>` | Parse and analyze a source file. |
| `alaco explain <file.aco>` | Check a file and show diagnostics when errors are found. |
| `alaco build <file.aco>` | Build a native executable next to the source file. |
| `alaco build <file.aco> --output <path>` | Build to a specified output path. |
| `alaco run <file.aco>` | Compile and run a program. |
| `alaco --help` | Show command-line help. |

## Documentation

- [Language guide](docs/language-guide.md) — syntax, types, functions, control flow, structs, imports, and shell expressions.
- [Standard library reference](docs/standard-library.md) — module APIs, return values, and runtime limitations.

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

## Compiler diagnostics

The analyzer checks function argument types before C++ generation and can suggest corrections for misspelled function names, module functions, and struct fields. Diagnostics include source locations when available.

## Examples and tests

The [`examples/`](examples/) directory contains small programs and standard-library integration examples. Run the compiler's test suite with:

```sh
cargo test
```
