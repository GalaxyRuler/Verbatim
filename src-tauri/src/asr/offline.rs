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

    fn path_to_string(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }
}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
pub struct OfflineRecognizer {}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
impl OfflineRecognizer {
    pub fn new(_paths: &AsrModelPaths, _language: &str) -> anyhow::Result<Self> {
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
