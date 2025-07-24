//! Trait implementation indexing and querying functionality

use anyhow::Result;
use fnv::FnvHashMap;
use rustdoc_types::{Crate as RustdocCrate, Id, Impl, Item, ItemEnum, Path, Type};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::types::*;

/// Index for trait implementations
#[derive(Debug, Clone)]
pub struct TraitImplIndex {
    /// Map from trait path to implementations
    trait_to_impls: FnvHashMap<String, Vec<TraitImpl>>,
    /// Map from type path to trait implementations
    type_to_impls: FnvHashMap<String, Vec<TypeImpl>>,
    /// Raw implementation data
    implementations: FnvHashMap<Id, ImplData>,
    /// Trait definitions
    traits: FnvHashMap<Id, TraitData>,
    /// Type definitions
    types: FnvHashMap<Id, TypeData>,
}

/// Internal representation of an implementation
#[derive(Debug, Clone)]
struct ImplData {
    id: Id,
    trait_id: Option<Id>,
    for_type: Type,
    generics: Vec<String>,
    where_clause: Option<String>,
    items: Vec<Id>,
    is_blanket: bool,
    is_synthetic: bool,
    source_location: Option<SourceLocation>,
}

/// Internal representation of a trait
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TraitData {
    id: Id,
    name: String,
    path: String,
    generics: Vec<String>,
    items: Vec<Id>,
}

