# Alaco language guide

Alaco is a small, statically checked language that compiles through generated C++. Source files use the `.aco` extension. This guide describes the syntax implemented by the current compiler; it is not a promise that every feature of Rust or C++ is supported.

## A first program

```aco
fn main() {
    let language = "Alaco";
    println("Hello from " + language + "!");
}
```

`main` is the program entry point. `std.io` is available without an import, so `print`, `println`, `eprint`, and `eprintln` can be called directly.

## Source files and basic syntax

Alaco source files use `.aco`. Statements such as variable declarations, assignments, function calls, and `return` normally end with a semicolon; the parser also accepts omitted semicolons in several statement positions. Blocks are delimited by `{` and `}`. Identifiers are used for variables, functions, types, and module names. Comments use `//` for the remainder of a line.

The compiler is statically checked: it analyzes types and names before generating C++. A program can therefore fail during parsing, semantic analysis, C++ compilation, or execution. A successful `alaco check` means the Alaco front end accepted the program; it does not build the native executable.

## Continue learning

<div class="vp-doc-cards">
  <a href="./language-types"><strong>01 · Types and expressions</strong><span>Variables, functions, conditions, and operators.</span></a>
  <a href="./language-control-flow"><strong>02 · Control flow and data</strong><span>Watchers, timed values, pipes, loops, match, and structs.</span></a>
  <a href="./language-modules"><strong>03 · Modules and strings</strong><span>Imports, module resolution, and string interpolation.</span></a>
  <a href="./language-projects"><strong>04 · Projects and runtime</strong><span>Manifests, shell expressions, diagnostics, and workflow.</span></a>
</div>

The compiler is evolving, so these pages describe the implemented subset rather than promising compatibility with Rust, C++, or a complete language specification.
