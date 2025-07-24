//! Doc Engine - Rust crate documentation fetching and processing
//!
//! This crate provides functionality to fetch Rust crate documentation,
//! build rustdoc JSON, and provide a high-level API for querying documentation.
//! It also supports fetching source code from local environments for Python and Node.js.

use anyhow::{Context, Result};
use index_core::{IndexCore, SymbolIndex, TraitImplIndex};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::fs;
use tracing::info;

use crate::processors::traits::{ImplementationContext, LanguageProcessor};

pub mod cache;
pub mod fetcher;
pub mod finder;
pub mod processors;
pub mod scraper;
pub mod types;

pub use types::*;

/// Helper function to convert between source location types
fn convert_source_location(sl: Option<index_core::SourceLocation>) -> Option<SourceLocation> {
    sl.map(|s| SourceLocation {
        file: s.file,
        line: s.line,
        column: s.column,
        end_line: s.end_line,
        end_column: s.end_column,
    })
}

/// Main documentation engine that coordinates fetching, caching, and indexing
#[derive(Debug, Clone)]
pub struct DocEngine {
    fetcher: Arc<fetcher::Fetcher>,
    cache: Arc<cache::Cache>,
    index: Arc<IndexCore>,
    memory_cache: Arc<Mutex<LruCache<String, Arc<CrateDocumentation>>>>,
    python_processor: Arc<processors::python::PythonProcessor>,
    node_processor: Arc<processors::node::NodeProcessor>,
}

