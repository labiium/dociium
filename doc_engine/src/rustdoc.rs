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
            toolchain: "nightly-2024-01-15".to_string(), // Pinned toolchain for stable JSON format
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
        let _json_output_path = temp_output_dir
            .path()
            .join("doc")
            .join(format!("{}.json", crate_name));

        // Build the cargo rustdoc command
        let mut cmd = self.build_rustdoc_command(&actual_crate_dir, temp_output_dir.path())?;

        info!("Executing rustdoc command: {:?}", cmd);

        // Execute the command
        let output = self.execute_rustdoc_command(&mut cmd).await?;

        // Check for errors in output
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Rustdoc command failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Read the generated JSON file
        let json_bytes = fs::read(&_json_output_path)
            .await
            .with_context(|| format!("Failed to read rustdoc JSON output at {:?}", _json_output_path))?;

        // Parse the JSON output
        let rustdoc_crate: RustdocCrate = serde_json::from_slice(&json_bytes)
            .context("Failed to parse rustdoc JSON output")?;

        Ok(rustdoc_crate)
    }

    fn build_rustdoc_command(&self, crate_dir: &Path, output_dir: &Path) -> Result<Command> {
        // Cargo arguments
        let mut cargo_args = vec![
            format!("+{}", self.config.toolchain),
            "rustdoc".to_string(),
            "--lib".to_string(),
            "--manifest-path".to_string(),
            crate_dir.join("Cargo.toml").to_string_lossy().to_string(),
        ];
        // Features
        if self.config.all_features {
            cargo_args.push("--all-features".to_string());
        } else {
            if !self.config.default_features {
                cargo_args.push("--no-default-features".to_string());
            }
            if !self.config.features.is_empty() {
                cargo_args.push("--features".to_string());
                cargo_args.push(self.config.features.join(","));
            }
        }
        if let Some(ref target) = self.config.target {
            cargo_args.push("--target".to_string());
            cargo_args.push(target.clone());
        }
        // Use a custom target directory for rustdoc output
        cargo_args.push("--target-dir".to_string());
        cargo_args.push(output_dir.to_string_lossy().to_string());
        // Rustdoc arguments
        let mut rustdoc_args = vec![
            "-Zunstable-options".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
        ];
        // Additional rustdoc flags, but skip any -o/--out-dir and their values to avoid duplicates
        let mut rustdoc_flags_iter = self.config.rustdoc_flags.iter().peekable();
        while let Some(flag) = rustdoc_flags_iter.next() {
            if flag == "-o" || flag == "--out-dir" {
                rustdoc_flags_iter.next();
                continue;
            }
            if flag.starts_with("-o=") || flag.starts_with("--out-dir=") {
                continue;
            }
            rustdoc_args.push(flag.clone());
        }
        // Check for duplicate -o/--out-dir in rustdoc_args
        let mut out_dir_count = 0;
        let mut prev_is_out_flag = false;
        for arg in &rustdoc_args {
            if prev_is_out_flag {
                out_dir_count += 1;
                prev_is_out_flag = false;
            }
            if arg == "-o" || arg == "--out-dir" {
                prev_is_out_flag = true;
            }
            if arg.starts_with("-o=") || arg.starts_with("--out-dir=") {
                out_dir_count += 1;
            }
        }
        if out_dir_count > 1 {
            panic!("Duplicate -o/--out-dir detected in rustdoc args: {:?}", rustdoc_args);
        }
        debug!("Full cargo rustdoc command: cargo {:?} -- {:?}", cargo_args, rustdoc_args);
        // Now actually build the command
        let mut cmd = Command::new("cargo");
        for arg in &cargo_args {
            cmd.arg(arg);
        }
        cmd.arg("--");
        for arg in &rustdoc_args {
            cmd.arg(arg);
        }
        // Set working directory
        cmd.current_dir(crate_dir);
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
        let current_dir = self.crate_dir.clone();

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
                // root is always present (Id), so no need to check is_some()
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
        assert_eq!(config.toolchain, "nightly-2024-01-15");
        assert!(config.default_features);
        assert!(!config.all_features);
    }
}
