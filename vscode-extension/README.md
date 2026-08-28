# Binary Preview for VS Code

Binary Preview adds hover information for numeric literals and binary file paths
in C, C++, Rust, and Python source files.

The extension starts `binary-preview-lsp` and downloads the matching executable
from the latest GitHub release when needed. To use a local build instead, set
`binaryPreview.server.path` to its absolute path.

If GitHub cannot be reached, the extension starts the most recently downloaded
language server from its local cache.

Use **Binary Preview: Restart Language Server** from the Command Palette after
changing the server executable.
