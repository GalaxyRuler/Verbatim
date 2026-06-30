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

    fn path_to_string(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[cfg(test)]
    pub(crate) fn sense_voice_config_for_test(
        paths: &AsrModelPaths,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        sense_voice_config(paths)
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
