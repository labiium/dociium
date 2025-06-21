//! Cache module for storing and retrieving crate documentation

use anyhow::{Context, Result};
use crc32fast::Hasher as Crc32Hasher; // For CRC32 checksum
use moka::future::Cache as MokaCache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256}; // Keep Sha256 for checksum
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{debug, info, warn};
use zstd::stream::{Decoder as ZstdDecoder, Encoder as ZstdEncoder};

/// Cache for storing crate documentation and metadata
#[derive(Debug, Clone)] // Clone so DocEngine can clone it
pub struct Cache {
    cache_dir: PathBuf,
    // `Arc` is not strictly necessary for MokaCache if Cache itself is Arc'd in DocEngine,
    // but MokaCache is already thread-safe (Arc<Inner>)
    memory_cache: MokaCache<String, Arc<crate::CrateDocumentation>>, // Store Arc<CrateDocumentation> directly
    // Stats
    hits: Arc<std::sync::atomic::AtomicU64>,
    misses: Arc<std::sync::atomic::AtomicU64>,
}

/// Disk cache entry metadata (stored alongside the compressed data or as part of a manifest)
/// For CrateDocumentation, the file name itself might be `crate_name@version.json.zst`
/// This struct would be for a separate metadata file if we don't embed it.
/// Or, if we store raw bytes (like tarballs), this metadata would be useful.
/// For CrateDocumentation, we serialize it, then compress.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskCacheItemMetadata {
    original_size: usize,
    stored_size: usize,
    created_at: u64,
    last_accessed: u64,
    version: String, // e.g., schema version of CrateDocumentation or this metadata
    sha256_checksum: String,
    crc32_checksum: u32,
    // Other potential metadata: content_type, encoding, etc.
}

/// Serializable cache entry for on-disk storage of CrateDocumentation
/// This will be what's actually written to a file like `crate@version.json.zst`
/// (though it's bincode, not json, then zstd).
/// The spec says `cache/docs/*.bin.zst`. So we'll bincode::serialize, then zstd::encode.
/// The metadata might be stored separately or not at all if filenames are informative enough.
/// For simplicity, let's assume the file itself is the compressed CrateDocumentation,
/// and metadata like checksums are handled during store/load.
/// The `CacheEntry` struct from before might be too complex if we simplify.

// Let's simplify: the file on disk IS the zstd-compressed bincode-serialized CrateDocumentation.
// Checksums will be verified on load. We might need a small metadata file next to it
// if we need to store things like original_size without decompressing, or the checksums themselves.

// For CacheStats, we'll need to iterate directory for disk usage.

/// Statistics about the cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of CrateDocumentation entries currently on disk in the 'docs' directory.
    pub total_docs_on_disk_entries: usize,
    /// Total size of CrateDocumentation entries on disk in the 'docs' directory (compressed size).
    pub total_docs_on_disk_size_bytes: u64,
    /// Number of CrateDocumentation entries currently in the memory cache.
    pub memory_cache_docs_entries: usize,
    // pub memory_cache_size_bytes: u64, // Moka does not easily expose byte size for complex Arc<T> values
    /// Hit rate for memory/disk lookups (hits / (hits + misses)).
    pub hit_rate: f64,
    /// Total disk usage of the entire rdocs-mcp cache directory (includes docs, symbols, traits, meta, tarballs).
    pub total_disk_usage_bytes: u64,
}

