//! Type definitions for the documentation engine

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
    pub homepage: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
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
    pub recent_downloads: Option<u64>,
    pub feature_flags: Vec<String>,
    pub dependencies: Vec<DependencyInfo>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub versions: Vec<VersionInfo>,
    pub authors: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Dependency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub version_req: String,
    pub kind: String,
    pub optional: bool,
    pub default_features: bool,
    pub features: Vec<String>,
}

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub downloads: u64,
    pub yanked: bool,
    pub created_at: Option<String>,
}

/// Documentation for a specific item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDoc {
    pub path: String,
    pub kind: String,
    pub rendered_markdown: String,
    pub source_location: Option<SourceLocation>,
    pub visibility: String,
    pub attributes: Vec<String>,
    pub signature: Option<String>,
    pub examples: Vec<String>,
    pub see_also: Vec<String>,
}

/// Source location information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
}

/// Trait implementation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitImpl {
    pub for_type: String,
    pub trait_path: String,
    pub generics: Vec<String>,
    pub where_clause: Option<String>,
    pub source_span: Option<SourceLocation>,
    pub impl_id: String,
    pub items: Vec<ImplItem>,
    pub is_blanket: bool,
    pub is_synthetic: bool,
}

/// Type implementation information (traits implemented by a type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeImpl {
    pub trait_path: String,
    pub generics: Vec<String>,
    pub where_clause: Option<String>,
    pub source_span: Option<SourceLocation>,
    pub impl_id: String,
    pub items: Vec<ImplItem>,
    pub is_blanket: bool,
    pub is_synthetic: bool,
}

/// Implementation item (methods, associated types, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplItem {
    pub name: String,
    pub kind: String,
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub source_location: Option<SourceLocation>,
}

/// Source code snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSnippet {
    pub code: String,
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub context_lines: u32,
    pub highlighted_line: Option<u32>,
    pub language: String,
}

/// Symbol search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSearchResult {
    pub path: String,
    pub kind: String,
    pub score: f32,
    pub doc_summary: Option<String>,
    pub source_location: Option<SourceLocation>,
    pub visibility: String,
    pub signature: Option<String>,
    pub module_path: String,
}

/// Rustdoc build configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustdocConfig {
    pub target_dir: String,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub target: Option<String>,
    pub toolchain: String,
    pub timeout_seconds: u64,
}

impl Default for RustdocConfig {
    fn default() -> Self {
        Self {
            target_dir: "target/doc".to_string(),
            features: Vec::new(),
            all_features: true,
            no_default_features: false,
            target: None,
            toolchain: "nightly".to_string(),
            timeout_seconds: 300, // 5 minutes
        }
    }
}

/// Cache entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub created_at: u64,
    pub last_accessed: u64,
    pub size_bytes: u64,
    pub version: String,
    pub checksum: String,
}

/// Error types for the documentation engine
#[derive(Debug, thiserror::Error)]
pub enum DocEngineError {
    #[error("Crate not found: {0}")]
    CrateNotFound(String),

    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),

    #[error("Failed to download crate: {0}")]
    DownloadError(#[from] reqwest::Error),

    #[error("Failed to build rustdoc: {0}")]
    RustdocBuildError(String),

    #[error("Failed to parse rustdoc JSON: {0}")]
    RustdocParseError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Index error: {0}")]
    IndexError(String),

    #[error("Invalid item path: {0}")]
    InvalidItemPath(String),

    #[error("Item not found: {0}")]
    ItemNotFound(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Result type for the documentation engine
pub type DocEngineResult<T> = Result<T, DocEngineError>;

/// Build status for rustdoc generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildStatus {
    NotStarted,
    InProgress,
    Completed,
    Failed(String),
}

/// Build metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetrics {
    pub crate_name: String,
    pub version: String,
    pub build_time_ms: u64,
    pub index_time_ms: u64,
    pub total_items: usize,
    pub memory_usage_mb: u64,
    pub cache_hit: bool,
    pub status: BuildStatus,
}

/// Search options for symbol search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub kinds: Option<Vec<String>>,
    pub limit: usize,
    pub offset: usize,
    pub include_private: bool,
    pub include_docs: bool,
    pub fuzzy_matching: bool,
    pub min_score: f32,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            kinds: None,
            limit: 20,
            offset: 0,
            include_private: false,
            include_docs: true,
            fuzzy_matching: true,
            min_score: 0.1,
        }
    }
}

/// Statistics about a crate's documentation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrateStats {
    pub name: String,
    pub version: String,
    pub total_items: usize,
    pub public_items: usize,
    pub private_items: usize,
    pub modules: usize,
    pub structs: usize,
    pub enums: usize,
    pub traits: usize,
    pub functions: usize,
    pub constants: usize,
    pub type_aliases: usize,
    pub macros: usize,
    pub implementations: usize,
    pub documented_items: usize,
    pub undocumented_items: usize,
    pub documentation_coverage: f32,
}

/// Feature flag information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureInfo {
    pub name: String,
    pub description: Option<String>,
    pub default: bool,
    pub dependencies: Vec<String>,
    pub enables: Vec<String>,
}

/// Module information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub path: String,
    pub name: String,
    pub doc: Option<String>,
    pub items: Vec<String>,
    pub submodules: Vec<String>,
    pub visibility: String,
    pub attributes: Vec<String>,
}
