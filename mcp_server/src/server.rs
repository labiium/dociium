//! Rust Documentation MCP Server
//!
//! A Model Context Protocol server that provides comprehensive access to Rust crate documentation,
//! trait implementations, and source code exploration.

use anyhow::Result;
use doc_engine::DocEngine;
use rmcp::{
    handler::server::tool::Parameters,
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RustDocsMcpServer {
    engine: Arc<DocEngine>,
}

impl RustDocsMcpServer {
    pub async fn new(cache_dir: &str) -> Result<Self> {
        let engine = Arc::new(DocEngine::new(cache_dir).await?);
        Ok(Self { engine })
    }
}

// Parameter structures for each tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchCratesParams {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CrateInfoParams {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetItemDocParams {
    pub crate_name: String,
    pub path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListTraitImplsParams {
    pub crate_name: String,
    pub trait_path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListImplsForTypeParams {
    pub crate_name: String,
    pub type_path: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceSnippetParams {
    pub crate_name: String,
    pub item_path: String,
    pub context_lines: Option<u32>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchSymbolsParams {
    pub crate_name: String,
    pub query: String,
    pub kinds: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub version: Option<String>,
}

#[tool(tool_box)]
impl RustDocsMcpServer {
    /// Search for crates on crates.io
    #[tool(description = "Search for crates on crates.io with optional limit")]
    pub async fn search_crates(&self, params: Parameters<SearchCratesParams>) -> String {
        let SearchCratesParams { query, limit } = params.0;
        match self.engine.search_crates(&query, limit.unwrap_or(10)).await {
            Ok(results) => serde_json::to_string(&json!(results)).unwrap_or_else(|e| {
                json!({
                    "error": format!("Serialization error: {}", e)
                })
                .to_string()
            }),
            Err(e) => json!({
                "error": format!("Failed to search crates: {}", e)
            })
            .to_string(),
        }
    }

    /// Get detailed information about a specific crate
    #[tool(description = "Get detailed information about a specific crate")]
    pub async fn crate_info(&self, params: Parameters<CrateInfoParams>) -> String {
        let CrateInfoParams { name } = params.0;
        match self.engine.crate_info(&name).await {
            Ok(info) => serde_json::to_string(&json!(info)).unwrap_or_else(|e| {
                json!({
                    "error": format!("Serialization error: {}", e)
                })
                .to_string()
            }),
            Err(e) => json!({
                "error": format!("Failed to get crate info: {}", e)
            })
            .to_string(),
        }
    }

    /// Get documentation for a specific item in a crate
    #[tool(description = "Get documentation for a specific item in a crate")]
    pub async fn get_item_doc(&self, params: Parameters<GetItemDocParams>) -> String {
        let GetItemDocParams {
            crate_name,
            path,
            version,
        } = params.0;
        match self
            .engine
            .get_item_doc(&crate_name, &path, version.as_deref())
            .await
        {
            Ok(doc) => serde_json::to_string(&json!(doc)).unwrap_or_else(|e| {
                json!({
                    "error": format!("Serialization error: {}", e)
                })
                .to_string()
            }),
            Err(e) => json!({
                "error": format!("Failed to get item documentation: {}", e)
            })
            .to_string(),
        }
    }

    /// List all implementations of a trait
    #[tool(description = "List all implementations of a specific trait")]
    pub async fn list_trait_impls(&self, params: Parameters<ListTraitImplsParams>) -> String {
        let ListTraitImplsParams {
            crate_name,
            trait_path,
            version,
        } = params.0;
        match self
            .engine
            .list_trait_impls(&crate_name, &trait_path, version.as_deref())
            .await
        {
            Ok(impls) => serde_json::to_string(&json!(impls)).unwrap_or_else(|e| {
                json!({
                    "error": format!("Serialization error: {}", e)
                })
                .to_string()
            }),
            Err(e) => json!({
                "error": format!("Failed to list trait implementations: {}", e)
            })
            .to_string(),
        }
    }

    /// List all trait implementations for a specific type
    #[tool(description = "List all trait implementations for a specific type")]
    pub async fn list_impls_for_type(&self, params: Parameters<ListImplsForTypeParams>) -> String {
        let ListImplsForTypeParams {
            crate_name,
            type_path,
            version,
        } = params.0;
        match self
            .engine
            .list_impls_for_type(&crate_name, &type_path, version.as_deref())
            .await
        {
            Ok(impls) => serde_json::to_string(&json!(impls)).unwrap_or_else(|e| {
                json!({
                    "error": format!("Serialization error: {}", e)
                })
                .to_string()
            }),
            Err(e) => json!({
                "error": format!("Failed to list type implementations: {}", e)
            })
            .to_string(),
        }
    }

    /// Get source code snippet for an item
    #[tool(description = "Get source code snippet for a specific item")]
    pub async fn source_snippet(&self, params: Parameters<SourceSnippetParams>) -> String {
        let SourceSnippetParams {
            crate_name,
            item_path,
            context_lines,
            version,
        } = params.0;
        match self
            .engine
            .source_snippet(
                &crate_name,
                &item_path,
                context_lines.unwrap_or(5),
                version.as_deref(),
            )
            .await
        {
            Ok(snippet) => serde_json::to_string(&json!(snippet)).unwrap_or_else(|e| {
                json!({
                    "error": format!("Serialization error: {}", e)
                })
                .to_string()
            }),
            Err(e) => json!({
                "error": format!("Failed to get source snippet: {}", e)
            })
            .to_string(),
        }
    }

    /// Search for symbols within a crate
    #[tool(description = "Search for symbols within a crate using full-text search")]
    pub async fn search_symbols(&self, params: Parameters<SearchSymbolsParams>) -> String {
        let SearchSymbolsParams {
            crate_name,
            query,
            kinds,
            limit,
            version,
        } = params.0;
        match self
            .engine
            .search_symbols(
                &crate_name,
                &query,
                kinds.as_deref(),
                limit.unwrap_or(20),
                version.as_deref(),
            )
            .await
        {
            Ok(results) => serde_json::to_string(&json!(results)).unwrap_or_else(|e| {
                json!({
                    "error": format!("Serialization error: {}", e)
                })
                .to_string()
            }),
            Err(e) => json!({
                "error": format!("Failed to search symbols: {}", e)
            })
            .to_string(),
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for RustDocsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "rust-docs-mcp-server".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "Rust Documentation MCP Server - Query Rust crate documentation, explore traits, implementations, and source code."
                    .to_string(),
            ),
        }
    }
}
