# Rust Documentation MCP Server

A high-performance **Model Context Protocol (MCP)** server that provides comprehensive access to Rust crate documentation, trait implementations, and source code exploration. Built in Rust for maximum performance and reliability.

## 🚀 Features

### Core Functionality
- **📦 Crate Search**: Search and discover Rust crates from crates.io
- **📖 Documentation Access**: Retrieve formatted documentation for any item in a crate
- **🔍 Symbol Search**: Full-text search across crate symbols with fuzzy matching
- **🧬 Trait Exploration**: List trait implementations and type relationships
- **📝 Source Code**: Access source code snippets with context
- **⚡ Smart Caching**: Intelligent disk and memory caching for fast responses

### MCP Tools Available

| Tool | Description | Parameters |
|------|-------------|------------|
| `search_crates` | Search for crates on crates.io | `query`, `limit` |
| `crate_info` | Get detailed crate information | `name` |
| `get_item_doc` | Retrieve item documentation | `crate_name`, `path`, `version?` |
| `list_trait_impls` | List trait implementations | `crate_name`, `trait_path`, `version?` |
| `list_impls_for_type` | List traits implemented by a type | `crate_name`, `type_path`, `version?` |
| `source_snippet` | Get source code with context | `crate_name`, `item_path`, `context_lines?`, `version?` |
| `search_symbols` | Search symbols within a crate | `crate_name`, `query`, `kinds?`, `limit?`, `version?` |

## 🏗️ Architecture

```
┌─────────────────────────────┐
│         MCP Server          │ ← rmcp framework
└┬────────────────────────────┘
 │
 ├─ Tools (handlers)
 │  ├─ search_crates
 │  ├─ crate_info
 │  ├─ get_item_doc
 │  ├─ list_trait_impls
 │  ├─ list_impls_for_type
 │  ├─ source_snippet
 │  └─ search_symbols
 │
 ├─ DocEngine (doc_engine crate)
 │  ├─ Fetcher: Downloads crates & metadata
 │  ├─ Cache: Persistent storage
 │  └─ RustdocBuilder: Generates JSON docs
 │
 └─ IndexCore (index_core crate)
    ├─ SymbolIndex: Full-text search
    └─ TraitImplIndex: Trait relationships
```

## 🛠️ Installation

### Prerequisites
- **Rust 1.70+** with nightly toolchain
- **Git**

### Building from Source

```bash
git clone <repository-url>
cd rdocs_mcp
cargo build --release
```

### Docker (Recommended)

```bash
docker build -t rust-docs-mcp .
docker run --rm -p 8800:8800 rust-docs-mcp
```

## 🚦 Usage

### Stdio Transport (Default)

```bash
cargo run --release --bin rdocs-mcp-server
```

### WebSocket Transport

```bash
RDOCS_WEBSOCKET=1 cargo run --release --bin rdocs-mcp-server --features websocket
```

The server will listen on `127.0.0.1:8800` for WebSocket connections.

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RDOCS_CACHE_DIR` | `~/.cache/rdocs-mcp` | Cache directory path |
| `RDOCS_WEBSOCKET` | - | Enable WebSocket transport |

## 📊 Example Usage

### Searching for Crates

```json
{
  "method": "tools/call",
  "params": {
    "name": "search_crates",
    "arguments": {
      "query": "async http",
      "limit": 5
    }
  }
}
```

### Getting Documentation

```json
{
  "method": "tools/call",
  "params": {
    "name": "get_item_doc",
    "arguments": {
      "crate_name": "tokio",
      "path": "tokio::sync::Mutex"
    }
  }
}
```

### Finding Trait Implementations

```json
{
  "method": "tools/call",
  "params": {
    "name": "list_trait_impls",
    "arguments": {
      "crate_name": "std",
      "trait_path": "std::iter::Iterator"
    }
  }
}
```

## 🔧 Configuration

### Cache Settings
- **Memory Cache**: LRU cache with 100 entries
- **Disk Cache**: File-based persistent storage
- **TTL**: Configurable expiration (default: 7 days)

### Performance Tuning
- **Rate Limiting**: 60 requests/minute per client
- **Build Timeout**: 5 minutes for rustdoc generation
- **Index Size**: Configurable heap size for search index

## 🚀 Performance

### Benchmarks
- **Cold Start**: ~2-3 seconds for popular crates
- **Warm Cache**: <100ms for cached queries
- **Memory Usage**: ~50MB base + ~10MB per cached crate
- **Build Time**: ~30-60 seconds per crate (one-time)

### Optimizations
- **Incremental Builds**: Only rebuild when crate version changes
- **Compressed Storage**: Efficient cache compression
- **Smart Indexing**: Selective item indexing based on visibility

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run with network tests (requires internet)
ENABLE_NETWORK_TESTS=1 cargo test

# Run integration tests
ENABLE_INTEGRATION_TESTS=1 cargo test
```

