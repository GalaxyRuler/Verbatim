use specta_typescript::{BigIntExportBehavior, Typescript};

fn main() {
    let output_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../src/bindings.ts".to_string());

    verbatim_app_lib::specta_builder()
        .export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            output_path,
        )
        .expect("failed to export TypeScript bindings");
}
