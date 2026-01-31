//! Pure Rust Python code analysis using tree-sitter.
//!
//! This module provides method-level extraction, class introspection,
//! and library-wide search capabilities without requiring Python runtime.

use anyhow::{Context, Result};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};
use walkdir::WalkDir;

/// Information about a single method in a class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    pub name: String,
    pub signature: String,
    pub docstring: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub is_staticmethod: bool,
    pub is_classmethod: bool,
    pub is_property: bool,
    pub is_async: bool,
}

/// Complete information about a Python class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInfo {
    pub name: String,
    pub docstring: Option<String>,
    pub methods: Vec<MethodInfo>,
    pub base_classes: Vec<String>,
    pub line_start: usize,
    pub line_end: usize,
}

/// Search modes for library-wide search.
#[derive(Debug, Clone, Copy)]
pub enum SearchMode {
    Name,      // Search function/class names only
    Signature, // Search function signatures
    Docstring, // Search docstrings
    FullText,  // Full-text code search
}

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file_path: PathBuf,
    pub item_type: String, // "class", "function", "method"
    pub item_name: String,
    pub class_name: Option<String>, // If it's a method
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub line_number: usize,
}

/// Extract all methods from a class using tree-sitter.
pub fn extract_class_methods(
    source_code: &str,
    class_name: &str,
    include_private: bool,
) -> Result<Vec<MethodInfo>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .context("Failed to set Python language")?;

    let tree = parser
        .parse(source_code, None)
        .context("Failed to parse Python source")?;

    let class_node = find_class_node(tree.root_node(), class_name, source_code.as_bytes())
        .context(format!("Class '{}' not found", class_name))?;

    let mut methods = Vec::new();

    // Get the class body
    if let Some(body) = class_node.child_by_field_name("body") {
        let mut cursor = body.walk();

        for child in body.children(&mut cursor) {
            match child.kind() {
                "function_definition" => {
                    if let Some(method_info) =
                        extract_method_info(child, source_code.as_bytes(), include_private)
                    {
                        methods.push(method_info);
                    }
                }
                "decorated_definition" => {
                    // Handle @staticmethod, @classmethod, @property
                    if let Some(method_info) =
                        extract_decorated_method(child, source_code.as_bytes(), include_private)
                    {
                        methods.push(method_info);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(methods)
}

/// Extract complete class information including all methods.
pub fn extract_class_info(source_code: &str, class_name: &str) -> Result<ClassInfo> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .context("Failed to set Python language")?;

    let tree = parser
        .parse(source_code, None)
        .context("Failed to parse Python source")?;

    let class_node = find_class_node(tree.root_node(), class_name, source_code.as_bytes())
        .context(format!("Class '{}' not found", class_name))?;

    let docstring = extract_first_docstring(class_node, source_code.as_bytes());

    // Extract base classes
    let mut base_classes = Vec::new();
    if let Some(superclasses) = class_node.child_by_field_name("superclasses") {
        let mut cursor = superclasses.walk();
        for child in superclasses.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "attribute" {
                if let Ok(base_name) = child.utf8_text(source_code.as_bytes()) {
                    base_classes.push(base_name.to_string());
                }
            }
        }
    }

    let methods = extract_class_methods(source_code, class_name, true)?;

    Ok(ClassInfo {
        name: class_name.to_string(),
        docstring,
        methods,
        base_classes,
        line_start: class_node.start_position().row + 1,
        line_end: class_node.end_position().row + 1,
    })
}

/// Extract a specific method from a class.
pub fn extract_specific_method(
    source_code: &str,
    class_name: &str,
    method_name: &str,
) -> Result<MethodInfo> {
    let methods = extract_class_methods(source_code, class_name, true)?;

    methods
        .into_iter()
        .find(|m| m.name == method_name)
        .context(format!(
            "Method '{}' not found in class '{}'",
            method_name, class_name
        ))
}

/// Search across an entire Python package.
pub fn search_package(
    package_path: &Path,
    pattern: &str,
    search_mode: SearchMode,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let regex = Regex::new(pattern).context("Invalid regex pattern")?;

    // Collect all .py files
    let py_files: Vec<_> = WalkDir::new(package_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("py"))
        .collect();

    // Process files in parallel
    let file_results: Vec<Vec<SearchResult>> = py_files
        .par_iter()
        .filter_map(|entry| search_file(entry.path(), &regex, &search_mode).ok())
        .collect();

    // Flatten and limit results
    let mut results = Vec::new();
    for file_result in file_results {
        for result in file_result {
            if results.len() >= limit {
                return Ok(results);
            }
            results.push(result);
        }
    }

    Ok(results)
}

// ===== Helper Functions =====

fn find_class_node<'a>(node: Node<'a>, class_name: &str, source: &'a [u8]) -> Option<Node<'a>> {
    if node.kind() == "class_definition" {
        if let Some(name_node) = node.child_by_field_name("name") {
            if name_node.utf8_text(source).ok() == Some(class_name) {
                return Some(node);
            }
        }
    }

    for child in node.children(&mut node.walk()) {
        if let Some(found) = find_class_node(child, class_name, source) {
            return Some(found);
        }
    }

    None
}

fn extract_method_info(
    method_node: Node,
    source: &[u8],
    include_private: bool,
) -> Option<MethodInfo> {
    let name_node = method_node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?.to_string();

    // Skip private methods if not requested (but keep dunder methods)
    if !include_private && name.starts_with('_') && !name.starts_with("__") {
        return None;
    }

    // Build signature
    let signature = build_signature(method_node, source)?;

    // Extract docstring
    let docstring = extract_first_docstring(method_node, source);

    // Check if async
    let is_async = method_node
        .parent()
        .and_then(|p| {
            if p.kind() == "decorated_definition" {
                p.children(&mut p.walk()).find(|c| c.kind() == "async")
            } else {
                None
            }
        })
        .is_some();

    Some(MethodInfo {
        name,
        signature,
        docstring,
        line_start: method_node.start_position().row + 1,
        line_end: method_node.end_position().row + 1,
        is_staticmethod: false,
        is_classmethod: false,
        is_property: false,
        is_async,
    })
}

fn extract_decorated_method(
    decorated_node: Node,
    source: &[u8],
    include_private: bool,
) -> Option<MethodInfo> {
    // Check decorators
    let mut is_staticmethod = false;
    let mut is_classmethod = false;
    let mut is_property = false;
    let mut is_async = false;

    let mut cursor = decorated_node.walk();
    let mut func_node = None;

    for child in decorated_node.children(&mut cursor) {
        match child.kind() {
            "decorator" => {
                if let Ok(decorator_text) = child.utf8_text(source) {
                    if decorator_text.contains("staticmethod") {
                        is_staticmethod = true;
                    } else if decorator_text.contains("classmethod") {
                        is_classmethod = true;
                    } else if decorator_text.contains("property") {
                        is_property = true;
                    }
                }
            }
            "async" => {
                is_async = true;
            }
            "function_definition" => {
                func_node = Some(child);
            }
            _ => {}
        }
    }

    if let Some(func_node) = func_node {
        let mut method_info = extract_method_info(func_node, source, include_private)?;
        method_info.is_staticmethod = is_staticmethod;
        method_info.is_classmethod = is_classmethod;
        method_info.is_property = is_property;
        method_info.is_async = is_async;
        return Some(method_info);
    }

    None
}

fn extract_first_docstring(node: Node, source: &[u8]) -> Option<String> {
    let body = node.child_by_field_name("body")?;
    let mut cursor = body.walk();

    for child in body.children(&mut cursor) {
        if child.kind() == "expression_statement" {
            for expr_child in child.children(&mut child.walk()) {
                if expr_child.kind() == "string" {
                    if let Ok(text) = expr_child.utf8_text(source) {
                        // Clean up docstring (remove quotes)
                        let cleaned = text
                            .trim_start_matches("\"\"\"")
                            .trim_start_matches("'''")
                            .trim_start_matches('"')
                            .trim_start_matches('\'')
                            .trim_end_matches("\"\"\"")
                            .trim_end_matches("'''")
                            .trim_end_matches('"')
                            .trim_end_matches('\'')
                            .trim();
                        return Some(cleaned.to_string());
                    }
                }
            }
        }
    }

    None
}

fn build_signature(func_node: Node, source: &[u8]) -> Option<String> {
    let name = func_node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?;
    let params = func_node
        .child_by_field_name("parameters")?
        .utf8_text(source)
        .ok()?;

    let return_type = func_node
        .child_by_field_name("return_type")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| format!(" -> {}", s.trim_start_matches("->")))
        .unwrap_or_default();

    Some(format!("{}{}{}", name, params, return_type))
}

fn search_file(
    file_path: &Path,
    pattern: &Regex,
    search_mode: &SearchMode,
) -> Result<Vec<SearchResult>> {
    let source_code = std::fs::read_to_string(file_path)?;
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .context("Failed to set Python language")?;

    let tree = parser
        .parse(&source_code, None)
        .context("Failed to parse Python source")?;

    let mut results = Vec::new();
    search_node(
        tree.root_node(),
        source_code.as_bytes(),
        pattern,
        search_mode,
        file_path,
        None,
        &mut results,
    );

    Ok(results)
}

fn search_node(
    node: Node,
    source: &[u8],
    pattern: &Regex,
    search_mode: &SearchMode,
    file_path: &Path,
    current_class: Option<&str>,
    results: &mut Vec<SearchResult>,
) {
    match node.kind() {
        "class_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Ok(class_name) = name_node.utf8_text(source) {
                    // Check if class name matches
                    if matches!(search_mode, SearchMode::Name | SearchMode::FullText)
                        && pattern.is_match(class_name)
                    {
                        let docstring = extract_first_docstring(node, source);
                        results.push(SearchResult {
                            file_path: file_path.to_path_buf(),
                            item_type: "class".to_string(),
                            item_name: class_name.to_string(),
                            class_name: None,
                            signature: None,
                            docstring,
                            line_number: node.start_position().row + 1,
                        });
                    }

                    // Search inside class
                    for child in node.children(&mut node.walk()) {
                        search_node(
                            child,
                            source,
                            pattern,
                            search_mode,
                            file_path,
                            Some(class_name),
                            results,
                        );
                    }
                }
            }
        }

        "function_definition" | "decorated_definition" => {
            let func_node = if node.kind() == "decorated_definition" {
                node.children(&mut node.walk())
                    .find(|c| c.kind() == "function_definition")
            } else {
                Some(node)
            };

            if let Some(func_node) = func_node {
                if let Some(name_node) = func_node.child_by_field_name("name") {
                    if let Ok(func_name) = name_node.utf8_text(source) {
                        let should_include = match search_mode {
                            SearchMode::Name => pattern.is_match(func_name),
                            SearchMode::Signature => {
                                if let Some(sig) = build_signature(func_node, source) {
                                    pattern.is_match(&sig)
                                } else {
                                    false
                                }
                            }
                            SearchMode::Docstring => {
                                if let Some(doc) = extract_first_docstring(func_node, source) {
                                    pattern.is_match(&doc)
                                } else {
                                    false
                                }
                            }
                            SearchMode::FullText => {
                                if let Ok(full_text) = func_node.utf8_text(source) {
                                    pattern.is_match(full_text)
                                } else {
                                    false
                                }
                            }
                        };

                        if should_include {
                            let signature = build_signature(func_node, source);
                            let docstring = extract_first_docstring(func_node, source);

                            results.push(SearchResult {
                                file_path: file_path.to_path_buf(),
                                item_type: if current_class.is_some() {
                                    "method"
                                } else {
                                    "function"
                                }
                                .to_string(),
                                item_name: func_name.to_string(),
                                class_name: current_class.map(|s| s.to_string()),
                                signature,
                                docstring,
                                line_number: func_node.start_position().row + 1,
                            });
                        }
                    }
                }
            }
        }

        _ => {
            // Continue searching children (but don't recurse into classes - handled above)
            if node.kind() != "class_definition" {
                for child in node.children(&mut node.walk()) {
                    search_node(
                        child,
                        source,
                        pattern,
                        search_mode,
                        file_path,
                        current_class,
                        results,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_class_methods() {
        let source = r#"
class MyClass:
    """A test class."""

    def __init__(self):
        pass

    def public_method(self, x: int) -> str:
        """A public method."""
        return str(x)

    def _private_method(self):
        """A private method."""
        pass

    @staticmethod
    def static_method():
        """A static method."""
        pass

    @classmethod
    def class_method(cls):
        """A class method."""
        pass

    @property
    def my_property(self):
        """A property."""
        return 42
"#;

        let methods = extract_class_methods(source, "MyClass", false).unwrap();

        // Should not include _private_method (include_private = false)
        assert_eq!(methods.len(), 5); // __init__, public_method, static_method, class_method, my_property

        let public_method = methods.iter().find(|m| m.name == "public_method").unwrap();
        assert!(public_method.signature.contains("int"));
        assert!(public_method.signature.contains("-> str"));
        assert_eq!(public_method.docstring.as_deref(), Some("A public method."));

        let static_method = methods.iter().find(|m| m.name == "static_method").unwrap();
        assert!(static_method.is_staticmethod);

        let class_method = methods.iter().find(|m| m.name == "class_method").unwrap();
        assert!(class_method.is_classmethod);

        let property = methods.iter().find(|m| m.name == "my_property").unwrap();
        assert!(property.is_property);
    }

    #[test]
    fn test_extract_class_info() {
        let source = r#"
class Parent:
    pass

class Child(Parent):
    """A child class."""

    def method(self):
        pass
"#;

        let info = extract_class_info(source, "Child").unwrap();
        assert_eq!(info.name, "Child");
        assert_eq!(info.docstring.as_deref(), Some("A child class."));
        assert_eq!(info.base_classes, vec!["Parent"]);
        assert_eq!(info.methods.len(), 1);
    }

    #[test]
    fn test_search_by_name() {
        let source = r#"
def find_user(user_id):
    pass

def find_product(product_id):
    pass

def delete_user(user_id):
    pass
"#;

        let temp_file = std::env::temp_dir().join("test_search.py");
        std::fs::write(&temp_file, source).unwrap();

        let results =
            search_package(temp_file.parent().unwrap(), "find_.*", SearchMode::Name, 10).unwrap();

        assert!(results.iter().any(|r| r.item_name == "find_user"));
        assert!(results.iter().any(|r| r.item_name == "find_product"));
        assert!(!results.iter().any(|r| r.item_name == "delete_user"));

        std::fs::remove_file(&temp_file).ok();
    }
}
