# Alaco (You can also call it A)

A programming language, just for hobby, not a big project

## How it works?

Unlike other compilers, where turn the tokens to assembly code, this compiler is going to turn the tokens to C++ code

Read the file -> Parse the token -> Convert tokens to C++ code -> Call the C++ compiler -> C++ compiler compiles it -> Now it's compiled and ready to run

## Standard library

`std.io` is available by default and provides `print`, `println`, `eprint`, and
`eprintln`. These functions support primitive values and values with a runtime
formatter.

Import additional modules explicitly:

```alaco
import std.math;
import std.string;

fn main() {
    println(math.clamp(math.abs(-12), 0, 10));
    println(string.concat("Ala", "co"));
    println(string.repeat_string("ha", 3));
}
```

Available `std.math` helpers: `min`, `max`, `abs`, `square`, `cube`, `sign`,
`clamp`, `in_range`, `is_even`, and `is_odd` (integer operations).

Available `std.string` helpers: `concat`, `is_empty`, and `repeat_string`.

More modules:

- `std.geometry`: rectangle area/perimeter, squared and Manhattan distance, and point-in-rectangle checks.
- `std.stats`: sum, average, minimum, and maximum helpers for small fixed groups of integers.
- `std.units`: Celsius/Fahrenheit, kilometers/miles, and kilograms/pounds conversions (`Float`).
- `std.validation`: non-empty string, numeric range, positive, and non-negative checks.
- `std.fs`: read/write UTF-8 text files, query file size in bytes, copy files (overwriting the destination), check existence, remove files, and create directories. `file_size` returns `-1` on failure. Failed reads return an empty string; mutating operations report success as `Bool`.
- `std.env`: read, check, and set environment variables, and retrieve the current working directory. `get` returns an empty string for missing variables, so pair it with `has` when needed.
- `std.path`: join paths and inspect basename, dirname, extension, stem, and whether a path is absolute. Path operations are lexical except for the absolute-path check; they do not require the target to exist.

## Diagnostics

The analyzer checks function argument types before C++ generation and suggests
likely corrections for misspelled function names, module functions, and struct
fields. Diagnostics highlight the relevant source location whenever available.
