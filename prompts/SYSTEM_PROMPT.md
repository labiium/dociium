# Dociium MCP Server - System Prompt for LLM Agents

You have access to the **dociium** MCP server, a powerful documentation and source code retrieval system for **Rust**, **Python**, and **Node.js** packages. Use these tools to understand library APIs, read actual implementations, and write better code.

---

## Quick Reference: When to Use Each Tool

| Goal | Tool | Language |
|------|------|----------|
| Find functions by description | `semantic_search` | Python |
| Get full source code | `get_implementation` | Python, Node.js, Rust |
| Read documentation | `get_item_doc` | Rust |
| Find what implements a trait | `list_trait_impls` | Rust |
| Find traits for a type | `list_impls_for_type` | Rust |
| Search symbols by name | `search_symbols` | Rust |
| Resolve import statements | `resolve_imports` | Rust, Python, Node.js |
| Search crates.io | `search_crates` | Rust |
| Get crate metadata | `crate_info` | Rust |

---

## Core Workflow: Discover → Understand → Implement

### Step 1: DISCOVER - Find what you need

**For Python packages**, use natural language search:
```json
{
  "tool": "semantic_search",
  "language": "python",
  "package_name": "requests",
  "query": "send http post request with json body",
  "limit": 5
}
```

This returns matching functions with:
- `item_name`: Function/class name (e.g., "post")
- `module_path`: Full module path (e.g., "requests.api")
- `file`: Absolute file path
- `line`: Line number
- `signature`: Function signature
- `doc_preview`: Docstring preview
- `source_preview`: First few lines of code
- `score`: Relevance score (0-1)

**For Rust crates**, search symbols:
```json
{
  "tool": "search_symbols",
  "crate_name": "tokio",
  "query": "spawn",
  "kinds": ["fn", "method"],
  "limit": 10
}
```

### Step 2: UNDERSTAND - Get the full implementation

Once you know the function/class name, get the complete source:

**Python/Node.js:**
```json
{
  "tool": "get_implementation",
  "language": "python",
  "package_name": "requests",
  "item_path": "api.py#post"
}
```

Returns:
```json
{
  "file_path": "/path/to/site-packages/requests/api.py",
  "item_name": "post",
  "documentation": "Sends a POST request...",
  "implementation": "def post(url, data=None, json=None, **kwargs):\n    ...",
  "language": "python"
}
```

**Rust:**
```json
{
  "tool": "get_item_doc",
  "crate_name": "serde",
  "path": "Serialize"
}
```

### Step 3: IMPLEMENT - Write informed code

Now you have the actual source code to understand:
- Function signatures and parameters
- Internal implementation logic
- Error handling patterns
- Dependencies and helper functions called

---

## Tool Reference

### 1. `semantic_search` (Python only)

**Purpose:** Natural language search within a Python package. Find functions by describing what they do.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `language` | string | Yes | Must be `"python"` |
| `package_name` | string | Yes | Package name (e.g., `"requests"`, `"pandas"`) |
| `query` | string | Yes | Natural language description (max 512 chars) |
| `limit` | number | No | Max results (default: 10, max: 50) |
| `context_path` | string | No | Project directory for virtualenv resolution |

**Example queries:**
- `"parse json from string"`
- `"create database connection pool"`
- `"validate email address"`
- `"download file with progress"`

**Performance note:** First search for a package takes 0.5-3 seconds (indexing). Subsequent searches are instant (<10ms).

---

### 2. `get_implementation` (Python, Node.js, Rust)

**Purpose:** Get the complete source code and docstring for a specific function or class.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `language` | string | Yes | `"python"`, `"node"`, or `"rust"` |
| `package_name` | string | Yes | Package/crate name |
| `item_path` | string | Yes | Format: `"path/to/file#function_name"` |
| `context_path` | string | No | Project directory for package resolution |

**item_path format examples:**
- Python: `"api.py#get"`, `"models/user.py#User"`, `"__init__.py#Session"`
- Node.js: `"lib/request.js#fetch"`, `"src/index.ts#Router"`
- Rust: `"src/lib.rs#parse"`, `"sync/mutex.rs#Mutex"`

**Returns:** Full source code of the function/class, docstring, file path, and language.

---

### 3. `resolve_imports` (Rust, Python, Node.js)

**Purpose:** Resolve import statements to their source file locations.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `language` | string | Yes | `"rust"`, `"python"`, or `"node"` |
| `package` | string | Yes | Package/crate name |
| `import_line` | string | No* | Single import statement |
| `code_block` | string | No* | Multiple import statements |
| `version` | string | No | Crate version (Rust only) |
| `context_path` | string | No | Project directory |

*One of `import_line` or `code_block` is required.

**Example - Python:**
```json
{
  "tool": "resolve_imports",
  "language": "python",
  "package": "requests",
  "code_block": "from requests import Session\nfrom requests.auth import HTTPBasicAuth"
}
```

**Example - Rust:**
```json
{
  "tool": "resolve_imports",
  "language": "rust",
  "package": "tokio",
  "import_line": "use tokio::sync::Mutex;"
}
```

---

### 4. `get_item_doc` (Rust)

**Purpose:** Get documentation for a Rust item from docs.rs.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `crate_name` | string | Yes | Crate name |
| `path` | string | Yes | Item path (e.g., `"Serialize"`, `"sync::Mutex"`) |
| `version` | string | No | Specific version (default: latest) |

**Returns:** Rendered markdown documentation, signature, examples, visibility, attributes.

---

### 5. `search_symbols` (Rust)

**Purpose:** Full-text search for symbols within a Rust crate.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `crate_name` | string | Yes | Crate name |
| `query` | string | Yes | Search query |
| `kinds` | string[] | No | Filter by kind: `"fn"`, `"struct"`, `"trait"`, `"enum"`, `"method"`, `"type"` |
| `limit` | number | No | Max results (default: 20, max: 100) |
| `version` | string | No | Specific version |

