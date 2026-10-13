# Alaco language guide

Alaco is a small, statically checked language that compiles through generated C++. Source files use the `.aco` extension. This guide describes the syntax implemented by the current compiler; it is not a promise that every feature of Rust or C++ is supported.

## A first program

```alaco
fn main() {
    let language = "Alaco";
    println("Hello from " + language + "!");
}
```

`main` is the program entry point. `std.io` is available without an import, so `print`, `println`, `eprint`, and `eprintln` can be called directly.

## Types and variables

The built-in types include:

| Type | Meaning | Example |
| --- | --- | --- |
| `Int` | Integer | `let count: Int = 3;` |
| `Float` | Floating-point number | `let temperature: Float = 21.5;` |
| `Bool` | `true` or `false` | `let ready: Bool = true;` |
| `String` | Text | `let title: String = "Hello";` |

Type annotations can be omitted when the compiler can infer a variable's type. Use `let mut` for a variable that will be reassigned:

```alaco
let mut total: Int = 0;
total = total + 1;
```

## Functions

Declare functions with `fn`, typed parameters, and an optional return type. Functions without a return type do not return a value.

Functions with a non-`Void` return type must return a value on every statically recognized path. The analyzer recognizes direct returns and `if`/`else` branches that both return; it does not assume that a loop will run or terminate.

```alaco
fn add(left: Int, right: Int) -> Int {
    return left + right;
}

fn main() {
    println(add(20, 22));
}
```

## Conditions and operators

`if` supports an optional `else` block. Conditions use `Bool` expressions:

```alaco
if score >= 10 {
    println("Passed");
} else {
    println("Try again");
}
```

The implemented binary operators include arithmetic (`+`, `-`, `*`, `/`, `%`), comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), logical `and` / `or`, and compound assignments (`+=`, `-=`, `*=`, `/=`). Logical operators require `Bool` operands; `and` binds more tightly than `or`. Parentheses can group expressions.

## Loops and match

Use `loop (repeat count)` to execute a block a fixed number of times:

```alaco
loop (repeat 3) {
    println("Again");
}
```

`match` selects a branch by matching a value against patterns. `_` is the fallback pattern:

```alaco
match answer {
    1 => println("one")
    2 => println("two")
    _ => println("something else")
}
```

## Structs

Structs group named, typed fields. Field access uses dot notation.

```alaco
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

## Imports

Import a standard-library module with its dotted path. Its final component is the namespace used to call its functions.

```alaco
import std.math;

fn main() {
    println(math.clamp(12, 0, 10));
}
```

See the [standard-library reference](standard-library.md) for available modules and their behavior.

## Shell expressions

Alaco supports a shell block expression. It runs the block as a command and returns a `ShellResult` value, which can be printed:

```alaco
fn main() {
    let result = shell {
        echo "Hello from a command"
    };
    print(result);
}
```

Shell commands are platform-dependent. Only run shell commands you trust.

## Compiler checks and diagnostics

The analyzer checks function argument types and catches common mistakes such as misspelled functions, module functions, and struct fields. Diagnostics use stable `alaco::analysis::*` codes; source labels target the expression or declaration responsible for the error. Unknown types are highlighted at the type name, type mismatches at the relevant expression, and duplicate declarations at the repeated declaration. Some whole-program errors, such as a missing `main`, have no source span to highlight. Use `alaco check file.aco` to parse and analyze without building or running the program.
