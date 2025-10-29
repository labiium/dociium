# Dociium

Fast multi-language documentation + source retrieval via the **Model Context Protocol (MCP)**.

Dociium lets AI assistants (Claude Desktop, Continue, etc.) fetch Rust crate docs (from docs.rs), Python / Node.js local package source, perform symbol + trait exploration, and resolve imports — all with robust caching and a canonical schema.

---

## ✨ Highlights

- **Rust**: Crate search, item docs, trait impl listings, symbol search, source snippet placeholders (real source integration roadmap).
- **Python / Node.js**: Local environment code + doc extraction (best‑effort heuristic parsing); Python supports `pip`, `uv`, `venv`, and `conda`.
- **Python Semantic Search**: Natural-language discovery across local package symbols (docstrings + signatures).
- **Import Resolution**: Best‑effort mapping of `use` / `from` / `import` statements to file + symbol locations (Rust / Python / Node).
- **Working Directory Support**: Configurable base context for package resolution; ideal for monorepos and multi-virtualenv setups.
- **Context-Aware Caching**: Import resolution cache includes context path to prevent cross-contamination between projects.
- **Streamable HTTP Transport**: Optional SSE/Web-compatible transport in addition to stdio.
- **Shared Canonical Types**: Stable JSON schema via `shared_types` (unifying multiple legacy internal representations).
- **Deterministic Symbol Index**: Rebuildable from cached search index; future pluggable search backends.
- **Resilient docs.rs parsing**: Hardened search-index.js extraction (brace balancing + pattern matching).
- **Layered Caching**: In‑memory LRU, disk (items / crates / indexes), import-resolution in‑process LRU+TTL.
- **Metrics**: Cache hit/miss rates, evictions, oldest-entry age, size accounting.
- **Safe Boundaries**: Input validation (crate names, item paths, context lines, search limits).

---

## 🚀 Quick Start

### Install

```bash
# From crates.io
cargo install dociium

# Or from source (latest main)
git clone https://github.com/labiium/dociium.git
cd dociium
cargo install --path .
```

### Minimal MCP Client Config

```jsonc
{
  "servers": {
    "dociium": {
      "command": "dociium"
    }
  }
}
```

### Ask Your Assistant

Examples you can literally type:

- “Search crates for async http client”
- “Show docs for tokio::sync::Mutex”
- “List impls of serde::Serialize”
- “What traits does std::vec::Vec implement?”
- "Resolve these imports:\nuse std::fmt::Display;\nuse serde::de::Deserializer;"
- "Find symbols in chrono related to time zone"
- "Show implementation of requests.get"
- "Get Node implementation of express Router"

**Note for uv users:** Works seamlessly with uv-managed Python environments—just ensure `uv` is in your PATH.

---

## 🧰 MCP Tools

| Tool | Description | Key Params | Notes |
|------|-------------|------------|-------|
| `search_crates` | Search crates.io | `query`, `limit` | Network call with timeout |
| `crate_info` | Crate metadata & versions | `name` | Includes downloads, deps |
| `get_item_doc` | Rust item documentation | `crate_name`, `path`, `version?` | On-demand docs.rs scrape |
| `list_trait_impls` | List impls of a trait | `crate_name`, `trait_path` | Uses processed search index |
| `list_impls_for_type` | Trait impls for a type | `crate_name`, `type_path` | Symmetric to above |
| `source_snippet` | Code snippet (placeholder) | `crate_name`, `item_path`, `context_lines?` | `context_lines ≤ 100` enforced |
| `search_symbols` | In-crate symbol search | `crate_name`, `query`, `kinds?`, `limit?` | Returns canonical `shared_types::SymbolSearchResult` |
| `get_implementation` | Local code (py/node/rust) | `language`, `package_name`, `item_path`, `context_path?` | `item_path` uses `file#symbol`; `context_path` resolved relative to working directory |
| `resolve_imports` | Resolve import/use lines | `language`, `package`, `import_line?` / `code_block?`, `context_path?` | Multi-line extraction; `context_path` affects package resolution |
| `semantic_search` | Semantic package search (Python) | `language`, `package_name`, `query`, `limit?`, `context_path?` | Uses TF‑IDF + docstring analysis; `context_path` resolved relative to working directory |
| `get_cache_stats` | Cache metrics snapshot | – | Provides hit/miss/size metrics |
| `clear_cache` | Clear all or crate-specific | `crate_name?` | Resets stats if full clear |
| `cleanup_cache` | TTL-based purge | – | Applies configured TTL |

