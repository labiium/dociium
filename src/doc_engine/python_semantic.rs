//! Semantic indexing and search for local Python packages.
//!
//! This module builds a lightweight semantic index for Python packages by
//! extracting public symbols (functions/classes), docstrings, and structural
//! metadata from local source files. The index provides cosine-similarity
//! search over TF‑IDF vectors derived from symbol names, docstrings, and
//! module context — enabling natural-language discovery of functionality.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use tree_sitter::{Node, Parser};
use walkdir::WalkDir;

use crate::shared_types::SemanticSearchResult;

const MAX_SNIPPET_LINES: usize = 6;
const MAX_DOC_PREVIEW_CHARS: usize = 200;

/// Semantic index for a Python package.
#[derive(Debug)]
pub struct PythonSemanticIndex {
    package_name: String,
    package_root: PathBuf,
    entries: Vec<PythonSemanticEntry>,
    idf: HashMap<String, f32>,
}

#[derive(Debug)]
struct PythonSemanticEntry {
    name: String,
    name_lower: String,
    qualified_path: String,
    qualified_lower: String,
    module_path: String,
    kind: String,
    file_path: PathBuf,
    line: u32,
    doc_preview: Option<String>,
    signature: Option<String>,
    source_preview: Option<String>,
    vector: SemanticVector,
}

#[derive(Debug)]
struct SemanticVector {
    weights: HashMap<String, f32>,
    norm: f32,
}

impl SemanticVector {
    fn new(weights: HashMap<String, f32>) -> Self {
        Self { weights, norm: 1.0 }
    }

    fn apply_idf(&mut self, idf: &HashMap<String, f32>) {
        for (token, weight) in self.weights.iter_mut() {
            let idf_weight = idf.get(token).copied().unwrap_or(1.0);
            *weight *= idf_weight;
        }
        let norm = self
            .weights
            .values()
            .map(|v| v * v)
            .fold(0.0_f32, |acc, val| acc + val);
        self.norm = norm.sqrt().max(f32::EPSILON);
    }
}

impl PythonSemanticIndex {
    /// Build a semantic index for the given package root.
    pub fn build(package_name: &str, package_root: &Path) -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .context("Unable to load tree-sitter grammar for Python")?;

        let mut entries = Vec::new();

        for entry in WalkDir::new(package_root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|s| s.to_str()) != Some("py") {
                continue;
            }

            let source = fs::read_to_string(entry.path()).with_context(|| {
                format!(
                    "Failed to read Python source file {}",
                    entry.path().display()
                )
            })?;

            let tree = parser
                .parse(&source, None)
                .ok_or_else(|| anyhow!("Failed to parse {}", entry.path().display()))?;

