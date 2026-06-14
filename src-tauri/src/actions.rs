#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use log::{debug, error, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tauri::Manager;
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Clone, serde::Serialize)]
struct RecordingErrorEvent {
    error_type: String,
    detail: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct LanguageGuardEvent {
    locked_language: String,
    preview: String,
}

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
fn build_system_prompt(prompt_template: &str) -> String {
    prompt_template.replace("${output}", "").trim().to_string()
}

fn validate_post_processed_text(transcription: &str, processed_text: &str) -> Result<(), String> {
    crate::adaptive::processor::validate_unrequested_translation(transcription, processed_text)
}

fn copy_text_to_clipboard(app: &AppHandle, text: &str, reason: &str) {
    if let Err(err) = app.clipboard().write_text(text.to_string()) {
        error!("Failed to copy text to clipboard after {}: {}", reason, err);
    }
}

fn language_guard_receipt(target_verified: bool) -> crate::adaptive::types::InsertionReceipt {
    crate::adaptive::types::InsertionReceipt {
        attempted: false,
        succeeded: false,
        method: crate::adaptive::types::InsertionMethod::None,
        target_verified,
        error: Some("language guard blocked paste".to_string()),
    }
}

fn native_translation_allows_language_guard_bypass(
    settings: &AppSettings,
    model_supports_translation: bool,
) -> bool {
    model_supports_translation && settings.translate_to_english
}

fn selected_model_supports_translation(app: &AppHandle, settings: &AppSettings) -> bool {
    app.try_state::<Arc<crate::managers::model::ModelManager>>()
        .and_then(|model_manager| model_manager.get_model_info(&settings.selected_model))
        .map(|info| info.supports_translation)
        .unwrap_or(false)
}

fn language_guard_blocks(app: &AppHandle, settings: &AppSettings, final_text: &str) -> bool {
    if native_translation_allows_language_guard_bypass(
        settings,
        selected_model_supports_translation(app, settings),
    ) {
        return false;
    }

    if !crate::adaptive::language_guard::contradicts_locked_language(
        &settings.selected_language,
        final_text,
    ) {
        return false;
    }

    warn!(
        "Language guard blocked paste because output script contradicts locked language '{}'",
        settings.selected_language
    );
    copy_text_to_clipboard(app, final_text, "language guard block");

    let preview = final_text.chars().take(80).collect();
    let _ = app.emit(
        "language-guard-blocked",
        LanguageGuardEvent {
            locked_language: settings.selected_language.clone(),
            preview,
        },
    );

    true
}

fn accept_post_processed_text(
    transcription: &str,
    processed_text: String,
    provider_id: &str,
) -> Option<String> {
    match validate_post_processed_text(transcription, &processed_text) {
        Ok(()) => Some(processed_text),
        Err(err) => {
            warn!(
                "Post-processing output rejected for provider '{}': {}. Falling back to raw transcript.",
                provider_id, err
            );
            None
        }
    }
}

fn is_local_post_process_provider(base_url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(base_url.trim()) else {
        return false;
    };

    if parsed.scheme() == "apple-intelligence" {
        return true;
    }

    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }

    matches!(
        parsed.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    )
}

fn can_egress_post_process_text(provider_base_url: &str, api_key: &str) -> bool {
    is_local_post_process_provider(provider_base_url) || !api_key.trim().is_empty()
}

