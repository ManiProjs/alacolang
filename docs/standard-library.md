# Standard library reference

Alaco's standard library is written in Alaco and backed by small native runtime helpers where operating-system access is needed. `std.io` is available by default. Import other modules explicitly, for example `import std.math;`, then call their functions through the module name (`math.abs(-4)`).

## `std.io` — output

Available automatically; no import is required.

| Function | Description |
| --- | --- |
| `print(value)` | Write a value to standard output without a newline. |
| `println(value)` | Write a value to standard output followed by a newline. |
| `eprint(value)` | Write a value to standard error without a newline. |
| `eprintln(value)` | Write a value to standard error followed by a newline. |

Printing supports `String`, `Int`, `Float`, `Bool`, and values with a generated runtime formatter, such as supported structs.

## `std.math` — integer helpers

Import with `import std.math;`.

| Function | Behavior |
| --- | --- |
| `min(a, b)` / `max(a, b)` | Return the smaller / larger integer. |
| `abs(value)` | Return the non-negative magnitude of an integer. |
| `square(value)` / `cube(value)` | Return the value multiplied by itself two / three times. |
| `sign(value)` | Return `-1`, `0`, or `1`. |
| `clamp(value, lower, upper)` | Limit a value to the inclusive bounds. |
| `in_range(value, lower, upper)` | Test whether the value is within the inclusive bounds. |
| `is_even(value)` / `is_odd(value)` | Test integer parity. |

## `std.string` — string helpers

Import with `import std.string;`.

| Function | Behavior |
| --- | --- |
| `concat(left, right)` | Concatenate two strings. The `+` operator also concatenates strings. |
| `is_empty(value)` | Return whether a string is empty. |
| `repeat_string(value, count)` | Repeat a string `count` times; a non-positive count performs no iterations. |

## `std.geometry` — integer geometry

Import with `import std.geometry;`.

| Function | Behavior |
| --- | --- |
| `rectangle_area(width, height)` | Multiply width by height. |
| `rectangle_perimeter(width, height)` | Calculate `2 * (width + height)`. |
| `squared_distance(x1, y1, x2, y2)` | Return squared Euclidean distance (no square root). |
| `manhattan_distance(x1, y1, x2, y2)` | Return Manhattan distance. |
| `is_inside_rectangle(x, y, left, top, right, bottom)` | Check whether a point is within the inclusive rectangle bounds. |

## `std.stats` — small integer statistics

Import with `import std.stats;`.

| Function | Behavior |
| --- | --- |
| `sum3(a, b, c)` | Sum three integers. |
| `average2(a, b)` / `average3(a, b, c)` | Calculate an integer average; fractional parts are discarded by integer division. |
| `min2(a, b)` / `max2(a, b)` | Return the minimum / maximum of two integers. |
| `min3(a, b, c)` / `max3(a, b, c)` | Return the minimum / maximum of three integers. |

## `std.units` — unit conversions

Import with `import std.units;`. All functions accept and return `Float`.

| Function | Conversion |
| --- | --- |
| `celsius_to_fahrenheit(value)` | Celsius → Fahrenheit |
| `fahrenheit_to_celsius(value)` | Fahrenheit → Celsius |
| `kilometers_to_miles(value)` | Kilometers → miles |
| `miles_to_kilometers(value)` | Miles → kilometers |
| `kilograms_to_pounds(value)` | Kilograms → pounds |
| `pounds_to_kilograms(value)` | Pounds → kilograms |

## `std.validation` — simple predicates

Import with `import std.validation;`.

| Function | Behavior |
| --- | --- |
| `is_non_empty(value: String)` | True when the string is not empty. Whitespace-only strings count as non-empty. |
| `is_in_range(value, lower, upper)` | True when an integer lies within inclusive bounds. |
| `is_positive(value)` | True when an integer is greater than zero. |
| `is_non_negative(value)` | True when an integer is zero or greater. |

## `std.fs` — filesystem operations

Import with `import std.fs;`. Paths are interpreted relative to the process's current working directory unless absolute. Operations return `Bool` where applicable so callers can check success.

| Function | Return | Behavior and limitations |
| --- | --- | --- |
| `read_text(path: String)` | `String` | Read file contents as text. A failed read returns `""`, so failure cannot be distinguished from a successfully read empty file using this function alone. |
| `write_text(path: String, contents: String)` | `Bool` | Write/truncate a file and report success. |
| `file_size(path: String)` | `Int` | Return file size in bytes, or `-1` if the size cannot be read. |
| `copy(source: String, destination: String)` | `Bool` | Copy a file, replacing an existing destination. Returns false if the copy fails. |
| `exists(path: String)` | `Bool` | Check whether the path exists. |
| `remove(path: String)` | `Bool` | Remove a file or an empty directory; reports failure as false. |
| `create_dir(path: String)` | `Bool` | Create a directory and any missing parent directories; reports failure as false. |

`file_size` uses bytes, not characters. File operations can fail because of permissions, invalid paths, missing files, or other operating-system errors.

## `std.env` — process environment

Import with `import std.env;`.

| Function | Return | Behavior |
| --- | --- | --- |
| `get(name: String)` | `String` | Read an environment variable; returns `""` if it is missing. |
| `has(name: String)` | `Bool` | Test whether the variable exists, including when its value is empty. |
| `set(name: String, value: String)` | `Bool` | Set an environment variable for the current process. |
| `current_dir()` | `String` | Return the current working directory, or an empty string if it cannot be obtained. |

Use `has` together with `get` when a missing variable must be distinguished from a variable set to an empty string. Environment changes affect the running process and are not a way to modify the parent shell's environment.

## `std.path` — lexical path helpers

Import with `import std.path;`. These functions inspect or combine path strings; they do not require the referenced file to exist.

| Function | Behavior |
| --- | --- |
| `join(left: String, right: String)` | Join two path components using the host platform's path rules. |
| `basename(value: String)` | Return the final path component. |
| `dirname(value: String)` | Return the parent path component. |
| `extension(value: String)` | Return the final extension without the dot (for `archive.tar.gz`, returns `gz`). |
| `stem(value: String)` | Return the final component without its final extension (for `archive.tar.gz`, returns `archive.tar`). |
| `is_absolute(value: String)` | Check whether the path is absolute according to the host platform. |

Path parsing and joining follow `std::filesystem` semantics, so separator and absolute-path behavior can differ between operating systems.

## Example

```alaco
import std.math;
import std.string;
import std.fs;

fn main() {
    println(math.clamp(12, 0, 10));
    println(string.repeat_string("ha", 3));

    if fs.write_text("hello.txt", "Hello, Alaco!") {
        println(fs.file_size("hello.txt"));
    }
}
```

The example creates `hello.txt` in the program's working directory. Remove it if you do not want to keep the file.

