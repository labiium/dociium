//! Fetcher module for downloading and managing Rust crate data

use anyhow::Result;
use semver::Version;
use serde::{Deserialize, Serialize};

use tempfile::TempDir;
use tracing::{debug, info};

use crate::types::*;

/// Fetcher handles downloading crates and metadata from crates.io
#[derive(Debug)]
pub struct Fetcher {
    _placeholder: (),
}

impl Fetcher {
    /// Create a new fetcher instance
    pub fn new() -> Self {
        Self { _placeholder: () }
    }

    /// Search for crates on crates.io (mock implementation)
    pub async fn search_crates(&self, query: &str, limit: u32) -> Result<Vec<CrateSearchResult>> {
        debug!("Mock searching crates.io for: {} (limit: {})", query, limit);

        let mut results = Vec::new();

        if !query.is_empty() && limit > 0 {
            // Return mock results for demonstration
            for i in 0..std::cmp::min(limit, 3) {
                results.push(CrateSearchResult {
                    name: format!("{}-mock-{}", query, i + 1),
                    latest_version: "1.0.0".to_string(),
                    description: Some(format!("Mock crate for {} search", query)),
                    downloads: 1000 * (i + 1) as u64,
                    repository: Some(format!("https://github.com/mock/{}-mock-{}", query, i + 1)),
                    documentation: Some(format!("https://docs.rs/{}-mock-{}", query, i + 1)),
                    homepage: None,
                    keywords: vec![query.to_string(), "mock".to_string()],
                    categories: vec!["development-tools".to_string()],
                    created_at: Some("2023-01-01T00:00:00Z".to_string()),
                    updated_at: Some("2024-01-01T00:00:00Z".to_string()),
                });
            }
        }

        info!("Found {} mock crates for query: {}", results.len(), query);
        Ok(results)
    }

    /// Get detailed information about a specific crate (mock implementation)
    pub async fn crate_info(&self, name: &str) -> Result<CrateInfo> {
        debug!("Mock fetching crate info for: {}", name);

        let crate_info = CrateInfo {
            name: name.to_string(),
            latest_version: "1.0.0".to_string(),
            description: Some(format!("Mock crate description for {}", name)),
            homepage: Some(format!("https://{}.rs", name)),
            repository: Some(format!("https://github.com/mock/{}", name)),
            documentation: Some(format!("https://docs.rs/{}", name)),
            license: Some("MIT OR Apache-2.0".to_string()),
            downloads: 50000,
            recent_downloads: Some(1500),
            feature_flags: vec!["default".to_string(), "serde".to_string()],
            dependencies: vec![DependencyInfo {
                name: "serde".to_string(),
                version_req: "1.0".to_string(),
                kind: "normal".to_string(),
                optional: false,
                default_features: true,
                features: vec![],
            }],
            keywords: vec!["rust".to_string(), "library".to_string()],
            categories: vec!["development-tools".to_string()],
            versions: vec![
                VersionInfo {
                    version: "1.0.0".to_string(),
                    downloads: 50000,
                    yanked: false,
                    created_at: Some("2023-01-01T00:00:00Z".to_string()),
                },
                VersionInfo {
                    version: "0.9.0".to_string(),
                    downloads: 25000,
                    yanked: false,
                    created_at: Some("2022-06-01T00:00:00Z".to_string()),
                },
            ],
            authors: vec!["Mock Author <mock@example.com>".to_string()],
            created_at: Some("2022-01-01T00:00:00Z".to_string()),
            updated_at: Some("2024-01-01T00:00:00Z".to_string()),
        };

        info!("Retrieved mock crate info for: {}", name);
        Ok(crate_info)
    }

