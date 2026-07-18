use crate::managers::model::EngineType;
use crate::providers::{
    EngineProvider, LanguageSelection, LifecycleCost, ModelAsset, ModelLocator,
    ProviderCapabilities, SpeechInput, SpeechRequest, SpeechResponse, SpeechTaskKind,
    StreamingSupport, TranslationPairSupport,
};
use ::transcribe_rs::{
    onnx::{
        canary::CanaryModel,
        cohere::CohereModel,
        gigaam::GigaAMModel,
        moonshine::{MoonshineModel, MoonshineVariant, StreamingModel},
        parakeet::{ParakeetModel, ParakeetParams, TimestampGranularity},
        sense_voice::{SenseVoiceModel, SenseVoiceParams},
        Quantization,
    },
    whisper_cpp::{WhisperEngine, WhisperInferenceParams, WhisperLoadParams},
    SpeechModel, TranscribeOptions,
};
use anyhow::Result;
use std::path::Path;

pub enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetModel),
    Moonshine(MoonshineModel),
    MoonshineStreaming(StreamingModel),
    SenseVoice(SenseVoiceModel),
    GigaAM(GigaAMModel),
    Canary(CanaryModel),
    Cohere(CohereModel),
}

pub struct TranscribeRsProvider {
    engine: Option<LoadedEngine>,
    model_id: Option<String>,
}

impl TranscribeRsProvider {
    pub fn new() -> Self {
        Self {
            engine: None,
            model_id: None,
        }
    }

    /// Load Whisper with parameters captured by the caller while it holds the
    /// model-load lock. Other engines still use their global configuration
    /// because transcribe-rs does not expose per-load parameters for them.
    pub fn load_with_whisper_params(
        &mut self,
        asset: &ModelAsset,
        use_gpu: bool,
        requested_gpu_device: i32,
    ) -> Result<()> {
        if !matches!(asset.metadata.engine_type, EngineType::Whisper) {
            return self.load(asset);
        }

        let model_path = match &asset.locator {
            ModelLocator::File(path) | ModelLocator::Directory(path) => path,
            ModelLocator::ManagedServer { .. } | ModelLocator::ExternalHttp { .. } => {
                return Err(anyhow::anyhow!(
                    "transcribe_rs provider requires a local file or directory model asset"
                ));
            }
        };
        let params = WhisperLoadParams {
            use_gpu,
            flash_attn: false,
            gpu_device: resolve_whisper_gpu_device(use_gpu, requested_gpu_device),
        };
        self.engine = Some(LoadedEngine::Whisper(WhisperEngine::load_with_params(
            model_path, params,
        )?));
        self.model_id = Some(asset.id.clone());
        Ok(())
    }
}

impl Default for TranscribeRsProvider {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn resolve_whisper_gpu_device(use_gpu: bool, requested_device: i32) -> i32 {
    if !use_gpu {
        return 0;
    }

