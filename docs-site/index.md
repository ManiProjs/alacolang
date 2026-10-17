# Alaco documentation

Alaco is a small, statically checked programming language implemented in Rust. Its compiler analyzes `.aco` source files and currently supports native executable builds through generated C++ plus an evolving JavaScript ES module target.

<div class="vp-doc-intro-actions">
  <a class="vp-doc-primary" href="./guide/getting-started">Get started <span>→</span></a>
  <a class="vp-doc-secondary" href="./guide/language">Explore the language <span>→</span></a>
</div>

## Follow a path

<div class="vp-doc-cards">
  <a href="./guide/getting-started"><strong>01 · Set up</strong><span>Build Alaco, create a project, and run your first program.</span></a>
  <a href="./guide/language"><strong>02 · Learn syntax</strong><span>Understand types, functions, control flow, structs, and imports.</span></a>
  <a href="./reference/standard-library"><strong>03 · Use the library</strong><span>Explore the APIs available to Alaco programs.</span></a>
  <a href="./guide/editor-support"><strong>04 · Improve your workflow</strong><span>Connect the language server to your editor.</span></a>
  <a href="./guide/testing"><strong>05 · Test programs</strong><span>Run focused `.aco` tests and keep regressions visible.</span></a>
  <a href="./guide/diagnostics"><strong>06 · Troubleshoot</strong><span>Understand compiler stages and diagnose common failures.</span></a>
</div>

## At a glance

- **Source files:** `.aco`
- **Compiler implementation:** Rust
- **Native output:** generated C++ compiled with a system C++ compiler
- **JavaScript output:** ES modules for Node.js, with an evolving set of supported features
- **Editor tooling:** Language Server Protocol, including diagnostics, completion, hover, navigation, and code actions

Alaco is actively evolving. These pages describe the current implementation, including limitations, rather than promising compatibility with Rust, C++, or a complete language specification.
