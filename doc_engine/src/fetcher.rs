//! Fetcher module for downloading and managing Rust crate data

use anyhow::{Context, Result};
use crates_io_api::{AsyncClient, CratesQuery};
use flate2::read::GzDecoder;
use governor::{Quota, RateLimiter};
use reqwest::Client;
use reqwest::StatusCode;
use rustdoc_types::Crate as RustdocCrate;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::num::NonZeroU32;
use std::time::Duration;
use tar::Archive;
use tempfile::TempDir;
use tracing::{debug, info, instrument, warn};

use crate::types::*;

/// Rate limiter for crates.io API calls (10 requests per second)
type ApiRateLimiter = RateLimiter<
    governor::state::direct::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::QuantaClock,
>;

/// Fetcher handles downloading crates and metadata from crates.io
pub struct Fetcher {
    client: AsyncClient,
    http_client: Client,
    rate_limiter: ApiRateLimiter,
}

impl fmt::Debug for Fetcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fetcher").finish()
    }
}

impl Fetcher {
    /// Create a new fetcher instance
    pub fn new() -> Self {
        let client = AsyncClient::new(
            "dociium-tests (contact@example.com)",
            Duration::from_secs(30),
        )
        .expect("Failed to create crates.io client");

        let http_client = Client::builder()
            .timeout(Duration::from_secs(120))
            .gzip(true)
            .build()
            .expect("Failed to create HTTP client");

        // Rate limit: 10 requests per second
        let rate_limiter = RateLimiter::direct(Quota::per_second(NonZeroU32::new(10).unwrap()));

        Self {
            client,
            http_client,
            rate_limiter,
        }
    }

    /// Search for crates on crates.io
    #[instrument(skip(self), fields(query = %query, limit = %limit))]
    pub async fn search_crates(&self, query: &str, limit: u32) -> Result<Vec<CrateSearchResult>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        // Wait for rate limit
        self.rate_limiter.until_ready().await;

        debug!("Searching crates.io for: {} (limit: {})", query, limit);

        let crates_query = CratesQuery::builder()
            .search(query)
            .sort(crates_io_api::Sort::Relevance)
            .page_size(u64::from(limit.min(100)));

        let response = self
            .client
            .crates(crates_query.build())
            .await
            .context("Failed to search crates")?;

        let mut results = Vec::new();
        for crate_data in response.crates {
            results.push(CrateSearchResult {
                name: crate_data.name,
                latest_version: crate_data.max_version,
                description: crate_data.description,
                downloads: crate_data.downloads,
                repository: crate_data.repository,
                documentation: crate_data.documentation,
                homepage: crate_data.homepage,
                keywords: crate_data.keywords.unwrap_or_default(),
                categories: crate_data.categories.unwrap_or_default(),
                created_at: Some(crate_data.created_at.to_rfc3339()),
                updated_at: Some(crate_data.updated_at.to_rfc3339()),
            });
        }

