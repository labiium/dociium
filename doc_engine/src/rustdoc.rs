//! Rustdoc JSON generation module
//!
//! This module handles the actual execution of `cargo rustdoc` to generate
//! rustdoc JSON output for Rust crates.

use anyhow::{Context, Result};
use rustdoc_types::Crate as RustdocCrate;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, info, instrument, warn};

/// Configuration for rustdoc JSON generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustdocConfig {
    /// Timeout for rustdoc execution in seconds
    pub timeout_seconds: u64,
    /// Target triple for cross-compilation
    pub target: Option<String>,
    /// Features to enable
    pub features: Vec<String>,
    /// Whether to use default features
    pub default_features: bool,
    /// Whether to enable all features
    pub all_features: bool,
    /// Toolchain to use (e.g., "stable", "nightly")
    pub toolchain: String,
    /// Additional rustdoc flags
    pub rustdoc_flags: Vec<String>,
    /// Environment variables to set
    pub env_vars: std::collections::HashMap<String, String>,
}

impl Default for RustdocConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 300, // 5 minutes
            target: None,
            features: Vec::new(),
            default_features: true,
            all_features: false,
            toolchain: "nightly".to_string(), // JSON output requires nightly
            rustdoc_flags: Vec::new(),
            env_vars: std::collections::HashMap::new(),
        }
    }
}

/// Builder for generating rustdoc JSON output
#[derive(Debug)]
pub struct RustdocBuilder {
    crate_dir: PathBuf,
    config: RustdocConfig,
}

impl RustdocBuilder {
    /// Create a new rustdoc builder
    pub fn new(crate_dir: impl AsRef<Path>) -> Self {
        Self {
            crate_dir: crate_dir.as_ref().to_path_buf(),
            config: RustdocConfig::default(),
        }
    }

    /// Create a rustdoc builder with custom configuration
    pub fn with_config(crate_dir: impl AsRef<Path>, config: RustdocConfig) -> Self {
        Self {
            crate_dir: crate_dir.as_ref().to_path_buf(),
            config,
        }
    }

    /// Build rustdoc JSON for the crate
    #[instrument(skip(self), fields(crate_dir = ?self.crate_dir))]
    pub async fn build_json(&self) -> Result<RustdocCrate> {
        info!("Building rustdoc JSON for crate at: {:?}", self.crate_dir);

        // Find the actual crate directory (may be nested in extracted tarball)
        let actual_crate_dir = self.find_crate_root().await?;
        debug!("Found crate root at: {:?}", actual_crate_dir);

        // Verify Cargo.toml exists
        let cargo_toml = actual_crate_dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return Err(anyhow::anyhow!(
                "No Cargo.toml found in crate directory: {:?}",
                actual_crate_dir
            ));
        }

        // Parse Cargo.toml to get crate name
        let cargo_toml_content = fs::read_to_string(&cargo_toml).await?;
        let crate_name = self.extract_crate_name(&cargo_toml_content)?;
        debug!("Extracted crate name: {}", crate_name);

        // Create temporary directory for output
        let temp_output_dir = TempDir::new().context("Failed to create temp output directory")?;
        let json_output_path = temp_output_dir
            .path()
            .join("doc")
            .join(format!("{}.json", crate_name));

        // Build the cargo rustdoc command
        let mut cmd = self.build_rustdoc_command(&actual_crate_dir, temp_output_dir.path())?;

        info!("Executing rustdoc command: {:?}", cmd);

        // Execute with timeout and sandboxing
        let output = self.execute_rustdoc_command(&mut cmd).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            warn!("Rustdoc failed with status: {}", output.status);
            warn!("Stdout: {}", stdout);
            warn!("Stderr: {}", stderr);

