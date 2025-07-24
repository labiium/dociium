//! Rust Documentation MCP Server Binary
//!
//! This binary runs the Rust Documentation MCP Server using the library implementation.

use anyhow::Result;
use mcp_server::RustDocsMcpServer;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with better formatting
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("mcp_server=debug".parse()?)
                .add_directive("doc_engine=debug".parse()?)
                .add_directive("index_core=debug".parse()?),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .init();

    tracing::info!(
        "Starting Rust Docs MCP Server v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Create cache directory
    let cache_dir = std::env::var("RDOCS_CACHE_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/.cache/rdocs-mcp", home)
    });

    tracing::info!("Using cache directory: {}", cache_dir);

    // Ensure cache directory exists
    std::fs::create_dir_all(&cache_dir)?;

    // Create the MCP server
    let server = RustDocsMcpServer::new(&cache_dir).await?;

    tracing::info!("MCP server initialized successfully");

    // Start the server with stdio transport
    tracing::info!("Starting stdio transport");
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
