# Alaco Language Server

Alaco ships a standalone Language Server Protocol (LSP) server in the `alaco` executable. It uses the compiler's existing lexer and parser, and the existing semantic analyzer for diagnostics. The server speaks JSON-RPC over standard input/output; **do not use stdout for logs** when running it as an LSP.

## Start the server

Build the compiler once, then configure your editor to run:

```sh
alaco lsp
```

When working from the repository before installing Alaco, use `cargo run -- lsp` (or the absolute path to `target/debug/alaco lsp`).

## Editor features

- Lexer, parser, and semantic diagnostics on open/change/save, including Alaco diagnostic codes and source ranges.
- Completion for language keywords, primitive types, symbols in the current file, standard-library modules, and common standard-library APIs.
- Hover for language constructs, user-defined functions and structs, primitive literals, I/O functions, and documented standard-library helpers.
- Go to definition, find references, document highlights, and rename across open documents.
- Function signature help, parameter-name inlay hints, document/workspace symbols, folding ranges, selection ranges, and semantic tokens.
- Document/range/on-type formatting and a source action to sort top-level imports.
- Code lenses to check the current document, and links from import paths to matching local source files when those files exist in the workspace.
- Resolved completion documentation, plus the `alaco.checkWorkspace` server command to refresh diagnostics for all open documents.

The implementation is deliberately in-process and currently reparses a changed document rather than maintaining an incremental syntax tree. Definitions, references, rename, and workspace symbols scan `.aco` files beneath the workspace root on demand; unsaved open buffers override the corresponding files on disk. This avoids requiring every file to be opened, but very large workspaces may benefit from a cached index and file watching. Symbol matching remains lexical, so rename is not yet scope-aware and can affect same-named identifiers that are unrelated. Code actions currently focus on import organization, and inlay hints show parameter names for known local function calls.

## VS Code

The bundled extension in `editors/vscode` starts `alaco lsp` automatically. Package it from that directory with `npx @vscode/vsce package`, then install the generated VSIX. Configure `alaco.executable` if the executable is not available on VS Code's `PATH`.

## Neovim (built-in LSP)

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "alaco",
  callback = function(args)
    vim.lsp.start({
      name = "alaco",
      cmd = { "alaco", "lsp" },
      root_dir = vim.fs.root(args.buf, { "Alaco.toml", ".git" }) or vim.fn.getcwd(),
    })
  end,
})
```

Make sure `.aco` files have the `alaco` filetype (for example, add `vim.filetype.add({ extension = { aco = "alaco" } })`).

The shipped plugin under `editors/nvim` configures this automatically and uses `vim.g.alaco_executable` if set.

## Vim

Install `prabirshrestha/vim-lsp` and copy `editors/vim/{ftdetect,syntax,plugin}` into your Vim runtime directory. The plugin registers `alaco lsp` for the `alaco` filetype. Set `g:alaco_executable` to override the executable path.

## Protocol details

The server supports UTF-16 LSP positions, full-document synchronization, standard Content-Length framing, and the usual `initialize`/`shutdown` lifecycle. It stores open documents in memory and sends diagnostics using `textDocument/publishDiagnostics`. It does not execute source code or invoke `clang++` during normal editor requests.
