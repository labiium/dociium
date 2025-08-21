//! Rust Documentation MCP Server
//!
//! A Model Context Protocol server that provides comprehensive access to Rust crate documentation,
//! trait implementations, and source code exploration.

use crate::doc_engine::DocEngine;
use anyhow::Result;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::{CallToolResult, Content, ErrorData, Implementation, ServerCapabilities, ServerInfo},
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RustDocsMcpServer {
    engine: Arc<DocEngine>,
    tool_router: ToolRouter<Self>,
}

impl RustDocsMcpServer {
    pub async fn new(cache_dir: impl AsRef<std::path::Path>) -> Result<Self> {
        let engine = Arc::new(DocEngine::new(cache_dir).await?);

        Ok(Self {
            engine,
            tool_router: Self::tool_router(),
        })
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
pub struct GetImplementationParams {
    /// The language of the package ("python" or "node").
    pub language: String,
    /// The name of the package as known to its package manager (e.g., "curly", "express").
    pub package_name: String,
    /// Path to the item within the package, format: "path/to/file#item_name".
    pub item_path: String,
    /// Optional path to a project/environment to search within (especially for Node.js). Defaults to current dir.
    pub context_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchSymbolsParams {
    pub crate_name: String,
    pub query: String,
    pub kinds: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClearCacheParams {
    pub crate_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CacheStatsParams {}

/// Utility to validate crate names
fn validate_crate_name(name: &str) -> Result<(), ErrorData> {
    if name.is_empty() {
        return Err(ErrorData::invalid_params(
            "Crate name cannot be empty",
            None,
        ));
    }

    if name.len() > 64 {
        return Err(ErrorData::invalid_params(
            "Crate name too long (max 64 characters)",
            None,
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ErrorData::invalid_params(
            "Crate name contains invalid characters",
            None,
        ));
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err(ErrorData::invalid_params(
            "Crate name cannot start or end with hyphen",
            None,
        ));
    }

    Ok(())
}

/// Utility to validate item paths
fn validate_item_path(path: &str) -> Result<(), ErrorData> {
    if path.is_empty() {
        return Err(ErrorData::invalid_params("Item path cannot be empty", None));
    }

    if path.len() > 512 {
        return Err(ErrorData::invalid_params(
            "Item path too long (max 512 characters)",
            None,
        ));
    }

    // Basic validation - should contain valid Rust identifiers separated by ::
    let parts: Vec<&str> = path.split("::").collect();
    for part in parts {
        if part.is_empty() {
            return Err(ErrorData::invalid_params(
                "Item path contains empty segments",
                None,
            ));
        }

        // Allow generics in the path
        let clean_part = part.split('<').next().unwrap_or(part);
        if !clean_part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(ErrorData::invalid_params(
                format!("Invalid identifier in path: {part}"),
                None,
            ));
        }
    }

    Ok(())
}

#[tool_router]
impl RustDocsMcpServer {
    /// Search for crates on crates.io
    #[tool(description = "Search for crates on crates.io with optional limit")]
    pub async fn search_crates(
        &self,
        params: Parameters<SearchCratesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let SearchCratesParams { query, limit } = params.0;

        // Validate query
        if query.trim().is_empty() {
            return Err(ErrorData::invalid_params(
                "Search query cannot be empty",
                None,
            ));
        }

        if query.len() > 256 {
            return Err(ErrorData::invalid_params(
                "Search query too long (max 256 characters)",
                None,
            ));
        }

        let results = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            self.engine.search_crates(&query, limit.unwrap_or(10)),
        )
        .await
        .map_err(|_| {
            ErrorData::internal_error(
                format!("Timeout searching crates with query: {query}"),
                None,
            )
        })?
        .map_err(|e| ErrorData::internal_error(format!("Failed to search crates: {e}"), None))?;

        let json_content = serde_json::to_string(&results)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Get detailed information about a specific crate
    #[tool(description = "Get detailed information about a specific crate")]
    pub async fn crate_info(
        &self,
        params: Parameters<CrateInfoParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let CrateInfoParams { name } = params.0;

        // Validate crate name
        validate_crate_name(&name)?;

        let info = self.engine.crate_info(&name).await.map_err(|e| {
            ErrorData::internal_error(format!("Failed to get crate info: {e}"), None)
        })?;

        let json_content = serde_json::to_string(&info)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Get documentation for a specific item in a crate
    #[tool(description = "Get documentation for a specific item in a crate")]
    pub async fn get_item_doc(
        &self,
        params: Parameters<GetItemDocParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let GetItemDocParams {
            crate_name,
            path,
            version,
        } = params.0;

        // Validate inputs
        validate_crate_name(&crate_name)?;
        validate_item_path(&path)?;

        tracing::info!(
            "MCP get_item_doc: crate={}, path={}, version={:?}",
            crate_name,
            path,
            version
        );

        // Add timeout to the entire operation
        let doc = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            self.engine
                .get_item_doc(&crate_name, &path, version.as_deref()),
        )
        .await
        .map_err(|_| {
            tracing::warn!("Timeout in get_item_doc for {}::{}", crate_name, path);
            ErrorData::internal_error(
                format!("Timeout getting documentation for {crate_name}::{path}"),
                None,
            )
        })?
        .map_err(|e| {
            tracing::error!("Error in get_item_doc for {}::{}: {}", crate_name, path, e);
            ErrorData::internal_error(format!("Failed to get item documentation: {e}"), None)
        })?;

        let json_content = serde_json::to_string(&doc)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// List all implementations of a trait
    #[tool(description = "List all implementations of a specific trait")]
    pub async fn list_trait_impls(
        &self,
        params: Parameters<ListTraitImplsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ListTraitImplsParams {
            crate_name,
            trait_path,
            version,
        } = params.0;

        // Validate inputs
        validate_crate_name(&crate_name)?;
        validate_item_path(&trait_path)?;

        let impls = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.engine
                .list_trait_impls(&crate_name, &trait_path, version.as_deref()),
        )
        .await
        .map_err(|_| {
            ErrorData::internal_error(
                format!("Timeout listing trait implementations for {crate_name}::{trait_path}"),
                None,
            )
        })?
        .map_err(|e| {
            ErrorData::internal_error(format!("Failed to list trait implementations: {e}"), None)
        })?;

        let json_content = serde_json::to_string(&impls)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// List all trait implementations for a specific type
    #[tool(description = "List all trait implementations for a specific type")]
    pub async fn list_impls_for_type(
        &self,
        params: Parameters<ListImplsForTypeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ListImplsForTypeParams {
            crate_name,
            type_path,
            version,
        } = params.0;

        // Validate inputs
        validate_crate_name(&crate_name)?;
        validate_item_path(&type_path)?;

        let impls = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.engine
                .list_impls_for_type(&crate_name, &type_path, version.as_deref()),
        )
        .await
        .map_err(|_| {
            ErrorData::internal_error(
                format!("Timeout listing type implementations for {crate_name}::{type_path}"),
                None,
            )
        })?
        .map_err(|e| {
            ErrorData::internal_error(format!("Failed to list type implementations: {e}"), None)
        })?;

        let json_content = serde_json::to_string(&impls)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Get source code snippet for an item
    #[tool(description = "Get source code snippet for a specific item")]
    pub async fn source_snippet(
        &self,
        params: Parameters<SourceSnippetParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let SourceSnippetParams {
            crate_name,
            item_path,
            context_lines,
            version,
        } = params.0;

        // Validate inputs
        validate_crate_name(&crate_name)?;
        validate_item_path(&item_path)?;

        // Default context lines if not provided
        let context = context_lines.unwrap_or(5);
        if context > 100 {
            return Err(ErrorData::invalid_params(
                "context_lines too large (max 100)",
                None,
            ));
        }

        let snippet = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.engine
                .source_snippet(&crate_name, &item_path, context, version.as_deref()),
        )
        .await
        .map_err(|_| {
            ErrorData::internal_error(
                format!("Timeout getting source snippet for {crate_name}::{item_path}"),
                None,
            )
        })?
        .map_err(|e| {
            ErrorData::internal_error(format!("Failed to get source snippet: {e}"), None)
        })?;

        let json_content = serde_json::to_string(&snippet)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Get the implementation and documentation for a code item from a local environment
    #[tool(
        description = "Get the implementation and documentation for an item from an installed package (Python/Node.js)."
    )]
    pub async fn get_implementation(
        &self,
        params: Parameters<GetImplementationParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let GetImplementationParams {
            language,
            package_name,
            item_path,
            context_path,
        } = params.0;

        if package_name.trim().is_empty() {
            return Err(ErrorData::invalid_params(
                "A valid package_name is required.",
                None,
            ));
        }
        if item_path.trim().is_empty() || !item_path.contains('#') {
            return Err(ErrorData::invalid_params(
                "item_path must be in the format 'path/to/file#item_name'.",
                None,
            ));
        }

        let context = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            self.engine.get_implementation_context(
                &language,
                &package_name,
                &item_path,
                context_path.as_deref(),
            ),
        )
        .await
        .map_err(|_| {
            ErrorData::internal_error(
                format!("Timeout getting implementation context for {item_path} in {package_name}"),
                None,
            )
        })?
        .map_err(|e| {
            ErrorData::internal_error(format!("Failed to get implementation context: {e}"), None)
        })?;

        let json_content = serde_json::to_string(&context)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Resolve import statements to concrete symbol source locations (best-effort).
    #[tool(description = "Resolve import statements (use/import/from) to symbol source locations")]
    pub async fn resolve_imports(
        &self,
        params: Parameters<crate::doc_engine::types::ImportResolutionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;

        // Basic validation
        if p.language.trim().is_empty() {
            return Err(ErrorData::invalid_params("language cannot be empty", None));
        }
        if p.package.trim().is_empty() {
            return Err(ErrorData::invalid_params("package cannot be empty", None));
        }
        if p.import_line.is_none() && p.code_block.is_none() {
            return Err(ErrorData::invalid_params(
                "Either import_line or code_block must be provided",
                None,
            ));
        }

        let fut = self.engine.resolve_imports(&p);

        let response = tokio::time::timeout(std::time::Duration::from_secs(30), fut)
            .await
            .map_err(|_| {
                ErrorData::internal_error(
                    "Timeout resolving imports (exceeded 30s)".to_string(),
                    None,
                )
            })?
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to resolve imports: {e}"), None)
            })?;

        let json_content = serde_json::to_string(&response)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Search for symbols within a crate
    #[tool(description = "Search for symbols within a crate using full-text search")]
    pub async fn search_symbols(
        &self,
        params: Parameters<SearchSymbolsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let SearchSymbolsParams {
            crate_name,
            query,
            kinds,
            limit,
            version,
        } = params.0;

        // Validate inputs
        validate_crate_name(&crate_name)?;

        if query.trim().is_empty() {
            return Err(ErrorData::invalid_params(
                "Search query cannot be empty",
                None,
            ));
        }

        if query.len() > 256 {
            return Err(ErrorData::invalid_params(
                "Search query too long (max 256 characters)",
                None,
            ));
        }

        // Validate limit
        let search_limit = limit.unwrap_or(20);
        if search_limit > 100 {
            return Err(ErrorData::invalid_params(
                "Search limit too large (max 100)",
                None,
            ));
        }

        let results = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.engine.search_symbols(
                &crate_name,
                &query,
                kinds.as_deref(),
                search_limit,
                version.as_deref(),
            ),
        )
        .await
        .map_err(|_| {
            ErrorData::internal_error(
                format!("Timeout searching symbols in {crate_name} for query: {query}"),
                None,
            )
        })?
        .map_err(|e| ErrorData::internal_error(format!("Failed to search symbols: {e}"), None))?;

        // Convert engine (legacy) SymbolSearchResult into shared_types canonical form
        let shared_results: Vec<crate::shared_types::SymbolSearchResult> = results
            .into_iter()
            .map(|r| crate::shared_types::SymbolSearchResult {
                path: r.path,
                kind: r.kind,
                score: r.score,
                doc_summary: r.doc_summary,
                source_location: r
                    .source_location
                    .map(|sl| crate::shared_types::SourceLocation {
                        file: sl.file,
                        line: sl.line,
                        column: sl.column,
                        end_line: sl.end_line,
                        end_column: sl.end_column,
                    }),
                visibility: r.visibility,
                signature: r.signature,
                module_path: r.module_path,
            })
            .collect();

        let json_content = serde_json::to_string(&shared_results)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Get cache statistics
    #[tool(description = "Get cache statistics and performance metrics")]
    pub async fn get_cache_stats(
        &self,
        _params: Parameters<CacheStatsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let stats = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.engine.get_cache_stats(),
        )
        .await
        .map_err(|_| ErrorData::internal_error("Timeout getting cache stats".to_string(), None))?
        .map_err(|e| ErrorData::internal_error(format!("Failed to get cache stats: {e}"), None))?;

        let json_content = serde_json::to_string(&stats)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Clear cache entries
    #[tool(description = "Clear cache entries for all crates or a specific crate")]
    pub async fn clear_cache(
        &self,
        params: Parameters<ClearCacheParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = if let Some(crate_name) = params.0.crate_name {
            // Clear cache for specific crate
            validate_crate_name(&crate_name)?;
            self.engine
                .clear_crate_cache(&crate_name)
                .await
                .map_err(|e| {
                    ErrorData::internal_error(format!("Failed to clear crate cache: {e}"), None)
                })?
        } else {
            // Clear all cache
            self.engine.clear_all_cache().await.map_err(|e| {
                ErrorData::internal_error(format!("Failed to clear all cache: {e}"), None)
            })?
        };

        let json_content = serde_json::to_string(&result)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }

    /// Cleanup expired cache entries
    #[tool(description = "Remove expired cache entries based on TTL")]
    pub async fn cleanup_cache(
        &self,
        _params: Parameters<CacheStatsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.engine.cleanup_expired_cache(),
        )
        .await
        .map_err(|_| ErrorData::internal_error("Timeout cleaning up cache".to_string(), None))?
        .map_err(|e| ErrorData::internal_error(format!("Failed to cleanup cache: {e}"), None))?;

        let json_content = serde_json::to_string(&result)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_content)]))
    }
}

