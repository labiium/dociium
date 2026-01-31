//! Dociium CLI - Multi-Language Documentation & Code Intelligence
//!
//! Provides both MCP server modes (stdio/http) and direct CLI access to all tools.

use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use axum::Router;
use clap::{Parser, Subcommand};
use dociium::{doc_engine::DocEngine, RustDocsMcpServer, ToolConfig};
use rmcp::{
    transport::{
        stdio, streamable_http_server::session::local::LocalSessionManager,
        StreamableHttpServerConfig, StreamableHttpService,
    },
    ServiceExt,
};
use tokio::sync::Notify;
use tracing_subscriber::EnvFilter;

mod cli_tools;

/// Dociium: Fast multi-language documentation and code intelligence
#[derive(Debug, Parser)]
#[command(
    name = "dociium",
    version,
    about = "Fast Rust/Python/Node documentation + code intelligence",
    long_about = "Dociium provides both MCP server modes and direct CLI access to documentation tools.\n\n\
                  Server modes:\n  \
                  dociium stdio    - Run as MCP server over stdio\n  \
                  dociium http     - Run as MCP server over HTTP\n\n\
                  Direct tool access:\n  \
                  dociium search-crates <query>           - Search crates.io\n  \
                  dociium get-item-doc <crate> <path>     - Get item documentation\n  \
                  dociium list-class-methods <pkg> <path> - List class methods\n  \
                  ...and more! Use --help on any subcommand for details."
)]
struct Cli {
    /// Explicit cache directory (overrides env + platform default)
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,

    /// Suppress info logs (only warnings+)
    #[arg(long, global = true)]
    quiet: bool,

    /// Force ANSI color output in logs
    #[arg(long, global = true)]
    color: bool,

    /// Disable ANSI color output
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run MCP server over stdio (default mode)
    Stdio {
        /// Enable only Rust tools (disables Python and Node.js tools)
        #[arg(long)]
        rust_only: bool,

        /// Enable only Python tools (disables Rust and Node.js tools)
        #[arg(long)]
        python_only: bool,

        /// Enable only Node.js tools (disables Rust and Python tools)
        #[arg(long)]
        node_only: bool,

        /// Disable Rust tools
        #[arg(long)]
        no_rust: bool,

        /// Disable Python tools
        #[arg(long)]
        no_python: bool,

        /// Disable Node.js tools
        #[arg(long)]
        no_node: bool,

        /// Disable cache management tools
        #[arg(long)]
        no_cache: bool,
    },

    /// Run MCP server over HTTP
    Http {
        /// Address to bind to (e.g., 127.0.0.1:8080)
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: String,

        /// HTTP path prefix for MCP endpoint
        #[arg(long, default_value = "/mcp")]
        path: String,

        /// Disable HTTP session persistence (stateless mode)
        #[arg(long)]
        stateless: bool,

        /// SSE keep-alive ping interval in seconds (0 to disable)
        #[arg(long)]
        keep_alive: Option<u64>,

        /// Enable only Rust tools (disables Python and Node.js tools)
        #[arg(long)]
        rust_only: bool,

        /// Enable only Python tools (disables Rust and Node.js tools)
        #[arg(long)]
        python_only: bool,

        /// Enable only Node.js tools (disables Rust and Python tools)
        #[arg(long)]
        node_only: bool,

        /// Disable Rust tools
        #[arg(long)]
        no_rust: bool,

        /// Disable Python tools
        #[arg(long)]
        no_python: bool,

        /// Disable Node.js tools
        #[arg(long)]
        no_node: bool,

        /// Disable cache management tools
        #[arg(long)]
        no_cache: bool,
    },

    /// Print the resolved cache directory and exit
    PrintCacheDir,

