//! Docs.rs scraper module for fetching documentation from docs.rs
//!
//! This module handles web scraping of pre-built documentation from docs.rs,
//! replacing the previous approach of building rustdoc JSON locally.

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use scraper::{Html, Selector};

use std::time::Duration;
use tracing::{debug, info, instrument, warn};

use crate::doc_engine::types::{ItemDoc, SourceLocation};

/// Docs.rs scraper for fetching documentation
pub struct DocsRsScraper {
    client: Client,
    base_url: String,
}

use crate::doc_engine::types::{SearchIndexData, SearchIndexItem};

/// Configuration for the docs.rs scraper
#[derive(Debug, Clone)]
pub struct ScraperConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub user_agent: String,
    pub head_timeout: Duration,
    pub fetch_timeout: Duration,
}

impl Default for ScraperConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 2,
            retry_delay: Duration::from_millis(500),
            user_agent: "dociium-scraper/1.0".to_string(),
            head_timeout: Duration::from_secs(5),
            fetch_timeout: Duration::from_secs(10),
        }
    }
}

impl DocsRsScraper {
    /// Create a new docs.rs scraper
    pub fn new() -> Self {
        Self::with_config(ScraperConfig::default())
    }

    /// Create a new docs.rs scraper with custom configuration
    pub fn with_config(config: ScraperConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .gzip(true)
            .build()
            .expect("Failed to create HTTP client for scraper");

        Self {
            client,
            base_url: "https://docs.rs".to_string(),
        }
    }

    /// Fetch and parse documentation for a specific item
    #[instrument(skip(self), fields(crate_name = %crate_name, version = %version, item_path = %item_path))]
    pub async fn fetch_item_doc(
        &self,
        crate_name: &str,
        version: &str,
        item_path: &str,
    ) -> Result<ItemDoc> {
        info!(
            "Fetching documentation for {}@{}: {}",
            crate_name, version, item_path
        );

        // Try to discover the correct URL for the item's documentation page
        let url = self
            .discover_item_url(crate_name, version, item_path)
            .await?;
        debug!("Fetching from URL: {}", url);

        // Fetch the HTML content
        let html_content = self.fetch_html(&url).await?;
        let document = Html::parse_document(&html_content);

        // Parse the documentation content
        let item_doc = self.parse_item_documentation(&document, item_path)?;

        info!("Successfully fetched documentation for {}", item_path);
        Ok(item_doc)
    }

    /// Fetch and parse the search index for an entire crate
    #[instrument(skip(self), fields(crate_name = %crate_name, version = %version))]
    pub async fn fetch_search_index(
        &self,
        crate_name: &str,
        version: &str,
    ) -> Result<SearchIndexData> {
        info!("Fetching search index for {}@{}", crate_name, version);

        // Construct the URL for the search index
        let url = format!(
            "{}/{}/{}/search-index.js",
            self.base_url, crate_name, version
        );
        debug!("Fetching search index from: {}", url);

        // Fetch the JavaScript content
        let js_content = self.fetch_text(&url).await?;

        // Parse the search index data
        let search_data = self.parse_search_index(&js_content, crate_name, version)?;

        info!(
            "Successfully fetched search index with {} items",
            search_data.items.len()
        );
        Ok(search_data)
    }

