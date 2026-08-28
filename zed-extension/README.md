# Binary Preview for Zed

Binary Preview adds hover information for numeric literals and binary file paths
in C, C++, Rust, and Python source files.

![Binary Preview hover demo](https://raw.githubusercontent.com/exelsior87/binary-preview/main/assets/binary-preview-demo.gif)

## Why this is a language server extension

The original goal was to open executables and libraries in a dedicated binary
editor view. The Zed extension API used by this project does not expose an API
for extensions to create arbitrary custom editor views or GUI panels.

Binary Preview therefore provides the analysis through the Language Server
Protocol. Instead of opening the binary directly, hover over a binary file path
in a supported source file. The server resolves the path and returns the file
size, header hex dump, and format-specific metadata as Markdown hover content.
Numeric literals are decoded through the same mechanism.

The binary parsing logic lives in the standalone `binary-preview-lsp` server,
so it can be reused by Zed and other LSP-capable editors. A dedicated binary
view can be added later if the extension API provides the necessary UI support.

## Features

- Decimal, hexadecimal (`0x`), and binary (`0b`) integer literals
- Decimal floating-point and scientific notation
- Binary file paths in quoted and unquoted source text
- PE, ELF, Mach-O, Universal Mach-O, and archive summaries
- File size and a hex dump of the first 64 bytes

## Language server installation

The extension first looks for `binary-preview-lsp` on the worktree's `PATH`.
If it is not found, the extension downloads the appropriate executable from the
latest [GitHub release](https://github.com/exelsior87/binary-preview/releases).
If the release cannot be checked, the extension uses the most recently cached
server when one is available.

The officially tested and supported platforms are Windows x86_64 and macOS
Apple Silicon.

## Development

Build the language server:

```sh
cargo build --manifest-path binary-preview-lsp/Cargo.toml
```

Build the Zed extension:

```sh
cargo build --manifest-path zed-extension/Cargo.toml
```

Put the locally built `binary-preview-lsp` executable on `PATH` to make the
extension use it instead of downloading a release.

## Known limitations

- Numeric literal parsing supports a common subset of C-like syntax.
- Unary signs are not included in numeric literal analysis.
- String escape sequences are not decoded when resolving file paths.
- Binary files are inspected through hover information rather than a dedicated
  binary editor.

## Source and issues

Source code and issue tracking are available in the
[Binary Preview repository](https://github.com/exelsior87/binary-preview).

## License

MIT