            let lines: Vec<&str> = source.lines().collect();
            entries.extend(extract_entries(
                &tree,
                &source,
                &lines,
                package_name,
                package_root,
                entry.path(),
            )?);
        }

        if entries.is_empty() {
            return Err(anyhow!(
                "No Python symbols discovered for package '{}'",
                package_name
            ));
        }

        let mut document_frequency: HashMap<String, usize> = HashMap::new();
        for entry in &entries {
            let mut seen = HashSet::new();
            for token in entry.vector.weights.keys() {
                if seen.insert(token.as_str()) {
                    *document_frequency.entry(token.clone()).or_insert(0) += 1;
                }
            }
        }

        let doc_count = entries.len() as f32;
        let mut idf = HashMap::with_capacity(document_frequency.len());
        for (token, df) in document_frequency {
            let weight = ((doc_count + 1.0) / (df as f32 + 1.0)).ln() + 1.0;
            idf.insert(token, weight);
        }

        for entry in entries.iter_mut() {
            entry.vector.apply_idf(&idf);
        }

        Ok(Self {
            package_name: package_name.to_string(),
            package_root: package_root.to_path_buf(),
            entries,
            idf,
        })
    }

    /// Execute a semantic search query, returning ranked results.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SemanticSearchResult> {
        if query.trim().is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut query_vector = build_text_vector(query, 1.0, 1.0);
        for (token, weight) in query_vector.iter_mut() {
            let idf_weight = self.idf.get(token).copied().unwrap_or(1.0);
            *weight *= idf_weight;
        }
        let query_norm = query_vector
            .values()
            .map(|v| v * v)
            .fold(0.0_f32, |acc, val| acc + val)
            .sqrt()
            .max(f32::EPSILON);
        let query_text = query.to_lowercase();

        let mut scored: Vec<(f32, &PythonSemanticEntry)> = Vec::new();
        for entry in &self.entries {
            let mut dot = 0.0_f32;
            for (token, q_weight) in &query_vector {
                if let Some(doc_weight) = entry.vector.weights.get(token) {
                    dot += q_weight * doc_weight;
                }
            }
            if dot == 0.0 {
                continue;
            }
            let mut score = dot / (entry.vector.norm * query_norm);

            if entry.name_lower == query_text {
                score += 0.35;
            } else if entry.name_lower.contains(&query_text) {
                score += 0.2;
            } else if entry.qualified_lower.contains(&query_text) {
                score += 0.1;
            }

            scored.push((score, entry));
        }

        scored.sort_by(|(a_score, a_entry), (b_score, b_entry)| {
            b_score
                .partial_cmp(a_score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a_entry.module_path.cmp(&b_entry.module_path))
                .then_with(|| a_entry.name.cmp(&b_entry.name))
        });

        scored
            .into_iter()
            .take(limit)
            .map(|(score, entry)| SemanticSearchResult {
                language: "python".to_string(),
                package: self.package_name.clone(),
                module_path: entry.module_path.clone(),
                item_name: entry.name.clone(),
                qualified_path: entry.qualified_path.clone(),
                kind: entry.kind.clone(),
                file: entry.file_path.to_string_lossy().into_owned(),
                line: entry.line,
                score,
                doc_preview: entry.doc_preview.clone(),
                signature: entry.signature.clone(),
                source_preview: entry.source_preview.clone(),
            })
            .collect()
    }

    /// Return the on-disk root used for indexing.
    pub fn package_root(&self) -> &Path {
        &self.package_root
    }
}

fn extract_entries(
    tree: &tree_sitter::Tree,
    source: &str,
    lines: &[&str],
    package_name: &str,
    package_root: &Path,
    file_path: &Path,
) -> Result<Vec<PythonSemanticEntry>> {
    let mut entries = Vec::new();
    let module_path = module_path(package_name, package_root, file_path)?;

    fn visit(
        node: Node,
        source: &str,
        lines: &[&str],
        module_path: &str,
        file_path: &Path,
        entries: &mut Vec<PythonSemanticEntry>,
    ) -> Result<()> {
        match node.kind() {
            "function_definition" | "class_definition" => {
                if let Some(entry) =
                    build_entry(node, source, lines, module_path, file_path, node.kind())?
                {
                    entries.push(entry);
                }
            }
            _ => {
                let mut walk = node.walk();
                for child in node.children(&mut walk) {
                    visit(child, source, lines, module_path, file_path, entries)?;
                }
            }
        }
        Ok(())
    }

    visit(
        tree.root_node(),
        source,
        lines,
        &module_path,
        file_path,
        &mut entries,
    )?;
    Ok(entries)
}

