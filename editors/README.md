# Alaco editor support

Editor integrations live here and intentionally have no third-party runtime dependencies.

## VS Code

Open `editors/vscode` as an extension development folder, or package it with `npx @vscode/vsce package` from that directory (requires Node.js and `@vscode/vsce`). Install the resulting VSIX using **Extensions: Install from VSIX...**. For local development, use **Developer: Install Extension from Location...** where supported, or press F5 with the folder open as an extension project.

The extension provides `.aco` syntax highlighting, snippets, file icons via the language identifier, and command-palette actions for checking, building, and running the current file. The `alaco` executable must be available on `PATH`.

## Neovim

Add `editors/nvim` to your runtime path, or copy its `ftdetect`, `syntax`, and `plugin` directories into `~/.config/nvim/`. No plugin manager is required. Open a `.aco` file and use `:AlacoCheck`, `:AlacoBuild`, or `:AlacoRun`.

## Vim

Copy `editors/vim/ftdetect`, `editors/vim/syntax`, and `editors/vim/plugin` into `~/.vim/` (or the equivalent Vim runtime directory). No plugin manager is required. Open a `.aco` file and use `:AlacoCheck`, `:AlacoBuild`, or `:AlacoRun`.

## Scope

These are syntax and CLI integrations, not language servers: semantic diagnostics, completion, go-to-definition, and refactoring are not claimed. Run commands use the current buffer's absolute path and quote it for the shell. Package commands can be run from a project terminal with `alaco manifest`, `alaco check`, `alaco build`, or `alaco run`.