async fn post_process_transcription(settings: &AppSettings, transcription: &str) -> Option<String> {
    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Post-processing enabled but no provider is selected");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if model.trim().is_empty() {
        debug!(
            "Post-processing skipped because provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let selected_prompt_id = match &settings.post_process_selected_prompt_id {
        Some(id) => id.clone(),
        None => {
            debug!("Post-processing skipped because no prompt is selected");
            return None;
        }
    };

    let prompt = match settings
        .post_process_prompts
        .iter()
        .find(|prompt| prompt.id == selected_prompt_id)
    {
        Some(prompt) => prompt.prompt.clone(),
        None => {
            debug!(
                "Post-processing skipped because prompt '{}' was not found",
                selected_prompt_id
            );
            return None;
        }
    };

    if prompt.trim().is_empty() {
        debug!("Post-processing skipped because the selected prompt is empty");
        return None;
    }

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if !can_egress_post_process_text(&provider.base_url, &api_key) {
        warn!(
            "Post-processing skipped for provider '{}' because remote providers require a configured API key before transcript egress",
            provider.id
        );
        return None;
    }

    // Disable reasoning for providers where post-processing rarely benefits from it.
    // - custom: top-level reasoning_effort (works for local OpenAI-compat servers)
    // - openrouter: nested reasoning object; exclude:true also keeps reasoning text
    //   out of the response so it can't pollute structured-output JSON parsing
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };

    if provider.supports_structured_output {
        debug!("Using structured outputs for provider '{}'", provider.id);

        let system_prompt = build_system_prompt(&prompt);
        let user_content = transcription.to_string();

        // Handle Apple Intelligence separately since it uses native Swift APIs
        if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                if !apple_intelligence::check_apple_intelligence_availability() {
                    debug!(
                        "Apple Intelligence selected but not currently available on this device"
                    );
                    return None;
                }

                let token_limit = model.trim().parse::<i32>().unwrap_or(0);
                return match apple_intelligence::process_text_with_system_prompt(
                    &system_prompt,
                    &user_content,
                    token_limit,
                ) {
                    Ok(result) => {
                        if result.trim().is_empty() {
                            debug!("Apple Intelligence returned an empty response");
                            None
                        } else {
                            let result = strip_invisible_chars(&result);
                            debug!(
                                "Apple Intelligence post-processing succeeded. Output length: {} chars",
                                result.len()
                            );
                            accept_post_processed_text(transcription, result, &provider.id)
                        }
                    }
                    Err(err) => {
                        error!("Apple Intelligence post-processing failed: {}", err);
                        None
                    }
                };
            }

            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                debug!("Apple Intelligence provider selected on unsupported platform");
                return None;
            }
        }

        // Define JSON schema for transcription output
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        match crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model,
            user_content,
            Some(system_prompt),
            Some(json_schema),
            reasoning_effort.clone(),
            reasoning.clone(),
        )
        .await
        {
            Ok(Some(content)) => {
                // Parse the JSON response to extract the transcription field
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(json) => {
                        if let Some(transcription_value) =
                            json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                        {
                            let result = strip_invisible_chars(transcription_value);
                            debug!(
                                "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                                provider.id,
                                result.len()
                            );
                            return accept_post_processed_text(transcription, result, &provider.id);
                        } else {
                            error!("Structured output response missing 'transcription' field");
                            return accept_post_processed_text(
                                transcription,
                                strip_invisible_chars(&content),
                                &provider.id,
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse structured output JSON: {}. Returning raw content.",
                            e
                        );
                        return accept_post_processed_text(
                            transcription,
                            strip_invisible_chars(&content),
                            &provider.id,
                        );
                    }
                }
            }
            Ok(None) => {
                error!("LLM API response has no content");
                return None;
            }
            Err(e) => {
                warn!(
                    "Structured output failed for provider '{}': {}. Falling back to legacy mode.",
                    provider.id, e
                );
                // Fall through to legacy mode below
            }
        }
    }

    // Legacy mode: Replace ${output} variable in the prompt with the actual text
    let processed_prompt = prompt.replace("${output}", transcription);
    debug!("Processed prompt length: {} chars", processed_prompt.len());

    match crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        processed_prompt,
        reasoning_effort,
        reasoning,
    )
    .await
    {
        Ok(Some(content)) => {
            let content = strip_invisible_chars(&content);
            debug!(
                "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                provider.id,
                content.len()
            );
            accept_post_processed_text(transcription, content, &provider.id)
        }
        Ok(None) => {
            error!("LLM API response has no content");
            None
        }
        Err(e) => {
            error!(
                "LLM post-processing failed for provider '{}': {}. Falling back to original transcription.",
                provider.id,
                e
            );
            None
        }
    }
}

async fn maybe_convert_chinese_variant(
    settings: &AppSettings,
    transcription: &str,
) -> Option<String> {
    // Check if language is set to Simplified or Traditional Chinese
    let is_simplified = settings.selected_language == "zh-Hans";
    let is_traditional = settings.selected_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("selected_language is not Simplified or Traditional Chinese; skipping translation");
        return None;
    }

    debug!(
        "Starting Chinese translation using OpenCC for language: {}",
        settings.selected_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
) -> ProcessedTranscription {
    let settings = get_settings(app);
    let mut final_text = transcription.to_string();
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;

    if let Some(converted_text) = maybe_convert_chinese_variant(&settings, transcription).await {
        final_text = converted_text;
    }

    if post_process {
        if let Some(processed_text) = post_process_transcription(&settings, &final_text).await {
            post_processed_text = Some(processed_text.clone());
            final_text = processed_text;

            if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                if let Some(prompt) = settings
                    .post_process_prompts
                    .iter()
                    .find(|prompt| &prompt.id == prompt_id)
                {
                    post_process_prompt = Some(prompt.prompt.clone());
                }
            }
        }
    } else if final_text != transcription {
        post_processed_text = Some(final_text.clone());
    }

    ProcessedTranscription {
        final_text,
        post_processed_text,
        post_process_prompt,
    }
}

