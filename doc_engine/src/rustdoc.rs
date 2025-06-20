//! Rustdoc module for building and parsing rustdoc JSON output

use anyhow::Result;
use rustdoc_types::{Crate as RustdocCrate, FORMAT_VERSION};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info};

use crate::types::*;

/// Builder for generating rustdoc JSON
pub struct RustdocBuilder {
    crate_dir: PathBuf,
    #[allow(dead_code)]
    config: RustdocConfig,
}

impl RustdocBuilder {
    /// Create a new rustdoc builder
    pub fn new(crate_dir: impl AsRef<Path>) -> Self {
        Self {
            crate_dir: crate_dir.as_ref().to_path_buf(),
            config: RustdocConfig::default(),
        }
    }

    /// Create a new rustdoc builder with custom config
    pub fn with_config(crate_dir: impl AsRef<Path>, config: RustdocConfig) -> Self {
        Self {
            crate_dir: crate_dir.as_ref().to_path_buf(),
            config,
        }
    }

    /// Build rustdoc JSON for the crate (mock implementation)
    pub async fn build_json(&self) -> Result<RustdocCrate> {
        info!(
            "Building mock rustdoc JSON for crate at: {:?}",
            self.crate_dir
        );

        // Find the actual crate directory (may be nested in extracted tarball)
        let actual_crate_dir = self.find_crate_root().await?;
        debug!("Found crate root at: {:?}", actual_crate_dir);

        // Check if we have a Cargo.toml
        let cargo_toml = actual_crate_dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return Err(anyhow::anyhow!("No Cargo.toml found in crate directory"));
        }

        // Parse Cargo.toml to get crate name
        let cargo_toml_content = fs::read_to_string(&cargo_toml).await?;
        let crate_name = self.extract_crate_name(&cargo_toml_content)?;

        // Create a mock rustdoc crate structure
        let rustdoc_crate = self.create_mock_rustdoc_crate(&crate_name)?;

        info!(
            "Successfully built mock rustdoc JSON with {} items",
            rustdoc_crate.index.len()
        );

