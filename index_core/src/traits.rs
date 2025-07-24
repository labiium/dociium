//! Trait implementation indexing and querying functionality

use anyhow::Result;
use fnv::FnvHashMap;
use serde::{Deserialize, Serialize};

use tracing::{debug, info};

use crate::types::*;

/// Search index data from docs.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexData {
    pub crate_name: String,
    pub version: String,
    pub items: Vec<SearchIndexItem>,
    pub paths: Vec<String>,
}

/// Individual item in the search index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexItem {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub description: String,
    pub parent_index: Option<usize>,
}

/// Index for trait implementations
#[derive(Debug, Clone)]
pub struct TraitImplIndex {
    /// Map from trait path to implementations
    trait_to_impls: FnvHashMap<String, Vec<TraitImpl>>,
    /// Map from type path to trait implementations
    type_to_impls: FnvHashMap<String, Vec<TypeImpl>>,
    /// Raw implementation data
    implementations: FnvHashMap<String, ImplData>,
    /// Trait definitions
    traits: FnvHashMap<String, TraitData>,
    /// Type definitions
    types: FnvHashMap<String, TypeData>,
}

/// Internal representation of an implementation
#[derive(Debug, Clone)]
struct ImplData {
    id: String,
    trait_id: Option<String>,
    for_type: String,
    generics: Vec<String>,
    where_clause: Option<String>,
    items: Vec<String>,
    is_blanket: bool,
    is_synthetic: bool,
    source_location: Option<SourceLocation>,
}

/// Internal representation of a trait
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TraitData {
    id: String,
    name: String,
    path: String,
    generics: Vec<String>,
    items: Vec<String>,
}

/// Internal representation of a type
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TypeData {
    id: String,
    name: String,
    path: String,
    kind: String,
}

impl TraitImplIndex {
    /// Create a new trait implementation index
    pub fn new() -> Self {
        Self {
            trait_to_impls: FnvHashMap::default(),
            type_to_impls: FnvHashMap::default(),
            implementations: FnvHashMap::default(),
            traits: FnvHashMap::default(),
            types: FnvHashMap::default(),
        }
    }

    /// Build the index from search index data
    pub fn from_search_index(search_data: &SearchIndexData) -> Result<Self> {
        let mut index = Self::new();
        index.build_from_search_index(search_data)?;
        Ok(index)
    }

    /// Build the index from search index data
    fn build_from_search_index(&mut self, search_data: &SearchIndexData) -> Result<()> {
        info!("Building trait implementation index from search data");

        // Process search index items
        for item in &search_data.items {
            match item.kind.as_str() {
                "trait" => {
                    let trait_data = TraitData {
                        id: item.path.clone(),
                        name: item.name.clone(),
                        path: item.path.clone(),
                        generics: Vec::new(), // TODO: Extract from description/signature
                        items: Vec::new(),
                    };
                    self.traits.insert(item.path.clone(), trait_data);
                }
                "struct" | "enum" | "union" => {
                    let type_data = TypeData {
                        id: item.path.clone(),
                        name: item.name.clone(),
                        path: item.path.clone(),
                        kind: item.kind.clone(),
                    };
                    self.types.insert(item.path.clone(), type_data);
                }
                "impl" => {
                    // Process implementation - this is limited by search index data
                    let impl_data = ImplData {
                        id: item.path.clone(),
                        trait_id: None, // TODO: Extract from description
                        for_type: item.name.clone(),
                        generics: Vec::new(),
                        where_clause: None,
                        items: Vec::new(),
                        is_blanket: false,
                        is_synthetic: false,
                        source_location: None,
                    };
                    self.implementations.insert(item.path.clone(), impl_data);
                }
                _ => {}
            }
        }

        // Build lookup maps with available data
        self.build_lookup_maps_from_search_data(search_data)?;

        info!(
            "Built trait implementation index from search data with {} traits, {} types, {} implementations",
            self.traits.len(),
            self.types.len(),
            self.implementations.len()
        );

        Ok(())
    }

    /// Build lookup maps from search index data (limited functionality)
    fn build_lookup_maps_from_search_data(&mut self, _search_data: &SearchIndexData) -> Result<()> {
        debug!("Building trait implementation lookup maps from search data");

        // Note: Search index has limited trait implementation information
        // This is a simplified version that works with available data

        for impl_data in self.implementations.values() {
            // For implementations found in search index
            if let Some(trait_id) = &impl_data.trait_id {
                if let Some(trait_data) = self.traits.get(trait_id) {
                    let trait_impl = TraitImpl {
                        for_type: impl_data.for_type.clone(),
                        trait_path: trait_data.path.clone(),
                        generics: impl_data.generics.clone(),
                        where_clause: impl_data.where_clause.clone(),
                        source_span: impl_data.source_location.clone(),
                        impl_id: impl_data.id.clone(),
                        items: self.build_impl_items_from_search(&impl_data.items),
                        is_blanket: impl_data.is_blanket,
                        is_synthetic: impl_data.is_synthetic,
                    };

                    self.trait_to_impls
                        .entry(trait_data.path.clone())
                        .or_default()
                        .push(trait_impl);
                }
            }
        }

        debug!(
            "Built lookup maps from search data: {} trait entries, {} type entries",
            self.trait_to_impls.len(),
            self.type_to_impls.len()
        );

        Ok(())
    }

