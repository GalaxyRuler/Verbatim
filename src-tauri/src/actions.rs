#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error};
use crate::dictation_transaction::{
    classify_final_text, classify_recording_stop, DictationTransactionTerminal, FinalTextDecision,
    RecordingStopDecision,
};
use crate::managers::audio::{
    is_selected_microphone_unavailable_error, AudioRecordingManager, RecordingStopResult,
};
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::operation_cancellation::{OperationCancellationState, OperationToken};
use crate::overlay::OverlayState;
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};
use crate::shortcut;
use crate::transform_mode::TransformAction;
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

const WHISPER_SAMPLE_RATE: usize = 16_000;
const MIN_OBSERVED_ACTIVE_SIGNAL_SAMPLES: usize = WHISPER_SAMPLE_RATE * 3 / 10;
const MIN_UNOBSERVED_USABLE_SPEECH_SAMPLES: usize = WHISPER_SAMPLE_RATE * 3 / 2;
const MIN_UNOBSERVED_SPEECH_RMS: f32 = 0.003;
const MIN_UNOBSERVED_SPEECH_PEAK: f32 = 0.02;

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
struct FinishGuard(AppHandle, u64);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished(self.1);
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str, generation: u64);
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
}

struct TransformShortcutAction {
    action: TransformAction,
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
fn build_system_prompt(prompt_template: &str) -> String {
    prompt_template.replace("${output}", "").trim().to_string()
}

fn validate_post_processed_text(transcription: &str, processed_text: &str) -> Result<(), String> {
    crate::text_processing::validate_preserved_text(transcription, processed_text)
        .map_err(|err| err.to_string())
}

fn copy_text_to_clipboard(app: &AppHandle, text: &str, reason: &str) {
    if let Err(err) = app.clipboard().write_text(text.to_string()) {
        error!("Failed to copy text to clipboard after {}: {}", reason, err);
    }
}

fn recording_has_usable_speech(result: &RecordingStopResult) -> bool {
    if result.device_error || result.samples.is_empty() || result.captured_sample_count == 0 {
        return false;
    }

    if result.observed_active_signal {
        return result.captured_sample_count >= MIN_OBSERVED_ACTIVE_SIGNAL_SAMPLES;
    }

    if result.captured_sample_count < MIN_UNOBSERVED_USABLE_SPEECH_SAMPLES {
        return false;
    }

    let sample_count = result.captured_sample_count.min(result.samples.len());
    let analyzed = &result.samples[..sample_count];
    let peak = analyzed
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    let rms =
        (analyzed.iter().map(|sample| sample * sample).sum::<f32>() / analyzed.len() as f32).sqrt();

    rms >= MIN_UNOBSERVED_SPEECH_RMS && peak >= MIN_UNOBSERVED_SPEECH_PEAK
}

fn transcription_completed_log_message(
    elapsed: std::time::Duration,
    transcription: &str,
) -> String {
    format!(
        "Transcription completed in {}ms ({} chars)",
        elapsed.as_millis(),
        transcription.chars().count()
    )
}

fn native_translation_allows_language_guard_bypass(
    settings: &AppSettings,
    model_supports_translation: bool,
) -> bool {
    crate::runtime_settings::dictation_runtime(settings, &[], model_supports_translation)
        .native_translation_to_english()
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

fn finish_dictation_transaction(app: &AppHandle, terminal: DictationTransactionTerminal) {
    let cleanup = terminal.cleanup_plan();
    if cleanup.hide_overlay {
        utils::hide_recording_overlay(app);
    }
    if cleanup.restore_idle_tray {
        change_tray_icon(app, TrayIconState::Idle);
    }
}

trait TranscriptionFailureContext {
    fn history_enabled(&self) -> bool;
    fn wav_saved(&self) -> bool;
    fn save_failed_history(&mut self);
    fn finish_transaction(&mut self, terminal: DictationTransactionTerminal);
}

fn finish_transcription_failed(context: &mut impl TranscriptionFailureContext) {
    let terminal = DictationTransactionTerminal::TranscriptionFailed;
    if context.history_enabled() && terminal.should_save_failed_history(context.wav_saved()) {
        context.save_failed_history();
    }
    context.finish_transaction(terminal);
}

struct ActionTranscriptionFailureContext<'a> {
    app: &'a AppHandle,
    history_manager: &'a HistoryManager,
    history_enabled: bool,
    wav_saved: bool,
    file_name: String,
    post_process: bool,
}

impl TranscriptionFailureContext for ActionTranscriptionFailureContext<'_> {
    fn history_enabled(&self) -> bool {
        self.history_enabled
    }

    fn wav_saved(&self) -> bool {
        self.wav_saved
    }

    fn save_failed_history(&mut self) {
        if let Err(save_err) = self.history_manager.save_entry(
            std::mem::take(&mut self.file_name),
            String::new(),
            self.post_process,
            None,
            None,
        ) {
            error!("Failed to save failed history entry: {}", save_err);
        }
    }

    fn finish_transaction(&mut self, terminal: DictationTransactionTerminal) {
        finish_dictation_transaction(self.app, terminal);
    }
}

fn current_or_new_operation_token(app: &AppHandle) -> Option<OperationToken> {
    app.try_state::<OperationCancellationState>().map(|state| {
        state
            .current_token()
            .unwrap_or_else(|| state.begin_operation())
    })
}

fn operation_is_cancelled(app: &AppHandle, token: Option<&OperationToken>) -> bool {
    let Some(token) = token else {
        return false;
    };

    app.try_state::<OperationCancellationState>().map_or_else(
        || token.provider_cancellation().is_cancelled(),
        |state| state.is_cancelled(token),
    )
}

fn finish_cancelled_operation(app: &AppHandle) {
    utils::emit_overlay_state_changed(app, OverlayState::Cancelled);
    finish_dictation_transaction(app, DictationTransactionTerminal::Cancelled);
}

fn cleanup_cancelled_wav(wav_path: &std::path::Path) {
    if wav_path.exists() {
        if let Err(err) = std::fs::remove_file(wav_path) {
            error!(
                "Failed to delete WAV for cancelled operation at {}: {}",
                wav_path.display(),
                err
            );
        }
    }
}

fn cleanup_cancelled_recording(
    history_manager: Arc<HistoryManager>,
    saved_entry_id: Option<i64>,
    wav_path: Option<std::path::PathBuf>,
) {
    if let Some(entry_id) = saved_entry_id {
        tauri::async_runtime::spawn(async move {
            if let Err(err) = history_manager.delete_entry(entry_id).await {
                error!(
                    "Failed to delete history entry {} for cancelled operation: {}",
                    entry_id, err
                );
            }
        });
    } else if let Some(wav_path) = wav_path {
        cleanup_cancelled_wav(&wav_path);
    }
}

struct AdaptiveInsertionRequest {
    app: AppHandle,
    history_manager: Arc<HistoryManager>,
    settings: AppSettings,
    final_text: String,
    context: crate::adaptive::types::CapturedContext,
    saved_entry_id: Option<i64>,
    cancelled_wav_path: Option<std::path::PathBuf>,
    operation_token: Option<OperationToken>,
    paste_started_at: Instant,
}

