# Binary Preview

Binary Preview is a language server and a set of editor extensions that show
useful information when hovering over numeric literals and binary file paths in
source code.

![Binary Preview hover demo](assets/binary-preview-demo.gif)

## Features

- C, C++, Rust, and Python source files
- Decimal, hexadecimal (`0x`), and binary (`0b`) integer literals
- Decimal floating-point and scientific notation
- Binary file paths in quoted and unquoted source text
- PE, ELF, Mach-O, Universal Mach-O, and archive summaries
- File size and a hex dump of the first 64 bytes

## Editor support

- [VS Code extension](vscode-extension): installation, settings, automatic LSP
  downloads, and local development
- [Zed extension](zed-extension): installation, LSP integration, and the design
  background specific to Zed

Both extensions use the same `binary-preview-lsp` implementation and provide
the same hover analysis.

## Repository structure

- `binary-preview-lsp/`: Rust language server and binary parsers
- `vscode-extension/`: VS Code language client
- `zed-extension/`: Zed language server extension
- `assets/`: shared project assets

## Supported platforms

Tested and supported platforms are:

- Windows x86_64
- macOS Apple Silicon

## Development

Build and test the language server:

```sh
cargo test --manifest-path binary-preview-lsp/Cargo.toml
cargo build --release --manifest-path binary-preview-lsp/Cargo.toml
```

Editor-specific build and development instructions are available in each
extension directory.

## Known limitations

Numeric literal parsing supports a common subset of C-like syntax rather than
each language's complete grammar.

- Python octal, underscore-separated, complex, and arbitrary-precision integer
  literals are not fully supported.
- Rust type suffixes and underscore-separated literals are not yet supported.
- Unary signs are not included in numeric literal analysis.
- String escape sequences are not decoded when resolving file paths.
- Binary files are inspected through source-code hover information rather than
  a dedicated binary editor view.

## License

MIT