fn serialize_json<T: serde::Serialize>(value: &T) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn should_mute_before_start_feedback(settings: &AppSettings) -> bool {
    settings.mute_while_recording && !settings.audio_feedback
}

fn adaptive_target_verified(
    original_context: &crate::adaptive::types::CapturedContext,
    current_context: &crate::adaptive::types::CapturedContext,
) -> bool {
    original_context.target_fingerprint.is_none()
        || current_context.target_fingerprint == original_context.target_fingerprint
}

fn skipped_wrong_target_receipt() -> crate::adaptive::types::InsertionReceipt {
    crate::adaptive::types::InsertionReceipt {
        attempted: false,
        succeeded: false,
        method: crate::adaptive::types::InsertionMethod::None,
        target_verified: false,
        error: Some("target changed before insertion".to_string()),
    }
}

pub(crate) async fn process_adaptive_transcription_output(
    settings: &AppSettings,
    transcription: &str,
    context: crate::adaptive::types::CapturedContext,
    shortcut: crate::adaptive::types::ShortcutIntent,
) -> crate::adaptive::types::AdaptiveProcessResult {
    let pre_route = crate::adaptive::routing::route_before_recording(
        &settings.adaptive_profiles,
        shortcut,
        &context,
        &settings.adaptive_language_shortlist,
        &settings.adaptive_default_profile_id,
    );
    let language = crate::adaptive::language::analyze_language(
        transcription,
        &settings.adaptive_language_shortlist,
    );
    let routing = crate::adaptive::routing::route_after_transcription(
        &settings.adaptive_profiles,
        pre_route,
        &context,
        &language,
        None,
    );
    let profile = crate::adaptive::profile::find_profile_or_default(
        &settings.adaptive_profiles,
        &routing.profile_id,
    );

    let final_text = crate::adaptive::processor::deterministic_process(transcription, profile);
    let final_text = match crate::adaptive::processor::validate_output(
        transcription,
        &final_text,
        profile,
    ) {
        Ok(()) => final_text,
        Err(err) => {
            warn!(
                    "Adaptive processing failed validation for profile '{}': {}. Falling back to raw transcript.",
                    profile.id, err
                );
            transcription.to_string()
        }
    };
    let post_process_prompt =
        crate::adaptive::processor::build_profile_prompt(transcription, profile);
    let post_processed_text = if final_text == transcription {
        None
    } else {
        Some(final_text.clone())
    };

    crate::adaptive::types::AdaptiveProcessResult {
        final_text,
        post_processed_text,
        post_process_prompt,
        language,
        routing,
    }
}

#[cfg(test)]
mod adaptive_action_tests {
    use super::*;
    use crate::adaptive::types::{CapturedContext, InsertionMethod, InsertionReceipt, TargetKind};

    fn context_with_fingerprint(fingerprint: Option<&str>) -> CapturedContext {
        CapturedContext {
            captured_at_ms: 0,
            process_name: fingerprint.map(ToString::to_string),
            window_title: None,
            window_title_hash: None,
            window_class: None,
            target_kind: TargetKind::Unknown,
            target_fingerprint: fingerprint.map(ToString::to_string),
            is_sensitive: false,
        }
    }

    #[test]
    fn serialize_json_returns_some_for_receipt() {
        let receipt = InsertionReceipt {
            attempted: true,
            succeeded: true,
            method: InsertionMethod::Direct,
            target_verified: true,
            error: None,
        };
        assert!(serialize_json(&receipt).unwrap().contains("succeeded"));
    }

    #[test]
    fn adaptive_target_verification_allows_matching_fingerprint() {
        assert!(adaptive_target_verified(
            &context_with_fingerprint(Some("notepad|edit")),
            &context_with_fingerprint(Some("notepad|edit"))
        ));
    }

    #[test]
    fn adaptive_target_verification_rejects_changed_fingerprint() {
        assert!(!adaptive_target_verified(
            &context_with_fingerprint(Some("outlook|richedit")),
            &context_with_fingerprint(Some("notepad|edit"))
        ));
    }

