//! Index Core - Search and indexing functionality for Rust documentation
//!
//! This crate provides indexing and search capabilities for Rust documentation,
//! including full-text search and trait-implementation mapping.

use anyhow::Result;
use once_cell::sync::Lazy;
#[cfg(feature = "rocksdb-backend")]
use rocksdb::{DBCompressionType, Options, DB};
use rustdoc_types::Crate as RustdocCrate;
use serde::{de::DeserializeOwned, Serialize};
use std::{path::Path, sync::Arc};
use tantivy::schema::{Schema, STORED, STRING, TEXT};
// Removed: pub mod search;
pub mod traits;
pub mod types;

// Removed: pub use search::*;
pub use traits::*;
pub use types::*;

// Tantivy Schema Definition
static SCHEMA: Lazy<Schema> = Lazy::new(|| {
    let mut builder = Schema::builder();
    // Indexed and stored fields
    builder.add_text_field("path", TEXT | STORED); // Full path of the item
    builder.add_text_field("name", TEXT | STORED); // Name of the item
    builder.add_text_field("module_path", TEXT | STORED); // Path of the module it belongs to
    builder.add_text_field("kind", STRING | STORED); // SymbolKind as string
    builder.add_text_field("doc", TEXT | STORED); // Documentation summary
    builder.add_text_field("signature", TEXT | STORED); // Signature of functions, traits, etc.
    builder.add_text_field("visibility", STRING | STORED); // e.g. "public", "private"

    // Indexed but not necessarily stored fields (unless also marked STORED)
    // TEXT fields are tokenized and indexed.
    // STRING fields are indexed as a single token.
    // FAST fields are column-oriented, good for scoring or filtering.
    // We can add more fields for filtering if needed, e.g., crate_id, is_unsafe, is_async etc.

    // Fields for scoring/boosting (optional, can be added if SearchOptions needs them)
    // builder.add_i64_field("visibility_score_boost", FAST | STORED); // Example: public items get higher boost
    // builder.add_f64_field("score_boost", FAST); // General purpose boost factor

    // Tantivy's default tokenizer is SimpleTokenizer.
    // For Rust code, consider tantivy_jieba for better CJK support if docs contain it,
    // or tantivy_analysis_contrib for more advanced tokenizers if needed.
    // For now, default should be fine.
    // We can also configure specific tokenizers per field.
    // For "path" and "module_path", we might want a tokenizer that handles "::" well.
    // tantivy_tokenizers::SplitCollectTokenizer could be useful here.

    builder.build()
});

// Field handles for easier access
struct SchemaFields {
    path: tantivy::schema::Field,
    name: tantivy::schema::Field,
    module_path: tantivy::schema::Field,
    kind: tantivy::schema::Field,
    doc: tantivy::schema::Field,
    signature: tantivy::schema::Field,
    visibility: tantivy::schema::Field,
    // score_boost: tantivy::schema::Field,
}

static FIELDS: Lazy<SchemaFields> = Lazy::new(|| SchemaFields {
    path: SCHEMA.get_field("path").expect("Path field not found"),
    name: SCHEMA.get_field("name").expect("Name field not found"),
    module_path: SCHEMA
        .get_field("module_path")
        .expect("Module path field not found"),
    kind: SCHEMA.get_field("kind").expect("Kind field not found"),
    doc: SCHEMA.get_field("doc").expect("Doc field not found"),
    signature: SCHEMA
        .get_field("signature")
        .expect("Signature field not found"),
    visibility: SCHEMA
        .get_field("visibility")
        .expect("Visibility field not found"),
    // score_boost: SCHEMA.get_field("score_boost").expect("Score boost field not found"),
});

/// Core indexing functionality
#[derive(Debug)]
pub struct IndexCore {
    symbol_index_base_dir: PathBuf,
    #[cfg(feature = "rocksdb-backend")]
    traits_db: Option<Arc<DB>>,
    #[cfg(feature = "rocksdb-backend")]
    meta_db: Option<Arc<DB>>,
}