    if requested_device == ::transcribe_rs::accel::GPU_DEVICE_AUTO {
        0
    } else {
        requested_device
    }
}

pub(crate) fn whisper_load_params_from_globals() -> WhisperLoadParams {
    let accelerator = ::transcribe_rs::accel::get_whisper_accelerator();
    let use_gpu = accelerator.use_gpu();
    WhisperLoadParams {
        use_gpu,
        flash_attn: false,
        gpu_device: resolve_whisper_gpu_device(
            use_gpu,
            ::transcribe_rs::accel::get_whisper_gpu_device(),
        ),
    }
}

pub fn run_whisper_gpu_preflight(model_path: &Path, gpu_device: i32) -> Result<()> {
    let params = WhisperLoadParams {
        use_gpu: true,
        flash_attn: false,
        gpu_device: resolve_whisper_gpu_device(true, gpu_device),
    };
    let mut engine = WhisperEngine::load_with_params(model_path, params)
        .map_err(|e| anyhow::anyhow!("Whisper GPU preflight load failed: {}", e))?;
    let audio = vec![0.0_f32; 16_000];
    let inference_params = WhisperInferenceParams::default();
    engine
        .transcribe_with(&audio, &inference_params)
        .map_err(|e| anyhow::anyhow!("Whisper GPU preflight inference failed: {}", e))?;
    Ok(())
}

fn normalize_language_for_engine(language: &str) -> String {
    if language == "zh-Hans" || language == "zh-Hant" {
        "zh".to_string()
    } else {
        language.to_string()
    }
}

fn build_whisper_initial_prompt(
    custom_words: &[String],
    _language_shortlist: &[String],
) -> Option<String> {
    let mut prompt_parts = Vec::new();

    if !custom_words.is_empty() {
        prompt_parts.push(format!("Relevant words: {}", custom_words.join(", ")));
    }

    if prompt_parts.is_empty() {
        None
    } else {
        Some(prompt_parts.join("\n"))
    }
}

const MIN_SAMPLES_FOR_INITIAL_PROMPT: usize = 16_000;

fn should_inject_initial_prompt(sample_count: usize) -> bool {
    sample_count >= MIN_SAMPLES_FOR_INITIAL_PROMPT
}

fn strip_prompt_echo<'a>(output: &'a str, prompt: &str) -> &'a str {
    if prompt.is_empty() {
        return output;
    }

    output.trim_start().strip_prefix(prompt).unwrap_or(output)
}

fn source_language_code(source_language: &LanguageSelection) -> String {
    match source_language {
        LanguageSelection::Auto => "auto".to_string(),
        LanguageSelection::Language(language) => language.clone(),
    }
}

fn transcription_language_candidates(
    selected_language: &str,
    language_shortlist: &[String],
) -> Vec<String> {
    if selected_language != "auto" {
        return vec![normalize_language_for_engine(selected_language)];
    }

    language_shortlist
        .iter()
        .map(|language| normalize_language_for_engine(language))
        .filter(|language| language != "auto")
        .fold(Vec::<String>::new(), |mut acc, language| {
            if !acc.contains(&language) {
                acc.push(language);
            }
            acc
        })
}

fn whisper_language_hint(
    selected_language: &str,
    _language_shortlist: &[String],
) -> Option<String> {
    match selected_language {
        "" | "auto" => None,
        language => Some(language.to_string()),
    }
}

fn translation_performed_for_engine(engine_type: &EngineType, requested: bool) -> bool {
    requested && matches!(engine_type, EngineType::Whisper | EngineType::Canary)
}

fn loaded_engine_type(engine: &LoadedEngine) -> EngineType {
    match engine {
        LoadedEngine::Whisper(_) => EngineType::Whisper,
        LoadedEngine::Parakeet(_) => EngineType::Parakeet,
        LoadedEngine::Moonshine(_) => EngineType::Moonshine,
        LoadedEngine::MoonshineStreaming(_) => EngineType::MoonshineStreaming,
        LoadedEngine::SenseVoice(_) => EngineType::SenseVoice,
        LoadedEngine::GigaAM(_) => EngineType::GigaAM,
        LoadedEngine::Canary(_) => EngineType::Canary,
        LoadedEngine::Cohere(_) => EngineType::Cohere,
    }
}

fn score_text_for_language(text: &str, language: &str) -> f32 {
    let mut arabic = 0usize;
    let mut cjk = 0usize;
    let mut latin = 0usize;
    let mut letters = 0usize;

    for ch in text.chars() {
        if !ch.is_alphabetic() {
            continue;
        }

        letters += 1;
        let codepoint = ch as u32;
        if matches!(
            codepoint,
            0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
        ) {
            arabic += 1;
        } else if matches!(
            codepoint,
            0x4E00..=0x9FFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
        ) {
            cjk += 1;
        } else if ch.is_ascii_alphabetic() {
            latin += 1;
        }
    }

    if letters == 0 {
        return 0.0;
    }

    let ratio = |count: usize| count as f32 / letters as f32;
    match language {
        "ar" | "fa" | "ur" | "ps" | "sd" => {
            let arabic_ratio = ratio(arabic);
            arabic_ratio * 2.0 + if arabic > 0 { 0.25 } else { 0.0 }
        }
        "zh" | "ja" | "ko" | "zh-Hans" | "zh-Hant" => ratio(cjk) * 2.0,
        _ => ratio(latin),
    }
}

