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
const DICTATION_LANGUAGE_SCHEMA_VERSION: u32 = 1;

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

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, Type)]
pub struct DictionaryDiagnostics {
    #[serde(default)]
    pub learned: u32,
    #[serde(default)]
    pub promoted: u32,
    #[serde(default)]
    pub reinforced: u32,
    #[serde(default)]
    pub routed: u32,
    #[serde(default)]
    pub skip_secure_field: u32,
    #[serde(default)]
    pub skip_secure_check_error: u32,
    #[serde(default)]
    pub skip_read_cap_exceeded: u32,
    #[serde(default)]
    pub skip_target_changed: u32,
    #[serde(default)]
    pub skip_no_post_paste_change: u32,
    #[serde(default)]
    pub skip_runtime_id: u32,
    #[serde(default)]
    pub since_ms: u64,
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
    #[serde(default = "default_bindings")]
    pub bindings: HashMap<String, ShortcutBinding>,
    #[serde(default = "default_push_to_talk")]
    pub push_to_talk: bool,
    #[serde(default = "default_audio_feedback")]
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
    pub selected_microphone_id: Option<String>,
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
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default)]
    pub dictation_language_mode: DictationLanguageMode,
    #[serde(default)]
    pub dictation_language_schema_version: u32,
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
    pub dictionary_diagnostics: DictionaryDiagnostics,
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
    #[serde(default)]
    pub allow_insecure_lan_post_process: bool,
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

fn default_bindings() -> HashMap<String, ShortcutBinding> {
    get_default_settings().bindings
}

fn default_push_to_talk() -> bool {
    true
}

fn default_audio_feedback() -> bool {
    false
}

fn default_adaptive_language_shortlist() -> Vec<String> {
    vec!["en".to_string(), "ar".to_string()]
}

pub fn normalize_bcp47(tag: &str) -> String {
    let mut subtags = tag.trim().split('-').filter(|subtag| !subtag.is_empty());
    let Some(primary) = subtags.next() else {
        return String::new();
    };

    let mut normalized = vec![primary.to_ascii_lowercase()];
    normalized.extend(subtags.map(|subtag| {
        if subtag.len() == 4 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            let mut bytes = subtag.to_ascii_lowercase().into_bytes();
            bytes[0] = bytes[0].to_ascii_uppercase();
            String::from_utf8(bytes).expect("ASCII BCP-47 script subtag remains UTF-8")
        } else if subtag.len() == 2 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            subtag.to_ascii_uppercase()
        } else {
            subtag.to_string()
        }
    }));
    normalized.join("-")
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

pub fn clamp_extra_recording_buffer_ms(v: u64) -> u64 {
    // Used by thread::sleep on the stop path; a typo must not hang dictation.
    v.min(2_000)
}

pub fn clamp_audio_feedback_volume(v: f32) -> f32 {
    if v.is_nan() {
        1.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

pub fn clamp_word_correction_threshold(v: f64) -> f64 {
    if v.is_nan() {
        0.18
    } else {
        v.clamp(0.0, 1.0)
    }
}

pub fn clamp_paste_delay_ms(v: u64) -> u64 {
    v.min(2_000)
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

    parsed.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .map(|address| address.is_loopback())
                .unwrap_or(false)
    })
}

pub fn validate_post_process_base_url(
    base_url: &str,
    allow_insecure_lan: bool,
) -> Result<(), String> {
    let parsed = reqwest::Url::parse(base_url.trim())
        .map_err(|error| format!("Invalid provider URL: {error}"))?;

    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_local_post_process_base_url(base_url) => Ok(()),
        "http" if allow_insecure_lan => Ok(()),
        "http" => Err("Non-loopback provider URLs must use HTTPS".to_string()),
        scheme => Err(format!("Unsupported provider URL scheme: {scheme}")),
    }
}

