//! Type definitions for the index core

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Search result for symbol search
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

/// Item documentation
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

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_items: usize,
    pub indexed_items: usize,
    pub search_index_size: u64,
    pub trait_implementations: usize,
    pub unique_traits: usize,
    pub unique_types: usize,
    pub last_updated: Option<String>,
}

/// Crate statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Error types for index operations
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("Index not found: {0}")]
    IndexNotFound(String),

    #[error("Search error: {0}")]
    SearchError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Query parse error: {0}")]
    QueryParseError(String),

    #[error("Index corruption detected: {0}")]
    IndexCorruption(String),

    #[error("Schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: String, found: String },

    #[error("Invalid field access: {0}")]
    InvalidField(String),

    #[error("Index build failed: {0}")]
    BuildError(String),
}

/// Result type for index operations
pub type IndexResult<T> = Result<T, IndexError>;

/// Search options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub kinds: Option<Vec<String>>,
    pub limit: usize,
    pub offset: usize,
    pub include_private: bool,
    pub include_docs: bool,
    pub fuzzy_matching: bool,
    pub min_score: f32,
    pub highlight_matches: bool,
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
            highlight_matches: false,
        }
    }
}

/// Index configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    pub heap_size_mb: usize,
    pub commit_interval_seconds: u64,
    pub max_docs_per_segment: usize,
    pub enable_fast_fields: bool,
    pub compression_level: u8,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            heap_size_mb: 50,
            commit_interval_seconds: 30,
            max_docs_per_segment: 10_000,
            enable_fast_fields: true,
            compression_level: 3,
        }
    }
}

/// Validation result for indexed data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub stats: ValidationStats,
}

/// Validation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationStats {
    pub total_checked: usize,
    pub valid_items: usize,
    pub invalid_items: usize,
    pub missing_docs: usize,
    pub broken_links: usize,
}

/// Build progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgress {
    pub phase: BuildPhase,
    pub items_processed: usize,
    pub total_items: usize,
    pub elapsed_seconds: u64,
    pub estimated_remaining_seconds: Option<u64>,
}

/// Build phases
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BuildPhase {
    Starting,
    ParsingRustdoc,
    BuildingTraitIndex,
    BuildingSearchIndex,
    Finalizing,
    Complete,
    Failed(String),
}

impl BuildPhase {
    pub fn is_complete(&self) -> bool {
        matches!(self, BuildPhase::Complete)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, BuildPhase::Failed(_))
    }
}

/// Memory usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub heap_used_mb: f64,
    pub heap_total_mb: f64,
    pub index_size_mb: f64,
    pub cache_size_mb: f64,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub search_time_ms: u64,
    pub index_time_ms: u64,
    pub total_searches: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub average_search_time_ms: f64,
}

/// Index health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: HealthLevel,
    pub issues: Vec<HealthIssue>,
    pub last_check: String,
    pub uptime_seconds: u64,
}

/// Health levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthLevel {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

/// Health issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthIssue {
    pub severity: HealthLevel,
    pub message: String,
    pub component: String,
    pub timestamp: String,
}

/// Configuration for trait implementation indexing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitIndexConfig {
    pub include_blanket_impls: bool,
    pub include_synthetic_impls: bool,
    pub max_depth: usize,
    pub include_private_traits: bool,
}

impl Default for TraitIndexConfig {
    fn default() -> Self {
        Self {
            include_blanket_impls: true,
            include_synthetic_impls: false,
            max_depth: 10,
            include_private_traits: false,
        }
    }
}

/// Batch operation for index updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    pub operations: Vec<IndexOperation>,
    pub commit_on_complete: bool,
}

/// Individual index operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexOperation {
    Add {
        id: String,
        document: HashMap<String, String>,
    },
    Update {
        id: String,
        document: HashMap<String, String>,
    },
    Delete {
        id: String,
    },
}

/// Query statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    pub query: String,
    pub execution_time_ms: u64,
    pub results_count: usize,
    pub filters_applied: Vec<String>,
    pub timestamp: String,
}
