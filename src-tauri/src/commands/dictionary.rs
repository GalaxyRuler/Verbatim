use tauri::AppHandle;

#[tauri::command]
#[specta::specta]
pub fn learn_custom_words_from_correction(
    app: AppHandle,
    dictated_text: String,
    corrected_text: String,
) -> Result<Vec<String>, String> {
    let mut settings = crate::settings::get_settings(&app);
    let candidates = crate::dictionary_learning::infer_auto_learn_candidates(
        &dictated_text,
        &corrected_text,
        &settings.custom_words,
    );

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let previous_count = settings.custom_words.len();
    let merged = crate::dictionary_learning::merge_auto_learn_candidates(
        &settings.custom_words,
        &candidates,
    );
    let learned_words = merged
        .iter()
        .skip(previous_count)
        .cloned()
        .collect::<Vec<_>>();

    if learned_words.is_empty() {
        return Ok(Vec::new());
    }

    settings.custom_words = merged;
    crate::settings::write_settings(&app, settings);

    Ok(learned_words)
}
