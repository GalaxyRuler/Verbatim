//! Streaming ASR wrapper for live partial transcripts.

use crate::asr::AsrModelPaths;

#[cfg(all(feature = "android-asr", any(target_os = "android", target_os = "ios")))]
mod platform {
    use super::*;

    pub struct StreamingRecognizer {
        recognizer: sherpa_onnx::OnlineRecognizer,
        stream: sherpa_onnx::OnlineStream,
    }

    impl StreamingRecognizer {
        pub fn new(paths: &AsrModelPaths) -> anyhow::Result<Self> {
            let mut config = sherpa_onnx::OnlineRecognizerConfig::default();
            config.model_config.transducer = sherpa_onnx::OnlineTransducerModelConfig {
                encoder: Some(path_to_string(&paths.streaming_encoder)),
                decoder: Some(path_to_string(&paths.streaming_decoder)),
                joiner: Some(path_to_string(&paths.streaming_joiner)),
            };
            config.model_config.tokens = Some(path_to_string(&paths.streaming_tokens));
            config.model_config.provider = Some("cpu".to_string());
            config.model_config.num_threads = 2;
            config.decoding_method = Some("greedy_search".to_string());
            config.max_active_paths = 4;
            config.enable_endpoint = true;
            config.rule1_min_trailing_silence = 2.4;
            config.rule2_min_trailing_silence = 1.2;
            config.rule3_min_utterance_length = 20.0;

            let recognizer = sherpa_onnx::OnlineRecognizer::create(&config).ok_or_else(|| {
                anyhow::anyhow!("failed to create streaming sherpa-onnx recognizer")
            })?;
            let stream = recognizer.create_stream();

            Ok(Self { recognizer, stream })
        }

        pub fn accept_waveform(
            &mut self,
            sample_rate: i32,
            samples: &[f32],
        ) -> anyhow::Result<bool> {
            self.stream.accept_waveform(sample_rate, samples);
            self.decode_ready();
            Ok(!self.partial_text()?.trim().is_empty())
        }

        pub fn partial_text(&self) -> anyhow::Result<String> {
            Ok(self
                .recognizer
                .get_result(&self.stream)
                .map(|result| result.text)
                .unwrap_or_default())
        }

        pub fn finish(&mut self) -> anyhow::Result<String> {
            self.stream.input_finished();
            self.decode_ready();

            self.recognizer
                .get_result(&self.stream)
                .map(|result| result.text)
                .ok_or_else(|| {
                    anyhow::anyhow!("streaming sherpa-onnx recognizer returned no result")
                })
        }

        fn decode_ready(&mut self) {
            while self.recognizer.is_ready(&self.stream) {
                self.recognizer.decode(&self.stream);
            }
        }
    }

    fn path_to_string(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }
}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
pub struct StreamingRecognizer {}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
impl StreamingRecognizer {
    pub fn new(_paths: &AsrModelPaths) -> anyhow::Result<Self> {
        anyhow::bail!("streaming sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn accept_waveform(&mut self, _sample_rate: i32, _samples: &[f32]) -> anyhow::Result<bool> {
        anyhow::bail!("streaming sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn partial_text(&self) -> anyhow::Result<String> {
        anyhow::bail!("streaming sherpa-onnx recognizer is only available in Android ASR builds")
    }

    pub fn finish(&mut self) -> anyhow::Result<String> {
        anyhow::bail!("streaming sherpa-onnx recognizer is only available in Android ASR builds")
    }
}

#[cfg(all(feature = "android-asr", any(target_os = "android", target_os = "ios")))]
pub use platform::StreamingRecognizer;

#[cfg(all(
    test,
    feature = "android-asr",
    any(target_os = "android", target_os = "ios")
))]
mod tests {
    use super::*;

    #[test]
    fn streaming_zipformer_emits_partial_and_final_text() {
        let Some(model_dir) = option_env!("VERBATIM_ANDROID_ASR_TEST_MODEL_DIR") else {
            eprintln!("set VERBATIM_ANDROID_ASR_TEST_MODEL_DIR to run this test");
            return;
        };
        let Some(wav_path) = option_env!("VERBATIM_ANDROID_ASR_TEST_WAV") else {
            eprintln!("set VERBATIM_ANDROID_ASR_TEST_WAV to run this test");
            return;
        };
        let expected = option_env!("VERBATIM_ANDROID_ASR_TEST_EXPECTED").unwrap_or("hello");

        let paths = AsrModelPaths::for_dir(std::path::Path::new(model_dir));
        let mut recognizer = StreamingRecognizer::new(&paths).unwrap();
        let wave = sherpa_onnx::Wave::read(wav_path).unwrap();

        assert_eq!(wave.sample_rate(), 16000);

        let chunk_size = (wave.sample_rate() as usize) / 10;
        let mut saw_partial = false;
        for chunk in wave.samples().chunks(chunk_size) {
            if recognizer
                .accept_waveform(wave.sample_rate(), chunk)
                .unwrap()
            {
                let partial = recognizer.partial_text().unwrap();
                saw_partial |= !partial.trim().is_empty();
            }
        }

        let final_text = recognizer.finish().unwrap();
        assert!(
            saw_partial,
            "expected at least one non-empty partial result"
        );
        assert!(
            final_text.to_lowercase().contains(expected),
            "expected final text to contain {expected:?}, got {final_text:?}"
        );
    }
}