---

## 🗂 Shared Types

Responses progressively adopt canonical structures in `shared_types.rs` (e.g. `SymbolSearchResult`, `SourceLocation`, `SourceSnippet`).

Goals:
1. Eliminate drift between internal modules.
2. Provide stable MCP-exposed JSON schemas.
3. Allow future richer typing (enums for visibility/kind) with backward-compatible variants.

---

## 🧩 Import Resolution

Supported patterns (best effort):

### Rust
```rust
use crate::module::Type;
use std::fmt::{Display, Debug};
```
Heuristics:
- Locates module file (`mod.rs`, `file.rs`).
- Scans for direct symbol definitions.
- Traverses simple `pub use path::To::Item;` re-exports.

Limitations:
- Macro-expanded items, glob imports, deep multi-hop chains, conditional modules not fully resolved yet.

### Python
```python
import package.sub.mod
from package.sub.mod import A, B
```
Heuristics:
- Maps module path to `file.py` or `__init__.py`.
- Scans for `class` / `def` definitions (no dynamic attribute detection).
- Does not yet interpret complex `__all__` manipulations or runtime aliasing.

### Node (ESM)
```js
import { A, B } from "pkg/sub";
import * as NS from "pkg";
import DefaultExport from "pkg/file";
```
Heuristics:
- Tries extensions: `.ts`, `.js`, `.mjs`, `.cjs`.
- Index resolution for directories (`index.*`).
- Scans exports (`export function|class|const|let|var`, common patterns).

Cache:
- In-process LRU (capacity 512) with 5-minute TTL.
- Key: `language::package::context_path::import_line` (context-aware to prevent collisions across different project directories).

---

## 🗄 Caching Architecture

Layer | Purpose | Tech | Notes
------|---------|------|------
In-Memory LRU | Recently accessed crate docs & versions | `lru::LruCache` | Fast reuse
Item Cache | Individual item docs (Rust on-demand) | Disk + memory map | gz (optional)
Crate Index Cache | `search-index.js` parsed dataset | Disk + memory | Avoid repeated scraping
Generic Data | Arbitrary blobs (future extensibility) | Disk | Prefixed file naming
Import Resolution | Per-process mapping of import → result | Custom LRU+TTL | No disk persistence
Metrics | Stats (hits, misses, evictions) | Internal counters | Exposed via `get_cache_stats`

Key Metrics (from `get_cache_stats`):
- `hit_rate`, `miss_rate`
- `evictions`
- `total_entries` / disk vs memory size
- `oldest_entry_age_hours`

---

## ⚙️ Configuration

### Cache Directory

Priority order for cache directory:
1. CLI: `--cache-dir`
2. Env: `RDOCS_CACHE_DIR`
3. Platform default (`$XDG_CACHE_HOME/dociium` or OS equivalent)
4. Fallback: `./.dociium-cache`

### Working Directory

The documentation engine supports an optional **working directory** that determines the base context for resolving local packages. This is particularly useful for:

- **Monorepos** with multiple Python virtualenvs (including `uv` projects) or Node.js workspaces
- **Project-specific package installations** that differ from global environments
- **Context-aware caching** to prevent cross-contamination between different project contexts
- **uv-managed projects** where packages are isolated in `.venv` directories

**Setting the Working Directory:**

The working directory can be configured programmatically when initializing the `DocEngine`:

```rust
use dociium::doc_engine::{DocEngine, DocEngineOptions};
use std::path::PathBuf;

let options = DocEngineOptions {
    working_dir: Some(PathBuf::from("/path/to/project")),
};
let engine = DocEngine::new_with_options("./cache", options).await?;
```

**Behavior:**

