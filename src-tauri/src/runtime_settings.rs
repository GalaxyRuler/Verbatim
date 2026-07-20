use crate::settings::{
    is_insecure_lan_post_process_base_url, is_local_post_process_base_url, AppSettings, LLMPrompt,
    OverlayPosition, PostProcessProvider,
};
use std::time::Duration;

const SHORTCUT_DEBOUNCE: Duration = Duration::from_millis(30);
const SHORTCUT_DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(500);
const SHORTCUT_MAX_LATCH_TAP_DURATION: Duration = Duration::from_millis(280);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutRuntime {
    push_to_talk: bool,
    latch_enabled: bool,
    debounce: Duration,
    double_tap_window: Duration,
    max_latch_tap_duration: Duration,
}

impl ShortcutRuntime {
    pub fn push_to_talk_mode() -> Self {
        Self {
            push_to_talk: true,
            latch_enabled: true,
            debounce: SHORTCUT_DEBOUNCE,
            double_tap_window: SHORTCUT_DOUBLE_TAP_WINDOW,
            max_latch_tap_duration: SHORTCUT_MAX_LATCH_TAP_DURATION,
        }
    }

    pub fn toggle_mode() -> Self {
        Self {
            push_to_talk: false,
            latch_enabled: false,
            debounce: SHORTCUT_DEBOUNCE,
            double_tap_window: SHORTCUT_DOUBLE_TAP_WINDOW,
            max_latch_tap_duration: SHORTCUT_MAX_LATCH_TAP_DURATION,
        }
    }

    pub fn as_toggle(&self) -> Self {
        Self {
            push_to_talk: false,
            latch_enabled: false,
            debounce: self.debounce,
            double_tap_window: self.double_tap_window,
            max_latch_tap_duration: self.max_latch_tap_duration,
        }
    }

    pub fn push_to_talk(&self) -> bool {
        self.push_to_talk
    }

    pub fn latch_enabled(&self) -> bool {
        self.latch_enabled
    }

    pub fn debounce(&self) -> Duration {
        self.debounce
    }

    pub fn double_tap_window(&self) -> Duration {
        self.double_tap_window
    }

    pub fn max_latch_tap_duration(&self) -> Duration {
        self.max_latch_tap_duration
    }
}