impl DocEngine {
    /// Create a new documentation engine
    pub async fn new(cache_dir: impl AsRef<Path>) -> Result<Self> {
        let cache_dir = cache_dir.as_ref();
        fs::create_dir_all(cache_dir)
            .await
            .context("Failed to create cache directory")?;

        let fetcher = Arc::new(fetcher::Fetcher::new());
        let cache = Arc::new(cache::Cache::new(cache_dir)?);
        let index = Arc::new(IndexCore::new(cache_dir.join("index"))?);
        let memory_cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())));
        let python_processor = Arc::new(processors::python::PythonProcessor);
        let node_processor = Arc::new(processors::node::NodeProcessor);

        Ok(Self {
            fetcher,
            cache,
            index,
            memory_cache,
            python_processor,
            node_processor,
        })
    }

    /// Search for crates on crates.io
    pub async fn search_crates(&self, query: &str, limit: u32) -> Result<Vec<CrateSearchResult>> {
        self.fetcher.search_crates(query, limit).await
    }

    /// Get detailed information about a crate
    pub async fn crate_info(&self, name: &str) -> Result<CrateInfo> {
        self.fetcher.crate_info(name).await
    }

    /// Get documentation for a specific item
    pub async fn get_item_doc(
        &self,
        crate_name: &str,
        path: &str,
        version: Option<&str>,
    ) -> Result<ItemDoc> {
        // Check item cache first
        let version_str = if let Some(v) = version {
            v.to_string()
        } else {
            let crate_info = self.fetcher.crate_info(crate_name).await?;
            crate_info.latest_version
        };

        if let Some(cached_item) = self.cache.get_item_doc(crate_name, &version_str, path)? {
            return Ok(cached_item);
        }

        // Fetch from docs.rs using scraper
        let scraper = scraper::DocsRsScraper::new();
        let item_doc = scraper
            .fetch_item_doc(crate_name, &version_str, path)
            .await?;

        // Cache the result
        self.cache
            .store_item_doc(crate_name, &version_str, path, &item_doc)?;

        Ok(item_doc)
    }

    /// List all implementations of a trait
    pub async fn list_trait_impls(
        &self,
        crate_name: &str,
        trait_path: &str,
        version: Option<&str>,
    ) -> Result<Vec<TraitImpl>> {
        let docs = self.ensure_crate_docs(crate_name, version).await?;
        docs.list_trait_impls(trait_path)
    }

    /// List all trait implementations for a type
    pub async fn list_impls_for_type(
        &self,
        crate_name: &str,
        type_path: &str,
        version: Option<&str>,
    ) -> Result<Vec<TypeImpl>> {
        let docs = self.ensure_crate_docs(crate_name, version).await?;
        docs.list_impls_for_type(type_path)
    }

    /// Get source code snippet for an item
    pub async fn source_snippet(
        &self,
        crate_name: &str,
        item_path: &str,
        context_lines: u32,
        version: Option<&str>,
    ) -> Result<SourceSnippet> {
        let docs = self.ensure_crate_docs(crate_name, version).await?;
        docs.source_snippet(item_path, context_lines).await
    }

    /// Search for symbols within a crate
    pub async fn search_symbols(
        &self,
        crate_name: &str,
        query: &str,
        kinds: Option<&[String]>,
        limit: u32,
        version: Option<&str>,
    ) -> Result<Vec<SymbolSearchResult>> {
        let docs = self.ensure_crate_docs(crate_name, version).await?;
        docs.search_symbols(query, kinds, limit)
    }

    /// Get implementation context from a local environment for various languages.
    pub async fn get_implementation_context(
        &self,
        language: &str,
        package_name: &str,
        item_path: &str, // Format: "path/to/file.py#my_function"
        context_path: Option<&str>,
    ) -> Result<ImplementationContext> {
        let (relative_path, item_name) = item_path
            .split_once('#')
            .context("Invalid item_path format. Expected 'path/to/file#item_name'")?;

        let default_context_path = PathBuf::from(".");
        let context_path = context_path
            .map(PathBuf::from)
            .unwrap_or(default_context_path);

        match language {
            "python" => {
                self.python_processor
                    .get_implementation_context(
                        package_name,
                        &context_path,
                        relative_path,
                        item_name,
                    )
                    .await
            }
            "node" => {
                self.node_processor
                    .get_implementation_context(
                        package_name,
                        &context_path,
                        relative_path,
                        item_name,
                    )
                    .await
            }
            "rust" => {
                // Rust logic is crate-based, not easily adaptable to this model.
                // Use the existing rust-specific tools instead.
                anyhow::bail!(
                    "For Rust, please use crate-specific tools like `get_item_doc` and `source_snippet`."
                )
            }
            _ => anyhow::bail!(
                "Unsupported language: '{}'. Supported languages are 'python' and 'node'.",
                language
            ),
        }
    }

    /// Ensure crate documentation is available and indexed
    async fn ensure_crate_docs(
        &self,
        crate_name: &str,
        version: Option<&str>,
    ) -> Result<Arc<CrateDocumentation>> {
        let cache_key = format!("{}@{}", crate_name, version.unwrap_or("latest"));

        // Check memory cache first
        {
            let mut cache = self.memory_cache.lock().unwrap();
            if let Some(docs) = cache.get(&cache_key) {
                return Ok(Arc::clone(docs));
            }
        }

        // Get crate information
        let crate_info = self.fetcher.crate_info(crate_name).await?;
        let target_version = if let Some(v) = version {
            v.to_string()
        } else {
            crate_info.latest_version
        };

        let _cache_key_versioned = format!("{crate_name}@{target_version}");

        // Check if we have cached search index data
        if let Some(search_data) = self.cache.get_crate_index(crate_name, &target_version)? {
            let docs = CrateDocumentation::new_from_search_index(search_data, &self.index).await?;
            let docs = Arc::new(docs);

            // Update memory cache
            {
                let mut cache = self.memory_cache.lock().unwrap();
                cache.put(cache_key, Arc::clone(&docs));
            }

            return Ok(docs);
        }

        // Need to fetch search index from docs.rs
        info!(
            "Fetching search index for {}@{}",
            crate_name, target_version
        );

        let search_data = self.fetch_search_index(crate_name, &target_version).await?;
        let docs =
            CrateDocumentation::new_from_search_index(search_data.clone(), &self.index).await?;

        // Cache the search index data
        self.cache
            .store_crate_index(crate_name, &target_version, &search_data)?;

        let docs = Arc::new(docs);

        // Update memory cache
        {
            let mut cache = self.memory_cache.lock().unwrap();
            cache.put(cache_key, Arc::clone(&docs));
        }

        Ok(docs)
    }

    /// Clear all cache entries
    pub async fn clear_all_cache(&self) -> Result<CacheOperationResult> {
        self.cache.clear_all()
    }

    /// Clear cache entries for a specific crate
    pub async fn clear_crate_cache(&self, crate_name: &str) -> Result<CacheOperationResult> {
        self.cache.clear_crate(crate_name)
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> Result<CacheStatistics> {
        self.cache.get_enhanced_stats()
    }

    /// Cleanup expired cache entries
    pub async fn cleanup_expired_cache(&self) -> Result<CacheOperationResult> {
        self.cache.cleanup_expired_entries()
    }

    /// Fetch search index from docs.rs
    async fn fetch_search_index(&self, crate_name: &str, version: &str) -> Result<SearchIndexData> {
        let scraper = scraper::DocsRsScraper::new();
        scraper.fetch_search_index(crate_name, version).await
    }
}

