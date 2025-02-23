# **shell-language-server**

This is a language server for shell scripts.

A language server is a program that provides "language intelligence" features
to text editors and any other compatible tools (clients).

It communicates with clients over `stdin`/`stdout` using the [Language Server
Protocol](https://en.wikipedia.org/wiki/Language_Server_Protocol), which is
based on [JSON-RPC](https://en.wikipedia.org/wiki/JSON-RPC).

Note that the project is far from done, and in many cases will not work
perfectly. Several shells are recognized, but they are all treated as POSIX
shell for now. Despite these limitations, the language server is already
useful.

## Table of contents

1. **[Supported features](#supported-features)**
2. **[Planned features](#planned-features)**
3. **[Annotations](#annotations)**
4. **[Configuration](#configuration)**
5. **[Dependencies](#dependencies)**
6. **[Build](#build)**
7. **[Setup](#setup)**

## Supported features

- Go to definition
- Hover documentation
- Find references
- Highlight references
- Rename variables and functions
- Complete variable, function, and command names
- Diagnostics reporting
- Annotations
- `shellcheck` integration
- `shfmt` integration

## Planned features

- Document symbols
- Signature help
- Code actions (e.g. replace a command name with its absolute path)
- Command argument completion
- Imports and exports
- Syntax highlighting

## Annotations

Annotations are written with special comments that begin with `##@`. They can
be used by the language server for various purposes, such as improved hover
documentation and signature help.

Function annotations apply to the first function defined after the annotation.

- `desc`: General description
- `param`: Describe function parameters
- `stdout`, `stderr`, `stdin`: Specify how standard input streams are used
- `exit`: Specify the exit status

Example use case:

```sh
##@ desc Get the number of entries in the given directory
##@ param Directory path
##@ stdout Number of directory entries
example () {
    ls -a "$1" | wc -l
}
```

## Configuration

The server can be configured with the following command line arguments:

- `--no-env-path`: Do not complete commands available through `$PATH`.
- `--no-env-vars`: Do not complete environment variable names.
- `--no-env`: Equivalent to `--no-env-path --no-env-vars`.
- `--path=ARG`: Use the given argument instead of `$PATH`.
- `--default-shell=SH`: Default to the given shell when a script has no shebang.
- `--exe=NAME:PATH`: Specify the path to an executable. Can be specified multiple times.
- `--shellcheck=BOOL`: Enable or disable shellcheck integration. Defaults to true.
- `--shfmt=BOOL`: Enable or disable shfmt integration. Defaults to false.
- `--debug`: Log every LSP request and response to `stderr`.

Consult the man page for examples and more information.

## Dependencies

`shell-language-server` depends on [serde](https://github.com/serde-rs/serde) +
[serde_json](https://github.com/serde-rs/json) for JSON serialization and
deserialization.

## Build

To build the executable, run `cargo build --release`. The executable will be
placed at `target/release/shell-language-server`.

## Setup

These instructions assume `shell-language-server` has been installed locally
and can be run without specifying its absolute path.

### Neovim

No plugins needed. You can customize client behavior and key bindings with the
[LspAttach](https://neovim.io/doc/user/lsp.html#LspAttach) event. Place the
following Lua snippet in your Neovim configuration file to automatically start
`shell-language-server` when you open a shell script:

```lua
vim.api.nvim_create_autocmd('FileType', {
    callback = function ()
        vim.lsp.start({
            name = 'shell-language-server',
            cmd = { 'shell-language-server' },
        })
    end,
    pattern = 'sh',
    group = vim.api.nvim_create_augroup('shell-language-server', { clear = true }),
    desc = 'Automatically start shell-language-server',
})
```
