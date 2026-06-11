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
    whisper_cpp::{WhisperEngine, WhisperInferenceParams},
    SpeechModel, TranscribeOptions,
};
use anyhow::Result;

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
}

impl Default for TranscribeRsProvider {
    fn default() -> Self {
        Self::new()
    }
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

fn whisper_language_hint(selected_language: &str, language_shortlist: &[String]) -> Option<String> {
    if selected_language == "auto" {
        None
    } else {
        transcription_language_candidates(selected_language, language_shortlist)
            .first()
            .cloned()
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
            EngineType::Whisper => LoadedEngine::Whisper(WhisperEngine::load(model_path)?),
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
        let translated = request.translation.is_some();

        let result = match self
            .engine
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("provider engine is not loaded"))?
        {
            LoadedEngine::Whisper(whisper_engine) => {
                let initial_prompt = build_whisper_initial_prompt(
                    &request.custom_words,
                    &request.language_shortlist,
                );
                let whisper_language =
                    whisper_language_hint(&selected_language, &request.language_shortlist);

                let params = WhisperInferenceParams {
                    language: whisper_language,
                    translate: translated,
                    initial_prompt,
                    ..Default::default()
                };

                whisper_engine
                    .transcribe_with(audio.as_ref(), &params)
                    .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))?
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
                    translate: translated,
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
            translated,
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
}
