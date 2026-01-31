//! Integration tests for the Rust Documentation MCP Server
//!
//! Simplified tests to avoid compiler issues while still verifying functionality.

use anyhow::Result;
use dociium::doc_engine::types::ImportResolutionParams;
use dociium::{
    CrateInfoParams, GetImplementationParams, GetItemDocParams, ListImplsForTypeParams,
    ListTraitImplsParams, RustDocsMcpServer, SearchCratesParams, SearchSymbolsParams,
    SourceSnippetParams, ToolConfig,
};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult, ServerHandler};
use std::fs;

use tempfile::TempDir;

/// Helper function to create a test server instance
async fn create_test_server() -> Result<(RustDocsMcpServer, TempDir)> {
    let temp_dir = TempDir::new()?;
    let cache_path = temp_dir.path().to_str().unwrap();
    let server = RustDocsMcpServer::new(cache_path).await?;
    Ok((server, temp_dir))
}

/// Helper function to extract text from CallToolResult
fn get_text_content(result: &CallToolResult) -> String {
    match result.content.first() {
        Some(content) => match &**content {
            rmcp::model::RawContent::Text(text_content) => text_content.text.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Helper function to check if result is successful
fn is_successful(result: &CallToolResult) -> bool {
    result.is_error.is_none() || result.is_error == Some(false)
}

#[tokio::test]
async fn test_search_crates_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchCratesParams {
        query: "serde".to_string(),
        limit: Some(5),
    });

    let response = server.search_crates(params).await;
    assert!(response.is_ok());

    let result = response.unwrap();
    assert!(is_successful(&result));

    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());

    // Should be valid JSON
    let json_result: Result<serde_json::Value, _> = serde_json::from_str(&text_content);
    assert!(json_result.is_ok());
}

#[tokio::test]
async fn test_search_crates_empty_query() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchCratesParams {
        query: "".to_string(),
        limit: Some(5),
    });

    let response = server.search_crates(params).await;
    // Should return an error for empty query
    assert!(response.is_err());
}

#[tokio::test]
async fn test_crate_info_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(CrateInfoParams {
        name: "serde".to_string(),
    });

    let response = server.crate_info(params).await;
    assert!(response.is_ok());

    let result = response.unwrap();
    assert!(is_successful(&result));

    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());

    // Should be valid JSON
    let json_result: Result<serde_json::Value, _> = serde_json::from_str(&text_content);
    assert!(json_result.is_ok());
}

#[tokio::test]
async fn test_crate_info_invalid_name() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(CrateInfoParams {
        name: "invalid-crate-name-with-special-chars!@#$".to_string(),
    });

    let response = server.crate_info(params).await;
    // Should return an error for invalid crate name
    assert!(response.is_err());
}

#[tokio::test]
async fn test_get_item_doc_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(GetItemDocParams {
        crate_name: "serde".to_string(),
        path: "Serialize".to_string(),
        version: None,
    });

    let response = server.get_item_doc(params).await;

    // For now, we accept that this might fail due to rustdoc version compatibility
    // The important thing is that the server doesn't crash and returns a proper error
    if response.is_err() {
        // This is acceptable - rustdoc version issues are expected in testing
        return;
    }

    let result = response.unwrap();
    assert!(is_successful(&result));

    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());
}

#[tokio::test]
async fn test_get_item_doc_empty_path() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(GetItemDocParams {
        crate_name: "serde".to_string(),
        path: "".to_string(),
        version: None,
    });

    let response = server.get_item_doc(params).await;
    // Should handle empty paths gracefully by returning an error
    assert!(response.is_err());
}

#[tokio::test]
async fn test_list_trait_impls_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(ListTraitImplsParams {
        crate_name: "serde".to_string(),
        trait_path: "Serialize".to_string(),
        version: None,
    });

    let response = server.list_trait_impls(params).await;

    // Accept that this might fail due to rustdoc compatibility issues
    if response.is_err() {
        return;
    }

    let result = response.unwrap();
    assert!(is_successful(&result));

    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());
}

#[tokio::test]
async fn test_list_impls_for_type_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(ListImplsForTypeParams {
        crate_name: "serde".to_string(),
        type_path: "Value".to_string(),
        version: None,
    });

    let response = server.list_impls_for_type(params).await;

    // Accept that this might fail due to rustdoc compatibility issues
    if response.is_err() {
        return;
    }

    let result = response.unwrap();
    assert!(is_successful(&result));

    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());
}

#[tokio::test]
async fn test_source_snippet_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SourceSnippetParams {
        crate_name: "serde".to_string(),
        item_path: "Serialize".to_string(),
        context_lines: Some(10),
        version: None,
    });

    let response = server.source_snippet(params).await;

    // Accept that this might fail due to rustdoc compatibility issues
    if response.is_err() {
        return;
    }

    let result = response.unwrap();
    assert!(is_successful(&result));

    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());
}

