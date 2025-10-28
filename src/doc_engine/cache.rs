//! Cache module for storing and retrieving crate documentation

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::io::{Read, Write};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{debug, info};

use super::CrateDocumentation;
use crate::doc_engine::types::{
    CacheConfig, CacheOperation, CacheOperationResult, CacheStatistics, CrateCacheEntry,
    ItemCacheEntry, ItemDoc, SearchIndexData,
};

// Removed unused import

/// Enhanced cache for storing crate documentation and metadata with item-level caching
#[derive(Debug)]
pub struct Cache {
    cache_dir: PathBuf,
    memory_cache: Arc<Mutex<HashMap<String, CachedItem>>>,
    item_cache: Arc<Mutex<HashMap<String, ItemCacheEntry>>>,
    crate_cache: Arc<Mutex<HashMap<String, CrateCacheEntry>>>,
    config: CacheConfig,
    stats: Arc<Mutex<InternalCacheStats>>,
}

/// Internal cache statistics
#[derive(Debug, Default)]
struct InternalCacheStats {
    hits: u64,
    misses: u64,
    evictions: u64,
    total_requests: u64,
}

/// In-memory cached item
#[derive(Debug, Clone)]
struct CachedItem {
    data: Vec<u8>,
    last_accessed: SystemTime,
    size: usize,
}

/// Serializable cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    data: Vec<u8>,
    created_at: u64,
    last_accessed: u64,
    size: usize,
    version: String,
    checksum: String,
    metadata: HashMap<String, String>,
}

impl Cache {
    /// Create a new cache instance with default configuration
    pub fn new(cache_dir: impl AsRef<Path>) -> Result<Self> {
        Self::with_config(cache_dir, CacheConfig::default())
    }

