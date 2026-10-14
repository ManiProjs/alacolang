# Alaco examples

Run examples from the repository root, for example:

```sh
alaco run examples/string_interpolation.aco
alaco check examples/type_safety.aco
```

## Language basics

| File | What it demonstrates |
| --- | --- |
| `functions.aco` | Function declarations, calls, and return values |
| `hello_five_times.aco` | Fixed-count `loop (repeat count)` |
| `match.aco` | `match` branches and the `_` fallback |
| `struct.aco` | Struct construction, field access, and printing |
| `string_interpolation.aco` | Interpolating variables with `${name}` |
| `shell.aco` | Running a host shell command and printing its result |
| `type_safety.aco` | Typed structs and passing a struct to a function |
| `previous_when.aco` | Previous variable values and edge-triggered `when` blocks |
| `timed_variables.aco` | Variables whose values are automatically cleared after a duration |
| `math_toolkit.aco` | Integer math, primality, statistics, and port validation |
| `geometry_report.aco` | Geometry calculations, statistics, and unit conversion |

## Standard library

| File | What it demonstrates |
| --- | --- |
| `stdlib.aco` | Math, string, geometry, statistics, units, and validation helpers |
| `runtime_stdlib.aco` | Runtime filesystem, environment, and path operations; creates and removes temporary test files/directories in the working directory |
| `json_demo.aco` | JSON validation, top-level typed lookups, and fallback values |
| `toml_demo.aco` | Validation and reading simple scalar TOML settings |
| `config_app.aco` | Reading JSON/TOML configuration values and checking environment setup |
| `http_demo.aco` | HTTPS GET from `example.com`; requires network access |

## Modules and packages

| Path | What it demonstrates |
| --- | --- |
| `modules/simple_import.aco` | Importing a sibling module with a namespace |
| `modules/named_import.aco` | Importing one function with `import hello from helpers;` |
| `modules/main.aco` | Dotted imports and module aliases; currently a syntax illustration whose `helpers/greetings.aco` and `helpers/math_tools.aco` dependencies are not included in this checkout |
| `package/Alaco.toml` | Example package manifest |

## Intentional diagnostic fixture

`a_file_with_error.aco` is intentionally invalid: it calls `printl`, a misspelling of `println`. Use it to check that name-analysis diagnostics catch a typo:

```sh
alaco check examples/a_file_with_error.aco
```

This fixture is expected to fail semantic checking; it is not a passing runtime example. Do not run every file under `examples/` expecting every one to compile successfully.

The HTTP example also contains a commented JSON POST example for a test endpoint. TOML support is intentionally a subset, not a full TOML 1.0 parser. See the [language guide](../docs/language-guide.md) and [standard-library reference](../docs/standard-library.md) for syntax, APIs, and limitations.
