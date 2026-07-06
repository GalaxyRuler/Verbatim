//! Regenerates `src/bindings.ts` from the specta command/event registry without
//! launching the Tauri app. Run with:
//!   cargo test --manifest-path src-tauri/Cargo.toml --test export_bindings -- --ignored
//!
//! Ignored by default so normal `cargo test` runs don't rewrite the checked-in
//! bindings file as a side effect.
#[test]
#[ignore]
fn export_bindings() {
    use specta_typescript::{BigIntExportBehavior, Typescript};

    verbatim_app_lib::specta_builder()
        .export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");
}
