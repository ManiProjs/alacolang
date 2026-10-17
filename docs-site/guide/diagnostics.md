# Diagnostics and troubleshooting

Alaco can reject a program during parsing, semantic analysis, native C++ compilation, or execution. Identify which stage failed before changing code: each stage points to a different kind of problem.

## Start with `check`

Run the checker on the smallest source file that reproduces the issue:

```sh
alaco check src/main.aco
```

The check command validates Alaco syntax, names, and types. It does not prove that the generated C++ will compile or that the program will behave correctly at runtime.

## Understand the failure stage

| Symptom | Likely stage | What to try |
| --- | --- | --- |
| Unexpected token or malformed block | Parsing | Check braces, delimiters, statement endings, and supported syntax. |
| Unknown function, type, module, or field | Semantic analysis | Check spelling, imports, aliases, and the declaration's scope. |
| Argument or assignment type mismatch | Semantic analysis | Compare the expected type with the value's actual type; don't assume implicit conversions. |
| C++ compiler missing or native build fails | Toolchain/code generation | Run `alaco doctor` and verify that a supported C++ compiler is installed. |
| Program compiles but fails when run | Runtime | Reduce the input and inspect file paths, environment, timers, shell commands, and other external effects. |
| JavaScript output behaves differently | JavaScript target | Run the generated `.mjs` with Node.js and check the [target guide](/guide/targets) for incomplete support. |

## Read source locations

Semantic diagnostics use `alaco::analysis::*` diagnostic codes. When source location information is available, the compiler labels the relevant expression or declaration. A whole-program problem, such as a missing entry point, may not have a specific source range to highlight.

When a diagnostic seems unrelated to the highlighted line, reduce the file until the issue remains. Then check nearby declarations and imports; a typo or invalid type can cause later expressions to fail analysis too.

## Check imports

Local imports are resolved relative to the file containing the import, not the shell's current directory. For example, `import helpers.greetings;` expects a `helpers/greetings.aco` path relative to that source file. Confirm capitalization and directory names, and check for cyclic imports or duplicate aliases.

Manifest dependency entries do not download packages or make third-party modules importable. See the [project workflow](/guide/project-workflow) and [manifest reference](/reference/manifest).

## Check the native toolchain

If source checking succeeds but `build` or `run` fails, verify the C++ compiler setup:

```sh
alaco doctor
alaco build src/main.aco
```

Keep the first error from the native compiler; later messages may be follow-on errors. If you report a bug, include a minimal `.aco` example, the command you ran, the first useful diagnostic, your operating system, and the output of `alaco --help` or `alaco doctor` when relevant. Remove credentials and other private information from logs before sharing them.

See [current limitations](/reference/limitations) for behavior that is not implemented yet.
