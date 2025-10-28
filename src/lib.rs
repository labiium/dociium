//! Rust Documentation MCP Server Library
//!
//! This library provides the core functionality for the Rust Documentation MCP Server,
//! including parameter types and the main server implementation.

pub use crate::server::{
    CrateInfoParams, GetImplementationParams, GetItemDocParams, ListImplsForTypeParams,
    ListTraitImplsParams, RustDocsMcpServer, SearchCratesParams, SearchSymbolsParams,
    SemanticSearchParams, SourceSnippetParams,
};

// Re-export commonly used dependencies for tests
pub use rmcp;
pub use serde_json;

pub mod doc_engine;
pub mod index_core;
pub mod server;
pub mod shared_types;

#[allow(dead_code)]
fn _ensure_shared_types_linked() {
    // Touch shared canonical types so they are never considered "truly" unused during
    // incremental builds or by overly aggressive static analysis phases.
    use crate::shared_types::{
        ImplItem, ItemDoc, SearchIndexData, SearchIndexItem, SemanticSearchResult, SourceLocation,
        SourceSnippet, SymbolSearchResult, TraitImpl, TypeImpl,
    };
    let _ = (
        std::any::type_name::<SourceLocation>(),
        std::any::type_name::<ItemDoc>(),
        std::any::type_name::<ImplItem>(),
        std::any::type_name::<TraitImpl>(),
        std::any::type_name::<TypeImpl>(),
        std::any::type_name::<SearchIndexItem>(),
        std::any::type_name::<SearchIndexData>(),
        std::any::type_name::<SymbolSearchResult>(),
        std::any::type_name::<SourceSnippet>(),
        std::any::type_name::<SemanticSearchResult>(),
    );
}
