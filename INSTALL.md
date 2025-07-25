# Installation Guide for Dociium

Dociium is a high-performance Rust Documentation MCP Server that provides fast access to Rust crate documentation through the Model Context Protocol (MCP).

## Quick Installation

### From Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/example/dociium.git
cd dociium

# Install directly from workspace root
cargo install --path .
```

### Alternative: Install from specific package

```bash
# Install from the mcp_server package
cargo install --path mcp_server
```

### From crates.io (Coming Soon)

```bash
# This will be available once published
cargo install dociium
```

## System Requirements

- **Rust**: 1.70.0 or later
- **Platform**: Linux, macOS, Windows
- **Memory**: 512MB RAM minimum, 1GB recommended
- **Disk**: 100MB for cache storage

## Verification

After installation, verify that `dociium` is available:

```bash
which dociium
# Should output: /Users/[username]/.cargo/bin/dociium
```

## Usage

### As MCP Server

Dociium is designed to work as an MCP server. Configure your MCP client to use:

```json
{
  "servers": {
    "dociium": {
      "command": "dociium",
      "args": []
    }
  }
}
```

### Environment Variables

- `RDOCS_CACHE_DIR`: Custom cache directory (default: `~/.cache/rdocs-mcp`)
- `RUST_LOG`: Logging level (default: `info`)

### Cache Management

The server automatically manages cache in `~/.cache/rdocs-mcp/`. You can:

- **View cache stats**: Use the `get_cache_stats` MCP tool
- **Clear cache**: Use the `clear_cache` MCP tool
- **Manual cleanup**: Remove the cache directory

## Available MCP Tools

Once running, Dociium provides these tools:

### Documentation Tools
- `get_item_doc`: Get documentation for specific Rust items
- `search_symbols`: Search for symbols within crates
- `source_snippet`: Get source code snippets

### Discovery Tools  
- `search_crates`: Search for crates on crates.io
- `crate_info`: Get detailed crate information

### Implementation Tools
- `list_trait_impls`: List trait implementations
- `list_impls_for_type`: List implementations for types
- `get_implementation`: Get implementation context

### Cache Tools
- `get_cache_stats`: View cache statistics
- `clear_cache`: Clear crate cache
- `cleanup_cache`: Remove expired cache entries

## Performance

Dociium is optimized for speed:

- **First request**: ~1 second for popular crates
- **Cached requests**: <1ms response time
- **Memory usage**: <100MB typical
- **Concurrent requests**: Fully async, no blocking

## Troubleshooting

### Installation Issues

**Error: `cargo install` fails with workspace error**
```bash
# Solution: Install from specific package
cargo install --path mcp_server
```

**Error: Binary already exists**
```bash
# Solution: Force reinstall
cargo install --path . --force
```

### Runtime Issues

**Error: Connection timeout**
- Check internet connectivity to docs.rs
- Verify firewall settings allow HTTPS requests
- Try clearing cache: remove `~/.cache/rdocs-mcp`

**Error: Cache permission issues**
```bash
# Fix permissions
chmod -R 755 ~/.cache/rdocs-mcp
```

**Error: High memory usage**
- Clear cache periodically
- Reduce `RDOCS_CACHE_SIZE` if set
- Restart the server process

## Development

### Building from Source

```bash
git clone https://github.com/example/dociium.git
cd dociium

# Build all packages
cargo build --release

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run --bin dociium
```

### Project Structure

```
dociium/
├── mcp_server/     # Main MCP server implementation
├── doc_engine/     # Documentation fetching and processing
├── index_core/     # Search and indexing functionality
└── tests/          # Integration tests
```

## Uninstallation

```bash
# Remove the binary
cargo uninstall dociium

# Remove cache (optional)
rm -rf ~/.cache/rdocs-mcp
```

## Support

- **Issues**: Report on GitHub Issues
- **Documentation**: See README.md
- **Performance**: See TESTING.md for benchmarks

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT License

at your option.