    #[test]
    fn adaptive_target_verification_allows_unknown_original_target() {
        assert!(adaptive_target_verified(
            &context_with_fingerprint(None),
            &context_with_fingerprint(Some("notepad|edit"))
        ));
    }

    #[test]
    fn wrong_target_receipt_is_not_attempted() {
        let receipt = skipped_wrong_target_receipt();
        assert!(!receipt.attempted);
        assert!(!receipt.succeeded);
        assert!(!receipt.target_verified);
        assert_eq!(receipt.method, InsertionMethod::None);
        assert_eq!(
            receipt.error.as_deref(),
            Some("target changed before insertion")
        );
    }

    #[test]
    fn mute_happens_before_start_feedback_when_feedback_is_disabled() {
        let mut settings = AppSettings {
            mute_while_recording: true,
            audio_feedback: false,
            ..crate::settings::get_default_settings()
        };

        assert!(should_mute_before_start_feedback(&settings));

        settings.audio_feedback = true;
        assert!(!should_mute_before_start_feedback(&settings));

        settings.mute_while_recording = false;
        assert!(!should_mute_before_start_feedback(&settings));
    }

    #[test]
    fn post_processing_rejects_english_to_arabic_translation() {
        let validation =
            validate_post_processed_text("Please send the file today", "يرجى إرسال الملف اليوم");
        assert!(validation.is_err());
    }

    #[test]
    fn post_processing_accepts_same_language_cleanup() {
        let validation = validate_post_processed_text(
            "please send the file today",
            "Please send the file today.",
        );
        assert!(validation.is_ok());
    }

    #[test]
    fn local_post_process_providers_do_not_require_api_key() {
        assert!(can_egress_post_process_text(
            "http://localhost:11434/v1",
            ""
        ));
        assert!(can_egress_post_process_text(
            "https://127.0.0.1:8080/v1",
            "   "
        ));
        assert!(can_egress_post_process_text("http://[::1]:11434/v1", ""));
        assert!(can_egress_post_process_text(
            "apple-intelligence://local",
            ""
        ));
    }

    #[test]
    fn remote_post_process_providers_require_api_key() {
        assert!(!can_egress_post_process_text(
            "https://api.openai.com/v1",
            ""
        ));
        assert!(!can_egress_post_process_text(
            "https://openrouter.ai/api/v1",
            "   "
        ));
        assert!(can_egress_post_process_text(
            "https://api.openai.com/v1",
            "sk-test"
        ));
    }

    #[test]
    fn localhost_lookalike_post_process_providers_require_api_key() {
        for base_url in [
            "http://localhost@evil.com/v1",
            "http://localhost.evil.com/v1",
            "https://127.0.0.1.evil.com/v1",
        ] {
            assert!(
                !can_egress_post_process_text(base_url, ""),
                "{base_url} must not be treated as a local provider"
            );
        }
    }

    #[test]
    fn unsupported_model_translation_setting_does_not_bypass_language_guard() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_language = "ar".to_string();
        settings.translation_enabled = true;
        settings.translate_to_english = true;

