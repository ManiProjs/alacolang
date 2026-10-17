# Project manifest

An Alaco project uses a file named `Alaco.toml` at its root. `alaco new` and `alaco init` create one for you.

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2026"
entry = "src/main.aco"

[dependencies]
# Example declarations; dependency fetching is not implemented yet.
# colors = "^1.0"
# local_tools = { path = "../local_tools" }
```

## Package fields

| Field | Required | Meaning |
| --- | --- | --- |
| `package.name` | Yes | Package identifier. |
| `package.version` | Yes | Semantic version string. |
| `package.edition` | No | Currently supports `2026`. |
| `package.entry` | No | Relative `.aco` entry point; defaults to `src/main.aco`. |
| `dependencies` | No | Dependency declarations are parsed and displayed, but not fetched or resolved into import namespaces yet. |

The entry path must be relative and point to a `.aco` file inside the package. Run `alaco manifest` to validate and inspect the manifest. Local `import` statements are still resolved relative to the importing source file; they do not currently use package registry resolution.

## Current package boundaries

The manifest format establishes metadata and a place to describe dependencies. It is not yet a package manager: dependency downloads, version solving, lockfiles, and third-party namespace exposure are not implemented.