fn build_entry(
    node: Node,
    source: &str,
    lines: &[&str],
    module_path: &str,
    file_path: &Path,
    kind: &str,
) -> Result<Option<PythonSemanticEntry>> {
    let name_node = node
        .child_by_field_name("name")
        .ok_or_else(|| anyhow!("Missing name for Python {kind} definition"))?;
    let name = name_node
        .utf8_text(source.as_bytes())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return Ok(None);
    }

    let docstring = docstring_for_node(&node, source);
    let doc_preview = docstring
        .as_ref()
        .map(|d| trim_preview(d, MAX_DOC_PREVIEW_CHARS));

    let start_row = node.start_position().row;
    let line = (start_row + 1) as u32;
    let signature = lines
        .get(start_row)
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty());

    let source_preview = snippet_for_node(&node, lines, start_row);

    let mut vector = HashMap::new();
    accumulate_identifier_tokens(&mut vector, &name, 1.6);
    accumulate_identifier_tokens(&mut vector, module_path, 0.75);
    if let Some(sig) = &signature {
        accumulate_text_tokens(&mut vector, sig, 0.6);
    }
    if let Some(doc) = &docstring {
        accumulate_text_tokens(&mut vector, doc, 1.25);
    }
    // Capture context of enclosing module path as phrase.
    accumulate_text_tokens(&mut vector, &module_path.replace('.', " "), 0.4);

    let qualified_path = format!("{}.{}", module_path, name);

    Ok(Some(PythonSemanticEntry {
        name_lower: name.to_lowercase(),
        qualified_lower: qualified_path.to_lowercase(),
        name,
        qualified_path,
        module_path: module_path.to_string(),
        kind: kind.trim_end_matches("_definition").to_string(),
        file_path: file_path.to_path_buf(),
        line,
        doc_preview,
        signature,
        source_preview,
        vector: SemanticVector::new(vector),
    }))
}

fn docstring_for_node(node: &Node, source: &str) -> Option<String> {
    let body = node.child_by_field_name("body")?;
    let mut walk = body.walk();
    let mut first_child = body.children(&mut walk);
    let first = first_child.next()?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let mut expr_walk = first.walk();
    for child in first.children(&mut expr_walk) {
        if child.kind() == "string" {
            let raw = child.utf8_text(source.as_bytes()).ok()?;
            return Some(clean_docstring(raw));
        }
    }
    None
}

fn clean_docstring(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = strip_delimited(trimmed).unwrap_or(trimmed);

    let lines: Vec<&str> = inner.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    if lines.len() == 1 {
        return lines[0].trim().to_string();
    }

    let mut min_indent: Option<usize> = None;
    for line in lines.iter().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let leading = line.chars().take_while(|c| c.is_whitespace()).count();
        min_indent = Some(min_indent.map_or(leading, |acc| acc.min(leading)));
    }
    let indent = min_indent.unwrap_or(0);

    let mut result = Vec::with_capacity(lines.len());
    result.push(lines[0].trim().to_string());
    for line in lines.iter().skip(1) {
        if line.len() >= indent {
            result.push(line[indent..].trim_end().to_string());
        } else {
            result.push(line.trim_end().to_string());
        }
    }
    result.join("\n").trim().to_string()
}

fn strip_delimited(trimmed: &str) -> Option<&str> {
    const DELIMS: [(&str, usize); 4] = [("\"\"\"", 3), ("'''", 3), ("\"", 1), ("'", 1)];
    for (delim, count) in DELIMS {
        if trimmed.starts_with(delim) && trimmed.ends_with(delim) && trimmed.len() >= count * 2 {
            let end = trimmed.len() - count;
            return Some(&trimmed[count..end]);
        }
    }
    None
}

fn snippet_for_node(node: &Node, lines: &[&str], start_row: usize) -> Option<String> {
    if start_row >= lines.len() {
        return None;
    }
    let end_row = node.end_position().row;
    let end = end_row.min(start_row + MAX_SNIPPET_LINES);
    let snippet = lines[start_row..=end.min(lines.len() - 1)]
        .iter()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    Some(snippet)
}

fn trim_preview(doc: &str, max_chars: usize) -> String {
    if doc.len() <= max_chars {
        return doc.to_string();
    }
    let mut preview = doc[..max_chars].to_string();
    preview.push('…');
    preview
}

fn module_path(package_name: &str, package_root: &Path, file: &Path) -> Result<String> {
    let rel = file
        .strip_prefix(package_root)
        .with_context(|| format!("{} is not under {}", file.display(), package_root.display()))?;

    let mut components: Vec<String> = Vec::new();
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            if let Some(part) = name.to_str() {
                components.push(part.to_string());
            }
        }
    }
    if let Some(last) = components.last() {
        if last == "__init__.py" {
            components.pop();
        }
    }
    if let Some(last) = components.last_mut() {
        if last.ends_with(".py") {
            let _ = last.split_off(last.len() - 3);
        }
    }
    let suffix = components
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if suffix.is_empty() {
        Ok(package_name.to_string())
    } else {
        Ok(format!("{}.{}", package_name, suffix.join(".")))
    }
}

