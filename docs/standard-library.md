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

## `std.json` — JSON text utilities

Import with `import std.json;`.

| Function | Behavior and limitations |
| --- | --- |
| `is_valid(text: String)` | Validate a JSON value, including nested arrays and objects, strings, numbers, booleans, and `null`. |
| `escape(text: String)` | Escape string contents for use inside a JSON string; it does not add surrounding quotes. |
| `quote(text: String)` | Escape a string and add JSON double quotes. |
| `has_key(text: String, key: String)` | Check for a key in a top-level JSON object. |
| `get_string(text: String, key: String, fallback: String)` | Read a top-level string property or return the fallback. |
| `get_int(text: String, key: String, fallback: Int)` | Read a top-level integer property or return the fallback. |
| `get_bool(text: String, key: String, fallback: Bool)` | Read a top-level boolean property or return the fallback. |

These typed accessors read top-level object properties; they do not expose a dynamic JSON value/map/list type or recursively navigate nested objects. String escape decoding covers common escapes.

## `std.toml` — TOML configuration utilities

Import with `import std.toml;`.

| Function | Behavior and limitations |
| --- | --- |
| `is_valid(text: String)` | Validate a limited subset of scalar TOML assignments and section headers. |
| `has_key(text: String, key: String)` | Check for a top-level key. |
| `get_string(text: String, key: String, fallback: String)` | Read a top-level quoted string or return the fallback. |
| `get_int(text: String, key: String, fallback: Int)` | Read a top-level integer or return the fallback. |
| `get_bool(text: String, key: String, fallback: Bool)` | Read a top-level boolean or return the fallback. |

This is not a complete TOML 1.0 parser. Accessors currently target top-level scalar keys; table-scoped keys, full arrays/inline tables, dates, and every grammar edge case are not fully supported.

## `std.http` — HTTP requests

Import with `import std.http;`.

| Function | Behavior and limitations |
| --- | --- |
| `is_url(url: String)` | Check whether a URL starts with `http://` or `https://`. |
| `get(url: String)` | Return the response body for a successful GET request. |
| `post_json(url: String, body: String)` | POST a body with `Content-Type: application/json` and return the response body. |

No `curl` executable is spawned. Windows uses WinHTTP, macOS uses CFNetwork, and other Unix-like builds use OpenSSL 1.1.0+ for TLS with native sockets for transport. The OpenSSL development headers and libraries must be installed when compiling an HTTP-using program on those Unix-like systems. HTTPS certificate and hostname verification are enabled. Failed requests and non-2xx responses return an empty string, so an empty response cannot be distinguished from a failure. The API does not expose status codes or response headers.

Runnable examples: `examples/http_demo.aco`, `examples/json_demo.aco`, `examples/toml_demo.aco`, and `examples/type_safety.aco`.

## `std.timing` — basic time utilities

Import with `import std.timing;`.

| Function | Behavior |
| --- | --- |
| `unix_seconds()` | Current Unix timestamp in whole seconds. |
| `sleep_ms(milliseconds: Int)` | Pause the current thread for the requested number of milliseconds; non-positive values do nothing. |

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
