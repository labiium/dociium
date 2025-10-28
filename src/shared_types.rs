//! Shared canonical type definitions for Dociium.
//!
//! Motivation:
//! The project originally duplicated many structurally identical data
//! structures across `doc_engine::types` and `index_core::types`, e.g.:
//! - SourceLocation
//! - ItemDoc
//! - ImplItem / TraitImpl / TypeImpl
//! - SymbolSearchResult
//! - SearchIndexItem / SearchIndexData
//! - SourceSnippet
//!
//! This module introduces *canonical* representations intended to be the
//! single source of truth. Other modules can (incrementally) migrate to
//! these definitions, minimizing maintenance burden and serialization /
//! schema drift.
//!
//! Integration Plan (incremental, low‑risk):
//! 1. Introduce `shared_types` (this file) with stable serde + schemars
//!    (schema) derivations.
//! 2. Add `pub mod shared_types;` to `lib.rs` (future patch).
//! 3. Add `From` / `Into` impls in *calling* code rather than here to
//!    avoid introducing circular compilation dependencies while both
//!    legacy type sets still exist.
//! 4. Gradually replace internal uses:
//!      - Prefer `shared_types::SourceLocation` etc.
//!      - Remove duplicated structs from `doc_engine::types` and
//!        `index_core::types` once references are gone.
//! 5. Expose via MCP schemas directly from this module for consistency.
//!
//! Design Notes:
//! - Field names retain the most widely used naming style to keep JSON
//!   stable (backwards compatibility).
//! - `visibility` kept as `String` (enum later when rustdoc variants
//!   fully enumerated).
//! - `kind` also left as `String` for forward compatibility.
//! - Optional fields kept as `Option<T>`; empty vectors prefer `Vec<T>`
//!   not `Option<Vec<T>>` to simplify consumer logic.
//! - Added `#[non_exhaustive]` where future evolution is likely.
//!
//! Future Enhancements:
//! - Introduce strongly typed enums for `ItemKind`, `Visibility`,
//!   `SymbolKind` with `serde(other)` fallbacks.
//! - Attach rich lifetime / generics info to impls when upstream
//!   extraction improves.
//!
//! This file is intentionally `dead_code`‑tolerant during migration.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(dead_code)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Canonical source span reference inside a file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
}

impl SourceLocation {
    pub fn single_point(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            end_line: None,
            end_column: None,
        }
    }
}

/// Documentation & analysis metadata for a single symbol / item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ItemDoc {
    pub path: String,
    pub kind: String,
    pub rendered_markdown: String,
    pub source_location: Option<SourceLocation>,
    pub visibility: String,
    pub attributes: Vec<String>,
    pub signature: Option<String>,
    pub examples: Vec<String>,
    pub see_also: Vec<String>,
}

/// Individual member (method / assoc item) inside an implementation block.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ImplItem {
    pub name: String,
    pub kind: String,
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub source_location: Option<SourceLocation>,
}

/// Trait implementation (impl <Trait> for <Type>).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct TraitImpl {
    pub for_type: String,
    pub trait_path: String,
    pub generics: Vec<String>,
    pub where_clause: Option<String>,
    pub source_span: Option<SourceLocation>,
    pub impl_id: String,
    pub items: Vec<ImplItem>,
    pub is_blanket: bool,
    pub is_synthetic: bool,
}

/// Implemented trait entry when listing traits *for* a type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct TypeImpl {
    pub trait_path: String,
    pub generics: Vec<String>,
    pub where_clause: Option<String>,
    pub source_span: Option<SourceLocation>,
    pub impl_id: String,
    pub items: Vec<ImplItem>,
    pub is_blanket: bool,
    pub is_synthetic: bool,
}

/// Lightweight search index item (mirrors docs.rs structure).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SearchIndexItem {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub description: String,
    pub parent_index: Option<usize>,
}

/// Crate‑scoped search index dataset (one version).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SearchIndexData {
    pub crate_name: String,
    pub version: String,
    pub items: Vec<SearchIndexItem>,
    pub paths: Vec<String>,
}

/// Symbol search result (post‑query scoring).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SymbolSearchResult {
    pub path: String,
    pub kind: String,
    pub score: f32,
    pub doc_summary: Option<String>,
    pub source_location: Option<SourceLocation>,
    pub visibility: String,
    pub signature: Option<String>,
    pub module_path: String,
}

/// Semantic search result for language-aware discovery (Python, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SemanticSearchResult {
    pub language: String,
    pub package: String,
    pub module_path: String,
    pub item_name: String,
    pub qualified_path: String,
    pub kind: String,
    pub file: String,
    pub line: u32,
    pub score: f32,
    pub doc_preview: Option<String>,
    pub signature: Option<String>,
    pub source_preview: Option<String>,
}

/// Code snippet with context.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SourceSnippet {
    pub code: String,
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub context_lines: u32,
    pub highlighted_line: Option<u32>,
    pub language: String,
}

/// Unified error domain stub (future: centralize).
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SharedTypeError {
    #[error("Conversion failure: {0}")]
    Conversion(String),
}

pub type SharedResult<T> = Result<T, SharedTypeError>;

/// Helper trait for fallible conversion into shared canonical types.
///
/// Implementations can be added externally to avoid cyclic deps:
/// e.g. impl TryIntoShared<shared_types::ItemDoc> for doc_engine::types::ItemDoc { ... }
pub trait TryIntoShared<T> {
    fn try_into_shared(self) -> SharedResult<T>;
}

/// Helper trait for converting *from* shared canonical forms.
/// (Symmetric with TryIntoShared for clearer intent.)
pub trait TryFromShared<T> {
    fn try_from_shared(value: T) -> SharedResult<Self>
    where
        Self: Sized;
}

/// Macro to implement trivial identity conversions for the shared set.
/// This reduces boilerplate if external code wants a uniform call site.
macro_rules! impl_identity_shared {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TryIntoShared<$ty> for $ty {
                fn try_into_shared(self) -> SharedResult<$ty> { Ok(self) }
            }
            impl TryFromShared<$ty> for $ty {
                fn try_from_shared(value: $ty) -> SharedResult<Self> { Ok(value) }
            }
        )*
    };
}
impl_identity_shared!(
    SourceLocation,
    ItemDoc,
    ImplItem,
    TraitImpl,
    TypeImpl,
    SearchIndexItem,
    SearchIndexData,
    SymbolSearchResult,
    SemanticSearchResult,
    SourceSnippet
);

/// Utility: map a collection with TryIntoShared.
pub fn map_vec_try_into<S, T, I>(iter: I) -> SharedResult<Vec<T>>
where
    I: IntoIterator<Item = S>,
    S: TryIntoShared<T>,
{
    iter.into_iter()
        .map(|s| s.try_into_shared())
        .collect::<SharedResult<Vec<T>>>()
}

/// Utility: shallow merge of doc summaries where the canonical result is missing.
/// Returns updated target reference.
pub fn backfill_doc_summary(target: &mut SymbolSearchResult, fallback: &SymbolSearchResult) {
    if target.doc_summary.is_none() && fallback.doc_summary.is_some() {
        target.doc_summary = fallback.doc_summary.clone();
    }
}
