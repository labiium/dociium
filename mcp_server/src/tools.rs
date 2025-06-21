//! MCP tools module for additional functionality and shared utilities

use serde::{Deserialize, Serialize};

/// Search result for crates.io search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateSearchResult {
    pub name: String,
    pub latest_version: String,
    pub description: Option<String>,
    pub downloads: u64,
    pub repository: Option<String>,
    pub documentation: Option<String>,
}

/// Detailed crate information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateInfo {
    pub name: String,
    pub latest_version: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub documentation: Option<String>,
    pub license: Option<String>,
    pub downloads: u64,
    pub feature_flags: Vec<String>,
    pub dependencies: Vec<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
}

/// Documentation for a specific item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDoc {
    pub path: String,
    pub kind: String,
    pub rendered_markdown: String,
    pub source_location: Option<SourceLocation>,
    pub visibility: String,
}

/// Source location information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Trait implementation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitImpl {
    pub for_type: String,
    pub generics: Vec<String>,
    pub where_clause: Option<String>,
    pub source_span: Option<SourceLocation>,
    pub impl_id: String,
}

/// Type implementation information (traits implemented by a type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeImpl {
    pub trait_path: String,
    pub generics: Vec<String>,
    pub where_clause: Option<String>,
    pub source_span: Option<SourceLocation>,
    pub impl_id: String,
}

/// Source code snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSnippet {
    pub code: String,
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub context_lines: u32,
}

/// Symbol search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSearchResult {
    pub path: String,
    pub kind: String,
    pub score: f32,
    pub doc_summary: Option<String>,
    pub source_location: Option<SourceLocation>,
}

/// Error response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub is_error: bool,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    #[allow(dead_code)]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            is_error: true,
            message: message.into(),
            details: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_details(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            is_error: true,
            message: message.into(),
            details: Some(details),
        }
    }
}

/// Helper function to create success response
#[allow(dead_code)]
pub fn success_response<T: Serialize>(data: T) -> serde_json::Value {
    serde_json::to_value(data).unwrap_or_else(|e| {
        serde_json::json!({
            "is_error": true,
            "message": format!("Serialization error: {}", e)
        })
    })
}

/// Helper function to create error response
#[allow(dead_code)]
pub fn error_response(message: impl Into<String>) -> serde_json::Value {
    serde_json::to_value(ErrorResponse::new(message)).unwrap()
}

/// Helper function to create error response with details
#[allow(dead_code)]
pub fn error_response_with_details(
    message: impl Into<String>,
    details: serde_json::Value,
) -> serde_json::Value {
    serde_json::to_value(ErrorResponse::with_details(message, details)).unwrap()
}

/// Utility to parse semantic version strings
#[allow(dead_code)]
pub fn parse_version(version: &str) -> Result<(u32, u32, u32), String> {
    let parts: Vec<&str> = version.trim_start_matches('v').split('.').collect();
    if parts.len() < 2 {
        return Err("Invalid version format".to_string());
    }

    let major = parts[0]
        .parse::<u32>()
        .map_err(|_| "Invalid major version")?;
    let minor = parts[1]
        .parse::<u32>()
        .map_err(|_| "Invalid minor version")?;
    let patch = if parts.len() > 2 {
        parts[2]
            .split('-')
            .next()
            .unwrap_or("0")
            .parse::<u32>()
            .map_err(|_| "Invalid patch version")?
    } else {
        0
    };

    Ok((major, minor, patch))
}

/// Utility to validate crate names
#[allow(dead_code)]
pub fn validate_crate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Crate name cannot be empty".to_string());
    }

    if name.len() > 64 {
        return Err("Crate name too long (max 64 characters)".to_string());
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Crate name contains invalid characters".to_string());
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err("Crate name cannot start or end with hyphen".to_string());
    }

    Ok(())
}

/// Utility to validate item paths
#[allow(dead_code)]
pub fn validate_item_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Item path cannot be empty".to_string());
    }

    if path.len() > 512 {
        return Err("Item path too long (max 512 characters)".to_string());
    }

    // Basic validation - should contain valid Rust identifiers separated by ::
    let parts: Vec<&str> = path.split("::").collect();
    for part in parts {
        if part.is_empty() {
            return Err("Item path contains empty segments".to_string());
        }

        // Allow generics in the path
        let clean_part = part.split('<').next().unwrap_or(part);
        if !clean_part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!("Invalid identifier in path: {}", part));
        }
    }

    Ok(())
}

/// Utility to validate semantic version strings
pub fn validate_version_str(version_str: Option<&str>) -> Result<(), String> {
    if let Some(v_str) = version_str {
        if v_str.trim().is_empty() {
            return Err("Version string cannot be empty if provided.".to_string());
        }
        // Allow "latest" as a special keyword, or try to parse as semver.
        if v_str.to_lowercase() == "latest" {
            return Ok(());
        }
        semver::Version::parse(v_str)
            .map_err(|e| format!("Invalid version string '{}': {}", v_str, e))?;
    }
    // If None, it's also valid (means "use latest applicable")
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.2.3").unwrap(), (1, 2, 3));
        assert_eq!(parse_version("v1.2.3").unwrap(), (1, 2, 3));
        assert_eq!(parse_version("1.2").unwrap(), (1, 2, 0));
        assert_eq!(parse_version("1.2.3-alpha").unwrap(), (1, 2, 3));
        assert!(parse_version("invalid").is_err());
    }

    #[test]
    fn test_validate_crate_name() {
        assert!(validate_crate_name("serde").is_ok());
        assert!(validate_crate_name("serde_json").is_ok());
        assert!(validate_crate_name("serde-json").is_ok());
        assert!(validate_crate_name("").is_err());
        assert!(validate_crate_name("-serde").is_err());
        assert!(validate_crate_name("serde-").is_err());
        assert!(validate_crate_name("serde@json").is_err());
    }

    #[test]
    fn test_validate_item_path() {
        assert!(validate_item_path("std::collections::HashMap").is_ok());
        assert!(validate_item_path("HashMap").is_ok());
        assert!(validate_item_path("std::collections::HashMap<K, V>").is_ok());
        assert!(validate_item_path("").is_err());
        assert!(validate_item_path("std::").is_err());
        assert!(validate_item_path("std::::HashMap").is_err());
    }
}