    // ===== Rust Documentation Tools =====
    /// Search for crates on crates.io
    SearchCrates {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(long, short, default_value = "10")]
        limit: u32,
    },

    /// Get detailed information about a crate
    CrateInfo {
        /// Crate name
        name: String,
    },

    /// Get documentation for a specific item in a crate
    GetItemDoc {
        /// Crate name
        crate_name: String,

        /// Item path (e.g., "std::vec::Vec")
        path: String,

        /// Crate version (optional)
        #[arg(long)]
        version: Option<String>,
    },

    /// List all implementations of a trait
    ListTraitImpls {
        /// Crate name
        crate_name: String,

        /// Trait path (e.g., "std::fmt::Display")
        trait_path: String,

        /// Crate version (optional)
        #[arg(long)]
        version: Option<String>,
    },

    /// List all trait implementations for a type
    ListImplsForType {
        /// Crate name
        crate_name: String,

        /// Type path (e.g., "Vec")
        type_path: String,

        /// Crate version (optional)
        #[arg(long)]
        version: Option<String>,
    },

    /// Get source code snippet for an item
    SourceSnippet {
        /// Crate name
        crate_name: String,

        /// Item path
        item_path: String,

        /// Number of context lines before/after
        #[arg(long, default_value = "5")]
        context: u32,

        /// Crate version (optional)
        #[arg(long)]
        version: Option<String>,
    },

    /// Search for symbols within a crate
    SearchSymbols {
        /// Crate name
        crate_name: String,

        /// Search query
        query: String,

        /// Filter by symbol kinds (comma-separated: struct,fn,trait,etc)
        #[arg(long)]
        kinds: Option<String>,

        /// Maximum number of results
        #[arg(long, short, default_value = "10")]
        limit: u32,

        /// Crate version (optional)
        #[arg(long)]
        version: Option<String>,
    },

    // ===== Python/Node.js Tools =====
    /// Get implementation from installed package
    GetImplementation {
        /// Language (python or node)
        #[arg(long, short)]
        language: String,

        /// Package name
        package: String,

        /// Item path (format: "path/to/file#ItemName")
        path: String,

        /// Context path (project directory)
        #[arg(long)]
        context: Option<String>,
    },

    /// List all methods of a class (Python)
    ListClassMethods {
        /// Package name
        package: String,

        /// Item path (format: "path/to/file#ClassName")
        path: String,

        /// Include private methods (starting with _)
        #[arg(long)]
        private: bool,

        /// Context path (project directory)
        #[arg(long, default_value = ".")]
        context: String,
    },

    /// Get a specific method from a class (Python)
    GetClassMethod {
        /// Package name
        package: String,

        /// Item path (format: "path/to/file#ClassName")
        path: String,

        /// Method name
        method: String,

        /// Context path (project directory)
        #[arg(long, default_value = ".")]
        context: String,
    },

    /// Search for code patterns across a Python package
    SearchPackageCode {
        /// Package name
        package: String,

        /// Regex pattern
        pattern: String,

        /// Search mode (name, signature, docstring, fulltext)
        #[arg(long, short, default_value = "name")]
        mode: String,

        /// Maximum number of results
        #[arg(long, short, default_value = "10")]
        limit: u32,

        /// Context path (project directory)
        #[arg(long, default_value = ".")]
        context: String,
    },

    /// Semantic search within a Python package
    SemanticSearch {
        /// Package name
        package: String,

        /// Natural language query
        query: String,

        /// Maximum number of results
        #[arg(long, short, default_value = "10")]
        limit: u32,

        /// Context path (project directory)
        #[arg(long, default_value = ".")]
        context: String,
    },

    // ===== Cache Management =====
    /// Get cache statistics
    CacheStats,

    /// Clear cache entries
    ClearCache {
        /// Crate name (optional, clears all if omitted)
        crate_name: Option<String>,
    },

    /// Cleanup expired cache entries
    CleanupCache,
}

fn resolve_cache_dir(cli: &Cli) -> PathBuf {
    if let Some(dir) = &cli.cache_dir {
        return dir.clone();
    }
    if let Ok(env_dir) = std::env::var("RDOCS_CACHE_DIR") {
        return PathBuf::from(env_dir);
    }
    if let Some(base) = dirs::cache_dir() {
        return base.join("dociium");
    }
    PathBuf::from(".").join(".dociium-cache")
}

fn build_tool_config(
    rust_only: bool,
    python_only: bool,
    node_only: bool,
    no_rust: bool,
    no_python: bool,
    no_node: bool,
    no_cache: bool,
) -> Result<ToolConfig> {
    // Check for conflicting flags
    let exclusive_count = [rust_only, python_only, node_only]
        .iter()
        .filter(|&&x| x)
        .count();

    if exclusive_count > 1 {
        return Err(anyhow!(
            "Cannot use multiple --*-only flags simultaneously (use at most one of --rust-only, --python-only, --node-only)"
        ));
    }

    // Build config based on flags
    let mut config = if rust_only {
        ToolConfig::rust_only()
    } else if python_only {
        ToolConfig::python_only()
    } else if node_only {
        ToolConfig::node_only()
    } else {
        ToolConfig::all()
    };

    // Apply disablement flags
    config = config.apply_disables(no_rust, no_python, no_node, no_cache);

    // Validate that at least one tool category is enabled
    config.validate()?;

    Ok(config)
}

