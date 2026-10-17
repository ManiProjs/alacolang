# Getting started

This guide takes you from a source checkout to a running Alaco program.

## 1. Install prerequisites

You need:

- **Rust and Cargo** to build the compiler from this repository.
- **A C++ compiler** supported by your platform for native `build` and `run` commands.
- **Node.js** only if you want to execute programs transpiled to JavaScript.

Clone the repository and build the compiler:

```sh
git clone https://github.com/ManiProjs/alaco.git
cd alaco
cargo build
```

Run `cargo run -- doctor` to check the native compiler setup. While developing Alaco itself, `cargo run -- <command>` invokes the compiler from this checkout. After installing the executable, use `alaco <command>` directly.

## 2. Create a project

```sh
cargo run -- new hello
cd hello
```

The generated project includes `Alaco.toml` and a source entry point. A typical entry point looks like this:

```text
fn main() {
    let language = "Alaco";
    println("Hello from " + language + "!");
}
```

`std.io` is available by default, so you do not need to import it for `print` or `println`.

## 3. Check, run, and build

```sh
cargo run -- check
cargo run -- run
cargo run -- build
```

- `check` parses and analyzes source without producing a native executable.
- `run` builds a temporary native executable, runs it, and cleans up the temporary output.
- `build` leaves the executable in the project's `target/` directory unless you choose an explicit output path.

A successful check means the Alaco front end accepted the program. Native compilation can still fail later if the generated C++ or the platform toolchain encounters a problem.

## 4. Make an intentional mistake

Try changing `println` to `printl`, then run `check` again. The semantic analyzer should report an unknown function and may suggest a correction. This is the central development loop: edit, check, run, and repeat.

## 5. Transpile to JavaScript

From the Alaco repository, run:

```sh
cargo run -- transpile examples/javascript.aco --target js --output hello.mjs
node hello.mjs
```

JavaScript output is an evolving target. Check the [target guide](/guide/targets) before relying on target-specific standard-library behavior.

## Next steps

- Read the [language guide](/guide/language).
- Learn the [project workflow](/guide/project-workflow).
- Browse the [CLI reference](/reference/cli) and [standard library](/reference/standard-library).
