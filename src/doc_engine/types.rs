//! Type definitions for the documentation engine

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

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

/// Scraper configuration for docs.rs fetching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScraperConfig {
    pub base_url: String,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub user_agent: String,
}

impl Default for ScraperConfig {
    fn default() -> Self {
        Self {
            base_url: "https://docs.rs".to_string(),
            timeout_seconds: 30,
            max_retries: 3,
            retry_delay_ms: 500,
            user_agent: "dociium-scraper/1.0".to_string(),
        }
    }
}

/// Cache entry metadata for tracking cached items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub created_at: u64,
    pub last_accessed: u64,
    pub size_bytes: u64,
    pub version: String,
    pub checksum: String,
    pub category: String,
    pub metadata: HashMap<String, String>,
}

/// Item-level cache entry for individual documentation items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemCacheEntry {
    pub item_doc: ItemDoc,
    pub cached_at: SystemTime,
    pub version: String,
    pub etag: Option<String>,
}

/// Crate-level cache entry for search indexes and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateCacheEntry {
    pub crate_name: String,
    pub version: String,
    pub search_index_data: Option<SearchIndexData>,
    pub cached_at: SystemTime,
    pub last_verified: SystemTime,
}

/// Search index data from docs.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexData {
    pub crate_name: String,
    pub version: String,
    pub items: Vec<SearchIndexItem>,
    pub paths: Vec<String>,
}

/// Individual item in the search index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexItem {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub description: String,
    pub parent_index: Option<usize>,
}

/// Cache configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub max_memory_entries: usize,
    pub max_disk_size_mb: u64,
    pub cleanup_interval_hours: u64,
    pub entry_ttl_hours: u64,
    pub enable_compression: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_memory_entries: 1000,
            max_disk_size_mb: 1000, // 1GB
            cleanup_interval_hours: 24,
            entry_ttl_hours: 168, // 1 week
            enable_compression: true,
        }
    }
}

/// Error types for the documentation engine
#[derive(Debug, thiserror::Error)]
pub enum DocEngineError {
    #[error("Crate not found: {0}")]
    CrateNotFound(String),

    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),

    #[error("Failed to fetch from docs.rs: {0}")]
    DocsRsFetchError(#[from] reqwest::Error),

    #[error("Failed to parse HTML content: {0}")]
    HtmlParseError(String),

    #[error("Failed to parse search index: {0}")]
    SearchIndexParseError(String),

    #[error("Failed to parse JSON: {0}")]
    JsonParseError(#[from] serde_json::Error),

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

    #[error("Documentation not available: {0}")]
    DocumentationNotAvailable(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Scraper error: {0}")]
    ScraperError(String),

    #[error("URL construction error: {0}")]
    UrlError(String),
}

/// Result type for the documentation engine
pub type DocEngineResult<T> = Result<T, DocEngineError>;

/// Fetch status for documentation scraping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FetchStatus {
    NotStarted,
    InProgress,
    Completed,
    Failed(String),
    Cached,
}

/// Fetch metrics for monitoring scraping performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchMetrics {
    pub crate_name: String,
    pub version: String,
    pub fetch_time_ms: u64,
    pub parse_time_ms: u64,
    pub total_items: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub network_requests: usize,
    pub status: FetchStatus,
}

/// Cache statistics and metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatistics {
    pub total_entries: usize,
    pub memory_entries: usize,
    pub disk_entries: usize,
    pub total_size_bytes: u64,
    pub memory_size_bytes: u64,
    pub disk_size_bytes: u64,
    pub hit_rate: f64,
    pub miss_rate: f64,
    pub evictions: u64,
    pub oldest_entry_age_hours: f64,
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

/// Cache management operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheOperation {
    Get,
    Put,
    Delete,
    Clear,
    Cleanup,
    Stats,
}

/// Cache management result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheOperationResult {
    pub operation: CacheOperation,
    pub success: bool,
    pub message: String,
    pub items_affected: usize,
    pub size_freed_bytes: u64,
}

/// Documentation source indicating where docs came from
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocSource {
    DocsRs { url: String, fetched_at: SystemTime },
    Cache { cached_at: SystemTime },
    Local { path: String },
}

/// Enhanced item documentation with source tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedItemDoc {
    pub item_doc: ItemDoc,
    pub source: DocSource,
    pub quality_score: f32,
    pub completeness_score: f32,
}
