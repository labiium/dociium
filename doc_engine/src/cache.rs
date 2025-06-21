//! Cache module for storing and retrieving crate documentation

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{debug, info};
use zstd::stream::{Decoder, Encoder};

// Removed unused import

/// Cache for storing crate documentation and metadata
#[derive(Debug)]
pub struct Cache {
    cache_dir: PathBuf,
    memory_cache: Arc<Mutex<HashMap<String, CachedItem>>>,
    max_memory_entries: usize,
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

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub total_size_bytes: u64,
    pub memory_cache_entries: usize,
    pub memory_cache_size_bytes: u64,
    pub hit_rate: f64,
    pub disk_usage_bytes: u64,
}

impl Cache {
    /// Create a new cache instance
    pub fn new(cache_dir: impl AsRef<Path>) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir)?;

        let memory_cache = Arc::new(Mutex::new(HashMap::new()));

        Ok(Self {
            cache_dir,
            memory_cache,
            max_memory_entries: 1000,
        })
    }

    /// Store crate documentation in cache
    pub fn store_crate_docs(&self, key: &str, docs: &crate::CrateDocumentation) -> Result<()> {
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
        let file_path = self.cache_dir.join(format!("{}.cache", key));
        let entry_bytes = bincode::serialize(&entry).context("Failed to serialize cache entry")?;
        fs::write(&file_path, entry_bytes)?;

        // Store in memory cache
        {
            let mut cache = self.memory_cache.lock().unwrap();

            // Evict old entries if needed
            if cache.len() >= self.max_memory_entries {
                self.evict_lru_entries(&mut cache, self.max_memory_entries / 4);
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
        Ok(())
    }

    /// Retrieve crate documentation from cache
    pub fn get_crate_docs(&self, key: &str) -> Result<Option<crate::CrateDocumentation>> {
        // Check memory cache first
        {
            let mut cache = self.memory_cache.lock().unwrap();
            if let Some(item) = cache.get_mut(key) {
                item.last_accessed = SystemTime::now();
                let docs: crate::CrateDocumentation = bincode::deserialize(&item.data)
                    .context("Failed to deserialize cached documentation")?;
                debug!("Cache hit (memory) for: {}", key);
                return Ok(Some(docs));
            }
        }

        // Check disk cache
        let file_path = self.cache_dir.join(format!("{}.cache", key));
        if file_path.exists() {
            let entry_bytes = fs::read(&file_path)?;
            let mut entry: CacheEntry =
                bincode::deserialize(&entry_bytes).context("Failed to deserialize cache entry")?;

            let decompressed = self._decompress_data(&entry.data)?;
            let docs: crate::CrateDocumentation = bincode::deserialize(&decompressed)
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
            return Ok(Some(docs));
        }

        debug!("Cache miss for: {}", key);
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

        let file_path = self.cache_dir.join(format!("{}_{}.cache", category, key));
        fs::write(&file_path, entry_bytes)?;

        debug!("Stored data for: {}:{}", category, key);
        Ok(())
    }

    /// Retrieve generic data from cache
    pub fn get_data(&self, category: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let file_path = self.cache_dir.join(format!("{}_{}.cache", category, key));

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
            return Ok(Some(decompressed));
        }

        debug!("Data not found for: {}:{}", category, key);
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
        let file_path = self.cache_dir.join(format!("{}_{}.cache", category, key));
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
            if path.is_file() && path.extension().map_or(false, |ext| ext == "cache") {
                fs::remove_file(path)?;
            }
        }

        info!("Cleared all cache entries");
        Ok(())
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> Result<CacheStats> {
        let mut total_entries = 0;
        let mut total_size_bytes = 0u64;

        // Count disk cache entries
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "cache") {
                total_entries += 1;
                total_size_bytes += entry.metadata()?.len();
            }
        }

        let (memory_cache_entries, memory_cache_size_bytes) = {
            let cache = self.memory_cache.lock().unwrap();
            let entries = cache.len();
            let size = cache.values().map(|item| item.size as u64).sum();
            (entries, size)
        };

        // Calculate disk usage
        let disk_usage_bytes = self.calculate_disk_usage()?;

        Ok(CacheStats {
            total_entries,
            total_size_bytes,
            memory_cache_entries,
            memory_cache_size_bytes,
            hit_rate: 0.0, // TODO: Implement hit rate tracking
            disk_usage_bytes,
        })
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
            if path.is_file() && path.extension().map_or(false, |ext| ext == "cache") {
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

    /// Compress data using zstd
    fn _compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new(Vec::new(), 3)?;
        std::io::copy(&mut std::io::Cursor::new(data), &mut encoder)?;
        let compressed = encoder.finish()?;
        Ok(compressed)
    }

    /// Decompress zstd-compressed data
    fn _decompress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = Decoder::new(data)?;
        let mut out = Vec::new();
        std::io::copy(&mut decoder, &mut out)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cache_creation() {
        let temp_dir = tempdir().unwrap();
        let cache = Cache::new(temp_dir.path());
        assert!(cache.is_ok());
    }

    #[test]
    fn test_store_and_retrieve_data() {
        let temp_dir = tempdir().unwrap();
        let cache = Cache::new(temp_dir.path()).unwrap();

        let test_data = b"Hello, world!";
        cache.store_data("test", "key1", test_data).unwrap();

        let retrieved = cache.get_data("test", "key1").unwrap();
        assert_eq!(retrieved, Some(test_data.to_vec()));
    }

    #[test]
    fn test_cache_miss() {
        let temp_dir = tempdir().unwrap();
        let cache = Cache::new(temp_dir.path()).unwrap();

        let result = cache.get_data("test", "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_remove_entry() {
        let temp_dir = tempdir().unwrap();
        let cache = Cache::new(temp_dir.path()).unwrap();

        let test_data = b"Hello, world!";
        cache.store_data("test", "key1", test_data).unwrap();

        let existed = cache.remove("test", "key1").unwrap();
        assert!(existed);

        let result = cache.get_data("test", "key1").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_compression() {
        let temp_dir = tempdir().unwrap();
        let cache = Cache::new(temp_dir.path()).unwrap();

        let test_data = b"Hello, world!".repeat(1000);
        let compressed = cache._compress_data(&test_data).unwrap();
        let decompressed = cache._decompress_data(&compressed).unwrap();

        assert_eq!(test_data, decompressed);
        assert!(compressed.len() < test_data.len());
    }
}