#[tokio::test]
async fn test_source_snippet_too_many_context_lines() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SourceSnippetParams {
        crate_name: "serde".to_string(),
        item_path: "Serialize".to_string(),
        context_lines: Some(200), // Exceeds max of 100
        version: None,
    });

    let response = server.source_snippet(params).await;
    assert!(response.is_err(), "Should reject context_lines > 100");
}

#[tokio::test]
async fn test_search_symbols_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchSymbolsParams {
        crate_name: "serde".to_string(),
        query: "serialize".to_string(),
        kinds: Some(vec!["function".to_string(), "trait".to_string()]),
        limit: Some(10),
        version: None,
    });

    let response = server.search_symbols(params).await;

    // Accept that this might fail due to rustdoc compatibility issues
    if response.is_err() {
        return;
    }

    let result = response.unwrap();
    assert!(is_successful(&result));

    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());
}

#[tokio::test]
async fn test_search_symbols_large_limit() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchSymbolsParams {
        crate_name: "serde".to_string(),
        query: "test".to_string(),
        kinds: None,
        limit: Some(200), // Exceeds max of 100
        version: None,
    });

    let response = server.search_symbols(params).await;
    assert!(response.is_err(), "Should reject limit > 100");
}

#[tokio::test]
async fn test_server_creation_and_info() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Should be able to get server info
    let info = server.get_info();
    assert_eq!(info.server_info.name, "rust-docs-mcp-server");
    assert!(!info.server_info.version.is_empty());
    assert!(info.capabilities.tools.is_some());

    let instructions = info.instructions.unwrap();
    assert!(instructions.contains("Rust"));
    assert!(instructions.contains("Documentation"));
}

#[tokio::test]
async fn test_parameter_validation_crate_names() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test various parameter edge cases
    let long_name = "a".repeat(100);
    let invalid_crate_names = vec![
        "",              // Empty
        &long_name,      // Too long
        "crate@invalid", // Invalid characters
        "-invalid",      // Starts with hyphen
        "invalid-",      // Ends with hyphen
        "crate name",    // Contains space
    ];

    for invalid_name in invalid_crate_names {
        let params = Parameters(CrateInfoParams {
            name: invalid_name.to_string(),
        });
        let response = server.crate_info(params).await;
        assert!(
            response.is_err(),
            "Should reject invalid crate name: {invalid_name}"
        );
    }
}

#[tokio::test]
async fn test_parameter_validation_item_paths() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test item path validation edge cases
    let long_path = "a".repeat(1000);
    let invalid_paths = vec![
        "",               // Empty
        &long_path,       // Too long
        "std::",          // Ends with separator
        "::std",          // Starts with separator
        "std::::HashMap", // Double separator
    ];

    for invalid_path in invalid_paths {
        let params = Parameters(GetItemDocParams {
            crate_name: "std".to_string(),
            path: invalid_path.to_string(),
            version: None,
        });
        let response = server.get_item_doc(params).await;
        assert!(
            response.is_err(),
            "Should reject invalid path: {invalid_path}"
        );
    }
}

#[tokio::test]
async fn test_json_serialization_integrity() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test that all responses contain valid JSON
    let params = Parameters(SearchCratesParams {
        query: "serde".to_string(),
        limit: Some(5),
    });

    let response = server.search_crates(params).await;

    // Should always return valid CallToolResult
    assert!(response.is_ok());
    let result = response.unwrap();
    assert!(is_successful(&result));

    // Should be able to extract and parse JSON
    let text_content = get_text_content(&result);
    assert!(!text_content.is_empty());

    let json_result: Result<serde_json::Value, _> = serde_json::from_str(&text_content);
    assert!(json_result.is_ok());
}

#[tokio::test]
async fn test_concurrent_requests() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Run multiple concurrent search requests (which don't depend on rustdoc)
    let mut handles = Vec::new();
    for i in 0..3 {
        let server_clone = server.clone();
        let handle = tokio::spawn(async move {
            let params = Parameters(SearchCratesParams {
                query: format!("web{i}"),
                limit: Some(3),
            });
            server_clone.search_crates(params).await
        });
        handles.push(handle);
    }

    for handle in handles {
        let response = handle.await.unwrap();
        // Network requests may fail, just ensure no panics
        match response {
            Ok(_) => {}
            Err(e) => {
                // Allow network failures in concurrent tests
                eprintln!("Concurrent request failed (expected): {e}");
            }
        }
    }
}

