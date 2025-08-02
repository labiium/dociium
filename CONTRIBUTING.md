# Contributing to DOCIIUM

Thank you for your interest in contributing to DOCIIUM! This guide will help you get started with contributing to our Multi-Language Documentation & Code MCP Server.

## 🚀 Quick Start

1. **Fork** the repository on GitHub
2. **Clone** your fork locally:
   ```bash
   git clone https://github.com/your-username/dociium.git
   cd dociium
   ```
3. **Create** a new branch for your feature:
   ```bash
   git checkout -b feature/your-feature-name
   ```
4. **Make** your changes
5. **Test** your changes
6. **Submit** a pull request

## 🛠️ Development Environment

### Prerequisites

- **Rust 1.70+** (latest stable recommended)
- **Git**
- **Internet connection** (for testing docs.rs integration)

### Setup

```bash
# Install Rust if you haven't already
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install from crates.io
cargo install dociium

# OR install from source
git clone https://github.com/labiium/dociium.git
cd dociium/mcp_server
cargo install --path .
```

### Development Tools

Install helpful development tools:

```bash
# Code formatting and linting
rustup component add rustfmt clippy

# Security auditing
cargo install cargo-audit

# Dependency checking
cargo install cargo-deny
```

## 🧪 Testing

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run tests with output
cargo test --workspace -- --nocapture

# Run integration tests (requires network)
cargo test --workspace --features integration-tests

# Run specific test
cargo test --package doc_engine test_name
```

### Test Guidelines

- **Unit tests**: Test individual functions and modules
- **Integration tests**: Test end-to-end workflows
- **Network tests**: Use `#[cfg(feature = "integration-tests")]` for tests requiring internet
- **Mock appropriately**: Avoid unnecessary network calls in unit tests

### Adding Tests

When adding new functionality:

1. Add unit tests for core logic
2. Add integration tests for new MCP tools
3. Test error conditions and edge cases
4. Ensure tests are deterministic and fast

## 📋 Code Style

### Formatting

We use standard Rust formatting:

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check
```

### Linting

We enforce clippy lints:

```bash
# Run clippy
cargo clippy --workspace --all-targets -- -D warnings

# Fix auto-fixable issues
cargo clippy --workspace --all-targets --fix
```

### Style Guidelines

- Use descriptive variable and function names
- Add documentation comments (`///`) for public APIs
- Keep functions focused and small
- Use `Result<T, E>` for fallible operations
- Prefer explicit error handling over `unwrap()`
- Use `tracing` for logging, not `println!`

## 🏗️ Project Structure

```
dociium/
├── mcp_server/           # Main MCP server implementation
│   ├── src/
│   │   ├── main.rs       # Binary entry point
│   │   ├── lib.rs        # Library entry point
│   │   ├── server.rs     # Server implementation
│   │   └── tools/        # MCP tool implementations
│   └── tests/            # Integration tests
├── doc_engine/           # Documentation processing engine
│   ├── src/
│   │   ├── lib.rs        # Main engine
│   │   ├── cache.rs      # Caching system
│   │   ├── fetcher.rs    # Crate metadata fetching
│   │   ├── scraper.rs    # docs.rs HTML scraping
│   │   ├── local.rs      # Local package analysis
│   │   └── types.rs      # Shared type definitions
│   └── tests/            # Unit tests
├── index_core/           # Search and indexing
│   ├── src/
│   │   ├── lib.rs        # Core indexing
│   │   ├── search.rs     # Full-text search
│   │   └── traits.rs     # Trait relationship indexing
│   └── tests/            # Unit tests
└── .github/              # CI/CD and templates
```

## 🔧 Types of Contributions

### 🐛 Bug Fixes

1. **Check existing issues** first
2. **Create an issue** if one doesn't exist
3. **Write a test** that reproduces the bug
4. **Fix the bug** and ensure tests pass
5. **Update documentation** if needed

### ✨ New Features

1. **Discuss the feature** in an issue first
2. **Consider the scope**: Does it fit DOCIIUM's mission?
3. **Design the API**: How will users interact with it?
4. **Implement incrementally**: Start with core functionality
5. **Add comprehensive tests**
6. **Update documentation**

### 📚 Documentation

- Fix typos and improve clarity
- Add examples for complex features
- Update API documentation
- Improve error messages

### 🏎️ Performance

- Profile before optimizing
- Add benchmarks for performance-critical code
- Consider memory usage and cache efficiency
- Test with realistic workloads

## 📝 Commit Guidelines

### Commit Message Format

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### Examples

```bash
feat(mcp): add source_snippet tool for viewing code with context

fix(cache): handle concurrent access to cache files safely

docs(readme): update installation instructions for cargo install

perf(scraper): optimize HTML parsing with streaming parser
```

## 🔄 Pull Request Process

### Before Submitting

1. **Rebase** your branch on the latest master:
   ```bash
   git fetch origin
   git rebase origin/master
   ```

2. **Run all checks**:
   ```bash
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   cargo fmt --check
   cargo build --release --bin dociium
   ```

3. **Update documentation** if needed

### PR Description

Include in your PR description:

- **What**: What changes you made
- **Why**: Why you made these changes
- **How**: How you implemented the changes
- **Testing**: How you tested the changes
- **Breaking changes**: Any breaking changes

### Review Process

1. **Automated checks** must pass (CI/CD)
2. **Code review** by maintainers
3. **Testing** on different platforms
4. **Documentation review** if applicable
5. **Merge** when approved

## 🎯 Areas for Contribution

### High Priority

- **Language support**: Add support for more programming languages
- **Performance**: Optimize caching and network requests
- **Error handling**: Improve error messages and recovery
- **Documentation**: Improve API docs and examples

### Medium Priority

- **Testing**: Increase test coverage
- **CI/CD**: Improve build and release automation
- **Monitoring**: Add metrics and observability
- **Security**: Security audits and improvements

### Good First Issues

Look for issues labeled `good first issue`:

- Documentation improvements
- Simple bug fixes
- Adding new tests
- Code cleanup and refactoring

## 🤝 Getting Help

### Communication Channels

- **GitHub Issues**: For bugs and feature requests
- **GitHub Discussions**: For questions and general discussion
- **Code Comments**: Inline questions during review

### Asking Questions

When asking for help:

1. **Search existing issues** first
2. **Provide context**: What you're trying to do
3. **Include details**: Error messages, environment info
4. **Show your work**: What you've already tried

## 📜 Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please:

- **Be respectful** and considerate
- **Be collaborative** and constructive
- **Be patient** with newcomers
- **Focus on the code**, not the person

## 🎉 Recognition

Contributors are recognized through:

- **GitHub contributor graph**
- **Release notes** mention significant contributions
- **Documentation credits** for major documentation work

## 🔗 Resources

### Rust Learning

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/)

### MCP Protocol

- [MCP Specification](https://spec.modelcontextprotocol.io/)
- [MCP SDK Documentation](https://docs.rs/rmcp/)

### Project Specific

- [docs.rs API](https://docs.rs/about/metadata)
- [tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)

---

Thank you for contributing to DOCIIUM! Your efforts help make documentation more accessible for developers everywhere. 🚀