#[tool_handler(router = self.tool_router)]
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
                "Rust Documentation MCP Server - Query Rust crate documentation, explore traits, implementations, and source code. Use search_crates to find crates, crate_info for details, get_item_doc for documentation, list_trait_impls/list_impls_for_type for implementation exploration, source_snippet for code viewing, search_symbols for symbol discovery, get_cache_stats for cache statistics, clear_cache to clear cache entries, and cleanup_cache to remove expired entries."
                    .to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: rmcp::model::InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::InitializeResult, ErrorData> {
        Ok(self.get_info())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_crate_name() {
        assert!(validate_crate_name("serde").is_ok());
        assert!(validate_crate_name("serde_json").is_ok());
        assert!(validate_crate_name("serde-json").is_ok());
        assert!(validate_crate_name("").is_err());
        assert!(validate_crate_name("-serde").is_err());
        assert!(validate_crate_name("serde-").is_err());
        assert!(validate_crate_name("serde@json").is_err());
        assert!(validate_crate_name(&"a".repeat(100)).is_err());
    }

    #[test]
    fn test_validate_item_path() {
        assert!(validate_item_path("std::collections::HashMap").is_ok());
        assert!(validate_item_path("HashMap").is_ok());
        assert!(validate_item_path("std::collections::HashMap<K, V>").is_ok());
        assert!(validate_item_path("").is_err());
        assert!(validate_item_path("std::").is_err());
        assert!(validate_item_path("std::::HashMap").is_err());
        assert!(validate_item_path(&"a".repeat(1000)).is_err());
    }
}
