use super::traits::{ImplementationContext, LanguageProcessor};
use crate::doc_engine::finder;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use tree_sitter::Parser;

#[derive(Debug)]
pub struct PythonProcessor;

fn extract_item_by_name(source_code: &str, item_name: &str) -> Result<String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .context("Error loading tree-sitter grammar for Python")?;

    let tree = parser.parse(source_code, None).unwrap();

    fn find_node<'a>(
        node: tree_sitter::Node<'a>,
        item_name: &str,
        source: &'a [u8],
    ) -> Option<tree_sitter::Node<'a>> {
        // Check if this node has a name field that matches
        if let Some(name_node) = node.child_by_field_name("name") {
            if name_node.utf8_text(source).ok() == Some(item_name) {
                return Some(node);
            }
        }

        // For function definitions, class definitions, etc.
        match node.kind() {
            "function_definition" | "class_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source).ok() == Some(item_name) {
                        return Some(node);
                    }
                }
            }
            _ => {}
        }

        // Recursively search children
        for child in node.children(&mut node.walk()) {
            if let Some(found) = find_node(child, item_name, source) {
                return Some(found);
            }
        }
        None
    }

    find_node(tree.root_node(), item_name, source_code.as_bytes())
        .map(|node| node.utf8_text(source_code.as_bytes()).unwrap().to_string())
        .context(format!("Item '{item_name}' not found in source code."))
}

fn extract_docstring(source_code: &str, item_name: &str) -> Option<String> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .is_err()
    {
        return None;
    }

    let tree = parser.parse(source_code, None)?;

    fn find_docstring<'a>(
        node: tree_sitter::Node<'a>,
        item_name: &str,
        source: &'a [u8],
    ) -> Option<String> {
        // Look for function or class with matching name
        match node.kind() {
            "function_definition" | "class_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source).ok() == Some(item_name) {
                        // Look for the body and find the first string literal
                        if let Some(body) = node.child_by_field_name("body") {
                            let mut cursor = body.walk();
                            for child in body.children(&mut cursor) {
                                if child.kind() == "expression_statement" {
                                    let mut expr_cursor = child.walk();
                                    for expr_child in child.children(&mut expr_cursor) {
                                        if expr_child.kind() == "string" {
                                            if let Ok(text) = expr_child.utf8_text(source) {
                                                // Remove quotes and clean up docstring
                                                let cleaned = text
                                                    .trim_start_matches("\"\"\"")
                                                    .trim_start_matches("'''")
                                                    .trim_end_matches("\"\"\"")
                                                    .trim_end_matches("'''")
                                                    .trim();
                                                return Some(cleaned.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return None;
                    }
                }
            }
            _ => {}
        }

        // Recursively search children
        for child in node.children(&mut node.walk()) {
            if let Some(found) = find_docstring(child, item_name, source) {
                return Some(found);
            }
        }
        None
    }

    find_docstring(tree.root_node(), item_name, source_code.as_bytes())
}

#[async_trait]
impl LanguageProcessor for PythonProcessor {
    async fn get_implementation_context(
        &self,
        package_name: &str,
        context_path: &Path,
        relative_path: &str,
        item_name: &str,
    ) -> Result<ImplementationContext> {
        let package_root =
            finder::find_python_package_path_with_context(package_name, Some(context_path))?;
        let file_path = package_root.join(relative_path);
        let source_code = tokio::fs::read_to_string(&file_path).await?;
        let implementation = extract_item_by_name(&source_code, item_name)?;
        let documentation = extract_docstring(&source_code, item_name);

        Ok(ImplementationContext {
            file_path: file_path.to_string_lossy().into_owned(),
            item_name: item_name.to_string(),
            documentation,
            implementation,
            language: "python".to_string(),
        })
    }
}
