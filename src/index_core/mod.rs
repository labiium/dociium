//! Index Core - Search and indexing functionality for Rust documentation
//!
//! This crate provides indexing and search capabilities for Rust documentation,
//! including full-text search and trait-implementation mapping.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub mod search;
pub mod traits;
pub mod types;

pub use search::*;
pub use traits::*;
pub use types::*;

/// Core indexing functionality
#[derive(Debug)]
pub struct IndexCore {
    index_dir: std::path::PathBuf,
    _placeholder: (),
}

impl IndexCore {
    /// Create a new index core
    pub fn new(index_dir: impl AsRef<Path>) -> Result<Self> {
        let index_dir = index_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&index_dir)?;

        Ok(Self {
            index_dir,
            _placeholder: (),
        })
    }

    /// Initialize the search index
    pub fn initialize_search_index(&mut self) -> Result<()> {
        // Placeholder implementation
        Ok(())
    }
}

impl Clone for IndexCore {
    fn clone(&self) -> Self {
        Self {
            index_dir: self.index_dir.clone(),
            _placeholder: (),
        }
    }
}

/// Symbol index for in-memory substring search over rustdoc search-index items.
/// This replaces the previous placeholder & mock result implementation with a
/// deterministic, data-backed index (O(N) scan, N = items) adequate for small/
/// medium crates. Future feature 'search-index' can swap in Tantivy.
#[derive(Debug, Clone)]
pub struct SymbolIndex {
    items: Vec<SymbolRecord>,
}

/// Internal lightweight record used for searching.
#[derive(Debug, Clone)]
struct SymbolRecord {
    name: String,
    path: String,
    kind: String,
    doc: Option<String>,
    module_path: String,
}

impl SymbolIndex {
    /// Create an empty symbol index.
    pub fn new(_index_core: IndexCore) -> Result<Self> {
        Ok(Self { items: Vec::new() })
    }

    /// Build a symbol index from docs.rs search index data.
    pub async fn from_search_index(
        search_index_data: &traits::SearchIndexData,
        index_core: &IndexCore,
    ) -> Result<Self> {
        let mut idx = Self::new(index_core.clone())?;
        idx.items.reserve(search_index_data.items.len());
        for it in &search_index_data.items {
            // Derive module path = path minus trailing ::Name if present.
            let module_path = if let Some(pos) = it.path.rfind("::") {
                it.path[..pos].to_string()
            } else {
                search_index_data.crate_name.clone()
            };
            idx.items.push(SymbolRecord {
                name: it.name.clone(),
                path: it.path.clone(),
                kind: it.kind.clone(),
                doc: Some(it.description.clone()).filter(|s| !s.is_empty()),
                module_path,
            });
        }
        Ok(idx)
    }

    /// Perform a simple case‑insensitive substring search.
    ///
    /// Scoring heuristic (higher is better):
    /// 1. Exact name match => 1.0
    /// 2. Name contains query => 0.75
    /// 3. Path contains query => 0.5
    ///    (Unmatched => excluded)
    ///
    /// Complexity: O(N) time, O(K) output (K = min(limit, matches)); O(1) extra space.
    pub fn search(
        &self,
        query: &str,
        kinds: Option<&[String]>,
        limit: usize,
    ) -> Result<Vec<SymbolSearchResult>> {
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let q = query.to_lowercase();
        let kind_filter: Option<std::collections::HashSet<&str>> = kinds.map(|ks| {
            ks.iter()
                .map(|s| s.as_str())
                .collect::<std::collections::HashSet<_>>()
        });

        let mut scored: Vec<(f32, &SymbolRecord)> = Vec::new();
        for rec in &self.items {
            if let Some(ref kf) = kind_filter {
                if !kf.contains(rec.kind.as_str()) {
                    continue;
                }
            }
            let name_l = rec.name.to_lowercase();
            let path_l = rec.path.to_lowercase();
            let score = if name_l == q {
                1.0
            } else if name_l.contains(&q) {
                0.75
            } else if path_l.contains(&q) {
                0.5
            } else {
                0.0
            };
            if score > 0.0 {
                scored.push((score, rec));
            }
        }

        // Sort by score desc then name asc for determinism
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.name.cmp(&b.1.name))
        });

        let mut out = Vec::new();
        for (score, rec) in scored.into_iter().take(limit) {
            out.push(SymbolSearchResult {
                path: rec.path.clone(),
                kind: rec.kind.clone(),
                score,
                doc_summary: rec.doc.clone(),
                source_location: None,
                visibility: "public".to_string(),
                signature: None, // rustdoc search-index.js lacks signatures
                module_path: rec.module_path.clone(),
            });
        }
        Ok(out)
    }
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

/// Search index data from docs.rs (lib version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexData {
    pub crate_name: String,
    pub version: String,
    pub items: Vec<SearchIndexItem>,
    pub paths: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_index_core_creation() {
        let temp_dir = tempdir().unwrap();
        let index_core = IndexCore::new(temp_dir.path());
        assert!(index_core.is_ok());
    }

    #[test]
    fn test_index_initialization() {
        let temp_dir = tempdir().unwrap();
        let mut index_core = IndexCore::new(temp_dir.path()).unwrap();
        let result = index_core.initialize_search_index();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_symbol_index_creation() {
        let temp_dir = tempdir().unwrap();
        let index_core = IndexCore::new(temp_dir.path()).unwrap();

        let symbol_index = SymbolIndex::new(index_core);
        assert!(symbol_index.is_ok());
    }

    #[tokio::test]
    async fn test_search_index_creation() {
        let temp_dir = tempdir().unwrap();
        let index_core = IndexCore::new(temp_dir.path()).unwrap();

        let search_data = traits::SearchIndexData {
            crate_name: "test".to_string(),
            version: "1.0.0".to_string(),
            items: vec![traits::SearchIndexItem {
                name: "test_function".to_string(),
                kind: "function".to_string(),
                path: "test::test_function".to_string(),
                description: "A test function".to_string(),
                parent_index: None,
            }],
            paths: vec!["test".to_string()],
        };

        let symbol_index = SymbolIndex::from_search_index(&search_data, &index_core).await;
        assert!(symbol_index.is_ok());
    }

    #[tokio::test]
    async fn test_symbol_search() {
        let temp_dir = tempdir().unwrap();
        let index_core = IndexCore::new(temp_dir.path()).unwrap();

        // Build a minimal search index with a single function symbol
        let search_data = traits::SearchIndexData {
            crate_name: "mycrate".to_string(),
            version: "0.1.0".to_string(),
            items: vec![traits::SearchIndexItem {
                name: "test".to_string(),
                kind: "function".to_string(),
                path: "mycrate::test".to_string(),
                description: "test function".to_string(),
                parent_index: None,
            }],
            paths: vec!["mycrate".to_string()],
        };

        let symbol_index = SymbolIndex::from_search_index(&search_data, &index_core)
            .await
            .unwrap();

        let results = symbol_index.search("test", None, 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "mycrate::test");
    }
}
