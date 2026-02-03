//! CLI tool implementations
//!
//! This module handles direct invocation of all dociium tools from the command line.

use anyhow::{bail, Context, Result};
use dociium::doc_engine::DocEngine;
use std::sync::Arc;

/// Validates that item_path doesn't include the package name as a prefix
fn validate_item_path(item_path: &str, package_name: &str) -> Result<()> {
    // Check if the path starts with the package name followed by a path separator
    let normalized_package = package_name.trim().to_lowercase().replace(['-', '_'], "");

    if let Some(hash_pos) = item_path.find('#') {
        let file_path = &item_path[..hash_pos];
        let path_parts: Vec<&str> = file_path.split('/').collect();

        if !path_parts.is_empty() {
            let first_part = path_parts[0].to_lowercase().replace(['-', '_'], "");

            // Check if first part matches package name
            if first_part == normalized_package {
                // Suggest the correct path
                let suggested_path = if path_parts.len() > 1 {
                    path_parts[1..].join("/") + &item_path[hash_pos..]
                } else {
                    item_path[hash_pos..].to_string()
                };

                bail!(
                    "item_path should not include the package name '{}' as a prefix. \
                     The path is relative to the package root. \
                     Try using '{}' instead of '{}'",
                    package_name,
                    suggested_path,
                    item_path
                );
            }
        }
    }

    Ok(())
}

pub async fn handle_command(cmd: crate::Commands, engine: Arc<DocEngine>) -> Result<()> {
    use crate::Commands::*;

    match cmd {
        // ===== Rust Documentation Tools =====
        SearchCrates { query, limit } => search_crates(&query, limit).await,

        CrateInfo { name } => crate_info(&name, &engine).await,

        GetItemDoc {
            crate_name,
            path,
            version,
        } => get_item_doc(&crate_name, &path, version.as_deref(), &engine).await,

        ListTraitImpls {
            crate_name,
            trait_path,
            version,
        } => list_trait_impls(&crate_name, &trait_path, version.as_deref(), &engine).await,

        ListImplsForType {
            crate_name,
            type_path,
            version,
        } => list_impls_for_type(&crate_name, &type_path, version.as_deref(), &engine).await,

        SourceSnippet {
            crate_name,
            item_path,
            context,
            version,
        } => {
            source_snippet(
                &crate_name,
                &item_path,
                context,
                version.as_deref(),
                &engine,
            )
            .await
        }

        SearchSymbols {
            crate_name,
            query,
            kinds,
            limit,
            version,
        } => {
            let kinds_vec = kinds.map(|k| k.split(',').map(|s| s.to_string()).collect());
            search_symbols(
                &crate_name,
                &query,
                kinds_vec,
                limit,
                version.as_deref(),
                &engine,
            )
            .await
        }

        // ===== Python/Node.js Tools =====
        GetImplementation {
            language,
            package,
            path,
            context,
        } => get_implementation(&language, &package, &path, context.as_deref(), &engine).await,

        ListClassMethods {
            package,
            path,
            private,
            context,
        } => list_class_methods(&package, &path, private, &context, &engine).await,

        GetClassMethod {
            package,
            path,
            method,
            context,
        } => get_class_method(&package, &path, &method, &context, &engine).await,

        SearchPackageCode {
            package,
            pattern,
            mode,
            limit,
            context,
        } => search_package_code(&package, &pattern, &mode, limit, &context, &engine).await,

        SemanticSearch {
            package,
            query,
            limit,
            context,
        } => semantic_search(&package, &query, limit, &context, &engine).await,

        // ===== Cache Management =====
        CacheStats => cache_stats(&engine).await,

        ClearCache { crate_name } => clear_cache(crate_name.as_deref(), &engine).await,

        CleanupCache => cleanup_cache(&engine).await,

        _ => unreachable!("Server commands handled in main"),
    }
}

// ===== Rust Documentation Tool Implementations =====