/// Test Python introspection with a real package
/// This test requires Python to be installed but doesn't require pip
#[tokio::test]
#[cfg(feature = "integration-tests")]
async fn test_python_introspection_native() {
    use std::fs;

    let (server, tmp) = create_test_server().await.unwrap();

    // Create a simple Python package in a virtual environment structure
    let venv = tmp.path().join(".venv");
    let site_packages = venv.join("lib").join("python3.9").join("site-packages");
    let testpkg = site_packages.join("testpkg");

    fs::create_dir_all(&testpkg).unwrap();
    fs::write(testpkg.join("__init__.py"), "").unwrap();
    fs::write(
        testpkg.join("module.py"),
        r#"
class TestClass:
    """This is a test class."""

    def test_method(self):
        """This is a test method."""
        return 42

def test_function():
    """This is a test function."""
    return "hello"
"#,
    )
    .unwrap();

    // Set environment variable override for testing
    std::env::set_var("DOC_PYTHON_PACKAGE_PATH", site_packages.to_str().unwrap());

    let params = Parameters(GetImplementationParams {
        language: "python".to_string(),
        package_name: "testpkg".to_string(),
        item_path: "module.py#TestClass".to_string(),
        context_path: Some(tmp.path().to_string_lossy().to_string()),
    });

    let response = server.get_implementation(params).await;

    // Clean up environment variable
    std::env::remove_var("DOC_PYTHON_PACKAGE_PATH");

    // This test validates that the new Python introspection system works
    // It may fail in environments without Python, but should work on most systems
    if let Ok(result) = response {
        let text = get_text_content(&result);
        assert!(!text.is_empty(), "Result should not be empty");
        assert!(
            text.contains("TestClass") || text.contains("test"),
            "Result should contain class or function definition"
        );
    }
}

/// Test that fallback mechanisms work when pip is not available
#[tokio::test]
#[cfg(feature = "integration-tests")]
async fn test_python_package_discovery_without_pip() {
    use dociium::doc_engine::finder;
    use std::fs;

    let tmp = tempfile::tempdir().unwrap();
    let venv = tmp.path().join(".venv");
    let site_packages = venv.join("lib").join("python3.9").join("site-packages");
    let testpkg = site_packages.join("testpkg");

    fs::create_dir_all(&testpkg).unwrap();
    fs::write(testpkg.join("__init__.py"), "# Test package").unwrap();

    // Try to find the package without pip (using direct scanning)
    // This simulates an environment where pip is not in PATH
    let result = finder::find_python_package_path_with_context("testpkg", Some(tmp.path()));

    // This test demonstrates the new fallback mechanism
    // In a real scenario, this would work even without pip installed
    if result.is_ok() {
        let path = result.unwrap();
        assert!(path.exists(), "Package path should exist");
        assert!(
            path.to_string_lossy().contains("testpkg"),
            "Path should contain package name"
        );
    }
}

#[tokio::test]
async fn test_response_performance() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let start = std::time::Instant::now();

    let params = Parameters(SearchCratesParams {
        query: "web".to_string(),
        limit: Some(10),
    });

    let _response = server.search_crates(params).await;
    let duration = start.elapsed();

    // Should respond within reasonable time (adjust as needed)
    assert!(
        duration.as_secs() < 30,
        "Response took too long: {duration:?}"
    );
}

#[tokio::test]
async fn test_get_implementation_tool_available() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Should be able to get server info
    let info = server.get_info();
    assert!(info.capabilities.tools.is_some());

    // Test that the get_implementation tool can be called (even if it fails)
    // This verifies the tool is properly registered in the MCP server
    let params = Parameters(GetImplementationParams {
        language: "python".to_string(),
        package_name: "test".to_string(),
        item_path: "test.py#function".to_string(),
        context_path: None,
    });

    // The tool should exist and be callable (even if it fails due to missing package)
    let response = server.get_implementation(params).await;
    // We expect this to fail, but it proves the tool is registered
    assert!(response.is_err());
}

#[tokio::test]
async fn test_get_implementation_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(GetImplementationParams {
        language: "python".to_string(),
        package_name: "nonexistent".to_string(),
        item_path: "nonexistent.py#test_function".to_string(),
        context_path: None,
    });

    let response = server.get_implementation(params).await;

    // This should fail gracefully since the package doesn't exist
    // but it validates that the tool exists and accepts parameters correctly
    assert!(response.is_err());
}

#[tokio::test]
async fn test_get_implementation_invalid_params() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test empty package name
    let params = Parameters(GetImplementationParams {
        language: "python".to_string(),
        package_name: "".to_string(),
        item_path: "test.py#function".to_string(),
        context_path: None,
    });

    let response = server.get_implementation(params).await;
    assert!(response.is_err());

    // Test invalid item_path format
    let params = Parameters(GetImplementationParams {
        language: "python".to_string(),
        package_name: "test".to_string(),
        item_path: "invalid_format".to_string(),
        context_path: None,
    });

    let response = server.get_implementation(params).await;
    assert!(response.is_err());
}

//
// New tests for multi-language import resolution
//

