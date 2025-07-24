//! Rust Documentation MCP Server Library
//!
//! This library provides the core functionality for the Rust Documentation MCP Server,
//! including parameter types and the main server implementation.

pub use crate::server::{
    CrateInfoParams, GetItemDocParams, ListImplsForTypeParams, ListTraitImplsParams,
    RustDocsMcpServer, SearchCratesParams, SearchSymbolsParams, SourceSnippetParams,
};

// Re-export commonly used dependencies for tests
pub use rmcp;
pub use serde_json;

pub mod server;