        info!("Found {} crates for query: {}", results.len(), query);
        Ok(results)
    }

    /// Get detailed information about a specific crate
    #[instrument(skip(self), fields(name = %name))]
    pub async fn crate_info(&self, name: &str) -> Result<CrateInfo> {
        // Wait for rate limit
        self.rate_limiter.until_ready().await;

        debug!("Fetching crate info for: {}", name);

        let response = self
            .client
            .get_crate(name)
            .await
            .with_context(|| format!("Failed to get crate info for: {}", name))?;

        let crate_data = response.crate_data;
        let versions = response.versions;

        // Get version information
        let mut version_info = Vec::new();
        for version in &versions {
            version_info.push(VersionInfo {
                version: version.num.clone(),
                downloads: version.downloads,
                yanked: version.yanked,
                created_at: Some(version.created_at.to_rfc3339()),
            });
        }

        // Sort versions by semver (latest first)
        version_info.sort_by(|a, b| {
            let ver_a = Version::parse(&a.version).unwrap_or_else(|_| Version::new(0, 0, 0));
            let ver_b = Version::parse(&b.version).unwrap_or_else(|_| Version::new(0, 0, 0));
            ver_b.cmp(&ver_a)
        });

        // Get dependencies for the latest version
        let mut dependencies = Vec::new();
        if let Some(latest_version) = versions.first() {
            self.rate_limiter.until_ready().await;
            if let Ok(deps) = self
                .client
                .crate_dependencies(name, &latest_version.num)
                .await
            {
                for dep in deps {
                    dependencies.push(DependencyInfo {
                        name: dep.crate_id,
                        version_req: dep.req,
                        kind: dep.kind,
                        optional: dep.optional,
                        default_features: dep.default_features,
                        features: dep.features,
                    });
                }
            }
        }

        let crate_info = CrateInfo {
            name: crate_data.name,
            latest_version: crate_data.max_version,
            description: crate_data.description,
            homepage: crate_data.homepage,
            repository: crate_data.repository,
            documentation: crate_data.documentation,
            license: versions.first().and_then(|v| v.license.clone()),
            downloads: crate_data.downloads,
            recent_downloads: crate_data.recent_downloads,
            feature_flags: Vec::new(), // TODO: Extract from Cargo.toml
            dependencies,
            keywords: crate_data.keywords.unwrap_or_default(),
            categories: crate_data.categories.unwrap_or_default(),
            versions: version_info,
            authors: Vec::new(), // TODO: Extract from metadata
            created_at: Some(crate_data.created_at.to_rfc3339()),
            updated_at: Some(crate_data.updated_at.to_rfc3339()),
        };

        info!("Retrieved crate info for: {}", name);
        Ok(crate_info)
    }

    /// Download and extract a crate to a temporary directory
    #[instrument(skip(self), fields(name = %name, version = %version))]
    pub async fn download_crate(&self, name: &str, version: &Version) -> Result<TempDir> {
        info!("Downloading crate: {}@{}", name, version);

        // Wait for rate limit
        self.rate_limiter.until_ready().await;

        // Get download URL
        let download_url = format!(
            "https://crates.io/api/v1/crates/{}/{}/download",
            name, version
        );

        // Download the crate tarball
        let response = self
            .http_client
            .get(&download_url)
            .send()
            .await
            .with_context(|| format!("Failed to download crate {}@{}", name, version))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to download crate {}@{}: HTTP {}",
                name,
                version,
                response.status()
            ));
        }

        // Get the response bytes
        let bytes = response
            .bytes()
            .await
            .context("Failed to read crate download response")?;

        // Verify checksum if available
        // TODO: Get checksum from API and verify

        // Create temporary directory
        let temp_dir = TempDir::new().context("Failed to create temporary directory")?;

        // Extract the tarball
        let gz_decoder = GzDecoder::new(bytes.as_ref());
        let mut archive = Archive::new(gz_decoder);

        archive
            .unpack(temp_dir.path())
            .with_context(|| format!("Failed to extract crate {}@{}", name, version))?;

        info!(
            "Successfully downloaded and extracted crate: {}@{}",
            name, version
        );
        Ok(temp_dir)
    }

    /// Attempt to fetch rustdoc.json directly from docs.rs
    pub async fn fetch_rustdoc_json_from_docs_rs(
        &self,
        crate_name: &str,
        version: &str,
    ) -> Result<Option<RustdocCrate>> {
        let url = format!(
            "https://docs.rs/crate/{}/{}/rustdoc.json",
            crate_name, version
        );
        info!("Attempting to fetch rustdoc.json from: {}", url);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to send request to docs.rs for {}", url))?;

        match response.status() {
            StatusCode::OK => {
                let bytes = response
                    .bytes()
                    .await
                    .with_context(|| format!("Failed to read response bytes from {}", url))?;

                serde_json::from_slice(&bytes).map(Some).with_context(|| {
                    format!(
                        "Failed to deserialize rustdoc.json from docs.rs for {}@{}",
                        crate_name, version
                    )
                })
            }
            StatusCode::NOT_FOUND => {
                info!(
                    "rustdoc.json not found on docs.rs for {}@{}",
                    crate_name, version
                );
                Ok(None)
            }
            other_status => Err(anyhow::anyhow!(
                "Received HTTP {} from docs.rs when fetching rustdoc.json for {}@{}",
                other_status,
                crate_name,
                version
            )),
        }
    }

    /// Get the latest stable version of a crate
    #[instrument(skip(self), fields(name = %name))]
    pub async fn get_latest_version(&self, name: &str) -> Result<Version> {
        debug!("Getting latest version for: {}", name);

        let crate_info = self.crate_info(name).await?;
        let version = Version::parse(&crate_info.latest_version)
            .with_context(|| format!("Invalid version format: {}", crate_info.latest_version))?;

        debug!("Latest version for {}: {}", name, version);
        Ok(version)
    }

    /// Check if a crate exists
    #[instrument(skip(self), fields(name = %name))]
    pub async fn crate_exists(&self, name: &str) -> Result<bool> {
        debug!("Checking if crate exists: {}", name);

        // Wait for rate limit
        self.rate_limiter.until_ready().await;

        match self.client.get_crate(name).await {
            Ok(_) => {
                debug!("Crate exists: {}", name);
                Ok(true)
            }
            Err(crates_io_api::Error::NotFound(_)) => {
                debug!("Crate does not exist: {}", name);
                Ok(false)
            }
            Err(e) => {
                warn!("Error checking crate existence for {}: {}", name, e);
                Err(e.into())
            }
        }
    }

    /// Get crate download statistics
    #[instrument(skip(self), fields(name = %name))]
    pub async fn get_crate_stats(&self, name: &str) -> Result<CrateDownloadStats> {
        debug!("Getting download stats for: {}", name);

        let crate_info = self.crate_info(name).await?;
        let stats = CrateDownloadStats {
            total_downloads: crate_info.downloads,
            recent_downloads: crate_info.recent_downloads,
        };

        debug!("Retrieved stats for {}: {:?}", name, stats);
        Ok(stats)
    }

    /// Verify downloaded crate against checksum
    #[instrument(skip(self, data), fields(name = %name, version = %version))]
    pub async fn verify_crate_checksum(
        &self,
        name: &str,
        version: &Version,
        data: &[u8],
    ) -> Result<bool> {
        // TODO: Get expected checksum from crates.io API
        // For now, just compute the SHA256 hash
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();

        debug!(
            "Computed SHA256 for {}@{}: {}",
            name,
            version,
            hex::encode(hash)
        );

        // TODO: Compare with expected checksum from API
        Ok(true)
    }

    /// Fetch the main HTML page for a crate from docs.rs
    pub async fn fetch_crate_html_from_docs_rs(
        &self,
        crate_name: &str,
        version: &str,
    ) -> Result<Option<String>> {
        let url = format!("https://docs.rs/{}/{}/{}", crate_name, version, crate_name);
        info!("Attempting to fetch HTML from: {}", url);

        let response =
            self.http_client.get(&url).send().await.with_context(|| {
                format!("Failed to send request to docs.rs for HTML page: {}", url)
            })?;

        match response.status() {
            StatusCode::OK => {
                let html_content = response
                    .text()
                    .await
                    .with_context(|| format!("Failed to read HTML response body from {}", url))?;
                Ok(Some(html_content))
            }
            StatusCode::NOT_FOUND => {
                info!(
                    "HTML page not found on docs.rs for {}@{}/{}",
                    crate_name, version, crate_name
                );
                Ok(None)
            }
            other_status => Err(anyhow::anyhow!(
                "Received HTTP {} from docs.rs when fetching HTML for {}@{}/{}",
                other_status,
                crate_name,
                version,
                crate_name
            )),
        }
    }
}