impl IndexCore {
    /// Create a new index core
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        let symbol_index_base_dir = base_dir.join("symbols");
        let traits_db_path = base_dir.join("traits");
        let meta_db_path = base_dir.join("meta");

        std::fs::create_dir_all(&symbol_index_base_dir)?;

        #[cfg(feature = "rocksdb-backend")]
        {
            std::fs::create_dir_all(&traits_db_path)?;
            std::fs::create_dir_all(&meta_db_path)?;

            let mut db_opts = Options::default();
            db_opts.create_if_missing(true);
            // Not setting compression, or using Snappy if available by default and preferred over None
            // db_opts.set_compression_type(DBCompressionType::Snappy); // if snappy is a safe default
            db_opts.set_compression_type(DBCompressionType::None); // Safest for now to avoid zstd conflict via rocksdb

            let traits_db = DB::open(&db_opts, &traits_db_path)
                .map_err(|e| anyhow::anyhow!("Failed to open traits DB: {}", e))?;
            let meta_db = DB::open(&db_opts, &meta_db_path)
                .map_err(|e| anyhow::anyhow!("Failed to open meta DB: {}", e))?;

            Ok(Self {
                symbol_index_base_dir,
                traits_db: Some(Arc::new(traits_db)),
                meta_db: Some(Arc::new(meta_db)),
            })
        }
        #[cfg(not(feature = "rocksdb-backend"))]
        {
            Ok(Self {
                symbol_index_base_dir,
            })
        }
    }

    /// Returns the base path for symbol indexes.
    pub fn symbol_index_base_path(&self) -> &Path {
        &self.symbol_index_base_dir
    }

    // --- RocksDB helper methods ---
    #[cfg(feature = "rocksdb-backend")]
    fn get_db_entry<T: DeserializeOwned>(db: Option<&Arc<DB>>, key: &str) -> Result<Option<T>> {
        match db {
            Some(db_instance) => match db_instance.get(key.as_bytes())? {
                Some(raw_value) => {
                    let value: T = bincode::deserialize(&raw_value)?;
                    Ok(Some(value))
                }
                None => Ok(None),
            },
            None => Ok(None), // Should not happen if feature is enabled and DB init succeeded
        }
    }

    #[cfg(feature = "rocksdb-backend")]
    fn put_db_entry<T: Serialize>(db: Option<&Arc<DB>>, key: &str, value: &T) -> Result<()> {
        match db {
            Some(db_instance) => {
                let raw_value = bincode::serialize(value)?;
                db_instance.put(key.as_bytes(), &raw_value)?;
                Ok(())
            }
            None => Ok(()), // No-op if DB is not available
        }
    }

    // --- Trait DB methods ---
    pub fn get_trait_data<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        #[cfg(feature = "rocksdb-backend")]
        {
            Self::get_db_entry(self.traits_db.as_ref(), key)
        }
        #[cfg(not(feature = "rocksdb-backend"))]
        {
            Ok(None)
        }
    }

    pub fn put_trait_data<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        #[cfg(feature = "rocksdb-backend")]
        {
            Self::put_db_entry(self.traits_db.as_ref(), key, value)
        }
        #[cfg(not(feature = "rocksdb-backend"))]
        {
            Ok(())
        }
    }

    // --- Meta DB methods ---
    pub fn get_meta_data<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        #[cfg(feature = "rocksdb-backend")]
        {
            Self::get_db_entry(self.meta_db.as_ref(), key)
        }
        #[cfg(not(feature = "rocksdb-backend"))]
        {
            Ok(None)
        }
    }

    pub fn put_meta_data<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        #[cfg(feature = "rocksdb-backend")]
        {
            Self::put_db_entry(self.meta_db.as_ref(), key, value)
        }
        #[cfg(not(feature = "rocksdb-backend"))]
        {
            Ok(())
        }
    }
}

