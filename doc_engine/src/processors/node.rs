use super::traits::{ImplementationContext, LanguageProcessor};
use crate::finder;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use tokio::fs;
use tree_sitter::Parser;

#[derive(Debug, Default)]
pub struct NodeProcessor;

#[derive(Clone, Copy)]
enum JsTsLanguage {
    JavaScript,
    TypeScript,
}

fn extract_item_by_name(source_code: &str, item_name: &str, lang: JsTsLanguage) -> Result<String> {
    let language = match lang {
        JsTsLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        JsTsLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    };
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .context("Error loading tree-sitter grammar for Node.js language")?;

    let tree = parser.parse(source_code, None).unwrap();

    fn find_node<'a>(
        node: tree_sitter::Node<'a>,
        item_name: &str,
        source: &'a [u8],
    ) -> Option<tree_sitter::Node<'a>> {
        // Check for function declarations, function expressions, class declarations
        match node.kind() {
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "class_declaration"
            | "method_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source).ok() == Some(item_name) {
                        return Some(node);
                    }
                }
            }
            "variable_declarator" => {
                // Handle const/let/var declarations like: const myFunc = () => {}
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source).ok() == Some(item_name) {
                        return Some(node);
                    }
                }
            }
            "export_statement" => {
                // Handle export statements
                if let Some(declaration) = node.child_by_field_name("declaration") {
                    if let Some(found) = find_node(declaration, item_name, source) {
                        return Some(found);
                    }
                }
            }
            _ => {}
        }

        // Generic name field check for other node types
        if let Some(name_node) = node.child_by_field_name("name") {
            if name_node.utf8_text(source).ok() == Some(item_name) {
                return Some(node);
            }
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
        .context(format!("Item '{}' not found in source code.", item_name))
}

fn extract_jsdoc_comment(source_code: &str, item_name: &str, lang: JsTsLanguage) -> Option<String> {
    let language = match lang {
        JsTsLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        JsTsLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    };
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return None;
    }

    let tree = parser.parse(source_code, None)?;

    fn find_jsdoc<'a>(
        node: tree_sitter::Node<'a>,
        item_name: &str,
        source: &'a [u8],
    ) -> Option<String> {
        // Look for function, class, or variable with matching name
        match node.kind() {
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "class_declaration"
            | "method_definition"
            | "variable_declarator" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source).ok() == Some(item_name) {
                        // Look for a preceding comment
                        if let Some(prev_sibling) = node.prev_sibling() {
                            if prev_sibling.kind() == "comment" {
                                if let Ok(text) = prev_sibling.utf8_text(source) {
                                    // Clean up JSDoc comment
                                    let cleaned = text
                                        .lines()
                                        .map(|line| {
                                            line.trim_start_matches("*")
                                                .trim_start_matches("/")
                                                .trim()
                                        })
                                        .filter(|line| !line.is_empty())
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    return Some(cleaned);
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
            if let Some(found) = find_jsdoc(child, item_name, source) {
                return Some(found);
            }
        }
        None
    }

    find_jsdoc(tree.root_node(), item_name, source_code.as_bytes())
}

#[async_trait]
impl LanguageProcessor for NodeProcessor {
    async fn get_implementation_context(
        &self,
        package_name: &str,
        context_path: &Path,
        relative_path: &str,
        item_name: &str,
    ) -> Result<ImplementationContext> {
        let package_root = finder::find_node_package_path(package_name, context_path)?;
        let file_path = package_root.join(relative_path);
        let source_code = fs::read_to_string(&file_path).await?;

        let lang_type = if relative_path.ends_with(".ts") || relative_path.ends_with(".tsx") {
            JsTsLanguage::TypeScript
        } else {
            JsTsLanguage::JavaScript
        };

        let implementation = extract_item_by_name(&source_code, item_name, lang_type)?;
        let documentation = extract_jsdoc_comment(&source_code, item_name, lang_type);
        let language_name = match lang_type {
            JsTsLanguage::TypeScript => "typescript",
            JsTsLanguage::JavaScript => "javascript",
        }
        .to_string();

        Ok(ImplementationContext {
            file_path: file_path.to_string_lossy().into_owned(),
            item_name: item_name.to_string(),
            documentation,
            implementation,
            language: language_name,
        })
    }
}
