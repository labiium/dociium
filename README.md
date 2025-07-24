# DOCIIUM: Multi-Language Documentation & Code MCP Server

A high-performance **Model Context Protocol (MCP)** server that provides comprehensive access to documentation and source code for multiple languages, including **Rust**, **Python**, and **Node.js (JavaScript/TypeScript)**. Built in Rust for maximum performance and reliability.

**🔄 NEW: Now uses docs.rs scraping for enhanced security and faster documentation access!**

## 🚀 Features

### Core Functionality
- **📦 Crate Search**: Search and discover Rust crates from crates.io
- **📖 Documentation Access**: Retrieve formatted documentation for any item in a crate
- **🔍 Symbol Search**: Full-text search across crate symbols with fuzzy matching *(feature gated until Tantivy support lands)*
- 🧬 **Trait Exploration**: List trait implementations and type relationships
- 📝 **Source Code**: Access source code snippets with context
- ⚡ **Smart Caching**: Intelligent disk and memory caching for fast responses
- 🌐 **Multi-Language Support**: Fetch implementation context from local Python (`venv`/`conda`) and Node.js (`node_modules`) environments.

### MCP Tools Available

| Tool | Description | Parameters |
|------|-------------|------------|
| `search_crates` | Search for crates on crates.io | `query`, `limit` |
| `crate_info` | Get detailed crate information | `name` |
| `get_item_doc` | Retrieve item documentation | `crate_name`, `path`, `version?` |
| `list_trait_impls` | List trait implementations | `crate_name`, `trait_path`, `version?` |
| `list_impls_for_type` | List traits implemented by a type | `crate_name`, `type_path`, `version?` |
| `source_snippet` | Get source code with context | `crate_name`, `item_path`, `context_lines?`, `version?` |
| `search_symbols` | (Rust) Search symbols within a crate | `crate_name`, `query`, `kinds?`, `limit?`, `version?` |
| `get_implementation` | (Python/Node) Get implementation from a local environment | `language`, `package_name`, `item_path`, `context_path?` |
| `get_cache_stats` | Get cache statistics and metrics | - |
| `clear_cache` | Clear cache entries (all or specific crate) | `crate_name?` |
| `cleanup_cache` | Remove expired cache entries | - |

### Usage Examples

#### Python Package Analysis
```json
{
  "method": "tools/call",
  "params": {
    "name": "get_implementation",
    "arguments": {
      "language": "python",
      "package_name": "requests",
      "item_path": "api.py#get"
    }
  }
}
```

#### Node.js Package Analysis
```json
{
  "method": "tools/call",
  "params": {
    "name": "get_implementation",
    "arguments": {
      "language": "node",
      "package_name": "express",
      "item_path": "lib/express.js#createApplication",
      "context_path": "/path/to/your/project"
    }
  }
}
```

#### Rust Crate Documentation
```json
{
  "method": "tools/call",
  "params": {
    "name": "get_item_doc",
    "arguments": {
      "crate_name": "tokio",
      "path": "sync::mpsc::channel"
    }
  }
}
```

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
 │  ├─ get_item_doc
 │  ├─ list_trait_impls
 │  ├─ list_impls_for_type
 │  ├─ source_snippet
 │  └─ search_symbols
 │
 └─ DocEngine (doc_engine crate)
    ├─ Package Finder: Locates packages in local environments (pip, npm)
    ├─ Docs.rs Scraper: Fetches pre-built documentation from docs.rs
    ├─ Language Processors
    │  ├─ Rust (scrapes docs.rs)
    │  ├─ Python (uses tree-sitter)
    │  └─ Node.js (uses tree-sitter)
    ├─ Smart Cache: Item-level caching with TTL
    └─ IndexCore (index_core crate)
       ├─ SymbolIndex: Full-text search from search-index.js
       └─ TraitImplIndex: Trait relationships
