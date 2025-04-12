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
- Scoped local variables and parameters
- Diagnostics (errors, warnings, hints)
- Additional diagnostics and code actions through [Shellcheck](https://www.shellcheck.net) integration
- Document and range formatting through [shfmt](https://github.com/mvdan/sh) integration
- Intelligent `man` and `help` integration based on the active shell
- Code actions
    - Insert full command path
    - Insert Shellcheck directives
    - Apply Shellcheck fixes
- Document symbols
- Enhanced syntax highlighting with semantic tokens
- Annotations
- Inlay hints:
    - Parameter annotation indices

## Planned features

- Signature help
- Module directives
- Highlight Shellcheck directives
- Dynamically register capabilities on configuration change
- Completion:
    - Paths
    - Command arguments
    - Comment directives
- Code actions:
    - Inline environment variables
    - Change shebang based on usage

## Annotations

Annotations are written with special comments that begin with `##@`. They can
be used by the language server for various purposes, such as improved hover
documentation and signature help.

### `##@ desc`
Provide a general description of the next function or variable.

### `##@ param`
Provide a description of the current function parameter or script parameter.
The first annotation applies to `$1`, the second one to `$2`, and so on.

The server provides inlay hints that label the annotations with their
corresponding parameter indices.

### `##@ script`
Apply previous `param` annotations to the script instead of the next function.

### Example use case

```sh
##@ desc Write the number of entries in the given directory to stdout
##@ param Directory path
example () {
    ls -a -- "$1" | wc -l
}
```

## Configuration

The server can be configured with the `workspace/didChangeConfiguration`
notification or the `initialize` request with the `initializationOptions`
field.

For example, to set `shell.defaultShell` to `"bash"` and leave other options to
their default values, use the following JSON object:

```json
{
    "shell": {
        "defaultShell": "bash"
    }
}
```

Supported configuration options are listed below:

### `shell.defaultShell`
- type: `string`
- default: `"sh"`
- description: The shell dialect to use when a document has no shebang.

### `shell.integrate.shellcheck.enable`
- type: `boolean`
- default: `true`
- description: Whether to enable Shellcheck integration.

### `shell.integrate.shellcheck.posixFallback`
- type: `boolean`
- default: `true`
- description: Whether to fall back to POSIX shell when Shellcheck does not support the current document's shell dialect.

### `shell.integrate.shellcheck.arguments`
- type: `string[]`
- default: `[]`
- description: Additional command line arguments to be passed to `shellcheck`.

### `shell.integrate.shfmt.enable`
- type: `boolean`
- default: `true`
- description: Whether to enable Shfmt integration.

### `shell.integrate.shfmt.posixFallback`
- type: `boolean`
- default: `true`
- description: Whether to fall back to POSIX shell when Shfmt does not support the current document's shell dialect.

### `shell.integrate.shfmt.arguments`
- type: `string[]`
- default: `[]`
- description: Additional command line arguments to be passed to `shfmt`.

### `shell.integrate.help.enable`
- type: `boolean`
- default: `true`
- description: Whether to enable `help` integration.

### `shell.integrate.man.enable`
- type: `boolean`
- default: `true`
- description: Whether to enable `man` integration.

### `shell.integrate.man.arguments`
- type: `string[]`
- default: `[]`
- description: Additional command line arguments to be passed to `man`.

### `shell.environment.path`
- type: `string[]`
- default: The server's `$PATH`.
- description: Directory paths to be used for executable discovery.

### `shell.environment.variables`
- type: `boolean`
- default: `true`
- description: Whether the server should inspect its environment variables. When this is set to `true`, environment variable names can be provided as completions.

### `shell.environment.executables`
- type: `boolean`
- default: `true`
- description: Whether the server should be aware of executables available through the `PATH` environment variable.

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
