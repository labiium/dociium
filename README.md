# Dociium

**Fast documentation access for Rust, Python, and Node.js packages via MCP (Model Context Protocol)**

Get instant access to documentation and source code from your AI assistant. Works with Claude Desktop, Continue, and other MCP-compatible tools.

## Quick Start

### Install

```bash
git clone https://github.com/labiium/dociium.git
cd dociium
cargo install --path .
```

### Configure your MCP client

Add to your MCP client configuration:

```json
{
  "servers": {
    "dociium": {
      "command": "dociium"
    }
  }
}
```

### Use with your AI assistant

Ask your AI assistant:
- "Show me the documentation for tokio::sync::Mutex"
- "Search for HTTP client crates in Rust"
- "What traits does Vec implement?"
- "Get the source code for the requests.get function in Python"

## What it does

- **🦀 Rust**: Search crates.io, get documentation, view source code, explore trait implementations
- **🐍 Python**: Access local package documentation and source code
- **📦 Node.js**: Browse TypeScript/JavaScript package implementations
- **⚡ Fast**: Intelligent caching makes repeated queries instant
- **🔍 Smart**: Fuzzy search, trait exploration, and cross-references

## Available Tools

| Tool | Use Case | Example |
|------|----------|---------|
| `search_crates` | Find Rust packages | Search for "async http client" |
| `get_item_doc` | Get documentation | Documentation for `tokio::sync::Mutex` |
| `crate_info` | Package details | Info about the `serde` crate |
| `list_trait_impls` | See trait implementations | What implements `Iterator`? |
| `source_snippet` | View source code | Source for `Vec::push` |
| `get_implementation` | Local Python/Node packages | Get `requests.get` from your venv |

## Examples

### Rust Documentation
```
Ask: "What is tokio::sync::Mutex and how do I use it?"
```
Your AI gets the full documentation, examples, and usage patterns.

### Python Packages
```
Ask: "Show me the implementation of requests.get"
```
Dociium finds the function in your local environment and provides the source.

### Crate Discovery
```
Ask: "Find me async HTTP client libraries for Rust"
```
Get a curated list with descriptions and popularity metrics.

## Performance

- **First request**: ~1 second (fetches and caches)
- **Cached requests**: <1ms 
- **Memory usage**: ~30MB + 5MB per cached crate
- **No builds required**: Uses pre-built documentation

## Configuration

Set custom cache directory:
```bash
export RDOCS_CACHE_DIR=/path/to/cache
```

Default cache: `~/.cache/rdocs-mcp` (Linux/Mac) or `%APPDATA%\rdocs-mcp` (Windows)

## Troubleshooting

**Documentation not found?**
- Ensure the package exists on docs.rs (Rust) or in your local environment (Python/Node)
- Check spelling and use exact paths like `tokio::sync::Mutex`

**Cache issues?**
```bash
rm -rf ~/.cache/rdocs-mcp  # Clear cache and retry
```

**Need help?**
- Check the [detailed documentation](docs/)
- Open an [issue](https://github.com/labiium/dociium/issues)

## Development

```bash
# Build from source
cargo build --release

# Run tests
cargo test

# Development mode with debug logging
RUST_LOG=debug cargo run
```

## License

MIT OR Apache-2.0

---

**Get documentation without leaving your AI conversation.**