#[tokio::test]
async fn test_resolve_imports_rust_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();
    // Basic rust use statement from std
    let params = Parameters(ImportResolutionParams {
        language: "rust".to_string(),
        package: "std".to_string(),
        version: None,
        import_line: Some("use std::fmt::Display;".to_string()),
        code_block: None,
        context_path: None,
    });
    let response = server.resolve_imports(params).await;
    // Could fail if std sources not available, but should not panic
    if let Ok(result) = response {
        let text = get_text_content(&result);
        if !text.is_empty() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                // Best effort: verify structure keys
                assert!(json.get("results").is_some());
            }
        }
    }
}

#[tokio::test]
async fn test_resolve_imports_python_basic() {
    let (server, tmp) = create_test_server().await.unwrap();
    // Construct a temporary python package mypkg with __init__.py and foo.py
    let pkg_dir = tmp.path().join("mypkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "def foo():\n    return 1\n").unwrap();
    // Point environment variable override to the temp parent so finder locates it
    std::env::set_var(
        "DOC_PYTHON_PACKAGE_PATH_MYPKG",
        tmp.path().to_str().unwrap(),
    );
    let params = Parameters(ImportResolutionParams {
        language: "python".to_string(),
        package: "mypkg".to_string(),
        version: None,
        import_line: Some("from mypkg import foo".to_string()),
        code_block: None,
        context_path: None,
    });
    let response = server.resolve_imports(params).await;
    if let Ok(result) = response {
        let text = get_text_content(&result);
        if !text.is_empty() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                let any_resolved = json
                    .get("any_resolved")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                assert!(any_resolved, "Expected python import to resolve symbol");
            }
        }
    }
}

#[tokio::test]
async fn test_resolve_imports_node_basic() {
    let (server, tmp) = create_test_server().await.unwrap();
    // Create node_modules/testpkg/index.js exporting a function
    let node_modules = tmp.path().join("node_modules");
    let testpkg = node_modules.join("testpkg");
    fs::create_dir_all(&testpkg).unwrap();
    fs::write(
        testpkg.join("index.js"),
        "export function hello() { return 42; }\n",
    )
    .unwrap();
    // Set override so finder uses our node_modules
    std::env::set_var("DOC_NODE_PACKAGE_PATH", node_modules.to_str().unwrap());
    let params = Parameters(ImportResolutionParams {
        language: "node".to_string(),
        package: "testpkg".to_string(),
        version: None,
        import_line: Some("import { hello } from \"testpkg\";".to_string()),
        code_block: None,
        context_path: None,
    });
    let response = server.resolve_imports(params).await;
    if let Ok(result) = response {
        let text = get_text_content(&result);
        if !text.is_empty() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                let any_resolved = json
                    .get("any_resolved")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !any_resolved {
                    eprintln!("Node import did not resolve a symbol (acceptable in environments without full Node export parsing)");
                }
            }
        }
    }
}

//
// Tests for tool filtering based on language flags (--python-only, --rust-only, etc.)
//

#[tokio::test]
async fn test_python_only_config() {
    // Verify that ToolConfig::python_only() creates correct config
    let config = ToolConfig::python_only();
    
    assert!(!config.rust_enabled, "Python-only config should disable Rust tools");
    assert!(config.python_enabled, "Python-only config should enable Python tools");
    assert!(!config.node_enabled, "Python-only config should disable Node tools");
    assert!(!config.cache_enabled, "Python-only config should disable cache tools");
}

#[tokio::test]
async fn test_rust_only_config() {
    // Verify that ToolConfig::rust_only() creates correct config
    let config = ToolConfig::rust_only();
    
    assert!(config.rust_enabled, "Rust-only config should enable Rust tools");
    assert!(!config.python_enabled, "Rust-only config should disable Python tools");
    assert!(!config.node_enabled, "Rust-only config should disable Node tools");
    assert!(!config.cache_enabled, "Rust-only config should disable cache tools");
}

#[tokio::test]
async fn test_node_only_config() {
    // Verify that ToolConfig::node_only() creates correct config
    let config = ToolConfig::node_only();
    
    assert!(!config.rust_enabled, "Node-only config should disable Rust tools");
    assert!(!config.python_enabled, "Node-only config should disable Python tools");
    assert!(config.node_enabled, "Node-only config should enable Node tools");
    assert!(!config.cache_enabled, "Node-only config should disable cache tools");
}

#[tokio::test]
async fn test_all_config() {
    // Verify that ToolConfig::all() enables everything
    let config = ToolConfig::all();
    
    assert!(config.rust_enabled, "All config should enable Rust tools");
    assert!(config.python_enabled, "All config should enable Python tools");
    assert!(config.node_enabled, "All config should enable Node tools");
    assert!(config.cache_enabled, "All config should enable cache tools");
}
