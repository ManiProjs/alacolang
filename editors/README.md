# Alaco editor support

Editor integrations live here and intentionally have no third-party runtime dependencies.

## VS Code

Open `editors/vscode` as an extension development folder, or package it with `npx @vscode/vsce package` from that directory (requires Node.js and `@vscode/vsce`). Install the resulting VSIX using **Extensions: Install from VSIX...**. For local development, use **Developer: Install Extension from Location...** where supported, or press F5 with the folder open as an extension project.

The extension provides `.aco` syntax highlighting, snippets, command-palette actions for checking/building/running, and starts `alaco lsp` automatically. LSP diagnostics, completion, hover, navigation, rename, formatting, semantic tokens, code actions, inlay hints, code lenses, and import links are provided by the server. Set `alaco.executable` to an absolute path if `alaco` is not on VS Code's `PATH`; set `alaco.languageServer.enabled` to `false` to disable LSP startup.

## Neovim

Add `editors/nvim` to your runtime path, or copy its `ftdetect`, `syntax`, and `plugin` directories into `~/.config/nvim/`. No plugin manager is required; Neovim's built-in LSP client starts `alaco lsp` automatically for `.aco` files. Use `vim.g.alaco_executable = "/absolute/path/to/alaco"` before loading the plugin if needed. Open a `.aco` file and use `:AlacoCheck`, `:AlacoBuild`, or `:AlacoRun`. LSP mappings are included: `gd` definition, `gr` references, `K` hover, `<leader>rn` rename, `<leader>ca` code action, `[d`/`]d` previous/next diagnostic, and `<leader>f` format.

## Vim

Copy `editors/vim/ftdetect`, `editors/vim/syntax`, and `editors/vim/plugin` into `~/.vim/` (or the equivalent Vim runtime directory). Syntax and compiler commands work without dependencies. For LSP support, install [`prabirshrestha/vim-lsp`](https://github.com/prabirshrestha/vim-lsp); the plugin automatically registers `alaco lsp` for the `alaco` filetype. Set `g:alaco_executable` to an absolute executable path if needed. Use vim-lsp's `:LspDefinition`, `:LspHover`, `:LspReferences`, `:LspRename`, and `:LspDocumentDiagnostics` commands. Open a `.aco` file and use `:AlacoCheck`, `:AlacoBuild`, or `:AlacoRun`.

## Scope

Run commands use the current buffer's absolute path. The LSP uses the existing compiler lexer, parser, and analyzer; it speaks JSON-RPC over stdio and never writes logs to stdout. Package commands can be run from a project terminal with `alaco manifest`, `alaco check`, `alaco build`, or `alaco run`.
