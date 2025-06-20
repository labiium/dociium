//! Doc Engine - Rust crate documentation fetching and processing
//!
//! This crate provides functionality to fetch Rust crate documentation,
//! build rustdoc JSON, and provide a high-level API for querying documentation.

use anyhow::{Context, Result};
use index_core::{IndexCore, SymbolIndex, TraitImplIndex};
use lru::LruCache;
use rustdoc_types::{Crate as RustdocCrate, Id};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    num::NonZeroUsize,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::fs;
use tracing::info;

pub mod cache;
pub mod fetcher;
pub mod rustdoc;
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

        Ok(Self {
            fetcher,
            cache,
            index,
            memory_cache,
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

    /// Resolve a user supplied version specification into a concrete semver [`Version`].
    ///
    /// The following forms are accepted:
    ///  * `"latest"`, `"newest"`, `"current"`, `"*"`, empty string or `None` – use the crate's
    ///    latest published version.
    ///  * A version prefixed with `v` such as `"v1.2.3"`.
    ///  * A short two component form like `"1.2"` which is interpreted as `"1.2.0"`.
    ///  * A full semver string `"1.2.3"`.
    fn resolve_version(
        &self,
        crate_info: &CrateInfo,
        version_spec: Option<&str>,
    ) -> Result<Version> {
        let spec = version_spec.unwrap_or("").trim();

        if spec.is_empty() || matches!(spec, "latest" | "newest" | "current" | "*") {
            return Version::parse(&crate_info.latest_version).context("Invalid latest version");
        }

        let spec = spec.strip_prefix('v').unwrap_or(spec);
        let normalized = if spec.matches('.').count() == 1 {
            format!("{}.0", spec)
        } else {
            spec.to_string()
        };

        Version::parse(&normalized)
            .with_context(|| format!("Invalid version format: {}", version_spec.unwrap_or("")))
    }

    /// Get documentation for a specific item
    pub async fn get_item_doc(
        &self,
        crate_name: &str,
        path: &str,
        version: Option<&str>,
    ) -> Result<ItemDoc> {
        let docs = self.ensure_crate_docs(crate_name, version).await?;
        docs.get_item_doc(path)
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

    /// Ensure crate documentation is available and indexed.
    ///
    /// The `version` parameter accepts convenient specifiers as understood by
    /// [`resolve_version`](Self::resolve_version).
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
        let target_version = self.resolve_version(&crate_info, version)?;

        let cache_key_versioned = format!("{}@{}", crate_name, target_version);

        // Check if we have cached documentation
        if let Some(cached_docs) = self.cache.get_crate_docs(&cache_key_versioned)? {
            let docs = Arc::new(cached_docs);

            // Update memory cache
            {
                let mut cache = self.memory_cache.lock().unwrap();
                cache.put(cache_key, Arc::clone(&docs));
            }

            return Ok(docs);
        }

        // Need to build documentation
        info!(
            "Building documentation for {}@{}",
            crate_name, target_version
        );

        let rustdoc_crate = self.build_rustdoc_json(crate_name, &target_version).await?;
        let docs = CrateDocumentation::new(rustdoc_crate, &self.index).await?;

        // Cache the documentation
        self.cache.store_crate_docs(&cache_key_versioned, &docs)?;

        let docs = Arc::new(docs);

        // Update memory cache
        {
            let mut cache = self.memory_cache.lock().unwrap();
            cache.put(cache_key, Arc::clone(&docs));
        }

        Ok(docs)
    }

    /// Build rustdoc JSON for a crate
    async fn build_rustdoc_json(
        &self,
        crate_name: &str,
        version: &Version,
    ) -> Result<RustdocCrate> {
        // Download and extract crate
        let temp_dir = self.fetcher.download_crate(crate_name, version).await?;

        // Build rustdoc JSON
        let rustdoc_builder = rustdoc::RustdocBuilder::new(temp_dir.path());
        rustdoc_builder.build_json().await
    }
}

/// Documentation for a specific crate
#[derive(Debug, Serialize, Deserialize)]
pub struct CrateDocumentation {
    rustdoc_crate: RustdocCrate,
    #[serde(skip)]
    trait_impl_index: TraitImplIndex,
    #[serde(skip)]
    symbol_index: Option<SymbolIndex>,
}

impl CrateDocumentation {
    /// Create new crate documentation
    pub async fn new(rustdoc_crate: RustdocCrate, index_core: &IndexCore) -> Result<Self> {
        // Build indexes
        let trait_impl_index = TraitImplIndex::from_rustdoc(&rustdoc_crate)?;
        let symbol_index = Some(SymbolIndex::from_rustdoc(&rustdoc_crate, index_core).await?);

        Ok(Self {
            rustdoc_crate,
            trait_impl_index,
            symbol_index,
        })
    }

    /// Get documentation for a specific item
    pub fn get_item_doc(&self, path: &str) -> Result<ItemDoc> {
        // Find the item by path
        let item_id = self.find_item_by_path(path)?;
        let item = self
            .rustdoc_crate
            .index
            .get(&item_id)
            .ok_or_else(|| anyhow::anyhow!("Item not found in index"))?;

        // Convert to ItemDoc
        Ok(ItemDoc {
            path: path.to_string(),
            kind: format!("{:?}", item.inner),
            rendered_markdown: item
                .docs
                .as_ref()
                .map(|d| d.clone())
                .unwrap_or_else(|| "No documentation available".to_string()),
            source_location: item.span.as_ref().map(|span| SourceLocation {
                file: span.filename.to_string_lossy().to_string(),
                line: span.begin.0 as u32,
                column: span.begin.1 as u32,
                end_line: Some(span.end.0 as u32),
                end_column: Some(span.end.1 as u32),
            }),
            visibility: format!("{:?}", item.visibility),
            attributes: Vec::new(), // TODO: Extract from rustdoc data
            signature: None,        // TODO: Extract from rustdoc data
            examples: Vec::new(),   // TODO: Extract from rustdoc data
            see_also: Vec::new(),   // TODO: Extract from rustdoc data
        })
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
        let item_id = self.find_item_by_path(item_path)?;
        let item = self
            .rustdoc_crate
            .index
            .get(&item_id)
            .ok_or_else(|| anyhow::anyhow!("Item not found in index"))?;

        if let Some(span) = &item.span {
            // This is a simplified implementation - in practice, you'd need to
            // access the original source files to extract the actual code
            Ok(SourceSnippet {
                code: format!(
                    "// Source code for {}\n// Located at {}:{}:{}",
                    item_path,
                    span.filename.display(),
                    span.begin.0,
                    span.begin.1
                ),
                file: span.filename.to_string_lossy().to_string(),
                line_start: (span.begin.0 as u32).saturating_sub(context_lines),
                line_end: (span.end.0 as u32) + context_lines,
                context_lines,
                highlighted_line: Some(span.begin.0 as u32),
                language: "rust".to_string(),
            })
        } else {
            Err(anyhow::anyhow!("No source location available for item"))
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

    /// Find an item by its path
    fn find_item_by_path(&self, path: &str) -> Result<Id> {
        // This is a simplified implementation - in practice, you'd need to
        // traverse the module structure to resolve the path
        for (id, item) in &self.rustdoc_crate.index {
            if let Some(name) = &item.name {
                if name == path || path.ends_with(&format!("::{}", name)) {
                    return Ok(id.clone());
                }
            }
        }

        Err(anyhow::anyhow!("Item not found: {}", path))
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
        let results = engine.search_crates("serde", 5).await.unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.name == "serde"));
    }

    #[tokio::test]
    async fn test_resolve_version_variants() {
        let temp_dir = tempdir().unwrap();
        let engine = DocEngine::new(temp_dir.path()).await.unwrap();

        let ci = CrateInfo {
            name: "dummy".into(),
            latest_version: "1.2.3".into(),
            description: None,
            homepage: None,
            repository: None,
            documentation: None,
            license: None,
            downloads: 0,
            recent_downloads: None,
            feature_flags: Vec::new(),
            dependencies: Vec::new(),
            keywords: Vec::new(),
            categories: Vec::new(),
            versions: Vec::new(),
            authors: Vec::new(),
            created_at: None,
            updated_at: None,
        };

        let cases = vec![
            (None, "1.2.3"),
            (Some(""), "1.2.3"),
            (Some("latest"), "1.2.3"),
            (Some("newest"), "1.2.3"),
            (Some("current"), "1.2.3"),
            (Some("*"), "1.2.3"),
            (Some("v1.2.3"), "1.2.3"),
            (Some("1.2"), "1.2.0"),
            (Some("1.2.3"), "1.2.3"),
        ];

        for (spec, expected) in cases {
            let resolved = engine.resolve_version(&ci, spec).unwrap();
            assert_eq!(resolved, Version::parse(expected).unwrap());
        }
    }

    #[tokio::test]
    async fn test_real_documentation_fetch() {

        let temp_dir = tempdir().unwrap();
        let engine = DocEngine::new(temp_dir.path()).await.unwrap();

        let item = engine
            .get_item_doc("itoa", "itoa::Buffer::from", Some("1.0.0"))
            .await
            .unwrap();

        assert!(
            item.rendered_markdown
                .contains("Converts a value to a string slice"),
            "Expected sentence is missing"
        );
    }
}
