use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub const APPLE_INTELLIGENCE_PROVIDER_ID: &str = "apple_intelligence";
pub const APPLE_INTELLIGENCE_DEFAULT_MODEL_ID: &str = "Apple Intelligence";

#[cfg_attr(not(test), allow(dead_code))]
static SETTINGS_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsWriteDomain {
    General,
    Audio,
    Insertion,
    Privacy,
    Models,
    PostProcessing,
    Diagnostics,
    Adaptive,
    Shortcuts,
}

impl SettingsWriteDomain {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SettingsWriteDomain::General => "general",
            SettingsWriteDomain::Audio => "audio",
            SettingsWriteDomain::Insertion => "insertion",
            SettingsWriteDomain::Privacy => "privacy",
            SettingsWriteDomain::Models => "models",
            SettingsWriteDomain::PostProcessing => "post_processing",
            SettingsWriteDomain::Diagnostics => "diagnostics",
            SettingsWriteDomain::Adaptive => "adaptive",
            SettingsWriteDomain::Shortcuts => "shortcuts",
        }
    }
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both old numeric format (1-5) and new string format ("trace", "debug", etc.)
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl<'de> Visitor<'de> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

impl From<LogLevel> for tauri_plugin_log::LogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
            LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
            LogLevel::Info => tauri_plugin_log::LogLevel::Info,
            LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
            LogLevel::Error => tauri_plugin_log::LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

