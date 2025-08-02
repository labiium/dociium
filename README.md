# DOCIIUM

A high-performance **Model Context Protocol (MCP)** server that provides comprehensive access to documentation and source code for multiple programming languages, including **Rust**, **Python**, and **Node.js**. Built in Rust for maximum performance and reliability.

[![CI](https://github.com/labiium/dociium/workflows/CI/badge.svg)](https://github.com/labiium/dociium/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/dociium.svg)](https://crates.io/crates/dociium)

## ✨ Features

- **🦀 Rust Documentation**: Fast access to docs.rs with intelligent URL discovery
- **🐍 Python Integration**: Local package analysis via tree-sitter parsing
- **📦 Node.js Support**: JavaScript/TypeScript package exploration
- **⚡ High Performance**: Sub-millisecond cached responses, ~1s cold starts
- **🧠 Smart Caching**: Multi-level caching with compression and TTL
- **🔍 Symbol Search**: Full-text search across crate symbols and documentation
- **🎯 MCP Protocol**: Native Model Context Protocol server implementation

## 🚀 Quick Start

### Installation

Install DOCIIUM using any of these methods:

```bash
# From crates.io (recommended)
cargo install dociium

# From source
git clone https://github.com/labiium/dociium.git
cd dociium
cargo install --path mcp_server

# From git directly
cargo install --git https://github.com/labiium/dociium.git dociium
```

### Basic Usage

Configure your MCP client (Claude Desktop, etc.):

```json
{
  "mcpServers": {
    "dociium": {
      "command": "dociium",
      "args": []
    }
  }
}
```

### Example Queries

**Search Rust crates:**
```json
{
  "tool": "search_crates",
  "arguments": {
    "query": "async http client",
    "limit": 5
  }
}
```

**Get documentation:**
```json
{
  "tool": "get_item_doc",
  "arguments": {
    "crate_name": "tokio",
    "path": "tokio::sync::Mutex"
  }
}
```

**Analyze Python packages:**
```json
{
  "tool": "get_implementation",
  "arguments": {
    "language": "python",
    "package_name": "requests",
    "item_path": "api.py#get"
  }
}
```

## 🛠️ Available Tools

| Tool | Description | Use Case |
|------|-------------|----------|
| `search_crates` | Search crates.io registry | Find relevant Rust packages |
| `crate_info` | Get detailed crate metadata | Understand package details |
| `get_item_doc` | Retrieve item documentation | Get API documentation |
| `search_symbols` | Search within crate symbols | Find specific functions/types |
| `list_trait_impls` | List trait implementations | Understand trait relationships |
| `list_impls_for_type` | List traits for a type | See available methods |
| `source_snippet` | Get source code with context | View implementation details |
| `get_implementation` | Local Python/Node.js analysis | Analyze local dependencies |
| `get_cache_stats` | Cache performance metrics | Monitor system performance |
| `clear_cache` | Cache management | Clear stale entries |

## 🏗️ Architecture

```
┌─────────────────────────────┐
│         MCP Server          │ ← rmcp framework
└┬────────────────────────────┘
 │
 ├─ Tools (MCP Handlers)
 │  ├─ search_crates
 │  ├─ get_implementation  
 │  ├─ crate_info
 │  └─ ...
 │
 └─ DocEngine (doc_engine crate)
    ├─ Package Finder: Local environment discovery
    ├─ Docs.rs Scraper: HTML parsing with URL discovery
    ├─ Language Processors
    │  ├─ Rust (docs.rs integration)
    │  ├─ Python (tree-sitter parsing)
    │  └─ Node.js (tree-sitter parsing)
    ├─ Smart Cache: Multi-level caching
    └─ IndexCore (index_core crate)
       ├─ SymbolIndex: Full-text search
       └─ TraitImplIndex: Relationship mapping
```

## ⚙️ Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RDOCS_CACHE_DIR` | `~/.cache/dociium` | Cache directory location |
| `RUST_LOG` | `info` | Logging level |

### Cache Settings

- **Memory Cache**: LRU cache with 1000 entries
- **Disk Cache**: Compressed storage with zstd
- **TTL**: 7 days default expiration
- **Size**: ~5MB per cached crate

## 📊 Performance

- **Cold Start**: ~1s for popular crates (docs.rs fetching)
- **Warm Cache**: <50ms for cached queries  
- **Memory Usage**: ~30MB base + ~5MB per cached crate
- **Throughput**: 100+ requests/second sustained

## 🧪 Development

### Prerequisites

- Rust 1.70+
- Internet connection for docs.rs access

### Building

```bash
git clone https://github.com/labiium/dociium.git
cd dociium
cargo build --release --bin dociium
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run with integration tests
cargo test --workspace --features integration-tests

# Run with network tests
ENABLE_NETWORK_TESTS=1 cargo test --workspace
```

### Project Structure

```
dociium/
├── mcp_server/           # Main MCP server binary
├── doc_engine/           # Documentation processing engine  
├── index_core/           # Search and indexing functionality
├── .github/              # CI/CD workflows
└── README.md             # This file
```

## 🔒 Security

- **No Code Execution**: Uses pre-built documentation only
- **Input Validation**: Comprehensive parameter sanitization
- **Rate Limiting**: Built-in request throttling
- **Network Security**: Restricted to trusted domains
- **Cache Isolation**: Per-crate cache separation

## 🐛 Troubleshooting

### Common Issues

**"Documentation not found"**
- Verify crate exists on docs.rs
- Check item path format (e.g., `tokio::sync::Mutex`)
- Try different version or use "latest"

**"Network timeout"**
- Check internet connectivity
- docs.rs may be temporarily unavailable
- Retry after a few moments

**"Cache permission errors"**
- Ensure write permissions to cache directory
- Set `RDOCS_CACHE_DIR` to writable location

### Performance Tuning

- Clear cache periodically: Use `clear_cache` tool
- Monitor with `get_cache_stats` tool
- Adjust `RDOCS_CACHE_DIR` for faster storage

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Setup

```bash
# Install development tools
cargo install cargo-audit cargo-deny

# Run development server
RUST_LOG=debug cargo run --bin dociium

# Run lints
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

## 📜 License

This project is dual-licensed under the **MIT OR Apache-2.0** license.

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

## 🙏 Acknowledgments

- **[rmcp](https://crates.io/crates/rmcp)**: Rust MCP framework
- **[docs.rs](https://docs.rs)**: Official Rust documentation hosting
- **[tree-sitter](https://tree-sitter.github.io/)**: Incremental parsing library
- **Rust Community**: For excellent tooling and ecosystem

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/labiium/dociium/issues)
- **Discussions**: [GitHub Discussions](https://github.com/labiium/dociium/discussions)
- **Documentation**: [docs.rs/dociium](https://docs.rs/dociium)

---

**Built with ❤️ for developers who need fast, reliable documentation access**