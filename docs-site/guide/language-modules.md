# Modules and strings

## Imports

Import a standard-library or local module. A single-component local import resolves to a sibling `.aco` file (`import helpers;` loads `helpers.aco`); dotted local imports map intermediate components to directories and the final component to a `.aco` file (`import helpers.greetings;` loads `helpers/greetings.aco`). Paths are resolved relative to the importing `.aco` file. The final component is the namespace used to call its functions by default. Use `as` to choose a different namespace.

To call a specific function without the module prefix, use a named import:

```aco
import hello from helpers;

fn main() {
    println(hello("Alaco"));
}
```

This loads `helpers.aco` and makes `hello` directly callable. Named imports currently support one function name per import statement. Try it with `alaco run examples/modules/named_import.aco` from the repository root.

```aco
import std.math;
import helpers;
import helpers.greetings;
import helpers.math_tools as maths;

fn main() {
    println(math.clamp(12, 0, 10));
    println(greetings.hello("Alaco"));
    println(maths.square(9));
}
```

For these local imports, the importing directory can contain `helpers.aco` as well as `helpers/greetings.aco` and `helpers/math_tools.aco` under a `helpers/` directory. Imported modules can import other modules relative to their own file; dependencies are loaded first. Duplicate aliases and cyclic imports are reported as errors. Run `alaco run examples/modules/main.aco`, `alaco run examples/modules/simple_import.aco`, or `alaco run examples/modules/named_import.aco` from the repository root to try the examples.

See the [standard-library reference](/reference/standard-library) for available modules and their behavior.

## Import resolution notes

- `import helpers;` resolves to `helpers.aco` next to the importing file.
- `import helpers.greetings;` resolves to `helpers/greetings.aco` next to the importing file.
- `import helpers as util;` gives the module a local alias.
- `import hello from helpers;` imports one named function without the module prefix.

Imports are resolved relative to the source file containing the import, not necessarily the shell's current directory. Imported modules are loaded before their dependents. Cyclic imports and duplicate module aliases are errors. Named imports currently select a single function per statement; package dependency declarations do not yet download packages or change this local import behavior.

## String interpolation

Insert a variable into a string with `${name}` syntax. Literal text and variable values are combined at runtime:

```aco
fn main() {
    let name = "Alaco";
    let version = 2026;
    println("Hello, ${name}! Welcome to Alaco ${version}.");
}
```

Interpolation currently supports variable names (including numeric and boolean variables), not arbitrary expressions. To try it, run `alaco run examples/string_interpolation.aco` from the repository root. Strings without `${...}` keep their existing behavior, including JSON text containing braces.

Interpolation is useful when a message combines fixed text with a value. For a calculation, compute the result first and interpolate the resulting variable:

```aco
fn main() {
    let subtotal = 18;
    let quantity = 3;
    let total = subtotal * quantity;
    println("Total: ${total}");
}
```