pub fn is_insecure_lan_post_process_base_url(base_url: &str) -> bool {
    reqwest::Url::parse(base_url.trim())
        .map(|parsed| parsed.scheme() == "http" && !is_local_post_process_base_url(base_url))
        .unwrap_or(false)
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

fn ensure_dictation_language_defaults(settings: &mut AppSettings) -> bool {
    if settings.dictation_language_schema_version >= DICTATION_LANGUAGE_SCHEMA_VERSION {
        return false;
    }

    settings.selected_language = normalize_bcp47(&settings.selected_language);
    settings.adaptive_language_shortlist = settings
        .adaptive_language_shortlist
        .iter()
        .map(|language| normalize_bcp47(language))
        .filter(|language| !language.is_empty() && language != "auto")
        .fold(Vec::new(), |mut languages, language| {
            if !languages.contains(&language) {
                languages.push(language);
            }
            languages
        });

    if settings.dictation_language_mode == DictationLanguageMode::Auto
        && !settings.selected_language.is_empty()
        && settings.selected_language != "auto"
    {
        settings.dictation_language_mode = DictationLanguageMode::Single;
    }

    settings.dictation_language_schema_version = DICTATION_LANGUAGE_SCHEMA_VERSION;
    true
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
    let migrated_v1 = crate::dictionary::migrate_dictionary_v1(settings);
    let migrated_v2 = crate::dictionary::migrate_dictionary_v2(settings);
    let synced = crate::dictionary::sync_legacy_custom_words_with_migration(
        settings,
        !settings_value_has_dictionary_entries(settings_value),
    );
    migrated_v1 || migrated_v2 || synced
}

fn ensure_snippet_defaults(settings: &mut AppSettings) -> bool {
    crate::snippets::sync_snippets(settings)
}

pub fn set_translation_target_language(settings: &mut AppSettings, target_language: String) {
    let target_language = normalize_bcp47(&target_language);
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
        selected_microphone_id: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        translation_enabled: false,
        translation_request: None,
        selected_language: "auto".to_string(),
        dictation_language_mode: DictationLanguageMode::default(),
        dictation_language_schema_version: DICTATION_LANGUAGE_SCHEMA_VERSION,
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
        dictionary_diagnostics: DictionaryDiagnostics::default(),
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
        allow_insecure_lan_post_process: false,
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
    pub fn clear_dictation_language_lock(&mut self) {
        self.selected_language = "auto".to_string();
        self.dictation_language_mode = DictationLanguageMode::Auto;
        self.adaptive_language_shortlist = default_adaptive_language_shortlist();
    }

    pub fn apply_dictation_language_mode(
        &mut self,
        mode: DictationLanguageMode,
        selected_language: Option<String>,
        languages: Vec<String>,
    ) -> Result<(), String> {
        let cleaned = languages.into_iter().fold(Vec::new(), |mut acc, language| {
            let language = normalize_bcp47(&language);
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
                    .map(normalize_bcp47)
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

fn settings_store_file_path<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let store_path = crate::portable::store_path(SETTINGS_STORE_PATH);
    if store_path.is_absolute() {
        return Ok(store_path);
    }

    crate::portable::resolve_app_data(app, SETTINGS_STORE_PATH)
        .map_err(|err| format!("resolve settings store path: {err}"))
}

fn settings_backup_directory<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
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

fn backup_unparseable_settings<R: tauri::Runtime>(
    app: &AppHandle<R>,
    settings_value: &serde_json::Value,
) {
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

fn recover_unparseable_settings<R: tauri::Runtime>(
    app: &AppHandle<R>,
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

fn open_settings_store<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<std::sync::Arc<tauri_plugin_store::Store<R>>, String> {
    open_settings_store_at_path(app, crate::portable::store_path(SETTINGS_STORE_PATH))
}

fn open_settings_store_at_path<R: tauri::Runtime>(
    app: &AppHandle<R>,
    store_path: PathBuf,
) -> Result<std::sync::Arc<tauri_plugin_store::Store<R>>, String> {
    // The Store plugin calls `AppHandle::state` while constructing a `StoreBuilder`, which
    // panics if the plugin state was never registered. Keep only that constructor call inside
    // the recovery boundary. `build` can also resolve paths, lock shared state, and deserialize
    // persisted data; errors and panics there must retain their native semantics.
    let store_builder = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.store_builder(store_path)
    }))
    .map_err(|_| "initialize settings store: Store plugin state unavailable".to_string())?;

    store_builder
        .build()
        .map_err(|error| format!("initialize settings store: {error}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsLoadOrigin {
    New,
    Parsed,
    Recovered,
}

pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    let _guard = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let store = match open_settings_store(app) {
        Ok(store) => store,
        Err(error) => {
            warn!("Failed to initialize settings store: {error}");
            return privacy_safe_settings_fallback();
        }
    };

    let mut settings_value_for_defaults = None;
    let (
        mut settings,
        settings_before_migrations,
        mut should_persist_settings,
        force_immediate_save,
        load_origin,
    ) = if let Some(settings_value) = store.get("settings") {
        settings_value_for_defaults = Some(settings_value.clone());
        // Parse the entire settings object.
        match serde_json::from_value::<AppSettings>(settings_value.clone()) {
            Ok(settings) => {
                debug!("{}", existing_settings_log_message(&settings));
                (
                    settings.clone(),
                    Some(settings),
                    false,
                    false,
                    SettingsLoadOrigin::Parsed,
                )
            }
            Err(error) => {
                let recovered_settings = recover_unparseable_settings(app, &settings_value, &error);
                (
                    recovered_settings.clone(),
                    Some(recovered_settings),
                    true,
                    true,
                    SettingsLoadOrigin::Recovered,
                )
            }
        }
    } else {
        (
            get_default_settings(),
            None,
            true,
            false,
            SettingsLoadOrigin::New,
        )
    };

    let defaults_changed =
        reconcile_loaded_settings_defaults(&mut settings, settings_value_for_defaults.as_ref());
    let credentials_changed = crate::credentials::prepare_post_process_api_keys_for_store(
        &mut settings,
        crate::credentials::CredentialStoreFailurePolicy::PreserveLegacyValue,
    );
    should_persist_settings |= defaults_changed || credentials_changed;
    if should_persist_settings {
        if let Err(error) = persist_loaded_settings_value(
            store.as_ref(),
            settings_before_migrations.as_ref(),
            &settings,
            force_immediate_save || credentials_changed,
        ) {
            if let Some(settings_before_migrations) = settings_before_migrations.as_ref() {
                settings = settings_before_migrations.clone();
            }
            // Startup initialization has no command caller to return this to.
            warn!("Failed to persist migrated settings: {error}");
            if load_origin != SettingsLoadOrigin::Parsed {
                // Only a clean parsed value is a persisted user setting that runtime may retain.
                // Do not enable data retention, context capture, or post-processing from an
                // unpersisted first-run default or recovered value.
                return privacy_safe_settings_fallback();
            }
        }
    }

    crate::credentials::hydrate_post_process_api_keys(&mut settings);

    settings
}

pub fn get_settings<R: tauri::Runtime>(app: &AppHandle<R>) -> AppSettings {
    match try_get_settings(app) {
        Ok(outcome) => settings_for_non_command_read(outcome),
        Err(error) => {
            warn!("Failed to load settings: {error}");
            privacy_safe_settings_fallback()
        }
    }
}

struct SettingsLoadOutcome {
    settings: AppSettings,
    persistence_error: Option<String>,
    load_origin: SettingsLoadOrigin,
}

fn settings_for_non_command_read(outcome: SettingsLoadOutcome) -> AppSettings {
    let SettingsLoadOutcome {
        settings,
        persistence_error,
        load_origin,
    } = outcome;

    if let Some(error) = persistence_error.as_deref() {
        warn!("Failed to persist reconciled settings: {error}");
        if load_origin != SettingsLoadOrigin::Parsed {
            // A first-run default or recovery result was never durably persisted. Fail closed
            // rather than enabling storage or context-derived data without a clean parsed user
            // value to preserve.
            return privacy_safe_settings_fallback();
        }
    }
    settings
}

fn settings_for_fallible_domain_write(outcome: SettingsLoadOutcome) -> Result<AppSettings, String> {
    let SettingsLoadOutcome {
        settings,
        persistence_error,
        ..
    } = outcome;
    match persistence_error {
        Some(error) => Err(error),
        None => Ok(settings),
    }
}

fn privacy_safe_settings_fallback() -> AppSettings {
    let mut settings = get_default_settings();
    settings.history_enabled = false;
    settings.recordings_enabled = false;
    settings.adaptive_profiles_enabled = false;
    settings.context_awareness_enabled = false;
    settings.context_nearby_text_enabled = false;
    settings.auto_add_dictionary_words = false;
    settings.adaptive_correction_memory_enabled = false;
    settings.post_process_enabled = false;
    settings.post_process_api_keys.clear();
    settings
}

fn try_get_settings<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<SettingsLoadOutcome, String> {
    let store = open_settings_store(app)?;

    Ok(load_settings_from_store_read_only(store.as_ref()))
}

fn load_settings_from_store_read_only<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> SettingsLoadOutcome {
    let mut settings_value_for_defaults = None;
    let (mut settings, load_origin) = if let Some(settings_value) = store.get("settings") {
        settings_value_for_defaults = Some(settings_value.clone());
        match serde_json::from_value::<AppSettings>(settings_value) {
            Ok(settings) => (settings, SettingsLoadOrigin::Parsed),
            Err(error) => {
                warn!("Failed to parse settings during read-only load: {error}");
                (
                    recover_settings_from_unparseable_value(
                        settings_value_for_defaults
                            .as_ref()
                            .expect("settings value is present during recovery"),
                    ),
                    SettingsLoadOrigin::Recovered,
                )
            }
        }
    } else {
        (get_default_settings(), SettingsLoadOrigin::New)
    };

    reconcile_loaded_settings_defaults(&mut settings, settings_value_for_defaults.as_ref());

    SettingsLoadOutcome {
        settings,
        // A first-run or recovered value is only safe to use after the startup loader has
        // durably persisted it. Ordinary reads must never become that persistence boundary.
        persistence_error: match load_origin {
            // A clean first-run default is safe to report on a read: history and
            // recordings default ON and the startup loader durably persists them
            // moments later. Failing closed here disabled retention on every fresh
            // install (regressing the packaged first-launch contract), because
            // early get_settings reads run before the startup loader persists.
            SettingsLoadOrigin::Parsed | SettingsLoadOrigin::New => None,
            // A recovered (previously unparseable) value has unknown user intent;
            // do not enable retention from it on a read until startup re-persists.
            SettingsLoadOrigin::Recovered => {
                Some("settings initialization is deferred to startup".to_string())
            }
        },
        load_origin,
    }
}

fn load_settings_from_store<R: tauri::Runtime>(
    app: &AppHandle<R>,
    store: &tauri_plugin_store::Store<R>,
) -> SettingsLoadOutcome {
    let mut settings_value_for_defaults = None;
    let (
        mut settings,
        settings_before_migrations,
        mut should_persist_settings,
        force_immediate_save,
        load_origin,
    ) = if let Some(settings_value) = store.get("settings") {
        settings_value_for_defaults = Some(settings_value.clone());
        match serde_json::from_value::<AppSettings>(settings_value.clone()) {
            Ok(settings) => (
                settings.clone(),
                Some(settings),
                false,
                false,
                SettingsLoadOrigin::Parsed,
            ),
            Err(error) => {
                let recovered_settings = recover_unparseable_settings(app, &settings_value, &error);
                (
                    recovered_settings.clone(),
                    Some(recovered_settings),
                    true,
                    true,
                    SettingsLoadOrigin::Recovered,
                )
            }
        }
    } else {
        (
            get_default_settings(),
            None,
            true,
            false,
            SettingsLoadOrigin::New,
        )
    };

    should_persist_settings |=
        reconcile_loaded_settings_defaults(&mut settings, settings_value_for_defaults.as_ref());
    let persistence_error = if should_persist_settings {
        persist_loaded_settings_value(
            store,
            settings_before_migrations.as_ref(),
            &settings,
            force_immediate_save,
        )
        .err()
    } else {
        None
    };
    if persistence_error.is_some() {
        if let Some(settings_before_migrations) = settings_before_migrations {
            settings = settings_before_migrations;
        }
    }

    SettingsLoadOutcome {
        settings,
        persistence_error,
        load_origin,
    }
}

fn reconcile_loaded_settings_defaults(
    settings: &mut AppSettings,
    settings_value_for_defaults: Option<&serde_json::Value>,
) -> bool {
    let binding_changed = ensure_binding_defaults(settings);
    let post_process_changed = ensure_post_process_defaults(settings);
    let adaptive_changed = ensure_adaptive_defaults(settings);
    let dictation_language_changed = ensure_dictation_language_defaults(settings);
    let translation_changed = ensure_translation_defaults(settings);
    let dictionary_changed =
        ensure_dictionary_defaults_for_loaded_value(settings, settings_value_for_defaults);
    let snippet_changed = ensure_snippet_defaults(settings);

    binding_changed
        || post_process_changed
        || adaptive_changed
        || dictation_language_changed
        || translation_changed
        || dictionary_changed
        || snippet_changed
}

/// Pure application of a mutation to an already-loaded settings value.
/// Kept separate so it is unit-testable without an AppHandle.
pub fn apply_settings_mutation<T>(
    settings: &mut AppSettings,
    f: impl FnOnce(&mut AppSettings) -> T,
) -> T {
    f(settings)
}

fn reconcile_selected_microphone_identity(previous_name: Option<&str>, settings: &mut AppSettings) {
    if settings.selected_microphone.as_deref() != previous_name {
        settings.selected_microphone_id = None;
    }
}

fn settings_change_requires_immediate_save(previous: &AppSettings, next: &AppSettings) -> bool {
    previous.dictionary_schema_version != next.dictionary_schema_version
        || previous.dictation_language_schema_version != next.dictation_language_schema_version
        || previous.history_enabled != next.history_enabled
        || previous.recordings_enabled != next.recordings_enabled
        || previous.history_limit != next.history_limit
        || previous.recording_retention_period != next.recording_retention_period
        || previous.adaptive_profiles_enabled != next.adaptive_profiles_enabled
        || previous.context_awareness_enabled != next.context_awareness_enabled
        || previous.context_nearby_text_enabled != next.context_nearby_text_enabled
        || previous.auto_add_dictionary_words != next.auto_add_dictionary_words
        || previous.adaptive_correction_memory_enabled != next.adaptive_correction_memory_enabled
        || previous.post_process_enabled != next.post_process_enabled
        || &*previous.post_process_api_keys != &*next.post_process_api_keys
}

fn persist_settings_value<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    settings_value: serde_json::Value,
    immediate_save: bool,
) -> Result<(), String> {
    let previous_settings_value = if immediate_save {
        store.get("settings")
    } else {
        None
    };
    store.set("settings", settings_value);
    if immediate_save {
        if let Err(error) = store.save() {
            match previous_settings_value {
                Some(previous_settings_value) => store.set("settings", previous_settings_value),
                None => {
                    store.delete("settings");
                }
            }
            return Err(format!("atomically persist settings: {error}"));
        }
    }
    Ok(())
}

fn persist_loaded_settings_value<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    settings_before_migrations: Option<&AppSettings>,
    settings: &AppSettings,
    force_immediate_save: bool,
) -> Result<(), String> {
    // No prior value means first-run initialization; persist privacy defaults before the
    // store debounce can lose them during an early shutdown.
    let immediate_save = force_immediate_save
        || settings_before_migrations.is_none()
        || settings_before_migrations.is_some_and(|settings_before| {
            settings_change_requires_immediate_save(settings_before, settings)
        });
    let settings_value =
        serde_json::to_value(settings).map_err(|error| format!("serialize settings: {error}"))?;
    persist_settings_value(store, settings_value, immediate_save)
}

/// The legacy public way to mutate debounced persisted settings. Holds the write lock across
/// the whole read-modify-write so concurrent mutations cannot lost-update each other.
/// Do NOT `.await` or emit Tauri events inside `f`; emit after this returns.
pub fn mutate_settings_locked<T>(app: &AppHandle, f: impl FnOnce(&mut AppSettings) -> T) -> T {
    let _guard = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut settings = get_settings(app);
    let settings_before = settings.clone();
    let selected_microphone_before = settings.selected_microphone.clone();
    let result = apply_settings_mutation(&mut settings, f);
    reconcile_selected_microphone_identity(selected_microphone_before.as_deref(), &mut settings);
    let immediate_save = settings_change_requires_immediate_save(&settings_before, &settings);
    if let Err(error) = write_settings_with_immediate_save(app, settings, immediate_save) {
        // Current callers mutate debounced settings only. Privacy-sensitive command paths use
        // the fallible domain writers below, so they never report success after a required save.
        warn!("Failed to persist settings mutation: {error}");
    }
    result
}

fn try_mutate_settings_locked_and_save_to_store<R, T, F>(
    store: &tauri_plugin_store::Store<R>,
    mut settings: AppSettings,
    mutate: F,
) -> Result<T, String>
where
    R: tauri::Runtime,
    F: FnOnce(&mut AppSettings) -> Result<T, String>,
{
    let selected_microphone_before = settings.selected_microphone.clone();
    let result = apply_settings_mutation(&mut settings, mutate)?;
    reconcile_selected_microphone_identity(selected_microphone_before.as_deref(), &mut settings);
    write_settings_to_store_with_immediate_save(store, settings, true)?;
    Ok(result)
}

/// Fallible locked mutation for command paths that must not report success until the
/// updated settings value is durable. The Store helper restores its previous cached value
/// when the forced save fails, so callers observe the same settings after an error.
/// Do NOT `.await` or emit Tauri events inside `mutate`; emit only after this returns `Ok`.
pub(crate) fn try_mutate_settings_locked_and_save<R, T, F>(
    app: &AppHandle<R>,
    mutate: F,
) -> Result<T, String>
where
    R: tauri::Runtime,
    F: FnOnce(&mut AppSettings) -> Result<T, String>,
{
    let _guard = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let store = open_settings_store(app)?;
    let settings =
        settings_for_fallible_domain_write(load_settings_from_store(app, store.as_ref()))?;
    try_mutate_settings_locked_and_save_to_store(store.as_ref(), settings, mutate)
}

// NOTE: `write_settings` and `get_settings` are the lock-free primitives. All MUTATION
// paths must go through `mutate_settings_locked`, `try_mutate_settings_locked_and_save`, or the
// domain writers (`write_settings_domain` / `try_write_settings_domain`), which take the same lock.
// The deny-list test `dictionary_mutation_paths_do_not_call_write_settings_directly`
// guards this for every migrated file.
pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let settings_before = get_settings(app);
    let immediate_save = settings_change_requires_immediate_save(&settings_before, &settings);
    if let Err(error) = write_settings_with_immediate_save(app, settings, immediate_save) {
        // This legacy helper is used by startup/native smoke code, not UI-facing sensitive
        // mutations. Those mutations go through the fallible domain writers below.
        warn!("Failed to persist settings: {error}");
    }
}

fn write_settings_with_immediate_save(
    app: &AppHandle,
    settings: AppSettings,
    immediate_save: bool,
) -> Result<(), String> {
    let store = open_settings_store(app)?;

    write_settings_to_store_with_immediate_save(store.as_ref(), settings, immediate_save)
}

fn write_settings_to_store_with_immediate_save<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    mut settings: AppSettings,
    immediate_save: bool,
) -> Result<(), String> {
    crate::dictionary::sync_legacy_custom_words(&mut settings);
    crate::snippets::sync_snippets(&mut settings);
    let credentials_changed = crate::credentials::prepare_post_process_api_keys_for_store(
        &mut settings,
        crate::credentials::CredentialStoreFailurePolicy::RejectNewValue,
    );
    let settings_value =
        serde_json::to_value(&settings).map_err(|error| format!("serialize settings: {error}"))?;
    persist_settings_value(store, settings_value, immediate_save || credentials_changed)
}

