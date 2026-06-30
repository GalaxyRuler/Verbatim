//! Offline ASR wrapper for accurate final transcripts.

use crate::asr::AsrModelPaths;

#[cfg(all(feature = "android-asr", any(target_os = "android", target_os = "ios")))]
mod platform {
    use super::*;

    pub struct OfflineRecognizer {
        inner: sherpa_onnx::OfflineRecognizer,
    }

    impl OfflineRecognizer {
        pub fn new(paths: &AsrModelPaths, language: &str) -> anyhow::Result<Self> {
            Self::from_config(whisper_config(paths, language))
        }

        pub fn new_sense_voice(paths: &AsrModelPaths) -> anyhow::Result<Self> {
            Self::from_config(sense_voice_config(paths))
        }

        pub fn new_canary(paths: &AsrModelPaths) -> anyhow::Result<Self> {
            Self::from_config(canary_config(paths))
        }

        pub fn new_canary_for_language(
            paths: &AsrModelPaths,
            language: &str,
        ) -> anyhow::Result<Self> {
            Self::from_config(canary_config_for_language(paths, language))
        }

        pub fn new_moonshine(paths: &AsrModelPaths) -> anyhow::Result<Self> {
            Self::from_config(moonshine_config(paths))
        }

        pub fn new_parakeet(paths: &AsrModelPaths) -> anyhow::Result<Self> {
            Self::from_config(parakeet_config(paths))
        }

        fn from_config(config: sherpa_onnx::OfflineRecognizerConfig) -> anyhow::Result<Self> {
            let inner = sherpa_onnx::OfflineRecognizer::create(&config).ok_or_else(|| {
                anyhow::anyhow!("failed to create offline sherpa-onnx recognizer")
            })?;

            Ok(Self { inner })
        }

        pub fn transcribe(&mut self, sample_rate: i32, samples: &[f32]) -> anyhow::Result<String> {
            let stream = self.inner.create_stream();
            stream.accept_waveform(sample_rate, samples);
            self.inner.decode(&stream);
            let result = stream.get_result().ok_or_else(|| {
                anyhow::anyhow!("offline sherpa-onnx recognizer returned no result")
            })?;

            Ok(result.text)
        }
    }

    fn whisper_config(
        paths: &AsrModelPaths,
        language: &str,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        config.model_config.whisper = sherpa_onnx::OfflineWhisperModelConfig {
            encoder: Some(path_to_string(&paths.whisper_encoder)),
            decoder: Some(path_to_string(&paths.whisper_decoder)),
            language: Some(language.to_string()),
            task: Some("transcribe".to_string()),
            ..Default::default()
        };
        config.model_config.tokens = Some(path_to_string(&paths.whisper_tokens));
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = 2;
        config.decoding_method = Some("greedy_search".to_string());
        config
    }

    fn sense_voice_config(paths: &AsrModelPaths) -> sherpa_onnx::OfflineRecognizerConfig {
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        config.model_config.sense_voice = sherpa_onnx::OfflineSenseVoiceModelConfig {
            model: Some(path_to_string(&paths.sense_voice_model)),
            language: Some("auto".to_string()),
            use_itn: true,
        };
        config.model_config.tokens = Some(path_to_string(&paths.sense_voice_tokens));
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = 2;
        config.decoding_method = Some("greedy_search".to_string());
        config
    }

    fn canary_config(paths: &AsrModelPaths) -> sherpa_onnx::OfflineRecognizerConfig {
        canary_config_for_language(paths, "en")
    }

    fn canary_config_for_language(
        paths: &AsrModelPaths,
        language: &str,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        let source_language = normalize_canary_language(language);
        config.model_config.canary = sherpa_onnx::OfflineCanaryModelConfig {
            encoder: Some(path_to_string(&paths.canary_encoder)),
            decoder: Some(path_to_string(&paths.canary_decoder)),
            src_lang: Some(source_language.to_string()),
            tgt_lang: Some(source_language.to_string()),
            use_pnc: true,
        };
        config.model_config.tokens = Some(path_to_string(&paths.canary_tokens));
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = 2;
        config.decoding_method = Some("greedy_search".to_string());
        config
    }

