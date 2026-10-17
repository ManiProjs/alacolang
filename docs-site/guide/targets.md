# Compiler targets

Alaco separates front-end analysis from target-specific code generation. The current compiler offers a native C++ path and a JavaScript ES module transpiler. These targets have different runtime requirements and feature boundaries.

## Native C++

The native path is the primary executable workflow:

```sh
alaco check src/main.aco
alaco build src/main.aco
alaco run src/main.aco
```

The compiler validates the program, generates C++, then invokes the system C++ compiler. Native builds therefore require a supported C++ toolchain. The exact compiler and linker behavior depends on the host platform.

Use `alaco build --output path/to/program` to choose an output path. Project builds use the project's `target/` directory by default.

## JavaScript ES modules

Use the `transpile` command to generate an `.mjs` module:

```sh
alaco transpile examples/javascript.aco --target js --output examples/javascript.mjs
node examples/javascript.mjs
```

The backend currently handles common IR constructs such as functions, structs, control flow, loops, match branches, interpolation, and console output. It includes JavaScript runtime helpers for selected JSON, TOML, and environment operations.

### Differences and limitations

- Timed variables and shell expressions are not supported by JavaScript output.
- Some native standard-library functions may have no JavaScript implementation yet.
- TOML support is a small scalar-oriented subset, not a complete TOML 1.0 implementation.
- Successful transpilation is not a guarantee that every target runtime behavior matches native C++.

Keep a small executable test for each feature you depend on, and run the generated module under the Node.js version you intend to support.

## Choosing a target

| Need | Suggested target |
| --- | --- |
| Native executable built on the current machine | C++ |
| Experimenting with JavaScript integration | JavaScript ES modules |
| Platform-specific system access | Check the relevant standard-library implementation first |

The target matrix will grow as the compiler's shared IR and runtime abstractions mature.