pub fn is_unbound_shortcut(raw: &str) -> bool {
    raw.trim().is_empty()
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum DictionaryEntrySource {
    Manual,
    AutoLearned,
    Imported,
}

impl Default for DictionaryEntrySource {
    fn default() -> Self {
        DictionaryEntrySource::Manual
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum DictionaryEntryPriority {
    Normal,
    Starred,
}

impl Default for DictionaryEntryPriority {
    fn default() -> Self {
        DictionaryEntryPriority::Normal
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct DictionaryEntry {
    pub id: String,
    pub phrase: String,
    #[serde(default)]
    pub replacement_of: Option<String>,
    #[serde(default)]
    pub source: DictionaryEntrySource,
    #[serde(default)]
    pub priority: DictionaryEntryPriority,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default)]
    pub user_confirmed: bool,
    #[serde(default)]
    pub needs_review: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct LearnCandidate {
    #[serde(default)]
    pub replacement_of: Option<String>,
    pub phrase: String,
    #[serde(default)]
    pub occurrences: u32,
    #[serde(default)]
    pub last_evidence_session: Option<String>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LLMPrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub models_endpoint: Option<String>,
    #[serde(default)]
    pub supports_structured_output: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranslationRoute {
    Auto,
    DirectSpeech,
    TextAfterTranscription,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct TranslationRequestSettings {
    pub source_language: String,
    pub target_language: String,
    pub route: TranslationRoute,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    Top,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    DontModify,
    CopyToClipboard,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    Enter,
    CtrlEnter,
    CmdEnter,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    Never,
    PreserveLimit,
    Days3,
    Weeks2,
    Months3,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    VerbatimKeys,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::VerbatimKeys;
    }
}

impl Default for ModelUnloadTimeout {
    fn default() -> Self {
        ModelUnloadTimeout::Min5
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        // Default to CtrlV for macOS and Windows, Direct for Linux
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl Default for ClipboardHandling {
    fn default() -> Self {
        ClipboardHandling::DontModify
    }
}

impl Default for AutoSubmitKey {
    fn default() -> Self {
        AutoSubmitKey::Enter
    }
}

impl ModelUnloadTimeout {
    #[cfg_attr(not(feature = "transcribe-rs-engine"), allow(dead_code))]
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Sec15 => Some(0), // Special case for debug - handled separately
        }
    }

    #[cfg_attr(not(feature = "transcribe-rs-engine"), allow(dead_code))]
    pub fn to_seconds(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Sec15 => Some(15),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(&self) -> &'static str {
        match self {
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(&self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(&self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

impl Default for TypingTool {
    fn default() -> Self {
        TypingTool::Auto
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum WhisperAcceleratorSetting {
    Auto,
    Cpu,
    Gpu,
}

impl Default for WhisperAcceleratorSetting {
    fn default() -> Self {
        WhisperAcceleratorSetting::Auto
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrtAcceleratorSetting {
    Auto,
    Cpu,
    Cuda,
    #[serde(rename = "directml")]
    DirectMl,
    Rocm,
}

impl Default for OrtAcceleratorSetting {
    fn default() -> Self {
        OrtAcceleratorSetting::Auto
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum DictationLanguageMode {
    Auto,
    Single,
    Multilingual,
}

impl Default for DictationLanguageMode {
    fn default() -> Self {
        DictationLanguageMode::Auto
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum FormattingLevel {
    None,
    Light,
    Medium,
    High,
}

impl Default for FormattingLevel {
    fn default() -> Self {
        FormattingLevel::Light
    }
}

#[derive(Clone, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub(crate) struct SecretMap(HashMap<String, String>);

impl fmt::Debug for SecretMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted: HashMap<&String, &str> = self
            .0
            .iter()
            .map(|(k, v)| (k, if v.is_empty() { "" } else { "[REDACTED]" }))
            .collect();
        redacted.fmt(f)
    }
}

impl std::ops::Deref for SecretMap {
    type Target = HashMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SecretMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/* useful for composing the initial JSON in the store ------------------ */
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppSettings {
    pub bindings: HashMap<String, ShortcutBinding>,
    pub push_to_talk: bool,
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_update_checks_enabled")]
    pub update_checks_enabled: bool,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default)]
    pub translation_enabled: bool,
    #[serde(default)]
    pub translation_request: Option<TranslationRequestSettings>,
    #[serde(default)]
    pub translation_provider_id: Option<String>,
    #[serde(default)]
    pub translation_model_id: Option<String>,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default)]
    pub dictation_language_mode: DictationLanguageMode,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    #[serde(default)]
    pub docked_pill_enabled: bool,
    #[serde(default = "default_warn_on_elevated_target")]
    pub warn_on_elevated_target: bool,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub dictionary_entries: Vec<DictionaryEntry>,
    #[serde(default)]
    pub dictionary_auto_learn_suppressed: Vec<String>,
    #[serde(default)]
    pub dictionary_learn_candidates: Vec<LearnCandidate>,
    #[serde(default)]
    pub dictionary_schema_version: u32,
    #[serde(default = "default_auto_add_dictionary_words")]
    pub auto_add_dictionary_words: bool,
    #[serde(default)]
    pub snippets: Vec<crate::snippets::SnippetEntry>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_enabled")]
    pub history_enabled: bool,
    #[serde(default = "default_recordings_enabled")]
    pub recordings_enabled: bool,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default = "default_post_process_enabled")]
    pub post_process_enabled: bool,
    #[serde(default)]
    pub formatting_level: FormattingLevel,
    #[serde(default = "default_post_process_provider_id")]
    pub post_process_provider_id: String,
    #[serde(default = "default_post_process_providers")]
    pub post_process_providers: Vec<PostProcessProvider>,
    #[serde(default = "default_post_process_api_keys")]
    pub post_process_api_keys: SecretMap,
    #[serde(default = "default_post_process_models")]
    pub post_process_models: HashMap<String, String>,
    #[serde(default = "default_post_process_prompts")]
    pub post_process_prompts: Vec<LLMPrompt>,
    #[serde(default)]
    pub post_process_selected_prompt_id: Option<String>,
    #[serde(default)]
    pub local_llm: crate::local_llm::LocalLlmSettings,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    #[serde(default)]
    pub adaptive_profiles_enabled: bool,
    #[serde(default)]
    pub context_awareness_enabled: bool,
    #[serde(default)]
    pub context_nearby_text_enabled: bool,
    #[serde(default = "default_adaptive_language_shortlist")]
    pub adaptive_language_shortlist: Vec<String>,
    #[serde(default = "default_adaptive_default_profile_id")]
    pub adaptive_default_profile_id: String,
    #[serde(default = "default_adaptive_profiles")]
    pub adaptive_profiles: Vec<crate::adaptive::profile::AdaptiveProfile>,
    #[serde(default = "default_adaptive_correction_memory_enabled")]
    pub adaptive_correction_memory_enabled: bool,
    #[serde(default = "default_adaptive_private_app_patterns")]
    pub adaptive_private_app_patterns: Vec<String>,
    #[serde(default)]
    pub whisper_accelerator: WhisperAcceleratorSetting,
    #[serde(default)]
    pub ort_accelerator: OrtAcceleratorSetting,
    #[serde(default = "default_whisper_gpu_device")]
    pub whisper_gpu_device: i32,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
}

fn default_model() -> String {
    "".to_string()
}

fn default_adaptive_language_shortlist() -> Vec<String> {
    vec!["en".to_string(), "ar".to_string()]
}

fn default_adaptive_default_profile_id() -> String {
    "default_clean".to_string()
}

fn default_adaptive_profiles() -> Vec<crate::adaptive::profile::AdaptiveProfile> {
    crate::adaptive::profile::default_profiles()
}

fn default_adaptive_correction_memory_enabled() -> bool {
    true
}

fn default_adaptive_private_app_patterns() -> Vec<String> {
    vec![
        "1password".to_string(),
        "bitwarden".to_string(),
        "keepass".to_string(),
    ]
}

fn default_always_on_microphone() -> bool {
    false
}

fn default_translate_to_english() -> bool {
    false
}

fn default_start_hidden() -> bool {
    false
}

fn default_true() -> bool {
    true
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_update_checks_enabled() -> bool {
    true
}

fn default_selected_language() -> String {
    "auto".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    #[cfg(target_os = "linux")]
    return OverlayPosition::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayPosition::Bottom;
}

fn default_debug_mode() -> bool {
    false
}

fn default_warn_on_elevated_target() -> bool {
    true
}

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

fn default_word_correction_threshold() -> f64 {
    0.18
}

fn default_auto_add_dictionary_words() -> bool {
    false
}

fn default_paste_delay_ms() -> u64 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_history_enabled() -> bool {
    true
}

fn default_recordings_enabled() -> bool {
    true
}

fn default_history_limit() -> usize {
    5
}

fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
}

fn default_post_process_enabled() -> bool {
    false
}

fn default_app_language() -> String {
    tauri_plugin_os::locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_post_process_provider_id() -> String {
    "openai".to_string()
}

fn default_post_process_providers() -> Vec<PostProcessProvider> {
    let mut providers = vec![
        PostProcessProvider {
            id: "openai".to_string(),
            label: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "zai".to_string(),
            label: "Z.AI".to_string(),
            base_url: "https://api.z.ai/api/paas/v4".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "anthropic".to_string(),
            label: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "groq".to_string(),
            label: "Groq".to_string(),
            base_url: "https://api.groq.com/openai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "cerebras".to_string(),
            label: "Cerebras".to_string(),
            base_url: "https://api.cerebras.ai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "lm_studio".to_string(),
            label: "LM Studio".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            allow_base_url_edit: true,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "ollama".to_string(),
            label: "Ollama".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            allow_base_url_edit: true,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "vllm".to_string(),
            label: "vLLM".to_string(),
            base_url: "http://localhost:8000/v1".to_string(),
            allow_base_url_edit: true,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
    ];

    // Note: We always include Apple Intelligence on macOS ARM64 without checking availability
    // at startup. The availability check is deferred to when the user actually tries to use it
    // (in actions.rs). This prevents crashes on macOS 26.x beta where accessing
    // SystemLanguageModel.default during early app initialization causes SIGABRT.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        providers.push(PostProcessProvider {
            id: APPLE_INTELLIGENCE_PROVIDER_ID.to_string(),
            label: "Apple Intelligence".to_string(),
            base_url: "apple-intelligence://local".to_string(),
            allow_base_url_edit: false,
            models_endpoint: None,
            supports_structured_output: true,
        });
    }

    // AWS Bedrock via Mantle (OpenAI-compatible endpoint)
    providers.push(PostProcessProvider {
        id: "bedrock_mantle".to_string(),
        label: "AWS Bedrock (Mantle)".to_string(),
        base_url: "https://bedrock-mantle.us-east-1.api.aws/v1".to_string(),
        allow_base_url_edit: false,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: true,
    });

    // Custom provider always comes last
    providers.push(PostProcessProvider {
        id: "custom".to_string(),
        label: "Custom".to_string(),
        base_url: "http://localhost:11434/v1".to_string(),
        allow_base_url_edit: true,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: false,
    });

    providers
}

fn default_post_process_api_keys() -> SecretMap {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(provider.id, String::new());
    }
    SecretMap(map)
}

fn default_model_for_provider(provider_id: &str) -> String {
    if provider_id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return APPLE_INTELLIGENCE_DEFAULT_MODEL_ID.to_string();
    }
    String::new()
}

pub fn is_local_post_process_base_url(base_url: &str) -> bool {
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

fn default_post_process_models() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(
            provider.id.clone(),
            default_model_for_provider(&provider.id),
        );
    }
    map
}

fn known_openai_compatible_port(provider_id: &str, port: Option<u16>) -> bool {
    matches!(provider_id, "lm_studio" | "ollama" | "vllm")
        || matches!(port, Some(1234 | 11434 | 8000))
}

fn normalize_known_openai_compatible_base_url(provider_id: &str, base_url: &str) -> Option<String> {
    let Ok(mut parsed) = reqwest::Url::parse(base_url.trim()) else {
        return None;
    };

    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }

    if !known_openai_compatible_port(provider_id, parsed.port_or_known_default()) {
        return None;
    }

    let path = parsed.path().trim_end_matches('/');
    if !path.is_empty() && path != "/" {
        return None;
    }

    parsed.set_path("/v1");
    Some(parsed.to_string().trim_end_matches('/').to_string())
}

fn default_post_process_prompts() -> Vec<LLMPrompt> {
    vec![LLMPrompt {
        id: "default_improve_transcriptions".to_string(),
        name: "Improve Transcriptions".to_string(),
        prompt: "Clean this transcript:\n1. Fix spelling, capitalization, and punctuation errors\n2. Convert number words to digits (twenty-five → 25, ten percent → 10%, five dollars → $5)\n3. Replace spoken punctuation with symbols (period → ., comma → ,, question mark → ?)\n4. Remove filler words (um, uh, like as filler)\n5. Keep the language in the original version (if it was french, keep it in french for example)\n\nPreserve exact meaning and word order. Do not paraphrase or reorder content.\n\nReturn only the cleaned transcript.\n\nTranscript:\n${output}".to_string(),
    }]
}

fn default_whisper_gpu_device() -> i32 {
    -1 // auto
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

fn ensure_post_process_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    for provider in default_post_process_providers() {
        // Use match to do a single lookup - either sync existing or add new
        match settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == provider.id)
        {
            Some(existing) => {
                // Sync supports_structured_output field for existing providers (migration)
                if existing.supports_structured_output != provider.supports_structured_output {
                    debug!(
                        "Updating supports_structured_output for provider '{}' from {} to {}",
                        provider.id,
                        existing.supports_structured_output,
                        provider.supports_structured_output
                    );
                    existing.supports_structured_output = provider.supports_structured_output;
                    changed = true;
                }
                if existing.allow_base_url_edit != provider.allow_base_url_edit {
                    existing.allow_base_url_edit = provider.allow_base_url_edit;
                    changed = true;
                }
                if existing.models_endpoint != provider.models_endpoint {
                    existing.models_endpoint = provider.models_endpoint.clone();
                    changed = true;
                }
            }
            None => {
                // Provider doesn't exist, add it
                settings.post_process_providers.push(provider.clone());
                changed = true;
            }
        }

        if !settings.post_process_api_keys.contains_key(&provider.id) {
            settings
                .post_process_api_keys
                .insert(provider.id.clone(), String::new());
            changed = true;
        }

        let default_model = default_model_for_provider(&provider.id);
        match settings.post_process_models.get_mut(&provider.id) {
            Some(existing) => {
                if existing.is_empty() && !default_model.is_empty() {
                    *existing = default_model.clone();
                    changed = true;
                }
            }
            None => {
                settings
                    .post_process_models
                    .insert(provider.id.clone(), default_model);
                changed = true;
            }
        }
    }

    for provider in settings.post_process_providers.iter_mut() {
        if let Some(normalized) =
            normalize_known_openai_compatible_base_url(&provider.id, &provider.base_url)
        {
            if normalized != provider.base_url {
                debug!(
                    "Normalizing post-process provider '{}' base URL from '{}' to '{}'",
                    provider.id, provider.base_url, normalized
                );
                provider.base_url = normalized;
                changed = true;
            }
        }
    }

    changed
}

fn ensure_adaptive_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    let original_profile_count = settings.adaptive_profiles.len();
    settings
        .adaptive_profiles
        .retain(|profile| profile.id != "translation");
    if settings.adaptive_profiles.len() != original_profile_count {
        changed = true;
    }

    for default_profile in default_adaptive_profiles() {
        if !settings
            .adaptive_profiles
            .iter()
            .any(|profile| profile.id == default_profile.id)
        {
            settings.adaptive_profiles.push(default_profile);
            changed = true;
        }
    }

    if settings.adaptive_language_shortlist.is_empty() {
        settings.adaptive_language_shortlist = default_adaptive_language_shortlist();
        changed = true;
    }

    if settings.adaptive_default_profile_id.is_empty() {
        settings.adaptive_default_profile_id = default_adaptive_default_profile_id();
        changed = true;
    }

    if settings.adaptive_default_profile_id == "translation" {
        settings.adaptive_default_profile_id = default_adaptive_default_profile_id();
        changed = true;
    }

    if settings.adaptive_private_app_patterns.is_empty() {
        settings.adaptive_private_app_patterns = default_adaptive_private_app_patterns();
        changed = true;
    }

    changed
}

fn ensure_translation_defaults(settings: &mut AppSettings) -> bool {
    if settings.translate_to_english && settings.translation_request.is_none() {
        settings.translation_enabled = true;
        settings.translation_request = Some(TranslationRequestSettings {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            route: TranslationRoute::Auto,
        });
        return true;
    }

    if !settings.translation_enabled && settings.translation_request.is_some() {
        settings.translation_request = None;
        return true;
    }

    false
}

fn settings_value_has_key(settings_value: Option<&serde_json::Value>, key: &str) -> bool {
    settings_value
        .and_then(serde_json::Value::as_object)
        .is_some_and(|settings| settings.contains_key(key))
}

#[cfg_attr(not(test), allow(dead_code))]
fn ensure_dictionary_defaults(settings: &mut AppSettings) -> bool {
    crate::dictionary::sync_legacy_custom_words(settings)
}

fn settings_value_has_dictionary_entries(settings_value: Option<&serde_json::Value>) -> bool {
    settings_value_has_key(settings_value, "dictionary_entries")
}

fn ensure_dictionary_defaults_for_loaded_value(
    settings: &mut AppSettings,
    settings_value: Option<&serde_json::Value>,
) -> bool {
    let migrated = crate::dictionary::migrate_dictionary_v1(settings);
    let synced = crate::dictionary::sync_legacy_custom_words_with_migration(
        settings,
        !settings_value_has_dictionary_entries(settings_value),
    );
    migrated || synced
}

fn ensure_snippet_defaults(settings: &mut AppSettings) -> bool {
    crate::snippets::sync_snippets(settings)
}

pub fn set_translation_target_language(settings: &mut AppSettings, target_language: String) {
    let mut request = settings
        .translation_request
        .clone()
        .unwrap_or(TranslationRequestSettings {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            route: TranslationRoute::Auto,
        });

    request.target_language = target_language.clone();
    settings.translation_enabled = true;
    settings.translate_to_english = target_language == "en";
    settings.translation_request = Some(request);
}

pub const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Post-Processing".to_string(),
            description: "Converts your speech into text and applies AI post-processing."
                .to_string(),
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: default_post_process_shortcut.to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );
    for (id, name, description) in [
        (
            "transform_polish",
            "Polish Selected Text",
            "Transforms the selected text with the configured post-processing provider.",
        ),
        (
            "transform_make_concise",
            "Make Selected Text Concise",
            "Makes the selected text more concise with the configured post-processing provider.",
        ),
        (
            "transform_turn_into_list",
            "Turn Selected Text Into List",
            "Turns the selected text into a list with the configured post-processing provider.",
        ),
        (
            "transform_translate",
            "Translate Selected Text",
            "Translates the selected text to your configured translation target.",
        ),
        (
            "transform_prompt_engineer",
            "Prompt Engineer Selected Text",
            "Rewrites the selected text as a clearer prompt.",
        ),
    ] {
        bindings.insert(
            id.to_string(),
            ShortcutBinding {
                id: id.to_string(),
                name: name.to_string(),
                description: description.to_string(),
                default_binding: String::new(),
                current_binding: String::new(),
            },
        );
    }

    AppSettings {
        bindings,
        push_to_talk: true,
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        selected_model: "".to_string(),
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        translation_enabled: false,
        translation_request: None,
        translation_provider_id: None,
        translation_model_id: None,
        selected_language: "auto".to_string(),
        dictation_language_mode: DictationLanguageMode::default(),
        overlay_position: default_overlay_position(),
        docked_pill_enabled: false,
        warn_on_elevated_target: true,
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        dictionary_entries: Vec::new(),
        dictionary_auto_learn_suppressed: Vec::new(),
        dictionary_learn_candidates: Vec::new(),
        dictionary_schema_version: 0,
        auto_add_dictionary_words: default_auto_add_dictionary_words(),
        snippets: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        history_enabled: default_history_enabled(),
        recordings_enabled: default_recordings_enabled(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
        paste_method: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        post_process_enabled: default_post_process_enabled(),
        formatting_level: FormattingLevel::default(),
        post_process_provider_id: default_post_process_provider_id(),
        post_process_providers: default_post_process_providers(),
        post_process_api_keys: default_post_process_api_keys(),
        post_process_models: default_post_process_models(),
        post_process_prompts: default_post_process_prompts(),
        post_process_selected_prompt_id: None,
        local_llm: crate::local_llm::LocalLlmSettings::default(),
        mute_while_recording: true,
        append_trailing_space: false,
        app_language: default_app_language(),
        experimental_enabled: false,
        lazy_stream_close: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        custom_filler_words: None,
        adaptive_profiles_enabled: false,
        context_awareness_enabled: false,
        context_nearby_text_enabled: false,
        adaptive_language_shortlist: default_adaptive_language_shortlist(),
        adaptive_default_profile_id: default_adaptive_default_profile_id(),
        adaptive_profiles: default_adaptive_profiles(),
        adaptive_correction_memory_enabled: default_adaptive_correction_memory_enabled(),
        adaptive_private_app_patterns: default_adaptive_private_app_patterns(),
        whisper_accelerator: WhisperAcceleratorSetting::default(),
        ort_accelerator: OrtAcceleratorSetting::default(),
        whisper_gpu_device: default_whisper_gpu_device(),
        extra_recording_buffer_ms: 0,
    }
}

impl AppSettings {
    pub fn apply_dictation_language_mode(
        &mut self,
        mode: DictationLanguageMode,
        selected_language: Option<String>,
        languages: Vec<String>,
    ) -> Result<(), String> {
        let cleaned = languages.into_iter().fold(Vec::new(), |mut acc, language| {
            let language = language.trim().to_lowercase();
            if !language.is_empty() && language != "auto" && !acc.contains(&language) {
                acc.push(language);
            }
            acc
        });

        self.dictation_language_mode = mode;
        match mode {
            DictationLanguageMode::Single => {
                let language = selected_language
                    .as_deref()
                    .map(str::trim)
                    .map(str::to_lowercase)
                    .filter(|language| !language.is_empty() && language != "auto")
                    .or_else(|| cleaned.first().cloned());
                let Some(language) = language else {
                    return Err("Single-language mode requires a language".to_string());
                };
                let shortlist = if cleaned.is_empty() {
                    vec![language.clone()]
                } else {
                    cleaned
                };
                if !shortlist.contains(&language) {
                    return Err(
                        "Single-language mode requires the selected language in the language list"
                            .to_string(),
                    );
                }
                self.selected_language = language.clone();
                self.adaptive_language_shortlist = shortlist;
            }
            DictationLanguageMode::Multilingual => {
                if cleaned.len() < 2 {
                    return Err("Multilingual mode requires at least two languages".to_string());
                }
                self.selected_language = "auto".to_string();
                self.adaptive_language_shortlist = cleaned;
            }
            DictationLanguageMode::Auto => {
                self.selected_language = "auto".to_string();
                self.adaptive_language_shortlist = if cleaned.is_empty() {
                    default_adaptive_language_shortlist()
                } else {
                    cleaned
                };
            }
        }
        Ok(())
    }

    pub fn active_post_process_provider(&self) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == self.post_process_provider_id)
    }

    pub fn post_process_provider(&self, provider_id: &str) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == provider_id)
    }

    pub fn post_process_provider_mut(
        &mut self,
        provider_id: &str,
    ) -> Option<&mut PostProcessProvider> {
        self.post_process_providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
    }

    #[cfg_attr(not(feature = "transcribe-rs-engine"), allow(dead_code))]
    pub fn dictionary_phrases(&self) -> Vec<String> {
        crate::dictionary::dictionary_phrases(&self.dictionary_entries)
    }
}

