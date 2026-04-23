# Dociium

Multi-language documentation and code discovery via the **Model Context Protocol (MCP)**.

Dociium enables AI assistants (Claude Desktop, Cline, etc.) to search and retrieve documentation and source code for Rust, Python, and Node.js packages—with semantic search, import resolution, and intelligent caching.

---

## ✨ Features

- **Rust**: Crate search (crates.io), documentation (docs.rs), trait implementations, symbol search
- **Python**: Semantic search, source code extraction, class method introspection, universal package manager support (pip, uv, poetry, pdm, conda)
- **Node.js**: Source code extraction with ESM/CJS support
- **Import Resolution**: Map `use`/`import`/`from` statements to source locations (Rust/Python/Node)
- **CLI + MCP Server**: Use as MCP server (stdio/HTTP) or invoke tools directly from command line
- **Smart Caching**: Multi-layer (in-memory LRU + disk) with metrics and TTL
- **Context-Aware**: Working directory support for monorepos and multi-virtualenv projects

---

## 🚀 Quick Start

### Install

```bash
# From crates.io
cargo install dociium

# From source
git clone https://github.com/labiium/dociium.git
cd dociium
cargo install --path .
```

### As MCP Server

Add to your MCP settings (e.g., Claude Desktop `claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "dociium": {
      "command": "dociium",
      "args": ["stdio"]
    }
  }
}
```

Then ask your assistant:
- "Search for async http clients on crates.io"
- "Show documentation for tokio::sync::Mutex"
- "Find Python functions for parsing JSON in the requests library"
- "What are all the methods on the Flask class?"

### As CLI Tool

```bash
# Rust crate search
dociium search-crates "async http"

# Python semantic search
dociium semantic-search requests "make http post request with json"

# Get Python class methods
dociium list-class-methods flask "app.py#Flask"

# Get implementation
dociium get-implementation --language python requests "sessions.py#Session"

# Cache statistics
dociium cache-stats
```

---

## 🧰 Available Tools

### Rust Documentation

| Tool | Description | Example |
|------|-------------|---------|
| `search_crates` | Search crates.io | `dociium search-crates "async runtime"` |
| `crate_info` | Get crate metadata | `dociium crate-info tokio` |
| `get_item_doc` | Fetch item docs from docs.rs | `dociium get-item-doc tokio "sync::Mutex"` |
| `list_trait_impls` | List implementations of a trait | `dociium list-trait-impls serde "Serialize"` |
| `list_impls_for_type` | List traits for a type | `dociium list-impls-for-type std "Vec"` |
| `search_symbols` | Search symbols in a crate | `dociium search-symbols tokio "spawn"` |
| `source_snippet` | Get source code (placeholder) | `dociium source-snippet tokio "sync::Mutex"` |

### Python & Node.js

| Tool | Description | Example |
|------|-------------|---------|
| `semantic_search` | Natural language search | `dociium semantic-search requests "retry failed requests"` |
| `get_implementation` | Get source code | `dociium get-implementation -l python requests "api.py#get"` |
| `list_class_methods` | List all methods of a class | `dociium list-class-methods flask "app.py#Flask"` |
| `get_class_method` | Get specific method | `dociium get-class-method flask "app.py#Flask" route` |
| `search_package_code` | Regex code search | `dociium search-package-code -l python flask "async def"` |

### Multi-Language

| Tool | Description | Example |
|------|-------------|---------|
| `resolve_imports` | Resolve import statements | Via MCP JSON-RPC |
| `cache_stats` | Get cache metrics | `dociium cache-stats` |
| `clear_cache` | Clear cache | `dociium clear-cache` |
| `cleanup_cache` | Remove expired entries | `dociium cleanup-cache` |

---

## 🔎 Python Semantic Search

The standout feature for Python developers. Search packages using natural language:

```bash
dociium semantic-search requests "create session with retry logic"
```

**How it works:**
- TF-IDF scoring over function names, docstrings, signatures, and module paths
- Indexes public symbols (functions and classes) from installed packages
- First search builds index (0.5-3s), subsequent searches <10ms
- Results include relevance scores, signatures, docstring previews, and source locations

**MCP JSON-RPC:**
```json
{
  "method": "tools/call",
  "params": {
    "name": "semantic_search",
    "arguments": {
      "language": "python",
      "package_name": "requests",
      "query": "send authenticated http request",
      "limit": 5
    }
  }
}
```

**Score interpretation:**
- 0.9+: Excellent match
- 0.7-0.9: Good match
- 0.5-0.7: Moderate relevance
- <0.5: Weak match

---

## 🐍 Python Package Discovery

Works with **any** Python package manager—no pip required!

**Multi-level fallback strategy:**

1. **Environment variables** (highest priority)
   - `DOC_PYTHON_PACKAGE_PATH_<PKG>` or `DOC_PYTHON_PACKAGE_PATH`

2. **Native introspection** (pure Rust, no Python runtime needed)
   - Scans virtual environments: `.venv`, `venv`, `$VIRTUAL_ENV`
   - Checks user site-packages: `~/.local/lib/python*/site-packages`
  - Scans system locations: `/usr/local/lib`, `/usr/lib`, `/opt/venv/lib`, `/opt/homebrew/lib`

3. **pip show** (if pip available)

4. **uv pip show** (if uv available)

5. **Direct filesystem scan** (last resort)

**Supported package managers:**
- pip
- uv
- poetry
- pdm
- conda
- pipenv
- Any tool that installs to standard site-packages

**Context-aware resolution:**
Use `context_path` parameter to target specific project virtualenvs:

```bash
dociium semantic-search --context /path/to/project mypackage "find something"
```

---

## 🧩 Import Resolution

Resolve import/use statements to their source locations:

**Rust:**
```rust
use tokio::sync::Mutex;
use std::collections::HashMap;
```

**Python:**
```python
from requests import Session
from requests.adapters import HTTPAdapter
```

**Node.js:**
```javascript
import { Router } from 'express';
import * as utils from './utils.js';
```

Returns file paths and line numbers for each imported symbol.

**Limitations:**
- Best-effort heuristics (not a full compiler)
- Macro-expanded items (Rust) not resolved
- Dynamic imports (Python `__getattr__`) not detected
- Complex re-export chains may be incomplete

---

## 🗄️ Caching

**Multi-layer architecture:**

| Layer | Storage | Purpose |
|-------|---------|---------|
| Memory LRU | In-process | Hot crate docs, versions |
| Disk (items) | Gzipped files | Individual Rust item docs |
| Disk (indexes) | JSON | Parsed search-index.js |
| Import cache | In-process LRU+TTL | Import resolution (5min TTL) |
| Semantic index | In-process | Python package TF-IDF vectors |

**Cache metrics:**
```bash
dociium cache-stats
```

Returns hit rates, miss rates, evictions, total entries, and oldest entry age.

**Cache management:**
```bash
# Clear all caches
dociium clear-cache

# Clear specific crate
dociium clear-cache --crate-name tokio

# Remove expired entries
dociium cleanup-cache
```

---

## ⚙️ Configuration

### Cache Directory

Priority:
1. CLI flag: `--cache-dir <path>`
2. Env: `RDOCS_CACHE_DIR`
3. Platform default: `$XDG_CACHE_HOME/dociium` (Linux), `~/Library/Caches/dociium` (macOS)
4. Fallback: `./.dociium-cache`

### Working Directory

Set programmatically when embedding:

```rust
use dociium::doc_engine::{DocEngine, DocEngineOptions};
use std::path::PathBuf;

let options = DocEngineOptions {
    working_dir: Some(PathBuf::from("/path/to/project")),
};
let engine = DocEngine::new_with_options("./cache", options).await?;
```

Or use `context_path` in tool calls (resolved relative to working directory).

### Environment Overrides

Force package locations:

```bash
export DOC_PYTHON_PACKAGE_PATH_requests=/custom/path/to/requests
export DOC_NODE_PACKAGE_PATH_express=/custom/path/to/express
```

### HTTP Server Mode

Run as HTTP server instead of stdio:

```bash
dociium http --http-listen 127.0.0.1:7777
```

Options:
- `--http-listen <addr:port>`: Bind address (required)
- `--http-path <path>`: Endpoint prefix (default `/mcp`)
- `--http-keep-alive-secs <n>`: SSE ping interval (default 30, 0=disabled)
- `--http-stateless`: Disable per-session state

---

## 🔐 Security

- **Path sanitization**: All file paths validated and normalized
- **Input validation**: Length and charset checks on crate names, versions, queries
- **Timeout protection**: All network calls have configurable timeouts
- **No shell execution**: All external commands use structured APIs (no `eval`)
- **Safe parsing**: Fallback-based parsing prevents crashes on malformed data

---

## 🧪 Testing

```bash
# All tests
cargo test

# With network tests (requires internet)
ENABLE_NETWORK_TESTS=1 cargo test

# Linting
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check
```

Test coverage:
- Integration tests for all MCP tools
- Unit tests for cache, search, import resolution
- Network tests (gated by env var)
- Cache metrics validation

---

## 📈 Roadmap

**Near-term:**
- Real Rust source snippet extraction (currently placeholder)
- Improved multi-hop import resolution
- Richer cache eviction policies
- Performance metrics export (Prometheus/OpenTelemetry)

**Medium-term:**
- Python `__all__` and re-export handling
- Node.js barrel file resolution
- Pluggable search backends (Tantivy integration)
- Persistent import cache

**Long-term:**
- Full rustdoc JSON ingestion
- Language server protocol (LSP) integration
- Multi-language semantic search
- Distributed cache sharing

---

## 🛠️ Development

```bash
# Clone and build
git clone https://github.com/labiium/dociium.git
cd dociium
cargo build --release

# Run as stdio MCP server
./target/release/dociium stdio

# Run as HTTP server
./target/release/dociium http --http-listen 127.0.0.1:7777

# Direct CLI usage
./target/release/dociium search-crates tokio
```

---

## 📜 License

Dual-licensed under **MIT OR Apache-2.0**.

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

---

## 🙌 Contributing

Contributions welcome! Please:

1. Open an issue describing the enhancement or fix
2. Include tests (integration and/or unit)
3. Maintain backward compatibility for MCP tool schemas
4. Run `cargo clippy` and `cargo fmt` before submitting

---

**Dociium** - Keep your AI assistant grounded in real code and documentation.

Built by [Labiium](https://github.com/labiium) with ❤️
