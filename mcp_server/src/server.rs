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

// --- Admin Tool Parameter Structs ---

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CacheStatsParams {
    // No parameters needed for cache_stats
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PurgeCrateParams {
    pub name: String,
    pub version: String, // Version is mandatory for purging a specific crate version
}

#[tool(tool_box)]
impl RustDocsMcpServer {
    /// Search for crates on crates.io
    #[tool(description = "Search for crates on crates.io with optional limit")]
    pub async fn search_crates(
        &self,
        params: Parameters<SearchCratesParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let SearchCratesParams { query, limit } = params.0;

        if query.trim().is_empty() {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                "Search query cannot be empty.".to_string(),
            ));
        }
        if query.len() > 200 {
            // Arbitrary reasonable limit for a search query
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                "Search query is too long (max 200 characters).".to_string(),
            ));
        }
        if let Some(l) = limit {
            if l == 0 || l > 100 {
                // Crates.io limit is 100
                return Err(rmcp::handler::server::tool::Error::new(
                    rmcp::handler::server::tool::ErrorKind::InvalidParams,
                    "Limit must be between 1 and 100.".to_string(),
                ));
            }
        }

        match self.engine.search_crates(&query, limit.unwrap_or(10)).await {
            Ok(results) => Ok(results),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to search crates: {}", e),
            )),
        }
    }

    // --- Admin Tools ---

    /// Get statistics about the documentation engine's cache.
    #[tool(description = "Get statistics about the documentation engine's cache.")]
    pub async fn cache_stats(
        &self,
        _params: Parameters<CacheStatsParams>, // No params used
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        match self.engine.stats().await {
            Ok(stats) => Ok(stats.cache_stats), // Return only the CacheStats part as per spec name "cache_stats"
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to get cache stats: {}", e),
            )),
        }
    }

    /// Purge a specific crate version from all caches.
    #[tool(description = "Purge a specific crate version from all caches.")]
    pub async fn purge_crate_cache(
        // Renamed to avoid conflict if a general `purge_crate` tool for other purposes was added
        &self,
        params: Parameters<PurgeCrateParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let PurgeCrateParams { name, version } = params.0;

        // Validation
        if let Err(msg) = crate::tools::validate_crate_name(&name) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_version_str(Some(&version)) {
            // Version is mandatory here
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }

        match self.engine.purge_crate(&name, &version).await {
            Ok(_) => Ok(json!({
                "status": "success",
                "message": format!("Successfully purged crate {}@{} from caches.", name, version)
            })),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to purge crate {}@{}: {}", name, version, e),
            )),
        }
    }

    /// Get detailed information about a specific crate
    #[tool(description = "Get detailed information about a specific crate")]
    pub async fn crate_info(
        &self,
        params: Parameters<CrateInfoParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let CrateInfoParams { name } = params.0;

        if let Err(msg) = crate::tools::validate_crate_name(&name) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }

        match self.engine.crate_info(&name).await {
            Ok(info) => Ok(info),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to get crate info: {}", e),
            )),
        }
    }

    /// Get documentation for a specific item in a crate
    #[tool(description = "Get documentation for a specific item in a crate")]
    #[tracing::instrument(skip(self, params), fields(tool_name = "get_item_doc", crate_name = %params.0.crate_name, item_path = %params.0.path, version = ?params.0.version))]
    pub async fn get_item_doc(
        &self,
        params: Parameters<GetItemDocParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let GetItemDocParams {
            crate_name,
            path,
            version,
        } = params.0;

        if let Err(msg) = crate::tools::validate_crate_name(&crate_name) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_item_path(&path) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_version_str(version.as_deref()) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }

        match self
            .engine
            .get_item_doc(&crate_name, &path, version.as_deref())
            .await
        {
            Ok(doc) => Ok(doc),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to get item documentation: {}", e),
            )),
        }
    }

    /// List all implementations of a trait
    #[tool(description = "List all implementations of a specific trait")]
    pub async fn list_trait_impls(
        &self,
        params: Parameters<ListTraitImplsParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let ListTraitImplsParams {
            crate_name,
            trait_path,
            version,
        } = params.0;

        if let Err(msg) = crate::tools::validate_crate_name(&crate_name) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_item_path(&trait_path) {
            // trait_path is like an item path
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_version_str(version.as_deref()) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }

        match self
            .engine
            .list_trait_impls(&crate_name, &trait_path, version.as_deref())
            .await
        {
            Ok(impls) => Ok(impls),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to list trait implementations: {}", e),
            )),
        }
    }

    /// List all trait implementations for a specific type
    #[tool(description = "List all trait implementations for a specific type")]
    pub async fn list_impls_for_type(
        &self,
        params: Parameters<ListImplsForTypeParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let ListImplsForTypeParams {
            crate_name,
            type_path,
            version,
        } = params.0;

        if let Err(msg) = crate::tools::validate_crate_name(&crate_name) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_item_path(&type_path) {
            // type_path is like an item path
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_version_str(version.as_deref()) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }

        match self
            .engine
            .list_impls_for_type(&crate_name, &type_path, version.as_deref())
            .await
        {
            Ok(impls) => Ok(impls),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to list type implementations: {}", e),
            )),
        }
    }

    /// Get source code snippet for an item
    #[tool(description = "Get source code snippet for a specific item")]
    pub async fn source_snippet(
        &self,
        params: Parameters<SourceSnippetParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let SourceSnippetParams {
            crate_name,
            item_path,
            context_lines,
            version,
        } = params.0;

        if let Err(msg) = crate::tools::validate_crate_name(&crate_name) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_item_path(&item_path) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Err(msg) = crate::tools::validate_version_str(version.as_deref()) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        let current_context_lines = context_lines.unwrap_or(5);
        if current_context_lines > 50 {
            // Arbitrary upper limit for context lines
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                "Context lines cannot exceed 50.".to_string(),
            ));
        }

        match self
            .engine
            .source_snippet(
                &crate_name,
                &item_path,
                current_context_lines,
                version.as_deref(),
            )
            .await
        {
            Ok(snippet) => Ok(snippet),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to get source snippet: {}", e),
            )),
        }
    }

    /// Search for symbols within a crate
    #[tool(description = "Search for symbols within a crate using full-text search")]
    pub async fn search_symbols(
        &self,
        params: Parameters<SearchSymbolsParams>,
    ) -> Result<impl Serialize, rmcp::handler::server::tool::Error> {
        let SearchSymbolsParams {
            crate_name,
            query,
            kinds,
            limit,
            version,
        } = params.0;

        if let Err(msg) = crate::tools::validate_crate_name(&crate_name) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if query.trim().is_empty() {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                "Search query cannot be empty.".to_string(),
            ));
        }
        if query.len() > 200 {
            // Arbitrary reasonable limit
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                "Search query is too long (max 200 characters).".to_string(),
            ));
        }
        if let Some(l) = limit {
            if l == 0 || l > 100 {
                // Max results reasonable limit
                return Err(rmcp::handler::server::tool::Error::new(
                    rmcp::handler::server::tool::ErrorKind::InvalidParams,
                    "Limit must be between 1 and 100.".to_string(),
                ));
            }
        }
        if let Err(msg) = crate::tools::validate_version_str(version.as_deref()) {
            return Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::InvalidParams,
                msg,
            ));
        }
        if let Some(k_vec) = &kinds {
            for k_str in k_vec {
                if k_str.trim().is_empty() {
                    return Err(rmcp::handler::server::tool::Error::new(
                        rmcp::handler::server::tool::ErrorKind::InvalidParams,
                        "Kind filter cannot contain empty strings.".to_string(),
                    ));
                }
                // Optional: Validate k_str against known SymbolKind string representations
            }
        }

        // Note on kinds: DocEngine::search_symbols takes Option<&[String]>.
        // CrateDocumentation::search_symbols needs to handle conversion to index_core::SymbolKind.

        match self
            .engine
            .search_symbols(
                &crate_name,
                &query,
                kinds.as_deref(), // kinds is Option<Vec<String>>
                limit.unwrap_or(20),
                version.as_deref(),
            )
            .await
        {
            Ok(results) => Ok(results),
            Err(e) => Err(rmcp::handler::server::tool::Error::new(
                rmcp::handler::server::tool::ErrorKind::ToolSideError,
                format!("Failed to search symbols: {}", e),
            )),
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