    /// Download and extract a crate to a temporary directory (mock implementation)
    pub async fn download_crate(&self, name: &str, version: &Version) -> Result<TempDir> {
        info!("Mock downloading crate: {}@{}", name, version);

        // Create a temporary directory with mock content
        let temp_dir = TempDir::new()?;

        // Create a basic Cargo.toml
        let cargo_toml = format!(
            r#"[package]
name = "{}"
version = "{}"
edition = "2021"
description = "Mock crate for testing"

[dependencies]
serde = "1.0"
"#,
            name, version
        );

        // Create a basic lib.rs
        let lib_rs = format!(
            r#"//! Mock crate {} documentation
//!
//! This is a mock implementation for testing purposes.

/// Main struct for {}
pub struct {} {{
    pub value: u32,
}}

impl {} {{
    /// Create a new instance
    pub fn new(value: u32) -> Self {{
        Self {{ value }}
    }}

    /// Get the value
    pub fn get(&self) -> u32 {{
        self.value
    }}
}}

/// Mock trait for demonstration
pub trait MockTrait {{
    /// Mock method
    fn mock_method(&self) -> String;
}}

impl MockTrait for {} {{
    fn mock_method(&self) -> String {{
        format!("Mock implementation: {{}}", self.value)
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn test_new() {{
        let instance = {}::new(42);
        assert_eq!(instance.get(), 42);
    }}
}}
"#,
            name,
            name,
            capitalize_first_letter(name),
            capitalize_first_letter(name),
            capitalize_first_letter(name),
            capitalize_first_letter(name)
        );

        // Write the files
        std::fs::write(temp_dir.path().join("Cargo.toml"), cargo_toml)?;
        std::fs::create_dir_all(temp_dir.path().join("src"))?;
        std::fs::write(temp_dir.path().join("src").join("lib.rs"), lib_rs)?;

        info!("Successfully created mock crate: {}@{}", name, version);
        Ok(temp_dir)
    }

    /// Get the latest stable version of a crate (mock implementation)
    pub async fn get_latest_version(&self, name: &str) -> Result<Version> {
        debug!("Mock getting latest version for: {}", name);
        let version = Version::parse("1.0.0")?;
        debug!("Mock latest version for {}: {}", name, version);
        Ok(version)
    }

    /// Check if a crate exists (mock implementation)
    pub async fn crate_exists(&self, name: &str) -> Result<bool> {
        debug!("Mock checking if crate exists: {}", name);
        // Mock: all crates exist except those with "nonexistent" in the name
        let exists = !name.contains("nonexistent");
        debug!("Mock crate exists {}: {}", name, exists);
        Ok(exists)
    }

    /// Get crate statistics (mock implementation)
    pub async fn get_crate_stats(&self, name: &str) -> Result<CrateDownloadStats> {
        debug!("Mock getting download stats for: {}", name);

        let stats = CrateDownloadStats {
            total_downloads: 50000,
            recent_downloads: Some(1500),
        };

        debug!("Mock retrieved stats for {}: {:?}", name, stats);
        Ok(stats)
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

/// Helper function to capitalize the first letter of a string
fn capitalize_first_letter(s: &str) -> String {
    s.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_creation() {
        let _fetcher = Fetcher::new();
        // Just test that we can create the fetcher without panicking
        assert!(true);
    }

    #[tokio::test]
    async fn test_mock_crate_exists() {
        let fetcher = Fetcher::new();

        // Test with a crate that should exist
        let exists = fetcher.crate_exists("serde").await.unwrap();
        assert!(exists);

        // Test with a crate that should not exist
        let exists = fetcher
            .crate_exists("nonexistent-crate-12345")
            .await
            .unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_mock_search_crates() {
        let fetcher = Fetcher::new();
        let results = fetcher.search_crates("test", 5).await.unwrap();

        assert!(!results.is_empty());
        assert!(results.len() <= 5);
        assert!(results.iter().all(|r| r.name.contains("test")));
    }

    #[tokio::test]
    async fn test_mock_get_latest_version() {
        let fetcher = Fetcher::new();
        let version = fetcher.get_latest_version("serde").await.unwrap();

        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 0);
        assert_eq!(version.patch, 0);
        assert!(version.pre.is_empty()); // Should be a stable version
    }

    #[tokio::test]
    async fn test_mock_download_crate() {
        let fetcher = Fetcher::new();
        let version = Version::parse("1.0.0").unwrap();
        let temp_dir = fetcher
            .download_crate("test-crate", &version)
            .await
            .unwrap();

        // Verify that files were created
        let cargo_toml = temp_dir.path().join("Cargo.toml");
        let lib_rs = temp_dir.path().join("src").join("lib.rs");

        assert!(cargo_toml.exists());
        assert!(lib_rs.exists());

        // Verify content
        let cargo_content = std::fs::read_to_string(&cargo_toml).unwrap();
        assert!(cargo_content.contains("test-crate"));
        assert!(cargo_content.contains("1.0.0"));

        let lib_content = std::fs::read_to_string(&lib_rs).unwrap();
        assert!(lib_content.contains("TestCrate"));
        assert!(lib_content.contains("Mock crate test-crate documentation"));
    }

    #[test]
    fn test_capitalize_first_letter() {
        assert_eq!(capitalize_first_letter("hello"), "Hello");
        assert_eq!(capitalize_first_letter("world"), "World");
        assert_eq!(capitalize_first_letter(""), "");
        assert_eq!(capitalize_first_letter("a"), "A");
        assert_eq!(capitalize_first_letter("test-crate"), "TestCrate");
    }
}
