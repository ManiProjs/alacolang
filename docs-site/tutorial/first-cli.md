# Tutorial: build a tiny CLI

This tutorial builds a small command-line program from existing Alaco features. It deliberately avoids pretending that Alaco has a full command-line parsing library: arguments and advanced process APIs are outside the current documented surface, so the example focuses on input-free output and calculations.

## 1. Create a project

```sh
alaco new tally
cd tally
```

Replace the generated entry point with:

```text
import std.math;
import std.string;

fn label_for(value: Int) -> String {
    if value >= 10 {
        return "large";
    } else {
        return "small";
    }
}

fn main() {
    let start = 4;
    let doubled = math.square(start);
    println("Alaco tally");
    println("Start: ${start}");
    println("Squared: ${doubled}");
    println("Category: " + label_for(doubled));
    println(string.repeat_string("-", 18));
}
```


## 2. Check before running

```sh
alaco check
alaco run
```

Try changing `start` to a `String`, then check again. The type checker should reject using the string as an integer input. Restore the integer before continuing.

## 3. Build a native executable

```sh
alaco build --output ./tally
./tally
```

The build requires a supported C++ compiler. On systems where the executable path or invocation differs, consult `alaco build --help` and `alaco doctor`.

## 4. Explore next

Try adding a struct, a `match` statement, or a fixed-count repeat loop. The [language guide](/guide/language) describes supported constructs and the [CLI reference](/reference/cli) covers compiler commands.