- When `working_dir` is set, all `context_path` parameters in MCP tool calls are resolved relative to this directory (unless an absolute path is provided)
- Python package discovery tries `pip show` first, then falls back to `uv pip show` if pip is unavailable (respecting local virtualenvs and uv-managed environments)
- Node.js `npm root` commands use the working directory as context
- Import resolution cache keys include the resolved context path, ensuring separate cache entries per project
- Falls back to `std::env::current_dir()` when not explicitly set

**uv Support:**

For Python users leveraging [uv](https://github.com/astral-sh/uv), the engine automatically detects and uses `uv pip show` when `pip` is not available. This works seamlessly with:
- `uv sync` and `uv run` managed environments
- Projects using `uv venv` for virtual environment creation
- `uv pip install` managed dependencies

No additional configuration needed—just ensure `uv` is in your PATH.

### Environment Variable Overrides

Environment variables for local package discovery (these take precedence over working directory resolution):

| Language | Variable Patterns | Purpose |
|----------|-------------------|---------|
| Python | `DOC_PYTHON_PACKAGE_PATH` / `DOC_PYTHON_PACKAGE_PATH_<PKG>` | Force root directory for scanning (bypasses `pip show` and working directory) |
| Node   | `DOC_NODE_PACKAGE_PATH` / `DOC_NODE_PACKAGE_PATH_<PKG>`     | Override `node_modules` root (bypasses `npm root` and working directory) |
| Python Semantic Index | `DOC_PYTHON_PACKAGE_PATH_*` (as above) | Same overrides used when building semantic index |

**Note:** Environment variables take highest precedence, followed by working directory context, then system defaults.

### CLI Transport Options

`dociium` defaults to stdio. To expose the HTTP transport instead:

```bash
dociium --transport streamable-http --http-listen 127.0.0.1:7777
```

Flags:

| Flag | Description |
|------|-------------|
| `--http-listen <ADDR>` | Required when using `streamable-http`; bind address/port. |
| `--http-path <PATH>` | Endpoint prefix (default `/mcp`). |
| `--http-keep-alive-secs <N>` | Override SSE ping interval (use `0` to disable). |
| `--http-stateless` | Disable per-session state (stateless mode). |

The HTTP transport mounts the MCP endpoint at `http://<addr><path>` using Server-Sent Events for streaming responses. Clients must send JSON POSTs with `Accept: application/json, text/event-stream` and consume the SSE stream for incremental output.

## 🔎 Python Semantic Search

The `semantic_search` tool ranks local Python symbols using a TF‑IDF model over docstrings, signatures, and module context. Typical request:

```jsonc
{
  "type": "call_tool",
  "name": "semantic_search",
  "arguments": {
    "language": "python",
    "package_name": "requests",
    "query": "create an http session with retries",
    "limit": 5
  }
}
```

Key notes:

- `package_name` is resolved using `pip show` or `uv pip show` in the context of the configured working directory (if set) or current directory.
- `context_path` parameter (when provided) is resolved relative to the working directory and overrides the default context for package resolution.
- Results include docstring previews, inferred signatures, and file/line offsets for quick navigation.
- Indexes are cached in-process and on disk for repeat queries (cleared via `clear_cache`).
- For monorepos, multi-virtualenv setups, or uv-managed projects, configure the working directory at engine initialization or use `context_path` to target specific project directories.

---

## 🐍 uv Integration

Dociium provides first-class support for Python users leveraging [uv](https://github.com/astral-sh/uv), the fast Python package manager written in Rust.

### Automatic Detection

The engine automatically tries `uv pip show` as a fallback when `pip` is not available, enabling seamless operation with uv-managed environments:

```bash
# Works out of the box with uv projects
cd my-uv-project
uv sync
# dociium will automatically use 'uv pip show' to locate packages
```

### Supported Workflows

- **`uv sync`**: Packages installed via `uv sync` are automatically discovered
- **`uv venv`**: Virtual environments created with `uv venv` work transparently
- **`uv pip install`**: Direct package installations are detected
- **`uv run`**: Scripts run via `uv run` can access dociium features

### Resolution Order

Python package discovery follows this precedence:

1. **Environment variables**: `DOC_PYTHON_PACKAGE_PATH` or `DOC_PYTHON_PACKAGE_PATH_<PKG>`
2. **pip**: `pip show <package>` (if pip is in PATH)
3. **uv**: `uv pip show <package>` (fallback if pip unavailable)

### Working Directory with uv

For uv projects with multiple virtual environments or monorepo setups, configure the working directory:

```rust
use dociium::doc_engine::{DocEngine, DocEngineOptions};
use std::path::PathBuf;

// Point to your uv project root
let options = DocEngineOptions {
    working_dir: Some(PathBuf::from("/path/to/uv-project")),
};
let engine = DocEngine::new_with_options("./cache", options).await?;
```

Alternatively, use `context_path` in MCP tool calls to target specific uv projects dynamically.

### Requirements

- `uv` must be installed and available in your PATH
- No additional configuration needed—detection is automatic

---

## 🔍 Symbol Search

Currently a deterministic in-memory index built from docs.rs search index:
- Linear scoring pass; suitable for small/medium crates.
- Roadmap: optional Tantivy or other inverted index for scaling large ecosystems.

---

## 🕸 docs.rs Scraping

Features:
- Hardened parsing of `search-index.js` (multiple historical variations).
- Balanced brace extraction prevents malformed slices.
- Item doc fetching probes multiple type prefixes (`fn`, `struct`, `trait`, etc.).
- Controlled timeouts & limited retries.

Planned Enhancements:
- ETag / conditional requests
- Backoff & telemetry for format shifts
- Fallback scraping of alternative selectors when primary CSS fails

---

## ⛔ Current Limitations

Area | Limitation | Planned
-----|------------|--------
Rust Source Snippets | Placeholder content only | Integrate local crate source unpacking
Deep Re-exports | Multi-hop + wildcard chains incomplete | Recursive graph with cycle guard
Python Dynamics | Runtime-added attrs / metaclass effects ignored | AST + runtime overlay
Node Export Patterns | Re-exports from barrel files partially handled | Secondary pass over `export * from`
Search Scaling | O(N) scan | Optional Tantivy feature flag
Trait Impl Richness | Limited metadata (blanket detection heuristic) | Rustdoc JSON ingestion (future)
Cache Persistence | Import cache ephemeral | Optional persistent layer w/ pruning

---

## 🔐 Security & Safety

- Sanitized filenames (path & reserved char replacement).
- Version & crate name validation (length + charset).
- Timeout wrapping all external network calls.
- No shell eval for imports; only structured parsing.
- Avoids panics on malformed search index through layered fallbacks.

---

## 📈 Roadmap

Phase | Focus
------|------
Short-Term | Complete shared type migration, scraper config consolidation, multi-hop import traversal, richer cache metrics tooling
Medium-Term | Real Rust source snippet extraction, async fs refactors, improved Python `__all__` + Node export graph
Long-Term | Pluggable large-scale search backend, persistent import resolution store, full observability (metrics + tracing exporters)

---

## 🧪 Testing

Category | Notes
---------|------
Integration Tests | Exercise MCP tools end-to-end (network-gated where applicable)
Externalized Unit Tests | All major internal modules tested from `tests/` (cache, search, import resolution timing)
Cache Metrics | Hit/miss rate invariants validated
Feature Gating | Network tests disabled unless explicitly enabled (e.g. `ENABLE_NETWORK_TESTS=1` or cargo feature)

Run locally:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

---

## 🛠 Development

```bash
# Build
cargo build

# Run server (stdio MCP)
dociium

# With explicit cache dir
dociium --cache-dir /tmp/dociium-cache

# Clear cache via MCP tool (example JSON call)
# { "tool": "clear_cache", "params": {} }
```

---

## 📜 License

Dual-licensed under **MIT OR Apache-2.0**.

---

## 🙌 Contributing

PRs welcome:
1. Open an issue describing enhancement / fix.
2. Include tests (integration or unit).
3. Maintain shared_types compatibility (avoid breaking schema fields).

---

**Dociium**: Keep your AI context grounded in real code & docs — fast, structured, reproducible.

Happy exploring!
