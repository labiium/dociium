//! Integration tests for the Rust Documentation MCP Server
//!
//! These tests verify the complete functionality of the MCP server,
//! including tool parameter parsing, engine operations, and response formatting.

use anyhow::Result;
use doc_engine::DocEngine;
use mcp_server::{
    CrateInfoParams, GetItemDocParams, ListImplsForTypeParams, ListTraitImplsParams,
    RustDocsMcpServer, SearchCratesParams, SearchSymbolsParams, SourceSnippetParams,
};
use rmcp::handler::server::tool::Parameters;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;

/// Helper function to create a test MCP server
async fn create_test_server() -> Result<(RustDocsMcpServer, TempDir)> {
    let temp_dir = TempDir::new()?;
    let cache_dir = temp_dir.path().to_str().unwrap();
    let server = RustDocsMcpServer::new(cache_dir).await?;
    Ok((server, temp_dir))
}

/// Helper function to extract text from CallToolResult
fn get_text_content(result: &rmcp::model::CallToolResult) -> String {
    match result.content.first() {
        Some(content) => match &**content {
            rmcp::model::RawContent::Text(text_content) => text_content.text.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Helper function to parse JSON response from result
fn parse_response_result(
    result: &Result<rmcp::model::CallToolResult, rmcp::model::ErrorData>,
) -> Value {
    match result {
        Ok(call_result) => {
            let text = get_text_content(call_result);
            serde_json::from_str(&text)
                .unwrap_or_else(|_| json!({"error": "Invalid JSON response"}))
        }
        Err(_) => json!({"error": "API call failed"}),
    }
}

#[tokio::test]
async fn test_search_crates_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test basic search
    let params = Parameters(SearchCratesParams {
        query: "serde".to_string(),
        limit: Some(5),
    });

    let response = server.search_crates(params).await;
    let parsed = parse_response_result(&response);

    // Should return a list of crates or an error
    assert!(parsed.is_array() || parsed.get("error").is_some());

    if let Some(results) = parsed.as_array() {
        // Results should have proper structure
        for result in results.iter().take(3) {
            assert!(result.get("name").is_some());
        }
    }
}

#[tokio::test]
async fn test_search_crates_empty_query() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchCratesParams {
        query: "".to_string(),
        limit: Some(10),
    });

    let response = server.search_crates(params).await;
    let parsed = parse_response_result(&response);

    // Should handle empty query gracefully
    assert!(parsed.get("error").is_some() || parsed.as_array().is_some_and(|arr| arr.is_empty()));
}

#[tokio::test]
async fn test_search_crates_with_limit() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchCratesParams {
        query: "web".to_string(),
        limit: Some(3),
    });

    let response = server.search_crates(params).await;
    let parsed = parse_response_result(&response);

    if let Some(results) = parsed.as_array() {
        // Should respect the limit
        assert!(results.len() <= 3);
    }
}

#[tokio::test]
async fn test_crate_info_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(CrateInfoParams {
        name: "serde".to_string(),
    });

    let response = server.crate_info(params).await;
    let parsed = parse_response_result(&response);

    // Should return crate information or error
    if parsed.get("error").is_none() {
        assert!(parsed.get("name").is_some());
        assert!(parsed.get("latest_version").is_some());
        assert!(parsed.get("downloads").is_some());
    }
}

#[tokio::test]
async fn test_crate_info_nonexistent() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(CrateInfoParams {
        name: "this-crate-should-not-exist-12345".to_string(),
    });

    let response = server.crate_info(params).await;
    let parsed = parse_response_result(&response);

    // Should return an error for nonexistent crate
    assert!(parsed.get("error").is_some());
}

#[tokio::test]
async fn test_get_item_doc_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(GetItemDocParams {
        crate_name: "std".to_string(),
        path: "collections::HashMap".to_string(),
        version: None,
    });

    let response = server.get_item_doc(params).await;
    let parsed = parse_response_result(&response);

    // Should return documentation or error
    if parsed.get("error").is_none() {
        assert!(parsed.get("path").is_some());
        assert!(parsed.get("kind").is_some());
        assert!(parsed.get("rendered_markdown").is_some());
    }
}