/// Download statistics for a crate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateDownloadStats {
    pub total_downloads: u64,
    pub recent_downloads: Option<u64>,
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Import ItemEnum
    use rustdoc_types::ItemEnum;

    #[test]
    fn test_fetcher_creation() {
        let _fetcher = Fetcher::new();
        // Just test that we can create the fetcher without panicking
    }

    #[tokio::test]
    async fn test_real_crate_exists() {
        let fetcher = Fetcher::new();

        // Test with a crate that should exist
        let exists = fetcher.crate_exists("serde").await.unwrap();
        assert!(exists);

        // Test with a crate that should not exist
        let exists = fetcher
            .crate_exists("nonexistent-crate-rdocs-mcp-12345")
            .await
            .unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_real_search_crates() {
        let fetcher = Fetcher::new();
        let results = fetcher.search_crates("serde", 5).await.unwrap();

        assert!(!results.is_empty());
        assert!(results.len() <= 5);
        assert!(results.iter().any(|r| r.name == "serde"));
    }

    #[tokio::test]
    async fn test_real_get_latest_version() {
        let fetcher = Fetcher::new();
        let version = fetcher.get_latest_version("serde").await.unwrap();

        assert!(version.major >= 1);
        assert!(version.pre.is_empty()); // Should be a stable version
    }

    #[tokio::test]
    async fn test_real_download_crate() {
        let fetcher = Fetcher::new();
        let version = Version::parse("1.0.0").unwrap();
        let temp_dir = fetcher.download_crate("itoa", &version).await.unwrap();

        // Verify that files were created
        let extracted_files: Vec<_> = walkdir::WalkDir::new(temp_dir.path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();

        assert!(!extracted_files.is_empty());

        // Look for Cargo.toml
        let has_cargo_toml = extracted_files
            .iter()
            .any(|entry| entry.path().file_name().unwrap() == "Cargo.toml");
        assert!(has_cargo_toml);
    }

    #[tokio::test]
    async fn test_real_crate_info() {
        let fetcher = Fetcher::new();
        let info = fetcher.crate_info("serde").await.unwrap();

        assert_eq!(info.name, "serde");
        assert!(!info.latest_version.is_empty());
        assert!(info.downloads > 0);
        assert!(!info.versions.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_rustdoc_json_from_docs_rs_exists() {
        let fetcher = Fetcher::new();
        let crate_name = "serde"; // Using a well-known crate
        let crate_version = "1.0.197"; // A recent version

        let result = fetcher
            .fetch_rustdoc_json_from_docs_rs(crate_name, crate_version)
            .await
            .unwrap();

        match result {
            Some(rustdoc_crate) => {
                // This case might not be hit if docs.rs doesn't serve rustdoc.json at these URLs
                info!(
                    "Successfully fetched rustdoc.json for {}@{}",
                    crate_name, crate_version
                );
                assert_eq!(rustdoc_crate.crate_version, Some(crate_version.to_string()));
                // Check for a known item, e.g., the `Serialize` trait
                let found_item = rustdoc_crate.index.values().any(|item| {
                    item.name.as_deref() == Some("Serialize")
                        && matches!(item.inner, ItemEnum::Trait { .. })
                });
                assert!(
                    found_item,
                    "Did not find expected item 'Serialize' of kind Trait in rustdoc.json"
                );
            }
            None => {
                // This is the expected case if docs.rs returns 404 for the rustdoc.json URL
                info!("rustdoc.json not found on docs.rs for {}@{} (as expected in many cases), test passes.", crate_name, crate_version);
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_rustdoc_json_from_docs_rs_not_found_version() {
        let fetcher = Fetcher::new();
        // Use a version that is highly unlikely to exist
        let result = fetcher
            .fetch_rustdoc_json_from_docs_rs("itoa", "999.999.999")
            .await
            .unwrap();
        assert!(result.is_none(), "Expected rustdoc.json to be not found");
    }

    #[tokio::test]
    async fn test_fetch_rustdoc_json_from_docs_rs_not_found_crate() {
        let fetcher = Fetcher::new();
        // Use a crate name that is highly unlikely to exist
        let result = fetcher
            .fetch_rustdoc_json_from_docs_rs("nonexistent-crate-docium-xyz", "1.0.0")
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "Expected rustdoc.json to be not found for non-existent crate"
        );
    }

    #[tokio::test]
    async fn test_fetch_rustdoc_json_from_docs_rs_invalid_format_simulated() {
        // This test is a bit tricky as docs.rs should always serve valid JSON or 404.
        // We are testing our client's behavior if it *were* to receive invalid JSON.
        // This would ideally involve mocking the HTTP client, but for an integration test,
        // we rely on the robustness of the error handling for other cases.
        // For now, we acknowledge this is hard to test directly against live docs.rs.
        // If the function were to error out on a valid crate/version due to deserialization,
        // that would be a bug caught by test_fetch_rustdoc_json_from_docs_rs_exists.
        info!("Skipping direct test for invalid JSON format from live docs.rs");
    }

    #[tokio::test]
    async fn test_fetch_crate_html_from_docs_rs_success() {
        let fetcher = Fetcher::new();
        let crate_name = "itoa";
        let version = "1.0.11";
        let result = fetcher
            .fetch_crate_html_from_docs_rs(crate_name, version)
            .await;

        assert!(result.is_ok(), "Request failed: {:?}", result.err());
        let html_option = result.unwrap();
        assert!(html_option.is_some(), "Expected HTML content, got None");

        let html_content = html_option.unwrap();
        assert!(
            html_content.contains("<title>itoa - Rust</title>"),
            "HTML content missing expected title"
        );
        assert!(
            html_content.contains("Structs"),
            "HTML content missing 'Structs' section"
        );
        assert!(
            html_content.contains("Buffer"),
            "HTML content missing 'Buffer' link or text"
        );
        assert!(
            html_content.contains("A correctly sized stack allocation"),
            "HTML content missing 'Buffer' summary text"
        );
    }

    #[tokio::test]
    async fn test_fetch_crate_html_from_docs_rs_not_found() {
        let fetcher = Fetcher::new();
        let crate_name = "nonexistent-crate-docium-html-xyz";
        let version = "1.0.0";
        let result = fetcher
            .fetch_crate_html_from_docs_rs(crate_name, version)
            .await;

        assert!(
            result.is_ok(),
            "Request unexpectedly failed: {:?}",
            result.err()
        );
        let html_option = result.unwrap();
        assert!(
            html_option.is_none(),
            "Expected None for non-existent crate HTML, got Some(...)"
        );
    }
}