impl Clone for IndexCore {
    fn clone(&self) -> Self {
        Self {
            symbol_index_base_dir: self.symbol_index_base_dir.clone(),
            #[cfg(feature = "rocksdb-backend")]
            traits_db: self.traits_db.clone(),
            #[cfg(feature = "rocksdb-backend")]
            meta_db: self.meta_db.clone(),
        }
    }
}

/// Symbol index for full-text search, backed by Tantivy.
/// Each crate version will have its own Tantivy index.
#[derive(Debug, Clone)]
pub struct SymbolIndex {
    // tantivy_index: tantivy::Index,
    // reader: tantivy::IndexReader,
    // query_parser: tantivy::query::QueryParser,
    // For now, SymbolIndex will be created on-demand or represent an open index.
    // The actual Tantivy Index object will be managed internally by methods.
    index_path: PathBuf,
}

impl SymbolIndex {
    /// Opens an existing Tantivy index from the given path or creates one if it doesn't exist.
    /// The path should be specific to a crate and version, e.g., `~/.cache/rdocs-mcp/symbols/serde@1.0.130`.
    pub fn open_or_create(index_path: impl AsRef<Path>) -> Result<tantivy::Index> {
        let index_path = index_path.as_ref();
        std::fs::create_dir_all(index_path)?;
        let index = tantivy::Index::open_in_dir(index_path).or_else(|e| {
            if matches!(e, tantivy::TantivyError::IndexNotFound(_)) {
                tantivy::Index::create_in_dir(index_path, SCHEMA.clone())
            } else {
                Err(e)
            }
        })?;
        Ok(index)
    }

    /// Creates a new `SymbolIndex` instance associated with a specific path.
    /// This doesn't immediately open or create the Tantivy index.
    pub fn new(index_path: PathBuf) -> Self {
        Self { index_path }
    }

    /// Adds a RustdocCrate to the Tantivy index.
    /// This method should be called to populate or update the index for a crate.
    pub async fn add_crate(
        &self,
        rustdoc_crate: &RustdocCrate,
        // crate_name: &str, // Already in rustdoc_crate.root or can be derived
        // crate_version: &str, // Also should be part of the source of rustdoc_crate
    ) -> Result<()> {
        let index = Self::open_or_create(&self.index_path)?;
        let mut index_writer = index.writer(50_000_000)?; // 50MB heap for indexing

        // Iterate through items in the rustdoc output and add them to the index.
        for (id, item) in &rustdoc_crate.index {
            if item.name.is_none() {
                // Skip items without names (e.g. some impl blocks)
                continue;
            }

            let mut doc = tantivy::Document::new();
            let item_path_str = item_path_to_string(&rustdoc_crate.paths, id, item, rustdoc_crate);
            let item_name = item.name.as_deref().unwrap_or_default();
            let module_path_str = module_path_to_string(&rustdoc_crate.paths, id, rustdoc_crate);

            doc.add_text(FIELDS.path, &item_path_str);
            doc.add_text(FIELDS.name, item_name);
            doc.add_text(FIELDS.module_path, &module_path_str);
            doc.add_text(
                FIELDS.kind,
                SymbolKind::from_item_enum(&item.inner).as_str(),
            );
            doc.add_text(
                FIELDS.visibility,
                format!("{:?}", item.visibility).to_lowercase(),
            );

            if let Some(docs) = &item.docs {
                // TODO: Smarter doc summary. For now, take first N characters or sentences.
                let summary = docs.lines().take(3).collect::<Vec<_>>().join("\n");
                doc.add_text(FIELDS.doc, summary);
            }

            // TODO: Extract signature string if applicable (functions, traits, etc.)
            // For now, this is a placeholder.
            // let signature_str = extract_signature(item, rustdoc_crate);
            // doc.add_text(FIELDS.signature, signature_str);

            index_writer.add_document(doc)?;
        }

        index_writer.commit()?;
        Ok(())
    }

