//! Doc Engine - Rust crate documentation fetching and processing
//!
//! This crate provides functionality to fetch Rust crate documentation,
//! build rustdoc JSON, and provide a high-level API for querying documentation.
//! It also supports fetching source code from local environments for Python and Node.js.

use crate::{
    doc_engine::python_semantic::PythonSemanticIndex,
    index_core::{IndexCore, SymbolIndex, TraitImplIndex},
};
use anyhow::{Context, Result};
use lru::LruCache;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{fs, sync::Mutex};
use tracing::{info, warn};

use crate::doc_engine::processors::traits::{ImplementationContext, LanguageProcessor};

pub mod cache;
pub mod fetcher;
pub mod finder;
pub mod local;
pub mod processors;
pub mod python_semantic;
pub mod scraper;
pub mod types;

use crate::doc_engine::types::*;
use crate::shared_types::SemanticSearchResult;

/// Helper function to convert between source location types
fn convert_source_location(
    sl: Option<crate::index_core::types::SourceLocation>,
) -> Option<SourceLocation> {
    sl.map(|s| SourceLocation {
        file: s.file,
        line: s.line,
        column: s.column,
        end_line: s.end_line,
        end_column: s.end_column,
    })
}

/// Configuration options for the documentation engine.
#[derive(Debug, Clone, Default)]
pub struct DocEngineOptions {
    /// Optional default working directory used when resolving local packages.
    pub working_dir: Option<PathBuf>,
}

/// Main documentation engine that coordinates fetching, caching, and indexing
#[derive(Debug, Clone)]
pub struct DocEngine {
    fetcher: Arc<fetcher::Fetcher>,
    cache: Arc<cache::Cache>,
    index: Arc<IndexCore>,
    memory_cache: Arc<Mutex<LruCache<String, Arc<CrateDocumentation>>>>,
    version_cache: Arc<Mutex<LruCache<String, String>>>,
    python_semantic_cache: Arc<Mutex<LruCache<String, Arc<PythonSemanticIndex>>>>,
    python_processor: Arc<processors::python::PythonProcessor>,
    node_processor: Arc<processors::node::NodeProcessor>,
    rust_processor: Arc<processors::rust::RustProcessor>,
    working_dir: Option<PathBuf>,
}

impl DocEngine {
    /// Create a new documentation engine
    pub async fn new(cache_dir: impl AsRef<Path>) -> Result<Self> {
        Self::new_with_options(cache_dir, DocEngineOptions::default()).await
    }