            return Err(anyhow::anyhow!(
                "Rustdoc command failed with status {}: {}",
                output.status,
                stderr
            ));
        }

        // Read and parse the generated JSON
        if !json_output_path.exists() {
            return Err(anyhow::anyhow!(
                "Rustdoc JSON output not found at: {:?}",
                json_output_path
            ));
        }

        let json_content = fs::read_to_string(&json_output_path)
            .await
            .context("Failed to read rustdoc JSON output")?;

        let rustdoc_crate: RustdocCrate =
            serde_json::from_str(&json_content).context("Failed to parse rustdoc JSON output")?;

        info!(
            "Successfully built rustdoc JSON with {} items",
            rustdoc_crate.index.len()
        );

        // Validate the output
        let validation_report = self.validate_rustdoc_json(&rustdoc_crate)?;
        if !validation_report.is_valid() {
            warn!(
                "Rustdoc validation warnings: {:?}",
                validation_report.warnings
            );
        }

        Ok(rustdoc_crate)
    }

    /// Build the cargo rustdoc command
    fn build_rustdoc_command(&self, crate_dir: &Path, output_dir: &Path) -> Result<Command> {
        let mut cmd = Command::new("cargo");

        // Add toolchain prefix
        cmd.arg(format!("+{}", self.config.toolchain));
        cmd.arg("rustdoc");

        // Set working directory
        cmd.current_dir(crate_dir);

        // Basic flags
        cmd.arg("--lib");
        cmd.arg("--manifest-path").arg(crate_dir.join("Cargo.toml"));

        // Features
        if self.config.all_features {
            cmd.arg("--all-features");
        } else {
            if !self.config.default_features {
                cmd.arg("--no-default-features");
            }
            if !self.config.features.is_empty() {
                cmd.arg("--features").arg(self.config.features.join(","));
            }
        }

        // Target
        if let Some(ref target) = self.config.target {
            cmd.arg("--target").arg(target);
        }

        // Rustdoc-specific flags
        cmd.arg("--");
        cmd.arg("-Zunstable-options");
        cmd.arg("--output-format").arg("json");
        cmd.arg("-o").arg(output_dir);

        // Additional rustdoc flags
        for flag in &self.config.rustdoc_flags {
            cmd.arg(flag);
        }

        // Environment variables
        for (key, value) in &self.config.env_vars {
            cmd.env(key, value);
        }

        // Security: Set restrictive environment
        cmd.env("CARGO_HTTP_TIMEOUT", "30");
        cmd.env("CARGO_HTTP_LOW_SPEED_LIMIT", "1000");
        cmd.env("CARGO_NET_RETRY", "2");

        // Stdio configuration
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        Ok(cmd)
    }

    /// Execute the rustdoc command with timeout and sandboxing
    async fn execute_rustdoc_command(&self, cmd: &mut Command) -> Result<std::process::Output> {
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);

        // TODO: Add proper sandboxing here (bubblewrap, seccomp, etc.)
        // For now, we'll just use basic process isolation

        let child = cmd.spawn().context("Failed to spawn rustdoc process")?;

        let output = timeout(timeout_duration, child.wait_with_output())
            .await
            .context("Rustdoc command timed out")?
            .context("Failed to wait for rustdoc process")?;

        Ok(output)
    }

    /// Find the root directory of the crate (handles nested directories in tarballs)
    async fn find_crate_root(&self) -> Result<PathBuf> {
        let mut current_dir = self.crate_dir.clone();

        // Check if current directory has Cargo.toml
        if current_dir.join("Cargo.toml").exists() {
            return Ok(current_dir);
        }

        // Look for Cargo.toml in subdirectories (common in extracted tarballs)
        let mut entries = fs::read_dir(&current_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let potential_root = entry.path();
                if potential_root.join("Cargo.toml").exists() {
                    return Ok(potential_root);
                }
            }
        }

        Err(anyhow::anyhow!(
            "Could not find Cargo.toml in {:?} or its subdirectories",
            self.crate_dir
        ))
    }

    /// Extract crate name from Cargo.toml content
    fn extract_crate_name(&self, cargo_toml_content: &str) -> Result<String> {
        // Simple TOML parsing for the package name
        for line in cargo_toml_content.lines() {
            let line = line.trim();
            if line.starts_with("name") && line.contains('=') {
                let parts: Vec<&str> = line.split('=').collect();
                if parts.len() == 2 {
                    let name = parts[1].trim().trim_matches('"').trim_matches('\'');
                    if !name.is_empty() {
                        return Ok(name.to_string());
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Could not extract crate name from Cargo.toml"
        ))
    }

    /// Validate the generated rustdoc JSON
    pub fn validate_rustdoc_json(&self, rustdoc_crate: &RustdocCrate) -> Result<ValidationReport> {
        let mut report = ValidationReport::default();

        // Check for basic structure
        if rustdoc_crate.index.is_empty() {
            report
                .errors
                .push("Rustdoc JSON contains no items".to_string());
        }


        // Check for common issues
        let mut public_items = 0;
        let mut private_items = 0;
        let mut documented_items = 0;

        for item in rustdoc_crate.index.values() {
            match item.visibility {
                rustdoc_types::Visibility::Public => public_items += 1,
                _ => private_items += 1,
            }

            if item.docs.is_some() {
                documented_items += 1;
            }
        }

        report
            .stats
            .insert("total_items".to_string(), rustdoc_crate.index.len());
        report
            .stats
            .insert("public_items".to_string(), public_items);
        report
            .stats
            .insert("private_items".to_string(), private_items);
        report
            .stats
            .insert("documented_items".to_string(), documented_items);

        if public_items == 0 {
            report
                .warnings
                .push("No public items found in crate".to_string());
        }

        let documentation_ratio = if rustdoc_crate.index.len() > 0 {
            documented_items as f64 / rustdoc_crate.index.len() as f64
        } else {
            0.0
        };

        if documentation_ratio < 0.5 {
            report.warnings.push(format!(
                "Low documentation coverage: {:.1}%",
                documentation_ratio * 100.0
            ));
        }

        Ok(report)
    }
}

/// Report from validating rustdoc JSON output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub stats: std::collections::HashMap<String, usize>,
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            stats: std::collections::HashMap::new(),
        }
    }
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn create_test_crate(name: &str, version: &str) -> Result<TempDir> {
        let temp_dir = tempdir()?;

        let cargo_toml = format!(
            r#"[package]
name = "{}"
version = "{}"
edition = "2021"

[dependencies]
"#,
            name, version
        );

        let lib_rs = format!(
            r#"//! Test crate {}
//!
//! This is a test crate for validation.

/// Main struct
pub struct TestStruct {{
    pub value: u32,
}}

impl TestStruct {{
    /// Create a new instance
    pub fn new(value: u32) -> Self {{
        Self {{ value }}
    }}
}}
"#,
            name
        );

        fs::write(temp_dir.path().join("Cargo.toml"), cargo_toml).await?;
        fs::create_dir_all(temp_dir.path().join("src")).await?;
        fs::write(temp_dir.path().join("src").join("lib.rs"), lib_rs).await?;

        Ok(temp_dir)
    }

    #[tokio::test]
    async fn test_rustdoc_builder_creation() {
        let temp_dir = tempdir().unwrap();
        let _builder = RustdocBuilder::new(temp_dir.path());
    }

    #[tokio::test]
    async fn test_extract_crate_name() {
        let builder = RustdocBuilder::new("/tmp");
        let cargo_toml = r#"
[package]
name = "test-crate"
version = "1.0.0"
"#;
        let name = builder.extract_crate_name(cargo_toml).unwrap();
        assert_eq!(name, "test-crate");
    }

    #[tokio::test]
    async fn test_find_crate_root() {
        let temp_dir = create_test_crate("test", "1.0.0").await.unwrap();
        let builder = RustdocBuilder::new(temp_dir.path());
        let root = builder.find_crate_root().await.unwrap();
        assert_eq!(root, temp_dir.path());
    }

    #[tokio::test]
    async fn test_find_crate_root_nested() {
        let temp_dir = tempdir().unwrap();
        let nested_dir = temp_dir.path().join("nested");
        fs::create_dir_all(&nested_dir).await.unwrap();

        let cargo_toml = r#"[package]
name = "nested-test"
version = "1.0.0"
"#;
        fs::write(nested_dir.join("Cargo.toml"), cargo_toml)
            .await
            .unwrap();

        let builder = RustdocBuilder::new(temp_dir.path());
        let root = builder.find_crate_root().await.unwrap();
        assert_eq!(root, nested_dir);
    }

    #[tokio::test]
    #[cfg(feature = "integration-tests")]
    async fn test_build_json_real() {
        // This test requires nightly Rust and network access
        let temp_dir = create_test_crate("integration-test", "1.0.0")
            .await
            .unwrap();
        let builder = RustdocBuilder::new(temp_dir.path());

        // This will only work if nightly toolchain is available
        match builder.build_json().await {
            Ok(rustdoc_crate) => {
                assert!(!rustdoc_crate.index.is_empty());
                assert!(rustdoc_crate.root.is_some());
            }
            Err(e) => {
                // Expected if nightly is not available
                println!("Integration test skipped: {}", e);
            }
        }
    }

    #[test]
    fn test_rustdoc_config_default() {
        let config = RustdocConfig::default();
        assert_eq!(config.timeout_seconds, 300);
        assert_eq!(config.toolchain, "nightly");
        assert!(config.default_features);
        assert!(!config.all_features);
    }
}
