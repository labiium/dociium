use anyhow::{anyhow, Context, Result};
use regex::Regex;
use std::fs;
use walkdir::WalkDir;

use crate::{
    finder,
    types::{ItemDoc, SourceLocation},
};

/// Fetch documentation for a Rust item by reading locally downloaded source files.
///
/// This looks for the crate in the local cargo registry (or Rust sysroot for
/// standard library crates) and extracts `///` style documentation comments for
/// the requested item path.
pub fn fetch_local_item_doc(crate_name: &str, version: &str, item_path: &str) -> Result<ItemDoc> {
    let crate_root = finder::find_rust_crate_path(crate_name, version)?;
    let segments: Vec<&str> = item_path.split("::").collect();
    let item_name = segments.last().ok_or_else(|| anyhow!("Empty item path"))?;

    // Regex to capture doc comments and the item's signature
    let re = Regex::new(&format!(
        r"(?m)^(?P<docs>(?:\s*///.*\n)*)\s*(?P<sig>pub\s+(?P<kind>struct|enum|trait|fn|type)\s+{}[^\n]*)",
        regex::escape(item_name)
    ))?;

    for entry in WalkDir::new(&crate_root).into_iter().filter_map(|e| e.ok()) {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
            let content = fs::read_to_string(entry.path())
                .with_context(|| format!("Failed to read {}", entry.path().display()))?;
            if let Some(caps) = re.captures(&content) {
                let docs = caps
                    .name("docs")
                    .map(|m| {
                        m.as_str()
                            .lines()
                            .map(|l| l.trim_start().trim_start_matches("///").trim())
                            .filter(|l| !l.is_empty())
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                let signature = caps
                    .name("sig")
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();
                let kind = caps
                    .name("kind")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                // Determine line number of the signature
                let sig_start = caps.name("sig").unwrap().start();
                let line = content[..sig_start].lines().count() + 1;

                return Ok(ItemDoc {
                    path: item_path.to_string(),
                    kind,
                    rendered_markdown: docs,
                    source_location: Some(SourceLocation {
                        file: entry.path().to_string_lossy().into_owned(),
                        line: line as u32,
                        column: 1,
                        end_line: None,
                        end_column: None,
                    }),
                    visibility: "public".to_string(),
                    attributes: vec![],
                    signature: Some(signature),
                    examples: vec![],
                    see_also: vec![],
                });
            }
        }
    }
    Err(anyhow!(
        "Item '{}' not found in crate '{}'",
        item_path,
        crate_name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    struct CargoHomeGuard(Option<String>);
    impl CargoHomeGuard {
        fn set(path: &Path) -> Self {
            let old = std::env::var("CARGO_HOME").ok();
            std::env::set_var("CARGO_HOME", path);
            CargoHomeGuard(old)
        }
    }
    impl Drop for CargoHomeGuard {
        fn drop(&mut self) {
            if let Some(ref old) = self.0 {
                std::env::set_var("CARGO_HOME", old);
            } else {
                std::env::remove_var("CARGO_HOME");
            }
        }
    }

    /// Set up a temporary crate in a fake cargo registry for testing
    fn setup_crate() -> (tempfile::TempDir, CargoHomeGuard) {
        let temp = tempdir().unwrap();
        let guard = CargoHomeGuard::set(temp.path());
        let crate_dir = temp
            .path()
            .join("registry")
            .join("src")
            .join("test-reg")
            .join("mycrate-0.1.0");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(
            crate_dir.join("src/lib.rs"),
            concat!(
                "/// Example struct\n",
                "pub struct MyStruct;\n\n",
                "/// Example function\n",
                "pub fn my_fn() {}\n",
            ),
        )
        .unwrap();
        (temp, guard)
    }

    #[test]
    fn fetches_struct_docs() {
        let (_dir, _guard) = setup_crate();
        let doc = fetch_local_item_doc("mycrate", "0.1.0", "mycrate::MyStruct").unwrap();
        assert_eq!(doc.kind, "struct");
        assert_eq!(doc.rendered_markdown, "Example struct");
        assert_eq!(doc.signature.as_deref(), Some("pub struct MyStruct;"));
        assert_eq!(doc.source_location.unwrap().line, 2);
    }

    #[test]
    fn fetches_function_docs() {
        let (_dir, _guard) = setup_crate();
        let doc = fetch_local_item_doc("mycrate", "0.1.0", "mycrate::my_fn").unwrap();
        assert_eq!(doc.kind, "fn");
        assert_eq!(doc.rendered_markdown, "Example function");
        assert!(doc.signature.unwrap().starts_with("pub fn my_fn"));
        assert_eq!(doc.source_location.unwrap().line, 5);
    }

    #[test]
    fn missing_item_errors() {
        let (_dir, _guard) = setup_crate();
        let err = fetch_local_item_doc("mycrate", "0.1.0", "mycrate::Missing").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