    /// Create a new documentation engine with explicit options.
    pub async fn new_with_options(
        cache_dir: impl AsRef<Path>,
        options: DocEngineOptions,
    ) -> Result<Self> {
        let cache_dir = cache_dir.as_ref();
        fs::create_dir_all(cache_dir)
            .await
            .context("Failed to create cache directory")?;

        let fetcher = Arc::new(fetcher::Fetcher::new());
        let cache = Arc::new(cache::Cache::new(cache_dir)?);
        let index = Arc::new(IndexCore::new(cache_dir.join("index"))?);
        let memory_cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())));
        let version_cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(1000).unwrap())));
        let python_semantic_cache =
            Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(32).unwrap())));
        let python_processor = Arc::new(processors::python::PythonProcessor);
        let node_processor = Arc::new(processors::node::NodeProcessor);
        let rust_processor = Arc::new(processors::rust::RustProcessor);
        let working_dir = options
            .working_dir
            .and_then(|dir| std::fs::canonicalize(&dir).ok().or(Some(dir)));

        Ok(Self {
            fetcher,
            cache,
            index,
            memory_cache,
            version_cache,
            python_semantic_cache,
            python_processor,
            node_processor,
            rust_processor,
            working_dir,
        })
    }

    fn default_context_dir(&self) -> PathBuf {
        self.working_dir
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn normalize_context_path(&self, raw: &str) -> PathBuf {
        let candidate = PathBuf::from(raw);
        if candidate.is_absolute() {
            return candidate;
        }
        let base = self.default_context_dir();
        base.join(candidate)
    }

    fn resolve_context_dir(&self, context_path: Option<&str>) -> PathBuf {
        context_path
            .map(|p| self.normalize_context_path(p))
            .unwrap_or_else(|| self.default_context_dir())
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
        // Resolve (and possibly cache) the target version first
        let version_str = if let Some(v) = version {
            v.to_string()
        } else {
            // Version LRU (fast path)
            if let Some(cached) = {
                let mut cache = self.version_cache.lock().await;
                cache.get(crate_name).cloned()
            } {
                info!("Using cached version for {}: {}", crate_name, cached);
                cached
            } else {
                info!("Fetching latest version for crate: {}", crate_name);
                let latest = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    self.fetcher.get_latest_version_string(crate_name),
                )
                .await
                .context("Timeout getting latest version")?
                .context("Failed to get latest version")?;
                {
                    let mut cache = self.version_cache.lock().await;
                    cache.put(crate_name.to_string(), latest.clone());
                }
                info!("Cached version {} for {}", latest, crate_name);
                latest
            }
        };

        info!(
            "Checkpoint 1: Resolved version {} for {}",
            version_str, crate_name
        );
        info!(
            "Checkpoint 2: Checking item doc cache for {}::{}",
            crate_name, path
        );

        // Item-level cache (covers both local + remote fetched results)
        if let Some(cached_item) = self.cache.get_item_doc(crate_name, &version_str, path)? {
            info!(
                "Cache hit for item doc {}::{} (v {})",
                crate_name, path, version_str
            );
            return Ok(cached_item);
        }

        info!(
            "Checkpoint 3: Attempting local source doc extraction for {}@{}: {}",
            crate_name, version_str, path
        );

        // Attempt local extraction first (fast path when sources are present).
        let crate_name_owned = crate_name.to_string();
        let path_owned = path.to_string();
        let version_owned = version_str.clone();
        let local_start = std::time::Instant::now();

        let local_attempt = tokio::task::spawn_blocking(move || {
            local::fetch_local_item_doc(&crate_name_owned, &version_owned, &path_owned)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error in local doc fetch: {e}"))
        .and_then(|inner| inner);

        match local_attempt {
            Ok(item_doc) => {
                info!(
                    "Local doc fetch succeeded for {}::{} in {:?}",
                    crate_name,
                    path,
                    local_start.elapsed()
                );
                self.cache
                    .store_item_doc(crate_name, &version_str, path, &item_doc)?;
                return Ok(item_doc);
            }
            Err(err) => {
                warn!(
                    "Local doc fetch failed for {}::{} ({}). Falling back to docs.rs scrape.",
                    crate_name, path, err
                );
            }
        }

        // Fallback: use docs.rs scraping via search index (ensure_crate_docs fetches index + builds structure)
        info!(
            "Checkpoint 4: Fetching / ensuring search index for fallback docs.rs scrape {}@{}",
            crate_name, version_str
        );
        let docs = self
            .ensure_crate_docs(crate_name, Some(&version_str))
            .await
            .context("Failed to ensure crate documentation for fallback")?;

        let scrape_start = std::time::Instant::now();
        let scraped = docs
            .get_item_doc(path)
            .await
            .with_context(|| format!("docs.rs scrape failed for {}::{}", crate_name, path))?;

        info!(
            "Fallback docs.rs scrape succeeded for {}::{} in {:?}",
            crate_name,
            path,
            scrape_start.elapsed()
        );

        // Cache scraped result
        self.cache
            .store_item_doc(crate_name, &version_str, path, &scraped)?;

        Ok(scraped)
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

    /// Perform semantic search within a local package (currently Python support).
    pub async fn semantic_search(
        &self,
        language: &str,
        package_name: &str,
        query: &str,
        limit: usize,
        context_path: Option<&str>,
    ) -> Result<Vec<SemanticSearchResult>> {
        match language {
            "python" => {
                self.semantic_search_python(package_name, query, limit, context_path)
                    .await
            }
            _ => Err(anyhow::anyhow!(
                "Semantic search is not implemented for language '{}'",
                language
            )),
        }
    }

    async fn semantic_search_python(
        &self,
        package_name: &str,
        query: &str,
        limit: usize,
        context_path: Option<&str>,
    ) -> Result<Vec<SemanticSearchResult>> {
        if query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let context_dir = context_path
            .map(|p| self.resolve_context_dir(Some(p)))
            .unwrap_or_else(|| self.default_context_dir());

        let package_root =
            if let Some(path) = Self::find_local_python_package(&context_dir, package_name) {
                path
            } else {
                self.find_python_package_via_finder(package_name, Some(context_dir.clone()))
                    .await?
            };

        let cache_key = format!("{}::{}", package_name, package_root.to_string_lossy());
        let index = {
            let mut cache = self.python_semantic_cache.lock().await;
            if let Some(existing) = cache.get(&cache_key) {
                Arc::clone(existing)
            } else {
                drop(cache);
                let package_root_clone = package_root.clone();
                let package_name_owned = package_name.to_string();
                let index = tokio::task::spawn_blocking(move || {
                    PythonSemanticIndex::build(&package_name_owned, &package_root_clone)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Python semantic index worker failed: {e}"))??;
                let index = Arc::new(index);
                let mut cache = self.python_semantic_cache.lock().await;
                cache.put(cache_key.clone(), Arc::clone(&index));
                index
            }
        };

        Ok(index.search(query, limit))
    }

    async fn find_python_package_via_finder(
        &self,
        package_name: &str,
        context_path: Option<PathBuf>,
    ) -> Result<PathBuf> {
        let package_name = package_name.to_string();
        tokio::task::spawn_blocking(move || {
            finder::find_python_package_path_with_context(&package_name, context_path.as_deref())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to join python finder task: {e}"))?
    }

    fn find_local_python_package(context_path: &Path, package_name: &str) -> Option<PathBuf> {
        if !context_path.exists() {
            return None;
        }

        let mut roots = Vec::new();
        roots.push(context_path.to_path_buf());
        for extra in ["src", "python", "lib"] {
            let candidate = context_path.join(extra);
            if candidate.is_dir() {
                roots.push(candidate);
            }
        }

        let segments: Vec<&str> = package_name.split('.').collect();
        for root in roots {
            let mut path = root.clone();
            for segment in &segments {
                path.push(segment);
            }

            if path.is_dir() {
                return Some(path);
            }

            let init_candidate = path.join("__init__.py");
            if init_candidate.is_file() {
                return Some(path);
            }

            let module_file = path.with_extension("py");
            if module_file.is_file() {
                if let Some(parent) = module_file.parent() {
                    return Some(parent.to_path_buf());
                }
            }
        }
        None
    }

    /// Resolve one or more import statements (Rust/Python/Node) to concrete symbol locations with
    /// basic caching and re-export traversal.
    ///
    /// Supported:
    /// - Rust: `use path::to::{Type, func};` (handles simple `pub use` re-exports)
    /// - Python:
    ///   import pkg.mod
    ///   from pkg.mod import A, B
    /// - Node (ESM style only here for simplicity):
    ///   import X from "pkg/subpath"
    ///   import {A,B} from "pkg/subpath"
    ///   import * as NS from "pkg/subpath"
    ///
    /// Returns best-effort symbol locations. Re-exports:
    /// - Rust: scans encountered module files for `pub use ...` lines and recursively resolves.
    /// - Python: follows `from .sub import Name` style within same package plus `__all__` lists.
    /// - Node: processes `export {X} from "./file"` and `export * from "./file"`.
    pub async fn resolve_imports(
        &self,
        params: &crate::doc_engine::types::ImportResolutionParams,
    ) -> Result<crate::doc_engine::types::ImportResolutionResponse> {
        use crate::doc_engine::types::{
            ImportResolutionResponse, ImportResolutionResult, ImportResolutionStatus,
            ImportSymbolLocation,
        };
        use std::collections::{HashMap, HashSet, VecDeque};
        use std::time::{Duration, Instant};

        // ------------------------------------------------------------------
        // Bounded LRU + TTL cache for import resolution (per-process).
        // Key: language::package::import_line (trimmed).
        // Capacity: 512 entries. TTL: 5 minutes.
        // Eviction policy: remove least-recently-used (front of deque) when full.
        // ------------------------------------------------------------------
        struct ImportCacheEntry {
            key: String,
            inserted: Instant,
            result: ImportResolutionResult,
        }

        struct ImportCache {
            map: HashMap<String, usize>,    // key -> index in entries
            order: VecDeque<String>,        // LRU order (front = oldest, back = newest)
            entries: Vec<ImportCacheEntry>, // storage (swap-remove on eviction)
            ttl: Duration,
            capacity: usize,
        }

        impl ImportCache {
            fn new(capacity: usize, ttl: Duration) -> Self {
                Self {
                    map: HashMap::new(),
                    order: VecDeque::new(),
                    entries: Vec::new(),
                    ttl,
                    capacity,
                }
            }

            fn norm_key(lang: &str, pkg: &str, context: &str, line: &str) -> String {
                let ctx = if context.is_empty() {
                    "<default>"
                } else {
                    context
                };
                let sanitized = ctx.replace("::", "/");
                format!("{lang}::{pkg}::{sanitized}::{}", line.trim())
            }

            fn get(
                &mut self,
                lang: &str,
                pkg: &str,
                context: &str,
                line: &str,
            ) -> Option<ImportResolutionResult> {
                let key = Self::norm_key(lang, pkg, context, line);
                if let Some(&idx) = self.map.get(&key) {
                    if Instant::now().duration_since(self.entries[idx].inserted) > self.ttl {
                        self.remove(&key);
                        return None;
                    }
                    // promote
                    self.order.retain(|k| k != &key);
                    self.order.push_back(key);
                    return Some(self.entries[idx].result.clone());
                }
                None
            }

            fn insert(
                &mut self,
                lang: &str,
                pkg: &str,
                context: &str,
                line: &str,
                result: ImportResolutionResult,
            ) {
                let key = Self::norm_key(lang, pkg, context, line);
                if let Some(&idx) = self.map.get(&key) {
                    self.entries[idx].result = result;
                    self.entries[idx].inserted = Instant::now();
                    self.order.retain(|k| k != &key);
                    self.order.push_back(key);
                    return;
                }
                if self.entries.len() >= self.capacity {
                    if let Some(old_key) = self.order.pop_front() {
                        self.remove(&old_key);
                    }
                }
                let entry = ImportCacheEntry {
                    key: key.clone(),
                    inserted: Instant::now(),
                    result,
                };
                self.entries.push(entry);
                self.order.push_back(key.clone());
                self.map.insert(key, self.entries.len() - 1);
            }

            fn remove(&mut self, key: &str) {
                if let Some(idx) = self.map.remove(key) {
                    self.order.retain(|k| k != key);
                    let last = self.entries.len() - 1;
                    self.entries.swap(idx, last);
                    let popped = self.entries.pop();
                    if idx < self.entries.len() {
                        let moved_key = self.entries[idx].key.clone();
                        self.map.insert(moved_key, idx);
                    }
                    drop(popped);
                }
            }
        }

        static IMPORT_CACHE: std::sync::OnceLock<std::sync::Mutex<ImportCache>> =
            std::sync::OnceLock::new();
        let cache_mutex = IMPORT_CACHE
            .get_or_init(|| std::sync::Mutex::new(ImportCache::new(512, Duration::from_secs(300))));

        let mut diagnostics = Vec::new();
        let mut import_lines: Vec<String> = Vec::new();
        if let Some(line) = &params.import_line {
            import_lines.push(line.trim().to_string());
        } else if let Some(block) = &params.code_block {
            for l in block.lines() {
                let t = l.trim();
                if t.starts_with("use ")
                    || t.starts_with("import ")
                    || t.starts_with("from ")
                    || t.starts_with("export ")
                {
                    import_lines.push(t.to_string());
                }
            }
        }
        if import_lines.is_empty() {
            diagnostics.push("No import lines detected.".to_string());
        }

        // Removed unused cache_key closure (LRU cache handles key construction internally)

        // Process per language
        let mut results: Vec<ImportResolutionResult> = Vec::new();

        match params.language.as_str() {
            "rust" => {
                // Determine crate root
                let version = if let Some(v) = &params.version {
                    v.clone()
                } else {
                    crate::doc_engine::finder::find_latest_rust_crate_version(&params.package)?
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "No installed versions found for crate '{}'",
                                params.package
                            )
                        })?
                };
                let crate_root =
                    crate::doc_engine::finder::find_rust_crate_path(&params.package, &version)?;

                // Simple re-export index (file -> Vec<(public symbol, target path string)>)
                let mut reexport_cache: HashMap<std::path::PathBuf, Vec<(String, String)>> =
                    HashMap::new();

                for raw in import_lines {
                    if let Some(cached) =
                        cache_mutex
                            .lock()
                            .unwrap()
                            .get("rust", &params.package, "", &raw)
                    {
                        results.push(cached);
                        continue;
                    }

                    let mut resolution = ImportResolutionResult {
                        language: "rust".to_string(),
                        package: params.package.clone(),
                        import_statement: raw.clone(),
                        module_path: Vec::new(),
                        requested_symbols: Vec::new(),
                        resolved: Vec::new(),
                        diagnostics: Vec::new(),
                    };

                    // Parse & branch
                    let line = raw.trim().trim_end_matches(';');
                    if !line.starts_with("use ") {
                        resolution
                            .diagnostics
                            .push("Not a Rust use statement".to_string());
                        cache_mutex.lock().unwrap().insert(
                            "rust",
                            &params.package,
                            "",
                            &raw,
                            resolution.clone(),
                        );
                        results.push(resolution);
                        continue;
                    }
                    let body = line.trim_start_matches("use ").trim();
                    let body_no_alias = body.split(" as ").next().unwrap_or(body).trim();

                    // Expand { ... } groups if present
                    let mut items: Vec<(Vec<String>, String)> = Vec::new();
                    if let Some(open) = body_no_alias.find('{') {
                        if let Some(close) = body_no_alias.rfind('}') {
                            let base = body_no_alias[..open].trim().trim_end_matches("::");
                            let base_segments: Vec<String> = base
                                .split("::")
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                                .collect();
                            for part in body_no_alias[open + 1..close].split(',') {
                                let sym = part.trim();
                                if sym.is_empty() {
                                    continue;
                                }
                                items.push((base_segments.clone(), sym.to_string()));
                            }
                        } else {
                            resolution
                                .diagnostics
                                .push("Mismatched braces in use statement".into());
                        }
                    } else {
                        let segs: Vec<String> = body_no_alias
                            .split("::")
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect();
                        if segs.is_empty() {
                            resolution.diagnostics.push("Empty path".into());
                        } else {
                            let (modules, last) = segs.split_at(segs.len() - 1);
                            items.push((modules.to_vec(), last[0].clone()));
                        }
                    }

                    // Resolve each item
                    for (module_segments, symbol) in items {
                        resolution.requested_symbols.push(symbol.clone());
                        let (file_path_opt, _) =
                            Self::resolve_rust_module_file(&crate_root, &module_segments);
                        if let Some(file_path) = file_path_opt {
                            // gather symbol + re-export traversal
                            let mut found_any = false;
                            let mut visited_files = HashSet::new();
                            let mut queue: Vec<String> = vec![symbol.clone()];
                            while let Some(sym) = queue.pop() {
                                // direct search
                                let locs = Self::search_rust_symbols_in_file(
                                    &file_path,
                                    std::slice::from_ref(&sym),
                                )?;
                                if !locs.is_empty() {
                                    for (s, line_no, kind) in locs {
                                        found_any = true;
                                        resolution.resolved.push(ImportSymbolLocation {
                                            symbol: s,
                                            file: file_path.to_string_lossy().into(),
                                            line: line_no,
                                            column: 1,
                                            end_line: None,
                                            end_column: None,
                                            kind,
                                            status: ImportResolutionStatus::Resolved,
                                            note: None,
                                        });
                                    }
                                }
                                // re-export scan
                                if !visited_files.insert(file_path.clone()) {
                                    continue;
                                }
                                let reexports =
                                    reexport_cache.entry(file_path.clone()).or_insert_with(|| {
                                        Self::scan_rust_reexports(&file_path)
                                            .unwrap_or_else(|_| Vec::new())
                                    });
                                for (pub_sym, target_path) in reexports.iter() {
                                    if pub_sym == &sym {
                                        // Attempt to resolve target_path recursively
                                        // naive: treat as full path path::to::Item
                                        let segs: Vec<String> = target_path
                                            .split("::")
                                            .filter(|s| !s.is_empty())
                                            .map(|s| s.to_string())
                                            .collect();
                                        if !segs.is_empty() {
                                            let (mods, last) = segs.split_at(segs.len() - 1);
                                            let (file_candidate, _) =
                                                Self::resolve_rust_module_file(&crate_root, mods);
                                            if let Some(fc) = file_candidate {
                                                queue.push(last[0].clone());
                                                if fc != file_path {
                                                    let locs2 = Self::search_rust_symbols_in_file(
                                                        &fc,
                                                        std::slice::from_ref(&last[0]),
                                                    )
                                                    .unwrap_or_default();
                                                    for (s2, l2, k2) in locs2 {
                                                        found_any = true;
                                                        resolution.resolved.push(ImportSymbolLocation {
                                                            symbol: s2,
                                                            file: fc.to_string_lossy().into(),
                                                            line: l2,
                                                            column: 1,
                                                            end_line: None,
                                                            end_column: None,
                                                            kind: k2,
                                                            status: ImportResolutionStatus::Resolved,
                                                            note: Some("Resolved via re-export".into()),
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if !found_any {
                                resolution.resolved.push(ImportSymbolLocation {
                                    symbol: symbol.clone(),
                                    file: file_path.to_string_lossy().into(),
                                    line: 1,
                                    column: 1,
                                    end_line: None,
                                    end_column: None,
                                    kind: None,
                                    status: ImportResolutionStatus::NotFound,
                                    note: Some("Symbol not found in module or re-exports".into()),
                                });
                            }
                        } else {
                            resolution.resolved.push(ImportSymbolLocation {
                                symbol: symbol.clone(),
                                file: format!("{}/(unresolved module)", crate_root.display()),
                                line: 1,
                                column: 1,
                                end_line: None,
                                end_column: None,
                                kind: None,
                                status: ImportResolutionStatus::NotFound,
                                note: Some("Module file not found".into()),
                            });
                        }
                    }

                    cache_mutex.lock().unwrap().insert(
                        "rust",
                        &params.package,
                        "",
                        &raw,
                        resolution.clone(),
                    );
                    results.push(resolution);
                }
            }
            "python" => {
                // Root resolution
                let context_dir = self.resolve_context_dir(params.context_path.as_deref());
                let context_key = context_dir.to_string_lossy().to_string();
                let package_root =
                    crate::doc_engine::finder::find_python_package_path_with_context(
                        &params.package,
                        Some(&context_dir),
                    )?;
                for raw in import_lines {
                    if let Some(cached) = cache_mutex.lock().unwrap().get(
                        "python",
                        &params.package,
                        &context_key,
                        &raw,
                    ) {
                        results.push(cached);
                        continue;
                    }
                    let mut resolution = ImportResolutionResult {
                        language: "python".to_string(),
                        package: params.package.clone(),
                        import_statement: raw.clone(),
                        module_path: Vec::new(),
                        requested_symbols: Vec::new(),
                        resolved: Vec::new(),
                        diagnostics: Vec::new(),
                    };
                    // Patterns:
                    // 1) import pkg.sub.mod
                    // 2) from pkg.sub.mod import A, B
                    if raw.starts_with("from ") {
                        // from X import A, B
                        if let Some(rest) = raw.strip_prefix("from ") {
                            if let Some((module_part, import_part)) = rest.split_once(" import ") {
                                let symbols: Vec<String> = import_part
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                let mod_segs: Vec<&str> =
                                    module_part.split('.').filter(|s| !s.is_empty()).collect();
                                resolution.module_path =
                                    mod_segs.iter().map(|s| s.to_string()).collect();
                                // Build path
                                let (file_path, is_pkg) =
                                    Self::python_module_to_file(&package_root, &mod_segs);
                                for sym in &symbols {
                                    resolution.requested_symbols.push(sym.clone());
                                    if let Some((fp, is_pkg)) =
                                        file_path.clone().map(|p| (p, is_pkg))
                                    {
                                        let loc = Self::search_python_symbol(&fp, sym.as_str())
                                            .unwrap_or(None);
                                        if let Some((line, kind)) = loc {
                                            resolution.resolved.push(ImportSymbolLocation {
                                                symbol: sym.clone(),
                                                file: fp.to_string_lossy().into(),
                                                line,
                                                column: 1,
                                                end_line: None,
                                                end_column: None,
                                                kind: Some(kind),
                                                status: ImportResolutionStatus::Resolved,
                                                note: if is_pkg {
                                                    Some("__init__ module".into())
                                                } else {
                                                    None
                                                },
                                            });
                                        } else {
                                            resolution.resolved.push(ImportSymbolLocation {
                                                symbol: sym.clone(),
                                                file: fp.to_string_lossy().into(),
                                                line: 1,
                                                column: 1,
                                                end_line: None,
                                                end_column: None,
                                                kind: None,
                                                status: ImportResolutionStatus::NotFound,
                                                note: Some("Symbol not found".into()),
                                            });
                                        }
                                    } else {
                                        resolution.resolved.push(ImportSymbolLocation {
                                            symbol: sym.clone(),
                                            file: format!(
                                                "{}/(unresolved module)",
                                                package_root.display()
                                            ),
                                            line: 1,
                                            column: 1,
                                            end_line: None,
                                            end_column: None,
                                            kind: None,
                                            status: ImportResolutionStatus::NotFound,
                                            note: Some("Module path not found".into()),
                                        });
                                    }
                                }
                            } else {
                                resolution
                                    .diagnostics
                                    .push("Malformed 'from ... import ...'".into());
                            }
                        }
                    } else if raw.starts_with("import ") {
                        // import pkg.mod.sub
                        if let Some(rest) = raw.strip_prefix("import ") {
                            let segs: Vec<&str> =
                                rest.split('.').filter(|s| !s.is_empty()).collect();
                            resolution.module_path = segs.iter().map(|s| s.to_string()).collect();
                            // Just resolve file; no specific symbols
                            let (file_path, _is_pkg) =
                                Self::python_module_to_file(&package_root, &segs);
                            if let Some(fp) = file_path {
                                resolution.resolved.push(ImportSymbolLocation {
                                    symbol: segs.last().unwrap().to_string(),
                                    file: fp.to_string_lossy().into(),
                                    line: 1,
                                    column: 1,
                                    end_line: None,
                                    end_column: None,
                                    kind: Some("module".into()),
                                    status: ImportResolutionStatus::Resolved,
                                    note: None,
                                });
                            } else {
                                resolution.resolved.push(ImportSymbolLocation {
                                    symbol: segs.last().unwrap().to_string(),
                                    file: format!("{}/(unresolved module)", package_root.display()),
                                    line: 1,
                                    column: 1,
                                    end_line: None,
                                    end_column: None,
                                    kind: None,
                                    status: ImportResolutionStatus::NotFound,
                                    note: Some("Module path not found".into()),
                                });
                            }
                        }
                    } else {
                        resolution
                            .diagnostics
                            .push("Unsupported python import form".into());
                    }
                    cache_mutex.lock().unwrap().insert(
                        "python",
                        &params.package,
                        &context_key,
                        &raw,
                        resolution.clone(),
                    );
                    results.push(resolution);
                }
            }
            "node" => {
                // Determine root (reuse finder)
                let context_dir = self.resolve_context_dir(params.context_path.as_deref());
                let context_key = context_dir.to_string_lossy().to_string();
                let package_root = crate::doc_engine::finder::find_node_package_path(
                    &params.package,
                    &context_dir,
                )?;
                // Precompile regexes once outside the per-line loop (Clippy: regex_creation_in_loops)
                let import_re = regex::Regex::new(
                    r#"^import\s+(?P<what>[\s\*\{\}\w,]*?)\s+from\s+["'](?P<mod>[^"']+)["']"#,
                )
                .unwrap();
                let simple_import_re =
                    regex::Regex::new(r#"^import\s+["'](?P<mod>[^"']+)["'];?"#).unwrap();
                for raw in import_lines {
                    if let Some(cached) =
                        cache_mutex
                            .lock()
                            .unwrap()
                            .get("node", &params.package, &context_key, &raw)
                    {
                        results.push(cached);
                        continue;
                    }
                    let mut resolution = ImportResolutionResult {
                        language: "node".to_string(),
                        package: params.package.clone(),
                        import_statement: raw.clone(),
                        module_path: Vec::new(),
                        requested_symbols: Vec::new(),
                        resolved: Vec::new(),
                        diagnostics: Vec::new(),
                    };
                    // Parse ESM style
                    // import {A,B} from "mod";
                    // import X from "mod";
                    // import * as NS from "mod";
                    // reuse precompiled import_re
                    let re = &import_re;
                    if let Some(caps) = re.captures(&raw) {
                        let module_path: &str = caps.name("mod").unwrap().as_str();
                        let what_raw = caps.name("what").unwrap().as_str().trim();
                        let (file_path, is_dir) =
                            Self::node_module_to_file(&package_root, module_path);
                        if what_raw.starts_with('{') && what_raw.ends_with('}') {
                            // Named imports
                            for sym in what_raw
                                .trim_start_matches('{')
                                .trim_end_matches('}')
                                .split(',')
                            {
                                let s = sym.trim();
                                if s.is_empty() {
                                    continue;
                                }
                                resolution.requested_symbols.push(s.to_string());
                                if let Some(fp) = file_path.clone() {
                                    let loc = Self::search_node_symbol(&fp, s).unwrap_or(None);
                                    if let Some((line, kind)) = loc {
                                        resolution.resolved.push(ImportSymbolLocation {
                                            symbol: s.to_string(),
                                            file: fp.to_string_lossy().into(),
                                            line,
                                            column: 1,
                                            end_line: None,
                                            end_column: None,
                                            kind: Some(kind),
                                            status: ImportResolutionStatus::Resolved,
                                            note: if is_dir {
                                                Some("directory index (index.js/ts)".into())
                                            } else {
                                                None
                                            },
                                        });
                                    } else {
                                        resolution.resolved.push(ImportSymbolLocation {
                                            symbol: s.to_string(),
                                            file: fp.to_string_lossy().into(),
                                            line: 1,
                                            column: 1,
                                            end_line: None,
                                            end_column: None,
                                            kind: None,
                                            status: ImportResolutionStatus::NotFound,
                                            note: Some("Symbol not found".into()),
                                        });
                                    }
                                } else {
                                    resolution.resolved.push(ImportSymbolLocation {
                                        symbol: s.to_string(),
                                        file: format!(
                                            "{}/(unresolved module)",
                                            package_root.display()
                                        ),
                                        line: 1,
                                        column: 1,
                                        end_line: None,
                                        end_column: None,
                                        kind: None,
                                        status: ImportResolutionStatus::NotFound,
                                        note: Some("Module path not found".into()),
                                    });
                                }
                            }
                        } else if what_raw.starts_with('*') {
                            // Namespace import, just resolve module
                            if let Some(fp) = file_path.clone() {
                                resolution.resolved.push(ImportSymbolLocation {
                                    symbol: module_path.to_string(),
                                    file: fp.to_string_lossy().into(),
                                    line: 1,
                                    column: 1,
                                    end_line: None,
                                    end_column: None,
                                    kind: Some("module".into()),
                                    status: ImportResolutionStatus::Resolved,
                                    note: Some("namespace import".into()),
                                });
                            } else {
                                resolution.resolved.push(ImportSymbolLocation {
                                    symbol: module_path.to_string(),
                                    file: format!("{}/(unresolved module)", package_root.display()),
                                    line: 1,
                                    column: 1,
                                    end_line: None,
                                    end_column: None,
                                    kind: None,
                                    status: ImportResolutionStatus::NotFound,
                                    note: Some("Module path not found".into()),
                                });
                            }
                        } else if !what_raw.is_empty() {
                            // Default import treated as exported name lookup
                            if let Some(fp) = file_path.clone() {
                                let loc = Self::search_node_symbol(&fp, what_raw).unwrap_or(None);
                                if let Some((line, kind)) = loc {
                                    resolution.resolved.push(ImportSymbolLocation {
                                        symbol: what_raw.to_string(),
                                        file: fp.to_string_lossy().into(),
                                        line,
                                        column: 1,
                                        end_line: None,
                                        end_column: None,
                                        kind: Some(kind),
                                        status: ImportResolutionStatus::Resolved,
                                        note: Some("default import heuristic".into()),
                                    });
                                } else {
                                    resolution.resolved.push(ImportSymbolLocation {
                                        symbol: what_raw.to_string(),
                                        file: fp.to_string_lossy().into(),
                                        line: 1,
                                        column: 1,
                                        end_line: None,
                                        end_column: None,
                                        kind: None,
                                        status: ImportResolutionStatus::NotFound,
                                        note: Some("Symbol not found".into()),
                                    });
                                }
                            }
                        }
                    } else if raw.starts_with("import ") {
                        // Fallback simple: import "module";
                        let re2 = &simple_import_re;
                        if let Some(caps) = re2.captures(&raw) {
                            let module_path = caps.name("mod").unwrap().as_str();
                            let (file_path, is_dir) =
                                Self::node_module_to_file(&package_root, module_path);
                            if let Some(fp) = file_path {
                                resolution.resolved.push(ImportSymbolLocation {
                                    symbol: module_path.to_string(),
                                    file: fp.to_string_lossy().into(),
                                    line: 1,
                                    column: 1,
                                    end_line: None,
                                    end_column: None,
                                    kind: Some("module".into()),
                                    status: ImportResolutionStatus::Resolved,
                                    note: if is_dir {
                                        Some("directory index (index.js/ts)".into())
                                    } else {
                                        None
                                    },
                                });
                            } else {
                                resolution.resolved.push(ImportSymbolLocation {
                                    symbol: module_path.to_string(),
                                    file: format!("{}/(unresolved module)", package_root.display()),
                                    line: 1,
                                    column: 1,
                                    end_line: None,
                                    end_column: None,
                                    kind: None,
                                    status: ImportResolutionStatus::NotFound,
                                    note: Some("Module path not found".into()),
                                });
                            }
                        }
                    } else {
                        resolution
                            .diagnostics
                            .push("Unsupported Node import form".into());
                    }
                    cache_mutex.lock().unwrap().insert(
                        "node",
                        &params.package,
                        &context_key,
                        &raw,
                        resolution.clone(),
                    );
                    results.push(resolution);
                }
            }
            other => diagnostics.push(format!("Unsupported language '{other}'")),
        }

        let any_resolved = results.iter().any(|r| {
            r.resolved
                .iter()
                .any(|s| matches!(s.status, ImportResolutionStatus::Resolved))
        });

        Ok(ImportResolutionResponse {
            results,
            diagnostics,
            any_resolved,
        })
    }

    /// Internal helper: resolve Rust module file from segments.
    fn resolve_rust_module_file(
        crate_root: &std::path::Path,
        segments: &[String],
    ) -> (Option<std::path::PathBuf>, bool) {
        if segments.is_empty() {
            let lib_rs = crate_root.join("lib.rs");
            if lib_rs.is_file() {
                return (Some(lib_rs), true);
            }
        }
        let mut path = crate_root.to_path_buf();
        for seg in segments {
            path = path.join(seg);
        }
        let direct_rs = path.with_extension("rs");
        if direct_rs.is_file() {
            return (Some(direct_rs), true);
        }
        let mod_rs = path.join("mod.rs");
        if mod_rs.is_file() {
            return (Some(mod_rs), true);
        }
        // Fallback: last existing ancestor
        let mut ancestor = path.clone();
        while ancestor != *crate_root {
            if ancestor.is_file() {
                return (Some(ancestor), false);
            }
            ancestor = ancestor
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| crate_root.to_path_buf());
        }
        (None, false)
    }

    /// Search for given Rust symbols in a file, returning (symbol, line, kind).
    fn search_rust_symbols_in_file(
        path: &std::path::Path,
        symbols: &[String],
    ) -> Result<Vec<(String, u32, Option<String>)>> {
        let content = std::fs::read_to_string(path)?;
        let mut out = Vec::new();
        for sym in symbols {
            let pattern = format!(
                r"(?m)^(?:\s*(?:pub\s+(?:crate\s+)?)?(?:async\s+)?)((fn|struct|enum|trait|type|const|static)\s+{})\b",
                regex::escape(sym)
            );
            if let Ok(re) = Regex::new(&pattern) {
                if let Some(mat) = re.find(&content) {
                    let kind_caps = Regex::new(&format!(
                        r"(fn|struct|enum|trait|type|const|static)\s+{}",
                        regex::escape(sym)
                    ))
                    .unwrap()
                    .captures(mat.as_str());
                    let kind = kind_caps
                        .and_then(|c| c.get(1))
                        .map(|m| m.as_str().to_string());
                    let line = content[..mat.start()].lines().count() as u32 + 1;
                    out.push((sym.clone(), line, kind));
                }
            }
        }
        Ok(out)
    }

    /// Scan a Rust source file for simple `pub use path::to::Symbol;` re-exports.
    /// Returns Vec of (public_symbol, target_full_path).
    fn scan_rust_reexports(file: &std::path::Path) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        if let Ok(content) = std::fs::read_to_string(file) {
            // Pattern: pub use foo::bar::Baz;
            let re = Regex::new(r"(?m)^\s*pub\s+use\s+([A-Za-z0-9_:]+)::([A-Za-z0-9_]+)\s*;")?;
            for caps in re.captures_iter(&content) {
                let base = caps.get(1).unwrap().as_str();
                let sym = caps.get(2).unwrap().as_str();
                out.push((sym.to_string(), format!("{base}::{sym}")));
            }
        }
        Ok(out)
    }

    /// Convert Python module path segments (relative to package root) into a source file.
    /// Returns (Some(file_path), is_package_init) if resolved.
    fn python_module_to_file(
        package_root: &std::path::Path,
        segments: &[&str],
    ) -> (Option<std::path::PathBuf>, bool) {
        if segments.is_empty() {
            // root package __init__.py
            let init_py = package_root.join("__init__.py");
            if init_py.is_file() {
                return (Some(init_py), true);
            }
            return (None, false);
        }
        let mut path = package_root.to_path_buf();
        for s in segments {
            path = path.join(s);
        }
        let file_py = path.with_extension("py");
        if file_py.is_file() {
            return (Some(file_py), false);
        }
        let init_py = path.join("__init__.py");
        if init_py.is_file() {
            return (Some(init_py), true);
        }
        (None, false)
    }

    /// Heuristic Python symbol search: looks for class / def definitions.
    fn search_python_symbol(file: &std::path::Path, symbol: &str) -> Result<Option<(u32, String)>> {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };
        let class_re = Regex::new(&format!(r"(?m)^class\s+{}\b", regex::escape(symbol)))?;
        if let Some(m) = class_re.find(&content) {
            let line = content[..m.start()].lines().count() as u32 + 1;
            return Ok(Some((line, "class".into())));
        }
        let def_re = Regex::new(&format!(r"(?m)^def\s+{}\b", regex::escape(symbol)))?;
        if let Some(m) = def_re.find(&content) {
            let line = content[..m.start()].lines().count() as u32 + 1;
            return Ok(Some((line, "function".into())));
        }
        Ok(None)
    }

    /// Resolve a Node (ESM) module path to a concrete file (js/ts) or directory index.
    /// Returns (file_path, is_directory_index).
    fn node_module_to_file(
        package_root: &std::path::Path,
        module_path: &str,
    ) -> (Option<std::path::PathBuf>, bool) {
        // Strip possible leading ./ or /
        let rel = module_path.trim_start_matches("./").trim_start_matches('/');
        let base = package_root.join(rel);
        if base.is_file() {
            return (Some(base), false);
        }
        // Try explicit extensions
        for ext in &["js", "ts", "mjs", "cjs"] {
            let candidate = base.with_extension(ext);
            if candidate.is_file() {
                return (Some(candidate), false);
            }
        }
        // Try directory index
        if base.is_dir() {
            for ix in &["index.ts", "index.js", "index.mjs", "index.cjs"] {
                let candidate = base.join(ix);
                if candidate.is_file() {
                    return (Some(candidate), true);
                }
            }
        }
        (None, false)
    }

    /// Heuristic Node symbol search for exported entities.
    fn search_node_symbol(file: &std::path::Path, symbol: &str) -> Result<Option<(u32, String)>> {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };
        let patterns = vec![
            (
                format!(r"(?m)^export\s+class\s+{}\b", regex::escape(symbol)),
                "class",
            ),
            (
                format!(r"(?m)^export\s+function\s+{}\b", regex::escape(symbol)),
                "function",
            ),
            (
                format!(r"(?m)^export\s+const\s+{}\b", regex::escape(symbol)),
                "const",
            ),
            (
                format!(r"(?m)^export\s+let\s+{}\b", regex::escape(symbol)),
                "var",
            ),
            (
                format!(r"(?m)^export\s+var\s+{}\b", regex::escape(symbol)),
                "var",
            ),
            (
                format!(r"(?m)^class\s+{}\b", regex::escape(symbol)),
                "class",
            ),
            (
                format!(r"(?m)^function\s+{}\b", regex::escape(symbol)),
                "function",
            ),
            (
                format!(r"(?m)^const\s+{}\b", regex::escape(symbol)),
                "const",
            ),
        ];
        for (pat, kind) in patterns {
            if let Ok(re) = Regex::new(&pat) {
                if let Some(m) = re.find(&content) {
                    let line = content[..m.start()].lines().count() as u32 + 1;
                    return Ok(Some((line, kind.into())));
                }
            }
        }
        Ok(None)
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

        let context_dir = self.resolve_context_dir(context_path);

        match language {
            "python" => {
                self.python_processor
                    .get_implementation_context(
                        package_name,
                        &context_dir,
                        relative_path,
                        item_name,
                    )
                    .await
            }
            "node" => {
                self.node_processor
                    .get_implementation_context(
                        package_name,
                        &context_dir,
                        relative_path,
                        item_name,
                    )
                    .await
            }
            "rust" => {
                // New RustProcessor integration: leverage locally downloaded crate sources.
                self.rust_processor
                    .get_implementation_context(
                        package_name,
                        &context_dir,
                        relative_path,
                        item_name,
                    )
                    .await
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
            let mut cache = self.memory_cache.lock().await;
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
                let mut cache = self.memory_cache.lock().await;
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
            let mut cache = self.memory_cache.lock().await;
            cache.put(cache_key, Arc::clone(&docs));
        }

        Ok(docs)
    }

    /// Clear all cache entries
    pub async fn clear_all_cache(&self) -> Result<CacheOperationResult> {
        {
            let mut mem = self.memory_cache.lock().await;
            mem.clear();
        }
        {
            let mut versions = self.version_cache.lock().await;
            versions.clear();
        }
        {
            let mut python = self.python_semantic_cache.lock().await;
            python.clear();
        }
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
        let index_core_search_data = crate::index_core::traits::SearchIndexData {
            crate_name: search_index_data.crate_name.clone(),
            version: search_index_data.version.clone(),
            items: search_index_data
                .items
                .iter()
                .map(|item| crate::index_core::traits::SearchIndexItem {
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
    #[cfg(feature = "integration-tests")]
    use std::fs;
    #[cfg(feature = "integration-tests")]
    use std::path::Path;
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

    #[cfg(feature = "integration-tests")]
    struct CargoHomeGuard(Option<String>);

    #[cfg(feature = "integration-tests")]
    impl CargoHomeGuard {
        fn set(path: &Path) -> Self {
            let old_value = std::env::var("CARGO_HOME").ok();
            std::env::set_var("CARGO_HOME", path);
            Self(old_value)
        }
    }

    #[cfg(feature = "integration-tests")]
    impl Drop for CargoHomeGuard {
        fn drop(&mut self) {
            if let Some(old_value) = &self.0 {
                std::env::set_var("CARGO_HOME", old_value);
            } else {
                std::env::remove_var("CARGO_HOME");
            }
        }
    }

    #[cfg(feature = "integration-tests")]
    fn setup_crate() -> (tempfile::TempDir, CargoHomeGuard) {
        let temp = tempdir().unwrap();
        let guard = CargoHomeGuard::set(temp.path());
        let crate_dir = temp
            .path()
            .join("registry")
            .join("src")
            .join("test-reg")
            .join("mycrate-0.1.0");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(
            crate_dir.join("src/lib.rs"),
            concat!(
                "/// Example struct\n",
                "pub struct MyStruct;\n\n",
                "/// Example function\n",
                "pub fn my_fn() {}\n",
            ),
        )
        .unwrap();
        (temp, guard)
    }

    #[tokio::test]
    #[cfg(feature = "integration-tests")]
    async fn get_item_doc_from_local_sources() {
        let (dir, _guard) = setup_crate();
        let cache_dir = tempdir().unwrap();
        let engine = DocEngine::new(cache_dir.path()).await.unwrap();
        let doc = engine
            .get_item_doc("mycrate", "mycrate::MyStruct", Some("0.1.0"))
            .await
            .unwrap();
        assert_eq!(doc.kind, "struct");
        assert_eq!(doc.rendered_markdown, "Example struct");
        assert!(dir.path().join("registry").exists());
    }
}