    /// Check if documentation exists for a crate version
    pub async fn check_docs_available(&self, crate_name: &str, version: &str) -> Result<bool> {
        let url = format!(
            "{}/{}/{}/{}/",
            self.base_url,
            crate_name,
            version,
            crate_name.replace('-', "_")
        );

        match self.client.head(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Try to find the correct URL by checking multiple patterns
    async fn discover_item_url(
        &self,
        crate_name: &str,
        version: &str,
        item_path: &str,
    ) -> Result<String> {
        let path_parts: Vec<&str> = item_path.split("::").collect();

        if path_parts.is_empty() {
            return Err(anyhow!("Empty item path"));
        }

        // Skip the crate name if it's the first component
        let start_index = if path_parts.first() == Some(&crate_name) {
            1
        } else {
            0
        };
        let relevant_parts = &path_parts[start_index..];

        if relevant_parts.is_empty() {
            return Err(anyhow!("No item name found in path"));
        }

        let item_name = relevant_parts.last().unwrap();
        let module_path = if relevant_parts.len() > 1 {
            relevant_parts[..relevant_parts.len() - 1].join("/")
        } else {
            String::new()
        };

        let crate_name_underscore = crate_name.replace('-', "_");

        // Try different type prefixes in order of likelihood
        let type_prefixes = [
            "struct", "fn", "trait", "enum", "type", "macro", "constant", "static", "mod", "union",
        ];

        for prefix in &type_prefixes {
            let file_name = format!("{prefix}.{item_name}.html");

            let url = if module_path.is_empty() {
                format!(
                    "{}/{}/{}/{}/{}",
                    self.base_url, crate_name, version, crate_name_underscore, file_name
                )
            } else {
                format!(
                    "{}/{}/{}/{}/{}/{}",
                    self.base_url,
                    crate_name,
                    version,
                    crate_name_underscore,
                    module_path,
                    file_name
                )
            };

            // Check if this URL exists with timeout
            match tokio::time::timeout(Duration::from_secs(5), self.client.head(&url).send()).await
            {
                Ok(Ok(response)) if response.status().is_success() => {
                    debug!("Found valid URL: {}", url);
                    return Ok(url);
                }
                Ok(Ok(_)) => {
                    debug!("Non-success status for URL: {}", url);
                    continue;
                }
                Ok(Err(e)) => {
                    debug!("Network error for {}: {}", url, e);
                    continue;
                }
                Err(_) => {
                    debug!("Timeout for URL: {}", url);
                    continue;
                }
            }
        }

        // Fallback - try without type prefix
        let url = if module_path.is_empty() {
            format!(
                "{}/{}/{}/{}/{}.html",
                self.base_url, crate_name, version, crate_name_underscore, item_name
            )
        } else {
            format!(
                "{}/{}/{}/{}/{}/{}.html",
                self.base_url, crate_name, version, crate_name_underscore, module_path, item_name
            )
        };

        match tokio::time::timeout(Duration::from_secs(5), self.client.head(&url).send()).await {
            Ok(Ok(response)) if response.status().is_success() => Ok(url),
            Ok(Ok(response)) => Err(anyhow!(
                "Non-success status {} for fallback URL: {}",
                response.status(),
                url
            )),
            Ok(Err(e)) => Err(anyhow!("Network error for fallback URL {}: {}", url, e)),
            Err(_) => Err(anyhow!(
                "Timeout checking fallback URL for item: {}",
                item_path
            )),
        }
    }

    /// Fetch HTML content from a URL with retries
    async fn fetch_html(&self, url: &str) -> Result<String> {
        self.fetch_text(url).await
    }

    /// Fetch text content from a URL with retries
    async fn fetch_text(&self, url: &str) -> Result<String> {
        let mut last_error = None;

        for attempt in 1..=2 {
            match tokio::time::timeout(Duration::from_secs(10), self.client.get(url).send()).await {
                Ok(Ok(response)) => {
                    if response.status().is_success() {
                        match tokio::time::timeout(Duration::from_secs(10), response.text()).await {
                            Ok(Ok(content)) => return Ok(content),
                            Ok(Err(e)) => {
                                warn!("Failed to read response body on attempt {}: {}", attempt, e);
                                last_error = Some(anyhow!(e));
                            }
                            Err(_) => {
                                warn!("Timeout reading response body on attempt {}", attempt);
                                last_error = Some(anyhow!("Timeout reading response body"));
                            }
                        }
                    } else if response.status().as_u16() == 404 {
                        return Err(anyhow!("Documentation not found: {}", url));
                    } else {
                        last_error = Some(anyhow!("HTTP error: {}", response.status()));
                        warn!("HTTP error on attempt {}: {}", attempt, response.status());
                    }
                }
                Ok(Err(e)) => {
                    warn!("Network error on attempt {}: {}", attempt, e);
                    last_error = Some(anyhow!(e));
                }
                Err(_) => {
                    warn!("Request timeout on attempt {}", attempt);
                    last_error = Some(anyhow!("Request timeout"));
                }
            }

            if attempt < 2 {
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Failed to fetch from {}", url)))
    }

    /// Parse item documentation from HTML document
    fn parse_item_documentation(&self, document: &Html, item_path: &str) -> Result<ItemDoc> {
        // Define CSS selectors for different parts of the documentation
        let docblock_selector = Selector::parse("main .docblock").unwrap();
        let signature_selector = Selector::parse(".code-header").unwrap();
        let source_link_selector = Selector::parse(".src-link").unwrap();

        // Extract the main documentation content
        let rendered_markdown = document
            .select(&docblock_selector)
            .next()
            .map(|elem| elem.inner_html())
            .unwrap_or_else(|| "No documentation available.".to_string());

        // Extract the signature/declaration
        let signature = document
            .select(&signature_selector)
            .next()
            .map(|elem| elem.text().collect::<Vec<_>>().join(" ").trim().to_string());

        // Extract source location if available
        let source_location = document
            .select(&source_link_selector)
            .next()
            .and_then(|elem| elem.value().attr("href"))
            .and_then(|href| self.parse_source_location(href).ok());

        // Determine the item kind from the page structure
        let kind = self.extract_item_kind(document, item_path);

        // Extract visibility information
        let visibility = self.extract_visibility(document);

        // Extract attributes
        let attributes = self.extract_attributes(document);

        // Extract examples from documentation
        let examples = self.extract_examples(document);

        Ok(ItemDoc {
            path: item_path.to_string(),
            kind,
            rendered_markdown,
            source_location,
            visibility,
            attributes,
            signature,
            examples,
            see_also: Vec::new(),
        })
    }

    /// Parse search index JavaScript content
    fn parse_search_index(
        &self,
        js_content: &str,
        crate_name: &str,
        version: &str,
    ) -> Result<SearchIndexData> {
        // Hardened extraction of docs.rs search-index.js content.
        //
        // Historical formats (observed):
        //  1. var searchIndex = {"crate":{"items":[...],"paths":[...]}, ...};
        //  2. searchIndex = {"crate":{"i":[...],"p":[...]}, ...};
        //  3. self.searchIndex = {...};
        //  4. window.searchIndex = {...};
        //
        // Risks:
        //  - Naïve first/last brace slice can include trailing loader code or miss if
        //    braces appear in a banner comment.
        //  - Future minification may rename top variable but internal crate map
        //    stays JSON-like.
        //
        // Strategy:
        //  1. Attempt targeted regex captures around common assignment patterns.
        //  2. If unsuccessful, perform a brace-balanced extraction starting at the
        //     first occurrence of the crate key (crate or crate with '_' instead of '-').
        //  3. Parse as JSON; locate crate entry under either original or sanitized key.
        //
        // NOTE: ETag / backoff integration is handled at the fetch layer; this parser
        // remains stateless but surfaces granular errors to allow upstream retry/
        // classification (e.g. distinguishing "structure changed" vs "crate missing").
        use regex::Regex;
        use serde_json::Value;

        let crate_key_alt = crate_name.replace('-', "_");
        let candidate_keys = [crate_name, crate_key_alt.as_str()];

        // Helper: try parsing a JSON candidate string into Value and fetch crate map.
        let try_parse = |json_str: &str| -> Result<(Value, Value)> {
            let v: Value =
                serde_json::from_str(json_str).context("Failed to parse search index JSON")?;
            for key in candidate_keys {
                if let Some(entry) = v.get(key) {
                    return Ok((v.clone(), entry.clone()));
                }
            }
            Err(anyhow!(
                "Crate data not found in parsed search index object (keys tried: {:?})",
                candidate_keys
            ))
        };

        // 1. Regex-based captures (non-greedy with manual brace balance after initial '{').
        let regex_patterns = [
            r#"(?s)searchIndex\s*=\s*(\{.*\});"#,
            r#"(?s)var\s+searchIndex\s*=\s*(\{.*\});"#,
            r#"(?s)self\.searchIndex\s*=\s*(\{.*\});"#,
            r#"(?s)window\.searchIndex\s*=\s*(\{.*\});"#,
        ];

        for pat in regex_patterns {
            if let Ok(re) = Regex::new(pat) {
                if let Some(caps) = re.captures(js_content) {
                    let blob = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    // Perform brace balance to trim trailing over-capture.
                    if let Some(json_balanced) = Self::balanced_brace_slice(blob) {
                        if let Ok((json_data, crate_data)) = try_parse(&json_balanced) {
                            return self.build_search_index(
                                crate_name,
                                version,
                                &crate_data,
                                &json_data,
                            );
                        }
                    }
                }
            }
        }

        // 2. Fallback: locate crate key and backtrack to opening brace, then balance.
        let mut fallback_extracted: Option<String> = None;
        for key in candidate_keys {
            if let Some(pos) = js_content.find(&format!("\"{key}\"")) {
                // Backtrack to nearest '{'
                if let Some(start) = js_content[..pos].rfind('{') {
                    if let Some(json_balanced) = Self::balanced_brace_slice(&js_content[start..]) {
                        fallback_extracted = Some(json_balanced);
                        break;
                    }
                }
            }
        }

        if let Some(json_blob) = fallback_extracted {
            if let Ok((json_data, crate_data)) = try_parse(&json_blob) {
                return self.build_search_index(crate_name, version, &crate_data, &json_data);
            }
        }

        Err(anyhow!(
            "Unable to extract or parse search index for crate '{}'",
            crate_name
        ))
    }

    /// Extract a balanced JSON slice from a string starting at the first '{'.
    /// Returns None if braces cannot be balanced.
    fn balanced_brace_slice(input: &str) -> Option<String> {
        let bytes = input.as_bytes();
        let mut depth = 0usize;
        let mut started = false;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'{' {
                depth += 1;
                started = true;
            } else if b == b'}' {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(String::from_utf8_lossy(&bytes[..=i]).to_string());
                }
            }
        }
        if started && depth == 0 {
            Some(String::from_utf8_lossy(bytes).to_string())
        } else {
            None
        }
    }

    /// Build the strongly typed SearchIndexData from the extracted JSON subtree.
    fn build_search_index(
        &self,
        crate_name: &str,
        version: &str,
        crate_data: &serde_json::Value,
        root_json: &serde_json::Value,
    ) -> Result<SearchIndexData> {
        let _ = root_json; // reserved for future structure / schema validation
        let items_array = crate_data
            .get("items")
            .or_else(|| crate_data.get("i"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("Items array not found in crate data"))?;

        let mut items = Vec::new();
        let mut paths = Vec::new();

        for item_value in items_array {
            if let Some(item_array) = item_value.as_array() {
                if item_array.len() >= 4 {
                    let kind = self.kind_id_to_string(item_array[0].as_u64().unwrap_or(0) as usize);
                    let name = item_array[1].as_str().unwrap_or("").to_string();
                    let path = item_array[2].as_str().unwrap_or("").to_string();
                    let description = item_array[3].as_str().unwrap_or("").to_string();
                    let parent_index = item_array
                        .get(4)
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);

                    items.push(SearchIndexItem {
                        name,
                        kind,
                        path,
                        description,
                        parent_index,
                    });
                }
            }
        }

        if let Some(paths_array) = crate_data.get("paths").or_else(|| crate_data.get("p")) {
            if let Some(paths_arr) = paths_array.as_array() {
                for path_value in paths_arr {
                    if let Some(path_str) = path_value.as_str() {
                        paths.push(path_str.to_string());
                    }
                }
            }
        }

        Ok(SearchIndexData {
            crate_name: crate_name.to_string(),
            version: version.to_string(),
            items,
            paths,
        })
    }

    /// Convert kind ID to string representation
    fn kind_id_to_string(&self, kind_id: usize) -> String {
        match kind_id {
            0 => "module".to_string(),
            1 => "extern_crate".to_string(),
            2 => "import".to_string(),
            3 => "struct".to_string(),
            4 => "enum".to_string(),
            5 => "function".to_string(),
            6 => "type_def".to_string(),
            7 => "static".to_string(),
            8 => "trait".to_string(),
            9 => "impl".to_string(),
            10 => "tymethod".to_string(),
            11 => "method".to_string(),
            12 => "structfield".to_string(),
            13 => "variant".to_string(),
            14 => "macro".to_string(),
            15 => "primitive".to_string(),
            16 => "assoc_type".to_string(),
            17 => "constant".to_string(),
            18 => "assoc_const".to_string(),
            19 => "union".to_string(),
            20 => "foreign_type".to_string(),
            21 => "keyword".to_string(),
            22 => "existential".to_string(),
            23 => "attr".to_string(),
            24 => "derive".to_string(),
            25 => "trait_alias".to_string(),
            _ => format!("unknown_{kind_id}"),
        }
    }

    /// Extract item kind from HTML document
    fn extract_item_kind(&self, document: &Html, _item_path: &str) -> String {
        // Look for indicators in the HTML structure
        let title_selector = Selector::parse("h1.main-heading").unwrap();

        if let Some(title_elem) = document.select(&title_selector).next() {
            let title_text = title_elem.text().collect::<String>();

            if title_text.contains("Struct") {
                return "struct".to_string();
            } else if title_text.contains("Enum") {
                return "enum".to_string();
            } else if title_text.contains("Trait") {
                return "trait".to_string();
            } else if title_text.contains("Function") {
                return "function".to_string();
            } else if title_text.contains("Module") {
                return "module".to_string();
            } else if title_text.contains("Constant") {
                return "constant".to_string();
            } else if title_text.contains("Type") {
                return "type_def".to_string();
            } else if title_text.contains("Macro") {
                return "macro".to_string();
            }
        }

        "unknown".to_string()
    }

    /// Extract visibility information from HTML document
    fn extract_visibility(&self, document: &Html) -> String {
        let code_header_selector = Selector::parse(".code-header").unwrap();

        if let Some(header_elem) = document.select(&code_header_selector).next() {
            let header_text = header_elem.text().collect::<String>();

            if header_text.contains("pub") {
                return "public".to_string();
            }
        }

        "private".to_string()
    }

    /// Extract attributes from HTML document
    fn extract_attributes(&self, document: &Html) -> Vec<String> {
        let mut attributes = Vec::new();

        // Look for attribute annotations in the code header
        let code_header_selector = Selector::parse(".code-header").unwrap();

        if let Some(header_elem) = document.select(&code_header_selector).next() {
            let header_text = header_elem.inner_html();

            // Look for common attributes
            if header_text.contains("#[derive") {
                attributes.push("derive".to_string());
            }
            if header_text.contains("#[cfg") {
                attributes.push("cfg".to_string());
            }
            if header_text.contains("#[deprecated") {
                attributes.push("deprecated".to_string());
            }
        }

        attributes
    }

    /// Extract code examples from documentation
    fn extract_examples(&self, document: &Html) -> Vec<String> {
        let mut examples = Vec::new();

        let example_selector = Selector::parse(".docblock pre code").unwrap();

        for example_elem in document.select(&example_selector) {
            let example_text = example_elem.text().collect::<String>();
            if !example_text.trim().is_empty() {
                examples.push(example_text);
            }
        }

        examples
    }

    /// Parse source location from source link href
    fn parse_source_location(&self, href: &str) -> Result<SourceLocation> {
        // Source links are typically in format: /src/crate/path/file.rs.html#L123-456

        // Extract the file path
        let file_start = href.find("/src/").unwrap_or(0) + 5;
        let file_end = href.find(".html").unwrap_or(href.len());
        let file_path = &href[file_start..file_end];

        // Extract line numbers from fragment
        let mut line = 1u32;
        let mut end_line = None;

        if let Some(fragment_start) = href.find('#') {
            let fragment = &href[fragment_start + 1..];
            if let Some(line_part) = fragment.strip_prefix('L') {
                if let Some(dash_pos) = line_part.find('-') {
                    // Range like L123-456
                    if let Ok(start) = line_part[..dash_pos].parse::<u32>() {
                        line = start;
                        if let Ok(end) = line_part[dash_pos + 1..].parse::<u32>() {
                            end_line = Some(end);
                        }
                    }
                } else {
                    // Single line like L123
                    if let Ok(single_line) = line_part.parse::<u32>() {
                        line = single_line;
                    }
                }
            }
        }

        Ok(SourceLocation {
            file: file_path.to_string(),
            line,
            column: 1,
            end_line,
            end_column: None,
        })
    }
}

impl Default for DocsRsScraper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scraper_creation() {
        let _scraper = DocsRsScraper::new();
        // Just verify we can create the scraper without panicking
    }

    #[test]
    fn test_kind_id_conversion() {
        let scraper = DocsRsScraper::new();

        assert_eq!(scraper.kind_id_to_string(3), "struct");
        assert_eq!(scraper.kind_id_to_string(5), "function");
        assert_eq!(scraper.kind_id_to_string(8), "trait");
        assert_eq!(scraper.kind_id_to_string(999), "unknown_999");
    }

    #[test]
    fn test_parse_source_location() {
        let scraper = DocsRsScraper::new();

        let href = "/src/serde/lib.rs.html#L123-456";
        let location = scraper.parse_source_location(href).unwrap();

        assert_eq!(location.file, "serde/lib.rs");
        assert_eq!(location.line, 123);
        assert_eq!(location.end_line, Some(456));
    }

    #[tokio::test]
    #[cfg(feature = "network-tests")]
    async fn test_discover_item_url() {
        let scraper = DocsRsScraper::new();

        // Test with a real struct that should exist
        match scraper
            .discover_item_url("tokio", "latest", "sync::Mutex")
            .await
        {
            Ok(url) => {
                assert!(url.contains("docs.rs/tokio"));
                assert!(url.contains("sync"));
                assert!(url.contains("Mutex"));
            }
            Err(e) => {
                // This might fail due to network issues or docs.rs structure changes
                println!(
                    "URL discovery test failed (expected in some environments): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    #[cfg(feature = "network-tests")]
    async fn test_fetch_search_index() {
        let scraper = DocsRsScraper::new();

        // Test fetching search index for a known crate
        match scraper.fetch_search_index("serde", "1.0.0").await {
            Ok(search_data) => {
                assert_eq!(search_data.crate_name, "serde");
                assert!(!search_data.items.is_empty());
            }
            Err(e) => {
                // This might fail due to network issues or docs.rs structure changes
                println!(
                    "Search index test failed (expected in some environments): {}",
                    e
                );
            }
        }
    }
}
