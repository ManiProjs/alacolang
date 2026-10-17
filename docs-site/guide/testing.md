# Testing Alaco programs

Alaco's `test` command discovers `.aco` test programs and builds and runs them as separate native programs. Use it to keep examples of expected behavior alongside your source code. For the compiler's own Rust tests, run `cargo test` from a checkout of the Alaco repository instead.

## Run the test suite

From the root of a project, run:

```sh
alaco test
```

You can also target a particular test file or directory:

```sh
alaco test tests/math.aco
alaco test tests/
```

Check `alaco test --help` for the exact options supported by your installed version. Tests are independent programs, so shared in-memory state between test files should not be expected.

## Write a focused test program

Keep each test small and make its expected result obvious. For example, a test can exercise a function and compare the result explicitly:

```text
fn square(value: Int) -> Int {
    return value * value;
}

fn main() {
    let actual = square(5);
    if actual != 25 {
        println("FAIL: square(5) should equal 25");
        // Exit status behavior depends on the program's supported runtime APIs;
        // don't treat printed output alone as an automatic assertion.
    } else {
        println("PASS: square(5)");
    }
}
```

This is a demonstration of a test shape, not a built-in assertion framework. A message printed by a program does not by itself make `alaco test` recognize a failed assertion; use the process exit behavior supported by your current runtime or inspect the output until a dedicated assertion API is available.

## Test in layers

1. Run `alaco check path/to/file.aco` to catch parse, name, and type errors quickly.
2. Run `alaco run path/to/file.aco` to exercise behavior during development.
3. Add a focused program under `tests/` and run `alaco test` to catch regressions.
4. Run `alaco build` when you need to validate the persistent native build path too.

## Be careful with side effects

Tests execute real programs on your machine. Review test source before running it, especially if it uses shell expressions, filesystem operations, environment variables, or network access. Keep test fixtures in a dedicated temporary directory and avoid depending on external services for basic language tests.

For command behavior, see the [CLI reference](/reference/cli). For the compiler's own implementation tests, the Rust suite is separate from `.aco` program tests.