    fn moonshine_config(paths: &AsrModelPaths) -> sherpa_onnx::OfflineRecognizerConfig {
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        config.model_config.moonshine = sherpa_onnx::OfflineMoonshineModelConfig {
            preprocessor: Some(path_to_string(&paths.moonshine_preprocessor)),
            encoder: Some(path_to_string(&paths.moonshine_encoder)),
            uncached_decoder: Some(path_to_string(&paths.moonshine_uncached_decoder)),
            cached_decoder: Some(path_to_string(&paths.moonshine_cached_decoder)),
            ..Default::default()
        };
        config.model_config.tokens = Some(path_to_string(&paths.moonshine_tokens));
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = 2;
        config.decoding_method = Some("greedy_search".to_string());
        config
    }

    fn parakeet_config(paths: &AsrModelPaths) -> sherpa_onnx::OfflineRecognizerConfig {
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        config.model_config.transducer = sherpa_onnx::OfflineTransducerModelConfig {
            encoder: Some(path_to_string(&paths.parakeet_encoder)),
            decoder: Some(path_to_string(&paths.parakeet_decoder)),
            joiner: Some(path_to_string(&paths.parakeet_joiner)),
        };
        config.model_config.tokens = Some(path_to_string(&paths.parakeet_tokens));
        config.model_config.model_type = Some("nemo_transducer".to_string());
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = 2;
        config.decoding_method = Some("greedy_search".to_string());
        config
    }

