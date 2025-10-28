//! Rust Documentation MCP Server Binary
//!
//! Provides an MCP (Model Context Protocol) server exposing multi‑language documentation
//! and source inspection (Rust / Python / Node.js) to compatible AI tooling.
//!
//! Added: basic CLI with --help / --version (clap) and improved cross‑platform cache
//! directory selection with override precedence:
//!   1. --cache-dir flag
//!   2. RDOCS_CACHE_DIR env var
//!   3. XDG / platform cache dir via dirs crate
//!   4. Fallback: ./.dociium-cache

use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use axum::Router;
use clap::{Parser, ValueEnum};
use dociium::{doc_engine::DocEngine, RustDocsMcpServer};
use rmcp::{
    transport::{
        stdio, streamable_http_server::session::local::LocalSessionManager,
        StreamableHttpServerConfig, StreamableHttpService,
    },
    ServiceExt,
};
use tokio::sync::Notify;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab_case")]
enum TransportMode {
    Stdio,
    StreamableHttp,
}

/// Command line interface for the dociium MCP server.
#[derive(Debug, Parser)]
#[command(
    name = "dociium",
    version,
    about = "Dociium MCP server: fast Rust/Python/Node documentation + code retrieval"
)]
struct Cli {
    /// Explicit cache directory (overrides env + platform default)
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Suppress info logs (only warnings+)
    #[arg(long)]
    quiet: bool,

    /// Force ANSI color output in logs
    #[arg(long)]
    color: bool,

    /// Disable ANSI color output
    #[arg(long)]
    no_color: bool,

    /// Print the resolved cache directory and exit
    #[arg(long)]
    print_cache_dir: bool,

    /// Transport to expose (stdio or streamable-http)
    #[arg(long, value_enum, default_value_t = TransportMode::Stdio)]
    transport: TransportMode,

    /// Address to bind the streamable HTTP transport to (e.g. 127.0.0.1:8080)
    #[arg(long, value_name = "ADDR")]
    http_listen: Option<String>,

    /// HTTP path prefix for the MCP endpoint (default /mcp)
    #[arg(long, default_value = "/mcp")]
    http_path: String,

    /// Disable HTTP session persistence (stateless mode)
    #[arg(long)]
    http_stateless: bool,

    /// Override SSE keep-alive ping interval (seconds). Use 0 to disable pings.
    #[arg(long, value_name = "SECONDS")]
    http_keep_alive_secs: Option<u64>,
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
    // Fallback: local hidden directory
    PathBuf::from(".").join(".dociium-cache")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Logging / tracing setup
    let mut fmt = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(
                    if cli.quiet {
                        "info"
                    } else {
                        "mcp_server=debug"
                    }
                    .parse()?,
                )
                .add_directive("doc_engine=debug".parse()?)
                .add_directive("index_core=debug".parse()?),
        )
        .with_writer(std::io::stderr)
        .with_target(false);

    // Color handling precedence: --no-color > inherited tty auto > --color
    if cli.no_color {
        fmt = fmt.with_ansi(false);
    } else if cli.color {
        fmt = fmt.with_ansi(true);
    } else {
        // auto: leave default (enabled if stderr is a TTY)
    }
    fmt.init();

    let cache_dir = resolve_cache_dir(&cli);
    if cli.print_cache_dir {
        println!("{}", cache_dir.display());
        return Ok(());
    }

    std::fs::create_dir_all(&cache_dir)?;
    tracing::info!(
        "Starting Dociium MCP Server v{} (cache: {})",
        env!("CARGO_PKG_VERSION"),
        cache_dir.display()
    );

    if cli.transport == TransportMode::StreamableHttp && cli.http_listen.is_none() {
        return Err(anyhow!(
            "--http-listen must be provided when using transport=streamable-http"
        ));
    }

    let engine = Arc::new(DocEngine::new(&cache_dir).await?);

    match cli.transport {
        TransportMode::Stdio => {
            let server = RustDocsMcpServer::from_engine(Arc::clone(&engine));
            tracing::info!("MCP server initialized; awaiting stdio transport messages");
            let service = server.serve(stdio()).await?;
            service.waiting().await?;
        }
        TransportMode::StreamableHttp => {
            let listen = cli.http_listen.as_ref().expect("validated above");
            let addr: SocketAddr = listen
                .parse()
                .with_context(|| format!("Invalid --http-listen address '{listen}'"))?;

            let mut config = StreamableHttpServerConfig {
                stateful_mode: !cli.http_stateless,
                ..Default::default()
            };
            if let Some(secs) = cli.http_keep_alive_secs {
                if secs == 0 {
                    config.sse_keep_alive = None;
                } else {
                    config.sse_keep_alive = Some(Duration::from_secs(secs));
                }
            }

            let session_manager = Arc::new(LocalSessionManager::default());
            let engine_for_service = Arc::clone(&engine);
            let http_service: StreamableHttpService<RustDocsMcpServer, _> =
                StreamableHttpService::new(
                    move || {
                        Ok::<_, std::io::Error>(RustDocsMcpServer::from_engine(Arc::clone(
                            &engine_for_service,
                        )))
                    },
                    session_manager,
                    config,
                );

            let mut route_path = cli.http_path.trim().to_string();
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
                "Streamable HTTP transport listening on http://{}{} (stateful sessions: {})",
                actual_addr,
                route_path,
                !cli.http_stateless
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

            tracing::info!("Press Ctrl+C to stop the HTTP transport…");
            tokio::signal::ctrl_c()
                .await
                .context("Failed to install Ctrl+C handler")?;
            shutdown.notify_waiters();

            match server_task.await {
                Ok(Ok(())) => tracing::info!("HTTP transport shut down cleanly"),
                Ok(Err(err)) => return Err(anyhow!("HTTP server exited with error: {err}")),
                Err(join_err) => return Err(anyhow!("HTTP server task join error: {join_err}")),
            }
        }
    }

    Ok(())
}