pub fn shortcut_runtime(settings: &AppSettings) -> ShortcutRuntime {
    if settings.push_to_talk {
        ShortcutRuntime::push_to_talk_mode()
    } else {
        ShortcutRuntime::toggle_mode()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayRuntime {
    position: OverlayPosition,
    docked: bool,
}

impl OverlayRuntime {
    pub fn position(&self) -> OverlayPosition {
        self.position
    }

    pub fn should_show_active_overlay(&self) -> bool {
        self.docked || self.position != OverlayPosition::None
    }

    pub fn should_show_docked_idle(&self) -> bool {
        self.docked
    }

    pub fn starts_expanded(&self) -> bool {
        !self.docked
    }
}

pub fn overlay_runtime(settings: &AppSettings) -> OverlayRuntime {
    OverlayRuntime {
        position: settings.overlay_position,
        docked: settings.docked_pill_enabled,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextRuntime {
    capture_context: bool,
    capture_nearby_text: bool,
    private_app_patterns: Vec<String>,
}

impl ContextRuntime {
    pub fn should_capture_context(&self) -> bool {
        self.capture_context
    }

    pub fn should_capture_nearby_text(&self) -> bool {
        self.capture_nearby_text
    }

    pub fn private_app_patterns(&self) -> &[String] {
        &self.private_app_patterns
    }
}

pub fn context_runtime(settings: &AppSettings) -> ContextRuntime {
    let capture_context = settings.adaptive_profiles_enabled && settings.context_awareness_enabled;
    let capture_nearby_text = capture_context && settings.context_nearby_text_enabled;

    ContextRuntime {
        capture_context,
        capture_nearby_text,
        private_app_patterns: settings.adaptive_private_app_patterns.clone(),
    }
}

#[derive(Clone, Debug)]
pub struct TextProcessingApiRuntime {
    pub provider: PostProcessProvider,
    pub model: String,
    pub api_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextProcessingIntent {
    DictationPostProcessing { requested: bool },
    ExplicitTransform,
}

#[derive(Clone, Debug)]
enum TextProcessingRuntimeMode {
    Disabled,
    ManagedLocal,
    ApiProvider(TextProcessingApiRuntime),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextProcessingSkipReason {
    NotRequested,
    DisabledBySettings,
    MissingProvider,
    MissingModel,
    RemoteMissingApiKey,
}

#[derive(Clone, Debug)]
pub struct TextProcessingRuntime {
    mode: TextProcessingRuntimeMode,
    skip_reason: Option<TextProcessingSkipReason>,
}

impl TextProcessingRuntime {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn should_run(&self) -> bool {
        self.skip_reason.is_none() && !matches!(self.mode, TextProcessingRuntimeMode::Disabled)
    }

    pub fn uses_managed_local(&self) -> bool {
        matches!(self.mode, TextProcessingRuntimeMode::ManagedLocal)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn should_attempt_api(&self) -> bool {
        self.should_run() && matches!(self.mode, TextProcessingRuntimeMode::ApiProvider(_))
    }

    pub fn api_provider(&self) -> Option<&TextProcessingApiRuntime> {
        match &self.mode {
            TextProcessingRuntimeMode::ApiProvider(runtime) => Some(runtime),
            TextProcessingRuntimeMode::Disabled | TextProcessingRuntimeMode::ManagedLocal => None,
        }
    }

    pub fn skip_reason(&self) -> Option<TextProcessingSkipReason> {
        self.skip_reason
    }
}

fn skipped_text_processing(reason: TextProcessingSkipReason) -> TextProcessingRuntime {
    TextProcessingRuntime {
        mode: TextProcessingRuntimeMode::Disabled,
        skip_reason: Some(reason),
    }
}

fn enabled_text_processing(mode: TextProcessingRuntimeMode) -> TextProcessingRuntime {
    TextProcessingRuntime {
        mode,
        skip_reason: None,
    }
}

pub fn text_processing_provider_runtime(
    settings: &AppSettings,
    intent: TextProcessingIntent,
) -> TextProcessingRuntime {
    let (requested, require_post_processing_enabled) = match intent {
        TextProcessingIntent::DictationPostProcessing { requested } => (requested, true),
        TextProcessingIntent::ExplicitTransform => (true, false),
    };

    if !requested {
        return skipped_text_processing(TextProcessingSkipReason::NotRequested);
    }

    if require_post_processing_enabled && !settings.post_process_enabled {
        return skipped_text_processing(TextProcessingSkipReason::DisabledBySettings);
    }

    if settings.local_llm.enabled {
        return enabled_text_processing(TextProcessingRuntimeMode::ManagedLocal);
    }

    let Some(provider) = settings.active_post_process_provider() else {
        return skipped_text_processing(TextProcessingSkipReason::MissingProvider);
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .map(String::as_str)
        .unwrap_or_default();
    if model.trim().is_empty() {
        return skipped_text_processing(TextProcessingSkipReason::MissingModel);
    }

    let insecure_lan = settings.allow_insecure_lan_post_process
        && is_insecure_lan_post_process_base_url(&provider.base_url);
    let api_key = if insecure_lan {
        ""
    } else {
        settings
            .post_process_api_keys
            .get(&provider.id)
            .map(String::as_str)
            .unwrap_or_default()
    };
    if !is_local_post_process_base_url(&provider.base_url)
        && !insecure_lan
        && api_key.trim().is_empty()
    {
        return skipped_text_processing(TextProcessingSkipReason::RemoteMissingApiKey);
    }

    enabled_text_processing(TextProcessingRuntimeMode::ApiProvider(
        TextProcessingApiRuntime {
            provider: provider.clone(),
            model: model.to_string(),
            api_key: api_key.to_string(),
        },
    ))
}

#[derive(Clone, Debug)]
pub struct PostProcessingApiRuntime {
    pub provider: PostProcessProvider,
    pub model: String,
    pub prompt: LLMPrompt,
    pub api_key: String,
}

#[derive(Clone, Debug)]
pub enum PostProcessingRuntimeMode {
    Disabled,
    ManagedLocal,
    ApiProvider(PostProcessingApiRuntime),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostProcessingSkipReason {
    NotRequested,
    DisabledBySettings,
    MissingProvider,
    MissingModel,
    MissingPrompt,
    EmptyPrompt,
    RemoteMissingApiKey,
}

#[derive(Clone, Debug)]
pub struct PostProcessingRuntime {
    mode: PostProcessingRuntimeMode,
    skip_reason: Option<PostProcessingSkipReason>,
}

impl PostProcessingRuntime {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn should_run(&self) -> bool {
        self.skip_reason.is_none() && !matches!(self.mode, PostProcessingRuntimeMode::Disabled)
    }

    pub fn uses_managed_local(&self) -> bool {
        matches!(self.mode, PostProcessingRuntimeMode::ManagedLocal)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn should_attempt_api(&self) -> bool {
        self.should_run() && matches!(self.mode, PostProcessingRuntimeMode::ApiProvider(_))
    }

    pub fn api_provider(&self) -> Option<&PostProcessingApiRuntime> {
        match &self.mode {
            PostProcessingRuntimeMode::ApiProvider(runtime) => Some(runtime),
            PostProcessingRuntimeMode::Disabled | PostProcessingRuntimeMode::ManagedLocal => None,
        }
    }

    pub fn skip_reason(&self) -> Option<PostProcessingSkipReason> {
        self.skip_reason
    }
}

fn skipped(reason: PostProcessingSkipReason) -> PostProcessingRuntime {
    PostProcessingRuntime {
        mode: PostProcessingRuntimeMode::Disabled,
        skip_reason: Some(reason),
    }
}

fn enabled(mode: PostProcessingRuntimeMode) -> PostProcessingRuntime {
    PostProcessingRuntime {
        mode,
        skip_reason: None,
    }
}

pub fn post_processing_runtime(settings: &AppSettings, requested: bool) -> PostProcessingRuntime {
    let text_runtime = text_processing_provider_runtime(
        settings,
        TextProcessingIntent::DictationPostProcessing { requested },
    );

    if text_runtime.uses_managed_local() {
        return enabled(PostProcessingRuntimeMode::ManagedLocal);
    }

    let Some(api_runtime) = text_runtime.api_provider() else {
        return skipped(match text_runtime.skip_reason() {
            Some(TextProcessingSkipReason::NotRequested) => PostProcessingSkipReason::NotRequested,
            Some(TextProcessingSkipReason::DisabledBySettings) => {
                PostProcessingSkipReason::DisabledBySettings
            }
            Some(TextProcessingSkipReason::MissingProvider) => {
                PostProcessingSkipReason::MissingProvider
            }
            Some(TextProcessingSkipReason::MissingModel) => PostProcessingSkipReason::MissingModel,
            Some(TextProcessingSkipReason::RemoteMissingApiKey) => {
                PostProcessingSkipReason::RemoteMissingApiKey
            }
            None => PostProcessingSkipReason::MissingProvider,
        });
    };

    let Some(prompt_id) = &settings.post_process_selected_prompt_id else {
        return skipped(PostProcessingSkipReason::MissingPrompt);
    };
    let Some(prompt) = settings
        .post_process_prompts
        .iter()
        .find(|prompt| &prompt.id == prompt_id)
    else {
        return skipped(PostProcessingSkipReason::MissingPrompt);
    };
    if prompt.prompt.trim().is_empty() {
        return skipped(PostProcessingSkipReason::EmptyPrompt);
    }

    enabled(PostProcessingRuntimeMode::ApiProvider(
        PostProcessingApiRuntime {
            provider: api_runtime.provider.clone(),
            model: api_runtime.model.clone(),
            prompt: prompt.clone(),
            api_key: api_runtime.api_key.clone(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insecure_lan_opt_in_never_carries_an_api_key() {
        let mut settings = crate::settings::get_default_settings();
        settings.post_process_enabled = true;
        settings.allow_insecure_lan_post_process = true;
        settings.post_process_provider_id = "custom".to_string();
        settings
            .post_process_provider_mut("custom")
            .expect("custom provider")
            .base_url = "http://203.0.113.20:8000/v1".to_string();
        settings
            .post_process_models
            .insert("custom".to_string(), "test-model".to_string());
        settings
            .post_process_api_keys
            .insert("custom".to_string(), "secret-key".to_string());

        let runtime = text_processing_provider_runtime(
            &settings,
            TextProcessingIntent::DictationPostProcessing { requested: true },
        );
        let provider = runtime
            .api_provider()
            .expect("opted-in insecure LAN provider should remain usable");
        assert!(provider.api_key.is_empty());
    }
}