    /// Get all implementations of a trait
    pub fn get_trait_impls(&self, trait_path: &str) -> Result<Vec<TraitImpl>> {
        Ok(self
            .trait_to_impls
            .get(trait_path)
            .cloned()
            .unwrap_or_default())
    }

    /// Get all trait implementations for a type
    pub fn get_type_impls(&self, type_path: &str) -> Result<Vec<TypeImpl>> {
        Ok(self
            .type_to_impls
            .get(type_path)
            .cloned()
            .unwrap_or_default())
    }

    /// Get all available traits
    pub fn get_all_traits(&self) -> Vec<String> {
        self.trait_to_impls.keys().cloned().collect()
    }

    /// Get all types with implementations
    pub fn get_all_types_with_impls(&self) -> Vec<String> {
        self.type_to_impls.keys().cloned().collect()
    }

    /// Search for traits by name pattern
    pub fn search_traits(&self, pattern: &str) -> Vec<String> {
        let pattern_lower = pattern.to_lowercase();
        self.trait_to_impls
            .keys()
            .filter(|trait_path| trait_path.to_lowercase().contains(&pattern_lower))
            .cloned()
            .collect()
    }

    /// Search for types by name pattern
    pub fn search_types(&self, pattern: &str) -> Vec<String> {
        let pattern_lower = pattern.to_lowercase();
        self.type_to_impls
            .keys()
            .filter(|type_path| type_path.to_lowercase().contains(&pattern_lower))
            .cloned()
            .collect()
    }

    /// Get statistics about the index
    pub fn get_stats(&self) -> TraitImplStats {
        TraitImplStats {
            total_traits: self.traits.len(),
            total_types: self.types.len(),
            total_implementations: self.implementations.len(),
            traits_with_impls: self.trait_to_impls.len(),
            types_with_impls: self.type_to_impls.len(),
            blanket_implementations: self
                .implementations
                .values()
                .filter(|impl_data| impl_data.is_blanket)
                .count(),
            synthetic_implementations: self
                .implementations
                .values()
                .filter(|impl_data| impl_data.is_synthetic)
                .count(),
        }
    }

    // Helper methods for search index data

    /// Build implementation items from search index (simplified)
    fn build_impl_items_from_search(&self, item_paths: &[String]) -> Vec<ImplItem> {
        item_paths
            .iter()
            .enumerate()
            .map(|(i, path)| ImplItem {
                name: path
                    .split("::")
                    .last()
                    .unwrap_or(&format!("item_{i}"))
                    .to_string(),
                kind: "method".to_string(), // Assume methods for impl items
                signature: None,
                doc: None,
                source_location: None,
            })
            .collect()
    }
}

impl Default for TraitImplIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about trait implementations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitImplStats {
    pub total_traits: usize,
    pub total_types: usize,
    pub total_implementations: usize,
    pub traits_with_impls: usize,
    pub types_with_impls: usize,
    pub blanket_implementations: usize,
    pub synthetic_implementations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_impl_index_creation() {
        let index = TraitImplIndex::new();
        assert_eq!(index.traits.len(), 0);
        assert_eq!(index.types.len(), 0);
        assert_eq!(index.implementations.len(), 0);
    }

    #[test]
    fn test_from_search_index() {
        let search_data = SearchIndexData {
            crate_name: "test_crate".to_string(),
            version: "1.0.0".to_string(),
            items: vec![
                SearchIndexItem {
                    name: "TestTrait".to_string(),
                    kind: "trait".to_string(),
                    path: "test_crate::TestTrait".to_string(),
                    description: "A test trait".to_string(),
                    parent_index: None,
                },
                SearchIndexItem {
                    name: "TestStruct".to_string(),
                    kind: "struct".to_string(),
                    path: "test_crate::TestStruct".to_string(),
                    description: "A test struct".to_string(),
                    parent_index: None,
                },
            ],
            paths: vec!["test_crate".to_string()],
        };

        let index = TraitImplIndex::from_search_index(&search_data).unwrap();
        assert_eq!(index.traits.len(), 1);
        assert_eq!(index.types.len(), 1);
    }

    #[test]
    fn test_build_impl_items_from_search() {
        let index = TraitImplIndex::new();

        let item_paths = vec![
            "test_crate::MyStruct::method1".to_string(),
            "test_crate::MyStruct::method2".to_string(),
        ];

        let impl_items = index.build_impl_items_from_search(&item_paths);
        assert_eq!(impl_items.len(), 2);
        assert_eq!(impl_items[0].name, "method1");
        assert_eq!(impl_items[1].name, "method2");
        assert_eq!(impl_items[0].kind, "method");
    }
}
