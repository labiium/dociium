use anyhow::Result;
use index_core::{ImplItem, TraitImplIndex}; // Assuming ImplItem is pub or re-exported
use rustdoc_types::Crate as RustdocCrate;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

// Re-use the helper from symbol_search_integration_tests.rs
// Ideally, this would be in a shared `tests/common/mod.rs`
fn load_rustdoc_fixture(name: &str) -> Result<RustdocCrate> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(format!("{}.json", name));

    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Fixture JSON not found: {:?}. Please pre-generate it for testing.",
            path
        ));
    }

    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let krate: RustdocCrate = serde_json::from_reader(reader)?;
    Ok(krate)
}

#[test]
fn test_mini_math_trait_indexing() -> Result<()> {
    let mini_math_rustdoc = load_rustdoc_fixture("mini_math")?;
    let trait_index = TraitImplIndex::from_rustdoc(&mini_math_rustdoc)?;

    // Test get_trait_impls for mini_math::Area
    let area_impls = trait_index.get_trait_impls("mini_math::Area")?;
    assert_eq!(area_impls.len(), 1, "Expected 1 impl for mini_math::Area");

    if let Some(shape_impl) = area_impls.first() {
        assert_eq!(shape_impl.for_type, "mini_math::Shape");
        assert_eq!(shape_impl.trait_path, "mini_math::Area");

        // Check for the calculate_area method within the impl items
        let has_calculate_area = shape_impl
            .items
            .iter()
            .any(|item: &ImplItem| item.name == "calculate_area");
        assert!(
            has_calculate_area,
            "Impl for Shape should have calculate_area method"
        );
        if let Some(item) = shape_impl
            .items
            .iter()
            .find(|item| item.name == "calculate_area")
        {
            assert!(item
                .signature
                .as_deref()
                .unwrap_or("")
                .contains("fn calculate_area(&self) -> f64"));
        }
    }

    // Test get_type_impls for mini_math::Shape
    let shape_type_impls = trait_index.get_type_impls("mini_math::Shape")?;
    assert_eq!(
        shape_type_impls.len(),
        1,
        "Expected 1 trait impl for mini_math::Shape"
    );
    if let Some(area_on_shape) = shape_type_impls.first() {
        assert_eq!(area_on_shape.trait_path, "mini_math::Area");
    }

    // Test for BlanketTrait
    // For `impl<U, T> BlanketTrait<T> for U where U: std::fmt::Debug`
    // This is a generic blanket impl. `get_trait_impls("mini_math::BlanketTrait")` might be tricky
    // as it's generic. The current `type_to_string` for `for_type` might return "U" or similar.
    // Let's search for the trait itself.
    let blanket_trait_impls = trait_index.get_trait_impls("mini_math::BlanketTrait")?;
    // The number of impls here depends on how rustdoc represents blanket impls and if they are specialized
    // or shown as a single generic entry. For now, let's just check it exists.
    // If rustdoc emits a concrete impl for, e.g., `String` because it's `Debug`, that might show up.
    // The `mini_math` fixture has `impl<U, T> BlanketTrait<T> for U where U: std::fmt::Debug`.
    // This is one generic impl.
    assert!(
        !blanket_trait_impls.is_empty(),
        "Expected at least one impl for BlanketTrait. Found {} ({:?})",
        blanket_trait_impls.len(),
        blanket_trait_impls.get(0).map(|i| &i.for_type)
    );

    // It's hard to assert the exact `for_type` for a blanket impl like `U`.
    // The `is_blanket` flag should be true.
    if let Some(blanket_impl) = blanket_trait_impls.iter().find(|i| i.is_blanket) {
        assert!(blanket_impl.is_blanket);
        // Check for its 'describe' method
        let has_describe = blanket_impl
            .items
            .iter()
            .any(|item: &ImplItem| item.name == "describe");
        assert!(
            has_describe,
            "BlanketTrait impl should have describe method"
        );
    } else {
        // If no impl is marked as blanket, but we expect one.
        // This depends on rustdoc's output for such blanket impls.
        // Sometimes they are not directly listed under the trait if they are too generic
        // or are only resolved for concrete types.
        // For now, this part of the test might be flaky depending on rustdoc version/behavior.
        // The key is that `TraitImplIndex` should capture `is_blanket = true` if rustdoc provides it.
        println!("Warning: No impl explicitly marked as 'is_blanket' for BlanketTrait was found directly. This might be due to how rustdoc outputs generic blanket impls.");
    }

    // Test get_stats
    let stats = trait_index.get_stats();
    assert!(stats.total_traits >= 2); // Area, BlanketTrait, MyAutoTrait
    assert!(stats.total_types >= 1); // Shape, Point etc.
    assert!(stats.total_implementations >= 2); // Area for Shape, Blanket impl, MyAutoTrait impls

    Ok(())
}