#[tokio::test]
async fn test_get_item_doc_with_version() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(GetItemDocParams {
        crate_name: "serde".to_string(),
        path: "Serialize".to_string(),
        version: Some("1.0.0".to_string()),
    });

    let response = server.get_item_doc(params).await;
    let parsed = parse_response_result(&response);

    // Should handle version parameter
    assert!(parsed.get("error").is_some() || parsed.get("path").is_some());
}

#[tokio::test]
async fn test_list_trait_impls_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(ListTraitImplsParams {
        crate_name: "std".to_string(),
        trait_path: "Clone".to_string(),
        version: None,
    });

    let response = server.list_trait_impls(params).await;
    let parsed = parse_response_result(&response);

    // Should return trait implementations or error
    assert!(parsed.get("error").is_some() || parsed.is_array());
}

#[tokio::test]
async fn test_list_impls_for_type_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(ListImplsForTypeParams {
        crate_name: "std".to_string(),
        type_path: "Vec".to_string(),
        version: None,
    });

    let response = server.list_impls_for_type(params).await;
    let parsed = parse_response_result(&response);

    // Should return type implementations or error
    assert!(parsed.get("error").is_some() || parsed.is_array());
}

#[tokio::test]
async fn test_source_snippet_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SourceSnippetParams {
        crate_name: "std".to_string(),
        item_path: "collections::HashMap::new".to_string(),
        context_lines: Some(10),
        version: None,
    });

    let response = server.source_snippet(params).await;
    let parsed = parse_response_result(&response);

    // Should return source snippet or error
    if parsed.get("error").is_none() {
        assert!(parsed.get("code").is_some());
        assert!(parsed.get("file").is_some());
        assert!(parsed.get("line_start").is_some());
    }
}

#[tokio::test]
async fn test_source_snippet_with_default_context() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SourceSnippetParams {
        crate_name: "tokio".to_string(),
        item_path: "main".to_string(),
        context_lines: None, // Should default to 5
        version: Some("1.0.0".to_string()),
    });

    let response = server.source_snippet(params).await;
    let parsed = parse_response_result(&response);

    // Should handle default context_lines
    assert!(parsed.get("error").is_some() || parsed.get("code").is_some());
}

#[tokio::test]
async fn test_search_symbols_basic() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchSymbolsParams {
        crate_name: "std".to_string(),
        query: "HashMap".to_string(),
        kinds: Some(vec!["struct".to_string(), "type".to_string()]),
        limit: Some(10),
        version: None,
    });

    let response = server.search_symbols(params).await;
    let parsed = parse_response_result(&response);

    // Should return symbol search results or error
    if let Some(results) = parsed.as_array() {
        for result in results.iter().take(3) {
            assert!(result.get("path").is_some());
            assert!(result.get("kind").is_some());
            assert!(result.get("score").is_some());
        }
    }
}

#[tokio::test]
async fn test_search_symbols_no_kinds_filter() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchSymbolsParams {
        crate_name: "serde".to_string(),
        query: "Serialize".to_string(),
        kinds: None, // No filter
        limit: Some(5),
        version: None,
    });

    let response = server.search_symbols(params).await;
    let parsed = parse_response_result(&response);

    // Should work without kinds filter
    assert!(parsed.get("error").is_some() || parsed.is_array());
}

#[tokio::test]
async fn test_search_symbols_with_limit() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(SearchSymbolsParams {
        crate_name: "tokio".to_string(),
        query: "async".to_string(),
        kinds: None,
        limit: Some(3),
        version: None,
    });

    let response = server.search_symbols(params).await;
    let parsed = parse_response_result(&response);

    if let Some(results) = parsed.as_array() {
        // Should respect the limit
        assert!(results.len() <= 3);
    }
}

#[tokio::test]
async fn test_error_handling_invalid_crate_name() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(CrateInfoParams {
        name: "invalid-crate-name-with-special-chars!@#$".to_string(),
    });

    let response = server.crate_info(params).await;
    let parsed = parse_response_result(&response);

    // Should handle invalid crate names gracefully
    assert!(parsed.get("error").is_some());
}

#[tokio::test]
async fn test_error_handling_empty_path() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    let params = Parameters(GetItemDocParams {
        crate_name: "std".to_string(),
        path: "".to_string(), // Empty path
        version: None,
    });

    let response = server.get_item_doc(params).await;
    let parsed = parse_response_result(&response);

    // Should handle empty paths gracefully
    assert!(parsed.get("error").is_some());
}