/// Internal representation of a type
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TypeData {
    id: Id,
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

    /// Build the index from rustdoc data
    pub fn from_rustdoc(rustdoc_crate: &RustdocCrate) -> Result<Self> {
        let mut index = Self::new();
        index.build_from_rustdoc(rustdoc_crate)?;
        Ok(index)
    }

    /// Build the index from rustdoc crate data
    fn build_from_rustdoc(&mut self, rustdoc_crate: &RustdocCrate) -> Result<()> {
        info!("Building trait implementation index");

        // First pass: collect all traits and types
        for (id, item) in &rustdoc_crate.index {
            match &item.inner {
                ItemEnum::Trait(trait_item) => {
                    let trait_data = TraitData {
                        id: id.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        path: self.build_item_path(id, &rustdoc_crate.index),
                        generics: self.extract_generics(&trait_item.generics),
                        items: trait_item.items.clone(),
                    };
                    self.traits.insert(id.clone(), trait_data);
                }
                ItemEnum::Struct(_) | ItemEnum::Enum(_) | ItemEnum::Union(_) => {
                    let type_data = TypeData {
                        id: id.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        path: self.build_item_path(id, &rustdoc_crate.index),
                        kind: self.get_item_kind(&item.inner),
                    };
                    self.types.insert(id.clone(), type_data);
                }
                _ => {}
            }
        }

        // Second pass: process implementations
        for (id, item) in &rustdoc_crate.index {
            if let ItemEnum::Impl(impl_item) = &item.inner {
                self.process_implementation(id, impl_item, item, &rustdoc_crate.index)?;
            }
        }

        // Third pass: build lookup maps
        self.build_lookup_maps()?;

        info!(
            "Built trait implementation index with {} traits, {} types, {} implementations",
            self.traits.len(),
            self.types.len(),
            self.implementations.len()
        );

        Ok(())
    }

    /// Process a single implementation
    fn process_implementation(
        &mut self,
        impl_id: &Id,
        impl_item: &Impl,
        item: &Item,
        _index: &HashMap<Id, Item>,
    ) -> Result<()> {
        let impl_data = ImplData {
            id: impl_id.clone(),
            trait_id: impl_item.trait_.as_ref().map(|path| path.id.clone()),
            for_type: impl_item.for_.clone(),
            generics: self.extract_generics(&impl_item.generics),
            where_clause: self.extract_where_clause(&impl_item.generics),
            items: impl_item.items.clone(),
            is_blanket: impl_item.blanket_impl.is_some(),
            is_synthetic: impl_item.is_synthetic,
            source_location: item.span.as_ref().map(|span| SourceLocation {
                file: span.filename.to_string_lossy().to_string(),
                line: span.begin.0 as u32,
                column: span.begin.1 as u32,
                end_line: Some(span.end.0 as u32),
                end_column: Some(span.end.1 as u32),
            }),
        };

        self.implementations.insert(impl_id.clone(), impl_data);
        Ok(())
    }

    /// Build lookup maps for efficient querying
    fn build_lookup_maps(&mut self) -> Result<()> {
        debug!("Building trait implementation lookup maps");

        for impl_data in self.implementations.values() {
            // Map trait -> implementations
            if let Some(trait_id) = &impl_data.trait_id {
                if let Some(trait_data) = self.traits.get(trait_id) {
                    let trait_impl = TraitImpl {
                        for_type: self.type_to_string(&impl_data.for_type),
                        trait_path: trait_data.path.clone(),
                        generics: impl_data.generics.clone(),
                        where_clause: impl_data.where_clause.clone(),
                        source_span: impl_data.source_location.clone(),
                        impl_id: format!("{:?}", impl_data.id),
                        items: self.build_impl_items(&impl_data.items),
                        is_blanket: impl_data.is_blanket,
                        is_synthetic: impl_data.is_synthetic,
                    };

                    self.trait_to_impls
                        .entry(trait_data.path.clone())
                        .or_insert_with(Vec::new)
                        .push(trait_impl);
                }
            }

            // Map type -> trait implementations
            let for_type_str = self.type_to_string(&impl_data.for_type);
            if let Some(trait_id) = &impl_data.trait_id {
                if let Some(trait_data) = self.traits.get(trait_id) {
                    let type_impl = TypeImpl {
                        trait_path: trait_data.path.clone(),
                        generics: impl_data.generics.clone(),
                        where_clause: impl_data.where_clause.clone(),
                        source_span: impl_data.source_location.clone(),
                        impl_id: format!("{:?}", impl_data.id),
                        items: self.build_impl_items(&impl_data.items),
                        is_blanket: impl_data.is_blanket,
                        is_synthetic: impl_data.is_synthetic,
                    };

                    self.type_to_impls
                        .entry(for_type_str)
                        .or_insert_with(Vec::new)
                        .push(type_impl);
                }
            }
        }

        debug!(
            "Built lookup maps: {} trait entries, {} type entries",
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

    // Helper methods

    /// Build item path from ID
    fn build_item_path(&self, id: &Id, index: &HashMap<Id, Item>) -> String {
        // This is a simplified implementation
        // In practice, you'd need to traverse the module hierarchy
        if let Some(item) = index.get(id) {
            item.name.clone().unwrap_or_else(|| format!("{:?}", id))
        } else {
            format!("{:?}", id)
        }
    }

    /// Extract generics from rustdoc generics
    fn extract_generics(&self, generics: &rustdoc_types::Generics) -> Vec<String> {
        generics
            .params
            .iter()
            .filter_map(|param| match &param.kind {
                rustdoc_types::GenericParamDefKind::Type { .. } => Some(param.name.clone()),
                rustdoc_types::GenericParamDefKind::Const { .. } => Some(param.name.clone()),
                rustdoc_types::GenericParamDefKind::Lifetime { .. } => Some(param.name.clone()),
            })
            .collect()
    }

    /// Extract where clause from generics
    fn extract_where_clause(&self, generics: &rustdoc_types::Generics) -> Option<String> {
        if generics.where_predicates.is_empty() {
            None
        } else {
            // Simplified where clause extraction
            Some(format!(
                "where /* {} predicates */",
                generics.where_predicates.len()
            ))
        }
    }

    /// Extract trait ID from path reference
    #[allow(dead_code)]
    fn extract_trait_id(&self, trait_path: &Path) -> Id {
        trait_path.id.clone()
    }

    /// Convert type to string representation
    fn type_to_string(&self, ty: &Type) -> String {
        match ty {
            Type::ResolvedPath(path) => path.path.clone(),
            Type::DynTrait(dyn_trait) => {
                format!(
                    "dyn {}",
                    dyn_trait
                        .traits
                        .first()
                        .map(|t| t.trait_.path.clone())
                        .unwrap_or_else(|| "Trait".to_string())
                )
            }
            Type::Generic(name) => name.clone(),
            Type::Primitive(name) => name.clone(),
            Type::FunctionPointer(_) => "fn".to_string(),
            Type::Tuple(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| self.type_to_string(t)).collect();
                format!("({})", type_strs.join(", "))
            }
            Type::Slice(inner) => format!("[{}]", self.type_to_string(inner)),
            Type::Array { type_, len } => format!("[{}; {}]", self.type_to_string(type_), len),
            Type::ImplTrait(bounds) => format!("impl {}", bounds.len()),
            Type::Infer => "_".to_string(),
            Type::RawPointer { is_mutable, type_ } => {
                format!(
                    "*{} {}",
                    if *is_mutable { "mut" } else { "const" },
                    self.type_to_string(type_)
                )
            }
            Type::BorrowedRef {
                lifetime: _,
                is_mutable,
                type_,
            } => {
                format!(
                    "&{} {}",
                    if *is_mutable { "mut " } else { "" },
                    self.type_to_string(type_)
                )
            }
            Type::QualifiedPath {
                name,
                args: _,
                self_type,
                trait_,
            } => {
                format!(
                    "<{} as {}>::{}",
                    self.type_to_string(self_type),
                    trait_.as_ref().map(|t| t.path.clone()).unwrap_or_default(),
                    name
                )
            }
            Type::Pat { .. } => "pattern".to_string(),
        }
    }

    /// Get item kind as string
    fn get_item_kind(&self, item: &ItemEnum) -> String {
        match item {
            ItemEnum::Struct(_) => "struct".to_string(),
            ItemEnum::Enum(_) => "enum".to_string(),
            ItemEnum::Union(_) => "union".to_string(),
            ItemEnum::Trait(_) => "trait".to_string(),
            ItemEnum::Function(_) => "function".to_string(),
            ItemEnum::TypeAlias(_) => "type_alias".to_string(),
            ItemEnum::Constant { .. } => "constant".to_string(),
            ItemEnum::Static(_) => "static".to_string(),
            ItemEnum::Macro(_) => "macro".to_string(),
            _ => "other".to_string(),
        }
    }

    /// Build implementation items
    fn build_impl_items(&self, item_ids: &[Id]) -> Vec<ImplItem> {
        // Simplified implementation - in practice, you'd resolve the actual items
        item_ids
            .iter()
            .enumerate()
            .map(|(i, _id)| ImplItem {
                name: format!("item_{}", i),
                kind: "unknown".to_string(),
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
    fn test_type_to_string() {
        let index = TraitImplIndex::new();

        // Test primitive type
        let primitive = Type::Primitive("u32".to_string());
        assert_eq!(index.type_to_string(&primitive), "u32");

        // Test generic type
        let generic = Type::Generic("T".to_string());
        assert_eq!(index.type_to_string(&generic), "T");

        // Test tuple type
        let tuple = Type::Tuple(vec![
            Type::Primitive("u32".to_string()),
            Type::Primitive("String".to_string()),
        ]);
        assert_eq!(index.type_to_string(&tuple), "(u32, String)");
    }

    #[test]
    fn test_get_item_kind() {
        let index = TraitImplIndex::new();

        let struct_item = ItemEnum::Struct(rustdoc_types::Struct {
            kind: rustdoc_types::StructKind::Unit,
            generics: rustdoc_types::Generics {
                params: Vec::new(),
                where_predicates: Vec::new(),
            },
            impls: Vec::new(),
        });

        assert_eq!(index.get_item_kind(&struct_item), "struct");
    }

    #[test]
    fn test_extract_generics() {
        let index = TraitImplIndex::new();

        let mut generics = rustdoc_types::Generics {
            params: Vec::new(),
            where_predicates: Vec::new(),
        };
        generics.params.push(rustdoc_types::GenericParamDef {
            name: "T".to_string(),
            kind: rustdoc_types::GenericParamDefKind::Type {
                bounds: Vec::new(),
                default: None,
                is_synthetic: false,
            },
        });

        let extracted = index.extract_generics(&generics);
        assert_eq!(extracted, vec!["T"]);
    }
}
