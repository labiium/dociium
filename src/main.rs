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

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use dociium::RustDocsMcpServer;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

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

    let server = RustDocsMcpServer::new(cache_dir.to_str().unwrap()).await?;
    tracing::info!("MCP server initialized; awaiting stdio transport messages");

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