fn existing_settings_log_message(settings: &AppSettings) -> String {
    format!(
        "Found existing settings ({} bindings, {} dictionary entries, {} custom words, {} snippets, {} post-process providers)",
        settings.bindings.len(),
        settings.dictionary_entries.len(),
        settings.custom_words.len(),
        settings.snippets.len(),
        settings.post_process_providers.len()
    )
}

fn recover_settings_from_unparseable_value(settings_value: &serde_json::Value) -> AppSettings {
    let default_settings = get_default_settings();
    let default_value = match serde_json::to_value(&default_settings) {
        Ok(value) => value,
        Err(_) => return default_settings,
    };
    let Some(source) = settings_value.as_object() else {
        return default_settings;
    };

    let mut merged_value = default_value.clone();
    let Some(merged_object) = merged_value.as_object_mut() else {
        return default_settings;
    };

    for (key, value) in source {
        let mut candidate = default_value.clone();
        if let Some(candidate_object) = candidate.as_object_mut() {
            candidate_object.insert(key.clone(), value.clone());
        }

        if serde_json::from_value::<AppSettings>(candidate).is_ok() {
            merged_object.insert(key.clone(), value.clone());
        }
    }

    serde_json::from_value(merged_value).unwrap_or(default_settings)
}

fn settings_store_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    let store_path = crate::portable::store_path(SETTINGS_STORE_PATH);
    if store_path.is_absolute() {
        return Ok(store_path);
    }

    crate::portable::resolve_app_data(app, SETTINGS_STORE_PATH)
        .map_err(|err| format!("resolve settings store path: {err}"))
}