// The old CacheEntry struct is no longer used with the new Moka + direct file storage approach.
// If we need to store metadata next to files, DiskCacheItemMetadata would be used.

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
    pub fn new(base_cache_dir: impl AsRef<Path>) -> Result<Self> {
        let base_cache_dir = base_cache_dir.as_ref().to_path_buf();
        let docs_cache_dir = base_cache_dir.join("docs");
        fs::create_dir_all(&docs_cache_dir).with_context(|| {
            format!(
                "Failed to create docs cache directory at {:?}",
                docs_cache_dir
            )
        })?;

        // Configure Moka cache (e.g., max capacity, time to live)
        // Max capacity is in number of entries.
        // Estimate CrateDocumentation size: can vary wildly. Let's say average 1MB serialized.
        // For 100MB memory cache, that's ~100 entries.
        let memory_cache = MokaCache::builder()
            .max_capacity(200) // Max 200 CrateDocumentation objects in memory
            .time_to_live(Duration::from_secs(2 * 60 * 60)) // 2 hours TTL
            .time_to_idle(Duration::from_secs(30 * 60)) // 30 minutes TTI
            .build();

        Ok(Self {
            cache_dir: docs_cache_dir, // Store path to specific 'docs' subdirectory
            memory_cache,
            hits: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            misses: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }

    fn get_disk_path(&self, key: &str) -> PathBuf {
        // key is typically "crate_name@version"
        self.cache_dir.join(format!("{}.bin.zst", key))
    }

    /// Store crate documentation in cache
    pub async fn store_crate_docs(
        &self,
        key: &str,
        docs: Arc<crate::CrateDocumentation>,
    ) -> Result<()> {
        // 1. Serialize CrateDocumentation using bincode
        let serialized_docs = bincode::serialize(docs.as_ref())
            .context("Failed to serialize crate documentation for caching")?;

        let original_size = serialized_docs.len();

        // 2. Compute checksums of serialized (uncompressed) data
        let mut crc_hasher = Crc32Hasher::new();
        crc_hasher.update(&serialized_docs);
        let crc32_checksum = crc_hasher.finalize();

        let sha256_checksum = format!("{:x}", Sha256::digest(&serialized_docs));

        // 3. Compress serialized data using zstd
        let mut encoder = ZstdEncoder::new(Vec::new(), 3) // ZSTD level 3
            .context("Failed to create zstd encoder")?;
        std::io::copy(&mut serialized_docs.as_slice(), &mut encoder)
            .context("Failed to copy data to zstd encoder")?;
        let compressed_data = encoder.finish().context("Failed to finish zstd encoding")?;
        let stored_size = compressed_data.len();

        // 4. Store compressed data to disk
        let file_path = self.get_disk_path(key);
        fs::write(&file_path, &compressed_data)
            .with_context(|| format!("Failed to write cached docs to disk at {:?}", file_path))?;

        // Optional: Store metadata (including checksums) to a separate .meta file or in a DB
        // For now, checksums are computed but not stored separately; they'd be re-verified on load.
        // If we want to store them, DiskCacheItemMetadata would be used here.
        debug!(
            "Stored crate docs for '{}': original_size={}, stored_size={}, crc32={}, sha256={}",
            key, original_size, stored_size, crc32_checksum, sha256_checksum
        );

        // 5. Add to memory cache (Moka stores Arc directly)
        self.memory_cache.insert(key.to_string(), docs).await;
        // Moka's insert invalidates and drops the old value if the key already exists.

        Ok(())
    }

    /// Retrieve crate documentation from cache
    pub async fn get_crate_docs(
        &self,
        key: &str,
    ) -> Result<Option<Arc<crate::CrateDocumentation>>> {
        // 1. Check memory cache first
        if let Some(docs) = self.memory_cache.get(key).await {
            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            debug!("Memory cache hit for: {}", key);
            return Ok(Some(docs));
        }

        // 2. Check disk cache
        let file_path = self.get_disk_path(key);
        if file_path.exists() {
            let compressed_data = fs::read(&file_path).with_context(|| {
                format!("Failed to read cached docs from disk at {:?}", file_path)
            })?;

            let mut decoder = ZstdDecoder::new(compressed_data.as_slice())
                .context("Failed to create zstd decoder")?;
            let mut decompressed_data = Vec::new();
            std::io::copy(&mut decoder, &mut decompressed_data)
                .context("Failed to decompress cached docs")?;

            // Verify checksums (optional, but good practice)
            let mut crc_hasher = Crc32Hasher::new();
            crc_hasher.update(&decompressed_data);
            let _crc32_checksum = crc_hasher.finalize(); // TODO: Compare if stored

            let _sha256_checksum = format!("{:x}", Sha256::digest(&decompressed_data)); // TODO: Compare if stored
                                                                                        // For now, we're not failing on checksum mismatch as we don't store them yet.
                                                                                        // This would be where you load DiskCacheItemMetadata and compare.

            let docs: crate::CrateDocumentation = bincode::deserialize(&decompressed_data)
                .context("Failed to deserialize cached crate documentation")?;

            let arc_docs = Arc::new(docs);
            self.memory_cache
                .insert(key.to_string(), Arc::clone(&arc_docs))
                .await;

            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            debug!("Disk cache hit for: {}", key);
            return Ok(Some(arc_docs));
        }

        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        debug!("Cache miss for: {}", key);
        Ok(None)
    }

    // /// Store generic data in cache (Example, can be re-implemented if needed)
    // pub async fn store_generic_data(&self, category: &str, key: &str, data: &[u8]) -> Result<()> { ... }
    // /// Retrieve generic data from cache (Example, can be re-implemented if needed)
    // pub async fn get_generic_data(&self, category: &str, key: &str) -> Result<Option<Vec<u8>>> { ... }

    /// Remove a specific CrateDocumentation entry from memory and disk cache.
    pub async fn remove_crate_docs(&self, key: &str) -> Result<bool> {
        self.memory_cache.invalidate(key).await; // Remove from Moka cache

        let file_path = self.get_disk_path(key);
        let existed_on_disk = file_path.exists();
        if existed_on_disk {
            fs::remove_file(&file_path).with_context(|| {
                format!("Failed to remove cached docs from disk at {:?}", file_path)
            })?;
            debug!("Removed disk cache entry for: {}", key);
        }
        Ok(existed_on_disk)
    }

    /// Clear all cache entries (both memory and disk for CrateDocumentation).
    pub async fn clear_all_crate_docs(&self) -> Result<()> {
        self.memory_cache.invalidate_all();
        // For Moka, clear() is sync, invalidate_all() is async if using_eventual_consistency.
        // For a full clear, re-initializing might be simpler or use run_pending_tasks after invalidate_all.
        // self.memory_cache.run_pending_tasks().await; // Ensure invalidations are processed for tests.

        // Clear disk cache (only .bin.zst files in the docs directory)
        let entries = fs::read_dir(&self.cache_dir)
            .with_context(|| format!("Failed to read cache directory {:?}", self.cache_dir))?;

        for entry in entries {
            let entry = entry.with_context(|| "Failed to read directory entry")?;
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "zst") {
                if path
                    .file_name()
                    .map_or(false, |name| name.to_string_lossy().ends_with(".bin.zst"))
                {
                    fs::remove_file(&path)
                        .with_context(|| format!("Failed to remove disk cache file {:?}", path))?;
                }
            }
        }
        self.hits.store(0, std::sync::atomic::Ordering::Relaxed);
        self.misses.store(0, std::sync::atomic::Ordering::Relaxed);
        info!("Cleared all CrateDocumentation cache entries.");
        Ok(())
    }

    /// Get cache statistics.
    pub async fn get_stats(&self) -> Result<CacheStats> {
        let mut disk_docs_entries = 0;
        let mut disk_docs_size_bytes = 0u64;

        // Calculate stats for the 'docs' cache directory
        for entry in fs::read_dir(&self.cache_dir)
            .with_context(|| format!("Failed to read docs cache directory {:?}", self.cache_dir))?
        {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file()
                    && path.extension().map_or(false, |ext| ext == "zst")
                    && path
                        .file_name()
                        .map_or(false, |name| name.to_string_lossy().ends_with(".bin.zst"))
                {
                    disk_docs_entries += 1;
                    if let Ok(metadata) = entry.metadata() {
                        disk_docs_size_bytes += metadata.len();
                    }
                }
            }
        }

        let mem_hits = self.hits.load(std::sync::atomic::Ordering::Relaxed);
        let mem_misses = self.misses.load(std::sync::atomic::Ordering::Relaxed);
        let total_lookups = mem_hits + mem_misses;
        let hit_rate = if total_lookups == 0 {
            0.0
        } else {
            mem_hits as f64 / total_lookups as f64
        };

        // Moka doesn't easily give byte size. We report entry count.
        let memory_cache_entries = self.memory_cache.entry_count() as usize;

        // Total disk usage of the entire base cache directory (docs, symbols, traits, meta, tarballs)
        // This requires `self.cache_dir` to be the *base* cache dir, not the 'docs' subdir.
        // Let's adjust Cache to store base_cache_dir and derive docs_cache_dir from it for this.
        // For now, this will only calculate size of 'docs' dir.
        // To do it properly, `calculate_disk_usage` needs the true base path.
        // The current self.cache_dir is `.../docs`. So we need parent.
        let base_cache_dir = self
            .cache_dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Cache dir has no parent"))?;
        let total_disk_usage_bytes = calculate_recursive_disk_usage(base_cache_dir)?;

        Ok(CacheStats {
            total_docs_on_disk_entries: disk_docs_entries,
            total_docs_on_disk_size_bytes: disk_docs_size_bytes,
            memory_cache_docs_entries: memory_cache_entries,
            // memory_cache_size_bytes: 0, // Moka does not expose this easily for complex types.
            hit_rate,
            total_disk_usage_bytes, // Total for the whole rdocs-mcp cache area
        })
    }

    // cleanup_expired and specific methods for tarballs etc. would be added here.
    // For CrateDocumentation, Moka handles TTL/TTI for memory.
    // Disk cleanup would need to iterate files and check mod times or stored metadata.
}

/// Helper to calculate disk usage of a directory recursively.
fn calculate_recursive_disk_usage(path: &Path) -> Result<u64> {
    let mut total_size = 0;
    let entries = fs::read_dir(path)
        .with_context(|| format!("Failed to read directory for size calculation: {:?}", path))?;

    for entry in entries {
        let entry = entry.with_context(|| "Failed to read directory entry for size calculation")?;
        let path = entry.path();
        if path.is_dir() {
            total_size += calculate_recursive_disk_usage(&path)?;
        } else if let Ok(metadata) = entry.metadata() {
            total_size += metadata.len();
        }
    }
    Ok(total_size)
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
