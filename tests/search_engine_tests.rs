//! External tests for the search engine and query builder functionality.
//!
//! These replace the former inline #[cfg(test)] module inside `index_core::search`.
//! They validate:
//!   - QueryBuilder string assembly
//!   - Default configuration invariants
//!   - Basic / fuzzy / exact search placeholder behaviors
//!   - Suggestions logic
//!   - Filter wiring exposure (even though current engine is placeholder)

use dociium::index_core::search::{QueryBuilder, SearchConfig, SearchEngine, SearchFilters};

/// Helper: assert substrings exist in query output
fn assert_contains_all(haystack: &str, needles: &[&str]) {
    for n in needles {
        assert!(
            haystack.contains(n),
            "Expected query string to contain '{n}', got: {haystack}"
        );
    }
}

#[test]
fn test_query_builder_basic() {
    let query = QueryBuilder::new()
        .add_term("test")
        .must("required")
        .should("optional")
        .must_not("exclude")
        .build_query_string();

    assert_contains_all(&query, &["test", "+required", "optional", "-exclude"]);
}

#[test]
fn test_query_builder_multiple_terms_and_filters() {
    let qb = QueryBuilder::new()
        .add_term("alpha")
        .add_term("beta")
        .must("gamma")
        .must_not("delta")
        .should("epsilon")
        .filter_kinds(vec!["function".into(), "struct".into()]);
    let q = qb.build_query_string();
    assert_contains_all(&q, &["alpha beta", "+gamma", "-delta", "epsilon"]);
    let filters = qb.get_filters();
    assert!(filters.kinds.as_ref().unwrap().contains(&"function".into()));
    assert!(filters.kinds.as_ref().unwrap().contains(&"struct".into()));
}

#[test]
fn test_search_config_default_values() {
    let cfg = SearchConfig::default();
    assert_eq!(cfg.fuzzy_distance, 2);
    assert_eq!(cfg.max_results, 100);
    assert!(cfg.boost_exact_matches);
    assert!(!cfg.case_sensitive);
    assert!(cfg.enable_stemming);
    assert!(cfg.min_score_threshold > 0.0);
}

#[test]
fn test_search_engine_basic_result() {
    let engine = SearchEngine::new().unwrap();
    let cfg = SearchConfig::default();
    let filters = SearchFilters::default();
    let results = engine.search("example_fn", &cfg, &filters).unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.path, "example_fn");
    assert_eq!(r.kind, "function");
    assert!(r.score >= 1.0);
    assert!(r.doc_summary.as_ref().unwrap().contains("example_fn"));
}

#[test]
fn test_search_engine_empty_query() {
    let engine = SearchEngine::new().unwrap();
    let cfg = SearchConfig::default();
    let filters = SearchFilters::default();
    let results = engine.search("", &cfg, &filters).unwrap();
    assert!(
        results.is_empty(),
        "Empty query should return no placeholder results"
    );
}

#[test]
fn test_fuzzy_search_basic() {
    let engine = SearchEngine::new().unwrap();
    let results = engine.fuzzy_search("Widget", 2, 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "Widget");
    assert!(results[0].1 > 0.0);
}

#[test]
fn test_fuzzy_search_empty_term() {
    let engine = SearchEngine::new().unwrap();
    let results = engine.fuzzy_search("", 2, 10).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_suggestions_basic() {
    let engine = SearchEngine::new().unwrap();
    let suggestions = engine.get_suggestions("Vec", 5).unwrap();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.starts_with("Vec")));
}

#[test]
fn test_suggestions_limit() {
    let engine = SearchEngine::new().unwrap();
    let suggestions = engine.get_suggestions("My", 1).unwrap();
    assert!(
        suggestions.len() <= 1,
        "Expected at most 1 suggestion, got {}",
        suggestions.len()
    );
}

#[test]
fn test_exact_search_basic() {
    let engine = SearchEngine::new().unwrap();
    let results = engine.exact_search("Thing", 5).unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.path, "Thing");
    assert_eq!(r.kind, "exact_match");
    assert!(r.score >= 2.0);
}

#[test]
fn test_exact_search_empty() {
    let engine = SearchEngine::new().unwrap();
    let results = engine.exact_search("", 5).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_query_builder_no_terms() {
    let q = QueryBuilder::new().build_query_string();
    assert_eq!(q, "");
}

#[test]
fn test_query_builder_only_negatives() {
    let q = QueryBuilder::new()
        .must_not("foo")
        .must_not("bar")
        .build_query_string();
    // Order not strictly guaranteed, but both must appear with '-'
    assert!(q.contains("-foo"));
    assert!(q.contains("-bar"));
}

#[test]
fn test_filter_structure_defaults() {
    let filters = SearchFilters::default();
    assert!(filters.kinds.is_none());
    assert!(filters.modules.is_none());
    assert!(filters.visibility.is_none());
    assert!(filters.has_docs.is_none());
    assert!(filters.exclude_deprecated);
}
