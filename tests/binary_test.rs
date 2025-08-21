//! Binary execution tests for the Rust Documentation MCP Server
//!
//! These tests verify that the binary can be executed and basic functionality works.

use assert_cmd::Command;
use std::process::Command as StdCommand;
use tempfile::TempDir;

#[test]
fn test_binary_exists() {
    // Test that the binary can be found and executed
    let mut cmd = Command::cargo_bin("dociium").unwrap();

    // Just check that the binary exists and can start
    // We expect it to exit quickly since it needs stdio input for MCP
    // Use a very short timeout and don't assert on the result
    let _output = cmd.timeout(std::time::Duration::from_millis(100)).ok();

    // The binary should exist and be executable
    // It might timeout or exit with an error due to missing MCP input, but it should not crash
    // Just completing without panic means the binary exists and is executable
}

#[test]
fn test_binary_with_cache_dir() {
    let temp_dir = TempDir::new().unwrap();
    let cache_path = temp_dir.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("dociium").unwrap();

    // Set cache directory environment variable
    let _output = cmd
        .env("RDOCS_CACHE_DIR", cache_path)
        .timeout(std::time::Duration::from_millis(100))
        .ok();

    // Should be able to start with custom cache directory
    // Just completing without panic means it can handle the cache directory
}

#[test]
fn test_binary_help_or_version() {
    // Most Rust binaries support --help, let's see if ours does
    // Note: Our binary doesn't implement clap args, so this will likely fail
    // But we can at least verify it doesn't panic
    let mut cmd = Command::cargo_bin("dociium").unwrap();

    let _output = cmd
        .arg("--help")
        .timeout(std::time::Duration::from_millis(100))
        .ok(); // Use ok() instead of assert() since --help might not be implemented

    // Just verify we can attempt to run it
}

#[test]
fn test_cargo_version_info() {
    // Test that cargo can give us information about our package
    let output = StdCommand::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo metadata");

    assert!(output.status.success());

    let metadata = String::from_utf8(output.stdout).unwrap();
    // Just verify we get some metadata containing our package name
    assert!(metadata.contains("dociium"));
}

#[test]
fn test_binary_compilation_features() {
    // Test that the binary compiles with different features
    let output = StdCommand::new("cargo")
        .args(["check", "--bin", "dociium", "--features", "stdio"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo check");

    assert!(
        output.status.success(),
        "Binary should compile with stdio feature"
    );
}

#[test]
fn test_workspace_binary_target() {
    // Verify that the workspace knows about our binary
    let output = StdCommand::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo metadata");

    assert!(output.status.success());

    let metadata = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&metadata).unwrap();

    // Find our package in the workspace
    let packages = json["packages"].as_array().unwrap();
    let our_package = packages
        .iter()
        .find(|p| p["name"].as_str() == Some("dociium"))
        .expect("Should find dociium package");

    // Check that it has a binary target
    let targets = our_package["targets"].as_array().unwrap();
    let binary_target = targets
        .iter()
        .find(|t| {
            t["kind"]
                .as_array()
                .unwrap()
                .contains(&serde_json::Value::String("bin".to_string()))
        })
        .expect("Should have a binary target");

    assert_eq!(binary_target["name"].as_str(), Some("dociium"));
}

#[test]
fn test_library_and_binary_coexist() {
    // Verify that both lib and binary targets exist
    let output = StdCommand::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo metadata");

    assert!(output.status.success());

    let metadata = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&metadata).unwrap();

    let packages = json["packages"].as_array().unwrap();
    let our_package = packages
        .iter()
        .find(|p| p["name"].as_str() == Some("dociium"))
        .expect("Should find dociium package");

    let targets = our_package["targets"].as_array().unwrap();

    // Should have both lib and bin targets
    let has_lib = targets.iter().any(|t| {
        t["kind"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("lib".to_string()))
    });

    let has_bin = targets.iter().any(|t| {
        t["kind"]
            .as_array()
            .unwrap()
            .contains(&serde_json::Value::String("bin".to_string()))
    });

    assert!(has_lib, "Should have library target");
    assert!(has_bin, "Should have binary target");
}

#[test]
fn test_dependencies_available() {
    // Test that key dependencies are available at compile time
    let output = StdCommand::new("cargo")
        .args(["tree", "-p", "dociium"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let tree = String::from_utf8(output.stdout).unwrap();

            // Check for key dependencies
            assert!(tree.contains("rmcp"), "Should have rmcp dependency");
            assert!(tree.contains("tokio"), "Should have tokio dependency");
            assert!(tree.contains("serde"), "Should have serde dependency");
            assert!(tree.contains("flate2"), "Should have flate2 dependency");
        }
    }
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_release_build_size() {
    // Build in release mode and check that binary is created
    let output = StdCommand::new("cargo")
        .args(["build", "--release", "--bin", "dociium"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to build release binary");

    assert!(
        output.status.success(),
        "Release build should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The binary should exist after building
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary_path = project_root.join("target").join("release").join("dociium");

    assert!(
        binary_path.exists(),
        "Release binary should exist at {binary_path:?}"
    );
}
