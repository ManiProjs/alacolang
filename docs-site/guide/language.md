# Alaco language guide

Alaco is a small, statically checked language that compiles through generated C++. Source files use the `.aco` extension. This guide describes the syntax implemented by the current compiler; it is not a promise that every feature of Rust or C++ is supported.

## A first program

```text
fn main() {
    let language = "Alaco";
    println("Hello from " + language + "!");
}
```

`main` is the program entry point. `std.io` is available without an import, so `print`, `println`, `eprint`, and `eprintln` can be called directly.

## Source files and basic syntax

Alaco source files use `.aco`. Statements such as variable declarations, assignments, function calls, and `return` normally end with a semicolon; the parser also accepts omitted semicolons in several statement positions. Blocks are delimited by `{` and `}`. Identifiers are used for variables, functions, types, and module names. Comments use `//` for the remainder of a line.

The compiler is statically checked: it analyzes types and names before generating C++. A program can therefore fail during parsing, semantic analysis, C++ compilation, or execution. A successful `alaco check` means the Alaco front end accepted the program; it does not build the native executable.

## Types and variables

The built-in types include:

| Type | Meaning | Example |
| --- | --- | --- |
| `Int` | Integer | `let count: Int = 3;` |
| `Float` | Floating-point number | `let temperature: Float = 21.5;` |
| `Bool` | `true` or `false` | `let ready: Bool = true;` |
| `String` | Text | `let title: String = "Hello";` |

Type annotations can be omitted when the compiler can infer a variable's type. Use `let mut` for a variable that will be reassigned:

```text
let mut total: Int = 0;
total = total + 1;
```

An ordinary `let` binding is immutable and cannot be reassigned. Annotate a binding when you want to make the intended type explicit or inference needs help. The current built-in types are intentionally small; do not assume that unlisted Rust or C++ types, generics, or implicit conversions are supported.

## Functions

Declare functions with `fn`, typed parameters, and an optional return type. Functions without a return type do not return a value.

Functions with a non-`Void` return type must return a value on every statically recognized path. The analyzer recognizes direct returns and `if`/`else` branches that both return; it does not assume that a loop will run or terminate.

```text
fn add(left: Int, right: Int) -> Int {
    return left + right;
}

fn main() {
    println(add(20, 22));
}
```

## Conditions and operators

`if` supports an optional `else` block. Conditions use `Bool` expressions:

```text
if score >= 10 {
    println("Passed");
} else {
    println("Try again");
}
```

The implemented binary operators include arithmetic (`+`, `-`, `*`, `/`, `%`), comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), logical `and` / `or`, and compound assignments (`+=`, `-=`, `*=`, `/=`). Logical operators require `Bool` operands; `and` binds more tightly than `or`. The unary `!` operator negates a boolean, and unary `-` negates a number. Parentheses can group expressions.

Equality is defined for `Int`, `Float`, `Bool`, and `String`; comparing structs, shell values, or `Void` is rejected. `%` currently requires two `Int` operands.

### Remembering values and reacting with `when`

`previous name` reads the value of a local variable immediately before its most recent assignment. Its initial previous value is the value established by the variable's declaration (or the function argument's initial value). It currently supports `Int`, `Float`, `Bool`, and `String` locals/parameters.

```text
let mut score = 0;
when score != previous score {
    println("Score changed: ${score}");
}
score = 1;
score = 2;
```

`when condition { ... }` checks the condition immediately when registered and after each subsequent statement in the same lexical block, running the block whenever the condition is true at one of those checkpoints. It is synchronous—not a background task. Put the `when` after the variables it observes have been declared. A watcher declared inside a block is scoped to that block.

### Timed variables

Give a local variable a deadline with `for` between its name and optional type:

```text
let cache for 20 seconds = "temporary";
let mut session for 1 minute: String = "active";
```

Supported units are `ms`/`milliseconds`, `s`/`seconds`, and `m`/`minutes`. When the deadline arrives, Alaco clears the stored value automatically. Reading, assigning, or using `previous` after expiry raises a runtime error instead of silently recreating the value. Timed variables currently support `Int`, `Float`, `Bool`, and `String`. The deadline is fixed at declaration; reassignment does not renew it. The compiler uses a small timer thread per timed variable, so use this for short-lived values rather than thousands of tiny-lived locals.

Use `==` and `!=` for equality; a condition must evaluate to `Bool`, so numeric values are not implicitly treated as true or false. `+` adds compatible numeric values and concatenates strings. When in doubt, keep expressions simple and use parentheses to make grouping clear.

### Pipe expressions

The forward pipe operator `|>` passes its left-hand value as the first argument to a function on the right. Any arguments already written in a call follow the piped value. Pipes associate from left to right:

```text
fn double(value: Int) -> Int {
    return value * 2;
}

fn add(left: Int, right: Int) -> Int {
    return left + right;
}

fn main() {
    let result = 4 |> double |> add(3);
    println(result); // 11
}
```

