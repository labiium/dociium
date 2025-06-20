use anyhow::Result;
use rustdoc_types::{
    Crate as RustdocCrate, Id, Item, ItemEnum, ItemKind, ItemSummary, Module, Visibility,
    FORMAT_VERSION,
};
use scraper::{Html, Selector};
use std::collections::HashMap;

/// Parse a docs.rs HTML page into a minimal [`RustdocCrate`].
pub fn parse_crate_root_from_html(
    html: &str,
    crate_name: &str,
    version: &str,
) -> Result<RustdocCrate> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("div.rustdoc").unwrap();
    let doc_text = document
        .select(&selector)
        .next()
        .map(|n| n.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    let root_id = Id("0".to_string());
    let item = Item {
        id: root_id.clone(),
        crate_id: 0,
        name: Some(crate_name.to_string()),
        span: None,
        visibility: Visibility::Public,
        docs: if doc_text.is_empty() {
            None
        } else {
            Some(doc_text)
        },
        links: HashMap::new(),
        attrs: Vec::new(),
        deprecation: None,
        inner: ItemEnum::Module(Module {
            is_crate: true,
            items: Vec::new(),
            is_stripped: false,
        }),
    };

    let mut index = HashMap::new();
    index.insert(root_id.clone(), item);

    let mut paths = HashMap::new();
    paths.insert(
        root_id.clone(),
        ItemSummary {
            crate_id: 0,
            path: vec![crate_name.to_string()],
            kind: ItemKind::Module,
        },
    );

    Ok(RustdocCrate {
        root: root_id,
        crate_version: Some(version.to_string()),
        includes_private: false,
        index,
        paths,
        external_crates: HashMap::new(),
        format_version: FORMAT_VERSION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetcher::Fetcher;

    #[tokio::test]
    async fn test_parse_html_root_from_docs_rs() {
        let fetcher = Fetcher::new();
        let html = fetcher
            .fetch_crate_html_from_docs_rs("itoa", "1.0.11")
            .await
            .expect("request")
            .expect("html not returned");
        let krate = parse_crate_root_from_html(&html, "itoa", "1.0.11").unwrap();
        let root = krate.index.get(&krate.root).unwrap();
        let doc = root.docs.as_ref().unwrap();
        assert!(doc.contains("integer primitives"));
    }
}
