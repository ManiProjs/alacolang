# Control flow and data

## Remembering values and reacting with `when`

`previous name` reads the value of a local variable immediately before its most recent assignment. Its initial previous value is the value established by the variable's declaration (or the function argument's initial value). It currently supports `Int`, `Float`, `Bool`, and `String` locals/parameters.

```aco
let mut score = 0;
when score != previous score {
    println("Score changed: ${score}");
}
score = 1;
score = 2;
```

`when condition { ... }` checks the condition immediately when registered and after each subsequent statement in the same lexical block, running the block whenever the condition is true at one of those checkpoints. It is synchronous—not a background task. Put the `when` after the variables it observes have been declared. A watcher declared inside a block is scoped to that block.

## Timed variables

Give a local variable a deadline with `for` between its name and optional type:

```aco
let cache for 20 seconds = "temporary";
let mut session for 1 minute: String = "active";
```

Supported units are `ms`/`milliseconds`, `s`/`seconds`, and `m`/`minutes`. When the deadline arrives, Alaco clears the stored value automatically. Reading, assigning, or using `previous` after expiry raises a runtime error instead of silently recreating the value. Timed variables currently support `Int`, `Float`, `Bool`, and `String`. The deadline is fixed at declaration; reassignment does not renew it. The compiler uses a small timer thread per timed variable, so use this for short-lived values rather than thousands of tiny-lived locals.

Use `==` and `!=` for equality; a condition must evaluate to `Bool`, so numeric values are not implicitly treated as true or false. `+` adds compatible numeric values and concatenates strings. When in doubt, keep expressions simple and use parentheses to make grouping clear.

## Pipe expressions

The forward pipe operator `|>` passes its left-hand value as the first argument to a function on the right. Any arguments already written in a call follow the piped value. Pipes associate from left to right:

```aco
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

```aco
loop (repeat 3) {
    println("Again");
}
```

`match` selects a branch by matching a value against patterns. `_` is the fallback pattern:

```aco
match answer {
    1 => println("one")
    2 => println("two")
    _ => println("something else")
}
```

The currently documented loop form is a fixed-count repeat loop. `match` supports literal patterns and `_` as a catch-all. Do not assume `while`, `for`, `break`, or advanced pattern matching are available unless the compiler's current grammar explicitly supports them.

## Structs

Structs group named, typed fields. Field access uses dot notation.

```aco
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

```aco
let Point { x, y: vertical } = point;
println(x);
println(vertical);
```