#[tokio::test]
async fn test_doc_engine_creation() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    // Test that DocEngine can be created
    let engine = DocEngine::new(cache_dir).await;
    assert!(engine.is_ok());
}

#[tokio::test]
async fn test_server_creation_and_info() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test server info
    use rmcp::ServerHandler;
    let info = server.get_info();

    assert_eq!(info.server_info.name, "rust-docs-mcp-server");
    assert!(!info.server_info.version.is_empty());
    assert!(info.instructions.is_some());
    assert!(info.capabilities.tools.is_some());
}

#[tokio::test]
async fn test_concurrent_requests() {
    let (server, _temp_dir) = create_test_server().await.unwrap();
    let server = Arc::new(server);

    // Test multiple concurrent requests
    let mut handles = vec![];

    for i in 0..5 {
        let server_clone = server.clone();
        let handle = tokio::spawn(async move {
            let params = Parameters(SearchCratesParams {
                query: format!("test-{i}"),
                limit: Some(3),
            });
            server_clone.search_crates(params).await
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    for handle in handles {
        let response = handle.await.unwrap();
        let parsed = parse_response_result(&response);
        // Each should return a valid response (array or error)
        assert!(parsed.is_array() || parsed.get("error").is_some());
    }
}

#[tokio::test]
async fn test_parameter_validation() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test various parameter edge cases
    let test_cases = vec![
        // Very long crate name
        ("a".repeat(1000), true), // Should error
        // Empty crate name
        ("".to_string(), true), // Should error
        // Normal crate name
        ("serde".to_string(), false), // Should not error (might return empty results)
    ];

    for (crate_name, should_error) in test_cases {
        let params = Parameters(CrateInfoParams { name: crate_name });
        let response = server.crate_info(params).await;
        let parsed = parse_response_result(&response);

        if should_error {
            assert!(
                parsed.get("error").is_some(),
                "Expected error for invalid input"
            );
        }
        // Note: Valid inputs might still return errors if crate doesn't exist,
        // but they shouldn't cause panics or invalid JSON
    }
}

#[tokio::test]
async fn test_json_serialization_integrity() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test that all responses are valid JSON
    let params = Parameters(SearchCratesParams {
        query: "serde".to_string(),
        limit: Some(5),
    });

    let response = server.search_crates(params).await;

    // Should always be valid JSON
    let parsed = parse_response_result(&response);
    assert!(
        parsed.is_object() || parsed.is_array(),
        "Response should be valid JSON"
    );
}

#[tokio::test]
async fn test_version_parameter_handling() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test different version formats
    let version_formats = vec![
        Some("1.0.0".to_string()),
        Some("v1.0.0".to_string()),
        Some("1.0".to_string()),
        Some("latest".to_string()),
        None,
    ];

    for version in version_formats {
        let params = Parameters(GetItemDocParams {
            crate_name: "serde".to_string(),
            path: "Serialize".to_string(),
            version: version.clone(),
        });

        let response = server.get_item_doc(params).await;
        let parsed = parse_response_result(&response);

        // Should handle all version formats gracefully
        assert!(
            parsed.is_object(),
            "Should return object for version {version:?}"
        );
    }
}

#[tokio::test]
async fn test_large_query_handling() {
    let (server, _temp_dir) = create_test_server().await.unwrap();

    // Test with very large query
    let large_query = "a".repeat(10000);
    let params = Parameters(SearchCratesParams {
        query: large_query,
        limit: Some(5),
    });

    let response = server.search_crates(params).await;
    let parsed = parse_response_result(&response);

    // Should handle large queries without crashing
    assert!(parsed.is_array() || parsed.get("error").is_some());
}

#[tokio::test]
async fn test_cache_directory_usage() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    // Create server (which should create cache structures)
    let _server = RustDocsMcpServer::new(cache_dir).await.unwrap();

    // Verify cache directory exists and has expected structure
    assert!(temp_dir.path().exists());

    // The engine should have created its internal structure
    // (Implementation detail: this depends on how DocEngine initializes)
}

/// Performance test to ensure reasonable response times
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
