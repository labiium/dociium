use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationContext {
    pub file_path: String,
    pub item_name: String,
    pub documentation: Option<String>,
    pub implementation: String,
    pub language: String,
}

#[async_trait]
pub trait LanguageProcessor {
    async fn get_implementation_context(
        &self,
        package_name: &str,
        context_path: &Path, // CWD or project root for resolving dependencies
        relative_path: &str, // e.g. "utils.py"
        item_name: &str,     // e.g. "my_function"
    ) -> Result<ImplementationContext>;
}
