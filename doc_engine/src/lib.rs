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
    fs::File,
    io::{BufReader, Read},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    html::{styled_line_to_html, IncludeBackground},
    parsing::SyntaxSet,
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

// Removed duplicated field block

impl DocEngine {
    /// Create a new documentation engine
    pub async fn new(base_cache_dir_arg: impl AsRef<Path>) -> Result<Self> {
        let base_cache_dir = base_cache_dir_arg.as_ref().to_path_buf();
        fs::create_dir_all(&base_cache_dir) // Ensure base cache dir exists
            .await
            .context("Failed to create base cache directory")?;

        let tarballs_dir = base_cache_dir.join("tarballs");
        fs::create_dir_all(&tarballs_dir) // Ensure tarballs dir exists
            .await
            .context("Failed to create tarballs cache directory")?;

        let fetcher = Arc::new(fetcher::Fetcher::new());
        // Cache::new now expects the base_cache_dir to derive its own 'docs' subdirectory.
        let cache = Arc::new(cache::Cache::new(&base_cache_dir)?);
        let index = Arc::new(IndexCore::new(base_cache_dir.join("index"))?);

        // This LruCache is for CrateDocumentation objects.
        // The new cache::Cache has its own Moka memory_cache for CrateDocumentation.
        // We should consolidate or clarify their roles.
        // For now, let's assume this LruCache is what ensure_crate_docs uses.
        // The spec says: "In-memory: switch to moka concurrent cache (async support, TTL)" for "Cross-Cutting Concerns > Cache"
        // and cache.rs was updated. This implies this LruCache here should be removed or replaced.
        // Let's remove this one and make ensure_crate_docs use the Moka cache in self.cache.
        // So, CrateDocumentation objects will be stored in self.cache.memory_cache.
        // Therefore, DocEngine no longer needs its own `memory_cache` field for CrateDocumentation.

        Ok(Self {
            base_cache_dir,
            fetcher,
            cache,
            index,
            // memory_cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap()))),
        })
    }

    fn get_tarballs_path(&self) -> PathBuf {
        self.base_cache_dir.join("tarballs")
    }

    // Removed duplicated get_tarballs_path and extra brace

