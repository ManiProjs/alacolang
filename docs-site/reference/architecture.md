# Compiler architecture

Alaco's compiler is written in Rust. Its pipeline is organized so source understanding and validation can be shared across output targets.

## Compilation pipeline

<pre class="vp-architecture">Source (.aco)
    ↓
Lexer → Tokens
    ↓
Parser → AST
    ↓
Semantic analysis → names and types
    ↓
Validated intermediate representation (IR)
    ↓
Target code generation
    ├── C++ → system C++ compiler → native executable
    └── JavaScript ES module (.mjs)</pre>

## Front end

- **Lexer and parser:** convert `.aco` source into structured syntax and report malformed input.
- **Semantic analyzer:** checks declarations, names, types, calls, fields, and return paths before code generation.
- **Intermediate representation:** provides a target-oriented form for supported language constructs.
- **IR validation and optimization:** validate assumptions and apply compiler transformations before generation.
- **Diagnostics:** report structured errors with source spans where available; semantic errors use stable `alaco::analysis::*` identifiers.

## Runtime boundaries

The native backend emits C++ and depends on a system C++ compiler. Runtime support for selected filesystem, environment, path, HTTP, JSON, TOML, and timing operations is intentionally small. The JavaScript backend implements some equivalents in JavaScript, but the available runtime helpers do not yet match every native feature.

## Editor services

The `alaco lsp` command implements Language Server Protocol over stdio. It reuses the compiler's lexer, parser, and semantic analysis for editor diagnostics. See [Editor support](/guide/editor-support) for configuration and supported features.