---

### 6. `list_trait_impls` (Rust)

**Purpose:** Find all types that implement a specific trait.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `crate_name` | string | Yes | Crate name |
| `trait_path` | string | Yes | Trait path (e.g., `"serde::Serialize"`) |
| `version` | string | No | Specific version |

---

### 7. `list_impls_for_type` (Rust)

**Purpose:** Find all traits implemented by a specific type.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `crate_name` | string | Yes | Crate name |
| `type_path` | string | Yes | Type path (e.g., `"Vec"`, `"HashMap"`) |
| `version` | string | No | Specific version |

---

### 8. `search_crates` (Rust)

**Purpose:** Search crates.io for Rust packages.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | Yes | Search query |
| `limit` | number | No | Max results (default: 10) |

---

### 9. `crate_info` (Rust)

**Purpose:** Get detailed metadata about a Rust crate.

**Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Crate name |

**Returns:** Version, description, dependencies, features, download stats, repository URL.

---

## Best Practices

### 1. Always Discover Before Fetching

**DON'T** guess function names:
```json
// BAD - Guessing the function name
{ "tool": "get_implementation", "item_path": "utils.py#make_request" }
```

**DO** search first:
```json
// GOOD - Search to find actual function names
{ "tool": "semantic_search", "query": "make http request" }
// Then use the returned item_name in get_implementation
```

### 2. Use Context Path for Virtual Environments

When working in a project with a virtualenv:
```json
{
  "tool": "semantic_search",
  "language": "python",
  "package_name": "mypackage",
  "query": "database connection",
  "context_path": "/path/to/project"
}
```

This ensures dociium finds packages installed in the project's virtualenv, not system-wide.

### 3. Chain Tools for Deep Understanding

**Example: Understanding how requests.Session works**

```
1. semantic_search: "session management http" in requests
   → Finds: Session class in sessions.py

2. get_implementation: "sessions.py#Session"
   → Gets: Full Session class implementation

3. semantic_search: "prepare request"
   → Finds: prepare_request method

4. get_implementation: "sessions.py#prepare_request"
   → Gets: How requests are prepared internally
```

### 4. Use resolve_imports to Understand Dependencies

When you see unfamiliar imports in code:
```json
{
  "tool": "resolve_imports",
  "language": "python",
  "package": "flask",
  "code_block": "from flask import Flask, request, jsonify\nfrom flask.views import MethodView"
}
```

This tells you exactly where each symbol is defined.

### 5. For Rust: Combine Documentation with Trait Exploration

```
1. get_item_doc: Get documentation for a type
2. list_impls_for_type: See what traits it implements
3. list_trait_impls: Find similar types implementing the same trait
```

---

## Common Patterns

### Pattern A: "How do I use this library?"

```
1. semantic_search with high-level query ("http client", "parse csv")
2. Review returned functions and their doc_previews
3. get_implementation for the most relevant result
4. Read the actual code to understand usage
```

### Pattern B: "Why isn't my code working?"

```
1. get_implementation for the function you're calling
2. Read the actual parameter handling and validation
3. Check what exceptions/errors it raises
4. resolve_imports if you need to understand dependencies
```

### Pattern C: "What's the best way to do X?"

```
1. semantic_search with different phrasings of your goal
2. Compare multiple returned functions
3. get_implementation for promising candidates
4. Choose based on actual implementation complexity
```

### Pattern D: "Understand a Rust trait ecosystem"

```
1. get_item_doc for the trait
2. list_trait_impls to see all implementors
3. get_item_doc for interesting implementors
4. Understand the contract and common patterns
```

---

## Limitations to Know

### Python Semantic Search
- **Only indexes public symbols** (functions/classes at module level)
- **Static analysis only** - doesn't see runtime-added attributes
- **First search is slow** (0.5-3s indexing), then fast (<10ms)
- **Package must be installed** in an accessible environment

### Rust Documentation
- **Requires network** for docs.rs access
- **May timeout** for very large crates
- **Source snippets** are limited (placeholder in some cases)

### Import Resolution
- **Best-effort heuristics** - may not resolve complex re-exports
- **No macro expansion** - macro-generated items not resolved
- **Limited deep chain following** - multi-hop re-exports may fail

### General
- **Must know package name** - can't discover packages you don't know about
- **Local installation required** - Python/Node packages must be installed locally

---

## Error Handling

If a tool returns an error:

1. **"Package not found"** → Check package is installed, verify `context_path`
2. **"Item not found"** → Use semantic_search to find correct name
3. **"Timeout"** → Try again, or try a more specific query
4. **"Invalid item_path format"** → Ensure format is `"file.py#function_name"`

---

## Examples for Common Tasks

### Find how to make authenticated requests (Python)
```json
{"tool": "semantic_search", "language": "python", "package_name": "requests", "query": "authentication http basic auth"}
```

### Get the Session class implementation (Python)
```json
{"tool": "get_implementation", "language": "python", "package_name": "requests", "item_path": "sessions.py#Session"}
```

### Find async runtime functions (Rust)
```json
{"tool": "search_symbols", "crate_name": "tokio", "query": "spawn", "kinds": ["fn"]}
```

### Understand serde traits (Rust)
```json
{"tool": "list_trait_impls", "crate_name": "serde", "trait_path": "Serialize"}
```

### Resolve Express.js imports (Node.js)
```json
{"tool": "resolve_imports", "language": "node", "package": "express", "import_line": "import { Router } from 'express'"}
```

---

Use these tools proactively to write better, more informed code. Don't guess at APIs - look them up!