        Ok(rustdoc_crate)
    }

    /// Create a mock rustdoc crate for demonstration purposes
    fn create_mock_rustdoc_crate(&self, crate_name: &str) -> Result<RustdocCrate> {
        use rustdoc_types::*;
        use std::collections::HashMap;

        let mut index = HashMap::new();
        let root_id = Id("0".to_string());

        // Create root module
        let root_module = Item {
            id: root_id.clone(),
            crate_id: 0,
            name: Some(crate_name.to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: Some(format!("Mock documentation for crate {}", crate_name)),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Module(Module {
                is_crate: true,
                items: vec![
                    Id("1".to_string()),
                    Id("2".to_string()),
                    Id("3".to_string()),
                    Id("4".to_string()),
                ],
                is_stripped: false,
            }),
        };

        // Create a mock struct
        let struct_item = Item {
            id: Id("1".to_string()),
            crate_id: 0,
            name: Some(format!("{}Struct", capitalize_first_letter(crate_name))),
            span: None,
            visibility: Visibility::Public,
            docs: Some("A mock struct for demonstration".to_string()),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Struct(Struct {
                kind: StructKind::Plain {
                    fields: vec![Id("5".to_string())],
                    fields_stripped: false,
                },
                generics: Generics {
                    params: vec![],
                    where_predicates: vec![],
                },
                impls: vec![Id("6".to_string())],
            }),
        };

        // Create a mock trait
        let trait_item = Item {
            id: Id("2".to_string()),
            crate_id: 0,
            name: Some("MockTrait".to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: Some("A mock trait for demonstration".to_string()),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Trait(Trait {
                is_auto: false,
                is_unsafe: false,
                items: vec![Id("7".to_string())],
                generics: Generics {
                    params: vec![],
                    where_predicates: vec![],
                },
                bounds: vec![],
                implementations: vec![Id("6".to_string())],
                is_object_safe: true,
            }),
        };

        // Create a mock function
        let function_item = Item {
            id: Id("3".to_string()),
            crate_id: 0,
            name: Some("mock_function".to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: Some("A mock function for demonstration".to_string()),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Function(Function {
                decl: FnDecl {
                    inputs: vec![],
                    output: None,
                    c_variadic: false,
                },
                generics: Generics {
                    params: vec![],
                    where_predicates: vec![],
                },
                header: Header {
                    const_: false,
                    unsafe_: false,
                    async_: false,
                    abi: Abi::Rust,
                },
                has_body: true,
            }),
        };

        // Create a mock constant
        let constant_item = Item {
            id: Id("4".to_string()),
            crate_id: 0,
            name: Some("MOCK_CONSTANT".to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: Some("A mock constant for demonstration".to_string()),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Constant {
                type_: Type::Primitive("u32".to_string()),
                const_: Constant {
                    expr: "42".to_string(),
                    value: Some("42".to_string()),
                    is_literal: true,
                },
            },
        };

        // Create a mock struct field
        let field_item = Item {
            id: Id("5".to_string()),
            crate_id: 0,
            name: Some("value".to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: Some("A mock field".to_string()),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::StructField(Type::Primitive("u32".to_string())),
        };

        // Create a mock implementation
        let impl_item = Item {
            id: Id("6".to_string()),
            crate_id: 0,
            name: None,
            span: None,
            visibility: Visibility::Default,
            docs: None,
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Impl(Impl {
                is_unsafe: false,
                generics: Generics {
                    params: vec![],
                    where_predicates: vec![],
                },
                provided_trait_methods: vec![],
                trait_: Some(Path {
                    name: "MockTrait".to_string(),
                    id: Id("2".to_string()),
                    args: None,
                }),
                for_: Type::ResolvedPath(Path {
                    name: format!("{}Struct", capitalize_first_letter(crate_name)),
                    id: Id("1".to_string()),
                    args: None,
                }),
                items: vec![Id("8".to_string())],
                negative: false,
                synthetic: false,
                blanket_impl: None,
            }),
        };

        // Create a mock trait method
        let method_item = Item {
            id: Id("7".to_string()),
            crate_id: 0,
            name: Some("mock_method".to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: Some("A mock trait method".to_string()),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Function(Function {
                decl: FnDecl {
                    inputs: vec![(
                        "self".to_string(),
                        Type::BorrowedRef {
                            lifetime: None,
                            mutable: false,
                            type_: Box::new(Type::Generic("Self".to_string())),
                        },
                    )],
                    output: Some(Type::Primitive("String".to_string())),
                    c_variadic: false,
                },
                generics: Generics {
                    params: vec![],
                    where_predicates: vec![],
                },
                header: Header {
                    const_: false,
                    unsafe_: false,
                    async_: false,
                    abi: Abi::Rust,
                },
                has_body: false,
            }),
        };

        // Create a mock implementation method
        let impl_method_item = Item {
            id: Id("8".to_string()),
            crate_id: 0,
            name: Some("mock_method".to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: Some("Implementation of mock_method".to_string()),
            links: HashMap::new(),
            attrs: vec![],
            deprecation: None,
            inner: ItemEnum::Function(Function {
                decl: FnDecl {
                    inputs: vec![(
                        "self".to_string(),
                        Type::BorrowedRef {
                            lifetime: None,
                            mutable: false,
                            type_: Box::new(Type::ResolvedPath(Path {
                                name: format!("{}Struct", capitalize_first_letter(crate_name)),
                                id: Id("1".to_string()),
                                args: None,
                            })),
                        },
                    )],
                    output: Some(Type::Primitive("String".to_string())),
                    c_variadic: false,
                },
                generics: Generics {
                    params: vec![],
                    where_predicates: vec![],
                },
                header: Header {
                    const_: false,
                    unsafe_: false,
                    async_: false,
                    abi: Abi::Rust,
                },
                has_body: true,
            }),
        };

        // Add all items to index
        index.insert(root_id.clone(), root_module);
        index.insert(Id("1".to_string()), struct_item);
        index.insert(Id("2".to_string()), trait_item);
        index.insert(Id("3".to_string()), function_item);
        index.insert(Id("4".to_string()), constant_item);
        index.insert(Id("5".to_string()), field_item);
        index.insert(Id("6".to_string()), impl_item);
        index.insert(Id("7".to_string()), method_item);
        index.insert(Id("8".to_string()), impl_method_item);

        Ok(RustdocCrate {
            root: root_id,
            crate_version: Some("1.0.0".to_string()),
            includes_private: false,
            index,
            paths: HashMap::new(),
            external_crates: HashMap::new(),
            format_version: FORMAT_VERSION,
        })
    }

    /// Find the actual crate root directory
    async fn find_crate_root(&self) -> Result<PathBuf> {
        // Check if the current directory has Cargo.toml
        let cargo_toml = self.crate_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            return Ok(self.crate_dir.clone());
        }

        // Look for Cargo.toml in subdirectories (common with extracted tarballs)
        let mut entries = fs::read_dir(&self.crate_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let cargo_toml = path.join("Cargo.toml");
                if cargo_toml.exists() {
                    return Ok(path);
                }
            }
        }

        Err(anyhow::anyhow!(
            "Could not find Cargo.toml in crate directory"
        ))
    }

    /// Extract crate name from Cargo.toml content
    fn extract_crate_name(&self, cargo_toml: &str) -> Result<String> {
        // Simple TOML parsing - in production, you might want to use a proper TOML parser
        for line in cargo_toml.lines() {
            let line = line.trim();
            if line.starts_with("name") && line.contains('=') {
                let parts: Vec<&str> = line.split('=').collect();
                if parts.len() == 2 {
                    let name = parts[1].trim().trim_matches('"').trim_matches('\'');
                    if !name.is_empty() {
                        return Ok(name.to_string());
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Could not extract crate name from Cargo.toml"
        ))
    }

    /// Validate rustdoc JSON structure
    pub fn validate_rustdoc_json(rustdoc_crate: &RustdocCrate) -> Result<ValidationReport> {
        let mut report = ValidationReport::default();

        // Check format version
        if rustdoc_crate.format_version != FORMAT_VERSION {
            report.warnings.push(format!(
                "Format version mismatch: expected {}, got {}",
                FORMAT_VERSION, rustdoc_crate.format_version
            ));
        }

        // Count different item types
        for (_, item) in &rustdoc_crate.index {
            match &item.inner {
                rustdoc_types::ItemEnum::Module(_) => report.stats.modules += 1,
                rustdoc_types::ItemEnum::Struct(_) => report.stats.structs += 1,
                rustdoc_types::ItemEnum::Enum(_) => report.stats.enums += 1,
                rustdoc_types::ItemEnum::Trait(_) => report.stats.traits += 1,
                rustdoc_types::ItemEnum::Function(_) => report.stats.functions += 1,
                rustdoc_types::ItemEnum::Constant { .. } => report.stats.constants += 1,
                rustdoc_types::ItemEnum::TypeAlias(_) => report.stats.type_aliases += 1,
                rustdoc_types::ItemEnum::Macro(_) => report.stats.macros += 1,
                _ => {} // Skip other types
            }

            if item.docs.is_some() {
                report.stats.documented_items += 1;
            }
        }

        report.stats.total_items = rustdoc_crate.index.len();
        report.stats.undocumented_items = report.stats.total_items - report.stats.documented_items;

        // Calculate documentation coverage
        if report.stats.total_items > 0 {
            report.stats.documentation_coverage =
                (report.stats.documented_items as f32 / report.stats.total_items as f32) * 100.0;
        }

        // Check for potential issues
        if report.stats.documentation_coverage < 50.0 {
            report.warnings.push(format!(
                "Low documentation coverage: {:.1}%",
                report.stats.documentation_coverage
            ));
        }

        if rustdoc_crate.index.is_empty() {
            report.warnings.push("No items found in crate".to_string());
        }

        Ok(report)
    }
}

/// Validation report for rustdoc JSON
#[derive(Debug)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub stats: CrateStats,
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            stats: CrateStats {
                name: String::new(),
                version: String::new(),
                total_items: 0,
                public_items: 0,
                private_items: 0,
                modules: 0,
                structs: 0,
                enums: 0,
                traits: 0,
                functions: 0,
                constants: 0,
                type_aliases: 0,
                macros: 0,
                implementations: 0,
                documented_items: 0,
                undocumented_items: 0,
                documentation_coverage: 0.0,
            },
        }
    }
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// Helper function to capitalize the first letter of a string
fn capitalize_first_letter(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    async fn create_test_crate(dir: &Path, name: &str) -> Result<()> {
        let cargo_toml = format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"
"#,
            name
        );

        let lib_rs = r#"
//! Test crate documentation

/// A test function
pub fn hello() -> &'static str {
    "Hello, world!"
}

/// A test struct
pub struct TestStruct {
    pub field: u32,
}

impl TestStruct {
    /// Create a new TestStruct
    pub fn new(field: u32) -> Self {
        Self { field }
    }
}
"#;

        fs::write(dir.join("Cargo.toml"), cargo_toml).await?;
        fs::create_dir_all(dir.join("src")).await?;
        fs::write(dir.join("src").join("lib.rs"), lib_rs).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_rustdoc_builder_creation() {
        let temp_dir = tempdir().unwrap();
        let builder = RustdocBuilder::new(temp_dir.path());
        assert_eq!(builder.crate_dir, temp_dir.path());
    }

    #[tokio::test]
    async fn test_extract_crate_name() {
        let temp_dir = tempdir().unwrap();
        let builder = RustdocBuilder::new(temp_dir.path());

        let cargo_toml = r#"
[package]
name = "test-crate"
version = "0.1.0"
edition = "2021"
"#;

        let name = builder.extract_crate_name(cargo_toml).unwrap();
        assert_eq!(name, "test-crate");
    }

    #[tokio::test]
    async fn test_find_crate_root() {
        let temp_dir = tempdir().unwrap();

        // Create a Cargo.toml in the temp directory
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"",
        )
        .await
        .unwrap();

        let builder = RustdocBuilder::new(temp_dir.path());
        let root = builder.find_crate_root().await.unwrap();
        assert_eq!(root, temp_dir.path());
    }

    #[tokio::test]
    async fn test_find_crate_root_nested() {
        let temp_dir = tempdir().unwrap();
        let nested_dir = temp_dir.path().join("nested-crate-1.0.0");
        fs::create_dir_all(&nested_dir).await.unwrap();

        // Create a Cargo.toml in the nested directory
        fs::write(nested_dir.join("Cargo.toml"), "[package]\nname = \"test\"")
            .await
            .unwrap();

        let builder = RustdocBuilder::new(temp_dir.path());
        let root = builder.find_crate_root().await.unwrap();
        assert_eq!(root, nested_dir);
    }

    #[tokio::test]
    async fn test_build_json_mock() {
        let temp_dir = tempdir().unwrap();
        create_test_crate(temp_dir.path(), "test-crate")
            .await
            .unwrap();

        let builder = RustdocBuilder::new(temp_dir.path());
        let result = builder.build_json().await;

        assert!(result.is_ok());
        let rustdoc_crate = result.unwrap();
        assert!(!rustdoc_crate.index.is_empty());
        assert!(rustdoc_crate.index.len() >= 5); // Should have at least a few mock items
    }

    #[tokio::test]
    async fn test_validate_rustdoc_json() {
        let temp_dir = tempdir().unwrap();
        create_test_crate(temp_dir.path(), "test-crate")
            .await
            .unwrap();

        let builder = RustdocBuilder::new(temp_dir.path());
        let rustdoc_crate = builder.build_json().await.unwrap();

        let report = RustdocBuilder::validate_rustdoc_json(&rustdoc_crate).unwrap();
        assert!(report.is_valid());
        assert!(report.stats.total_items > 0);
        assert!(report.stats.documentation_coverage > 0.0);
    }

    #[test]
    fn test_capitalize_first_letter() {
        assert_eq!(capitalize_first_letter("hello"), "Hello");
        assert_eq!(capitalize_first_letter("world"), "World");
        assert_eq!(capitalize_first_letter(""), "");
        assert_eq!(capitalize_first_letter("a"), "A");
        assert_eq!(capitalize_first_letter("test-crate"), "Test-crate");
    }
}