    fn normalize_canary_language(language: &str) -> &'static str {
        let normalized = language
            .trim()
            .split(['-', '_'])
            .next()
            .unwrap_or("en")
            .to_ascii_lowercase();
        match normalized.as_str() {
            "en" => "en",
            "es" => "es",
            "de" => "de",
            "fr" => "fr",
            _ => "en",
        }
    }

    fn path_to_string(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[cfg(test)]
    pub(crate) fn sense_voice_config_for_test(
        paths: &AsrModelPaths,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        sense_voice_config(paths)
    }

    #[cfg(test)]
    pub(crate) fn canary_config_for_test(
        paths: &AsrModelPaths,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        canary_config(paths)
    }

    #[cfg(test)]
    pub(crate) fn canary_config_for_language_for_test(
        paths: &AsrModelPaths,
        language: &str,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        canary_config_for_language(paths, language)
    }

    #[cfg(test)]
    pub(crate) fn moonshine_config_for_test(
        paths: &AsrModelPaths,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        moonshine_config(paths)
    }

    #[cfg(test)]
    pub(crate) fn parakeet_config_for_test(
        paths: &AsrModelPaths,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        parakeet_config(paths)
    }
}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
pub struct OfflineRecognizer {}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
impl OfflineRecognizer {
    pub fn new(_paths: &AsrModelPaths, _language: &str) -> anyhow::Result<Self> {
        anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn new_sense_voice(_paths: &AsrModelPaths) -> anyhow::Result<Self> {
        anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn new_canary(_paths: &AsrModelPaths) -> anyhow::Result<Self> {
        anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn new_canary_for_language(
        _paths: &AsrModelPaths,
        _language: &str,
    ) -> anyhow::Result<Self> {
        anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn new_moonshine(_paths: &AsrModelPaths) -> anyhow::Result<Self> {
        anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn new_parakeet(_paths: &AsrModelPaths) -> anyhow::Result<Self> {
        anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn transcribe(&mut self, _sample_rate: i32, _samples: &[f32]) -> anyhow::Result<String> {
        anyhow::bail!("offline sherpa-onnx recognizer is only available in Android ASR builds")
    }
}

#[cfg(all(feature = "android-asr", any(target_os = "android", target_os = "ios")))]
pub use platform::OfflineRecognizer;

#[cfg(all(
    test,
    feature = "android-asr",
    any(target_os = "android", target_os = "ios")
))]
mod tests {
    use super::*;

    #[test]
    fn sense_voice_config_uses_auto_language_itn_tokens_and_cpu() {
        let paths = AsrModelPaths::for_dir(std::path::Path::new("/models/sensevoice"));
        let config = platform::sense_voice_config_for_test(&paths);

        assert_eq!(
            config.model_config.sense_voice.model.as_deref(),
            Some("/models/sensevoice/sense_voice/model.onnx")
        );
        assert_eq!(
            config.model_config.sense_voice.language.as_deref(),
            Some("auto")
        );
        assert!(config.model_config.sense_voice.use_itn);
        assert_eq!(
            config.model_config.tokens.as_deref(),
            Some("/models/sensevoice/sense_voice/tokens.txt")
        );
        assert_eq!(config.model_config.provider.as_deref(), Some("cpu"));
    }

    #[test]
    fn canary_config_uses_transcribe_mode_pnc_tokens_and_cpu() {
        let paths = AsrModelPaths::for_dir(std::path::Path::new("/models/canary"));
        let config = platform::canary_config_for_test(&paths);

        assert_eq!(
            config.model_config.canary.encoder.as_deref(),
            Some("/models/canary/canary/encoder.onnx")
        );
        assert_eq!(
            config.model_config.canary.decoder.as_deref(),
            Some("/models/canary/canary/decoder.onnx")
        );
        assert_eq!(config.model_config.canary.src_lang.as_deref(), Some("en"));
        assert_eq!(config.model_config.canary.tgt_lang.as_deref(), Some("en"));
        assert!(config.model_config.canary.use_pnc);
        assert_eq!(
            config.model_config.tokens.as_deref(),
            Some("/models/canary/canary/tokens.txt")
        );
        assert_eq!(config.model_config.provider.as_deref(), Some("cpu"));
    }

    #[test]
    fn canary_config_for_language_keeps_transcription_target_on_source_language() {
        let paths = AsrModelPaths::for_dir(std::path::Path::new("/models/canary"));
        let config = platform::canary_config_for_language_for_test(&paths, "de-DE");

        assert_eq!(config.model_config.canary.src_lang.as_deref(), Some("de"));
        assert_eq!(config.model_config.canary.tgt_lang.as_deref(), Some("de"));
    }

    #[test]
    fn moonshine_config_uses_v1_decoders_tokens_and_cpu() {
        let paths = AsrModelPaths::for_dir(std::path::Path::new("/models/moonshine"));
        let config = platform::moonshine_config_for_test(&paths);

        assert_eq!(
            config.model_config.moonshine.preprocessor.as_deref(),
            Some("/models/moonshine/moonshine/preprocess.onnx")
        );
        assert_eq!(
            config.model_config.moonshine.encoder.as_deref(),
            Some("/models/moonshine/moonshine/encode.int8.onnx")
        );
        assert_eq!(
            config.model_config.moonshine.uncached_decoder.as_deref(),
            Some("/models/moonshine/moonshine/uncached_decode.int8.onnx")
        );
        assert_eq!(
            config.model_config.moonshine.cached_decoder.as_deref(),
            Some("/models/moonshine/moonshine/cached_decode.int8.onnx")
        );
        assert_eq!(
            config.model_config.tokens.as_deref(),
            Some("/models/moonshine/moonshine/tokens.txt")
        );
        assert_eq!(config.model_config.provider.as_deref(), Some("cpu"));
    }

    #[test]
    fn parakeet_config_uses_nemo_transducer_files_tokens_and_cpu() {
        let paths = AsrModelPaths::for_dir(std::path::Path::new("/models/parakeet"));
        let config = platform::parakeet_config_for_test(&paths);

        assert_eq!(
            config.model_config.transducer.encoder.as_deref(),
            Some("/models/parakeet/parakeet/encoder.int8.onnx")
        );
        assert_eq!(
            config.model_config.transducer.decoder.as_deref(),
            Some("/models/parakeet/parakeet/decoder.int8.onnx")
        );
        assert_eq!(
            config.model_config.transducer.joiner.as_deref(),
            Some("/models/parakeet/parakeet/joiner.int8.onnx")
        );
        assert_eq!(
            config.model_config.tokens.as_deref(),
            Some("/models/parakeet/parakeet/tokens.txt")
        );
        assert_eq!(
            config.model_config.model_type.as_deref(),
            Some("nemo_transducer")
        );
        assert_eq!(config.model_config.provider.as_deref(), Some("cpu"));
    }

    #[test]
    fn offline_whisper_transcribes_fixture() {
        let Some(model_dir) = option_env!("VERBATIM_ANDROID_ASR_TEST_MODEL_DIR") else {
            eprintln!("set VERBATIM_ANDROID_ASR_TEST_MODEL_DIR to run this test");
            return;
        };
        let Some(wav_path) = option_env!("VERBATIM_ANDROID_ASR_TEST_WAV") else {
            eprintln!("set VERBATIM_ANDROID_ASR_TEST_WAV to run this test");
            return;
        };

        let paths = AsrModelPaths::for_dir(std::path::Path::new(model_dir));
        let mut recognizer = OfflineRecognizer::new(&paths, "en").unwrap();
        let wave = sherpa_onnx::Wave::read(wav_path).unwrap();

        assert_eq!(wave.sample_rate(), 16000);
        let text = recognizer
            .transcribe(wave.sample_rate(), wave.samples())
            .unwrap();
        assert!(text.to_lowercase().contains("nightfall"));
    }
}