/// Documentation for a specific crate
#[derive(Debug, Serialize, Deserialize)]
pub struct CrateDocumentation {
    crate_name: String,
    version: String,
    search_index_data: SearchIndexData,
    #[serde(skip)]
    trait_impl_index: TraitImplIndex,
    #[serde(skip)]
    symbol_index: Option<SymbolIndex>,
}

impl CrateDocumentation {
    /// Create new crate documentation from search index data
    pub async fn new_from_search_index(
        search_index_data: SearchIndexData,
        index_core: &IndexCore,
    ) -> Result<Self> {
        // Convert to index_core types
        let index_core_search_data = index_core::traits::SearchIndexData {
            crate_name: search_index_data.crate_name.clone(),
            version: search_index_data.version.clone(),
            items: search_index_data
                .items
                .iter()
                .map(|item| index_core::traits::SearchIndexItem {
                    name: item.name.clone(),
                    kind: item.kind.clone(),
                    path: item.path.clone(),
                    description: item.description.clone(),
                    parent_index: item.parent_index,
                })
                .collect(),
            paths: search_index_data.paths.clone(),
        };

        // Build indexes from search data
        let trait_impl_index = TraitImplIndex::from_search_index(&index_core_search_data)?;
        let symbol_index =
            Some(SymbolIndex::from_search_index(&index_core_search_data, index_core).await?);

        Ok(Self {
            crate_name: search_index_data.crate_name.clone(),
            version: search_index_data.version.clone(),
            search_index_data,
            trait_impl_index,
            symbol_index,
        })
    }

    /// Get documentation for a specific item (now uses on-demand scraping)
    pub async fn get_item_doc(&self, path: &str) -> Result<ItemDoc> {
        // Use scraper to fetch item documentation on-demand
        let scraper = scraper::DocsRsScraper::new();
        scraper
            .fetch_item_doc(&self.crate_name, &self.version, path)
            .await
    }

    /// List all implementations of a trait
    pub fn list_trait_impls(&self, trait_path: &str) -> Result<Vec<TraitImpl>> {
        let impls = self.trait_impl_index.get_trait_impls(trait_path)?;
        // Convert from index_core types to doc_engine types
        Ok(impls
            .into_iter()
            .map(|impl_data| TraitImpl {
                for_type: impl_data.for_type,
                trait_path: impl_data.trait_path,
                generics: impl_data.generics,
                where_clause: impl_data.where_clause,
                source_span: convert_source_location(impl_data.source_span),
                impl_id: impl_data.impl_id,
                items: impl_data
                    .items
                    .into_iter()
                    .map(|item| ImplItem {
                        name: item.name,
                        kind: item.kind,
                        signature: item.signature,
                        doc: item.doc,
                        source_location: convert_source_location(item.source_location),
                    })
                    .collect(),
                is_blanket: impl_data.is_blanket,
                is_synthetic: impl_data.is_synthetic,
            })
            .collect())
    }