fn setup_logging(cli: &Cli) -> Result<()> {
    let mut fmt = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(if cli.quiet { "warn" } else { "info" }.parse()?)
                .add_directive("doc_engine=debug".parse()?)
                .add_directive("index_core=debug".parse()?),
        )
        .with_writer(std::io::stderr)
        .with_target(false);

    if cli.no_color {
        fmt = fmt.with_ansi(false);
    } else if cli.color {
        fmt = fmt.with_ansi(true);
    }

    fmt.init();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle PrintCacheDir early (no logging needed)
    if matches!(cli.command, Commands::PrintCacheDir) {
        let cache_dir = resolve_cache_dir(&cli);
        println!("{}", cache_dir.display());
        return Ok(());
    }

    setup_logging(&cli)?;

    let cache_dir = resolve_cache_dir(&cli);
    std::fs::create_dir_all(&cache_dir)?;

    match cli.command {
        Commands::Stdio {
            rust_only,
            python_only,
            node_only,
            no_rust,
            no_python,
            no_node,
            no_cache,
        } => {
            let config = build_tool_config(
                rust_only,
                python_only,
                node_only,
                no_rust,
                no_python,
                no_node,
                no_cache,
            )?;
            run_stdio_server(&cache_dir, config).await
        }
        Commands::Http {
            listen,
            path,
            stateless,
            keep_alive,
            rust_only,
            python_only,
            node_only,
            no_rust,
            no_python,
            no_node,
            no_cache,
        } => {
            let config = build_tool_config(
                rust_only,
                python_only,
                node_only,
                no_rust,
                no_python,
                no_node,
                no_cache,
            )?;
            run_http_server(&cache_dir, &listen, &path, stateless, keep_alive, config).await
        }
        Commands::PrintCacheDir => {
            unreachable!("Handled above")
        }

        // Delegate all tool commands to cli_tools module
        cmd => {
            let engine = Arc::new(DocEngine::new(&cache_dir).await?);
            cli_tools::handle_command(cmd, engine).await
        }
    }
}

async fn run_stdio_server(cache_dir: &PathBuf, config: ToolConfig) -> Result<()> {
    tracing::info!(
        "Starting Dociium MCP Server v{} (cache: {}, rust: {}, python: {}, node: {}, cache_mgmt: {})",
        env!("CARGO_PKG_VERSION"),
        cache_dir.display(),
        config.rust_enabled,
        config.python_enabled,
        config.node_enabled,
        config.cache_enabled
    );

    let engine = Arc::new(DocEngine::new(cache_dir).await?);
    let server = RustDocsMcpServer::from_engine_with_config(engine, config);

    tracing::info!("MCP server initialized; awaiting stdio transport messages");
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

async fn run_http_server(
    cache_dir: &PathBuf,
    listen: &str,
    path: &str,
    stateless: bool,
    keep_alive: Option<u64>,
    tool_config: ToolConfig,
) -> Result<()> {
    let addr: SocketAddr = listen
        .parse()
        .with_context(|| format!("Invalid listen address '{listen}'"))?;

    tracing::info!(
        "Starting Dociium HTTP Server v{} (cache: {}, rust: {}, python: {}, node: {}, cache_mgmt: {})",
        env!("CARGO_PKG_VERSION"),
        cache_dir.display(),
        tool_config.rust_enabled,
        tool_config.python_enabled,
        tool_config.node_enabled,
        tool_config.cache_enabled
    );

    let mut config = StreamableHttpServerConfig {
        stateful_mode: !stateless,
        ..Default::default()
    };

    if let Some(secs) = keep_alive {
        if secs == 0 {
            config.sse_keep_alive = None;
        } else {
            config.sse_keep_alive = Some(Duration::from_secs(secs));
        }
    }

    let engine = Arc::new(DocEngine::new(cache_dir).await?);
    let session_manager = Arc::new(LocalSessionManager::default());

    let http_service: StreamableHttpService<RustDocsMcpServer, _> = StreamableHttpService::new(
        move || {
            Ok::<_, std::io::Error>(RustDocsMcpServer::from_engine_with_config(
                Arc::clone(&engine),
                tool_config.clone(),
            ))
        },
        session_manager,
        config,
    );

    let mut route_path = path.trim().to_string();
    if route_path.is_empty() {
        route_path.push('/');
    }
    if !route_path.starts_with('/') {
        route_path.insert(0, '/');
    }
    if route_path.len() > 1 && route_path.ends_with('/') {
        route_path.pop();
    }

    let router = Router::new().nest_service(route_path.as_str(), http_service);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual_addr = listener.local_addr()?;

    tracing::info!(
        "HTTP transport listening on http://{}{} (stateful: {})",
        actual_addr,
        route_path,
        !stateless
    );

    let shutdown = Arc::new(Notify::new());
    let shutdown_signal = shutdown.clone();
    let server_task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                shutdown_signal.notified().await;
            })
            .await
    });

    tracing::info!("Press Ctrl+C to stop the server...");
    tokio::signal::ctrl_c()
        .await
        .context("Failed to install Ctrl+C handler")?;

    shutdown.notify_waiters();

    match server_task.await {
        Ok(Ok(())) => tracing::info!("HTTP server shut down cleanly"),
        Ok(Err(err)) => return Err(anyhow!("HTTP server error: {err}")),
        Err(join_err) => return Err(anyhow!("HTTP server task join error: {join_err}")),
    }

    Ok(())
}
