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
    pub fn from_rustdoc(krate: &RustdocCrate) -> Result<Self> {
        let mut index = Self::new();
        index.build_from_rustdoc(krate)?;
        Ok(index)
    }

    /// Build the index from rustdoc crate data
    fn build_from_rustdoc(&mut self, krate: &RustdocCrate) -> Result<()> {
        info!(
            "Building trait implementation index for crate: {}",
            krate.root.0
        );

        // First pass: collect all traits and types
        for (id, item) in &krate.index {
            match &item.inner {
                ItemEnum::Trait(trait_item) => {
                    let trait_data = TraitData {
                        id: id.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        path: self.build_item_path(id, krate),
                        generics: self.extract_generics(&trait_item.generics),
                        items: trait_item.items.clone(),
                    };
                    self.traits.insert(id.clone(), trait_data);
                }
                ItemEnum::Struct(_)
                | ItemEnum::Enum(_)
                | ItemEnum::Union(_)
                | ItemEnum::TypeAlias(_) => {
                    let type_data = TypeData {
                        id: id.clone(),
                        name: item.name.clone().unwrap_or_default(),
                        path: self.build_item_path(id, krate),
                        kind: self.get_item_kind_string(&item.inner),
                    };
                    self.types.insert(id.clone(), type_data);
                }
                _ => {}
            }
        }

        // Second pass: process implementations
        for (id, item) in &krate.index {
            if let ItemEnum::Impl(impl_item) = &item.inner {
                // Pass krate to process_implementation for context if needed later for signature building
                self.process_implementation(id, impl_item, item, krate)?;
            }
        }

        // Third pass: build lookup maps
        self.build_lookup_maps(krate)?;

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
        _krate: &RustdocCrate, // krate available if needed for context
    ) -> Result<()> {
        let impl_data = ImplData {
            id: impl_id.clone(),
            trait_id: impl_item.trait_.as_ref().map(|path| path.id.clone()),
            for_type: impl_item.for_.clone(),
            generics: self.extract_generics(&impl_item.generics),
            where_clause: self.extract_where_clause(&impl_item.generics),
            items: impl_item.items.clone(),
            is_blanket: impl_item.blanket_impl.is_some(),
            is_synthetic: impl_item.synthetic,
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
    fn build_lookup_maps(&mut self, krate: &RustdocCrate) -> Result<()> {
        // Added krate
        debug!("Building trait implementation lookup maps");

        for impl_data in self.implementations.values() {
            // Map trait -> implementations
            if let Some(trait_id) = &impl_data.trait_id {
                if let Some(trait_data) = self.traits.get(trait_id) {
                    let trait_impl = TraitImpl {
                        for_type: self.type_to_string(&impl_data.for_type, krate),
                        trait_path: trait_data.path.clone(),
                        generics: impl_data.generics.clone(),
                        where_clause: impl_data.where_clause.clone(),
                        source_span: impl_data.source_location.clone(),
                        impl_id: format!("{:?}", impl_data.id), // Consider a more stable ID
                        items: self.build_impl_items(&impl_data.items, krate),
                        is_blanket: impl_data.is_blanket,
                        is_synthetic: impl_data.is_synthetic,
                        // TODO: Add negative impls if identifiable
                        // TODO: Add auto trait status if identifiable
                    };

                    self.trait_to_impls
                        .entry(trait_data.path.clone())
                        .or_default()
                        .push(trait_impl);
                }
            }

            // Map type -> trait implementations
            let for_type_str = self.type_to_string(&impl_data.for_type, krate);
            if let Some(trait_id) = &impl_data.trait_id {
                // This is for trait impls
                if let Some(trait_data) = self.traits.get(trait_id) {
                    let type_impl = TypeImpl {
                        trait_path: trait_data.path.clone(),
                        generics: impl_data.generics.clone(),
                        where_clause: impl_data.where_clause.clone(),
                        source_span: impl_data.source_location.clone(),
                        impl_id: format!("{:?}", impl_data.id),
                        items: self.build_impl_items(&impl_data.items, krate),
                        is_blanket: impl_data.is_blanket,
                        is_synthetic: impl_data.is_synthetic,
                        // TODO: Add negative impls
                        // TODO: Add auto trait status
                    };

                    self.type_to_impls
                        .entry(for_type_str.clone()) // Use clone if for_type_str is used again
                        .or_default()
                        .push(type_impl);
                }
            } else { // This is for inherent impls (impl MyType { ... })
                 // The current structure (TypeImpl, TraitImpl) is focused on trait implementations.
                 // If inherent impl items also need to be listed under a type,
                 // a different structure or an extension to TypeImpl might be needed.
                 // For now, focusing on trait impls as per TraitImplIndex's primary role.
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

    /// Build item path from ID using the krate's paths map.
    fn build_item_path(&self, id: &Id, krate: &RustdocCrate) -> String {
        if let Some(path_info) = krate.paths.get(id) {
            return path_info.path.join("::");
        }
        // Fallback for items not in paths map (e.g. some impls)
        // or if root is the item.
        if Some(id) == krate.root.as_ref() {
            return krate
                .index
                .get(&krate.root.as_ref().unwrap())
                .and_then(|i| i.name.clone())
                .unwrap_or_default();
        }
        // Fallback to item name if path not found by ID.
        krate
            .index
            .get(id)
            .and_then(|item| item.name.clone())
            .unwrap_or_else(|| format!("id:{}", id.0))
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

    /// Convert type to string representation, using krate for path resolution.
    fn type_to_string(&self, ty: &Type, krate: &RustdocCrate) -> String {
        match ty {
            Type::ResolvedPath(path) => {
                // Try to resolve full path if ID is present
                if let Some(path_info) = krate.paths.get(&path.id) {
                    path_info.path.join("::")
                } else {
                    path.name.clone() // Fallback to name
                }
            }
            Type::DynTrait(dyn_trait) => {
                let trait_names: Vec<String> = dyn_trait
                    .traits
                    .iter()
                    .map(|t| self.type_to_string(&Type::ResolvedPath(t.trait_.clone()), krate))
                    .collect();
                format!("dyn {}", trait_names.join(" + "))
            }
            Type::Generic(name) => name.clone(),
            Type::Primitive(name) => name.clone(),
            Type::FunctionPointer(fp) => {
                let inputs = fp
                    .decl
                    .inputs
                    .iter()
                    .map(|(_, t)| self.type_to_string(t, krate))
                    .collect::<Vec<_>>()
                    .join(", ");
                let output = fp
                    .decl
                    .output
                    .as_ref()
                    .map_or_else(|| "()".to_string(), |t| self.type_to_string(t, krate));
                // Consider safety (unsafe) and abi
                format!("fn({}) -> {}", inputs, output)
            }
            Type::Tuple(types) => {
                let type_strs: Vec<String> = types
                    .iter()
                    .map(|t| self.type_to_string(t, krate))
                    .collect();
                format!("({})", type_strs.join(", "))
            }
            Type::Slice(inner) => format!("[{}]", self.type_to_string(inner, krate)),
            Type::Array { type_, len } => {
                format!("[{}; {}]", self.type_to_string(type_, krate), len)
            }
            Type::ImplTrait(bounds) => {
                let bound_strs: Vec<String> = bounds
                    .iter()
                    .map(|b| self.generic_bound_to_string(b, krate))
                    .collect();
                format!("impl {}", bound_strs.join(" + "))
            }
            Type::Infer => "_".to_string(),
            Type::RawPointer { mutable, type_ } => {
                format!(
                    "*{} {}",
                    if *mutable { "mut" } else { "const" },
                    self.type_to_string(type_, krate)
                )
            }
            Type::BorrowedRef {
                lifetime,
                mutable,
                type_,
            } => {
                format!(
                    "&{}{}{}",
                    lifetime
                        .as_ref()
                        .map_or("".to_string(), |l| l.to_string() + " "),
                    if *mutable { "mut " } else { "" },
                    self.type_to_string(type_, krate)
                )
            }
            Type::QualifiedPath {
                name,
                args, // TODO: Handle GenericArgs properly
                self_type,
                trait_,
            } => {
                let self_type_str = self.type_to_string(self_type, krate);
                let trait_str = trait_.as_ref().map_or("Self".to_string(), |p| {
                    self.type_to_string(&Type::ResolvedPath(p.clone()), krate)
                });
                // Args might need more detailed formatting
                let args_str = if let Some(bindings) = args.as_ref().map(|a| &a.bindings) {
                    if !bindings.is_empty() {
                        format!(
                            "<{}>",
                            bindings
                                .iter()
                                .map(|b| self.generic_binding_to_string(b, krate))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    } else {
                        "".to_string()
                    }
                } else {
                    "".to_string()
                };

                format!("<{} as {}>::{}{}", self_type_str, trait_str, name, args_str)
            }
            Type::Pat { .. } => "pattern".to_string(), // Should not typically appear in resolved types
        }
    }

    /// Helper for converting GenericBound to string
    fn generic_bound_to_string(
        &self,
        bound: &rustdoc_types::GenericBound,
        krate: &RustdocCrate,
    ) -> String {
        match bound {
            rustdoc_types::GenericBound::TraitBound {
                trait_,
                generic_params,
                modifier,
            } => {
                let mut s = String::new();
                if !generic_params.is_empty() {
                    // e.g. for<'a>
                    s.push_str("for<");
                    s.push_str(
                        &generic_params
                            .iter()
                            .map(|gp| gp.name.clone())
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    s.push_str("> ");
                }
                s.push_str(match modifier {
                    rustdoc_types::TraitBoundModifier::None => "",
                    rustdoc_types::TraitBoundModifier::Maybe => "?",
                    rustdoc_types::TraitBoundModifier::MaybeConst => "~const ", // nightly feature
                });
                s.push_str(&self.type_to_string(&Type::ResolvedPath(trait_.clone()), krate));
                s
            }
            rustdoc_types::GenericBound::Lifetime(lt) => lt.clone(),
        }
    }

    /// Helper for converting GenericBinding to string
    fn generic_binding_to_string(
        &self,
        binding: &rustdoc_types::GenericBinding,
        krate: &RustdocCrate,
    ) -> String {
        match binding {
            rustdoc_types::GenericBinding::TypeBinding {
                name,
                args,
                binding_type,
            } => {
                let mut s = name.clone();
                // TODO: args if present (GenericArgs)
                match binding_type {
                    rustdoc_types::TypeBindingKind::Equality(term) => {
                        s.push_str(" = ");
                        s.push_str(&self.type_to_string(term, krate));
                    }
                    rustdoc_types::TypeBindingKind::Constraint(bounds) => {
                        if !bounds.is_empty() {
                            s.push_str(": ");
                            s.push_str(
                                &bounds
                                    .iter()
                                    .map(|b| self.generic_bound_to_string(b, krate))
                                    .collect::<Vec<_>>()
                                    .join(" + "),
                            );
                        }
                    }
                }
                s
            }
            rustdoc_types::GenericBinding::Constraint { name, bounds } => {
                let mut s = name.clone();
                if !bounds.is_empty() {
                    s.push_str(": ");
                    s.push_str(
                        &bounds
                            .iter()
                            .map(|b| self.generic_bound_to_string(b, krate))
                            .collect::<Vec<_>>()
                            .join(" + "),
                    );
                }
                s
            }
            rustdoc_types::GenericBinding::Lifetime { name, lifetime } => {
                format!("{}: {}", name, lifetime)
            }
        }
    }

    /// Get item kind as string
    fn get_item_kind_string(&self, item_enum: &ItemEnum) -> String {
        SymbolKind::from_item_enum(item_enum).as_str().to_string()
    }

    /// Build detailed ImplItem from item IDs by looking them up in the crate.
    fn build_impl_items(&self, item_ids: &[Id], krate: &RustdocCrate) -> Vec<ImplItem> {
        item_ids
            .iter()
            .filter_map(|id| krate.index.get(id))
            .map(|item| {
                let kind_str = self.get_item_kind_string(&item.inner);
                // TODO: More sophisticated signature extraction.
                let signature = match &item.inner {
                    ItemEnum::Function(f) => {
                        Some(self.format_fn_signature(item.name.as_deref().unwrap_or(""), f, krate))
                    }
                    ItemEnum::Constant(c) => Some(format!(
                        "const {}: {}",
                        item.name.as_deref().unwrap_or("_"),
                        self.type_to_string(&c.type_, krate)
                    )),
                    ItemEnum::TypeAlias(ta) => Some(format!(
                        "type {} = {};",
                        item.name.as_deref().unwrap_or("_"),
                        self.type_to_string(&ta.type_, krate)
                    )),
                    // Add other kinds as needed
                    _ => item.name.clone(),
                };

                ImplItem {
                    name: item
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("id:{}", item.id.0)),
                    kind: kind_str,
                    signature,
                    doc: item.docs.clone(),
                    source_location: item.span.as_ref().map(|s| SourceLocation {
                        file: s.filename.to_string_lossy().into_owned(),
                        line: s.begin.0 as u32,
                        column: s.begin.1 as u32,
                        end_line: Some(s.end.0 as u32),
                        end_column: Some(s.end.1 as u32),
                    }),
                }
            })
            .collect()
    }

    fn format_fn_signature(
        &self,
        name: &str,
        func: &rustdoc_types::Function,
        krate: &RustdocCrate,
    ) -> String {
        let mut sig = String::new();
        // TODO: Handle async, const, unsafe from func.header
        sig.push_str("fn ");
        sig.push_str(name);
        // TODO: Handle func.generics properly
        sig.push('(');
        let inputs = func
            .decl
            .inputs
            .iter()
            .map(|(param_name, param_type)| {
                format!("{}: {}", param_name, self.type_to_string(param_type, krate))
            })
            .collect::<Vec<_>>()
            .join(", ");
        sig.push_str(&inputs);
        sig.push(')');
        if let Some(output_type) = &func.decl.output {
            sig.push_str(" -> ");
            sig.push_str(&self.type_to_string(output_type, krate));
        }
        // TODO: Where clause from func.generics.where_predicates
        sig
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
                synthetic: false,
            },
        });

        let extracted = index.extract_generics(&generics);
        assert_eq!(extracted, vec!["T"]);
    }
}