fn accumulate_identifier_tokens(target: &mut HashMap<String, f32>, ident: &str, weight: f32) {
    if ident.is_empty() {
        return;
    }
    let lower = ident.to_lowercase();
    *target.entry(lower.clone()).or_insert(0.0) += weight;

    for token in split_identifier(ident) {
        *target.entry(token).or_insert(0.0) += weight * 0.8;
    }
}

fn accumulate_text_tokens(target: &mut HashMap<String, f32>, text: &str, weight: f32) {
    let tokens = split_freeform(text);
    for token in tokens {
        if is_stop_word(&token) {
            continue;
        }
        *target.entry(token).or_insert(0.0) += weight;
    }
}

fn build_text_vector(text: &str, main_weight: f32, phrase_bonus: f32) -> HashMap<String, f32> {
    let mut vector = HashMap::new();
    let tokens = split_freeform(text);
    for token in &tokens {
        if is_stop_word(token) {
            continue;
        }
        *vector.entry(token.clone()).or_insert(0.0) += main_weight;
    }
    for window in tokens.windows(2) {
        if window.iter().all(|tok| !is_stop_word(tok)) {
            let phrase = format!("{} {}", window[0], window[1]);
            *vector.entry(phrase).or_insert(0.0) += phrase_bonus;
        }
    }
    vector
}

fn split_identifier(ident: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in ident.chars() {
        if ch == '_' || ch == '.' {
            if !current.is_empty() {
                tokens.push(current.to_lowercase());
                current.clear();
            }
            continue;
        }
        if ch.is_uppercase() && !current.is_empty() {
            tokens.push(current.to_lowercase());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current.to_lowercase());
    }
    tokens
}

fn split_freeform(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn is_stop_word(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "or"
            | "for"
            | "with"
            | "of"
            | "a"
            | "an"
            | "to"
            | "in"
            | "is"
            | "are"
            | "on"
            | "by"
            | "be"
            | "this"
            | "that"
            | "it"
            | "from"
            | "into"
            | "as"
            | "at"
            | "self"
            | "cls"
            | "returns"
            | "return"
            | "args"
            | "kwargs"
            | "true"
            | "false"
            | "none"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn indexes_and_searches_python_symbols() {
        let tmp = tempdir().unwrap();
        let pkg_root = tmp.path().join("sample_pkg");
        fs::create_dir_all(&pkg_root).unwrap();

        write_file(
            &pkg_root.join("__init__.py"),
            "from .account import register_user, deactivate_user\n",
        );
        write_file(
            &pkg_root.join("account.py"),
            r#"
class AccountManager:
    """High level helper coordinating account lifecycle events."""

    def create(self, email, password):
        """Provision a new account and send the onboarding email."""
        return {"email": email}

def register_user(email: str, password: str) -> dict:
    """Create a brand new user account and emit onboarding events."""
    return {"email": email}

def deactivate_user(user_id: str) -> None:
    """Disable an account and revoke all refresh tokens for security."""
    pass
"#,
        );

        let index = PythonSemanticIndex::build("sample_pkg", &pkg_root).unwrap();
        let results = index.search("create user account", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].item_name, "register_user");
        assert!(results[0]
            .doc_preview
            .as_ref()
            .unwrap()
            .contains("brand new user account"));
        assert_eq!(results[0].module_path, "sample_pkg.account");

        let deactivate = index
            .search("revoke refresh token", 5)
            .into_iter()
            .find(|res| res.item_name == "deactivate_user")
            .expect("expected deactivate_user result");
        assert!(deactivate.score > 0.0);
        assert!(deactivate
            .doc_preview
            .unwrap()
            .contains("revoke all refresh tokens"));

        let manager = index
            .search("lifecycle helper", 5)
            .into_iter()
            .find(|res| res.item_name == "AccountManager")
            .expect("expected AccountManager class");
        assert_eq!(manager.kind, "class");
        assert!(manager.doc_preview.unwrap().contains("lifecycle"));
    }
}