fn settings_backup_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let settings_path = settings_store_file_path(app)?;
    settings_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "settings store path has no parent directory".to_string())
}

fn backup_settings_value_to_dir(
    backup_dir: &Path,
    settings_value: &serde_json::Value,
) -> Result<PathBuf, String> {
    fs::create_dir_all(backup_dir)
        .map_err(|err| format!("create settings backup directory: {err}"))?;
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S%3f");
    let backup_path = backup_dir.join(format!("settings_store.parse-error.{timestamp}.json"));
    let contents = serde_json::to_string_pretty(settings_value)
        .map_err(|err| format!("serialize settings backup: {err}"))?;
    fs::write(&backup_path, contents).map_err(|err| format!("write settings backup: {err}"))?;
    Ok(backup_path)
}

fn backup_unparseable_settings(app: &AppHandle, settings_value: &serde_json::Value) {
    match settings_backup_directory(app)
        .and_then(|dir| backup_settings_value_to_dir(&dir, settings_value))
    {
        Ok(path) => warn!(
            "Backed up unparseable settings before recovery to {}",
            path.display()
        ),
        Err(err) => warn!("Failed to back up unparseable settings: {}", err),
    }
}

fn recover_unparseable_settings(
    app: &AppHandle,
    settings_value: &serde_json::Value,
    error: &serde_json::Error,
) -> AppSettings {
    warn!("Failed to parse settings: {}", error);
    backup_unparseable_settings(app, settings_value);
    recover_settings_from_unparseable_value(settings_value)
}

