use crate::settings::{
    AppSettings, AutoSubmitKey, ClipboardHandling, DictationLanguageMode, FormattingLevel,
    KeyboardImplementation, LLMPrompt, LogLevel, ModelUnloadTimeout, OrtAcceleratorSetting,
    OverlayPosition, PasteMethod, PostProcessProvider, RecordingRetentionPeriod, SecretMap,
    ShortcutBinding, SoundTheme, TranslationRequestSettings, TypingTool, WhisperAcceleratorSetting,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

pub(crate) const CURRENT_SETTINGS_DOMAIN_VERSION: u32 = 1;
pub(crate) const SETTINGS_DOMAIN_IDS: &[&str] = &[
    "general",
    "audio",
    "insertion",
    "privacy",
    "models",
    "post_processing",
    "diagnostics",
    "adaptive",
    "shortcuts",
];

pub(crate) fn default_settings_domain_versions() -> HashMap<String, u32> {
    SETTINGS_DOMAIN_IDS
        .iter()
        .map(|id| ((*id).to_string(), CURRENT_SETTINGS_DOMAIN_VERSION))
        .collect()
}

#[derive(Deserialize)]
struct DomainSettingsStoreDocument {
    #[serde(default)]
    settings_schema_version: Option<u32>,
    domains: VersionedSettingsDomains,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct SettingsStoreDocument {
    pub settings_schema_version: u32,
    pub domains: VersionedSettingsDomains,
}

#[cfg(test)]
pub(crate) const GENERAL_SETTINGS_DOMAIN_FIELDS: &[&str] = &[
    "start_hidden",
    "autostart_enabled",
    "update_checks_enabled",
    "overlay_position",
    "docked_pill_enabled",
    "app_language",
    "experimental_enabled",
    "show_tray_icon",
    "custom_words",
    "dictionary_entries",
    "dictionary_auto_learn_suppressed",
    "auto_add_dictionary_words",
    "snippets",
];

#[cfg(test)]
pub(crate) const AUDIO_SETTINGS_DOMAIN_FIELDS: &[&str] = &[
    "audio_feedback",
    "audio_feedback_volume",
    "sound_theme",
    "always_on_microphone",
    "selected_microphone",
    "clamshell_microphone",
    "selected_output_device",
    "mute_while_recording",
    "extra_recording_buffer_ms",
];

#[cfg(test)]
pub(crate) const INSERTION_SETTINGS_DOMAIN_FIELDS: &[&str] = &[
    "paste_method",
    "clipboard_handling",
    "auto_submit",
    "auto_submit_key",
    "append_trailing_space",
    "paste_delay_ms",
    "typing_tool",
    "external_script_path",
];

#[cfg(test)]
pub(crate) const PRIVACY_SETTINGS_DOMAIN_FIELDS: &[&str] = &[
    "history_enabled",
    "recordings_enabled",
    "history_limit",
    "recording_retention_period",
];

#[cfg(test)]
pub(crate) const MODELS_SETTINGS_DOMAIN_FIELDS: &[&str] = &[
    "selected_model",
    "model_unload_timeout",
    "local_llm",
    "whisper_accelerator",
    "ort_accelerator",
    "whisper_gpu_device",
];

#[cfg(test)]
pub(crate) const POST_PROCESSING_SETTINGS_DOMAIN_FIELDS: &[&str] = &[
    "post_process_enabled",
    "formatting_level",
    "post_process_provider_id",
    "post_process_providers",
    "post_process_api_keys",
    "post_process_models",
    "post_process_prompts",
    "post_process_selected_prompt_id",
    "translate_to_english",
    "translation_enabled",
    "translation_request",
    "translation_provider_id",
    "translation_model_id",
];

#[cfg(test)]
pub(crate) const DIAGNOSTICS_SETTINGS_DOMAIN_FIELDS: &[&str] =
    &["debug_mode", "log_level", "lazy_stream_close"];

#[cfg(test)]
pub(crate) const ADAPTIVE_SETTINGS_DOMAIN_FIELDS: &[&str] = &[
    "selected_language",
    "dictation_language_mode",
    "word_correction_threshold",
    "custom_filler_words",
    "adaptive_profiles_enabled",
    "context_awareness_enabled",
    "context_nearby_text_enabled",
    "adaptive_language_shortlist",
    "adaptive_default_profile_id",
    "adaptive_profiles",
    "adaptive_correction_memory_enabled",
    "adaptive_private_app_patterns",
];

#[cfg(test)]
pub(crate) const SHORTCUTS_SETTINGS_DOMAIN_FIELDS: &[&str] =
    &["bindings", "push_to_talk", "keyboard_implementation"];

#[cfg(test)]
pub(crate) const SETTINGS_DOMAIN_FIELD_GROUPS: &[(&str, &[&str])] = &[
    ("general", GENERAL_SETTINGS_DOMAIN_FIELDS),
    ("audio", AUDIO_SETTINGS_DOMAIN_FIELDS),
    ("insertion", INSERTION_SETTINGS_DOMAIN_FIELDS),
    ("privacy", PRIVACY_SETTINGS_DOMAIN_FIELDS),
    ("models", MODELS_SETTINGS_DOMAIN_FIELDS),
    ("post_processing", POST_PROCESSING_SETTINGS_DOMAIN_FIELDS),
    ("diagnostics", DIAGNOSTICS_SETTINGS_DOMAIN_FIELDS),
    ("adaptive", ADAPTIVE_SETTINGS_DOMAIN_FIELDS),
    ("shortcuts", SHORTCUTS_SETTINGS_DOMAIN_FIELDS),
];

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct VersionedSettingsDomains {
    pub general: GeneralSettingsDomain,
    pub audio: AudioSettingsDomain,
    pub insertion: InsertionSettingsDomain,
    pub privacy: PrivacySettingsDomain,
    pub models: ModelsSettingsDomain,
    pub post_processing: PostProcessingSettingsDomain,
    pub diagnostics: DiagnosticsSettingsDomain,
    pub adaptive: AdaptiveSettingsDomain,
    pub shortcuts: ShortcutsSettingsDomain,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct GeneralSettingsDomain {
    pub version: u32,
    pub start_hidden: bool,
    pub autostart_enabled: bool,
    pub update_checks_enabled: bool,
    pub overlay_position: OverlayPosition,
    pub docked_pill_enabled: bool,
    pub app_language: String,
    pub experimental_enabled: bool,
    pub show_tray_icon: bool,
    pub custom_words: Vec<String>,
    pub dictionary_entries: Vec<crate::settings::DictionaryEntry>,
    pub dictionary_auto_learn_suppressed: Vec<String>,
    pub auto_add_dictionary_words: bool,
    pub snippets: Vec<crate::snippets::SnippetEntry>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AudioSettingsDomain {
    pub version: u32,
    pub audio_feedback: bool,
    pub audio_feedback_volume: f32,
    pub sound_theme: SoundTheme,
    pub always_on_microphone: bool,
    pub selected_microphone: Option<String>,
    pub clamshell_microphone: Option<String>,
    pub selected_output_device: Option<String>,
    pub mute_while_recording: bool,
    pub extra_recording_buffer_ms: u64,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct InsertionSettingsDomain {
    pub version: u32,
    pub paste_method: PasteMethod,
    pub clipboard_handling: ClipboardHandling,
    pub auto_submit: bool,
    pub auto_submit_key: AutoSubmitKey,
    pub append_trailing_space: bool,
    pub paste_delay_ms: u64,
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PrivacySettingsDomain {
    pub version: u32,
    pub history_enabled: bool,
    pub recordings_enabled: bool,
    pub history_limit: usize,
    pub recording_retention_period: RecordingRetentionPeriod,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ModelsSettingsDomain {
    pub version: u32,
    pub selected_model: String,
    pub model_unload_timeout: ModelUnloadTimeout,
    pub local_llm: crate::local_llm::LocalLlmSettings,
    pub whisper_accelerator: WhisperAcceleratorSetting,
    pub ort_accelerator: OrtAcceleratorSetting,
    pub whisper_gpu_device: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessingSettingsDomain {
    pub version: u32,
    pub post_process_enabled: bool,
    pub formatting_level: FormattingLevel,
    pub post_process_provider_id: String,
    pub post_process_providers: Vec<PostProcessProvider>,
    pub post_process_api_keys: SecretMap,
    pub post_process_models: HashMap<String, String>,
    pub post_process_prompts: Vec<LLMPrompt>,
    pub post_process_selected_prompt_id: Option<String>,
    pub translate_to_english: bool,
    pub translation_enabled: bool,
    pub translation_request: Option<TranslationRequestSettings>,
    pub translation_provider_id: Option<String>,
    pub translation_model_id: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct DiagnosticsSettingsDomain {
    pub version: u32,
    pub debug_mode: bool,
    pub log_level: LogLevel,
    pub lazy_stream_close: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AdaptiveSettingsDomain {
    pub version: u32,
    pub selected_language: String,
    pub dictation_language_mode: DictationLanguageMode,
    pub word_correction_threshold: f64,
    pub custom_filler_words: Option<Vec<String>>,
    pub adaptive_profiles_enabled: bool,
    pub context_awareness_enabled: bool,
    pub context_nearby_text_enabled: bool,
    pub adaptive_language_shortlist: Vec<String>,
    pub adaptive_default_profile_id: String,
    pub adaptive_profiles: Vec<crate::adaptive::profile::AdaptiveProfile>,
    pub adaptive_correction_memory_enabled: bool,
    pub adaptive_private_app_patterns: Vec<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutsSettingsDomain {
    pub version: u32,
    pub bindings: HashMap<String, ShortcutBinding>,
    pub push_to_talk: bool,
    pub keyboard_implementation: KeyboardImplementation,
}

fn domain_version(settings: &AppSettings, id: &str) -> u32 {
    settings
        .settings_domain_versions
        .get(id)
        .copied()
        .unwrap_or(CURRENT_SETTINGS_DOMAIN_VERSION)
}

impl VersionedSettingsDomains {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn into_flat_settings(self) -> AppSettings {
        let mut settings = crate::settings::get_default_settings();

        settings.settings_domain_versions = HashMap::from([
            ("general".to_string(), self.general.version),
            ("audio".to_string(), self.audio.version),
            ("insertion".to_string(), self.insertion.version),
            ("privacy".to_string(), self.privacy.version),
            ("models".to_string(), self.models.version),
            ("post_processing".to_string(), self.post_processing.version),
            ("diagnostics".to_string(), self.diagnostics.version),
            ("adaptive".to_string(), self.adaptive.version),
            ("shortcuts".to_string(), self.shortcuts.version),
        ]);

        settings.start_hidden = self.general.start_hidden;
        settings.autostart_enabled = self.general.autostart_enabled;
        settings.update_checks_enabled = self.general.update_checks_enabled;
        settings.overlay_position = self.general.overlay_position;
        settings.docked_pill_enabled = self.general.docked_pill_enabled;
        settings.app_language = self.general.app_language;
        settings.experimental_enabled = self.general.experimental_enabled;
        settings.show_tray_icon = self.general.show_tray_icon;
        settings.custom_words = self.general.custom_words;
        settings.dictionary_entries = self.general.dictionary_entries;
        settings.dictionary_auto_learn_suppressed = self.general.dictionary_auto_learn_suppressed;
        settings.auto_add_dictionary_words = self.general.auto_add_dictionary_words;
        settings.snippets = self.general.snippets;

        settings.audio_feedback = self.audio.audio_feedback;
        settings.audio_feedback_volume = self.audio.audio_feedback_volume;
        settings.sound_theme = self.audio.sound_theme;
        settings.always_on_microphone = self.audio.always_on_microphone;
        settings.selected_microphone = self.audio.selected_microphone;
        settings.clamshell_microphone = self.audio.clamshell_microphone;
        settings.selected_output_device = self.audio.selected_output_device;
        settings.mute_while_recording = self.audio.mute_while_recording;
        settings.extra_recording_buffer_ms = self.audio.extra_recording_buffer_ms;

        settings.paste_method = self.insertion.paste_method;
        settings.clipboard_handling = self.insertion.clipboard_handling;
        settings.auto_submit = self.insertion.auto_submit;
        settings.auto_submit_key = self.insertion.auto_submit_key;
        settings.append_trailing_space = self.insertion.append_trailing_space;
        settings.paste_delay_ms = self.insertion.paste_delay_ms;
        settings.typing_tool = self.insertion.typing_tool;
        settings.external_script_path = self.insertion.external_script_path;

        settings.history_enabled = self.privacy.history_enabled;
        settings.recordings_enabled = self.privacy.recordings_enabled;
        settings.history_limit = self.privacy.history_limit;
        settings.recording_retention_period = self.privacy.recording_retention_period;

        settings.selected_model = self.models.selected_model;
        settings.model_unload_timeout = self.models.model_unload_timeout;
        settings.local_llm = self.models.local_llm;
        settings.whisper_accelerator = self.models.whisper_accelerator;
        settings.ort_accelerator = self.models.ort_accelerator;
        settings.whisper_gpu_device = self.models.whisper_gpu_device;

        settings.post_process_enabled = self.post_processing.post_process_enabled;
        settings.formatting_level = self.post_processing.formatting_level;
        settings.post_process_provider_id = self.post_processing.post_process_provider_id;
        settings.post_process_providers = self.post_processing.post_process_providers;
        settings.post_process_api_keys = self.post_processing.post_process_api_keys;
        settings.post_process_models = self.post_processing.post_process_models;
        settings.post_process_prompts = self.post_processing.post_process_prompts;
        settings.post_process_selected_prompt_id =
            self.post_processing.post_process_selected_prompt_id;
        settings.translate_to_english = self.post_processing.translate_to_english;
        settings.translation_enabled = self.post_processing.translation_enabled;
        settings.translation_request = self.post_processing.translation_request;
        settings.translation_provider_id = self.post_processing.translation_provider_id;
        settings.translation_model_id = self.post_processing.translation_model_id;

        settings.debug_mode = self.diagnostics.debug_mode;
        settings.log_level = self.diagnostics.log_level;
        settings.lazy_stream_close = self.diagnostics.lazy_stream_close;

        settings.selected_language = self.adaptive.selected_language;
        settings.dictation_language_mode = self.adaptive.dictation_language_mode;
        settings.word_correction_threshold = self.adaptive.word_correction_threshold;
        settings.custom_filler_words = self.adaptive.custom_filler_words;
        settings.adaptive_profiles_enabled = self.adaptive.adaptive_profiles_enabled;
        settings.context_awareness_enabled = self.adaptive.context_awareness_enabled;
        settings.context_nearby_text_enabled = self.adaptive.context_nearby_text_enabled;
        settings.adaptive_language_shortlist = self.adaptive.adaptive_language_shortlist;
        settings.adaptive_default_profile_id = self.adaptive.adaptive_default_profile_id;
        settings.adaptive_profiles = self.adaptive.adaptive_profiles;
        settings.adaptive_correction_memory_enabled =
            self.adaptive.adaptive_correction_memory_enabled;
        settings.adaptive_private_app_patterns = self.adaptive.adaptive_private_app_patterns;

        settings.bindings = self.shortcuts.bindings;
        settings.push_to_talk = self.shortcuts.push_to_talk;
        settings.keyboard_implementation = self.shortcuts.keyboard_implementation;

        settings
    }
}

fn merge_missing_json_fields(target: &mut serde_json::Value, defaults: &serde_json::Value) {
    let (Some(target_object), Some(default_object)) =
        (target.as_object_mut(), defaults.as_object())
    else {
        return;
    };

    for (key, default_value) in default_object {
        match target_object.get_mut(key) {
            Some(target_value) => merge_missing_json_fields(target_value, default_value),
            None => {
                target_object.insert(key.clone(), default_value.clone());
            }
        }
    }
}

fn migrate_domain_settings_store_value(mut value: serde_json::Value) -> serde_json::Value {
    let Ok(default_value) = settings_store_value(&crate::settings::get_default_settings()) else {
        return value;
    };
    let Some(default_domains) = default_value.get("domains") else {
        return value;
    };
    let Some(domains) = value.get_mut("domains") else {
        return value;
    };

    merge_missing_json_fields(domains, default_domains);
    value
}

pub(crate) fn parse_settings_store_value(
    value: serde_json::Value,
) -> Result<AppSettings, serde_json::Error> {
    match serde_json::from_value::<DomainSettingsStoreDocument>(
        migrate_domain_settings_store_value(value.clone()),
    ) {
        Ok(document) => {
            let mut settings = document.domains.into_flat_settings();
            if let Some(version) = document.settings_schema_version {
                settings.settings_schema_version = version;
            }
            Ok(settings)
        }
        Err(_) => serde_json::from_value::<AppSettings>(value),
    }
}

pub(crate) fn settings_store_value(
    settings: &AppSettings,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(settings_store_document(settings))
}

pub fn settings_store_document(settings: &AppSettings) -> SettingsStoreDocument {
    SettingsStoreDocument {
        settings_schema_version: settings.settings_schema_version,
        domains: settings.domains(),
    }
}

impl AppSettings {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn domains(&self) -> VersionedSettingsDomains {
        VersionedSettingsDomains {
            general: GeneralSettingsDomain {
                version: domain_version(self, "general"),
                start_hidden: self.start_hidden,
                autostart_enabled: self.autostart_enabled,
                update_checks_enabled: self.update_checks_enabled,
                overlay_position: self.overlay_position,
                docked_pill_enabled: self.docked_pill_enabled,
                app_language: self.app_language.clone(),
                experimental_enabled: self.experimental_enabled,
                show_tray_icon: self.show_tray_icon,
                custom_words: self.custom_words.clone(),
                dictionary_entries: self.dictionary_entries.clone(),
                dictionary_auto_learn_suppressed: self.dictionary_auto_learn_suppressed.clone(),
                auto_add_dictionary_words: self.auto_add_dictionary_words,
                snippets: self.snippets.clone(),
            },
            audio: AudioSettingsDomain {
                version: domain_version(self, "audio"),
                audio_feedback: self.audio_feedback,
                audio_feedback_volume: self.audio_feedback_volume,
                sound_theme: self.sound_theme,
                always_on_microphone: self.always_on_microphone,
                selected_microphone: self.selected_microphone.clone(),
                clamshell_microphone: self.clamshell_microphone.clone(),
                selected_output_device: self.selected_output_device.clone(),
                mute_while_recording: self.mute_while_recording,
                extra_recording_buffer_ms: self.extra_recording_buffer_ms,
            },
            insertion: InsertionSettingsDomain {
                version: domain_version(self, "insertion"),
                paste_method: self.paste_method,
                clipboard_handling: self.clipboard_handling,
                auto_submit: self.auto_submit,
                auto_submit_key: self.auto_submit_key,
                append_trailing_space: self.append_trailing_space,
                paste_delay_ms: self.paste_delay_ms,
                typing_tool: self.typing_tool,
                external_script_path: self.external_script_path.clone(),
            },
            privacy: PrivacySettingsDomain {
                version: domain_version(self, "privacy"),
                history_enabled: self.history_enabled,
                recordings_enabled: self.recordings_enabled,
                history_limit: self.history_limit,
                recording_retention_period: self.recording_retention_period,
            },
            models: ModelsSettingsDomain {
                version: domain_version(self, "models"),
                selected_model: self.selected_model.clone(),
                model_unload_timeout: self.model_unload_timeout,
                local_llm: self.local_llm.clone(),
                whisper_accelerator: self.whisper_accelerator,
                ort_accelerator: self.ort_accelerator,
                whisper_gpu_device: self.whisper_gpu_device,
            },
            post_processing: PostProcessingSettingsDomain {
                version: domain_version(self, "post_processing"),
                post_process_enabled: self.post_process_enabled,
                formatting_level: self.formatting_level,
                post_process_provider_id: self.post_process_provider_id.clone(),
                post_process_providers: self.post_process_providers.clone(),
                post_process_api_keys: self.post_process_api_keys.clone(),
                post_process_models: self.post_process_models.clone(),
                post_process_prompts: self.post_process_prompts.clone(),
                post_process_selected_prompt_id: self.post_process_selected_prompt_id.clone(),
                translate_to_english: self.translate_to_english,
                translation_enabled: self.translation_enabled,
                translation_request: self.translation_request.clone(),
                translation_provider_id: self.translation_provider_id.clone(),
                translation_model_id: self.translation_model_id.clone(),
            },
            diagnostics: DiagnosticsSettingsDomain {
                version: domain_version(self, "diagnostics"),
                debug_mode: self.debug_mode,
                log_level: self.log_level,
                lazy_stream_close: self.lazy_stream_close,
            },
            adaptive: AdaptiveSettingsDomain {
                version: domain_version(self, "adaptive"),
                selected_language: self.selected_language.clone(),
                dictation_language_mode: self.dictation_language_mode,
                word_correction_threshold: self.word_correction_threshold,
                custom_filler_words: self.custom_filler_words.clone(),
                adaptive_profiles_enabled: self.adaptive_profiles_enabled,
                context_awareness_enabled: self.context_awareness_enabled,
                context_nearby_text_enabled: self.context_nearby_text_enabled,
                adaptive_language_shortlist: self.adaptive_language_shortlist.clone(),
                adaptive_default_profile_id: self.adaptive_default_profile_id.clone(),
                adaptive_profiles: self.adaptive_profiles.clone(),
                adaptive_correction_memory_enabled: self.adaptive_correction_memory_enabled,
                adaptive_private_app_patterns: self.adaptive_private_app_patterns.clone(),
            },
            shortcuts: ShortcutsSettingsDomain {
                version: domain_version(self, "shortcuts"),
                bindings: self.bindings.clone(),
                push_to_talk: self.push_to_talk,
                keyboard_implementation: self.keyboard_implementation,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{
        get_default_settings, AutoSubmitKey, ClipboardHandling, DictationLanguageMode,
        FormattingLevel, LogLevel, ModelUnloadTimeout, PasteMethod, RecordingRetentionPeriod,
        TypingTool,
    };

    #[test]
    fn domain_document_deserializes_and_rebuilds_flat_settings() {
        let mut settings = get_default_settings();
        settings
            .settings_domain_versions
            .insert("general".to_string(), 3);
        settings
            .settings_domain_versions
            .insert("post_processing".to_string(), 4);
        settings.start_hidden = true;
        settings.autostart_enabled = true;
        settings.update_checks_enabled = false;
        settings.app_language = "en-US".to_string();
        settings.custom_words = vec!["VerbatimTerm".to_string()];
        settings.audio_feedback = true;
        settings.audio_feedback_volume = 0.42;
        settings.always_on_microphone = true;
        settings.selected_microphone = Some("Microphone Array".to_string());
        settings.paste_method = PasteMethod::ExternalScript;
        settings.clipboard_handling = ClipboardHandling::CopyToClipboard;
        settings.auto_submit = true;
        settings.auto_submit_key = AutoSubmitKey::CtrlEnter;
        settings.typing_tool = TypingTool::Xdotool;
        settings.external_script_path = Some("C:\\tools\\verbatim-insert.ps1".to_string());
        settings.history_enabled = false;
        settings.recordings_enabled = false;
        settings.history_limit = 12;
        settings.recording_retention_period = RecordingRetentionPeriod::Weeks2;
        settings.selected_model = "verbatim-smoke-model".to_string();
        settings.model_unload_timeout = ModelUnloadTimeout::Min10;
        settings.post_process_enabled = true;
        settings.formatting_level = FormattingLevel::High;
        settings.post_process_provider_id = "lm_studio".to_string();
        settings
            .post_process_api_keys
            .insert("lm_studio".to_string(), "stored-value".to_string());
        settings.debug_mode = true;
        settings.log_level = LogLevel::Debug;
        settings.lazy_stream_close = true;
        settings.selected_language = "en".to_string();
        settings.dictation_language_mode = DictationLanguageMode::Single;
        settings.adaptive_profiles_enabled = true;
        settings.context_awareness_enabled = true;
        settings.adaptive_language_shortlist = vec!["en".to_string()];
        settings.push_to_talk = false;

        let domain_value = serde_json::to_value(settings.domains()).expect("domain json");
        let domain_document: VersionedSettingsDomains =
            serde_json::from_value(domain_value).expect("domain document parses");
        let rebuilt = domain_document.into_flat_settings();

        assert_eq!(
            serde_json::to_value(&rebuilt).expect("rebuilt settings json"),
            serde_json::to_value(&settings).expect("original settings json")
        );
    }

    #[test]
    fn settings_store_parser_accepts_flat_and_domain_documents() {
        let mut settings = get_default_settings();
        settings.settings_schema_version = 11;
        settings
            .settings_domain_versions
            .insert("privacy".to_string(), 5);
        settings.history_enabled = false;
        settings.recordings_enabled = false;
        settings.selected_model = "verbatim-smoke-model".to_string();

        let flat_value = serde_json::to_value(&settings).expect("flat settings json");
        let parsed_flat = parse_settings_store_value(flat_value).expect("flat settings parse");
        assert_eq!(
            serde_json::to_value(&parsed_flat).expect("parsed flat json"),
            serde_json::to_value(&settings).expect("original settings json")
        );

        let domain_value = serde_json::json!({
            "settings_schema_version": settings.settings_schema_version,
            "domains": settings.domains(),
        });
        let parsed_domain =
            parse_settings_store_value(domain_value).expect("domain settings parse");

        assert_eq!(
            serde_json::to_value(&parsed_domain).expect("parsed domain json"),
            serde_json::to_value(&settings).expect("original settings json")
        );
    }

    #[test]
    fn settings_store_value_writes_domain_document_that_parser_accepts() {
        let mut settings = get_default_settings();
        settings.settings_schema_version = 8;
        settings.selected_model = "verbatim-smoke-model".to_string();
        settings.history_enabled = false;

        let store_value = settings_store_value(&settings).expect("settings store value");
        assert!(store_value.get("domains").is_some());
        assert!(store_value.get("settings_domain_versions").is_none());

        let parsed = parse_settings_store_value(store_value).expect("domain settings parse");
        assert_eq!(
            serde_json::to_value(&parsed).expect("parsed settings json"),
            serde_json::to_value(&settings).expect("original settings json")
        );
    }

    #[test]
    fn settings_store_parser_migrates_domain_documents_with_missing_fields() {
        let mut settings = get_default_settings();
        settings.settings_schema_version = 8;
        settings.selected_model = "verbatim-smoke-model".to_string();
        settings.history_enabled = false;
        settings.recordings_enabled = false;
        settings.show_tray_icon = false;

        let mut store_value = settings_store_value(&settings).expect("settings store value");
        let domains = store_value
            .get_mut("domains")
            .and_then(serde_json::Value::as_object_mut)
            .expect("domains object");
        domains
            .get_mut("privacy")
            .and_then(serde_json::Value::as_object_mut)
            .expect("privacy domain")
            .remove("recordings_enabled");
        domains
            .get_mut("general")
            .and_then(serde_json::Value::as_object_mut)
            .expect("general domain")
            .remove("show_tray_icon");

        let parsed =
            parse_settings_store_value(store_value).expect("migrated domain settings parse");

        assert_eq!(parsed.settings_schema_version, 8);
        assert_eq!(parsed.selected_model, "verbatim-smoke-model");
        assert!(!parsed.history_enabled);
        assert!(parsed.recordings_enabled);
        assert!(parsed.show_tray_icon);
    }

    #[test]
    fn settings_store_parser_migrates_domain_documents_with_missing_domains() {
        let mut settings = get_default_settings();
        settings.settings_schema_version = 8;
        settings.selected_model = "verbatim-smoke-model".to_string();
        settings.debug_mode = true;
        settings.log_level = LogLevel::Debug;

        let mut store_value = settings_store_value(&settings).expect("settings store value");
        store_value
            .get_mut("domains")
            .and_then(serde_json::Value::as_object_mut)
            .expect("domains object")
            .remove("diagnostics");

        let parsed =
            parse_settings_store_value(store_value).expect("migrated domain settings parse");

        assert_eq!(parsed.settings_schema_version, 8);
        assert_eq!(parsed.selected_model, "verbatim-smoke-model");
        assert!(!parsed.debug_mode);
        assert_eq!(parsed.log_level, LogLevel::Info);
    }
}