fn select_best_language_candidate(
    candidates: Vec<(String, ::transcribe_rs::TranscriptionResult)>,
) -> ::transcribe_rs::TranscriptionResult {
    candidates
        .into_iter()
        .max_by(
            |(left_language, left_result), (right_language, right_result)| {
                score_text_for_language(&left_result.text, left_language)
                    .partial_cmp(&score_text_for_language(&right_result.text, right_language))
                    .unwrap_or(std::cmp::Ordering::Equal)
            },
        )
        .map(|(_, result)| result)
        .unwrap_or(::transcribe_rs::TranscriptionResult {
            text: String::new(),
            segments: None,
        })
}

impl EngineProvider for TranscribeRsProvider {
    fn provider_id(&self) -> &'static str {
        "transcribe_rs"
    }

    fn capabilities(&self, asset: &ModelAsset) -> ProviderCapabilities {
        ProviderCapabilities {
            tasks: vec![SpeechTaskKind::Transcribe],
            translation_pairs: if asset.metadata.supports_translation {
                TranslationPairSupport::EnglishOnly
            } else {
                TranslationPairSupport::None
            },
            streaming: match asset.metadata.engine_type {
                EngineType::MoonshineStreaming => StreamingSupport::PartialText,
                _ => StreamingSupport::None,
            },
            lifecycle: LifecycleCost::Expensive,
        }
    }

    fn load(&mut self, asset: &ModelAsset) -> Result<()> {
        let model_path = match &asset.locator {
            ModelLocator::File(path) | ModelLocator::Directory(path) => path,
            ModelLocator::ManagedServer { .. } | ModelLocator::ExternalHttp { .. } => {
                return Err(anyhow::anyhow!(
                    "transcribe_rs provider requires a local file or directory model asset"
                ));
            }
        };

        let loaded = match asset.metadata.engine_type {
            EngineType::Whisper => LoadedEngine::Whisper(WhisperEngine::load_with_params(
                model_path,
                whisper_load_params_from_globals(),
            )?),
            EngineType::Parakeet => {
                LoadedEngine::Parakeet(ParakeetModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::Moonshine => LoadedEngine::Moonshine(MoonshineModel::load(
                model_path,
                MoonshineVariant::Base,
                &Quantization::default(),
            )?),
            EngineType::MoonshineStreaming => LoadedEngine::MoonshineStreaming(
                StreamingModel::load(model_path, 0, &Quantization::default())?,
            ),
            EngineType::SenseVoice => {
                LoadedEngine::SenseVoice(SenseVoiceModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::GigaAM => {
                LoadedEngine::GigaAM(GigaAMModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::Canary => {
                LoadedEngine::Canary(CanaryModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::Cohere => {
                LoadedEngine::Cohere(CohereModel::load(model_path, &Quantization::Int8)?)
            }
        };

        self.engine = Some(loaded);
        self.model_id = Some(asset.id.clone());
        Ok(())
    }

    fn unload(&mut self) {
        self.engine = None;
        self.model_id = None;
    }

    fn run(&mut self, request: SpeechRequest) -> Result<SpeechResponse> {
        let audio = match request.input {
            SpeechInput::Audio(audio) => audio,
            SpeechInput::Text(_) => {
                return Err(anyhow::anyhow!(
                    "transcribe_rs provider requires audio input for transcription"
                ));
            }
        };

        if request.cancellation.is_cancelled() {
            return Err(anyhow::anyhow!(
                "transcription cancelled before provider run"
            ));
        }

        let model_id = self
            .model_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("provider has no loaded model"))?;
        let selected_language = source_language_code(&request.source_language);
        let translation_requested = request.translation.is_some();
        let engine = self
            .engine
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("provider engine is not loaded"))?;
        let translation_performed =
            translation_performed_for_engine(&loaded_engine_type(engine), translation_requested);

        let result = match engine {
            LoadedEngine::Whisper(whisper_engine) => {
                let initial_prompt = if should_inject_initial_prompt(audio.as_ref().len()) {
                    build_whisper_initial_prompt(&request.custom_words, &request.language_shortlist)
                } else {
                    None
                };
                let whisper_language =
                    whisper_language_hint(&selected_language, &request.language_shortlist);

                let params = WhisperInferenceParams {
                    language: whisper_language,
                    translate: translation_requested,
                    initial_prompt,
                    ..Default::default()
                };

                let mut result = whisper_engine
                    .transcribe_with(audio.as_ref(), &params)
                    .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))?;
                if let Some(prompt) = params.initial_prompt.as_deref() {
                    result.text = strip_prompt_echo(&result.text, prompt).to_string();
                }
                result
            }
            LoadedEngine::Parakeet(parakeet_engine) => {
                let params = ParakeetParams {
                    timestamp_granularity: Some(TimestampGranularity::Segment),
                    ..Default::default()
                };
                parakeet_engine
                    .transcribe_with(audio.as_ref(), &params)
                    .map_err(|e| anyhow::anyhow!("Parakeet transcription failed: {}", e))?
            }
            LoadedEngine::Moonshine(moonshine_engine) => moonshine_engine
                .transcribe(audio.as_ref(), &TranscribeOptions::default())
                .map_err(|e| anyhow::anyhow!("Moonshine transcription failed: {}", e))?,
            LoadedEngine::MoonshineStreaming(streaming_engine) => streaming_engine
                .transcribe(audio.as_ref(), &TranscribeOptions::default())
                .map_err(|e| anyhow::anyhow!("Moonshine streaming transcription failed: {}", e))?,
            LoadedEngine::SenseVoice(sense_voice_engine) => {
                let language = match selected_language.as_str() {
                    "zh" | "zh-Hans" | "zh-Hant" => Some("zh".to_string()),
                    "en" => Some("en".to_string()),
                    "ja" => Some("ja".to_string()),
                    "ko" => Some("ko".to_string()),
                    "yue" => Some("yue".to_string()),
                    _ => None,
                };
                let params = SenseVoiceParams {
                    language,
                    use_itn: Some(true),
                };
                sense_voice_engine
                    .transcribe_with(audio.as_ref(), &params)
                    .map_err(|e| anyhow::anyhow!("SenseVoice transcription failed: {}", e))?
            }
            LoadedEngine::GigaAM(gigaam_engine) => gigaam_engine
                .transcribe(audio.as_ref(), &TranscribeOptions::default())
                .map_err(|e| anyhow::anyhow!("GigaAM transcription failed: {}", e))?,
            LoadedEngine::Canary(canary_engine) => {
                let lang = if selected_language == "auto" {
                    None
                } else {
                    Some(selected_language.clone())
                };
                let options = TranscribeOptions {
                    language: lang,
                    translate: translation_requested,
                    ..Default::default()
                };
                canary_engine
                    .transcribe(audio.as_ref(), &options)
                    .map_err(|e| anyhow::anyhow!("Canary transcription failed: {}", e))?
            }
            LoadedEngine::Cohere(cohere_engine) => {
                let language_candidates = transcription_language_candidates(
                    &selected_language,
                    &request.language_shortlist,
                );

                if selected_language == "auto" && language_candidates.len() > 1 {
                    let mut results = Vec::new();
                    let mut last_error = None;

                    for language in language_candidates {
                        let options = TranscribeOptions {
                            language: Some(language.clone()),
                            ..Default::default()
                        };

                        match cohere_engine.transcribe(audio.as_ref(), &options) {
                            Ok(result) => results.push((language, result)),
                            Err(error) => last_error = Some(error),
                        }
                    }

                    if results.is_empty() {
                        return Err(anyhow::anyhow!(
                            "Cohere transcription failed: {}",
                            last_error
                                .map(|error| error.to_string())
                                .unwrap_or_else(|| "no language candidates succeeded".to_string())
                        ));
                    }

                    select_best_language_candidate(results)
                } else {
                    let lang = language_candidates.first().cloned();
                    let options = TranscribeOptions {
                        language: lang,
                        ..Default::default()
                    };
                    cohere_engine
                        .transcribe(audio.as_ref(), &options)
                        .map_err(|e| anyhow::anyhow!("Cohere transcription failed: {}", e))?
                }
            }
        };

        Ok(SpeechResponse {
            text: result.text,
            detected_language: None,
            detection_confidence: None,
            translation_performed,
            provider_id: self.provider_id(),
            model_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::model::{EngineType, ModelInfo};
    use crate::providers::{ModelAsset, SpeechTaskKind, TranslationPairSupport};
    use std::path::PathBuf;
    use std::sync::Mutex;

    static ACCELERATOR_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn asset(engine_type: EngineType, supports_translation: bool) -> ModelAsset {
        ModelAsset::from_model_info(
            ModelInfo {
                id: "test".to_string(),
                name: "Test".to_string(),
                description: "Test".to_string(),
                filename: "test.bin".to_string(),
                url: None,
                sha256: None,
                size_mb: 1,
                is_downloaded: true,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type,
                license_label: "MIT".to_string(),
                accelerator_support: vec!["whisper-cpp".to_string()],
                accuracy_score: 0.0,
                speed_score: 0.0,
                supports_translation,
                is_recommended: false,
                supported_languages: vec!["en".to_string(), "ar".to_string()],
                supports_language_selection: true,
                is_custom: false,
            },
            PathBuf::from("test.bin"),
        )
    }

    #[test]
    fn transcribe_rs_provider_supports_transcription_for_current_engines() {
        let provider = TranscribeRsProvider::new();
        let capabilities = provider.capabilities(&asset(EngineType::Whisper, false));
        assert!(capabilities.supports_task(&SpeechTaskKind::Transcribe));
    }

    #[test]
    fn transcribe_rs_provider_reports_english_only_translation_when_model_supports_translation() {
        let provider = TranscribeRsProvider::new();
        let capabilities = provider.capabilities(&asset(EngineType::Whisper, true));
        assert_eq!(
            capabilities.translation_pairs,
            TranslationPairSupport::EnglishOnly
        );
    }

    #[test]
    fn whisper_load_params_disable_flash_attention_and_pin_auto_gpu_to_zero() {
        use ::transcribe_rs::accel::{
            set_whisper_accelerator, set_whisper_gpu_device, WhisperAccelerator, GPU_DEVICE_AUTO,
        };

        let _guard = ACCELERATOR_TEST_LOCK.lock().unwrap();
        set_whisper_accelerator(WhisperAccelerator::Auto);
        set_whisper_gpu_device(GPU_DEVICE_AUTO);

        let params = whisper_load_params_from_globals();

        assert!(params.use_gpu);
        assert!(!params.flash_attn);
        assert_eq!(params.gpu_device, 0);
    }

    #[test]
    fn whisper_load_params_honor_cpu_and_explicit_gpu_device() {
        use ::transcribe_rs::accel::{
            set_whisper_accelerator, set_whisper_gpu_device, WhisperAccelerator,
        };

        let _guard = ACCELERATOR_TEST_LOCK.lock().unwrap();
        set_whisper_accelerator(WhisperAccelerator::Gpu);
        set_whisper_gpu_device(1);
        let gpu_params = whisper_load_params_from_globals();
        assert!(gpu_params.use_gpu);
        assert!(!gpu_params.flash_attn);
        assert_eq!(gpu_params.gpu_device, 1);

        set_whisper_accelerator(WhisperAccelerator::CpuOnly);
        set_whisper_gpu_device(1);
        let cpu_params = whisper_load_params_from_globals();
        assert!(!cpu_params.use_gpu);
        assert!(!cpu_params.flash_attn);
        assert_eq!(cpu_params.gpu_device, 0);
    }

    #[test]
    fn whisper_initial_prompt_ignores_language_shortlist() {
        let prompt = build_whisper_initial_prompt(
            &[],
            &["en".to_string(), "ar".to_string(), "en".to_string()],
        );

        assert!(prompt.is_none());
    }

    #[test]
    fn whisper_initial_prompt_preserves_custom_words() {
        let prompt = build_whisper_initial_prompt(
            &["Verbatim".to_string(), "Codex".to_string()],
            &["auto".to_string()],
        )
        .expect("prompt should include custom words");

        assert!(prompt.contains("Relevant words: Verbatim, Codex"));
        assert!(!prompt.contains("The speech may be in these languages"));
    }

    #[test]
    fn initial_prompt_skipped_for_short_audio() {
        assert!(!should_inject_initial_prompt(12_000));
        assert!(should_inject_initial_prompt(24_000));
    }

    #[test]
    fn prompt_echo_is_stripped_from_head() {
        let prompt = "Kubernetes, Verbatim, GalaxyRuler";
        assert_eq!(
            strip_prompt_echo("Kubernetes, Verbatim, GalaxyRuler hello world", prompt),
            " hello world"
        );
        assert_eq!(strip_prompt_echo("hello world", prompt), "hello world");
    }

    #[test]
    fn normalizes_chinese_language_variants_for_engine_hints() {
        assert_eq!(normalize_language_for_engine("zh-Hans"), "zh");
        assert_eq!(normalize_language_for_engine("zh-Hant"), "zh");
        assert_eq!(normalize_language_for_engine("ar"), "ar");
    }

    #[test]
    fn uses_shortlist_as_candidates_when_language_is_auto() {
        assert_eq!(
            transcription_language_candidates(
                "auto",
                &["en".to_string(), "ar".to_string(), "en".to_string()],
            ),
            vec!["en".to_string(), "ar".to_string()]
        );
    }

    #[test]
    fn forced_language_overrides_shortlist_candidates() {
        assert_eq!(
            transcription_language_candidates("ar", &["en".to_string()]),
            vec!["ar".to_string()]
        );
    }

    #[test]
    fn whisper_auto_language_uses_native_auto_detect() {
        assert_eq!(
            whisper_language_hint("auto", &["en".to_string(), "ar".to_string()]),
            None
        );
    }

    #[test]
    fn locked_language_reaches_whisper_params() {
        let no_shortlist: Vec<String> = vec![];
        assert_eq!(
            whisper_language_hint("ar", &no_shortlist),
            Some("ar".to_string())
        );
        assert_eq!(
            whisper_language_hint("en", &no_shortlist),
            Some("en".to_string())
        );
        assert_eq!(whisper_language_hint("auto", &no_shortlist), None);
        assert_eq!(whisper_language_hint("", &no_shortlist), None);

        let shortlist = vec!["ar".to_string(), "en".to_string()];
        assert_eq!(
            whisper_language_hint("ar", &shortlist),
            Some("ar".to_string())
        );
        assert_eq!(whisper_language_hint("auto", &shortlist), None);
    }

    #[test]
    fn translation_performed_is_truthful_for_each_engine() {
        assert!(translation_performed_for_engine(&EngineType::Whisper, true));
        assert!(translation_performed_for_engine(&EngineType::Canary, true));
        assert!(!translation_performed_for_engine(
            &EngineType::Parakeet,
            true
        ));
        assert!(!translation_performed_for_engine(
            &EngineType::Whisper,
            false
        ));
    }

    #[test]
    fn cohere_candidate_selection_prefers_arabic_script_for_arabic_hint() {
        let result = select_best_language_candidate(vec![
            (
                "en".to_string(),
                ::transcribe_rs::TranscriptionResult {
                    text: "this is an unrelated English sentence".to_string(),
                    segments: None,
                },
            ),
            (
                "ar".to_string(),
                ::transcribe_rs::TranscriptionResult {
                    text: "هذا نص عربي واضح".to_string(),
                    segments: None,
                },
            ),
        ]);

        assert_eq!(result.text, "هذا نص عربي واضح");
    }
}