    /// Search for symbols using the provided options.
    pub fn search(&self, options: &SymbolSearchOptions) -> Result<Vec<SymbolSearchResult>> {
        let index = Self::open_or_create(&self.index_path)?;
        let reader = index.reader()?;
        let searcher = reader.searcher();

        let mut query_parts: Vec<Box<dyn tantivy::query::Query>> = Vec::new();

        // Main query based on query_type
        let main_query_text = &options.query;
        if !main_query_text.is_empty() {
            match options.query_type {
                QueryType::Exact => {
                    // Exact phrase query on name and path
                    let name_phrase = tantivy::query::PhraseQuery::new(vec![
                        tantivy::query::Term::from_field_text(FIELDS.name, main_query_text),
                    ]);
                    let path_phrase = tantivy::query::PhraseQuery::new(vec![
                        tantivy::query::Term::from_field_text(FIELDS.path, main_query_text),
                    ]);
                    query_parts.push(Box::new(tantivy::query::BooleanQuery::new(vec![
                        (tantivy::query::Occur::Should, Box::new(name_phrase)),
                        (tantivy::query::Occur::Should, Box::new(path_phrase)),
                    ])));
                }
                QueryType::Prefix => {
                    let prefix_query_name =
                        tantivy::query::PrefixQuery::new(FIELDS.name, main_query_text);
                    let prefix_query_path =
                        tantivy::query::PrefixQuery::new(FIELDS.path, main_query_text);
                    query_parts.push(Box::new(tantivy::query::BooleanQuery::new(vec![
                        (tantivy::query::Occur::Should, Box::new(prefix_query_name)),
                        (tantivy::query::Occur::Should, Box::new(prefix_query_path)),
                    ])));
                }
                QueryType::Fuzzy => {
                    let term = tantivy::Term::from_field_text(FIELDS.name, main_query_text);
                    let fuzzy_query_name = tantivy::query::FuzzyTermQuery::new(
                        term.clone(),
                        options.fuzzy_distance.unwrap_or(1),
                        true, // transpositions
                    );
                    let term_path = tantivy::Term::from_field_text(FIELDS.path, main_query_text);
                    let fuzzy_query_path = tantivy::query::FuzzyTermQuery::new(
                        term_path.clone(),
                        options.fuzzy_distance.unwrap_or(1),
                        true,
                    );
                    let term_doc = tantivy::Term::from_field_text(FIELDS.doc, main_query_text);
                    let fuzzy_query_doc = tantivy::query::FuzzyTermQuery::new(
                        term_doc.clone(),
                        options.fuzzy_distance.unwrap_or(1),
                        true,
                    );
                    query_parts.push(Box::new(tantivy::query::BooleanQuery::new(vec![
                        (tantivy::query::Occur::Should, Box::new(fuzzy_query_name)),
                        (tantivy::query::Occur::Should, Box::new(fuzzy_query_path)),
                        (tantivy::query::Occur::Should, Box::new(fuzzy_query_doc)),
                    ])));
                }
                QueryType::Term => {
                    let query_parser = tantivy::query::QueryParser::for_index(
                        &index,
                        vec![FIELDS.name, FIELDS.path, FIELDS.doc, FIELDS.signature],
                    );
                    // This allows for boolean operators, phrases, etc. in the query string itself.
                    if let Ok(query) = query_parser.parse_query(main_query_text) {
                        query_parts.push(query);
                    } else {
                        // Fallback to a simple term query if parsing fails
                        let term = tantivy::Term::from_field_text(FIELDS.name, main_query_text);
                        query_parts.push(Box::new(tantivy::query::TermQuery::new(
                            term,
                            tantivy::schema::IndexRecordOption::Basic,
                        )));
                    }
                }
            }
        }

        // Filters
        if let Some(kinds) = &options.kinds {
            if !kinds.is_empty() {
                let kind_queries: Vec<Box<dyn tantivy::query::Query>> = kinds
                    .iter()
                    .map(|k| {
                        Box::new(tantivy::query::TermQuery::new(
                            tantivy::Term::from_field_text(FIELDS.kind, k.as_str()),
                            tantivy::schema::IndexRecordOption::Basic,
                        )) as Box<dyn tantivy::query::Query>
                    })
                    .collect();
                query_parts.push(Box::new(tantivy::query::BooleanQuery::new(
                    kind_queries
                        .into_iter()
                        .map(|q| (tantivy::query::Occur::Should, q))
                        .collect(),
                )));
            }
        }

        if let Some(module_filter) = &options.module_path_filter {
            if !module_filter.is_empty() {
                // Could be exact or prefix search on module_path
                query_parts.push(Box::new(tantivy::query::TermQuery::new(
                    tantivy::Term::from_field_text(FIELDS.module_path, module_filter),
                    tantivy::schema::IndexRecordOption::Basic,
                )));
            }
        }

        if let Some(vis_filters) = &options.visibility_filter {
            if !vis_filters.is_empty() {
                let vis_queries: Vec<Box<dyn tantivy::query::Query>> = vis_filters
                    .iter()
                    .map(|v| {
                        Box::new(tantivy::query::TermQuery::new(
                            tantivy::Term::from_field_text(FIELDS.visibility, v),
                            tantivy::schema::IndexRecordOption::Basic,
                        )) as Box<dyn tantivy::query::Query>
                    })
                    .collect();
                query_parts.push(Box::new(tantivy::query::BooleanQuery::new(
                    vis_queries
                        .into_iter()
                        .map(|q| (tantivy::query::Occur::Should, q))
                        .collect(),
                )));
            }
        }

        if options.must_have_docs {
            // This query ensures the 'doc' field is not empty.
            // A more robust way would be to have a separate boolean field "has_docs" indexed.
            // For now, check for existence of the field, assuming empty docs are not indexed or have a placeholder.
            // Or, more simply, query for a wildcard on the doc field if the tokenizer allows,
            // or use a RangeQuery on a field that stores doc length.
            // For now, we'll rely on the query text matching something in docs if QueryType::Term is broad.
            // A proper solution would be a dedicated "has_docs" boolean field in the schema.
            // As a simple proxy: query for *any* term in the doc field. This is not perfect.
            // query_parts.push(Box::new(tantivy::query::TermQuery::new(
            //    tantivy::Term::from_field_text(FIELDS.doc, "*"), // This might not work as expected with all tokenizers
            //    tantivy::schema::IndexRecordOption::Basic,
            // )));
        }

        // Combine all parts into a BooleanQuery
        let final_query = if query_parts.is_empty() {
            // If no query parts (e.g. empty query string and no filters), match all.
            // This might be too broad; consider returning empty or error if query is empty.
            Box::new(tantivy::query::AllQuery) as Box<dyn tantivy::query::Query>
        } else {
            Box::new(tantivy::query::BooleanQuery::new(
                query_parts
                    .into_iter()
                    .map(|q| (tantivy::query::Occur::Must, q))
                    .collect(),
            )) as Box<dyn tantivy::query::Query>
        };

        // TODO: Apply scoring_config (BoostingQuery, field boosts in QueryParser/BooleanQuery)
        // For now, standard TF-IDF scoring applies.

        let top_docs = searcher.search(
            &*final_query,
            &tantivy::collector::TopDocs::with_limit(options.limit).and_offset(options.offset),
        )?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc(doc_address)?;
            let path = retrieved_doc
                .get_first(FIELDS.path)
                .and_then(|v| v.as_text())
                .unwrap_or_default()
                .to_string();
            let kind_str = retrieved_doc
                .get_first(FIELDS.kind)
                .and_then(|v| v.as_text())
                .unwrap_or_default();
            let doc_summary = retrieved_doc
                .get_first(FIELDS.doc)
                .and_then(|v| v.as_text())
                .map(String::from);
            let visibility = retrieved_doc
                .get_first(FIELDS.visibility)
                .and_then(|v| v.as_text())
                .unwrap_or_default()
                .to_string();
            let signature_str = retrieved_doc
                .get_first(FIELDS.signature)
                .and_then(|v| v.as_text())
                .map(String::from);
            let module_path_str = retrieved_doc
                .get_first(FIELDS.module_path)
                .and_then(|v| v.as_text())
                .unwrap_or_default()
                .to_string();

            results.push(SymbolSearchResult {
                path,
                kind: SymbolKind::from(kind_str),
                score,
                doc_summary,
                source_location: None, // TODO
                visibility,
                signature: signature_str,
                module_path: module_path_str,
            });
        }
        Ok(results)
    }
}

