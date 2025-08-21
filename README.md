# Dociium

Fast multi-language documentation + source retrieval via the **Model Context Protocol (MCP)**.

Dociium lets AI assistants (Claude Desktop, Continue, etc.) fetch Rust crate docs (from docs.rs), Python / Node.js local package source, perform symbol + trait exploration, and resolve imports — all with robust caching and a canonical schema.

---

## ✨ Highlights

- **Rust**: Crate search, item docs, trait impl listings, symbol search, source snippet placeholders (real source integration roadmap).
- **Python / Node.js**: Local environment code + doc extraction (best‑effort heuristic parsing).
- **Import Resolution**: Best‑effort mapping of `use` / `from` / `import` statements to file + symbol locations (Rust / Python / Node).
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
- “Resolve these imports:\nuse std::fmt::Display;\nuse serde::de::Deserializer;”
- “Find symbols in chrono related to time zone”
- “Show implementation of requests.get”
- “Get Node implementation of express Router”

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
| `get_implementation` | Local code (py/node/rust) | `language`, `package_name`, `item_path`, `context_path?` | `item_path` uses `file#symbol` |
| `resolve_imports` | Resolve import/use lines | `language`, `package`, `import_line?` / `code_block?` | Multi-line extraction |
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
- Key: `language::package::import_line`.

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

Priority order for cache directory:
1. CLI: `--cache-dir`
2. Env: `RDOCS_CACHE_DIR`
3. Platform default (`$XDG_CACHE_HOME/dociium` or OS equivalent)
4. Fallback: `./.dociium-cache`

Environment variables for local discovery overrides:

| Language | Variable Patterns | Purpose |
|----------|-------------------|---------|
| Python | `DOC_PYTHON_PACKAGE_PATH` / `DOC_PYTHON_PACKAGE_PATH_<PKG>` | Force root directory for scanning |
| Node   | `DOC_NODE_PACKAGE_PATH` / `DOC_NODE_PACKAGE_PATH_<PKG>`     | Override `node_modules` root |

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