    /// Preload a crate: download its tarball and optionally build its documentation.
    pub async fn preload_crate(
        &self,
        name: &str,
        version_str: &str,
        mode: BuildMode,
    ) -> Result<()> {
        let version = Version::parse(version_str)
            .with_context(|| format!("Invalid version string for preload: {}", version_str))?;
        info!(
            "Preloading crate {}@{} in mode {:?}",
            name, version_str, mode
        );

        match mode {
            BuildMode::DownloadOnly => {
                let (_temp_dir, tarball_bytes) = self
                    .fetcher
                    .download_crate(name, &version)
                    .await
                    .with_context(|| {
                        format!("Failed to download {}@{} for preload", name, version_str)
                    })?;

                let tarball_filename = format!("{}-{}.tar.gz", name, version);
                let tarball_save_path = self.get_tarballs_path().join(&tarball_filename);
                fs::write(&tarball_save_path, &tarball_bytes)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to save crate tarball to {:?} during preload",
                            tarball_save_path
                        )
                    })?;
                info!(
                    "Downloaded and saved tarball for {}@{} to {:?}",
                    name, version_str, tarball_save_path
                );
            }
            BuildMode::Full => {
                // ensure_crate_docs already handles download, build, indexing, and caching (including tarball).
                self.ensure_crate_docs(name, Some(version_str))
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to fully preload (download and build) {}@{}",
                            name, version_str
                        )
                    })?;
                info!("Fully preloaded {}@{}", name, version_str);
            }
        }
        Ok(())
    }

    /// Purge a specific crate version from all caches.
    pub async fn purge_crate(&self, name: &str, version_str: &str) -> Result<()> {
        info!("Purging crate {}@{} from caches", name, version_str);
        let cache_key = format!("{}@{}", name, version_str);

        // 1. Remove from DocEngine's CrateDocumentation cache (memory and disk .bin.zst)
        self.cache
            .remove_crate_docs(&cache_key)
            .await
            .with_context(|| format!("Failed to purge docs cache for {}", cache_key))?;
        info!("Purged docs cache for {}", cache_key);

        // 2. Remove the Tantivy symbol index directory
        let symbol_index_path = self.index.symbol_index_base_path().join(&cache_key);
        if symbol_index_path.exists() {
            tokio::fs::remove_dir_all(&symbol_index_path)
                .await
                .with_context(|| {
                    format!(
                        "Failed to remove symbol index directory {:?}",
                        symbol_index_path
                    )
                })?;
            info!("Removed symbol index directory {:?}", symbol_index_path);
        } else {
            info!(
                "Symbol index directory for {} not found, skipping removal.",
                cache_key
            );
        }

        // 3. Remove the tarball
        let tarball_filename = format!("{}-{}.tar.gz", name, version_str);
        let tarball_path = self.get_tarballs_path().join(&tarball_filename);
        if tarball_path.exists() {
            tokio::fs::remove_file(&tarball_path)
                .await
                .with_context(|| format!("Failed to remove tarball {:?}", tarball_path))?;
            info!("Removed tarball {:?}", tarball_path);
        } else {
            info!(
                "Tarball for {}@{} not found, skipping removal.",
                name, version_str
            );
        }

        // 4. TODO: Remove from RocksDB (traits and metadata) if applicable.
        // This would require IndexCore to expose methods to delete data by crate/version key.
        // self.index.purge_trait_data_for_crate(&cache_key)?;
        // self.index.purge_meta_data_for_crate(&cache_key)?;

        Ok(())
    }

    /// Get overall statistics for the DocEngine.
    pub async fn stats(&self) -> Result<EngineStats> {
        let cache_stats = self
            .cache
            .get_stats()
            .await
            .context("Failed to get cache stats from cache module")?;

        // Estimate indexed_crates_count from number of items in the disk cache for docs
        // This is an approximation. A more accurate count might come from meta_db in IndexCore.
        let indexed_crates_count = cache_stats.total_docs_on_disk_entries;

        Ok(EngineStats {
            cache_stats,
            indexed_crates_count,
            // TODO: Populate symbol_index_stats and trait_index_stats if IndexCore provides them
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

    /// Ensure crate documentation is available and indexed
    #[tracing::instrument(skip(self), fields(crate_name = %crate_name, version = ?version))]
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
            Version::parse(v).context("Invalid version format")?
        } else {
            Version::parse(&crate_info.latest_version).context("Invalid latest version")?
        };

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

        // build_rustdoc_json now needs to return the temp_dir path as well, or we call download separately
        // Let's modify build_rustdoc_json to return both RustdocCrate and the PathBuf of the source root.
        let (rustdoc_crate, source_root_path_opt) = self
            .build_rustdoc_json_and_get_source_path(crate_name, &target_version)
            .await?;
        let docs =
            CrateDocumentation::new(rustdoc_crate, &self.index, source_root_path_opt).await?;

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

    /// Build rustdoc JSON for a crate and return it along with the source path.
    async fn build_rustdoc_json_and_get_source_path(
        &self,
        crate_name: &str,
        version: &Version,
    ) -> Result<(RustdocCrate, Option<PathBuf>)> {
        // Download and extract crate
        let (temp_dir, tarball_bytes) = self.fetcher.download_crate(crate_name, version).await?;
        let source_path = temp_dir.path().to_path_buf(); // Path to the root of extracted source

        // Save the tarball
        let tarball_filename = format!("{}-{}.tar.gz", crate_name, version);
        let tarball_save_path = self.get_tarballs_path().join(&tarball_filename);
        fs::write(&tarball_save_path, &tarball_bytes)
            .await
            .with_context(|| format!("Failed to save crate tarball to {:?}", tarball_save_path))?;
        info!("Saved crate tarball to {:?}", tarball_save_path);

        // Build rustdoc JSON
        // RustdocBuilder might find the actual crate root if it's nested (e.g. target_dir/crate_name-version/)
        // So, the source_path passed to CrateDocumentation should be this actual root.
        let rustdoc_builder = rustdoc::RustdocBuilder::new(&source_path);
        let rustdoc_crate = rustdoc_builder.build_json().await?;

        // The RustdocBuilder itself might have resolved a deeper path if Cargo.toml wasn't at source_path directly.
        // For snippet extraction, we need the path that span.filename is relative to.
        // This is typically the directory containing the Cargo.toml that was built.
        // RustdocBuilder::find_crate_root() already does this.
        // We should use the path returned by find_crate_root (or the one passed to RustdocBuilder if it was already correct).
        // For simplicity, let's assume temp_dir.path() is the correct root for now,
        // and RustdocBuilder uses it. If find_crate_root changes it, that internal path would be best.
        // For now, temp_dir.path() is the most straightforward to pass.
        // A more robust solution might involve RustdocBuilder exposing the discovered crate root.

        Ok((rustdoc_crate, Some(source_path)))
    }
}

/// Documentation for a specific crate
// Lazy static for Syntect resources
static SYNTAX_SET: once_cell::sync::Lazy<SyntaxSet> =
    once_cell::sync::Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: once_cell::sync::Lazy<ThemeSet> =
    once_cell::sync::Lazy::new(ThemeSet::load_defaults);

#[derive(Debug, Serialize, Deserialize)]
pub struct CrateDocumentation {
    rustdoc_crate: RustdocCrate,
    /// The absolute path to the root of the downloaded crate source code.
    /// This is needed for resolving relative paths from rustdoc spans.
    #[serde(skip, default = "default_path_buf")]
    // Skip for serde, provide default for deserialization if needed
    source_root_path: Option<PathBuf>,
    #[serde(skip)]
    trait_impl_index: TraitImplIndex,
    #[serde(skip)]
    symbol_index: Option<SymbolIndex>,
}

fn default_path_buf() -> Option<PathBuf> {
    None
}

impl CrateDocumentation {
    /// Create new crate documentation
    pub async fn new(
        rustdoc_crate: RustdocCrate,
        index_core: &IndexCore,
        source_root_path: Option<PathBuf>, // Added source_root_path
    ) -> Result<Self> {
        // Build indexes
        let trait_impl_index = TraitImplIndex::from_rustdoc(&rustdoc_crate)?;

        // Construct the specific symbol index path for this crate
        // Assuming crate name and version can be derived or are passed here.
        // For now, let's assume a placeholder name/version for the path.
        // This needs to be robustly determined by DocEngine.
        let crate_name = rustdoc_crate
            .index
            .get(&rustdoc_crate.root)
            .and_then(|item| item.name.as_ref())
            .map_or("unknown_crate", |s| s);
        let crate_version = rustdoc_crate.crate_version.as_deref().unwrap_or("0.0.0");
        let symbol_index_path = index_core
            .symbol_index_base_path()
            .join(format!("{}@{}", crate_name, crate_version));

        let symbol_index_instance = SymbolIndex::new(symbol_index_path);
        symbol_index_instance.add_crate(&rustdoc_crate).await?;

        Ok(Self {
            rustdoc_crate,
            source_root_path,
            trait_impl_index,
            symbol_index: Some(symbol_index_instance),
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
            .ok_or_else(|| anyhow::anyhow!("Item '{}' not found in index", item_path))?;

        let span = item.span.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "No source location (span) available for item '{}'",
                item_path
            )
        })?;

        let source_root = self.source_root_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Source root path not configured for this crate documentation")
        })?;

        // The span.filename is relative to the crate source root.
        let absolute_file_path = source_root.join(&span.filename);

        if !absolute_file_path.exists() {
            return Err(anyhow::anyhow!(
                "Source file not found: {:?}. (Resolved from span: {:?}, root: {:?})",
                absolute_file_path,
                span.filename,
                source_root
            ));
        }

        let file = File::open(&absolute_file_path)?;
        let mut reader = BufReader::new(file);
        let mut file_content = String::new();
        reader.read_to_string(&mut file_content)?;

        let lines: Vec<&str> = file_content.lines().collect();
        let total_lines = lines.len();

        // Rustdoc spans are 0-indexed for lines and columns internally, but typically displayed 1-indexed.
        // Let's assume span.begin.0 and span.end.0 are 1-indexed line numbers as per rustdoc JSON spec for spans.
        // If they are 0-indexed, adjust accordingly. The spec says "line numbers are 1-indexed".
        let highlight_start_line_1based = span.begin.0;
        let highlight_end_line_1based = span.end.0;

        let display_start_line_1based = (highlight_start_line_1based as u32)
            .saturating_sub(context_lines)
            .max(1);
        let display_end_line_1based =
            ((highlight_end_line_1based as u32) + context_lines).min(total_lines as u32);

        let mut snippet_code = String::new();
        let syntax = SYNTAX_SET
            .find_syntax_by_extension("rs")
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());
        // Using a common theme, e.g., "base16-ocean.dark" or "InspiredGitHub"
        let theme = &THEME_SET.themes["base16-ocean.dark"]; // Or choose another default
        let mut h = HighlightLines::new(syntax, theme);

        for line_num_1based in display_start_line_1based..=display_end_line_1based {
            if let Some(line_content) = lines.get((line_num_1based - 1) as usize) {
                let regions: Vec<(Style, &str)> = h.highlight_line(line_content, &SYNTAX_SET)?;
                // Convert to HTML or plain text with ANSI for CLI. For now, let's keep it simple.
                // For SourceSnippet, we might want to return plain text or structured lines.
                // The spec doesn't specify format, let's assume plain text for now.
                // If HTML is needed later, `styled_line_to_html` can be used.
                snippet_code.push_str(line_content);
                snippet_code.push('\n');
            }
        }
        // Remove trailing newline if any
        if snippet_code.ends_with('\n') {
            snippet_code.pop();
        }

        Ok(SourceSnippet {
            code: snippet_code,
            file: span.filename.to_string_lossy().into_owned(),
            line_start: display_start_line_1based,
            line_end: display_end_line_1based,
            context_lines,
            highlighted_line: Some(highlight_start_line_1based as u32), // Main item start line
            language: "rust".to_string(),
        })
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

    /// Find an item by its fully qualified path using the rustdoc_crate.paths map.
    fn find_item_by_path(&self, path_str: &str) -> Result<Id> {
        // The `paths` map in RustdocCrate stores `Id` -> `Path` (which contains `Vec<String>`).
        // We need to find an Id whose resolved path matches `path_str`.
        // A more efficient way would be to invert the `paths` map on creation,
        // from `String path -> Id`, but that would require preprocessing.
        // For now, iterate and match.

        // The rustdoc JSON `paths` field is a map from ID to a `Path` struct,
        // where `Path.path` is `Vec<String>`.
        // The key for lookup should be the string path.

        // First, try a direct lookup if the `path_str` is exactly what might be a key in a pre-processed map.
        // However, rustdoc_crate.paths is Id -> Path. So we must iterate.
        for (id, path_info) in &self.rustdoc_crate.paths {
            // path_info.path is Vec<String>, e.g., ["my_crate", "module", "MyStruct"]
            let current_item_path_str = path_info.path.join("::");
            if current_item_path_str == path_str {
                return Ok(id.clone());
            }
        }

        // Fallback: The spec mentions "ends_with" for the old naive way.
        // The `paths` map should provide exact matches for fully qualified paths.
        // If `path_str` could be partial, or if the `paths` map isn't exhaustive for all searchable items,
        // then a scan might still be needed as a fallback, but the goal is to rely on `paths`.
        // The provided path_str should ideally be fully qualified.

        // Check if the path_str matches any item's name directly if it's not fully qualified.
        // This is a bit ambiguous with "path-aware item resolution inside rustdoc JSON (no more naive “ends_with”)"
        // If path_str is "MyStruct" and there's only one "MyStruct", this might be acceptable.
        // But if it's "module::MyStruct", paths map is the way.
        // Let's assume path_str is intended to be fully qualified for now.

        Err(anyhow::anyhow!(
            "Item with path '{}' not found in rustdoc_crate.paths map.",
            path_str
        ))
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
}
