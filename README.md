# **shell-language-server**

This is a language server for shell scripts.

A language server is a program that provides "language intelligence" features
to text editors and any other compatible tools (clients).

It communicates with clients over `stdin`/`stdout` using the [Language Server
Protocol](https://en.wikipedia.org/wiki/Language_Server_Protocol), which is
based on [JSON-RPC](https://en.wikipedia.org/wiki/JSON-RPC).

## Table of contents

1. **[Features](#features)**
2. **[Planned features](#planned-features)**
3. **[Annotations](#annotations)**
4. **[Configuration](#configuration)**
5. **[Dependencies](#dependencies)**
6. **[Build](#build)**
7. **[Setup](#setup)**

## Features

- Go to definition
- Hover documentation
- Find and highlight references
- Rename variables and functions
- Complete variable, function, and command names
- Enhanced syntax highlighting with semantic tokens
- Code actions
    - Insert full command path
    - Apply Shellcheck fixes
- Annotations
- Diagnostics (warnings, errors, hints)
- Additional diagnostics and code actions through [shellcheck](https://www.shellcheck.net) integration
- Document and range formatting through [shfmt](https://github.com/mvdan/sh) integration
- Intelligent `man` and `help` integration based on the active shell

## Planned features

- Document symbols
- Signature help
- Module directives
- Scoped locals and parameters
- Highlight Shellcheck directives
- Dynamically register capabilities on configuration change
- Completion:
    - Paths
    - Command arguments
    - Comment directives
- Code actions:
    - Insert Shellcheck directives
    - Change shebang based on usage

## Annotations

Annotations are written with special comments that begin with `##@`. They can
be used by the language server for various purposes, such as improved hover
documentation and signature help.

Function annotations apply to the first function defined after the annotation.

- `desc`: General description
- `param`: Describe function parameters

Example use case:

```sh
##@ desc Write the number of entries in the given directory to stdout
##@ param Directory path
example () {
    ls -a "$1" | wc -l
}
```

## Configuration

The server can be configured with the `workspace/didChangeConfiguration`
notification or the `initialize` request with the `initializationOptions`
field. The settings must conform to the following JSON structure, where every
field is optional:

```
{
    "shell": {
        "integrate": {
            "man": boolean,
            "help": boolean,
            "shfmt": boolean,
            "shellcheck": boolean
        },
        "environment": {
            "path": string,
            "variables": boolean,
            "executables": boolean
        },
        "default_shell": string
    }
}
```

A fallback command line flag `--settings-json=ARG` is also provided. For
example: `--settings-json={"shell":{"integrate":{"shfmt":false}}}`

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
