# Current limitations

Alaco is a hobby language and an evolving compiler. These limitations are part of the current implementation, not a promise that the missing features will work in a future release.

## Language and type system

- The built-in type system is deliberately small (`Int`, `Float`, `Bool`, `String`, and supported struct types). Do not assume Rust/C++ generics or broad implicit conversions.
- The documented loop form is `loop (repeat count)`; common constructs such as `while`, `break`, and advanced pattern matching are not generally available.
- Interpolation currently inserts variable names, rather than arbitrary expressions.
- `when` checks are synchronous checkpoints in a lexical block, not asynchronous reactive tasks.
- Timed variables use a timer thread per variable in the native implementation and are intended for short-lived values, not thousands of tiny-lived locals.

## Standard library

- JSON helpers validate JSON and access selected top-level typed values; they do not expose a dynamic JSON tree API.
- TOML helpers implement a subset of scalar settings and are not a complete TOML 1.0 parser.
- HTTP helpers expose response bodies but do not provide a full request/response abstraction with status and headers.
- Several APIs return a sentinel such as an empty string or `-1` on failure rather than a structured error type.

## Packages and modules

- Local imports resolve relative to the importing file. Manifest dependency declarations are parsed but do not download packages or make external packages importable.
- Named imports currently select one function per import statement.

## JavaScript backend

- Timed variables and shell expressions are not supported by JavaScript output.
- Runtime parity with the native C++ target is incomplete; test the generated `.mjs` file under Node.js.
- Target-specific support for some standard-library operations may be incomplete.

## Tooling

- The language server reparses changed documents rather than maintaining an incremental syntax tree.
- Workspace symbol/reference/rename features scan `.aco` files on demand; identifier matching is not fully scope-aware, so rename may affect unrelated same-named identifiers.

For practical examples and the current behavior of each module, see the [standard library reference](/reference/standard-library) and the repository's example catalog.
