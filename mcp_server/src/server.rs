//! Rust Documentation MCP Server
//!
//! A Model Context Protocol server that provides comprehensive access to Rust crate documentation,
//! trait implementations, and source code exploration.

use anyhow::Result;
use doc_engine::DocEngine;
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
    pub async fn new(cache_dir: &str) -> Result<Self> {
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
pub struct SearchSymbolsParams {
    pub crate_name: String,
    pub query: String,
    pub kinds: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub version: Option<String>,
}

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
                format!("Invalid identifier in path: {}", part),
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

        let results = self
            .engine
            .search_crates(&query, limit.unwrap_or(10))
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to search crates: {}", e), None)
            })?;

        let json_content = serde_json::to_string(&results)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

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
            ErrorData::internal_error(format!("Failed to get crate info: {}", e), None)
        })?;

        let json_content = serde_json::to_string(&info)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

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

        let doc = self
            .engine
            .get_item_doc(&crate_name, &path, version.as_deref())
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to get item documentation: {}", e), None)
            })?;

        let json_content = serde_json::to_string(&doc)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

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

        let impls = self
            .engine
            .list_trait_impls(&crate_name, &trait_path, version.as_deref())
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    format!("Failed to list trait implementations: {}", e),
                    None,
                )
            })?;

        let json_content = serde_json::to_string(&impls)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

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

        let impls = self
            .engine
            .list_impls_for_type(&crate_name, &type_path, version.as_deref())
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    format!("Failed to list type implementations: {}", e),
                    None,
                )
            })?;

        let json_content = serde_json::to_string(&impls)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

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

        // Validate context_lines
        let context = context_lines.unwrap_or(5);
        if context > 100 {
            return Err(ErrorData::invalid_params(
                "Context lines too large (max 100)",
                None,
            ));
        }

        let snippet = self
            .engine
            .source_snippet(&crate_name, &item_path, context, version.as_deref())
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to get source snippet: {}", e), None)
            })?;

        let json_content = serde_json::to_string(&snippet)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

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

        let results = self
            .engine
            .search_symbols(
                &crate_name,
                &query,
                kinds.as_deref(),
                search_limit,
                version.as_deref(),
            )
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to search symbols: {}", e), None)
            })?;

        let json_content = serde_json::to_string(&results)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

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
                "Rust Documentation MCP Server - Query Rust crate documentation, explore traits, implementations, and source code. Use search_crates to find crates, crate_info for details, get_item_doc for documentation, list_trait_impls/list_impls_for_type for implementation exploration, source_snippet for code viewing, and search_symbols for symbol discovery."
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