    /// Create a new cache instance with custom configuration
    pub fn with_config(cache_dir: impl AsRef<Path>, config: CacheConfig) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir)?;

        // Create subdirectories for different cache types
        fs::create_dir_all(cache_dir.join("items"))?;
        fs::create_dir_all(cache_dir.join("crates"))?;
        fs::create_dir_all(cache_dir.join("indexes"))?;

        let memory_cache = Arc::new(Mutex::new(HashMap::new()));
        let item_cache = Arc::new(Mutex::new(HashMap::new()));
        let crate_cache = Arc::new(Mutex::new(HashMap::new()));
        let stats = Arc::new(Mutex::new(InternalCacheStats::default()));

        Ok(Self {
            cache_dir,
            memory_cache,
            item_cache,
            crate_cache,
            config,
            stats,
        })
    }

    /// Store crate documentation in cache
    pub fn store_crate_docs(&self, key: &str, docs: &CrateDocumentation) -> Result<()> {
        let serialized =
            bincode::serialize(docs).context("Failed to serialize crate documentation")?;

        let compressed = self._compress_data(&serialized)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = CacheEntry {
            data: compressed,
            created_at: now,
            last_accessed: now,
            size: serialized.len(),
            version: "1.0".to_string(),
            checksum: format!("{:x}", sha2::Sha256::digest(&serialized)),
            metadata: HashMap::new(),
        };

        // Store to disk
        let file_path = self.cache_dir.join(format!("{key}.cache"));
        let entry_bytes = bincode::serialize(&entry).context("Failed to serialize cache entry")?;
        fs::write(&file_path, entry_bytes)?;

        // Store in memory cache
        {
            let mut cache = self.memory_cache.lock().unwrap();

            // Evict old entries if needed
            if cache.len() >= self.config.max_memory_entries {
                self.evict_lru_entries(&mut cache, self.config.max_memory_entries / 4);
            }

            cache.insert(
                key.to_string(),
                CachedItem {
                    data: serialized.clone(),
                    last_accessed: SystemTime::now(),
                    size: serialized.len(),
                },
            );
        }

        debug!("Stored crate documentation for: {}", key);
        self.increment_stat("puts");
        Ok(())
    }

    /// Retrieve crate documentation from cache
    pub fn get_crate_docs(&self, key: &str) -> Result<Option<CrateDocumentation>> {
        // Check memory cache first
        {
            let mut cache = self.memory_cache.lock().unwrap();
            if let Some(item) = cache.get_mut(key) {
                item.last_accessed = SystemTime::now();
                let docs: CrateDocumentation = bincode::deserialize(&item.data)
                    .context("Failed to deserialize cached documentation")?;
                debug!("Cache hit (memory) for: {}", key);
                self.increment_stat("hits");
                return Ok(Some(docs));
            }
        }

        // Check disk cache
        let file_path = self.cache_dir.join(format!("{key}.cache"));
        if file_path.exists() {
            let entry_bytes = fs::read(&file_path)?;
            let mut entry: CacheEntry =
                bincode::deserialize(&entry_bytes).context("Failed to deserialize cache entry")?;

            let decompressed = self._decompress_data(&entry.data)?;
            let docs: CrateDocumentation = bincode::deserialize(&decompressed)
                .context("Failed to deserialize cached documentation")?;

            // Update last accessed time
            entry.last_accessed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let updated_entry_bytes =
                bincode::serialize(&entry).context("Failed to serialize updated cache entry")?;
            fs::write(&file_path, updated_entry_bytes)?;

            // Add to memory cache
            {
                let mut cache = self.memory_cache.lock().unwrap();
                cache.insert(
                    key.to_string(),
                    CachedItem {
                        data: decompressed.clone(),
                        last_accessed: SystemTime::now(),
                        size: decompressed.len(),
                    },
                );
            }

            debug!("Cache hit (disk) for: {}", key);
            self.increment_stat("hits");
            return Ok(Some(docs));
        }

        debug!("Cache miss for: {}", key);
        self.increment_stat("misses");
        Ok(None)
    }

    /// Store generic data in cache
    pub fn store_data(&self, category: &str, key: &str, data: &[u8]) -> Result<()> {
        let compressed = self._compress_data(data)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = CacheEntry {
            data: compressed,
            created_at: now,
            last_accessed: now,
            size: data.len(),
            version: "1.0".to_string(),
            checksum: format!("{:x}", sha2::Sha256::digest(data)),
            metadata: HashMap::new(),
        };

        let entry_bytes = bincode::serialize(&entry).context("Failed to serialize cache entry")?;

        let file_path = self.cache_dir.join(format!("{category}_{key}.cache"));
        fs::write(&file_path, entry_bytes)?;

        debug!("Stored data for: {}:{}", category, key);
        self.increment_stat("puts");
        Ok(())
    }

    /// Retrieve generic data from cache
    pub fn get_data(&self, category: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let file_path = self.cache_dir.join(format!("{category}_{key}.cache"));

        if file_path.exists() {
            let entry_bytes = fs::read(&file_path)?;
            let mut entry: CacheEntry =
                bincode::deserialize(&entry_bytes).context("Failed to deserialize cache entry")?;

            let decompressed = self._decompress_data(&entry.data)?;

            // Update last accessed time
            entry.last_accessed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let updated_entry_bytes =
                bincode::serialize(&entry).context("Failed to serialize updated cache entry")?;
            fs::write(&file_path, updated_entry_bytes)?;

            debug!("Retrieved data for: {}:{}", category, key);
            self.increment_stat("hits");
            return Ok(Some(decompressed));
        }

        debug!("Data not found for: {}:{}", category, key);
        self.increment_stat("misses");
        Ok(None)
    }

    /// Remove an entry from cache
    pub fn remove(&self, category: &str, key: &str) -> Result<bool> {
        // Remove from memory cache
        {
            let mut cache = self.memory_cache.lock().unwrap();
            cache.remove(key);
        }

        // Remove from disk cache
        let file_path = self.cache_dir.join(format!("{category}_{key}.cache"));
        let existed = file_path.exists();
        if existed {
            fs::remove_file(&file_path)?;
            debug!("Removed cache entry: {}:{}", category, key);
        }

        Ok(existed)
    }

    /// Clear all cache entries
    pub fn clear(&self) -> Result<()> {
        // Clear memory cache
        {
            let mut cache = self.memory_cache.lock().unwrap();
            cache.clear();
        }

        // Clear disk cache
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "cache") {
                fs::remove_file(path)?;
            }
        }

        info!("Cleared all cache entries");
        Ok(())
    }

    /// Get cache statistics (legacy call now delegates to enhanced stats).
    /// This preserves backward compatibility while ensuring callers
    /// receive fully instrumented metrics.
    pub fn get_stats(&self) -> Result<CacheStatistics> {
        self.get_enhanced_stats()
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&self, max_age: Duration) -> Result<usize> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - max_age.as_secs();

        let mut removed_count = 0;

        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "cache") {
                if let Ok(entry_bytes) = fs::read(&path) {
                    if let Ok(cache_entry) = bincode::deserialize::<CacheEntry>(&entry_bytes) {
                        if cache_entry.last_accessed < cutoff {
                            fs::remove_file(&path)?;
                            removed_count += 1;
                        }
                    }
                }
            }
        }

        if removed_count > 0 {
            info!("Cleaned up {} expired cache entries", removed_count);
        }

        Ok(removed_count)
    }

    /// Compress data using gzip
    pub fn _compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        let compressed = encoder.finish()?;
        Ok(compressed)
    }

    /// Decompress gzip-compressed data
    pub fn _decompress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = GzDecoder::new(data);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out)?;
        Ok(out)
    }

    /// Evict LRU entries from memory cache
    fn evict_lru_entries(&self, cache: &mut HashMap<String, CachedItem>, count: usize) {
        let mut entries: Vec<_> = cache
            .iter()
            .map(|(k, v)| (k.clone(), v.last_accessed))
            .collect();
        entries.sort_by_key(|(_, last_accessed)| *last_accessed);

        for (key, _) in entries.into_iter().take(count) {
            cache.remove(&key);
        }

        debug!("Evicted {} entries from memory cache", count);
    }

    /// Store individual item documentation in cache
    pub fn store_item_doc(
        &self,
        crate_name: &str,
        version: &str,
        item_path: &str,
        item_doc: &ItemDoc,
    ) -> Result<()> {
        let cache_key = format!("{crate_name}@{version}::{item_path}");

        let entry = ItemCacheEntry {
            item_doc: item_doc.clone(),
            cached_at: SystemTime::now(),
            version: version.to_string(),
            etag: None,
        };

        // Store in memory cache
        {
            let mut cache = self.item_cache.lock().unwrap();

            // Evict old entries if needed
            if cache.len() >= self.config.max_memory_entries {
                self.evict_item_cache_entries(&mut cache, self.config.max_memory_entries / 4);
            }

            cache.insert(cache_key.clone(), entry.clone());
        }

        // Store to disk
        let file_path = self
            .cache_dir
            .join("items")
            .join(format!("{}.item", self.sanitize_filename(&cache_key)));
        let serialized = bincode::serialize(&entry)?;
        let compressed = if self.config.enable_compression {
            self._compress_data(&serialized)?
        } else {
            serialized
        };

        fs::write(&file_path, compressed)?;

        debug!("Stored item documentation for: {}", cache_key);
        self.increment_stat("puts");
        Ok(())
    }

    /// Retrieve individual item documentation from cache
    pub fn get_item_doc(
        &self,
        crate_name: &str,
        version: &str,
        item_path: &str,
    ) -> Result<Option<ItemDoc>> {
        let cache_key = format!("{crate_name}@{version}::{item_path}");

        // Check memory cache first
        {
            let cache = self.item_cache.lock().unwrap();
            if let Some(entry) = cache.get(&cache_key) {
                debug!("Item cache hit (memory) for: {}", cache_key);
                self.increment_stat("hits");
                return Ok(Some(entry.item_doc.clone()));
            }
        }

        // Check disk cache
        let file_path = self
            .cache_dir
            .join("items")
            .join(format!("{}.item", self.sanitize_filename(&cache_key)));
        if file_path.exists() {
            let compressed = fs::read(&file_path)?;
            let serialized = if self.config.enable_compression {
                self._decompress_data(&compressed)?
            } else {
                compressed
            };

            if let Ok(entry) = bincode::deserialize::<ItemCacheEntry>(&serialized) {
                // Add to memory cache
                {
                    let mut cache = self.item_cache.lock().unwrap();
                    cache.insert(cache_key.clone(), entry.clone());
                }

                debug!("Item cache hit (disk) for: {}", cache_key);
                self.increment_stat("hits");
                return Ok(Some(entry.item_doc));
            }
        }

        debug!("Item cache miss for: {}", cache_key);
        self.increment_stat("misses");
        Ok(None)
    }

    /// Store crate-level search index data
    pub fn store_crate_index(
        &self,
        crate_name: &str,
        version: &str,
        search_data: &SearchIndexData,
    ) -> Result<()> {
        let cache_key = format!("{crate_name}@{version}");

        let entry = CrateCacheEntry {
            crate_name: crate_name.to_string(),
            version: version.to_string(),
            search_index_data: Some(search_data.clone()),
            cached_at: SystemTime::now(),
            last_verified: SystemTime::now(),
        };

        // Store in memory cache
        {
            let mut cache = self.crate_cache.lock().unwrap();
            cache.insert(cache_key.clone(), entry.clone());
        }

        // Store to disk
        let file_path = self
            .cache_dir
            .join("crates")
            .join(format!("{}.crate", self.sanitize_filename(&cache_key)));
        let serialized = bincode::serialize(&entry)?;
        let compressed = if self.config.enable_compression {
            self._compress_data(&serialized)?
        } else {
            serialized
        };

        fs::write(&file_path, compressed)?;

        debug!("Stored crate index for: {}", cache_key);
        self.increment_stat("puts");
        Ok(())
    }

    /// Retrieve crate-level search index data
    pub fn get_crate_index(
        &self,
        crate_name: &str,
        version: &str,
    ) -> Result<Option<SearchIndexData>> {
        let cache_key = format!("{crate_name}@{version}");

        // Check memory cache first
        {
            let cache = self.crate_cache.lock().unwrap();
            if let Some(entry) = cache.get(&cache_key) {
                if let Some(ref search_data) = entry.search_index_data {
                    debug!("Crate cache hit (memory) for: {}", cache_key);
                    self.increment_stat("hits");
                    return Ok(Some(search_data.clone()));
                }
            }
        }

        // Check disk cache
        let file_path = self
            .cache_dir
            .join("crates")
            .join(format!("{}.crate", self.sanitize_filename(&cache_key)));
        if file_path.exists() {
            let compressed = fs::read(&file_path)?;
            let serialized = if self.config.enable_compression {
                self._decompress_data(&compressed)?
            } else {
                compressed
            };

            if let Ok(entry) = bincode::deserialize::<CrateCacheEntry>(&serialized) {
                // Add to memory cache
                {
                    let mut cache = self.crate_cache.lock().unwrap();
                    cache.insert(cache_key.clone(), entry.clone());
                }

                debug!("Crate cache hit (disk) for: {}", cache_key);
                self.increment_stat("hits");
                if let Some(search_data) = entry.search_index_data {
                    return Ok(Some(search_data));
                }
            }
        }

        debug!("Crate cache miss for: {}", cache_key);
        self.increment_stat("misses");
        Ok(None)
    }

    /// Clear all cache entries
    pub fn clear_all(&self) -> Result<CacheOperationResult> {
        let mut items_affected = 0;
        let mut size_freed = 0u64;

        // Clear memory caches
        {
            let mut memory_cache = self.memory_cache.lock().unwrap();
            items_affected += memory_cache.len();
            memory_cache.clear();
        }
        {
            let mut item_cache = self.item_cache.lock().unwrap();
            items_affected += item_cache.len();
            item_cache.clear();
        }
        {
            let mut crate_cache = self.crate_cache.lock().unwrap();
            items_affected += crate_cache.len();
            crate_cache.clear();
        }

        // Clear disk cache
        for subdir in &["items", "crates", "indexes"] {
            let dir_path = self.cache_dir.join(subdir);
            if dir_path.exists() {
                for entry in fs::read_dir(&dir_path)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() {
                        if let Ok(metadata) = path.metadata() {
                            size_freed += metadata.len();
                        }
                        fs::remove_file(path)?;
                        items_affected += 1;
                    }
                }
            }
        }

        // Clear legacy cache files
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "cache") {
                if let Ok(metadata) = path.metadata() {
                    size_freed += metadata.len();
                }
                fs::remove_file(path)?;
                items_affected += 1;
            }
        }

        // Reset stats
        {
            let mut stats = self.stats.lock().unwrap();
            *stats = InternalCacheStats::default();
        }

        info!(
            "Cleared all cache entries: {} items, {} bytes freed",
            items_affected, size_freed
        );

        Ok(CacheOperationResult {
            operation: CacheOperation::Clear,
            success: true,
            message: format!("Cleared {items_affected} items, freed {size_freed} bytes"),
            items_affected,
            size_freed_bytes: size_freed,
        })
    }

    /// Clear cache entries for a specific crate
    pub fn clear_crate(&self, crate_name: &str) -> Result<CacheOperationResult> {
        let mut items_affected = 0;
        let mut size_freed = 0u64;

        // Clear from memory caches
        {
            let mut item_cache = self.item_cache.lock().unwrap();
            let keys_to_remove: Vec<String> = item_cache
                .keys()
                .filter(|key| key.starts_with(&format!("{crate_name}@")))
                .cloned()
                .collect();

            for key in keys_to_remove {
                item_cache.remove(&key);
                items_affected += 1;
            }
        }

        {
            let mut crate_cache = self.crate_cache.lock().unwrap();
            let keys_to_remove: Vec<String> = crate_cache
                .keys()
                .filter(|key| key.starts_with(&format!("{crate_name}@")))
                .cloned()
                .collect();

            for key in keys_to_remove {
                crate_cache.remove(&key);
                items_affected += 1;
            }
        }

        // Clear from disk cache
        for subdir in &["items", "crates", "indexes"] {
            let dir_path = self.cache_dir.join(subdir);
            if dir_path.exists() {
                for entry in fs::read_dir(&dir_path)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                            if filename.starts_with(&format!("{crate_name}@")) {
                                if let Ok(metadata) = path.metadata() {
                                    size_freed += metadata.len();
                                }
                                fs::remove_file(path)?;
                                items_affected += 1;
                            }
                        }
                    }
                }
            }
        }

        info!(
            "Cleared cache for crate {}: {} items, {} bytes freed",
            crate_name, items_affected, size_freed
        );

        Ok(CacheOperationResult {
            operation: CacheOperation::Delete,
            success: true,
            message: format!(
                "Cleared crate {crate_name} cache: {items_affected} items, {size_freed} bytes freed"
            ),
            items_affected,
            size_freed_bytes: size_freed,
        })
    }

    /// Get enhanced cache statistics
    pub fn get_enhanced_stats(&self) -> Result<CacheStatistics> {
        let (memory_entries, memory_size, item_entries, crate_entries) = {
            let memory_cache = self.memory_cache.lock().unwrap();
            let item_cache = self.item_cache.lock().unwrap();
            let crate_cache = self.crate_cache.lock().unwrap();

            let memory_size = memory_cache.values().map(|item| item.size as u64).sum();

            (
                memory_cache.len(),
                memory_size,
                item_cache.len(),
                crate_cache.len(),
            )
        };

        // Calculate disk usage
        let disk_size = self.calculate_disk_usage()?;

        // Count disk entries
        let mut disk_entries = 0;
        for subdir in &["items", "crates", "indexes"] {
            let dir_path = self.cache_dir.join(subdir);
            if dir_path.exists() {
                for entry in fs::read_dir(&dir_path)? {
                    if entry?.path().is_file() {
                        disk_entries += 1;
                    }
                }
            }
        }

        let (hits, _misses, evictions, total_requests) = {
            let stats = self.stats.lock().unwrap();
            (
                stats.hits,
                stats.misses,
                stats.evictions,
                stats.total_requests,
            )
        };

        let hit_rate = if total_requests > 0 {
            hits as f64 / total_requests as f64
        } else {
            0.0
        };

        let miss_rate = 1.0 - hit_rate;

        // Calculate oldest entry age
        let oldest_entry_age_hours = self.calculate_oldest_entry_age()?;

        Ok(CacheStatistics {
            total_entries: memory_entries + item_entries + crate_entries + disk_entries,
            memory_entries: memory_entries + item_entries + crate_entries,
            disk_entries,
            total_size_bytes: memory_size + disk_size,
            memory_size_bytes: memory_size,
            disk_size_bytes: disk_size,
            hit_rate,
            miss_rate,
            evictions,
            oldest_entry_age_hours,
        })
    }

    /// Cleanup expired entries based on TTL
    pub fn cleanup_expired_entries(&self) -> Result<CacheOperationResult> {
        let ttl = Duration::from_secs(self.config.entry_ttl_hours * 3600);
        let cutoff = SystemTime::now() - ttl;

        let mut items_affected = 0;
        let mut size_freed = 0u64;

        // Cleanup item cache files
        let items_dir = self.cache_dir.join("items");
        if items_dir.exists() {
            for entry in fs::read_dir(&items_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Ok(metadata) = path.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if modified < cutoff {
                                size_freed += metadata.len();
                                fs::remove_file(&path)?;
                                items_affected += 1;

                                // Also remove from memory cache
                                if let Some(filename) = path.file_stem().and_then(|n| n.to_str()) {
                                    let mut item_cache = self.item_cache.lock().unwrap();
                                    item_cache.remove(filename);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Cleanup crate cache files
        let crates_dir = self.cache_dir.join("crates");
        if crates_dir.exists() {
            for entry in fs::read_dir(&crates_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Ok(metadata) = path.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if modified < cutoff {
                                size_freed += metadata.len();
                                fs::remove_file(&path)?;
                                items_affected += 1;

                                // Also remove from memory cache
                                if let Some(filename) = path.file_stem().and_then(|n| n.to_str()) {
                                    let mut crate_cache = self.crate_cache.lock().unwrap();
                                    crate_cache.remove(filename);
                                }
                            }
                        }
                    }
                }
            }
        }

        if items_affected > 0 {
            info!(
                "Cleaned up {} expired cache entries, freed {} bytes",
                items_affected, size_freed
            );
        }

        Ok(CacheOperationResult {
            operation: CacheOperation::Cleanup,
            success: true,
            message: format!(
                "Cleaned up {items_affected} expired entries, freed {size_freed} bytes"
            ),
            items_affected,
            size_freed_bytes: size_freed,
        })
    }

    /// Sanitize filename for safe filesystem usage
    pub fn sanitize_filename(&self, input: &str) -> String {
        input
            .replace("::", "_")
            .replace("/", "_")
            .replace("\\", "_")
            .replace("<", "_")
            .replace(">", "_")
            .replace(":", "_")
            .replace("\"", "_")
            .replace("|", "_")
            .replace("?", "_")
            .replace("*", "_")
    }

    /// Evict LRU entries from item cache
    fn evict_item_cache_entries(&self, cache: &mut HashMap<String, ItemCacheEntry>, count: usize) {
        let mut entries: Vec<_> = cache
            .iter()
            .map(|(k, v)| (k.clone(), v.cached_at))
            .collect();
        entries.sort_by_key(|(_, cached_at)| *cached_at);

        for (key, _) in entries.into_iter().take(count) {
            cache.remove(&key);
        }

        self.increment_stat("evictions");
        debug!("Evicted {} entries from item cache", count);
    }

    /// Increment cache statistics
    /// Only count GET operations (hits/misses) toward total_requests so that
    /// hit_rate = hits / (hits + misses). Puts/evictions are excluded from the
    /// denominator to avoid skewing accuracy metrics.
    fn increment_stat(&self, stat_type: &str) {
        let mut stats = self.stats.lock().unwrap();
        match stat_type {
            "hits" => {
                stats.hits += 1;
                stats.total_requests += 1;
            }
            "misses" => {
                stats.misses += 1;
                stats.total_requests += 1;
            }
            "evictions" => stats.evictions += 1,
            "puts" => {} // excluded from denominator
            _ => {}
        }
    }

    /// Calculate the age of the oldest cache entry in hours
    fn calculate_oldest_entry_age(&self) -> Result<f64> {
        let mut oldest = SystemTime::now();

        // Check all cache directories
        for subdir in &["items", "crates", "indexes"] {
            let dir_path = self.cache_dir.join(subdir);
            if dir_path.exists() {
                for entry in fs::read_dir(&dir_path)? {
                    let entry = entry?;
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(created) = metadata.created().or_else(|_| metadata.modified()) {
                            if created < oldest {
                                oldest = created;
                            }
                        }
                    }
                }
            }
        }

        let age = SystemTime::now().duration_since(oldest).unwrap_or_default();
        Ok(age.as_secs_f64() / 3600.0)
    }

    /// Calculate disk usage
    fn calculate_disk_usage(&self) -> Result<u64> {
        let mut total_size = 0u64;

        fn visit_dir(dir: &Path, total_size: &mut u64) -> Result<()> {
            if dir.is_dir() {
                for entry in fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_dir() {
                        visit_dir(&path, total_size)?;
                    } else {
                        *total_size += entry.metadata()?.len();
                    }
                }
            }
            Ok(())
        }

        visit_dir(&self.cache_dir, &mut total_size)?;
        Ok(total_size)
    }
}

// Tests moved to separate integration test file: tests/cache_tests.rs
