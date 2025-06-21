//! Type definitions for the index core

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Search result for symbol search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Const,
    Macro,
    TypeAlias,
    Module,
    Unknown, // Added for completeness
}

impl SymbolKind {
    pub fn from_item_enum(item_enum: &rustdoc_types::ItemEnum) -> Self {
        match item_enum {
            rustdoc_types::ItemEnum::Function(_) => SymbolKind::Function,
            rustdoc_types::ItemEnum::Struct(_) => SymbolKind::Struct,
            rustdoc_types::ItemEnum::Enum(_) => SymbolKind::Enum,
            rustdoc_types::ItemEnum::Trait(_) => SymbolKind::Trait,
            rustdoc_types::ItemEnum::Constant { .. } => SymbolKind::Const,
            rustdoc_types::ItemEnum::Macro(_) => SymbolKind::Macro,
            rustdoc_types::ItemEnum::TypeAlias(_) => SymbolKind::TypeAlias,
            rustdoc_types::ItemEnum::Module(_) => SymbolKind::Module,
            _ => SymbolKind::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "Function",
            SymbolKind::Struct => "Struct",
            SymbolKind::Enum => "Enum",
            SymbolKind::Trait => "Trait",
            SymbolKind::Const => "Const",
            SymbolKind::Macro => "Macro",
            SymbolKind::TypeAlias => "TypeAlias",
            SymbolKind::Module => "Module",
            SymbolKind::Unknown => "Unknown",
        }
    }
}

impl From<&str> for SymbolKind {
    fn from(s: &str) -> Self {
        match s {
            "Function" => SymbolKind::Function,
            "Struct" => SymbolKind::Struct,
            "Enum" => SymbolKind::Enum,
            "Trait" => SymbolKind::Trait,
            "Const" => SymbolKind::Const,
            "Macro" => SymbolKind::Macro,
            "TypeAlias" => SymbolKind::TypeAlias,
            "Module" => SymbolKind::Module,
            _ => SymbolKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSearchResult {
    pub path: String,
    pub kind: SymbolKind,
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

/// Specifies the type of search query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryType {
    Exact,
    Prefix,
    Fuzzy, // Standard Levenshtein distance based fuzzy search
    Term,  // General term matching, can include wildcards if supported by parser
}

/// Configuration for how search results should be scored and boosted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Boost factor for exact matches on the item name.
    pub name_exact_match_boost: f32,
    /// Boost factor for prefix matches on the item name.
    pub name_prefix_match_boost: f32,
    /// Boost factor for matches in the item's path.
    pub path_match_boost: f32,
    /// Boost factor for matches in documentation.
    pub doc_match_boost: f32,
    /// General boost for item kind (e.g., boost Functions more than Modules).
    pub kind_boost: HashMap<SymbolKind, f32>,
    /// Custom per-field boosts. Key is field name, value is boost factor.
    pub field_boosts: HashMap<String, f32>,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            name_exact_match_boost: 3.0,
            name_prefix_match_boost: 1.5,
            path_match_boost: 1.2,
            doc_match_boost: 1.0,
            kind_boost: HashMap::new(), // No specific kind boosts by default
            field_boosts: HashMap::new(),
        }
    }
}

/// Search options to control how a symbol search is performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSearchOptions {
    /// The main search query string.
    pub query: String,
    /// Type of query to perform (Exact, Prefix, Fuzzy, Term).
    pub query_type: QueryType,
    /// For Fuzzy queries, the Levenshtein distance. Defaults to 1 or 2.
    pub fuzzy_distance: Option<u8>,
    /// Maximum number of results to return.
    pub limit: usize,
    /// Offset for pagination.
    pub offset: usize,
    /// Filter results by specific kinds of symbols.
    pub kinds: Option<Vec<SymbolKind>>,
    /// Filter results by module path (exact match or prefix).
    pub module_path_filter: Option<String>,
    /// Filter results by visibility (e.g., "public").
    pub visibility_filter: Option<Vec<String>>, // e.g. ["public", "documented"]
    /// Whether to include items marked as deprecated.
    pub include_deprecated: bool,
    /// Whether to search only items that have documentation.
    pub must_have_docs: bool,
    /// Configuration for scoring and boosting results.
    pub scoring_config: ScoringConfig,
    /// If true, Tantivy will try to provide highlighted snippets of matched terms.
    /// This requires the fields to be STORED.
    pub highlight_matches: bool,
    // TODO: Add options for crate_ids if searching across multiple crates in one index (not current model)
}

impl Default for SymbolSearchOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            query_type: QueryType::Term, // Default to general term matching
            fuzzy_distance: Some(1),
            limit: 20,
            offset: 0,
            kinds: None,
            module_path_filter: None,
            visibility_filter: None,
            include_deprecated: false, // Usually, users don't want deprecated items by default
            must_have_docs: false,
            scoring_config: ScoringConfig::default(),
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