fn ensure_binding_defaults(settings: &mut AppSettings) -> bool {
    let default_settings = get_default_settings();
    let mut changed = false;

    for (key, value) in default_settings.bindings {
        if !settings.bindings.contains_key(&key) {
            debug!("Adding missing binding: {}", key);
            settings.bindings.insert(key, value);
            changed = true;
        }
    }

    changed
}

pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    // Initialize store
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    let mut settings_value_for_defaults = None;
    let mut settings = if let Some(settings_value) = store.get("settings") {
        settings_value_for_defaults = Some(settings_value.clone());
        // Parse the entire settings object
        match serde_json::from_value::<AppSettings>(settings_value.clone()) {
            Ok(mut settings) => {
                debug!("{}", existing_settings_log_message(&settings));
                let updated = ensure_binding_defaults(&mut settings);

                if updated {
                    debug!("Settings updated with new bindings");
                    store.set("settings", serde_json::to_value(&settings).unwrap());
                }

                settings
            }
            Err(e) => {
                let recovered_settings = recover_unparseable_settings(app, &settings_value, &e);
                store.set(
                    "settings",
                    serde_json::to_value(&recovered_settings).unwrap(),
                );
                recovered_settings
            }
        }
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    let binding_changed = ensure_binding_defaults(&mut settings);
    let post_process_changed = ensure_post_process_defaults(&mut settings);
    let adaptive_changed = ensure_adaptive_defaults(&mut settings);
    let translation_changed = ensure_translation_defaults(&mut settings);
    let dictionary_changed = ensure_dictionary_defaults_for_loaded_value(
        &mut settings,
        settings_value_for_defaults.as_ref(),
    );
    let snippet_changed = ensure_snippet_defaults(&mut settings);
    let credentials_changed = crate::credentials::prepare_post_process_api_keys_for_store(
        &mut settings,
        crate::credentials::CredentialStoreFailurePolicy::PreserveLegacyValue,
    );
    if binding_changed
        || post_process_changed
        || adaptive_changed
        || translation_changed
        || dictionary_changed
        || snippet_changed
        || credentials_changed
    {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    crate::credentials::hydrate_post_process_api_keys(&mut settings);

    settings
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    let mut settings_value_for_defaults = None;
    let mut settings = if let Some(settings_value) = store.get("settings") {
        settings_value_for_defaults = Some(settings_value.clone());
        serde_json::from_value::<AppSettings>(settings_value.clone()).unwrap_or_else(|err| {
            let recovered_settings = recover_unparseable_settings(app, &settings_value, &err);
            store.set(
                "settings",
                serde_json::to_value(&recovered_settings).unwrap(),
            );
            recovered_settings
        })
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    let binding_changed = ensure_binding_defaults(&mut settings);
    let post_process_changed = ensure_post_process_defaults(&mut settings);
    let adaptive_changed = ensure_adaptive_defaults(&mut settings);
    let translation_changed = ensure_translation_defaults(&mut settings);
    let dictionary_changed = ensure_dictionary_defaults_for_loaded_value(
        &mut settings,
        settings_value_for_defaults.as_ref(),
    );
    let snippet_changed = ensure_snippet_defaults(&mut settings);
    if binding_changed
        || post_process_changed
        || adaptive_changed
        || translation_changed
        || dictionary_changed
        || snippet_changed
    {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

/// Pure application of a mutation to an already-loaded settings value.
/// Kept separate so it is unit-testable without an AppHandle.
pub fn apply_settings_mutation<T>(
    settings: &mut AppSettings,
    f: impl FnOnce(&mut AppSettings) -> T,
) -> T {
    f(settings)
}

/// The ONLY public way to mutate persisted settings. Holds the write lock across the
/// whole read-modify-write so concurrent mutations cannot lost-update each other.
/// Do NOT `.await` or emit Tauri events inside `f`; emit after this returns.
pub fn mutate_settings_locked<T>(app: &AppHandle, f: impl FnOnce(&mut AppSettings) -> T) -> T {
    let _guard = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut settings = get_settings(app);
    let result = apply_settings_mutation(&mut settings, f);
    write_settings(app, settings);
    result
}

// NOTE: `write_settings` and `get_settings` are the lock-free primitives. All MUTATION
// paths must go through `mutate_settings_locked`. A deny-list test guards this (added in a
// later task).
pub fn write_settings(app: &AppHandle, mut settings: AppSettings) {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    crate::dictionary::sync_legacy_custom_words(&mut settings);
    crate::snippets::sync_snippets(&mut settings);
    crate::credentials::prepare_post_process_api_keys_for_store(
        &mut settings,
        crate::credentials::CredentialStoreFailurePolicy::RejectNewValue,
    );
    store.set("settings", serde_json::to_value(&settings).unwrap());
}

pub(crate) fn write_settings_domain<F>(
    app: &AppHandle,
    domain: SettingsWriteDomain,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings),
{
    try_write_settings_domain(app, domain, |settings| {
        mutate(settings);
        Ok(())
    })
}

pub(crate) fn try_write_settings_domain<F>(
    app: &AppHandle,
    domain: SettingsWriteDomain,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings) -> Result<(), String>,
{
    let mut settings = get_settings(app);
    try_mutate_settings_domain(&mut settings, domain, mutate)?;
    write_settings(app, settings);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn mutate_settings_domain<F>(
    settings: &mut AppSettings,
    domain: SettingsWriteDomain,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings),
{
    try_mutate_settings_domain(settings, domain, |settings| {
        mutate(settings);
        Ok(())
    })
}

pub(crate) fn try_mutate_settings_domain<F>(
    settings: &mut AppSettings,
    _domain: SettingsWriteDomain,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings) -> Result<(), String>,
{
    let mut next = settings.clone();
    mutate(&mut next)?;
    *settings = next;
    Ok(())
}

pub fn reset_settings_to_defaults_with_backup(app: &AppHandle) -> Result<(), String> {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .map_err(|err| format!("initialize settings store: {err}"))?;

    if let Some(settings_value) = store.get("settings") {
        let backup_dir = settings_backup_directory(app)?;
        backup_settings_value_to_dir(&backup_dir, &settings_value)?;
    }

    let default_settings = get_default_settings();
    let default_value = serde_json::to_value(&default_settings)
        .map_err(|err| format!("serialize default settings: {err}"))?;
    store.set("settings", default_value);
    Ok(())
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    let bindings = get_bindings(app);

    let binding = bindings.get(id).unwrap().clone();

    binding
}

pub fn get_history_limit(app: &AppHandle) -> usize {
    let settings = get_settings(app);
    settings.history_limit
}

pub fn get_recording_retention_period(app: &AppHandle) -> RecordingRetentionPeriod {
    let settings = get_settings(app);
    settings.recording_retention_period
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
    }

    #[test]
    fn apply_settings_mutation_runs_closure_and_returns_value() {
        let mut settings = crate::settings::get_default_settings();
        let added = crate::settings::apply_settings_mutation(&mut settings, |s| {
            s.custom_words.push("Robyn".to_string());
            s.custom_words.len()
        });
        assert_eq!(added, 1);
        assert_eq!(settings.custom_words, vec!["Robyn".to_string()]);
    }

    #[test]
    fn settings_domain_mutation_allows_declared_domain_only() {
        let mut settings = get_default_settings();
        settings.history_enabled = true;
        settings.recordings_enabled = true;
        settings.selected_model = "unchanged-model".to_string();

        mutate_settings_domain(&mut settings, SettingsWriteDomain::Privacy, |settings| {
            settings.history_enabled = false;
            settings.recordings_enabled = false;
        })
        .expect("privacy mutation should be accepted");

        assert!(!settings.history_enabled);
        assert!(!settings.recordings_enabled);
        assert_eq!(settings.selected_model, "unchanged-model");
    }

    #[test]
    fn model_settings_round_trip_preserves_cpu_accelerator_and_startup_sensitive_fields() {
        let mut settings = get_default_settings();
        settings.push_to_talk = false;
        settings.selected_microphone = Some("Studio Microphone".to_string());

        mutate_settings_domain(&mut settings, SettingsWriteDomain::Models, |settings| {
            settings.whisper_accelerator = WhisperAcceleratorSetting::Cpu;
            settings.whisper_gpu_device = 0;
        })
        .expect("model mutation should be accepted");

        let reparsed: AppSettings =
            serde_json::from_value(serde_json::to_value(settings).unwrap()).unwrap();

        assert_eq!(reparsed.whisper_accelerator, WhisperAcceleratorSetting::Cpu);
        assert_eq!(reparsed.whisper_gpu_device, 0);
        assert!(!reparsed.push_to_talk);
        assert_eq!(
            reparsed.selected_microphone,
            Some("Studio Microphone".to_string())
        );
    }

    #[test]
    fn transform_shortcut_defaults_are_visible_but_unbound() {
        let settings = get_default_settings();
        for id in [
            "transform_polish",
            "transform_make_concise",
            "transform_turn_into_list",
            "transform_translate",
            "transform_prompt_engineer",
        ] {
            let binding = settings.bindings.get(id).expect("transform binding");
            assert!(is_unbound_shortcut(&binding.default_binding));
            assert!(is_unbound_shortcut(&binding.current_binding));
        }
    }

    #[test]
    fn binding_defaults_migrate_transform_shortcuts_into_existing_settings() {
        let mut settings = get_default_settings();
        for id in [
            "transform_polish",
            "transform_make_concise",
            "transform_turn_into_list",
            "transform_translate",
            "transform_prompt_engineer",
        ] {
            settings.bindings.remove(id);
        }

        assert!(ensure_binding_defaults(&mut settings));

        for id in [
            "transform_polish",
            "transform_make_concise",
            "transform_turn_into_list",
            "transform_translate",
            "transform_prompt_engineer",
        ] {
            let binding = settings.bindings.get(id).expect("transform binding");
            assert!(is_unbound_shortcut(&binding.current_binding));
        }
    }

    #[test]
    fn default_settings_mute_system_audio_while_recording() {
        let settings = get_default_settings();
        assert!(settings.mute_while_recording);
    }

    #[test]
    fn default_settings_disable_auto_add_dictionary_words() {
        let settings = get_default_settings();
        assert!(!settings.auto_add_dictionary_words);
    }

    #[test]
    fn default_settings_start_with_empty_dictionary_entries() {
        let settings = get_default_settings();
        assert!(settings.dictionary_entries.is_empty());
    }

    #[test]
    fn default_settings_start_with_empty_dictionary_auto_learn_suppression() {
        let settings = get_default_settings();
        assert!(settings.dictionary_auto_learn_suppressed.is_empty());
    }

    #[test]
    fn default_settings_have_empty_candidates_and_schema_zero() {
        let settings = crate::settings::get_default_settings();
        assert!(settings.dictionary_learn_candidates.is_empty());
        assert_eq!(settings.dictionary_schema_version, 0);
    }

    #[test]
    fn learn_candidate_round_trips() {
        let c = crate::settings::LearnCandidate {
            replacement_of: Some("robin".into()),
            phrase: "Robyn".into(),
            occurrences: 1,
            last_evidence_session: Some("s1".into()),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: crate::settings::LearnCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phrase, "Robyn");
        assert_eq!(back.occurrences, 1);
    }

    #[test]
    fn default_settings_start_with_empty_snippets() {
        let settings = get_default_settings();
        assert!(settings.snippets.is_empty());
    }

    #[test]
    fn default_settings_keep_empty_custom_words_for_compatibility() {
        let settings = get_default_settings();
        assert!(settings.custom_words.is_empty());
    }

    #[test]
    fn default_settings_disable_post_processing() {
        let settings = get_default_settings();
        assert!(!settings.post_process_enabled);
    }

    #[test]
    fn default_settings_do_not_enable_managed_local_llm() {
        let settings = get_default_settings();

        assert!(!settings.local_llm.enabled);
        assert_eq!(settings.local_llm.runtime_mode, "managed");
        assert_eq!(settings.local_llm.runtime_host, "127.0.0.1");
        assert_eq!(settings.local_llm.runtime_port, 0);
        assert_eq!(settings.local_llm.max_output_tokens, 512);
    }

    #[test]
    fn default_settings_use_info_log_level() {
        let settings = get_default_settings();
        assert_eq!(settings.log_level, LogLevel::Info);
    }

    #[test]
    fn default_settings_preserve_history_and_recording_storage() {
        let settings = get_default_settings();

        assert!(settings.history_enabled);
        assert!(settings.recordings_enabled);
        assert_eq!(settings.history_limit, 5);
        assert_eq!(
            settings.recording_retention_period,
            RecordingRetentionPeriod::PreserveLimit
        );
    }

    #[test]
    fn existing_settings_log_message_does_not_include_user_content() {
        let mut settings = get_default_settings();
        settings.custom_words.push("ConfidentialWord".to_string());

        let message = existing_settings_log_message(&settings);

        assert!(message.contains("Found existing settings"));
        assert!(!message.contains("ConfidentialWord"));
    }

    #[test]
    fn parse_failure_recovery_preserves_valid_user_fields() {
        let mut settings_value = serde_json::to_value(get_default_settings()).unwrap();
        let object = settings_value.as_object_mut().unwrap();
        object.insert("log_level".to_string(), serde_json::json!("verbose"));
        object.insert("custom_words".to_string(), serde_json::json!(["Robyn"]));
        object.insert(
            "dictionary_entries".to_string(),
            serde_json::json!([
                {
                    "id": "dict_1_robyn",
                    "phrase": "Robyn",
                    "source": "manual",
                    "priority": "normal",
                    "created_at_ms": 1,
                    "updated_at_ms": 1
                }
            ]),
        );
        object.insert(
            "snippets".to_string(),
            serde_json::json!([
                {
                    "id": "snippet_1_email",
                    "trigger": "email signature",
                    "content": "Regards,\nAbdullah",
                    "created_at_ms": 1,
                    "updated_at_ms": 1
                }
            ]),
        );
        object.insert(
            "post_process_api_keys".to_string(),
            serde_json::json!({"openai": "dummy-api-key"}),
        );

        assert!(serde_json::from_value::<AppSettings>(settings_value.clone()).is_err());

        let recovered = recover_settings_from_unparseable_value(&settings_value);

        assert_eq!(recovered.log_level, LogLevel::Info);
        assert_eq!(recovered.custom_words, vec!["Robyn"]);
        assert_eq!(recovered.dictionary_entries[0].phrase, "Robyn");
        assert_eq!(recovered.snippets[0].trigger, "email signature");
        assert_eq!(
            recovered
                .post_process_api_keys
                .get("openai")
                .map(String::as_str),
            Some("dummy-api-key")
        );
    }

    #[test]
    fn settings_parse_failure_backup_writes_recoverable_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_value = serde_json::json!({
            "log_level": "verbose",
            "custom_words": ["Robyn"]
        });

        let backup_path =
            backup_settings_value_to_dir(temp_dir.path(), &settings_value).expect("backup path");

        assert!(backup_path.exists());
        assert!(backup_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("settings_store.parse-error."));
        let restored: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(backup_path).unwrap()).unwrap();
        assert_eq!(restored, settings_value);
    }

    #[test]
    fn default_post_process_providers_include_local_openai_compatible_servers() {
        let providers = default_post_process_providers();

        let lm_studio = providers
            .iter()
            .find(|provider| provider.id == "lm_studio")
            .expect("LM Studio provider");
        assert_eq!(lm_studio.base_url, "http://localhost:1234/v1");
        assert!(lm_studio.allow_base_url_edit);
        assert_eq!(lm_studio.models_endpoint.as_deref(), Some("/models"));
        assert!(!lm_studio.supports_structured_output);

        let ollama = providers
            .iter()
            .find(|provider| provider.id == "ollama")
            .expect("Ollama provider");
        assert_eq!(ollama.base_url, "http://localhost:11434/v1");
        assert!(ollama.allow_base_url_edit);
        assert_eq!(ollama.models_endpoint.as_deref(), Some("/models"));
        assert!(!ollama.supports_structured_output);

        let vllm = providers
            .iter()
            .find(|provider| provider.id == "vllm")
            .expect("vLLM provider");
        assert_eq!(vllm.base_url, "http://localhost:8000/v1");
        assert!(vllm.allow_base_url_edit);
        assert_eq!(vllm.models_endpoint.as_deref(), Some("/models"));
        assert!(!vllm.supports_structured_output);
    }

    #[test]
    fn post_process_defaults_normalize_known_openai_compatible_base_urls() {
        let mut settings = get_default_settings();

        settings
            .post_process_provider_mut("custom")
            .unwrap()
            .base_url = "http://192.0.2.10:1234".to_string();
        settings
            .post_process_provider_mut("lm_studio")
            .unwrap()
            .base_url = "http://localhost:1234/".to_string();
        settings
            .post_process_provider_mut("ollama")
            .unwrap()
            .base_url = "http://localhost:11434".to_string();
        settings.post_process_provider_mut("vllm").unwrap().base_url =
            "http://localhost:8000".to_string();

        assert!(ensure_post_process_defaults(&mut settings));

        assert_eq!(
            settings.post_process_provider("custom").unwrap().base_url,
            "http://192.0.2.10:1234/v1"
        );
        assert_eq!(
            settings
                .post_process_provider("lm_studio")
                .unwrap()
                .base_url,
            "http://localhost:1234/v1"
        );
        assert_eq!(
            settings.post_process_provider("ollama").unwrap().base_url,
            "http://localhost:11434/v1"
        );
        assert_eq!(
            settings.post_process_provider("vllm").unwrap().base_url,
            "http://localhost:8000/v1"
        );
    }

    #[test]
    fn local_post_process_base_url_detection_accepts_local_presets_only() {
        assert!(is_local_post_process_base_url("http://localhost:1234/v1"));
        assert!(is_local_post_process_base_url("http://localhost:11434/v1"));
        assert!(is_local_post_process_base_url("http://localhost:8000/v1"));
        assert!(is_local_post_process_base_url("https://127.0.0.1:8080/v1"));
        assert!(is_local_post_process_base_url("http://[::1]:11434/v1"));
        assert!(is_local_post_process_base_url("apple-intelligence://local"));

        for base_url in [
            "http://localhost.evil.com/v1",
            "http://localhost@evil.com/v1",
            "https://127.0.0.1.evil.com/v1",
            "https://api.openai.com/v1",
        ] {
            assert!(
                !is_local_post_process_base_url(base_url),
                "{base_url} must not be treated as a local provider"
            );
        }
    }

    #[test]
    fn default_settings_use_light_formatting() {
        let settings = get_default_settings();
        assert_eq!(settings.formatting_level, FormattingLevel::Light);
    }

    #[test]
    fn default_settings_use_auto_language() {
        let settings = get_default_settings();
        assert_eq!(settings.selected_language, "auto");
    }

    #[test]
    fn default_settings_select_auto_dictation_language_mode() {
        let settings = get_default_settings();
        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Auto
        );
    }

    #[test]
    fn default_settings_keep_docked_pill_disabled() {
        let settings = get_default_settings();
        assert!(!settings.docked_pill_enabled);
    }

    #[test]
    fn dictation_language_mode_maps_to_language_settings() {
        let mut settings = get_default_settings();

        settings
            .apply_dictation_language_mode(
                DictationLanguageMode::Single,
                Some("de".to_string()),
                vec!["fr".to_string(), "de".to_string(), "ja".to_string()],
            )
            .expect("single language mode");
        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Single
        );
        assert_eq!(settings.selected_language, "de");
        assert_eq!(
            settings.adaptive_language_shortlist,
            vec!["fr".to_string(), "de".to_string(), "ja".to_string()]
        );

        settings
            .apply_dictation_language_mode(
                DictationLanguageMode::Multilingual,
                None,
                vec!["fr".to_string(), "ja".to_string()],
            )
            .expect("multilingual mode");
        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Multilingual
        );
        assert_eq!(settings.selected_language, "auto");
        assert_eq!(
            settings.adaptive_language_shortlist,
            vec!["fr".to_string(), "ja".to_string()]
        );

        settings
            .apply_dictation_language_mode(
                DictationLanguageMode::Auto,
                None,
                vec!["fr".to_string(), "ja".to_string()],
            )
            .expect("auto mode");
        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Auto
        );
        assert_eq!(settings.selected_language, "auto");
        assert_eq!(
            settings.adaptive_language_shortlist,
            vec!["fr".to_string(), "ja".to_string()]
        );
    }

    #[test]
    fn dictation_language_mode_rejects_invalid_language_lists() {
        let mut settings = get_default_settings();

        assert!(settings
            .apply_dictation_language_mode(DictationLanguageMode::Single, None, vec![])
            .is_err());
        assert!(settings
            .apply_dictation_language_mode(
                DictationLanguageMode::Multilingual,
                None,
                vec!["fr".to_string()]
            )
            .is_err());
        assert!(settings
            .apply_dictation_language_mode(
                DictationLanguageMode::Single,
                Some("ja".to_string()),
                vec!["fr".to_string()]
            )
            .is_err());
    }

    #[test]
    fn dictionary_defaults_migrate_legacy_custom_words() {
        let mut settings = get_default_settings();
        settings.custom_words = vec!["Robyn".to_string()];

        let changed = ensure_dictionary_defaults(&mut settings);

        assert!(changed);
        assert_eq!(settings.dictionary_entries.len(), 1);
        assert_eq!(settings.dictionary_phrases(), vec!["Robyn"]);
        assert_eq!(settings.custom_words, vec!["Robyn"]);
    }

    #[test]
    fn dictionary_defaults_do_not_rehydrate_explicitly_empty_entries_from_legacy_words() {
        let mut settings_value = serde_json::to_value(get_default_settings()).unwrap();
        let object = settings_value.as_object_mut().unwrap();
        object.insert("custom_words".to_string(), serde_json::json!(["Gibbeteen"]));
        object.insert("dictionary_entries".to_string(), serde_json::json!([]));

        let mut settings: AppSettings = serde_json::from_value(settings_value.clone()).unwrap();

        let changed =
            ensure_dictionary_defaults_for_loaded_value(&mut settings, Some(&settings_value));

        assert!(changed);
        assert!(settings.dictionary_entries.is_empty());
        assert!(settings.custom_words.is_empty());
    }

    #[test]
    fn dictionary_defaults_still_migrate_missing_legacy_entries_key() {
        let mut settings_value = serde_json::to_value(get_default_settings()).unwrap();
        let object = settings_value.as_object_mut().unwrap();
        object.insert("custom_words".to_string(), serde_json::json!(["Robyn"]));
        object.remove("dictionary_entries");

        let mut settings: AppSettings = serde_json::from_value(settings_value.clone()).unwrap();

        let changed =
            ensure_dictionary_defaults_for_loaded_value(&mut settings, Some(&settings_value));

        assert!(changed);
        assert_eq!(settings.dictionary_phrases(), vec!["Robyn"]);
        assert_eq!(settings.custom_words, vec!["Robyn"]);
    }

    #[test]
    fn snippet_defaults_normalize_stored_entries() {
        let mut settings = get_default_settings();
        settings.snippets = vec![crate::snippets::SnippetEntry {
            id: "snippet_1_email".to_string(),
            trigger: "  email   signature  ".to_string(),
            content: "Signature".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }];

        let changed = ensure_snippet_defaults(&mut settings);

        assert!(changed);
        assert_eq!(settings.snippets[0].trigger, "email signature");
    }

    #[test]
    fn default_settings_enable_adaptive_data_but_not_adaptive_mode() {
        let settings = get_default_settings();

        assert!(!settings.adaptive_profiles_enabled);
        assert!(!settings.context_awareness_enabled);
        assert!(!settings.context_nearby_text_enabled);
        assert_eq!(
            settings.adaptive_language_shortlist,
            vec!["en".to_string(), "ar".to_string()]
        );
        assert_eq!(settings.adaptive_default_profile_id, "default_clean");
        assert!(settings
            .adaptive_profiles
            .iter()
            .any(|profile| profile.id == "raw"));
        assert!(settings
            .adaptive_profiles
            .iter()
            .any(|profile| profile.id == "mixed_multilingual"));
        assert!(settings
            .adaptive_profiles
            .iter()
            .all(|profile| profile.id != "translation"));
        assert!(settings.adaptive_correction_memory_enabled);
    }

    #[test]
    fn adaptive_defaults_remove_legacy_translation_profile() {
        let mut settings = get_default_settings();
        let mut legacy_profile = settings.adaptive_profiles[0].clone();
        legacy_profile.id = "translation".to_string();
        settings.adaptive_profiles.push(legacy_profile);
        settings.adaptive_default_profile_id = "translation".to_string();

        let changed = ensure_adaptive_defaults(&mut settings);

        assert!(changed);
        assert_eq!(settings.adaptive_default_profile_id, "default_clean");
        assert!(settings
            .adaptive_profiles
            .iter()
            .all(|profile| profile.id != "translation"));
    }

    #[test]
    fn legacy_translate_to_english_maps_to_translation_request() {
        let mut settings = get_default_settings();
        settings.translate_to_english = true;

        let changed = ensure_translation_defaults(&mut settings);

        assert!(changed);
        assert!(settings.translation_enabled);
        assert_eq!(
            settings
                .translation_request
                .as_ref()
                .expect("translation request")
                .target_language,
            "en"
        );
    }

    #[test]
    fn translation_request_absent_when_translation_is_disabled() {
        let settings = get_default_settings();
        assert!(!settings.translation_enabled);
        assert!(settings.translation_request.is_none());
    }

    #[test]
    fn setting_translation_target_language_updates_general_translation_request() {
        let mut settings = get_default_settings();
        settings.translation_enabled = true;
        settings.translation_request = Some(TranslationRequestSettings {
            source_language: "fr".to_string(),
            target_language: "en".to_string(),
            route: TranslationRoute::TextAfterTranscription,
        });

        set_translation_target_language(&mut settings, "de".to_string());

        assert!(settings.translation_enabled);
        assert!(!settings.translate_to_english);
        let request = settings.translation_request.expect("translation request");
        assert_eq!(request.source_language, "fr");
        assert_eq!(request.target_language, "de");
        assert_eq!(request.route, TranslationRoute::TextAfterTranscription);
    }

    #[test]
    fn debug_output_redacts_api_keys() {
        let mut settings = get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "sk-proj-secret-key-12345".to_string());
        settings.post_process_api_keys.insert(
            "anthropic".to_string(),
            "sk-ant-secret-key-67890".to_string(),
        );
        settings
            .post_process_api_keys
            .insert("empty_provider".to_string(), "".to_string());

        let debug_output = format!("{:?}", settings);

        assert!(!debug_output.contains("sk-proj-secret-key-12345"));
        assert!(!debug_output.contains("sk-ant-secret-key-67890"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn secret_map_debug_redacts_values() {
        let map = SecretMap(HashMap::from([("key".into(), "secret".into())]));
        let out = format!("{:?}", map);
        assert!(!out.contains("secret"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn dictionary_entry_defaults_active_true_flags_false() {
        // Legacy JSON without the new fields must deserialize as active + untouched flags.
        let legacy = r#"{"id":"dict_1_robyn","phrase":"Robyn"}"#;
        let entry: crate::settings::DictionaryEntry = serde_json::from_str(legacy).unwrap();
        assert!(entry.active);
        assert!(!entry.user_confirmed);
        assert!(!entry.needs_review);
    }
}
