//! Rust Documentation MCP Server Binary
//!
//! This binary runs the Rust Documentation MCP Server using the library implementation.

use anyhow::{Context, Result};
use mcp_server::RustDocsMcpServer;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::{prelude::*, registry::Registry, EnvFilter};

#[cfg(feature = "opentelemetry-export")]
use opentelemetry::KeyValue;
#[cfg(feature = "opentelemetry-export")]
use opentelemetry::{
    runtime,
    sdk::{trace as sdktrace, Resource},
};
#[cfg(feature = "opentelemetry-export")]
use opentelemetry_otlp::WithExportConfig;

#[cfg(feature = "opentelemetry-export")]
fn init_otel_tracer() -> Result<()> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic() // Requires opentelemetry-otlp feature "grpc-tonic"
                .with_endpoint(
                    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                        .unwrap_or_else(|_| "http://localhost:4317".to_string()),
                ),
        )
        .with_trace_config(sdktrace::config().with_resource(Resource::new(vec![
            KeyValue::new("service.name", "rdocs-mcp-server"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ])))
        .install_batch(runtime::Tokio) // Requires opentelemetry feature "rt-tokio"
        .context("Failed to install OTLP tracer pipeline")?;
    Ok(())
}

#[cfg(not(feature = "opentelemetry-export"))]
fn init_otel_tracer() -> Result<()> {
    // No-op if feature is not enabled
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize OpenTelemetry tracer if feature is enabled
    init_otel_tracer().context("Failed to initialize OpenTelemetry tracer")?;

    // Initialize tracing with better formatting
    let env_filter = EnvFilter::from_default_env()
        .add_directive("mcp_server=debug".parse()?)
        .add_directive("doc_engine=debug".parse()?)
        .add_directive("index_core=debug".parse()?);

    let subscriber = Registry::default().with(env_filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .with_target(true),
    );

    #[cfg(feature = "opentelemetry-export")]
    let subscriber = subscriber.with(
        tracing_opentelemetry::layer()
            .with_tracer(opentelemetry::global::tracer("rdocs-mcp-server")),
    );

    subscriber.init();

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