fn try_persist_settings_domain_with_immediate_save<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    settings_before: &AppSettings,
    settings: AppSettings,
    force_immediate_save: bool,
) -> Result<(), String> {
    let immediate_save =
        force_immediate_save || settings_change_requires_immediate_save(settings_before, &settings);
    write_settings_to_store_with_immediate_save(store, settings, immediate_save)
}

fn try_write_settings_domain_with_immediate_save_to_store<R, F>(
    store: &tauri_plugin_store::Store<R>,
    domain: SettingsWriteDomain,
    mut settings: AppSettings,
    force_immediate_save: bool,
    mutate: F,
) -> Result<(), String>
where
    R: tauri::Runtime,
    F: FnOnce(&mut AppSettings) -> Result<(), String>,
{
    let settings_before = settings.clone();
    try_mutate_settings_domain(&mut settings, domain, mutate)?;
    try_persist_settings_domain_with_immediate_save(
        store,
        &settings_before,
        settings,
        force_immediate_save,
    )
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

/// Domain-scoped counterpart of `mutate_settings_locked`: holds `SETTINGS_WRITE_LOCK`
/// across the whole read-modify-write so domain writes cannot lost-update against
/// concurrent locked mutations. The lock is not reentrant — never call this (or
/// `write_settings_domain`) from inside a `mutate_settings_locked` closure. Do NOT
/// `.await` or emit Tauri events inside `mutate`; emit after this returns.
pub(crate) fn try_write_settings_domain<F>(
    app: &AppHandle,
    domain: SettingsWriteDomain,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings) -> Result<(), String>,
{
    try_write_settings_domain_with_immediate_save(app, domain, false, mutate)
}

pub(crate) fn try_write_settings_domain_and_save<F>(
    app: &AppHandle,
    domain: SettingsWriteDomain,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings) -> Result<(), String>,
{
    try_write_settings_domain_with_immediate_save(app, domain, true, mutate)
}

fn try_write_settings_domain_with_immediate_save<F>(
    app: &AppHandle,
    domain: SettingsWriteDomain,
    force_immediate_save: bool,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings) -> Result<(), String>,
{
    let _guard = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let store = open_settings_store(app)?;
    let settings =
        settings_for_fallible_domain_write(load_settings_from_store(app, store.as_ref()))?;
    try_write_settings_domain_with_immediate_save_to_store(
        store.as_ref(),
        domain,
        settings,
        force_immediate_save,
        mutate,
    )?;
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
    let selected_microphone_before = next.selected_microphone.clone();
    mutate(&mut next)?;
    reconcile_selected_microphone_identity(selected_microphone_before.as_deref(), &mut next);
    *settings = next;
    Ok(())
}

pub fn reset_settings_to_defaults_with_backup(app: &AppHandle) -> Result<(), String> {
    let store = open_settings_store(app)?;

    if let Some(settings_value) = store.get("settings") {
        let backup_dir = settings_backup_directory(app)?;
        backup_settings_value_to_dir(&backup_dir, &settings_value)?;
    }

    let default_settings = get_default_settings();
    let default_value = serde_json::to_value(&default_settings)
        .map_err(|err| format!("serialize default settings: {err}"))?;
    persist_settings_value(store.as_ref(), default_value, true)
        .map_err(|err| format!("atomically reset settings: {err}"))?;
    Ok(())
}

pub fn get_bindings<R: tauri::Runtime>(app: &AppHandle<R>) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding<R: tauri::Runtime>(
    app: &AppHandle<R>,
    id: &str,
) -> Option<ShortcutBinding> {
    let bindings = get_bindings(app);

    bindings
        .get(id)
        .cloned()
        .or_else(|| get_default_settings().bindings.get(id).cloned())
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::Listener;

    fn duplicate_dictionary_settings() -> AppSettings {
        let mut settings = get_default_settings();
        settings.dictionary_schema_version = 1;
        settings.dictionary_entries = vec![
            DictionaryEntry {
                id: "dict_shared".to_string(),
                phrase: "Alpha".to_string(),
                replacement_of: None,
                source: DictionaryEntrySource::Manual,
                priority: DictionaryEntryPriority::Normal,
                created_at_ms: 1,
                updated_at_ms: 1,
                active: true,
                user_confirmed: false,
                needs_review: false,
            },
            DictionaryEntry {
                id: "dict_shared".to_string(),
                phrase: "Bravo".to_string(),
                replacement_of: None,
                source: DictionaryEntrySource::Manual,
                priority: DictionaryEntryPriority::Normal,
                created_at_ms: 2,
                updated_at_ms: 2,
                active: true,
                user_confirmed: false,
                needs_review: false,
            },
        ];
        settings
    }

    struct TestAppDataCleanup(PathBuf);

    impl Drop for TestAppDataCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn unique_store_test_app(
        name: &str,
    ) -> (tauri::App<tauri::test::MockRuntime>, TestAppDataCleanup) {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        context.config_mut().identifier = format!(
            "com.galaxyruler.verbatim.settings-test.{name}.{}.{}",
            std::process::id(),
            unique
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(context)
            .expect("build isolated Tauri test app");
        let app_data_dir = crate::portable::app_data_dir(app.handle())
            .expect("resolve isolated test app data directory");

        (app, TestAppDataCleanup(app_data_dir))
    }

    #[test]
    fn tauri_plugin_store_lockfile_is_pinned_to_atomic_fork() {
        let store_package = include_str!("../Cargo.lock")
            .split("[[package]]")
            .find(|package| {
                package
                    .lines()
                    .any(|line| line.trim() == "name = \"tauri-plugin-store\"")
            })
            .expect("tauri-plugin-store package in Cargo.lock");
        let source = store_package
            .lines()
            .find_map(|line| line.trim().strip_prefix("source = \"")?.strip_suffix('"'))
            .expect("tauri-plugin-store source in Cargo.lock");

        assert!(
            source.starts_with("git+https://github.com/GalaxyRuler/plugins-workspace"),
            "tauri-plugin-store must begin with the GalaxyRuler atomic-persistence fork source, not crates.io or another Git source; got {source}"
        );
    }

    #[test]
    fn patched_store_save_writes_valid_json_without_temp_droppings() {
        let temp_dir = tempfile::tempdir().expect("create store tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");

        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store");

        store.set("privacy", serde_json::json!(true));
        store.save().expect("save store through patched plugin");

        let directory_entries = std::fs::read_dir(
            store_path
                .parent()
                .expect("settings path has a parent directory"),
        )
        .expect("read store directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect store directory entries");
        assert_eq!(directory_entries.len(), 1, "no temporary files remain");
        assert_eq!(
            directory_entries[0].file_name(),
            std::ffi::OsStr::new("settings.json")
        );

        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&store_path).expect("read persisted store"))
                .expect("persisted store is valid JSON");
        assert_eq!(persisted["privacy"], serde_json::json!(true));
    }

    #[test]
    fn immediate_save_policy_is_limited_to_sensitive_settings_changes() {
        let before = get_default_settings();

        for mutate in [
            |settings: &mut AppSettings| settings.history_enabled = !settings.history_enabled,
            |settings: &mut AppSettings| settings.recordings_enabled = !settings.recordings_enabled,
            |settings: &mut AppSettings| settings.history_limit += 1,
            |settings: &mut AppSettings| {
                settings.recording_retention_period = RecordingRetentionPeriod::Never
            },
            |settings: &mut AppSettings| {
                settings.adaptive_profiles_enabled = !settings.adaptive_profiles_enabled
            },
            |settings: &mut AppSettings| {
                settings.context_awareness_enabled = !settings.context_awareness_enabled
            },
            |settings: &mut AppSettings| {
                settings.context_nearby_text_enabled = !settings.context_nearby_text_enabled
            },
            |settings: &mut AppSettings| {
                settings.auto_add_dictionary_words = !settings.auto_add_dictionary_words
            },
            |settings: &mut AppSettings| {
                settings.adaptive_correction_memory_enabled =
                    !settings.adaptive_correction_memory_enabled
            },
            |settings: &mut AppSettings| {
                settings.post_process_enabled = !settings.post_process_enabled
            },
            |settings: &mut AppSettings| {
                settings
                    .post_process_api_keys
                    .insert("openai".to_string(), "changed".to_string());
            },
        ] {
            let mut after = before.clone();
            mutate(&mut after);
            assert!(
                settings_change_requires_immediate_save(&before, &after),
                "sensitive settings change must force an atomic save"
            );
        }

        for mutate in [
            |settings: &mut AppSettings| settings.selected_model = "other-model".to_string(),
            |settings: &mut AppSettings| settings.app_language = "de".to_string(),
            |settings: &mut AppSettings| settings.post_process_provider_id = "ollama".to_string(),
        ] {
            let mut after = before.clone();
            mutate(&mut after);
            assert!(
                !settings_change_requires_immediate_save(&before, &after),
                "ordinary settings changes must remain debounced"
            );
        }
    }

    #[test]
    fn adaptive_correction_memory_command_uses_fallible_adaptive_domain_writer() {
        let source = include_str!("commands/adaptive.rs");
        let command_start = source
            .find("pub fn set_adaptive_correction_memory_enabled")
            .expect("find adaptive correction-memory command");
        let command = &source[command_start..];
        let command_end = command
            .find("\n}\n\n#[tauri::command]")
            .expect("find adaptive correction-memory command end");
        let command = &command[..command_end];

        assert!(command.contains("try_write_settings_domain("));
        assert!(command.contains("SettingsWriteDomain::Adaptive"));
        assert!(!command.contains("mutate_settings_locked("));
    }

    #[test]
    fn immediate_settings_save_persists_to_disk_without_auto_save() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store");

        persist_settings_value(
            store.as_ref(),
            serde_json::json!({ "history_enabled": false }),
            true,
        )
        .expect("immediately persist privacy setting");

        let persisted: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&store_path).expect("read immediately persisted settings"),
        )
        .expect("persisted settings are valid JSON");
        assert_eq!(
            persisted["settings"]["history_enabled"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn default_settings_creation_saves_immediately_without_auto_save() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store");

        let default_settings = get_default_settings();
        persist_loaded_settings_value(store.as_ref(), None, &default_settings, false)
            .expect("persist new default settings");

        let persisted: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&store_path).expect("read immediately persisted default settings"),
        )
        .expect("persisted default settings are valid JSON");
        let expected = serde_json::to_value(&default_settings).expect("serialize default settings");
        for key in [
            "history_enabled",
            "recordings_enabled",
            "adaptive_profiles_enabled",
            "context_awareness_enabled",
            "auto_add_dictionary_words",
        ] {
            assert_eq!(persisted["settings"][key], expected[key], "persisted {key}");
        }
    }

    #[test]
    fn get_settings_reconciles_legacy_bindings_without_mutating_the_store() {
        let (app, _app_data_cleanup) = unique_store_test_app("read-only-reconciliation");
        let store = app
            .store_builder(PathBuf::from(SETTINGS_STORE_PATH))
            .disable_auto_save()
            .build()
            .expect("build settings store with auto-save disabled");
        let mut legacy_settings = get_default_settings();
        legacy_settings.bindings.remove("cancel");
        let legacy_value = serde_json::to_value(&legacy_settings)
            .expect("serialize legacy settings without cancel binding");
        store.set("settings", legacy_value.clone());
        store.save().expect("seed legacy settings on disk");
        let store_path =
            settings_store_file_path(app.handle()).expect("resolve settings store path");
        let before_file = fs::read(&store_path).expect("read seeded settings store");

        let first_read = get_settings(app.handle());
        let second_read = get_settings(app.handle());

        assert!(
            first_read.bindings.contains_key("cancel"),
            "read-time reconciliation must still make legacy bindings usable"
        );
        assert!(second_read.bindings.contains_key("cancel"));
        assert_eq!(
            store.get("settings"),
            Some(legacy_value),
            "read-only settings loads must not update the Store cache"
        );
        assert_eq!(
            fs::read(&store_path).expect("read settings store after reads"),
            before_file,
            "read-only settings loads must not rewrite the Store file"
        );
    }

    #[test]
    fn unknown_stored_binding_returns_none_without_panicking() {
        let (app, _app_data_cleanup) = unique_store_test_app("unknown-stored-binding");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            get_stored_binding(app.handle(), "removed_legacy_binding")
        }));
        let binding = result.expect("a missing legacy binding must not panic during lookup");

        assert!(
            binding.is_none(),
            "an unknown legacy binding must not fabricate a shortcut"
        );
    }

    #[test]
    fn legacy_settings_without_bindings_push_to_talk_or_audio_feedback_use_defaults() {
        let defaults = get_default_settings();
        let mut legacy_value =
            serde_json::to_value(&defaults).expect("serialize default settings as legacy input");
        let object = legacy_value
            .as_object_mut()
            .expect("default settings serialize as an object");
        object.remove("bindings");
        object.remove("push_to_talk");
        object.remove("audio_feedback");

        let settings: AppSettings = serde_json::from_value(legacy_value)
            .expect("deserialize old settings without recovery");

        assert_eq!(settings.bindings.len(), defaults.bindings.len());
        assert_eq!(
            settings
                .bindings
                .get("transcribe")
                .expect("default transcribe binding")
                .current_binding,
            defaults
                .bindings
                .get("transcribe")
                .expect("default transcribe binding")
                .current_binding
        );
        assert_eq!(settings.push_to_talk, defaults.push_to_talk);
        assert_eq!(settings.audio_feedback, defaults.audio_feedback);
    }

    #[test]
    fn settings_serialization_omits_obsolete_translation_provider_fields() {
        let value =
            serde_json::to_value(get_default_settings()).expect("serialize default settings");

        assert!(value.get("translation_provider_id").is_none());
        assert!(value.get("translation_model_id").is_none());
    }

    #[test]
    fn immediate_domain_write_returns_persistence_failure_without_panicking() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let blocked_parent = temp_dir.path().join("settings-parent-file");
        std::fs::write(&blocked_parent, "not a directory").expect("create blocked parent file");
        let store_path = blocked_parent.join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store before attempted persistence");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            try_write_settings_domain_with_immediate_save_to_store(
                store.as_ref(),
                SettingsWriteDomain::Privacy,
                get_default_settings(),
                false,
                |settings| {
                    settings.history_enabled = !settings.history_enabled;
                    Ok(())
                },
            )
        }));
        let error = result
            .expect("immediate persistence failure must not panic")
            .expect_err("blocked store path must return an error");

        assert!(
            error.contains("atomically persist settings"),
            "unexpected persistence error: {error}"
        );
    }

    #[test]
    fn forced_save_mutation_rolls_back_cached_settings_on_failure() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let blocked_parent = temp_dir.path().join("settings-parent-file");
        std::fs::write(&blocked_parent, "not a directory").expect("create blocked parent file");
        let store_path = blocked_parent.join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store before attempted persistence");
        let before = get_default_settings();
        store.set(
            "settings",
            serde_json::to_value(&before).expect("serialize original settings"),
        );

        let error =
            try_mutate_settings_locked_and_save_to_store(store.as_ref(), before, |settings| {
                settings.dictionary_entries.push(DictionaryEntry {
                    id: "dict_1_robyn".to_string(),
                    phrase: "Robyn".to_string(),
                    replacement_of: None,
                    source: DictionaryEntrySource::Manual,
                    priority: DictionaryEntryPriority::Normal,
                    created_at_ms: 1,
                    updated_at_ms: 1,
                    active: true,
                    user_confirmed: false,
                    needs_review: false,
                });
                Ok(())
            })
            .expect_err("blocked store path must fail the forced save");

        assert!(error.contains("atomically persist settings"));
        let cached: AppSettings = serde_json::from_value(
            store
                .get("settings")
                .expect("original settings remain cached"),
        )
        .expect("cached settings deserialize");
        assert!(cached.dictionary_entries.is_empty());
        assert!(cached.custom_words.is_empty());
    }

    #[test]
    fn adaptive_correction_memory_domain_write_saves_immediately_without_auto_save() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build settings store");

        try_write_settings_domain_with_immediate_save_to_store(
            store.as_ref(),
            SettingsWriteDomain::Adaptive,
            get_default_settings(),
            false,
            |settings| {
                settings.adaptive_correction_memory_enabled = false;
                Ok(())
            },
        )
        .expect("persist adaptive correction-memory preference immediately");

        let persisted: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&store_path).expect("read immediately persisted settings"),
        )
        .expect("persisted settings are valid JSON");
        assert_eq!(
            persisted["settings"]["adaptive_correction_memory_enabled"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn adaptive_correction_memory_domain_write_propagates_immediate_save_failure() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let blocked_parent = temp_dir.path().join("settings-parent-file");
        std::fs::write(&blocked_parent, "not a directory").expect("create blocked parent file");
        let store_path = blocked_parent.join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store before attempted persistence");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            try_write_settings_domain_with_immediate_save_to_store(
                store.as_ref(),
                SettingsWriteDomain::Adaptive,
                get_default_settings(),
                false,
                |settings| {
                    settings.adaptive_correction_memory_enabled = false;
                    Ok(())
                },
            )
        }));
        let error = result
            .expect("adaptive correction-memory persistence failure must not panic")
            .expect_err("blocked store path must return an error");

        assert!(
            error.contains("atomically persist settings"),
            "unexpected persistence error: {error}"
        );
        assert!(
            store.get("settings").is_none(),
            "a failed adaptive correction-memory save must not retain a cache-only value"
        );
    }

    #[test]
    fn unavailable_settings_store_uses_privacy_safe_runtime_fallback() {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app without the Store plugin");
        let error = match open_settings_store(app.handle()) {
            Ok(_) => panic!("missing Store plugin state must become an initialization error"),
            Err(error) => error,
        };
        assert!(error.contains("Store plugin state unavailable"));
        let settings = get_settings(app.handle());

        assert!(!settings.history_enabled);
        assert!(!settings.recordings_enabled);
        assert!(!settings.adaptive_profiles_enabled);
        assert!(!settings.context_awareness_enabled);
        assert!(!settings.context_nearby_text_enabled);
        assert!(!settings.auto_add_dictionary_words);
        assert!(!settings.adaptive_correction_memory_enabled);
        assert!(!settings.post_process_enabled);
        assert!(settings.post_process_api_keys.is_empty());
    }

    #[test]
    fn unrelated_store_build_panic_is_not_relabelled_as_missing_plugin() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings.json");
        std::fs::write(&store_path, b"trigger custom deserializer")
            .expect("seed settings store for deserializer");

        let app = tauri::test::mock_builder()
            .plugin(
                tauri_plugin_store::Builder::new()
                    .default_deserialize_fn(|_| panic!("injected store deserializer panic"))
                    .build(),
            )
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");

        let panic = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open_settings_store_at_path(app.handle(), store_path)
        })) {
            Ok(_) => panic!("store build panic must propagate"),
            Err(panic) => panic,
        };
        let message = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str));
        assert_eq!(message, Some("injected store deserializer panic"));
    }

    #[test]
    fn clean_first_run_read_reports_default_retention_not_privacy_safe_fallback() {
        // Regression: a read-only load of a clean (empty) store must report the
        // documented first-launch defaults (history/recordings ON), not the
        // privacy-safe fallback. Early get_settings reads happen before the
        // startup loader persists; failing closed here disabled retention on
        // every fresh install (packaged first-launch smoke regression).
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");

        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build empty settings store");
        assert!(store.get("settings").is_none());

        let outcome = load_settings_from_store_read_only(store.as_ref());
        assert_eq!(outcome.load_origin, SettingsLoadOrigin::New);
        assert!(
            outcome.persistence_error.is_none(),
            "a clean first-run read must not be flagged as unpersistable"
        );

        let runtime_settings = settings_for_non_command_read(outcome);
        assert!(runtime_settings.history_enabled);
        assert!(runtime_settings.recordings_enabled);
    }

    #[test]
    fn failed_first_run_default_persist_uses_privacy_safe_runtime_fallback() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_dir = temp_dir.path().join("settings");
        std::fs::create_dir_all(&store_dir).expect("create settings directory");
        let store_path = store_dir.join("settings.json");
        std::fs::write(&store_path, b"{}").expect("seed empty settings store");

        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("load empty settings store");

        let moved_store_dir = temp_dir.path().join("settings-original");
        std::fs::rename(&store_dir, &moved_store_dir).expect("move loaded store directory");
        std::fs::write(&store_dir, "blocked settings parent")
            .expect("replace settings directory with a file");

        let outcome = load_settings_from_store(app.handle(), store.as_ref());
        assert_eq!(outcome.load_origin, SettingsLoadOrigin::New);
        assert!(outcome
            .persistence_error
            .as_deref()
            .is_some_and(|error| error.contains("atomically persist settings")));

        let runtime_settings = settings_for_non_command_read(outcome);
        assert!(!runtime_settings.history_enabled);
        assert!(!runtime_settings.recordings_enabled);
        assert!(!runtime_settings.context_awareness_enabled);
        assert!(!runtime_settings.post_process_enabled);
        assert!(runtime_settings.post_process_api_keys.is_empty());
    }

    #[test]
    fn failed_first_run_persist_does_not_leave_permissive_settings_in_store_cache() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_dir = temp_dir.path().join("settings");
        std::fs::create_dir_all(&store_dir).expect("create settings directory");
        let store_path = store_dir.join("settings.json");

        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build empty settings store");
        assert!(store.get("settings").is_none());

        let moved_store_dir = temp_dir.path().join("settings-original");
        std::fs::rename(&store_dir, &moved_store_dir).expect("move loaded store directory");
        std::fs::write(&store_dir, "blocked settings parent")
            .expect("replace settings directory with a file");

        let first_runtime_settings =
            settings_for_non_command_read(load_settings_from_store(app.handle(), store.as_ref()));
        assert!(!first_runtime_settings.history_enabled);
        assert!(!first_runtime_settings.recordings_enabled);

        let second_runtime_settings =
            settings_for_non_command_read(load_settings_from_store(app.handle(), store.as_ref()));
        assert!(
            !second_runtime_settings.history_enabled,
            "a second read of the same Store must not treat unsaved defaults as durable"
        );
        assert!(!second_runtime_settings.recordings_enabled);
        assert!(
            store.get("settings").is_none(),
            "a failed first-run save must restore the missing settings cache entry"
        );
    }

    #[test]
    fn failed_recovered_settings_repair_uses_privacy_safe_runtime_fallback_and_restores_malformed_cache(
    ) {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_dir = temp_dir.path().join("settings");
        std::fs::create_dir_all(&store_dir).expect("create settings directory");
        let store_path = store_dir.join("settings.json");
        let malformed_settings = serde_json::json!({
            "history_enabled": "not a boolean",
            "recordings_enabled": ["not a boolean"],
        });
        std::fs::write(
            &store_path,
            serde_json::to_vec(&serde_json::json!({ "settings": malformed_settings }))
                .expect("serialize malformed settings store"),
        )
        .expect("seed malformed settings store");

        let (app, _app_data_cleanup) = unique_store_test_app("failed-recovery-repair");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("load malformed settings store");
        assert_eq!(store.get("settings"), Some(malformed_settings.clone()));

        let moved_store_dir = temp_dir.path().join("settings-original");
        std::fs::rename(&store_dir, &moved_store_dir).expect("move loaded store directory");
        std::fs::write(&store_dir, "blocked settings parent")
            .expect("replace settings directory with a file");

        let first_outcome = load_settings_from_store(app.handle(), store.as_ref());
        assert_eq!(first_outcome.load_origin, SettingsLoadOrigin::Recovered);
        assert!(first_outcome
            .persistence_error
            .as_deref()
            .is_some_and(|error| error.contains("atomically persist settings")));
        let first_runtime_settings = settings_for_non_command_read(first_outcome);
        assert!(
            !first_runtime_settings.history_enabled,
            "a failed recovered-settings repair must not enable history at runtime"
        );
        assert!(
            !first_runtime_settings.recordings_enabled,
            "a failed recovered-settings repair must not enable recordings at runtime"
        );
        assert_eq!(
            store.get("settings"),
            Some(malformed_settings.clone()),
            "a failed recovery save must restore the malformed cache entry"
        );

        let command_error = settings_for_fallible_domain_write(load_settings_from_store(
            app.handle(),
            store.as_ref(),
        ))
        .expect_err("a command writer must receive the failed recovered-settings repair");
        assert!(
            command_error.contains("atomically persist settings"),
            "unexpected recovered-settings repair error: {command_error}"
        );

        let second_runtime_settings =
            settings_for_non_command_read(load_settings_from_store(app.handle(), store.as_ref()));
        assert!(
            !second_runtime_settings.history_enabled,
            "a second same-Store read must not revive recovered permissive history defaults"
        );
        assert!(
            !second_runtime_settings.recordings_enabled,
            "a second same-Store read must not revive recovered permissive recording defaults"
        );
        assert_eq!(
            store.get("settings"),
            Some(malformed_settings),
            "each failed recovery save must retain the malformed cache entry"
        );
    }

    #[test]
    fn failed_reconciliation_save_keeps_existing_privacy_settings_for_runtime_reads() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_dir = temp_dir.path().join("settings");
        std::fs::create_dir_all(&store_dir).expect("create settings directory");
        let store_path = store_dir.join("settings.json");
        let mut persisted_settings = get_default_settings();
        persisted_settings.history_enabled = false;
        persisted_settings.recordings_enabled = false;
        persisted_settings.post_process_api_keys.clear();
        let persisted_store = serde_json::json!({
            "settings": serde_json::to_value(&persisted_settings)
                .expect("serialize seeded settings"),
        });
        std::fs::write(
            &store_path,
            serde_json::to_vec(&persisted_store).expect("serialize seeded store"),
        )
        .expect("seed settings store");

        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("load seeded settings store");
        assert_eq!(store.get("settings").unwrap()["history_enabled"], false);
        assert_eq!(store.get("settings").unwrap()["recordings_enabled"], false);

        let moved_store_dir = temp_dir.path().join("settings-original");
        std::fs::rename(&store_dir, &moved_store_dir).expect("move loaded store directory");
        std::fs::write(&store_dir, "blocked settings parent")
            .expect("replace settings directory with a file");

        let outcome = load_settings_from_store(app.handle(), store.as_ref());
        assert_eq!(outcome.load_origin, SettingsLoadOrigin::Parsed);
        assert!(outcome
            .persistence_error
            .as_deref()
            .is_some_and(|error| error.contains("atomically persist settings")));

        let runtime_settings = settings_for_non_command_read(outcome);
        assert!(!runtime_settings.history_enabled);
        assert!(!runtime_settings.recordings_enabled);
        assert_eq!(
            store.get("settings"),
            Some(persisted_store["settings"].clone()),
            "a failed reconciliation save must restore the original cached settings"
        );

        let second_outcome = load_settings_from_store(app.handle(), store.as_ref());
        assert!(second_outcome
            .persistence_error
            .as_deref()
            .is_some_and(|error| error.contains("atomically persist settings")));
        let second_runtime_settings = settings_for_non_command_read(second_outcome);
        assert!(!second_runtime_settings.history_enabled);
        assert!(!second_runtime_settings.recordings_enabled);
        assert_eq!(
            store.get("settings"),
            Some(persisted_store["settings"].clone()),
            "each failed reconciliation save must leave the original cached settings intact"
        );
    }

    #[test]
    fn loaded_sensitive_post_process_key_migration_saves_immediately_without_auto_save() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store");

        let mut before = get_default_settings();
        before.post_process_api_keys.clear();
        let mut migrated = before.clone();
        assert!(ensure_post_process_defaults(&mut migrated));
        assert!(settings_change_requires_immediate_save(&before, &migrated));

        persist_loaded_settings_value(store.as_ref(), Some(&before), &migrated, false)
            .expect("persist sensitive post-process key migration");

        let persisted: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&store_path).expect("read immediately persisted settings"),
        )
        .expect("persisted settings are valid JSON");
        assert!(persisted["settings"]["post_process_api_keys"]
            .as_object()
            .is_some_and(|api_keys| !api_keys.is_empty()));
    }

    #[test]
    fn dictionary_v2_migration_saves_immediately_without_auto_save() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store");
        let before = duplicate_dictionary_settings();
        store.set(
            "settings",
            serde_json::to_value(&before).expect("serialize pre-v2 settings"),
        );

        let outcome = load_settings_from_store(app.handle(), store.as_ref());

        assert!(outcome.persistence_error.is_none());
        assert_eq!(outcome.settings.dictionary_schema_version, 2);
        assert_eq!(outcome.settings.dictionary_entries[0].id, "dict_shared");
        assert_eq!(outcome.settings.dictionary_entries[1].id, "dict_shared-2");
        let persisted: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&store_path).expect("read immediately persisted migration"),
        )
        .expect("persisted settings are valid JSON");
        assert_eq!(persisted["settings"]["dictionary_schema_version"], 2);
        assert_eq!(
            persisted["settings"]["dictionary_entries"][1]["id"],
            "dict_shared-2"
        );
    }

    #[test]
    fn failed_dictionary_v2_migration_keeps_version_entries_cache_and_events_unchanged() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_dir = temp_dir.path().join("settings");
        std::fs::create_dir_all(&store_dir).expect("create settings directory");
        let store_path = store_dir.join("settings.json");
        let before = duplicate_dictionary_settings();
        let before_value = serde_json::to_value(&before).expect("serialize pre-v2 settings");
        std::fs::write(
            &store_path,
            serde_json::to_vec(&serde_json::json!({ "settings": before_value.clone() }))
                .expect("serialize settings store"),
        )
        .expect("seed settings store");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("load seeded store");
        let emitted = Arc::new(AtomicUsize::new(0));
        let emitted_for_listener = Arc::clone(&emitted);
        let _listener = app.listen_any("dictionary-candidates-learned", move |_| {
            emitted_for_listener.fetch_add(1, Ordering::SeqCst);
        });
        let moved_store_dir = temp_dir.path().join("settings-original");
        std::fs::rename(&store_dir, &moved_store_dir).expect("move loaded store directory");
        std::fs::write(&store_dir, "block settings parent")
            .expect("replace settings directory with a file");

        let outcome = load_settings_from_store(app.handle(), store.as_ref());

        assert!(outcome
            .persistence_error
            .as_deref()
            .is_some_and(|error| error.contains("atomically persist settings")));
        assert_eq!(outcome.settings.dictionary_schema_version, 1);
        assert_eq!(
            outcome.settings.dictionary_entries,
            before.dictionary_entries
        );
        assert_eq!(store.get("settings"), Some(before_value));
        assert_eq!(emitted.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn recovered_settings_save_immediately_without_auto_save() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store");

        let recovered_before_migrations = get_default_settings();
        let recovered = recovered_before_migrations.clone();
        persist_loaded_settings_value(
            store.as_ref(),
            Some(&recovered_before_migrations),
            &recovered,
            true,
        )
        .expect("immediately persist recovered settings");

        assert!(
            store_path.exists(),
            "recovery that can replace sensitive settings must not wait for debounce"
        );
    }

    #[test]
    fn loaded_non_sensitive_binding_migration_remains_debounced() {
        let temp_dir = tempfile::tempdir().expect("create settings tempdir");
        let store_path = temp_dir.path().join("settings").join("settings.json");
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build Tauri test app");
        let store = app
            .store_builder(&store_path)
            .disable_auto_save()
            .build()
            .expect("build store");

        let mut before = get_default_settings();
        let removed_binding = before
            .bindings
            .keys()
            .next()
            .expect("default settings contain a binding")
            .clone();
        before.bindings.remove(&removed_binding);
        let mut migrated = before.clone();
        assert!(ensure_binding_defaults(&mut migrated));
        assert!(!settings_change_requires_immediate_save(&before, &migrated));

        persist_loaded_settings_value(store.as_ref(), Some(&before), &migrated, false)
            .expect("persist non-sensitive binding migration");

        assert!(
            !store_path.exists(),
            "non-sensitive migration must remain on the debounced store path"
        );
    }

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
    }

    #[test]
    fn dictionary_mutation_paths_do_not_call_write_settings_directly() {
        // Guard: all settings mutation paths in these files must go through
        // `mutate_settings_locked` or the locked domain writers
        // (`write_settings_domain` / `try_write_settings_domain`). A direct
        // `write_settings` call in any of them would reintroduce the lost-update race
        // this hardening effort removed — originally for the dictionary paths, then
        // extended to the remaining unlocked writers (audio/adaptive/transcription/
        // models/local_llm/snippets/model-manager), then to the domain-write callers
        // (shortcut/history/diagnostics/transcription-manager).
        // The substring check is safe: `write_settings_domain(` does not contain
        // `write_settings(`.
        // CWD for unit tests is the manifest dir (src-tauri), so paths are relative to it.
        for path in [
            "src/commands/dictionary.rs",
            "src/post_paste_learning.rs",
            "src/commands/audio.rs",
            "src/commands/adaptive.rs",
            "src/commands/transcription.rs",
            "src/commands/models.rs",
            "src/commands/local_llm.rs",
            "src/commands/snippets.rs",
            "src/managers/model.rs",
            "src/shortcut/mod.rs",
            "src/commands/history.rs",
            "src/commands/mod.rs",
            "src/managers/transcription.rs",
        ] {
            let source = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
            assert!(
                !source.contains("write_settings("),
                "{path} must mutate via mutate_settings_locked or the locked domain writers, not write_settings"
            );
        }
    }

    #[test]
    fn dictionary_durable_mutation_paths_use_forced_save_writer() {
        let commands = std::fs::read_to_string("src/commands/dictionary.rs")
            .expect("read dictionary commands");
        assert!(commands.contains("try_mutate_settings_locked_and_save("));
        assert!(!commands.contains("mutate_settings_locked("));

        let watcher =
            std::fs::read_to_string("src/post_paste_learning.rs").expect("read post-paste watcher");
        let learning_start = watcher
            .find("fn learn_from_text_snapshots(")
            .expect("find post-paste learning function");
        let learning_end = watcher[learning_start..]
            .find("\nfn auto_learn_outcome_log_message")
            .expect("find post-paste learning function end");
        let learning = &watcher[learning_start..learning_start + learning_end];
        assert!(learning.contains("try_mutate_settings_locked_and_save("));
        assert!(!learning.contains("mutate_settings_locked("));
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
    fn settings_domain_clears_microphone_id_only_when_selected_name_changes() {
        let mut settings = get_default_settings();
        settings.selected_microphone = Some("Old Microphone".to_string());
        settings.selected_microphone_id = Some("wasapi:OLD".to_string());

        mutate_settings_domain(&mut settings, SettingsWriteDomain::Privacy, |settings| {
            settings.history_enabled = !settings.history_enabled;
        })
        .expect("unrelated mutation should succeed");
        assert_eq!(
            settings.selected_microphone_id.as_deref(),
            Some("wasapi:OLD")
        );

        mutate_settings_domain(&mut settings, SettingsWriteDomain::Audio, |settings| {
            settings.selected_microphone = Some("New Microphone".to_string());
        })
        .expect("microphone selection mutation should succeed");
        assert_eq!(settings.selected_microphone_id, None);
    }

    #[test]
    fn microphone_identity_reconciliation_preserves_same_name_and_clears_changed_name() {
        let mut settings = get_default_settings();
        settings.selected_microphone = Some("Old Microphone".to_string());
        settings.selected_microphone_id = Some("wasapi:OLD".to_string());

        reconcile_selected_microphone_identity(Some("Old Microphone"), &mut settings);
        assert_eq!(
            settings.selected_microphone_id.as_deref(),
            Some("wasapi:OLD")
        );

        settings.selected_microphone = Some("New Microphone".to_string());
        reconcile_selected_microphone_identity(Some("Old Microphone"), &mut settings);
        assert_eq!(settings.selected_microphone_id, None);
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
    fn provider_url_policy_requires_tls_off_loopback() {
        assert!(validate_post_process_base_url("https://203.0.113.20:8000/v1", false).is_ok());
        assert!(validate_post_process_base_url("http://localhost:8000/v1", false).is_ok());
        assert!(validate_post_process_base_url("http://127.0.0.1:8000/v1", false).is_ok());
        assert!(validate_post_process_base_url("http://[::1]:8000/v1", false).is_ok());
        assert!(validate_post_process_base_url("http://203.0.113.20:8000/v1", false).is_err());
        assert!(validate_post_process_base_url("http://203.0.113.20:8000/v1", true).is_ok());
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
    fn dictation_language_setter_preserves_bcp47_casing() {
        let mut settings = get_default_settings();

        settings
            .apply_dictation_language_mode(
                DictationLanguageMode::Single,
                Some("ZH-hans".to_string()),
                vec![
                    "ZH-hans".to_string(),
                    "EN-us".to_string(),
                    "PT-br".to_string(),
                    "AR-sa".to_string(),
                ],
            )
            .expect("single language mode with BCP-47 tags");

        assert_eq!(settings.selected_language, "zh-Hans");
        assert_eq!(
            settings.adaptive_language_shortlist,
            vec!["zh-Hans", "en-US", "pt-BR", "ar-SA"]
        );
    }

    #[test]
    fn legacy_concrete_language_without_mode_loads_as_single() {
        let mut legacy_value =
            serde_json::to_value(get_default_settings()).expect("serialize default settings");
        let legacy = legacy_value
            .as_object_mut()
            .expect("settings serialize as an object");
        legacy.remove("dictation_language_mode");
        legacy.remove("dictation_language_schema_version");
        legacy.insert("selected_language".to_string(), serde_json::json!("AR-sa"));
        legacy.insert(
            "adaptive_language_shortlist".to_string(),
            serde_json::json!(["AR-sa", "EN-us"]),
        );

        let mut settings: AppSettings =
            serde_json::from_value(legacy_value.clone()).expect("deserialize legacy fixture");
        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Auto
        );

        assert!(reconcile_loaded_settings_defaults(
            &mut settings,
            Some(&legacy_value)
        ));
        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Single
        );
        assert_eq!(settings.selected_language, "ar-SA");
        assert_eq!(settings.adaptive_language_shortlist, vec!["ar-SA", "en-US"]);
        assert_eq!(settings.dictation_language_schema_version, 1);
    }

    #[test]
    fn legacy_explicit_auto_mode_with_concrete_language_loads_as_single() {
        let mut legacy_value =
            serde_json::to_value(get_default_settings()).expect("serialize default settings");
        let legacy = legacy_value
            .as_object_mut()
            .expect("settings serialize as an object");
        legacy.remove("dictation_language_schema_version");
        legacy.insert(
            "dictation_language_mode".to_string(),
            serde_json::json!("auto"),
        );
        legacy.insert("selected_language".to_string(), serde_json::json!("pt-BR"));

        let mut settings: AppSettings =
            serde_json::from_value(legacy_value.clone()).expect("deserialize legacy fixture");
        assert!(reconcile_loaded_settings_defaults(
            &mut settings,
            Some(&legacy_value)
        ));

        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Single
        );
        assert_eq!(settings.selected_language, "pt-BR");
        assert_eq!(settings.dictation_language_schema_version, 1);
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

    #[test]
    fn extra_recording_buffer_clamps_to_two_seconds() {
        assert_eq!(clamp_extra_recording_buffer_ms(99_999_999), 2_000);
        assert_eq!(clamp_extra_recording_buffer_ms(150), 150);
    }

    #[test]
    fn audio_feedback_volume_normalizes_non_finite_and_out_of_range_values() {
        assert_eq!(clamp_audio_feedback_volume(f32::NAN), 1.0);
        assert_eq!(clamp_audio_feedback_volume(-3.0), 0.0);
        assert_eq!(clamp_audio_feedback_volume(9.0), 1.0);
    }

    #[test]
    fn word_correction_threshold_normalizes_non_finite_and_out_of_range_values() {
        assert_eq!(clamp_word_correction_threshold(f64::NAN), 0.18);
        assert_eq!(clamp_word_correction_threshold(7.5), 1.0);
        assert_eq!(clamp_word_correction_threshold(-0.1), 0.0);
    }

    #[test]
    fn paste_delay_clamps_to_two_seconds() {
        assert_eq!(clamp_paste_delay_ms(60_000), 2_000);
    }
}