        assert!(!native_translation_allows_language_guard_bypass(
            &settings, false
        ));
    }

    #[test]
    fn supported_model_translation_setting_bypasses_language_guard() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_language = "ar".to_string();
        settings.translation_enabled = true;
        settings.translate_to_english = true;

        assert!(native_translation_allows_language_guard_bypass(
            &settings, true
        ));
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Load ASR model and VAD model in parallel
        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });

        let binding_id = binding_id.to_string();
        change_tray_icon(app, TrayIconState::Recording);
        show_recording_overlay(app);

        // Get the microphone mode to determine audio feedback timing
        let settings = get_settings(app);
        if settings.adaptive_profiles_enabled {
            if let Some(store) = app.try_state::<crate::adaptive::session::ActiveDictationContext>()
            {
                let context = crate::adaptive::context::capture_context(
                    &settings.adaptive_private_app_patterns,
                );
                store.insert(&binding_id, context);
            }
        }
        let is_always_on = settings.always_on_microphone;
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_error: Option<String> = None;
        if is_always_on {
            if let Err(e) = rm.try_start_recording(&binding_id) {
                debug!("Recording failed: {}", e);
                recording_error = Some(e);
            } else if should_mute_before_start_feedback(&settings) {
                rm.apply_mute();
            } else if settings.audio_feedback || settings.mute_while_recording {
                // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
                debug!("Always-on mode: Playing audio feedback immediately");
                let rm_clone = Arc::clone(&rm);
                let app_clone = app.clone();
                std::thread::spawn(move || {
                    play_feedback_sound_blocking(&app_clone, SoundType::Start);
                    rm_clone.apply_mute();
                });
            }
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            match rm.try_start_recording(&binding_id) {
                Ok(()) => {
                    debug!("Recording started in {:?}", recording_start_time.elapsed());
                    if should_mute_before_start_feedback(&settings) {
                        rm.apply_mute();
                    } else if settings.audio_feedback || settings.mute_while_recording {
                        // Small delay to ensure microphone stream is active before feedback playback
                        let app_clone = app.clone();
                        let rm_clone = Arc::clone(&rm);
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            debug!("Handling delayed audio feedback/mute sequence");
                            play_feedback_sound_blocking(&app_clone, SoundType::Start);
                            rm_clone.apply_mute();
                        });
                    }
                }
                Err(e) => {
                    debug!("Failed to start recording: {}", e);
                    recording_error = Some(e);
                }
            }
        }

        if recording_error.is_none() {
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        } else {
            // Starting failed (for example due to blocked microphone permissions).
            // Revert UI state so we don't stay stuck in the recording overlay.
            if let Some(store) = app.try_state::<crate::adaptive::session::ActiveDictationContext>()
            {
                store.clear(&binding_id);
            }
            utils::hide_recording_overlay(app);
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        show_transcribing_overlay(app);

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let post_process = self.post_process;

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("verbatim-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save
                    let transcription_time = Instant::now();
                    let transcription_result = tm.transcribe(samples);

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "Transcription completed in {:?}: '{}'",
                                transcription_time.elapsed(),
                                transcription
                            );

                            let settings = get_settings(&ah);
                            let adaptive_context = if settings.adaptive_profiles_enabled {
                                ah.try_state::<crate::adaptive::session::ActiveDictationContext>()
                                    .and_then(|store| store.take(&binding_id))
                            } else {
                                None
                            };

                            if post_process || adaptive_context.is_some() {
                                show_processing_overlay(&ah);
                            }

                            if let Some(context) = adaptive_context {
                                let processed = process_adaptive_transcription_output(
                                    &settings,
                                    &transcription,
                                    context.clone(),
                                    crate::adaptive::types::ShortcutIntent::Default,
                                )
                                .await;
                                let profile = crate::adaptive::profile::find_profile_or_default(
                                    &settings.adaptive_profiles,
                                    &processed.routing.profile_id,
                                );

                                let saved_entry_id = if wav_saved {
                                    let metadata =
                                        crate::managers::history::AdaptiveHistoryMetadata {
                                            profile_id: Some(profile.id.clone()),
                                            profile_name: Some(profile.name.clone()),
                                            routing_json: serialize_json(&processed.routing),
                                            context_json: serialize_json(&context),
                                            language_json: serialize_json(&processed.language),
                                            insertion_json: None,
                                            parent_entry_id: None,
                                        };
                                    match hm.save_entry_with_metadata(
                                        file_name,
                                        transcription,
                                        post_process,
                                        processed.post_processed_text.clone(),
                                        processed.post_process_prompt.clone(),
                                        metadata,
                                    ) {
                                        Ok(entry) => Some(entry.id),
                                        Err(err) => {
                                            error!(
                                                "Failed to save adaptive history entry: {}",
                                                err
                                            );
                                            None
                                        }
                                    }
                                } else {
                                    None
                                };

                                if processed.final_text.is_empty() {
                                    utils::hide_recording_overlay(&ah);
                                    change_tray_icon(&ah, TrayIconState::Idle);
                                } else {
                                    let ah_clone = ah.clone();
                                    let hm_for_receipt = Arc::clone(&hm);
                                    let paste_time = Instant::now();
                                    let final_text = processed.final_text;
                                    let original_context = context.clone();
                                    let private_patterns =
                                        settings.adaptive_private_app_patterns.clone();
                                    let settings_for_guard = settings.clone();
                                    ah.run_on_main_thread(move || {
                                        let current_context =
                                            crate::adaptive::context::capture_context(
                                                &private_patterns,
                                            );
                                        let target_verified = adaptive_target_verified(
                                            &original_context,
                                            &current_context,
                                        );
                                        let receipt = if target_verified {
                                            if language_guard_blocks(
                                                &ah_clone,
                                                &settings_for_guard,
                                                &final_text,
                                            ) {
                                                language_guard_receipt(true)
                                            } else {
                                                let receipt = utils::paste_with_receipt(
                                                    final_text.clone(),
                                                    ah_clone.clone(),
                                                    true,
                                                );
                                                if !receipt.succeeded && receipt.attempted {
                                                    copy_text_to_clipboard(
                                                        &ah_clone,
                                                        &final_text,
                                                        "adaptive paste failure",
                                                    );
                                                }
                                                receipt
                                            }
                                        } else {
                                            error!(
                                                "Adaptive paste skipped because the foreground target changed before insertion"
                                            );
                                            let _ = ah_clone.emit("paste-error", ());
                                            skipped_wrong_target_receipt()
                                        };
                                        if receipt.succeeded {
                                            debug!(
                                                "Text pasted successfully in {:?}",
                                                paste_time.elapsed()
                                            );
                                        } else {
                                            error!(
                                                "Failed to paste transcription: {:?}",
                                                receipt.error
                                            );
                                            let _ = ah_clone.emit("paste-error", ());
                                        }
                                        if let Some(entry_id) = saved_entry_id {
                                            if let Some(receipt_json) = serialize_json(&receipt) {
                                                if let Err(err) = hm_for_receipt
                                                    .update_insertion_receipt(
                                                        entry_id,
                                                        receipt_json,
                                                    )
                                                {
                                                    error!(
                                                        "Failed to update insertion receipt: {}",
                                                        err
                                                    );
                                                }
                                            }
                                        }
                                        utils::hide_recording_overlay(&ah_clone);
                                        change_tray_icon(&ah_clone, TrayIconState::Idle);
                                    })
                                    .unwrap_or_else(|e| {
                                        error!("Failed to run paste on main thread: {:?}", e);
                                        utils::hide_recording_overlay(&ah);
                                        change_tray_icon(&ah, TrayIconState::Idle);
                                    });
                                }
                            } else {
                                let processed =
                                    process_transcription_output(&ah, &transcription, post_process)
                                        .await;

                                // Save to history if WAV was saved
                                if wav_saved {
                                    if let Err(err) = hm.save_entry(
                                        file_name,
                                        transcription,
                                        post_process,
                                        processed.post_processed_text.clone(),
                                        processed.post_process_prompt.clone(),
                                    ) {
                                        error!("Failed to save history entry: {}", err);
                                    }
                                }

                                if processed.final_text.is_empty() {
                                    utils::hide_recording_overlay(&ah);
                                    change_tray_icon(&ah, TrayIconState::Idle);
                                } else {
                                    let ah_clone = ah.clone();
                                    let paste_time = Instant::now();
                                    let final_text = processed.final_text;
                                    let settings_for_guard = settings.clone();
                                    ah.run_on_main_thread(move || {
                                        if !language_guard_blocks(
                                            &ah_clone,
                                            &settings_for_guard,
                                            &final_text,
                                        ) {
                                            match utils::paste(final_text.clone(), ah_clone.clone())
                                            {
                                                Ok(()) => debug!(
                                                    "Text pasted successfully in {:?}",
                                                    paste_time.elapsed()
                                                ),
                                                Err(e) => {
                                                    error!("Failed to paste transcription: {}", e);
                                                    copy_text_to_clipboard(
                                                        &ah_clone,
                                                        &final_text,
                                                        "paste failure",
                                                    );
                                                    let _ = ah_clone.emit("paste-error", ());
                                                }
                                            }
                                        }
                                        utils::hide_recording_overlay(&ah_clone);
                                        change_tray_icon(&ah_clone, TrayIconState::Idle);
                                    })
                                    .unwrap_or_else(|e| {
                                        error!("Failed to run paste on main thread: {:?}", e);
                                        utils::hide_recording_overlay(&ah);
                                        change_tray_icon(&ah, TrayIconState::Idle);
                                    });
                                }
                            }
                        }
                        Err(err) => {
                            debug!("Global Shortcut Transcription error: {}", err);
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    post_process,
                                    None,
                                    None,
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            utils::hide_recording_overlay(&ah);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                utils::hide_recording_overlay(&ah);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
        if let Some(store) = app.try_state::<crate::adaptive::session::ActiveDictationContext>() {
            store.clear(binding_id);
        }
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction { post_process: true }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});
