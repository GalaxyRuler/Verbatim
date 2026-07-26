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
use crate::providers::LanguageOutcome;
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};
use crate::shortcut;
use crate::transform_mode::TransformAction;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use crate::TranscriptionCoordinator;
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

#[derive(Clone, serde::Serialize)]
struct TransformSelectionCaptureBlockedEvent {
    reason_code: String,
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

fn language_guard_should_block(outcome: &LanguageOutcome, final_text: &str) -> bool {
    if outcome.translation_performed {
        return false;
    }

    if outcome.supported == crate::providers::Support::Unsupported {
        return false;
    }

    let Some(effective_language) = outcome.effective.as_deref() else {
        return false;
    };

    crate::adaptive::language_guard::contradicts_locked_language(effective_language, final_text)
}

fn language_guard_blocks(app: &AppHandle, outcome: &LanguageOutcome, final_text: &str) -> bool {
    if !language_guard_should_block(outcome, final_text) {
        return false;
    }

    let Some(effective_language) = outcome.effective.as_deref() else {
        return false;
    };

    warn!(
        "Language guard blocked paste because output script contradicts locked language '{}'",
        effective_language
    );
    copy_text_to_clipboard(app, final_text, "language guard block");

    let preview = final_text.chars().take(80).collect();
    let _ = app.emit(
        "language-guard-blocked",
        LanguageGuardEvent {
            locked_language: effective_language.to_string(),
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
    language_outcome: LanguageOutcome,
    final_text: String,
    context: crate::adaptive::types::CapturedContext,
    paste_target: Option<String>,
    saved_entry_id: Option<i64>,
    cancelled_wav_path: Option<std::path::PathBuf>,
    operation_token: Option<OperationToken>,
    paste_started_at: Instant,
}

fn complete_adaptive_insertion(request: AdaptiveInsertionRequest) {
    let AdaptiveInsertionRequest {
        app,
        history_manager,
        settings: _settings,
        language_outcome,
        final_text,
        context,
        paste_target,
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

    if let Err(error) = crate::native_smoke::wait_for_barrier("before_insertion") {
        error!("Native smoke insertion barrier failed: {error}");
    }
    if operation_is_cancelled(&app, operation_token.as_ref()) {
        cleanup_cancelled_recording(
            Arc::clone(&history_manager),
            saved_entry_id,
            cancelled_wav_path,
        );
        finish_cancelled_operation(&app);
        return;
    }
    let target_verified = paste_target_is_current(paste_target.as_deref());
    let expected_target = if target_verified { paste_target } else { None };
    let attempt = if target_verified {
        if language_guard_blocks(&app, &language_outcome, &final_text) {
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
    crate::native_smoke::record_insertion_receipt(&receipt);

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
    _settings: AppSettings,
    language_outcome: LanguageOutcome,
    final_text: String,
    saved_entry_id: Option<i64>,
    cancelled_wav_path: Option<std::path::PathBuf>,
    operation_token: Option<OperationToken>,
    paste_target: Option<String>,
    paste_started_at: Instant,
) {
    if operation_is_cancelled(&app, operation_token.as_ref()) {
        cleanup_cancelled_recording(history_manager, saved_entry_id, cancelled_wav_path);
        finish_cancelled_operation(&app);
        return;
    }

    if let Err(error) = crate::native_smoke::wait_for_barrier("before_insertion") {
        error!("Native smoke insertion barrier failed: {error}");
    }
    if operation_is_cancelled(&app, operation_token.as_ref()) {
        cleanup_cancelled_recording(history_manager, saved_entry_id, cancelled_wav_path);
        finish_cancelled_operation(&app);
        return;
    }
    let target_verified = paste_target_is_current(paste_target.as_deref());
    let expected_target = if target_verified { paste_target } else { None };
    let attempt = if !target_verified {
        error!("Classic paste skipped because the foreground target changed before insertion");
        crate::insertion::InsertionAttempt::classic_target_changed()
    } else if language_guard_blocks(&app, &language_outcome, &final_text) {
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
    let recovery_event = outcome.paste_recovery_event();
    let receipt = outcome.receipt;
    crate::native_smoke::record_insertion_receipt(&receipt);
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
            receipt.error.as_deref()
        );
        if let Some(recovery_event) = recovery_event {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostProcessingPromptSource {
    UserSelected,
    AdaptiveProfile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedPostProcessingPrompt {
    prompt: String,
    source: PostProcessingPromptSource,
}

fn resolve_post_processing_prompt(
    settings: &AppSettings,
    adaptive_profile: Option<&crate::adaptive::profile::AdaptiveProfile>,
    deterministic_output: &str,
) -> Option<ResolvedPostProcessingPrompt> {
    if let Some(prompt_id) = settings.post_process_selected_prompt_id.as_deref() {
        return settings
            .post_process_prompts
            .iter()
            .find(|prompt| prompt.id == prompt_id && !prompt.prompt.trim().is_empty())
            .map(|prompt| ResolvedPostProcessingPrompt {
                prompt: prompt.prompt.clone(),
                source: PostProcessingPromptSource::UserSelected,
            });
    }

    adaptive_profile
        .and_then(|profile| {
            crate::adaptive::processor::build_profile_prompt(deterministic_output, profile)
        })
        .filter(|prompt| !prompt.trim().is_empty())
        .map(|prompt| ResolvedPostProcessingPrompt {
            prompt,
            source: PostProcessingPromptSource::AdaptiveProfile,
        })
}

fn settings_for_post_processing_prompt(
    settings: &AppSettings,
    resolved_prompt: Option<&ResolvedPostProcessingPrompt>,
) -> AppSettings {
    const ADAPTIVE_PROFILE_PROMPT_ID: &str = "__adaptive_profile_prompt";

    let mut runtime_settings = settings.clone();
    if let Some(ResolvedPostProcessingPrompt {
        prompt,
        source: PostProcessingPromptSource::AdaptiveProfile,
    }) = resolved_prompt
    {
        runtime_settings
            .post_process_prompts
            .retain(|candidate| candidate.id != ADAPTIVE_PROFILE_PROMPT_ID);
        runtime_settings
            .post_process_prompts
            .push(crate::settings::LLMPrompt {
                id: ADAPTIVE_PROFILE_PROMPT_ID.to_string(),
                name: "Adaptive profile".to_string(),
                prompt: prompt.clone(),
            });
        runtime_settings.post_process_selected_prompt_id =
            Some(ADAPTIVE_PROFILE_PROMPT_ID.to_string());
    }
    runtime_settings
}

struct OptionalLlmStageResult {
    completion: crate::pipeline::PipelineCompletion,
    prompt: Option<String>,
}

async fn run_optional_llm_stage_with<Invoke, InvokeFuture>(
    requested: bool,
    settings: &AppSettings,
    private_session_enabled: bool,
    adaptive_profile: Option<&crate::adaptive::profile::AdaptiveProfile>,
    deterministic_output: &str,
    invoke: Invoke,
) -> OptionalLlmStageResult
where
    Invoke: FnOnce(AppSettings, Option<ResolvedPostProcessingPrompt>, String) -> InvokeFuture,
    InvokeFuture: std::future::Future<Output = Option<String>>,
{
    if !should_run_requested_post_processing(requested, settings, private_session_enabled) {
        return OptionalLlmStageResult {
            completion: crate::pipeline::PipelineCompletion {
                llm_output: None,
                llm_invoked: false,
            },
            prompt: None,
        };
    }

    let resolved_prompt =
        resolve_post_processing_prompt(settings, adaptive_profile, deterministic_output);
    let runtime_settings = settings_for_post_processing_prompt(settings, resolved_prompt.as_ref());
    let runtime = crate::runtime_settings::post_processing_runtime(&runtime_settings, true);
    if !runtime.should_run() {
        debug!(
            "Post-processing skipped by runtime settings: {:?}",
            runtime.skip_reason()
        );
        return OptionalLlmStageResult {
            completion: crate::pipeline::PipelineCompletion {
                llm_output: None,
                llm_invoked: false,
            },
            prompt: None,
        };
    }

    let prompt = resolved_prompt
        .as_ref()
        .map(|resolved| resolved.prompt.clone());
    let llm_output = invoke(
        runtime_settings,
        resolved_prompt,
        deterministic_output.to_string(),
    )
    .await;
    OptionalLlmStageResult {
        completion: crate::pipeline::PipelineCompletion {
            llm_output,
            llm_invoked: true,
        },
        prompt,
    }
}
struct SharedPipelineOutput {
    pipeline: crate::pipeline::PipelineResult,
    prompt: Option<String>,
}

async fn execute_post_transcription_pipeline_with<Invoke, InvokeFuture>(
    settings: &AppSettings,
    raw_input: &str,
    adaptive_profile: Option<&crate::adaptive::profile::AdaptiveProfile>,
    effective_language: Option<&str>,
    post_process_requested: bool,
    private_session_enabled: bool,
    invoke: Invoke,
) -> SharedPipelineOutput
where
    Invoke: FnOnce(AppSettings, Option<ResolvedPostProcessingPrompt>, String) -> InvokeFuture,
    InvokeFuture: std::future::Future<Output = Option<String>>,
{
    let prepared = crate::pipeline::prepare_post_transcription_pipeline(
        crate::pipeline::PipelinePreparationInput {
            raw_input,
            selected_language: &settings.selected_language,
            formatting_level: settings.formatting_level,
            adaptive_profile,
            effective_language,
        },
    );
    let deterministic_output = prepared.deterministic_output.clone();
    let llm_stage = run_optional_llm_stage_with(
        post_process_requested,
        settings,
        private_session_enabled,
        adaptive_profile,
        &deterministic_output,
        invoke,
    )
    .await;
    let pipeline =
        crate::pipeline::finalize_post_transcription_pipeline(prepared, llm_stage.completion);
    let prompt = if pipeline
        .llm_output
        .as_ref()
        .is_some_and(|output| output == &pipeline.final_text)
    {
        llm_stage.prompt
    } else {
        None
    };

    SharedPipelineOutput { pipeline, prompt }
}

async fn invoke_single_llm_stage_with_fallback<Structured, StructuredFuture, Legacy, LegacyFuture>(
    supports_structured_output: bool,
    structured: Structured,
    legacy: Legacy,
) -> Option<String>
where
    Structured: FnOnce() -> StructuredFuture,
    StructuredFuture: std::future::Future<Output = Result<Option<String>, String>>,
    Legacy: FnOnce() -> LegacyFuture,
    LegacyFuture: std::future::Future<Output = Result<Option<String>, String>>,
{
    if supports_structured_output {
        match structured().await {
            Ok(output) => return output,
            Err(err) => {
                warn!(
                    "Structured post-processing failed: {}. Retrying the same logical LLM stage in legacy mode.",
                    err
                );
            }
        }
    }

    match legacy().await {
        Ok(output) => output,
        Err(err) => {
            error!("Legacy post-processing failed: {}", err);
            None
        }
    }
}

pub(crate) fn dictation_storage_policy(
    settings: &AppSettings,
    private_session_enabled: bool,
) -> (bool, bool) {
    let history_enabled = settings.history_enabled && !private_session_enabled;
    let recordings_enabled = history_enabled && settings.recordings_enabled;
    (history_enabled, recordings_enabled)
}

fn render_post_processing_prompt(
    prompt: &ResolvedPostProcessingPrompt,
    transcription: &str,
) -> String {
    match prompt.source {
        PostProcessingPromptSource::UserSelected => {
            prompt.prompt.replace("${output}", transcription)
        }
        PostProcessingPromptSource::AdaptiveProfile => prompt.prompt.clone(),
    }
}

fn structured_post_processing_request_parts(
    prompt: &ResolvedPostProcessingPrompt,
    transcription: &str,
) -> (String, Option<String>) {
    match prompt.source {
        PostProcessingPromptSource::UserSelected => (
            transcription.to_string(),
            Some(build_system_prompt(&prompt.prompt)),
        ),
        PostProcessingPromptSource::AdaptiveProfile => (prompt.prompt.clone(), None),
    }
}

async fn post_process_with_managed_local_llm(
    app: &AppHandle,
    settings: &crate::local_llm::LocalLlmSettings,
    transcription: &str,
    resolved_prompt: Option<&ResolvedPostProcessingPrompt>,
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

    let user_content = resolved_prompt
        .map(|prompt| render_post_processing_prompt(prompt, transcription))
        .unwrap_or_else(|| transcription.to_string());
    let provider_cancellation = operation_token.map(|token| token.provider_cancellation());
    match crate::llm_client::send_chat_completion_with_schema_and_cancellation(
        &endpoint.provider,
        String::new(),
        &endpoint.model,
        user_content,
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
                "Managed local post-processing failed: {}. Falling back to deterministic transcript.",
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
    resolved_prompt: Option<&ResolvedPostProcessingPrompt>,
    operation_token: Option<&OperationToken>,
) -> Option<String> {
    let runtime = crate::runtime_settings::post_processing_runtime(settings, true);

    if runtime.uses_managed_local() {
        return post_process_with_managed_local_llm(
            app,
            &settings.local_llm,
            transcription,
            resolved_prompt,
            operation_token,
        )
        .await;
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
    let api_key = api_runtime.api_key;
    let resolved_prompt =
        resolved_prompt
            .cloned()
            .unwrap_or_else(|| ResolvedPostProcessingPrompt {
                prompt: api_runtime.prompt.prompt,
                source: PostProcessingPromptSource::UserSelected,
            });

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    if operation_is_cancelled(app, operation_token) {
        debug!("Post-processing skipped because operation was cancelled before provider request");
        return None;
    }

    let provider_cancellation = operation_token.map(|token| token.provider_cancellation());

    if provider.supports_structured_output && provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        let (user_content, system_prompt) =
            structured_post_processing_request_parts(&resolved_prompt, transcription);

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            if !apple_intelligence::check_apple_intelligence_availability() {
                debug!("Apple Intelligence selected but not currently available on this device");
                return None;
            }

            let token_limit = model.trim().parse::<i32>().unwrap_or(0);
            return match apple_intelligence::process_text_with_system_prompt(
                system_prompt.as_deref().unwrap_or_default(),
                &user_content,
                token_limit,
            ) {
                Ok(result) if !result.trim().is_empty() => {
                    let result = crate::text_processing::strip_invisible_chars(&result);
                    accept_post_processed_text(transcription, result, &provider.id)
                }
                Ok(_) => {
                    debug!("Apple Intelligence returned an empty response");
                    None
                }
                Err(err) => {
                    error!("Apple Intelligence post-processing failed: {}", err);
                    None
                }
            };
        }

        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            let _ = (user_content, system_prompt);
            debug!("Apple Intelligence provider selected on unsupported platform");
            return None;
        }
    }

    let structured_prompt = resolved_prompt.clone();
    let structured_request = || async {
        let (user_content, system_prompt) =
            structured_post_processing_request_parts(&structured_prompt, transcription);
        let request = crate::text_processing::TextProviderRequest::structured(
            user_content,
            system_prompt,
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
            Ok(Some(content)) => Ok(accept_structured_post_processed_text(
                transcription,
                &content,
                &provider.id,
            )),
            Ok(None) => {
                error!("LLM API response has no content");
                Ok(None)
            }
            Err(err) => Err(format!("provider '{}': {}", provider.id, err)),
        }
    };

    let legacy_prompt = resolved_prompt.clone();
    let legacy_request = || async {
        if operation_is_cancelled(app, operation_token) {
            debug!("Post-processing skipped because operation was cancelled before legacy request");
            return Ok(None);
        }

        let processed_prompt = render_post_processing_prompt(&legacy_prompt, transcription);
        debug!("Processed prompt length: {} chars", processed_prompt.len());
        let request = crate::text_processing::TextProviderRequest::new(processed_prompt, None);

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
                let content = crate::text_processing::strip_invisible_chars(&content);
                debug!(
                    "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                    provider.id,
                    content.len()
                );
                Ok(accept_post_processed_text(
                    transcription,
                    content,
                    &provider.id,
                ))
            }
            Ok(None) => {
                error!("LLM API response has no content");
                Ok(None)
            }
            Err(err) => Err(format!("provider '{}': {}", provider.id, err)),
        }
    };

    invoke_single_llm_stage_with_fallback(
        provider.supports_structured_output,
        structured_request,
        legacy_request,
    )
    .await
}
pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

async fn process_transcription_output_with_profile<Invoke, InvokeFuture>(
    settings: &AppSettings,
    transcription: &str,
    adaptive_profile: Option<&crate::adaptive::profile::AdaptiveProfile>,
    effective_language: Option<&str>,
    post_process_requested: bool,
    private_session_enabled: bool,
    invoke: Invoke,
) -> ProcessedTranscription
where
    Invoke: FnOnce(AppSettings, Option<ResolvedPostProcessingPrompt>, String) -> InvokeFuture,
    InvokeFuture: std::future::Future<Output = Option<String>>,
{
    let shared = execute_post_transcription_pipeline_with(
        settings,
        transcription,
        adaptive_profile,
        effective_language,
        post_process_requested,
        private_session_enabled,
        invoke,
    )
    .await;

    if let Some(reason) = &shared.pipeline.fallback_reason {
        warn!("Post-transcription pipeline used a fallback: {:?}", reason);
    }
    let post_processed_text = (shared.pipeline.final_text != shared.pipeline.raw_input)
        .then(|| shared.pipeline.final_text.clone());

    ProcessedTranscription {
        final_text: shared.pipeline.final_text,
        post_processed_text,
        post_process_prompt: shared.prompt,
    }
}

pub(crate) async fn process_transcription_output_with_profile_on_app(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    adaptive_profile: Option<&crate::adaptive::profile::AdaptiveProfile>,
    effective_language: Option<&str>,
    post_process_requested: bool,
    private_session_enabled: bool,
    operation_token: Option<OperationToken>,
) -> ProcessedTranscription {
    let operation_token_for_llm = operation_token.clone();
    process_transcription_output_with_profile(
        settings,
        transcription,
        adaptive_profile,
        effective_language,
        post_process_requested,
        private_session_enabled,
        move |runtime_settings, resolved_prompt, provider_input| async move {
            post_process_transcription(
                app,
                &runtime_settings,
                &provider_input,
                resolved_prompt.as_ref(),
                operation_token_for_llm.as_ref(),
            )
            .await
        },
    )
    .await
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    post_process_requested: bool,
    private_session_enabled: bool,
    operation_token: Option<OperationToken>,
) -> ProcessedTranscription {
    process_transcription_output_with_profile_on_app(
        app,
        settings,
        transcription,
        None,
        None,
        post_process_requested,
        private_session_enabled,
        operation_token,
    )
    .await
}
fn serialize_json<T: serde::Serialize>(value: &T) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn should_mute_before_start_feedback(settings: &AppSettings) -> bool {
    settings.mute_while_recording && !settings.audio_feedback
}

#[cfg(test)]
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

fn capture_paste_target() -> Option<String> {
    crate::adaptive::context::capture_dispatch_target()
}

fn paste_target_is_current(expected_target: Option<&str>) -> bool {
    let current_target = capture_paste_target();
    paste_target_matches(expected_target, current_target.as_deref())
}

fn paste_target_matches(expected_target: Option<&str>, current_target: Option<&str>) -> bool {
    expected_target.is_none() || current_target == expected_target
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
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    effective_language: Option<&str>,
    context: crate::adaptive::types::CapturedContext,
    shortcut: crate::adaptive::types::ShortcutIntent,
    post_process_requested: bool,
    private_session_enabled: bool,
    operation_token: Option<OperationToken>,
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
        &settings.adaptive_default_profile_id,
    );
    let profile = crate::adaptive::profile::find_profile_or_default(
        &settings.adaptive_profiles,
        &routing.profile_id,
    );

    let processed = process_transcription_output_with_profile_on_app(
        app,
        settings,
        transcription,
        Some(profile),
        effective_language,
        post_process_requested,
        private_session_enabled,
        operation_token,
    )
    .await;

    crate::adaptive::types::AdaptiveProcessResult {
        final_text: processed.final_text,
        post_processed_text: processed.post_processed_text,
        post_process_prompt: processed.post_process_prompt,
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
    fn adaptive_target_verification_rejects_another_window_of_the_same_app() {
        assert!(!adaptive_target_verified(
            &context_with_fingerprint(Some("notepad.exe|notepad|29|101")),
            &context_with_fingerprint(Some("notepad.exe|notepad|2a|202"))
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
    fn paste_target_guard_applies_without_context_awareness() {
        assert!(paste_target_matches(
            None,
            Some("notepad.exe|notepad|29|101")
        ));
        assert!(paste_target_matches(
            Some("notepad.exe|notepad|29|101"),
            Some("notepad.exe|notepad|29|101")
        ));
        assert!(!paste_target_matches(
            Some("notepad.exe|notepad|29|101"),
            Some("notepad.exe|notepad|2a|202")
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

    #[derive(Clone, Copy)]
    enum TestPostTranscriptionMode {
        Classic,
        Adaptive,
    }

    fn failed_history_rows_for(
        mode: TestPostTranscriptionMode,
        transcription_result: Result<String, ()>,
        observed_active_signal: bool,
    ) -> usize {
        let mut context = CountingTranscriptionFailureContext::default();
        let terminal = match mode {
            TestPostTranscriptionMode::Classic | TestPostTranscriptionMode::Adaptive => {
                match transcription_result {
                    Err(()) => DictationTransactionTerminal::TranscriptionFailed,
                    Ok(final_text) => match classify_final_text(final_text, observed_active_signal)
                    {
                        FinalTextDecision::Terminal(terminal) => terminal,
                        FinalTextDecision::Continue(_) => {
                            DictationTransactionTerminal::InsertionCompleted
                        }
                    },
                }
            }
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
    fn one_dictation_finishes_exactly_one_terminal_in_both_pipeline_modes() {
        for mode in [
            TestPostTranscriptionMode::Classic,
            TestPostTranscriptionMode::Adaptive,
        ] {
            assert_eq!(failed_history_rows_for(mode, Err(()), false), 1);
            assert_eq!(failed_history_rows_for(mode, Ok(String::new()), true), 1);
            assert_eq!(failed_history_rows_for(mode, Ok(String::new()), false), 0);
        }
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
    fn ignored_translation_request_does_not_bypass_language_guard() {
        let outcome = LanguageOutcome {
            requested: Some("ar".to_string()),
            effective: Some("ar".to_string()),
            supported: crate::providers::Support::Supported,
            translation_requested: true,
            translation_performed: false,
            ..LanguageOutcome::default()
        };

        assert!(language_guard_should_block(
            &outcome,
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn performed_translation_bypasses_language_guard() {
        let outcome = LanguageOutcome {
            requested: Some("ar".to_string()),
            effective: Some("ar".to_string()),
            supported: crate::providers::Support::Supported,
            translation_requested: true,
            translation_performed: true,
            ..LanguageOutcome::default()
        };

        assert!(!language_guard_should_block(
            &outcome,
            "This is a translated English sentence"
        ));
    }

    #[test]
    fn unsupported_locked_language_falls_back_to_auto_without_guard_block() {
        let outcome = LanguageOutcome {
            requested: Some("ar".to_string()),
            effective: None,
            supported: crate::providers::Support::Unsupported,
            ..LanguageOutcome::default()
        };

        assert!(!language_guard_should_block(
            &outcome,
            "This is a correct English transcription"
        ));
    }

    #[test]
    fn supported_locked_language_still_blocks_contradictory_script() {
        let outcome = LanguageOutcome {
            requested: Some("ar".to_string()),
            effective: Some("ar".to_string()),
            supported: crate::providers::Support::Supported,
            ..LanguageOutcome::default()
        };

        assert!(language_guard_should_block(
            &outcome,
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn multilingual_auto_mode_keeps_language_guard_inert() {
        let outcome = LanguageOutcome {
            requested: None,
            effective: None,
            supported: crate::providers::Support::Unknown,
            detected: None,
            ..LanguageOutcome::default()
        };

        assert!(!language_guard_should_block(
            &outcome,
            "This is a clear English sentence"
        ));
    }

    #[test]
    fn language_guard_consumes_outcome_effective_instead_of_raw_setting() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_language = "en".to_string();
        let outcome = LanguageOutcome {
            requested: Some("en".to_string()),
            effective: Some("ar".to_string()),
            supported: crate::providers::Support::Supported,
            ..LanguageOutcome::default()
        };
        assert_ne!(
            settings.selected_language,
            outcome.effective.as_deref().expect("effective language")
        );

        assert!(language_guard_should_block(
            &outcome,
            "This is a clear English sentence"
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

    #[test]
    fn adaptive_profile_prompt_is_selected_when_user_has_not_selected_one() {
        let mut settings = crate::settings::get_default_settings();
        settings.post_process_selected_prompt_id = None;
        let profile = settings
            .adaptive_profiles
            .iter()
            .find(|profile| profile.id == "email")
            .expect("email profile");

        let resolved =
            resolve_post_processing_prompt(&settings, Some(profile), "Cleaned transcript")
                .expect("adaptive profile prompt");

        assert_eq!(resolved.source, PostProcessingPromptSource::AdaptiveProfile);
        assert_eq!(
            resolved.prompt,
            crate::adaptive::processor::build_profile_prompt("Cleaned transcript", profile)
                .expect("profile prompt")
        );
    }

    #[test]
    fn user_selected_prompt_wins_over_adaptive_profile_prompt() {
        let mut settings = crate::settings::get_default_settings();
        settings.post_process_selected_prompt_id = Some("custom".to_string());
        settings.post_process_prompts = vec![crate::settings::LLMPrompt {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            prompt: "My explicit prompt: ${output}".to_string(),
        }];
        let profile = settings
            .adaptive_profiles
            .iter()
            .find(|profile| profile.id == "email")
            .expect("email profile");

        let resolved =
            resolve_post_processing_prompt(&settings, Some(profile), "Cleaned transcript")
                .expect("user prompt");

        assert_eq!(resolved.source, PostProcessingPromptSource::UserSelected);
        assert_eq!(resolved.prompt, "My explicit prompt: ${output}");
    }

    #[tokio::test]
    async fn structured_failure_uses_legacy_retry_for_one_logical_output() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let structured_calls = Arc::new(AtomicUsize::new(0));
        let legacy_calls = Arc::new(AtomicUsize::new(0));
        let structured_calls_for_request = Arc::clone(&structured_calls);
        let legacy_calls_for_request = Arc::clone(&legacy_calls);

        let output = invoke_single_llm_stage_with_fallback(
            true,
            move || async move {
                structured_calls_for_request.fetch_add(1, Ordering::SeqCst);
                Err("structured request failed".to_string())
            },
            move || async move {
                legacy_calls_for_request.fetch_add(1, Ordering::SeqCst);
                Ok(Some("legacy output".to_string()))
            },
        )
        .await;

        assert_eq!(output, Some("legacy output".to_string()));
        assert_eq!(structured_calls.load(Ordering::SeqCst), 1);
        assert_eq!(legacy_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn structured_success_does_not_apply_a_legacy_output() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let legacy_calls = Arc::new(AtomicUsize::new(0));
        let legacy_calls_for_request = Arc::clone(&legacy_calls);

        let output = invoke_single_llm_stage_with_fallback(
            true,
            || async { Ok(Some("structured output".to_string())) },
            move || async move {
                legacy_calls_for_request.fetch_add(1, Ordering::SeqCst);
                Ok(Some("legacy output".to_string()))
            },
        )
        .await;

        assert_eq!(output, Some("structured output".to_string()));
        assert_eq!(legacy_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn adaptive_profile_processor_invokes_requested_llm_once_and_uses_its_output() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let settings = post_processing_settings_for_provider("http://127.0.0.1:11434/v1", "");
        let profile = settings
            .adaptive_profiles
            .iter()
            .find(|profile| profile.id == "email")
            .expect("email profile");
        let invoke_count = Arc::new(AtomicUsize::new(0));
        let invoke_count_for_call = Arc::clone(&invoke_count);

        let output = process_transcription_output_with_profile(
            &settings,
            "hello world",
            Some(profile),
            Some("en"),
            true,
            false,
            move |_, _, _| {
                invoke_count_for_call.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Some("Hello world.".to_string()))
            },
        )
        .await;

        assert_eq!(invoke_count.load(Ordering::SeqCst), 1);
        assert_eq!(output.final_text, "Hello world.");
    }

    #[tokio::test]
    async fn adaptive_requested_post_processing_invokes_shared_llm_and_uses_output() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let settings = post_processing_settings_for_provider("http://127.0.0.1:11434/v1", "");
        let profile = settings
            .adaptive_profiles
            .iter()
            .find(|profile| profile.id == "email")
            .expect("email profile");
        let invoke_count = Arc::new(AtomicUsize::new(0));
        let invoke_count_for_call = Arc::clone(&invoke_count);

        let output = execute_post_transcription_pipeline_with(
            &settings,
            "hello world",
            Some(profile),
            Some("en"),
            true,
            false,
            move |_, _, _| {
                invoke_count_for_call.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Some("Hello world.".to_string()))
            },
        )
        .await;

        assert_eq!(invoke_count.load(Ordering::SeqCst), 1);
        assert!(output.pipeline.llm_invoked);
        assert_eq!(output.pipeline.llm_output.as_deref(), Some("Hello world."));
        assert_eq!(output.pipeline.final_text, "Hello world.");
    }
    #[tokio::test]
    async fn adaptive_profile_alone_does_not_authorize_an_llm_stage() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let settings = post_processing_settings_for_provider("http://127.0.0.1:11434/v1", "");
        let profile = settings
            .adaptive_profiles
            .iter()
            .find(|profile| profile.id == "email")
            .expect("email profile");
        let invoke_count = Arc::new(AtomicUsize::new(0));
        let invoke_count_for_call = Arc::clone(&invoke_count);

        let output = execute_post_transcription_pipeline_with(
            &settings,
            "hello world",
            Some(profile),
            Some("en"),
            false,
            false,
            move |_, _, _| {
                invoke_count_for_call.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Some("surprise output".to_string()))
            },
        )
        .await;

        assert_eq!(invoke_count.load(Ordering::SeqCst), 0);
        assert!(!output.pipeline.llm_invoked);
        assert!(output.pipeline.llm_output.is_none());
    }

    #[tokio::test]
    async fn adaptive_profile_prompt_is_sent_by_shared_llm_stage_without_user_prompt() {
        let mut settings = post_processing_settings_for_provider("http://127.0.0.1:11434/v1", "");
        settings.post_process_selected_prompt_id = None;
        let profile = settings
            .adaptive_profiles
            .iter()
            .find(|profile| profile.id == "email")
            .expect("email profile");
        let expected_prompt =
            crate::adaptive::processor::build_profile_prompt("hello world", profile)
                .expect("profile prompt");

        let output = execute_post_transcription_pipeline_with(
            &settings,
            "hello world",
            Some(profile),
            Some("en"),
            true,
            false,
            move |_, resolved_prompt, _| {
                assert_eq!(
                    resolved_prompt.map(|resolved| resolved.prompt),
                    Some(expected_prompt)
                );
                std::future::ready(Some("Hello world.".to_string()))
            },
        )
        .await;

        assert_eq!(output.pipeline.final_text, "Hello world.");
        assert!(output.pipeline.llm_invoked);
    }

    #[tokio::test]
    async fn chinese_variant_conversion_is_shared_by_classic_and_adaptive_modes() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_language = "zh-Hans".to_string();
        let profile = settings
            .adaptive_profiles
            .iter()
            .find(|profile| profile.id == "default_clean")
            .expect("default_clean profile");

        let classic = execute_post_transcription_pipeline_with(
            &settings,
            "軟件",
            None,
            None,
            false,
            false,
            |_, _, _| std::future::ready(None),
        )
        .await;
        let adaptive = execute_post_transcription_pipeline_with(
            &settings,
            "軟件",
            Some(profile),
            None,
            false,
            false,
            |_, _, _| std::future::ready(None),
        )
        .await;

        assert_eq!(classic.pipeline.final_text, "软件");
        assert_eq!(adaptive.pipeline.final_text, "软件");
        assert!(classic.pipeline.zh_conversion_applied);
        assert!(adaptive.pipeline.zh_conversion_applied);
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Keep a runtime-only identity for the exact foreground window. This
        // does not read a title or nearby text, and lets every dictation mode
        // reject a focus switch before it inserts text.
        if let Some(targets) = app.try_state::<crate::adaptive::session::ActivePasteTarget>() {
            targets.insert(binding_id, capture_paste_target());
        }

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
            if let Some(targets) = app.try_state::<crate::adaptive::session::ActivePasteTarget>() {
                targets.clear(&binding_id);
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
                    let transcription_result = tm.transcribe_with_cancellation_context(
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
                        Ok(transcription_output) => {
                            let transcription = transcription_output.text;
                            let language_outcome = transcription_output.outcome;
                            debug!(
                                "{}",
                                transcription_completed_log_message(
                                    transcription_time.elapsed(),
                                    &transcription
                                )
                            );

                            let mut settings = get_settings(&ah);
                            crate::credentials::hydrate_runtime_post_process_api_keys(
                                &ah,
                                &mut settings,
                            );
                            let effective_post_process = should_run_requested_post_processing(
                                post_process,
                                &settings,
                                private_session_enabled,
                            );
                            let captured_context = ah
                                .try_state::<crate::adaptive::session::ActiveDictationContext>()
                                .and_then(|store| store.take(&binding_id));
                            let paste_target = ah
                                .try_state::<crate::adaptive::session::ActivePasteTarget>()
                                .and_then(|targets| targets.take(&binding_id));
                            let adaptive_context = if settings.adaptive_profiles_enabled {
                                Some(
                                    captured_context
                                        .clone()
                                        .unwrap_or_else(crate::adaptive::context::unknown_context),
                                )
                            } else {
                                None
                            };

                            if effective_post_process || adaptive_context.is_some() {
                                show_processing_overlay(&ah);
                            }

                            if let Some(context) = adaptive_context {
                                let processed = process_adaptive_transcription_output(
                                    &ah,
                                    &settings,
                                    &transcription,
                                    transcription_output.effective_language.as_deref(),
                                    context.clone(),
                                    crate::adaptive::types::ShortcutIntent::Default,
                                    post_process,
                                    private_session_enabled,
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
                                    language_outcome,
                                    final_text,
                                    context: context.clone(),
                                    paste_target,
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
                                    &settings,
                                    &transcription,
                                    post_process,
                                    private_session_enabled,
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
                                        language_outcome,
                                        final_text,
                                        saved_entry_id,
                                        cancelled_wav_path,
                                        operation_token.clone(),
                                        paste_target,
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
                            if let Some(targets) =
                                ah.try_state::<crate::adaptive::session::ActivePasteTarget>()
                            {
                                targets.clear(&binding_id);
                            }
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
                    if let Some(targets) =
                        ah.try_state::<crate::adaptive::session::ActivePasteTarget>()
                    {
                        targets.clear(&binding_id);
                    }
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
        if let Some(targets) = app.try_state::<crate::adaptive::session::ActivePasteTarget>() {
            targets.clear(binding_id);
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
                    if matches!(
                        err.as_str(),
                        crate::selection::SECURE_FIELD_REASON_CODE
                            | crate::selection::SECURE_CHECK_ERROR_REASON_CODE
                    ) {
                        let _ = app.emit(
                            "transform-selection-capture-blocked",
                            TransformSelectionCaptureBlockedEvent {
                                reason_code: err.clone(),
                            },
                        );
                    }
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