    /// List all trait implementations for a type
    pub fn list_impls_for_type(&self, type_path: &str) -> Result<Vec<TypeImpl>> {
        let impls = self.trait_impl_index.get_type_impls(type_path)?;
        // Convert from index_core types to doc_engine types
        Ok(impls
            .into_iter()
            .map(|impl_data| TypeImpl {
                trait_path: impl_data.trait_path,
                generics: impl_data.generics,
                where_clause: impl_data.where_clause,
                source_span: convert_source_location(impl_data.source_span),
                impl_id: impl_data.impl_id,
                items: impl_data
                    .items
                    .into_iter()
                    .map(|item| ImplItem {
                        name: item.name,
                        kind: item.kind,
                        signature: item.signature,
                        doc: item.doc,
                        source_location: convert_source_location(item.source_location),
                    })
                    .collect(),
                is_blanket: impl_data.is_blanket,
                is_synthetic: impl_data.is_synthetic,
            })
            .collect())
    }

    /// Get source code snippet for an item
    pub async fn source_snippet(
        &self,
        item_path: &str,
        context_lines: u32,
    ) -> Result<SourceSnippet> {
        // Find the item in the search index
        if let Some(item) = self.find_item_by_path(item_path)? {
            // For now, we return a placeholder since source code access
            // requires additional implementation beyond docs.rs scraping
            Ok(SourceSnippet {
                code: format!(
                    "// Source code for {item_path}\n// (Source code viewing not yet implemented for docs.rs scraping)"
                ),
                file: format!("{}.rs", item.name),
                line_start: 1,
                line_end: context_lines,
                context_lines,
                highlighted_line: Some(1),
                language: "rust".to_string(),
            })
        } else {
            Err(anyhow::anyhow!("Item not found: {}", item_path))
        }
    }

    /// Search for symbols within the crate
    pub fn search_symbols(
        &self,
        query: &str,
        kinds: Option<&[String]>,
        limit: u32,
    ) -> Result<Vec<SymbolSearchResult>> {
        if let Some(symbol_index) = &self.symbol_index {
            let results = symbol_index.search(query, kinds, limit as usize)?;
            // Convert from index_core types to doc_engine types
            Ok(results
                .into_iter()
                .map(|result| SymbolSearchResult {
                    path: result.path,
                    kind: result.kind,
                    score: result.score,
                    doc_summary: result.doc_summary,
                    source_location: convert_source_location(result.source_location),
                    visibility: result.visibility,
                    signature: result.signature,
                    module_path: result.module_path,
                })
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find an item by its path in search index data
    fn find_item_by_path(&self, path: &str) -> Result<Option<SearchIndexItem>> {
        for item in &self.search_index_data.items {
            if item.path == path || item.path.ends_with(&format!("::{}", item.name)) {
                return Ok(Some(item.clone()));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_doc_engine_creation() {
        let temp_dir = tempdir().unwrap();
        let engine = DocEngine::new(temp_dir.path()).await;
        assert!(engine.is_ok());
    }

    #[tokio::test]
    async fn test_search_crates() {
        let temp_dir = tempdir().unwrap();
        let engine = DocEngine::new(temp_dir.path()).await.unwrap();

        // This test requires network access
        if std::env::var("ENABLE_NETWORK_TESTS").is_ok() {
            let results = engine.search_crates("serde", 5).await.unwrap();
            assert!(!results.is_empty());
            assert!(results.iter().any(|r| r.name == "serde"));
        }
    }

    #[tokio::test]
    async fn test_crate_documentation_from_search_index() {
        let temp_dir = tempdir().unwrap();
        let index_core = IndexCore::new(temp_dir.path()).unwrap();

        let search_data = SearchIndexData {
            crate_name: "test_crate".to_string(),
            version: "1.0.0".to_string(),
            items: vec![SearchIndexItem {
                name: "TestStruct".to_string(),
                kind: "struct".to_string(),
                path: "test_crate::TestStruct".to_string(),
                description: "A test struct".to_string(),
                parent_index: None,
            }],
            paths: vec!["test_crate".to_string()],
        };

        let docs = CrateDocumentation::new_from_search_index(search_data, &index_core).await;
        assert!(docs.is_ok());

        let docs = docs.unwrap();
        assert_eq!(docs.crate_name, "test_crate");
        assert_eq!(docs.version, "1.0.0");
    }
}