This is equivalent to `add(double(4), 3)`. Pipe inputs are checked against the target function's parameter types before C++ is generated.

## Loops and match

Use `loop (repeat count)` to execute a block a fixed number of times:

```text
loop (repeat 3) {
    println("Again");
}
```

`match` selects a branch by matching a value against patterns. `_` is the fallback pattern:

```text
match answer {
    1 => println("one")
    2 => println("two")
    _ => println("something else")
}
```

The currently documented loop form is a fixed-count repeat loop. `match` supports literal patterns and `_` as a catch-all. Do not assume `while`, `for`, `break`, or advanced pattern matching are available unless the compiler's current grammar explicitly supports them.

## Structs

Structs group named, typed fields. Field access uses dot notation.

```text
struct User {
    name: String,
    age: Int,
}

fn main() {
    let user = User {
        name: "Ada",
        age: 36,
    };
    println(user.name);
    println(user);
}
```

You can destructure selected fields into local variables. By default, each binding keeps the field's name; use `field: binding` to rename it. Fields you do not list are ignored.

```text
let Point { x, y: vertical } = point;
println(x);
println(vertical);
```

## Imports

Import a standard-library or local module. A single-component local import resolves to a sibling `.aco` file (`import helpers;` loads `helpers.aco`); dotted local imports map intermediate components to directories and the final component to a `.aco` file (`import helpers.greetings;` loads `helpers/greetings.aco`). Paths are resolved relative to the importing `.aco` file. The final component is the namespace used to call its functions by default. Use `as` to choose a different namespace.

To call a specific function without the module prefix, use a named import:

```text
import hello from helpers;

fn main() {
    println(hello("Alaco"));
}
```

This loads `helpers.aco` and makes `hello` directly callable. Named imports currently support one function name per import statement. Try it with `alaco run examples/modules/named_import.aco` from the repository root.

```text
import std.math;
import helpers;
import helpers.greetings;
import helpers.math_tools as maths;

fn main() {
    println(math.clamp(12, 0, 10));
    println(greetings.hello("Alaco"));
    println(maths.square(9));
}
```

For these local imports, the importing directory can contain `helpers.aco` as well as `helpers/greetings.aco` and `helpers/math_tools.aco` under a `helpers/` directory. Imported modules can import other modules relative to their own file; dependencies are loaded first. Duplicate aliases and cyclic imports are reported as errors. Run `alaco run examples/modules/main.aco`, `alaco run examples/modules/simple_import.aco`, or `alaco run examples/modules/named_import.aco` from the repository root to try the examples.

See the [standard-library reference](/reference/standard-library) for available modules and their behavior.

### Import resolution notes

- `import helpers;` resolves to `helpers.aco` next to the importing file.
- `import helpers.greetings;` resolves to `helpers/greetings.aco` next to the importing file.
- `import helpers as util;` gives the module a local alias.
- `import hello from helpers;` imports one named function without the module prefix.

Imports are resolved relative to the source file containing the import, not necessarily the shell's current directory. Imported modules are loaded before their dependents. Cyclic imports and duplicate module aliases are errors. Named imports currently select a single function per statement; package dependency declarations do not yet download packages or change this local import behavior.

### String interpolation

Insert a variable into a string with `${name}` syntax. Literal text and variable values are combined at runtime:

```text
fn main() {
    let name = "Alaco";
    let version = 2026;
    println("Hello, ${name}! Welcome to Alaco ${version}.");
}
```

Interpolation currently supports variable names (including numeric and boolean variables), not arbitrary expressions. To try it, run `alaco run examples/string_interpolation.aco` from the repository root. Strings without `${...}` keep their existing behavior, including JSON text containing braces.

Interpolation is useful when a message combines fixed text with a value. For a calculation, compute the result first and interpolate the resulting variable:

```text
fn main() {
    let subtotal = 18;
    let quantity = 3;
    let total = subtotal * quantity;
    println("Total: ${total}");
}
```

## Package manifests

Alaco projects use `Alaco.toml` for package metadata. A minimal manifest has a `[package]` table with `name`, semantic `version`, `edition = "2026"`, and an optional `.aco` `entry` path (default `src/main.aco`). `[dependencies]` accepts version strings and inline tables with `path` or `version` fields. Run `alaco manifest` to validate and print this metadata. Dependency downloading and dependency-aware import resolution are not implemented yet; local imports remain relative to the importing file.

Use `alaco new my_app` to create a project scaffold, or `alaco init` to initialize the current directory. Inside a project, commands without an explicit source path use the configured entry point where applicable. Keep application code under `src/` and reusable local modules beside the source file that imports them, or in matching subdirectories for dotted module paths.

## Shell expressions

Alaco supports a shell block expression. It runs the block as a command and returns a `ShellResult` value, which can be printed:

```text
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