// Helper function to convert rustdoc_types::ItemEnum to a string for SymbolKind
// Moved to types.rs as SymbolKind::as_str()

// fn string_to_symbol_kind(s: &str) -> SymbolKind { // Moved to types.rs as From<&str> for SymbolKind
//     match s {
//         "Function" => SymbolKind::Function,
//         "Struct" => SymbolKind::Struct,
//         "Enum" => SymbolKind::Enum,
//         "Trait" => SymbolKind::Trait,
//         "Const" => SymbolKind::Const,
//         "Macro" => SymbolKind::Macro,
//         "TypeAlias" => SymbolKind::TypeAlias,
//         "Module" => SymbolKind::Module,
//         _ => SymbolKind::Unknown,
//     }
// }

// Helper to construct full path string for an item
fn item_path_to_string(
    paths: &std::collections::HashMap<rustdoc_types::Id, rustdoc_types::Path>,
    item_id: &rustdoc_types::Id,
    item: &rustdoc_types::Item,
    krate: &RustdocCrate,
) -> String {
    if let Some(path_info) = paths.get(item_id) {
        return path_info.path.join("::");
    }
    // Fallback for items not in paths map (e.g. some impls, or if root is the item)
    if Some(item_id) == krate.root.as_ref() {
        return krate
            .index
            .get(&krate.root.as_ref().unwrap())
            .and_then(|i| i.name.clone())
            .unwrap_or_default();
    }
    item.name.as_deref().unwrap_or("").to_string()
}