fn complete_adaptive_insertion(request: AdaptiveInsertionRequest) {
    let AdaptiveInsertionRequest {
        app,
        history_manager,
        settings,
        final_text,
        context,
        saved_entry_id,
        cancelled_wav_path,
        operation_token,
        paste_started_at,
    } = request;

    if operation_is_cancelled(&app, operation_token.as_ref()) {
        cleanup_cancelled_recording(
            Arc::clone(&history_manager),
            saved_entry_id,
            cancelled_wav_path,
        );
        finish_cancelled_operation(&app);
        return;
    }

    let verify_adaptive_target =
        should_verify_adaptive_target(&context) && should_capture_adaptive_context(&settings);
    let target_verified = if verify_adaptive_target {
        let context_runtime = crate::runtime_settings::context_runtime(&settings);
        let current_context = crate::adaptive::context::capture_context(
            context_runtime.private_app_patterns(),
            context_runtime.should_capture_nearby_text(),
        );
        adaptive_target_verified(&context, &current_context)
    } else {
        true
    };
    let expected_target = if target_verified && verify_adaptive_target {
        context.target_fingerprint.clone()
    } else {
        None
    };
    let attempt = if target_verified {
        if language_guard_blocks(&app, &settings, &final_text) {
            crate::insertion::InsertionAttempt::adaptive_guard_blocked()
        } else {
            let paste_text = prepare_adaptive_paste_text(&final_text, &context);
            force_ltr_input_direction_before_paste(&app, &final_text, &context);
            crate::insertion::InsertionAttempt::adaptive_ready(paste_text)
                .with_expected_target(expected_target)
        }
    } else {
        error!("Adaptive paste skipped because the foreground target changed before insertion");
        crate::insertion::InsertionAttempt::adaptive_target_changed()
    };

    let mut insertion_transaction = crate::insertion::InsertionTransaction::new(|request| {
        let cancellation_check = || operation_is_cancelled(&app, operation_token.as_ref());
        utils::paste_with_receipt_with_auto_learn_and_cancellation(
            request.text,
            app.clone(),
            request.target_verified,
            request.expected_target,
            request.auto_learn_eligible,
            Some(&cancellation_check),
        )
    });
    let outcome = insertion_transaction.run(attempt);
    if let Some(recovery) = &outcome.recovery_copy {
        copy_text_to_clipboard(&app, &recovery.text, recovery.reason);
    }
    let recovery_event = outcome.paste_recovery_event();
    let receipt = outcome.receipt;

    if outcome.emit_inserted {
        debug!(
            "Text pasted successfully in {:?}",
            paste_started_at.elapsed()
        );
        utils::emit_overlay_state_changed(&app, OverlayState::Inserted);
    }
    if outcome.emit_paste_error {
        error!(
            "Failed to paste transcription: {:?}",
            receipt.error.as_deref()
        );
        if let Some(recovery_event) = recovery_event {
            let _ = app.emit("paste-error", recovery_event);
        }
    }
    if let Some(entry_id) = saved_entry_id {
        if let Some(receipt_json) = serialize_json(&receipt) {
            if let Err(err) = history_manager.update_insertion_receipt(entry_id, receipt_json) {
                error!("Failed to update insertion receipt: {}", err);
            }
        }
    }
    finish_dictation_transaction(&app, DictationTransactionTerminal::InsertionCompleted);
}

fn complete_classic_insertion(
    app: AppHandle,
    history_manager: Arc<HistoryManager>,
    settings: AppSettings,
    final_text: String,
    saved_entry_id: Option<i64>,
    cancelled_wav_path: Option<std::path::PathBuf>,
    operation_token: Option<OperationToken>,
    context: Option<crate::adaptive::types::CapturedContext>,
    paste_started_at: Instant,
) {
    if operation_is_cancelled(&app, operation_token.as_ref()) {
        cleanup_cancelled_recording(history_manager, saved_entry_id, cancelled_wav_path);
        finish_cancelled_operation(&app);
        return;
    }

    let verify_classic_target = should_verify_classic_target(&settings, context.as_ref());
    let target_verified = classic_target_verified(&settings, context.as_ref());
    let expected_target = if target_verified && verify_classic_target {
        context
            .as_ref()
            .and_then(|context| context.target_fingerprint.clone())
    } else {
        None
    };
    let attempt = if !target_verified {
        error!("Classic paste skipped because the foreground target changed before insertion");
        crate::insertion::InsertionAttempt::classic_target_changed()
    } else if language_guard_blocks(&app, &settings, &final_text) {
        crate::insertion::InsertionAttempt::classic_guard_blocked()
    } else {
        crate::insertion::InsertionAttempt::classic_ready(final_text)
            .with_expected_target(expected_target)
    };
    let mut insertion_transaction = crate::insertion::InsertionTransaction::new(|request| {
        let cancellation_check = || operation_is_cancelled(&app, operation_token.as_ref());
        utils::paste_with_receipt_with_auto_learn_and_cancellation(
            request.text,
            app.clone(),
            request.target_verified,
            request.expected_target,
            request.auto_learn_eligible,
            Some(&cancellation_check),
        )
    });
    let outcome = insertion_transaction.run(attempt);
    if let Some(recovery) = &outcome.recovery_copy {
        copy_text_to_clipboard(&app, &recovery.text, recovery.reason);
    }
    if outcome.emit_inserted {
        debug!(
            "Text pasted successfully in {:?}",
            paste_started_at.elapsed()
        );
        utils::emit_overlay_state_changed(&app, OverlayState::Inserted);
    }
    if outcome.emit_paste_error {
        error!(
            "Failed to paste transcription: {:?}",
            outcome.receipt.error.as_deref()
        );
        if let Some(recovery_event) = outcome.paste_recovery_event() {
            let _ = app.emit("paste-error", recovery_event);
        }
    }
    finish_dictation_transaction(&app, DictationTransactionTerminal::InsertionCompleted);
}

