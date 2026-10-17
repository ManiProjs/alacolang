# Types, functions, and expressions

## Types and variables

The built-in types include:

| Type | Meaning | Example |
| --- | --- | --- |
| `Int` | Integer | `let count: Int = 3;` |
| `Float` | Floating-point number | `let temperature: Float = 21.5;` |
| `Bool` | `true` or `false` | `let ready: Bool = true;` |
| `String` | Text | `let title: String = "Hello";` |

Type annotations can be omitted when the compiler can infer a variable's type. Use `let mut` for a variable that will be reassigned:

```aco
let mut total: Int = 0;
total = total + 1;
```

An ordinary `let` binding is immutable and cannot be reassigned. Annotate a binding when you want to make the intended type explicit or inference needs help. The current built-in types are intentionally small; do not assume that unlisted Rust or C++ types, generics, or implicit conversions are supported.

## Functions

Declare functions with `fn`, typed parameters, and an optional return type. Functions without a return type do not return a value.

Functions with a non-`Void` return type must return a value on every statically recognized path. The analyzer recognizes direct returns and `if`/`else` branches that both return; it does not assume that a loop will run or terminate.

```aco
fn add(left: Int, right: Int) -> Int {
    return left + right;
}

fn main() {
    println(add(20, 22));
}
```

## Conditions and operators

`if` supports an optional `else` block. Conditions use `Bool` expressions:

```aco
if score >= 10 {
    println("Passed");
} else {
    println("Try again");
}
```

The implemented binary operators include arithmetic (`+`, `-`, `*`, `/`, `%`), comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), logical `and` / `or`, and compound assignments (`+=`, `-=`, `*=`, `/=`). Logical operators require `Bool` operands; `and` binds more tightly than `or`. The unary `!` operator negates a boolean, and unary `-` negates a number. Parentheses can group expressions.

Equality is defined for `Int`, `Float`, `Bool`, and `String`; comparing structs, shell values, or `Void` is rejected. `%` currently requires two `Int` operands.