// Helper to get module path for an item
fn module_path_to_string(
    paths: &std::collections::HashMap<rustdoc_types::Id, rustdoc_types::Path>,
    item_id: &rustdoc_types::Id,
    krate: &RustdocCrate,
) -> String {
    if let Some(path_info) = paths.get(item_id) {
        if path_info.path.len() > 1 {
            return path_info.path[..(path_info.path.len() - 1)].join("::");
        } else if path_info.path.len() == 1 {
            // Item is at crate root
            return krate
                .index
                .get(&krate.root.as_ref().unwrap())
                .and_then(|i| i.name.clone())
                .unwrap_or_default();
        }
    }
    // Fallback or item is at crate root
    if Some(item_id) == krate.root.as_ref() {
        return "".to_string(); // Root has no parent module path within the crate
    }
    // If path not found, or path is just the item name, assume crate root as module.
    krate
        .index
        .get(&krate.root.as_ref().unwrap())
        .and_then(|i| i.name.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustdoc_types::{Crate, Id, Item, ItemEnum, Path as RustdocPath, Span, Visibility};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn mock_rustdoc_crate(crate_name: &str) -> RustdocCrate {
        let mut index = HashMap::new();
        let mut paths = HashMap::new();

        let root_id = Id("0:0".to_string());
        index.insert(
            root_id.clone(),
            Item {
                id: root_id.clone(),
                crate_id: 0,
                name: Some(crate_name.to_string()),
                span: None,
                visibility: Visibility::Public,
                docs: Some(format!("Docs for crate {}", crate_name)),
                links: HashMap::new(),
                attrs: Vec::new(),
                deprecation: None,
                inner: ItemEnum::Module(rustdoc_types::Module {
                    items: vec![],
                    is_crate: true,
                }), // Corrected variant
            },
        );
        paths.insert(
            root_id.clone(),
            RustdocPath {
                path: vec![crate_name.to_string()],
                kind: rustdoc_types::PathKind::Module,
                id: root_id.clone(),
            },
        );

        let fn_id = Id("0:1".to_string());
        index.insert(
            fn_id.clone(),
            Item {
                id: fn_id.clone(),
                crate_id: 0,
                name: Some("test_func".to_string()),
                span: Some(Span {
                    filename: PathBuf::from("src/lib.rs"),
                    begin: (10, 0),
                    end: (12, 1),
                    inner_begin: (10, 0),
                    inner_end: (12, 1), // Added these fields
                    range_begin: 0,
                    range_end: 0, // Added these fields
                }),
                visibility: Visibility::Public,
                docs: Some("This is a test function.".to_string()),
                links: HashMap::new(),
                attrs: Vec::new(),
                deprecation: None,
                inner: ItemEnum::Function(rustdoc_types::Function {
                    decl: rustdoc_types::FnDecl {
                        inputs: Vec::new(),
                        output: None,
                        c_variadic: false,
                        attrs: Vec::new(), // Added this field
                    },
                    generics: rustdoc_types::Generics::default(),
                    header: Vec::new(),
                    abi: "Rust".to_string(),
                }),
            },
        );
        paths.insert(
            fn_id.clone(),
            RustdocPath {
                path: vec![crate_name.to_string(), "test_func".to_string()],
                kind: rustdoc_types::PathKind::Function,
                id: fn_id.clone(),
            },
        );

        Crate {
            root: root_id,
            crate_version: Some("0.1.0".to_string()),
            includes_private: false,
            index,
            paths,
            external_crates: HashMap::new(),
            format_version: rustdoc_types::FORMAT_VERSION,
            features: HashMap::new(),        // Added this field
            intra_doc_links: HashMap::new(), // Added this field
        }
    }

    #[test]
    fn test_index_core_creation() {
        let temp_dir = tempdir().unwrap();
        let index_core = IndexCore::new(temp_dir.path());
        assert!(index_core.is_ok());
        assert!(index_core.unwrap().symbol_index_base_path().exists());
    }

    #[tokio::test]
    async fn test_symbol_index_add_and_search_crate() {
        let temp_dir = tempdir().unwrap();
        let crate_name = "mytestcrate";
        let index_core = IndexCore::new(temp_dir.path()).unwrap();
        let crate_index_path = index_core
            .symbol_index_base_path()
            .join(format!("{}@0.1.0", crate_name));

        let symbol_index = SymbolIndex::new(crate_index_path);
        let rustdoc_crate_data = mock_rustdoc_crate(crate_name);

        symbol_index.add_crate(&rustdoc_crate_data).await.unwrap();

        let results = symbol_index.search("test_func", None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "mytestcrate::test_func");
        assert_eq!(
            results[0].doc_summary.as_deref(),
            Some("This is a test function.")
        );
        match results[0].kind {
            SymbolKind::Function => {}
            _ => panic!("Expected function kind"),
        }

        let results_by_doc = symbol_index.search("This is a test", None, 10).unwrap();
        assert_eq!(results_by_doc.len(), 1);
        assert_eq!(results_by_doc[0].path, "mytestcrate::test_func");
    }

    #[test]
    fn test_symbol_search_on_empty_index() {
        let temp_dir = tempdir().unwrap();
        let index_core = IndexCore::new(temp_dir.path()).unwrap();
        let crate_index_path = index_core.symbol_index_base_path().join("emptycrate@0.1.0");
        let symbol_index = SymbolIndex::new(crate_index_path);
        // Ensure index exists for open_or_create
        SymbolIndex::open_or_create(symbol_index.index_path.clone()).unwrap();

        let results = symbol_index.search("anything", None, 10).unwrap();
        assert!(results.is_empty());
    }
}