fn accept_post_processed_text(
    transcription: &str,
    processed_text: String,
    provider_id: &str,
) -> Option<String> {
    if crate::text_processing::looks_like_llm_noise(&processed_text) {
        warn!(
            "Post-processing output rejected for provider '{}': model envelope noise. Falling back to raw transcript.",
            provider_id
        );
        return None;
    }

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

fn accept_structured_post_processed_text(
    transcription: &str,
    content: &str,
    provider_id: &str,
) -> Option<String> {
    match crate::text_processing::extract_structured_text(content, TRANSCRIPTION_FIELD) {
        Ok(result) => {
            debug!(
                "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                provider_id,
                result.len()
            );
            accept_post_processed_text(transcription, result, provider_id)
        }
        Err(err) => {
            error!(
                "Structured output parse failed: {}. Falling back to raw transcript.",
                err
            );
            None
        }
    }
}

fn should_run_requested_post_processing(
    requested: bool,
    settings: &AppSettings,
    private_session_enabled: bool,
) -> bool {
    requested && settings.post_process_enabled && !private_session_enabled
}

pub(crate) fn dictation_storage_policy(
    settings: &AppSettings,
    private_session_enabled: bool,
) -> (bool, bool) {
    let history_enabled = settings.history_enabled && !private_session_enabled;
    let recordings_enabled = history_enabled && settings.recordings_enabled;
    (history_enabled, recordings_enabled)
}

async fn post_process_with_managed_local_llm(
    app: &AppHandle,
    settings: &crate::local_llm::LocalLlmSettings,
    transcription: &str,
    operation_token: Option<&OperationToken>,
) -> Option<String> {
    let Some(manager) = app.try_state::<Arc<crate::local_llm::download::LocalLlmManager>>() else {
        debug!("Managed local post-processing skipped because local LLM manager is unavailable");
        return None;
    };

    let endpoint = match manager.ensure_runtime(settings).await {
        Ok(endpoint) => endpoint,
        Err(err) => {
            warn!(
                "Managed local post-processing skipped because runtime is unavailable: {}",
                err
            );
            return None;
        }
    };

    debug!(
        "Starting managed local post-processing with model '{}'",
        endpoint.model_id
    );

    if operation_is_cancelled(app, operation_token) {
        debug!("Managed local post-processing skipped because operation was cancelled");
        return None;
    }

    let provider_cancellation = operation_token.map(|token| token.provider_cancellation());
    match crate::llm_client::send_chat_completion_with_schema_and_cancellation(
        &endpoint.provider,
        String::new(),
        &endpoint.model,
        transcription.to_string(),
        Some(crate::local_llm::runtime::local_post_processing_system_prompt()),
        None,
        None,
        None,
        provider_cancellation.as_ref(),
    )
    .await
    {
        Ok(Some(content)) => {
            let content = crate::text_processing::strip_invisible_chars(&content);
            debug!(
                "Managed local post-processing succeeded. Output length: {} chars",
                content.len()
            );
            accept_post_processed_text(
                transcription,
                content,
                crate::local_llm::runtime::VERBATIM_LOCAL_PROVIDER_ID,
            )
        }
        Ok(None) => {
            warn!("Managed local post-processing returned no content");
            None
        }
        Err(err) => {
            warn!(
                "Managed local post-processing failed: {}. Falling back to configured provider or raw transcript.",
                err
            );
            None
        }
    }
}

async fn post_process_transcription(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    operation_token: Option<&OperationToken>,
) -> Option<String> {
    let runtime = crate::runtime_settings::post_processing_runtime(settings, true);

    if runtime.uses_managed_local() {
        if let Some(processed_text) = post_process_with_managed_local_llm(
            app,
            &settings.local_llm,
            transcription,
            operation_token,
        )
        .await
        {
            return Some(processed_text);
        }
        return None;
    }

    let api_runtime = match runtime.api_provider() {
        Some(api_runtime) => api_runtime.clone(),
        None => {
            debug!(
                "Post-processing skipped by runtime settings: {:?}",
                runtime.skip_reason()
            );
            return None;
        }
    };

    let provider = api_runtime.provider;
    let model = api_runtime.model;
    let prompt = api_runtime.prompt.prompt;
    let api_key = api_runtime.api_key;

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    if operation_is_cancelled(app, operation_token) {
        debug!("Post-processing skipped because operation was cancelled before provider request");
        return None;
    }

    let provider_cancellation = operation_token.map(|token| token.provider_cancellation());

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
                            let result = crate::text_processing::strip_invisible_chars(&result);
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

        let request = crate::text_processing::TextProviderRequest::structured(
            user_content,
            Some(system_prompt),
            TRANSCRIPTION_FIELD,
            "The cleaned and processed transcription text",
        );

        match crate::text_processing::send_text_provider_request_with_cancellation(
            &provider,
            api_key.clone(),
            &model,
            request,
            provider_cancellation.as_ref(),
        )
        .await
        {
            Ok(Some(content)) => {
                return accept_structured_post_processed_text(
                    transcription,
                    &content,
                    &provider.id,
                );
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
    let request = crate::text_processing::TextProviderRequest::new(processed_prompt, None);

    if operation_is_cancelled(app, operation_token) {
        debug!("Post-processing skipped because operation was cancelled before legacy request");
        return None;
    }

    match crate::text_processing::send_text_provider_request_with_cancellation(
        &provider,
        api_key,
        &model,
        request,
        provider_cancellation.as_ref(),
    )
    .await
    {
        Ok(Some(content)) => {
            let content = crate::text_processing::strip_invisible_chars(&content);
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
    operation_token: Option<OperationToken>,
) -> ProcessedTranscription {
    let mut settings = get_settings(app);
    crate::credentials::hydrate_runtime_post_process_api_keys(app, &mut settings);
    let private_session_enabled = crate::private_session::is_enabled(app);
    let mut final_text = transcription.to_string();
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;

    if let Some(converted_text) = maybe_convert_chinese_variant(&settings, transcription).await {
        final_text = converted_text;
    }

    let formatted_text = crate::adaptive::smart_formatting::format_transcript(
        &final_text,
        settings.formatting_level,
    );
    if formatted_text != final_text {
        final_text = formatted_text;
        post_processed_text = Some(final_text.clone());
    }

    if should_run_requested_post_processing(post_process, &settings, private_session_enabled) {
        if let Some(processed_text) =
            post_process_transcription(app, &settings, &final_text, operation_token.as_ref()).await
        {
            if let Err(err) = validate_post_processed_text(transcription, &processed_text) {
                warn!(
                    "Post-processing output rejected against raw transcript: {}. Falling back to deterministic formatted transcript.",
                    err
                );
                return ProcessedTranscription {
                    final_text,
                    post_processed_text,
                    post_process_prompt,
                };
            }
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

fn prepare_adaptive_paste_text(
    final_text: &str,
    context: &crate::adaptive::types::CapturedContext,
) -> String {
    crate::adaptive::text_direction::stabilize_ltr_paste_text(final_text, &context.target_kind)
}

fn should_capture_adaptive_context(settings: &AppSettings) -> bool {
    crate::runtime_settings::context_runtime(settings).should_capture_context()
}

fn should_capture_target_context(settings: &AppSettings) -> bool {
    should_capture_adaptive_context(settings) || settings.context_awareness_enabled
}

fn should_check_target_privacy_exclusion(settings: &AppSettings) -> bool {
    !crate::runtime_settings::context_runtime(settings)
        .private_app_patterns()
        .is_empty()
}

fn should_capture_start_context(settings: &AppSettings) -> bool {
    should_capture_target_context(settings) || should_check_target_privacy_exclusion(settings)
}

fn target_privacy_exclusion_blocks_recording(
    context: &crate::adaptive::types::CapturedContext,
) -> bool {
    context.is_sensitive
}

fn should_verify_adaptive_target(context: &crate::adaptive::types::CapturedContext) -> bool {
    context.target_fingerprint.is_some()
}

fn should_verify_classic_target(
    settings: &AppSettings,
    context: Option<&crate::adaptive::types::CapturedContext>,
) -> bool {
    settings.context_awareness_enabled && context.is_some_and(should_verify_adaptive_target)
}

fn classic_target_verified(
    settings: &AppSettings,
    context: Option<&crate::adaptive::types::CapturedContext>,
) -> bool {
    let Some(context) = context else {
        return true;
    };
    if !should_verify_classic_target(settings, Some(context)) {
        return true;
    }

    let context_runtime = crate::runtime_settings::context_runtime(settings);
    let current_context = crate::adaptive::context::capture_context(
        context_runtime.private_app_patterns(),
        context_runtime.should_capture_nearby_text(),
    );
    adaptive_target_verified(context, &current_context)
}

#[cfg(target_os = "windows")]
fn force_ltr_input_direction_before_paste(
    app: &AppHandle,
    final_text: &str,
    context: &crate::adaptive::types::CapturedContext,
) {
    if !crate::adaptive::text_direction::should_stabilize_ltr_paste_text(
        final_text,
        &context.target_kind,
    ) {
        return;
    }

    let Some(enigo_state) = app.try_state::<crate::input::EnigoState>() else {
        return;
    };
    let Ok(mut enigo) = enigo_state.0.lock() else {
        warn!("Failed to lock Enigo before setting LTR reading order");
        return;
    };
    if let Err(err) = crate::input::send_ltr_reading_order(&mut enigo) {
        warn!("Failed to set LTR reading order before paste: {}", err);
    }
}

#[cfg(not(target_os = "windows"))]
fn force_ltr_input_direction_before_paste(
    _app: &AppHandle,
    _final_text: &str,
    _context: &crate::adaptive::types::CapturedContext,
) {
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

    let final_text = if settings.formatting_level == crate::settings::FormattingLevel::None {
        transcription.to_string()
    } else {
        crate::adaptive::processor::deterministic_process(transcription, profile)
    };
    let final_text = crate::adaptive::smart_formatting::format_transcript(
        &final_text,
        settings.formatting_level,
    );
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

    fn post_processing_settings_for_provider(base_url: &str, api_key: &str) -> AppSettings {
        let mut settings = crate::settings::get_default_settings();
        let provider_id = "test-provider".to_string();

        settings.post_process_enabled = true;
        settings.local_llm.enabled = false;
        settings.post_process_provider_id = provider_id.clone();
        settings.post_process_selected_prompt_id = Some("test-prompt".to_string());
        settings.post_process_prompts = vec![crate::settings::LLMPrompt {
            id: "test-prompt".to_string(),
            name: "Test Prompt".to_string(),
            prompt: "Clean up the dictated transcript without changing its meaning.".to_string(),
        }];
        settings
            .post_process_models
            .insert(provider_id.clone(), "test-model".to_string());
        settings
            .post_process_providers
            .push(crate::settings::PostProcessProvider {
                id: provider_id.clone(),
                label: "Test Provider".to_string(),
                base_url: base_url.to_string(),
                allow_base_url_edit: true,
                models_endpoint: None,
                supports_structured_output: true,
            });

        if !api_key.is_empty() {
            settings
                .post_process_api_keys
                .insert(provider_id, api_key.to_string());
        }

        settings
    }

    fn api_post_processing_is_available(base_url: &str, api_key: &str) -> bool {
        let settings = post_processing_settings_for_provider(base_url, api_key);
        crate::runtime_settings::post_processing_runtime(&settings, true)
            .api_provider()
            .is_some()
    }

    fn post_processing_skip_reason(
        base_url: &str,
        api_key: &str,
    ) -> Option<crate::runtime_settings::PostProcessingSkipReason> {
        let settings = post_processing_settings_for_provider(base_url, api_key);
        crate::runtime_settings::post_processing_runtime(&settings, true).skip_reason()
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
    fn transform_shortcuts_are_mapped_to_actions() {
        for id in [
            "transform_polish",
            "transform_make_concise",
            "transform_turn_into_list",
            "transform_translate",
            "transform_prompt_engineer",
        ] {
            assert!(ACTION_MAP.contains_key(id), "{id} should be actionable");
        }
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
    fn adaptive_target_recheck_requires_original_fingerprint() {
        assert!(!should_verify_adaptive_target(&context_with_fingerprint(
            None
        )));
        assert!(should_verify_adaptive_target(&context_with_fingerprint(
            Some("notepad|edit")
        )));
    }

    #[test]
    fn classic_target_recheck_requires_context_awareness_and_fingerprint() {
        let mut settings = crate::settings::get_default_settings();
        settings.context_awareness_enabled = false;

        assert!(!should_verify_classic_target(
            &settings,
            Some(&context_with_fingerprint(Some("notepad|edit")))
        ));

        settings.context_awareness_enabled = true;
        assert!(!should_verify_classic_target(
            &settings,
            Some(&context_with_fingerprint(None))
        ));
        assert!(should_verify_classic_target(
            &settings,
            Some(&context_with_fingerprint(Some("notepad|edit")))
        ));
    }

    #[test]
    fn adaptive_email_paste_text_gets_ltr_direction_marks() {
        let mut context = context_with_fingerprint(Some("outlook.exe|rctrl_renwnd32"));
        context.target_kind = TargetKind::Email;

        let result = prepare_adaptive_paste_text("Dear James,\n\nHow did you come?", &context);

        assert_eq!(
            result,
            "\u{200E}Dear James,\u{200E}\n\n\u{200E}How did you come?\u{200E}"
        );
    }

    #[test]
    fn adaptive_notes_paste_text_gets_ltr_direction_marks() {
        let mut context = context_with_fingerprint(Some("notepad.exe|notepad"));
        context.target_kind = TargetKind::Notes;

        let result = prepare_adaptive_paste_text("I like simple notes.", &context);

        assert_eq!(result, "\u{200E}I like simple notes.\u{200E}");
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
    fn managed_local_post_processing_rejects_script_loss() {
        let accepted = accept_post_processed_text(
            "meeting at two pm بخصوص التقرير النهائي",
            "Meeting at 2 PM regarding the final report.".to_string(),
            crate::local_llm::runtime::VERBATIM_LOCAL_PROVIDER_ID,
        );

        assert!(accepted.is_none());
    }

    #[test]
    fn managed_local_post_processing_rejects_excessive_expansion() {
        let accepted = accept_post_processed_text(
            "send the invoice",
            "send the invoice ".repeat(80),
            crate::local_llm::runtime::VERBATIM_LOCAL_PROVIDER_ID,
        );

        assert!(accepted.is_none());
    }

    #[test]
    fn managed_local_post_processing_rejects_short_source_term_loss() {
        let accepted = accept_post_processed_text(
            "email signature",
            "Regards,\nAbdullah".to_string(),
            crate::local_llm::runtime::VERBATIM_LOCAL_PROVIDER_ID,
        );

        assert!(accepted.is_none());
    }

    #[test]
    fn remote_post_processing_rejects_excessive_expansion() {
        let accepted = accept_post_processed_text(
            "send the invoice",
            "send the invoice ".repeat(80),
            "openai",
        );

        assert!(accepted.is_none());
    }

    #[test]
    fn remote_post_processing_rejects_short_source_term_loss() {
        let accepted = accept_post_processed_text(
            "email signature",
            "Regards,\nAbdullah".to_string(),
            "openai",
        );

        assert!(accepted.is_none());
    }

    #[test]
    fn remote_post_processing_rejects_llm_noise() {
        let accepted = accept_post_processed_text(
            "hello world",
            "Sure, here's the cleaned text: hello world".to_string(),
            "openai",
        );

        assert!(accepted.is_none());
    }

    #[test]
    fn structured_post_processing_parse_failure_falls_back_to_transcript() {
        let accepted = accept_structured_post_processed_text(
            "hello world",
            r#"{"message":"Sure, here's the cleaned text: hello world"}"#,
            "openai",
        );

        assert!(accepted.is_none());
    }

    #[derive(Default)]
    struct CountingTranscriptionFailureContext {
        failed_history_rows: usize,
        finished: Vec<DictationTransactionTerminal>,
    }

    impl TranscriptionFailureContext for CountingTranscriptionFailureContext {
        fn history_enabled(&self) -> bool {
            true
        }

        fn wav_saved(&self) -> bool {
            true
        }

        fn save_failed_history(&mut self) {
            self.failed_history_rows += 1;
        }

        fn finish_transaction(&mut self, terminal: DictationTransactionTerminal) {
            self.finished.push(terminal);
        }
    }

    fn failed_history_rows_for(
        transcription_result: Result<String, ()>,
        observed_active_signal: bool,
    ) -> usize {
        let mut context = CountingTranscriptionFailureContext::default();
        let terminal = match transcription_result {
            Err(()) => DictationTransactionTerminal::TranscriptionFailed,
            Ok(final_text) => match classify_final_text(final_text, observed_active_signal) {
                FinalTextDecision::Terminal(terminal) => terminal,
                FinalTextDecision::Continue(_) => DictationTransactionTerminal::InsertionCompleted,
            },
        };

        if terminal == DictationTransactionTerminal::TranscriptionFailed {
            finish_transcription_failed(&mut context);
        } else {
            context.finish_transaction(terminal);
        }

        assert_eq!(context.finished, vec![terminal]);
        context.failed_history_rows
    }

    #[test]
    fn failed_history_rows_are_exactly_once_for_failure_terminals() {
        assert_eq!(failed_history_rows_for(Err(()), false), 1);
        assert_eq!(failed_history_rows_for(Ok(String::new()), true), 1);
        assert_eq!(failed_history_rows_for(Ok(String::new()), false), 0);
    }

    #[test]
    fn transcription_completed_log_message_does_not_include_transcript_text() {
        let transcript = "Confidential dictated sentence.";
        let message =
            transcription_completed_log_message(std::time::Duration::from_millis(123), transcript);

        assert!(message.contains("123ms"));
        assert!(message.contains("31 chars"));
        assert!(!message.contains(transcript));
        assert!(!message.contains("Confidential"));
    }

    #[test]
    fn local_post_process_providers_do_not_require_api_key() {
        assert!(api_post_processing_is_available(
            "http://localhost:11434/v1",
            ""
        ));
        assert!(api_post_processing_is_available(
            "https://127.0.0.1:8080/v1",
            "   "
        ));
        assert!(api_post_processing_is_available(
            "http://[::1]:11434/v1",
            ""
        ));
        assert!(api_post_processing_is_available(
            "apple-intelligence://local",
            ""
        ));
    }

    #[test]
    fn remote_post_process_providers_require_api_key() {
        assert_eq!(
            post_processing_skip_reason("https://api.openai.com/v1", ""),
            Some(crate::runtime_settings::PostProcessingSkipReason::RemoteMissingApiKey)
        );
        assert_eq!(
            post_processing_skip_reason("https://openrouter.ai/api/v1", "   "),
            Some(crate::runtime_settings::PostProcessingSkipReason::RemoteMissingApiKey)
        );
        assert!(api_post_processing_is_available(
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
            assert_eq!(
                post_processing_skip_reason(base_url, ""),
                Some(crate::runtime_settings::PostProcessingSkipReason::RemoteMissingApiKey),
                "{base_url} must not be treated as a local provider",
            );
        }
    }

    #[test]
    fn disabled_post_processing_setting_blocks_requested_post_process_paths() {
        let mut settings = crate::settings::get_default_settings();
        settings.post_process_enabled = false;

        assert!(!should_run_requested_post_processing(
            true, &settings, false
        ));

        settings.post_process_enabled = true;
        assert!(should_run_requested_post_processing(true, &settings, false));
        assert!(!should_run_requested_post_processing(
            false, &settings, false
        ));
        assert!(!should_run_requested_post_processing(true, &settings, true));
    }

    #[test]
    fn private_session_disables_history_and_wav_storage_policy() {
        let mut settings = crate::settings::get_default_settings();

        assert_eq!(dictation_storage_policy(&settings, false), (true, true));
        assert_eq!(dictation_storage_policy(&settings, true), (false, false));

        settings.recordings_enabled = false;
        assert_eq!(dictation_storage_policy(&settings, false), (true, false));

        settings.history_enabled = false;
        settings.recordings_enabled = true;
        assert_eq!(dictation_storage_policy(&settings, false), (false, false));
    }

    #[test]
    fn local_post_processing_mode_does_not_attempt_api_fallback() {
        let mut settings =
            post_processing_settings_for_provider("https://api.openai.com/v1", "sk-test");

        settings.local_llm.enabled = false;
        let api_runtime = crate::runtime_settings::post_processing_runtime(&settings, true);
        assert!(api_runtime.api_provider().is_some());
        assert!(!api_runtime.uses_managed_local());

        settings.local_llm.enabled = true;
        let local_runtime = crate::runtime_settings::post_processing_runtime(&settings, true);
        assert!(local_runtime.uses_managed_local());
        assert!(local_runtime.api_provider().is_none());
    }

    #[test]
    fn unsupported_model_translation_setting_does_not_bypass_language_guard() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_language = "ar".to_string();
        settings.translation_enabled = true;
        settings.translate_to_english = true;
        settings.translation_request = Some(crate::settings::TranslationRequestSettings {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            route: crate::settings::TranslationRoute::Auto,
        });

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
        settings.translation_request = Some(crate::settings::TranslationRequestSettings {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            route: crate::settings::TranslationRoute::Auto,
        });

        assert!(native_translation_allows_language_guard_bypass(
            &settings, true
        ));
    }

    #[test]
    fn short_false_positive_without_active_signal_is_not_usable_speech() {
        let result = crate::managers::audio::RecordingStopResult {
            samples: vec![0.01; 20_000],
            captured_sample_count: 4_000,
            observed_active_signal: false,
            diagnostic_state: crate::managers::mic_diagnostics::MicDiagnosticState::Recording,
            device_error: false,
            vad_fallback: false,
        };

        assert!(!recording_has_usable_speech(&result));
    }

    #[test]
    fn very_short_tap_is_not_usable_even_with_active_signal() {
        let result = crate::managers::audio::RecordingStopResult {
            samples: vec![0.01; 20_000],
            captured_sample_count: 3_200,
            observed_active_signal: true,
            diagnostic_state: crate::managers::mic_diagnostics::MicDiagnosticState::Recording,
            device_error: false,
            vad_fallback: false,
        };

        assert!(!recording_has_usable_speech(&result));
    }

    #[test]
    fn short_recording_with_active_signal_is_usable_speech() {
        let result = crate::managers::audio::RecordingStopResult {
            samples: vec![0.01; 20_000],
            captured_sample_count: 8_000,
            observed_active_signal: true,
            diagnostic_state: crate::managers::mic_diagnostics::MicDiagnosticState::Recording,
            device_error: false,
            vad_fallback: false,
        };

        assert!(recording_has_usable_speech(&result));
    }

    #[test]
    fn recording_with_device_error_is_not_usable_speech() {
        let result = crate::managers::audio::RecordingStopResult {
            samples: vec![0.01; 20_000],
            captured_sample_count: 8_000,
            observed_active_signal: true,
            diagnostic_state: crate::managers::mic_diagnostics::MicDiagnosticState::MicFailed,
            device_error: true,
            vad_fallback: false,
        };

        assert!(!recording_has_usable_speech(&result));
    }

    #[test]
    fn observed_active_signal_keeps_long_enough_speech_usable() {
        let result = crate::managers::audio::RecordingStopResult {
            samples: vec![0.01; 24_000],
            captured_sample_count: 24_000,
            observed_active_signal: true,
            diagnostic_state: crate::managers::mic_diagnostics::MicDiagnosticState::Recording,
            device_error: false,
            vad_fallback: false,
        };

        assert!(recording_has_usable_speech(&result));
    }

    #[test]
    fn quiet_long_buffer_without_active_signal_is_not_usable_speech() {
        let result = crate::managers::audio::RecordingStopResult {
            samples: vec![0.0005; 24_000],
            captured_sample_count: 24_000,
            observed_active_signal: false,
            diagnostic_state: crate::managers::mic_diagnostics::MicDiagnosticState::Silence,
            device_error: false,
            vad_fallback: false,
        };

        assert!(!recording_has_usable_speech(&result));
    }

    #[test]
    fn energetic_buffer_without_level_observation_can_still_be_usable() {
        let mut samples = vec![0.0; 24_000];
        for sample in samples.iter_mut().take(24_000) {
            *sample = 0.04;
        }
        let result = crate::managers::audio::RecordingStopResult {
            samples,
            captured_sample_count: 24_000,
            observed_active_signal: false,
            diagnostic_state: crate::managers::mic_diagnostics::MicDiagnosticState::Recording,
            device_error: false,
            vad_fallback: false,
        };

        assert!(recording_has_usable_speech(&result));
    }

    #[test]
    fn adaptive_context_capture_requires_context_awareness() {
        let mut settings = crate::settings::get_default_settings();
        settings.adaptive_profiles_enabled = true;
        settings.context_awareness_enabled = false;

        assert!(!should_capture_adaptive_context(&settings));

        settings.context_awareness_enabled = true;
        assert!(should_capture_adaptive_context(&settings));
    }

    #[test]
    fn target_context_capture_allows_context_awareness_without_adaptive_profiles() {
        let mut settings = crate::settings::get_default_settings();
        settings.adaptive_profiles_enabled = false;
        settings.context_awareness_enabled = true;

        assert!(!should_capture_adaptive_context(&settings));
        assert!(should_capture_target_context(&settings));
    }

    #[test]
    fn target_privacy_exclusion_checks_default_private_patterns_without_context_awareness() {
        let mut settings = crate::settings::get_default_settings();
        settings.context_awareness_enabled = false;
        settings.adaptive_profiles_enabled = false;

        assert!(should_check_target_privacy_exclusion(&settings));
        assert!(should_capture_start_context(&settings));
        assert!(!should_capture_target_context(&settings));
    }

    #[test]
    fn target_privacy_exclusion_is_disabled_when_patterns_are_empty() {
        let mut settings = crate::settings::get_default_settings();
        settings.context_awareness_enabled = false;
        settings.adaptive_profiles_enabled = false;
        settings.adaptive_private_app_patterns.clear();

        assert!(!should_check_target_privacy_exclusion(&settings));
        assert!(!should_capture_start_context(&settings));
    }

    #[test]
    fn target_privacy_exclusion_blocks_sensitive_context() {
        let mut context = context_with_fingerprint(Some("bitwarden.exe|Chrome_WidgetWin_1"));
        context.is_sensitive = true;

        assert!(target_privacy_exclusion_blocks_recording(&context));

        context.is_sensitive = false;
        assert!(!target_privacy_exclusion_blocks_recording(&context));
    }

    #[test]
    fn adaptive_guard_block_outcome_is_not_attempted_and_keeps_feedback() {
        let attempt = crate::insertion::InsertionAttempt::adaptive_guard_blocked();

        let outcome = crate::insertion::resolve_insertion_attempt(attempt, |_| {
            panic!("guarded insertion must not paste")
        });

        assert!(!outcome.receipt.attempted);
        assert!(!outcome.receipt.succeeded);
        assert_eq!(outcome.receipt.method, InsertionMethod::None);
        assert!(outcome.receipt.target_verified);
        assert_eq!(
            outcome.receipt.error.as_deref(),
            Some("language guard blocked paste")
        );
        assert!(outcome.emit_paste_error);
        assert!(!outcome.emit_inserted);
        assert!(outcome.recovery_copy.is_none());
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
        let context_runtime = crate::runtime_settings::context_runtime(&settings);
        let mut target_privacy_excluded = false;
        let should_store_target_context = should_capture_target_context(&settings);
        if should_capture_start_context(&settings) {
            if let Some(store) = app.try_state::<crate::adaptive::session::ActiveDictationContext>()
            {
                let context = crate::adaptive::context::capture_context(
                    context_runtime.private_app_patterns(),
                    context_runtime.should_capture_nearby_text(),
                );
                if target_privacy_exclusion_blocks_recording(&context) {
                    target_privacy_excluded = true;
                    store.clear(&binding_id);
                } else if should_store_target_context {
                    store.insert(&binding_id, context);
                } else {
                    store.clear(&binding_id);
                }
            }
        } else if let Some(store) =
            app.try_state::<crate::adaptive::session::ActiveDictationContext>()
        {
            store.clear(&binding_id);
        }
        let is_always_on = settings.always_on_microphone;
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_error: Option<String> = None;
        if target_privacy_excluded {
            recording_error = Some("target_privacy_excluded".to_string());
        } else if is_always_on {
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
            if let Some(cancellation_state) = app.try_state::<OperationCancellationState>() {
                cancellation_state.begin_operation();
            }

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
                } else if is_selected_microphone_unavailable_error(&err) {
                    "selected_microphone_unavailable"
                } else if err == "target_privacy_excluded" {
                    "target_privacy_excluded"
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

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str, generation: u64) {
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
        let operation_token = current_or_new_operation_token(app);

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone(), generation);
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            let stop_result = rm.stop_recording(&binding_id);
            if stop_result
                .as_ref()
                .is_some_and(|result| result.device_error)
            {
                debug!("Microphone disconnected; preserving mic-failed overlay state");
                if operation_is_cancelled(&ah, operation_token.as_ref()) {
                    finish_cancelled_operation(&ah);
                } else {
                    change_tray_icon(&ah, TrayIconState::Idle);
                }
                return;
            }

            match classify_recording_stop(stop_result, recording_has_usable_speech) {
                RecordingStopDecision::Continue(stop_result) => {
                    if stop_result.vad_fallback {
                        warn!(
                            "Continuing transcription with raw audio because VAD output was empty"
                        );
                    }
                    debug!(
                        "Recording stopped and samples retrieved in {:?}, sample count: {}, captured sample count: {}, active signal observed: {}, diagnostic state: {:?}, VAD fallback: {}",
                        stop_recording_time.elapsed(),
                        stop_result.samples.len(),
                        stop_result.captured_sample_count,
                        stop_result.observed_active_signal,
                        stop_result.diagnostic_state,
                        stop_result.vad_fallback
                    );

                    if operation_is_cancelled(&ah, operation_token.as_ref()) {
                        finish_cancelled_operation(&ah);
                        return;
                    }

                    let observed_active_signal = stop_result.observed_active_signal;
                    let samples = stop_result.samples;
                    let history_settings = get_settings(&ah);
                    let private_session_enabled = crate::private_session::is_enabled(&ah);
                    let (history_enabled, recordings_enabled) =
                        dictation_storage_policy(&history_settings, private_session_enabled);

                    // Save WAV concurrently with transcription when recording storage is enabled.
                    let sample_count = samples.len();
                    let file_name = if recordings_enabled {
                        format!("verbatim-{}.wav", chrono::Utc::now().timestamp())
                    } else {
                        String::new()
                    };
                    let wav_path_for_verify =
                        recordings_enabled.then(|| hm.recordings_dir().join(&file_name));
                    let wav_handle = wav_path_for_verify.clone().map(|wav_path| {
                        let samples_for_wav = samples.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                        })
                    });

                    // Transcribe concurrently with WAV save
                    let transcription_time = Instant::now();
                    let transcription_result = tm.transcribe_with_cancellation(
                        samples,
                        operation_token
                            .as_ref()
                            .map(OperationToken::provider_cancellation)
                            .unwrap_or_default(),
                    );

                    // Await WAV save and verify
                    let wav_saved = match (wav_handle, wav_path_for_verify.as_ref()) {
                        (Some(handle), Some(wav_path)) => match handle.await {
                            Ok(Ok(())) => {
                                match crate::audio_toolkit::verify_wav_file(wav_path, sample_count)
                                {
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
                        },
                        _ => false,
                    };
                    let saved_wav_path = wav_saved.then(|| wav_path_for_verify.clone()).flatten();

                    if operation_is_cancelled(&ah, operation_token.as_ref()) {
                        if let Some(wav_path) = &saved_wav_path {
                            cleanup_cancelled_wav(wav_path);
                        }
                        finish_cancelled_operation(&ah);
                        return;
                    }

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "{}",
                                transcription_completed_log_message(
                                    transcription_time.elapsed(),
                                    &transcription
                                )
                            );

                            let settings = get_settings(&ah);
                            let effective_post_process = should_run_requested_post_processing(
                                post_process,
                                &settings,
                                private_session_enabled,
                            );
                            let captured_context = ah
                                .try_state::<crate::adaptive::session::ActiveDictationContext>()
                                .and_then(|store| store.take(&binding_id));
                            let adaptive_context = if settings.adaptive_profiles_enabled {
                                Some(
                                    captured_context
                                        .clone()
                                        .unwrap_or_else(crate::adaptive::context::unknown_context),
                                )
                            } else {
                                None
                            };
                            let classic_context = if settings.adaptive_profiles_enabled {
                                None
                            } else {
                                captured_context
                            };

                            if effective_post_process || adaptive_context.is_some() {
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

                                if operation_is_cancelled(&ah, operation_token.as_ref()) {
                                    if let Some(wav_path) = &saved_wav_path {
                                        cleanup_cancelled_wav(wav_path);
                                    }
                                    finish_cancelled_operation(&ah);
                                    return;
                                }

                                let final_text = match classify_final_text(
                                    processed.final_text,
                                    observed_active_signal,
                                ) {
                                    FinalTextDecision::Continue(final_text) => final_text,
                                    FinalTextDecision::Terminal(
                                        DictationTransactionTerminal::TranscriptionFailed,
                                    ) => {
                                        warn!(
                                            "Transcription returned empty output despite observed active signal; saving failed history entry"
                                        );
                                        let mut failure_context =
                                            ActionTranscriptionFailureContext {
                                                app: &ah,
                                                history_manager: hm.as_ref(),
                                                history_enabled,
                                                wav_saved,
                                                file_name: file_name.clone(),
                                                post_process,
                                            };
                                        finish_transcription_failed(&mut failure_context);
                                        return;
                                    }
                                    FinalTextDecision::Terminal(terminal) => {
                                        finish_dictation_transaction(&ah, terminal);
                                        return;
                                    }
                                };

                                let profile = crate::adaptive::profile::find_profile_or_default(
                                    &settings.adaptive_profiles,
                                    &processed.routing.profile_id,
                                );

                                let saved_entry_id = if history_enabled {
                                    let metadata =
                                        crate::managers::history::AdaptiveHistoryMetadata {
                                            profile_id: Some(profile.id.clone()),
                                            profile_name: Some(profile.name.clone()),
                                            routing_json: serialize_json(&processed.routing),
                                            context_json:
                                                crate::adaptive::context::context_history_metadata_json(
                                                    &context,
                                                ),
                                            language_json: serialize_json(&processed.language),
                                            insertion_json: None,
                                            parent_entry_id: None,
                                        };
                                    match hm.save_entry_with_metadata(
                                        file_name.clone(),
                                        transcription,
                                        effective_post_process,
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
                                let cancelled_wav_path = saved_wav_path.clone();

                                if operation_is_cancelled(&ah, operation_token.as_ref()) {
                                    cleanup_cancelled_recording(
                                        Arc::clone(&hm),
                                        saved_entry_id,
                                        cancelled_wav_path,
                                    );
                                    finish_cancelled_operation(&ah);
                                    return;
                                }

                                let insertion = AdaptiveInsertionRequest {
                                    app: ah.clone(),
                                    history_manager: Arc::clone(&hm),
                                    settings: settings.clone(),
                                    final_text,
                                    context: context.clone(),
                                    saved_entry_id,
                                    cancelled_wav_path,
                                    operation_token: operation_token.clone(),
                                    paste_started_at: Instant::now(),
                                };
                                if matches!(
                                    insertion.settings.paste_method,
                                    crate::settings::PasteMethod::ExternalScript
                                ) {
                                    std::thread::spawn(move || {
                                        complete_adaptive_insertion(insertion)
                                    });
                                } else {
                                    ah.run_on_main_thread(move || {
                                        complete_adaptive_insertion(insertion);
                                    })
                                    .unwrap_or_else(|e| {
                                        error!("Failed to run paste on main thread: {:?}", e);
                                        finish_dictation_transaction(
                                            &ah,
                                            DictationTransactionTerminal::InsertionSchedulingFailed,
                                        );
                                    });
                                }
                            } else {
                                let processed = process_transcription_output(
                                    &ah,
                                    &transcription,
                                    effective_post_process,
                                    operation_token.clone(),
                                )
                                .await;

                                if operation_is_cancelled(&ah, operation_token.as_ref()) {
                                    if let Some(wav_path) = &saved_wav_path {
                                        cleanup_cancelled_wav(wav_path);
                                    }
                                    finish_cancelled_operation(&ah);
                                    return;
                                }

                                let final_text = match classify_final_text(
                                    processed.final_text,
                                    observed_active_signal,
                                ) {
                                    FinalTextDecision::Continue(final_text) => final_text,
                                    FinalTextDecision::Terminal(
                                        DictationTransactionTerminal::TranscriptionFailed,
                                    ) => {
                                        warn!(
                                            "Transcription returned empty output despite observed active signal; saving failed history entry"
                                        );
                                        let mut failure_context =
                                            ActionTranscriptionFailureContext {
                                                app: &ah,
                                                history_manager: hm.as_ref(),
                                                history_enabled,
                                                wav_saved,
                                                file_name: file_name.clone(),
                                                post_process,
                                            };
                                        finish_transcription_failed(&mut failure_context);
                                        return;
                                    }
                                    FinalTextDecision::Terminal(terminal) => {
                                        finish_dictation_transaction(&ah, terminal);
                                        return;
                                    }
                                };

                                let saved_entry_id = if history_enabled {
                                    match hm.save_entry(
                                        file_name.clone(),
                                        transcription,
                                        effective_post_process,
                                        processed.post_processed_text.clone(),
                                        processed.post_process_prompt.clone(),
                                    ) {
                                        Ok(entry) => Some(entry.id),
                                        Err(err) => {
                                            error!("Failed to save history entry: {}", err);
                                            None
                                        }
                                    }
                                } else {
                                    None
                                };
                                let cancelled_wav_path = saved_wav_path.clone();

                                if operation_is_cancelled(&ah, operation_token.as_ref()) {
                                    cleanup_cancelled_recording(
                                        Arc::clone(&hm),
                                        saved_entry_id,
                                        cancelled_wav_path,
                                    );
                                    finish_cancelled_operation(&ah);
                                    return;
                                }

                                let app_for_insertion = ah.clone();
                                let settings_for_insertion = settings.clone();
                                let paste_started_at = Instant::now();
                                let external_script = matches!(
                                    settings_for_insertion.paste_method,
                                    crate::settings::PasteMethod::ExternalScript
                                );
                                let insertion = move || {
                                    complete_classic_insertion(
                                        app_for_insertion,
                                        Arc::clone(&hm),
                                        settings_for_insertion,
                                        final_text,
                                        saved_entry_id,
                                        cancelled_wav_path,
                                        operation_token.clone(),
                                        classic_context,
                                        paste_started_at,
                                    );
                                };
                                if external_script {
                                    std::thread::spawn(insertion);
                                } else {
                                    ah.run_on_main_thread(insertion).unwrap_or_else(|e| {
                                        error!("Failed to run paste on main thread: {:?}", e);
                                        finish_dictation_transaction(
                                            &ah,
                                            DictationTransactionTerminal::InsertionSchedulingFailed,
                                        );
                                    });
                                }
                            }
                        }
                        Err(err) => {
                            error!("Global Shortcut Transcription error: {}", err);
                            if operation_is_cancelled(&ah, operation_token.as_ref()) {
                                if let Some(wav_path) = &saved_wav_path {
                                    cleanup_cancelled_wav(wav_path);
                                }
                                finish_cancelled_operation(&ah);
                                return;
                            }

                            let mut failure_context = ActionTranscriptionFailureContext {
                                app: &ah,
                                history_manager: hm.as_ref(),
                                history_enabled,
                                wav_saved,
                                file_name,
                                post_process,
                            };
                            finish_transcription_failed(&mut failure_context);
                        }
                    }
                }
                RecordingStopDecision::Terminal(terminal) => {
                    let terminal = if operation_is_cancelled(&ah, operation_token.as_ref()) {
                        DictationTransactionTerminal::Cancelled
                    } else {
                        terminal
                    };
                    match terminal {
                        DictationTransactionTerminal::NoRecording => {
                            debug!("No samples retrieved from recording stop");
                        }
                        DictationTransactionTerminal::NoUsableSpeech => {
                            debug!(
                                "Recording did not contain usable speech; skipping transcription and paste"
                            );
                        }
                        _ => {}
                    }
                    finish_dictation_transaction(&ah, terminal);
                }
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

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str, _generation: u64) {
        // Nothing to do on stop for cancel
    }
}

impl ShortcutAction for TransformShortcutAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let app = app.clone();
        let history_manager = Arc::clone(&app.state::<Arc<HistoryManager>>());
        let action = self.action.clone();
        let target_language = if matches!(action, TransformAction::TranslateToSelectedLanguage) {
            Some(crate::commands::transform::shortcut_target_language(
                &get_settings(&app),
            ))
        } else {
            None
        };
        let binding_id = binding_id.to_string();

        tauri::async_runtime::spawn(async move {
            match crate::commands::transform::run_transform_selected_text(
                app.clone(),
                history_manager,
                action,
                target_language,
            )
            .await
            {
                Ok(result) => {
                    debug!(
                        "Transform shortcut '{}' completed with status {:?}",
                        binding_id, result.status
                    );
                }
                Err(err) => {
                    warn!("Transform shortcut '{}' failed: {}", binding_id, err);
                }
            }
        });
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str, _generation: u64) {
        // Transform shortcuts run once on key press.
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

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str, _generation: u64) {
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
    for (id, action) in [
        ("transform_polish", TransformAction::Polish),
        ("transform_make_concise", TransformAction::MakeConcise),
        ("transform_turn_into_list", TransformAction::TurnIntoList),
        (
            "transform_translate",
            TransformAction::TranslateToSelectedLanguage,
        ),
        ("transform_prompt_engineer", TransformAction::PromptEngineer),
    ] {
        map.insert(
            id.to_string(),
            Arc::new(TransformShortcutAction { action }) as Arc<dyn ShortcutAction>,
        );
    }
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});