```

## 🛠️ Installation

### Prerequisites
- **Rust 1.70+** (nightly toolchain no longer required!)
- **Git**
- **Internet connection** for docs.rs access

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
cargo run --release --bin dociium
```

### WebSocket Transport

```bash
RDOCS_WEBSOCKET=1 cargo run --release --bin dociium --features websocket
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
- **Memory Cache**: LRU cache with 1000 entries
- **Item-level Cache**: Individual documentation items cached separately
- **Crate Index Cache**: Search indexes cached per crate
- **TTL**: Configurable expiration (default: 7 days)
- **Compression**: Optional zstd compression for disk storage

### Performance Tuning
- **Rate Limiting**: 60 requests/minute per client
- **Build Timeout**: 5 minutes for rustdoc generation
- **Index Size**: Configurable heap size for search index

## 🚀 Performance

### Benchmarks
- **Cold Start**: ~500ms for popular crates (docs.rs fetching)
- **Warm Cache**: <50ms for cached queries
- **Memory Usage**: ~30MB base + ~5MB per cached crate
- **No Build Time**: Documentation pre-built on docs.rs

### Optimizations
- **On-demand Fetching**: Only fetch documentation when requested
- **Compressed Storage**: Efficient cache compression with zstd
- **Smart Indexing**: Uses docs.rs search-index.js for symbol search
- **Item-level Caching**: Cache individual items to minimize network requests

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
│   │   ├── fetcher.rs    # Crate metadata fetching
│   │   ├── cache.rs      # Multi-level caching
│   │   ├── scraper.rs    # Docs.rs HTML scraper
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

- **No Code Execution**: Eliminates RCE vulnerabilities by using pre-built docs
- **Rate Limiting**: Prevents abuse with configurable limits
- **Input Validation**: Comprehensive validation of all inputs
- **Network Security**: Only fetches from trusted docs.rs domain
- **Cache Isolation**: Per-crate cache isolation

## 🐛 Troubleshooting

### Common Issues

**"Documentation not found"**
- Ensure the crate has documentation published on docs.rs
- Try a different version or use "latest"

**"Network timeout"**
- Check internet connectivity
- docs.rs may be temporarily unavailable

**"Cache permission errors"**
- Ensure write permissions to cache directory
- Set `RDOCS_CACHE_DIR` to writable location

**"HTML parsing errors"**
- Clear cache and retry: `rm -rf ~/.cache/rdocs-mcp`
- docs.rs HTML structure may have changed

### Cache Management

Cache management is integrated into the MCP server tools:

```json
// View cache statistics
{
  "method": "tools/call",
  "params": {
    "name": "get_cache_stats",
    "arguments": {}
  }
}

// Clear all cache
{
  "method": "tools/call",
  "params": {
    "name": "clear_cache",
    "arguments": {}
  }
}

// Clear cache for specific crate
{
  "method": "tools/call",
  "params": {
    "name": "clear_cache",
    "arguments": {
      "crate_name": "serde"
    }
  }
}

// Clean up expired entries
{
  "method": "tools/call",
  "params": {
    "name": "cleanup_cache",
    "arguments": {}
  }
}
```

## 🛣️ Roadmap

### Phase 1 (Current)
- ✅ Basic MCP server with stdio transport
- ✅ Core documentation tools
- ✅ Docs.rs scraping architecture
- ✅ Item-level caching with compression
- ✅ Integrated cache management via MCP tools

### Phase 2 (Next)
- 🔄 Enhanced trait implementation detection
- 🔄 Source code viewing from docs.rs
- 🔄 WebSocket transport
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
- **[scraper](https://crates.io/crates/scraper)**: HTML parsing and CSS selection
- **[crates_io_api](https://crates.io/crates/crates_io_api)**: crates.io API client
- **[docs.rs](https://docs.rs)**: Official Rust documentation hosting

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/your-org/rust-docs-mcp/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/rust-docs-mcp/discussions)
- **Documentation**: [docs.rs](https://docs.rs/rust-docs-mcp)

---

**Built with ❤️ for the Rust community**