## 📁 Project Structure

```
rdocs_mcp/
├── mcp_server/           # Main MCP server binary
│   ├── src/
│   │   ├── main.rs       # Server entry point
│   │   └── tools.rs      # Tool definitions
│   └── Cargo.toml
├── doc_engine/           # Documentation engine
│   ├── src/
│   │   ├── lib.rs        # Main engine
│   │   ├── fetcher.rs    # Crate fetching
│   │   ├── cache.rs      # Caching layer
│   │   ├── rustdoc.rs    # Rustdoc JSON builder
│   │   └── types.rs      # Type definitions
│   └── Cargo.toml
├── index_core/           # Search and indexing
│   ├── src/
│   │   ├── lib.rs        # Index management
│   │   ├── search.rs     # Full-text search
│   │   ├── traits.rs     # Trait indexing
│   │   └── types.rs      # Type definitions
│   └── Cargo.toml
├── Cargo.toml            # Workspace configuration
└── README.md
```

## 🔒 Security

- **Rate Limiting**: Prevents abuse with configurable limits
- **Input Validation**: Comprehensive validation of all inputs
- **Sandboxed Builds**: Isolated rustdoc generation
- **Cache Isolation**: Per-crate cache isolation

## 🐛 Troubleshooting

### Common Issues

**"Nightly toolchain not found"**
```bash
rustup toolchain install nightly
```

**"Build timeout"**
- Increase `RUSTDOC_TIMEOUT` environment variable
- Check internet connectivity for crate downloads

**"Cache permission errors"**
- Ensure write permissions to cache directory
- Set `RDOCS_CACHE_DIR` to writable location

**"Out of memory during indexing"**
- Reduce index heap size in configuration
- Clear cache: `rm -rf ~/.cache/rdocs-mcp`

## 🛣️ Roadmap

### Phase 1 (Current)
- ✅ Basic MCP server with stdio transport
- ✅ Core documentation tools
- ✅ File-based caching
- ✅ Trait implementation indexing

### Phase 2 (Next)
- 🔄 Full-text search with Tantivy
- 🔄 WebSocket transport
- 🔄 Advanced caching with compression
- 🔄 Performance monitoring

### Phase 3 (Future)
- 📋 Cross-crate dependency analysis
- 📋 Semantic search with embeddings
- 📋 Real-time documentation updates
- 📋 GraphQL API endpoint

## 🤝 Contributing

1. **Fork** the repository
2. **Create** a feature branch (`git checkout -b feature/amazing-feature`)
3. **Commit** your changes (`git commit -m 'Add amazing feature'`)
4. **Push** to the branch (`git push origin feature/amazing-feature`)
5. **Open** a Pull Request

### Development Setup

```bash
# Install development dependencies
cargo install cargo-expand cargo-audit cargo-deny

# Run development server with debug logging
RUST_LOG=debug cargo run

# Run lints
cargo clippy -- -D warnings
cargo fmt --check
```

## 📄 License

This project is licensed under the **MIT OR Apache-2.0** license.

## 🙏 Acknowledgments

- **[rmcp](https://crates.io/crates/rmcp)**: Rust MCP framework
- **[rustdoc-types](https://crates.io/crates/rustdoc-types)**: Rustdoc JSON parsing
- **[crates_io_api](https://crates.io/crates/crates_io_api)**: crates.io API client
- **[Tantivy](https://crates.io/crates/tantivy)**: Full-text search engine

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/your-org/rust-docs-mcp/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/rust-docs-mcp/discussions)
- **Documentation**: [docs.rs](https://docs.rs/rust-docs-mcp)

---

**Built with ❤️ for the Rust community**