async fn search_crates(query: &str, limit: u32) -> Result<()> {
    use dociium::doc_engine::fetcher::Fetcher;
    let fetcher = Fetcher::new();
    let results = fetcher
        .search_crates(query, limit)
        .await
        .context("Failed to search crates")?;

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn crate_info(name: &str, engine: &DocEngine) -> Result<()> {
    let info = engine
        .crate_info(name)
        .await
        .context("Failed to get crate info")?;

    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}

async fn get_item_doc(
    crate_name: &str,
    path: &str,
    version: Option<&str>,
    engine: &DocEngine,
) -> Result<()> {
    let doc = engine
        .get_item_doc(crate_name, path, version)
        .await
        .context("Failed to get item documentation")?;

    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

async fn list_trait_impls(
    crate_name: &str,
    trait_path: &str,
    version: Option<&str>,
    engine: &DocEngine,
) -> Result<()> {
    let impls = engine
        .list_trait_impls(crate_name, trait_path, version)
        .await
        .context("Failed to list trait implementations")?;

    println!("{}", serde_json::to_string_pretty(&impls)?);
    Ok(())
}

async fn list_impls_for_type(
    crate_name: &str,
    type_path: &str,
    version: Option<&str>,
    engine: &DocEngine,
) -> Result<()> {
    let impls = engine
        .list_impls_for_type(crate_name, type_path, version)
        .await
        .context("Failed to list implementations for type")?;

    println!("{}", serde_json::to_string_pretty(&impls)?);
    Ok(())
}

async fn source_snippet(
    crate_name: &str,
    item_path: &str,
    context_lines: u32,
    version: Option<&str>,
    engine: &DocEngine,
) -> Result<()> {
    let snippet = engine
        .source_snippet(crate_name, item_path, context_lines, version)
        .await
        .context("Failed to get source snippet")?;

    println!("{}", serde_json::to_string_pretty(&snippet)?);
    Ok(())
}

async fn search_symbols(
    crate_name: &str,
    query: &str,
    kinds: Option<Vec<String>>,
    limit: u32,
    version: Option<&str>,
    engine: &DocEngine,
) -> Result<()> {
    let results = engine
        .search_symbols(crate_name, query, kinds.as_deref(), limit, version)
        .await
        .context("Failed to search symbols")?;

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

// ===== Python/Node.js Tool Implementations =====

async fn get_implementation(
    language: &str,
    package: &str,
    path: &str,
    context: Option<&str>,
    engine: &DocEngine,
) -> Result<()> {
    // Validate that item_path doesn't include package name prefix for Python/Node
    let lang_lower = language.trim().to_lowercase();
    if lang_lower == "python" || lang_lower == "node" {
        validate_item_path(path, package)?;
    }

    let ctx = engine
        .get_implementation_context(language, package, path, context)
        .await
        .context("Failed to get implementation")?;

    println!("{}", serde_json::to_string_pretty(&ctx)?);
    Ok(())
}

async fn list_class_methods(
    package: &str,
    path: &str,
    include_private: bool,
    context: &str,
    engine: &DocEngine,
) -> Result<()> {
    // Validate that item_path doesn't include package name prefix
    validate_item_path(path, package)?;

    // Parse path: "relative/path#ClassName"
    let parts: Vec<&str> = path.split('#').collect();
    if parts.len() != 2 {
        bail!("Path must be in format 'path/to/file#ClassName'");
    }

    let relative_path = parts[0];
    let class_name = parts[1];
    let context_path = std::path::PathBuf::from(context);

    let methods = engine
        .python_processor
        .list_class_methods(
            package,
            &context_path,
            relative_path,
            class_name,
            include_private,
        )
        .await
        .context("Failed to list class methods")?;

    println!("{}", serde_json::to_string_pretty(&methods)?);
    Ok(())
}

async fn get_class_method(
    package: &str,
    path: &str,
    method_name: &str,
    context: &str,
    engine: &DocEngine,
) -> Result<()> {
    // Validate that item_path doesn't include package name prefix
    validate_item_path(path, package)?;

    // Parse path: "relative/path#ClassName"
    let parts: Vec<&str> = path.split('#').collect();
    if parts.len() != 2 {
        bail!("Path must be in format 'path/to/file#ClassName'");
    }

    let relative_path = parts[0];
    let class_name = parts[1];
    let context_path = std::path::PathBuf::from(context);

    let method = engine
        .python_processor
        .get_class_method(
            package,
            &context_path,
            relative_path,
            class_name,
            method_name,
        )
        .await
        .context("Failed to get class method")?;

    println!("{}", serde_json::to_string_pretty(&method)?);
    Ok(())
}

async fn search_package_code(
    package: &str,
    pattern: &str,
    mode: &str,
    limit: u32,
    context: &str,
    engine: &DocEngine,
) -> Result<()> {
    use dociium::doc_engine::python_analyzer::SearchMode;

    let search_mode = match mode.to_lowercase().as_str() {
        "name" => SearchMode::Name,
        "signature" => SearchMode::Signature,
        "docstring" => SearchMode::Docstring,
        "fulltext" => SearchMode::FullText,
        _ => anyhow::bail!("Invalid search mode. Use: name, signature, docstring, or fulltext"),
    };

    let context_path = std::path::PathBuf::from(context);

    let results = engine
        .python_processor
        .search_package(package, &context_path, pattern, search_mode, limit as usize)
        .await
        .context("Failed to search package code")?;

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn semantic_search(
    package: &str,
    query: &str,
    limit: u32,
    context: &str,
    engine: &DocEngine,
) -> Result<()> {
    let results = engine
        .semantic_search("python", package, query, limit as usize, Some(context))
        .await
        .context("Failed to perform semantic search")?;

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

// ===== Cache Management Implementations =====

async fn cache_stats(engine: &DocEngine) -> Result<()> {
    let stats = engine
        .get_cache_stats()
        .await
        .context("Failed to get cache stats")?;

    println!("{}", serde_json::to_string_pretty(&stats)?);
    Ok(())
}

async fn clear_cache(crate_name: Option<&str>, engine: &DocEngine) -> Result<()> {
    let result = if let Some(name) = crate_name {
        engine.clear_crate_cache(name).await?
    } else {
        engine.clear_all_cache().await?
    };

    if let Some(name) = crate_name {
        println!(
            "✓ Cleared cache for crate: {} ({} entries removed)",
            name, result.items_affected
        );
    } else {
        println!(
            "✓ Cleared all cache entries ({} removed)",
            result.items_affected
        );
    }
    Ok(())
}

async fn cleanup_cache(engine: &DocEngine) -> Result<()> {
    let result = engine
        .cleanup_expired_cache()
        .await
        .context("Failed to cleanup cache")?;

    println!("✓ Removed {} expired cache entries", result.items_affected);
    Ok(())
}
