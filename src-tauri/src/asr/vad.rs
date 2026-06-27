//! Voice activity detection wrapper for Android ASR.

use crate::asr::AsrModelPaths;

pub struct VadSpeechSegment {
    pub start: i32,
    pub samples: Vec<f32>,
}

#[cfg(all(feature = "android-asr", any(target_os = "android", target_os = "ios")))]
mod platform {
    use super::*;

    pub struct SileroVadSegmenter {
        detector: sherpa_onnx::VoiceActivityDetector,
        pending_samples: Vec<f32>,
        window_size: usize,
    }

    impl SileroVadSegmenter {
        pub fn new(paths: &AsrModelPaths, sample_rate: i32) -> anyhow::Result<Self> {
            let window_size = silero_window_size(sample_rate)?;
            let config = sherpa_onnx::VadModelConfig {
                silero_vad: sherpa_onnx::SileroVadModelConfig {
                    model: Some(path_to_string(&paths.vad)),
                    threshold: 0.5,
                    min_silence_duration: 0.25,
                    min_speech_duration: 0.25,
                    window_size: window_size as i32,
                    max_speech_duration: 20.0,
                },
                sample_rate,
                num_threads: 1,
                provider: Some("cpu".to_string()),
                debug: false,
                ..Default::default()
            };

            let detector = sherpa_onnx::VoiceActivityDetector::create(&config, 60.0)
                .ok_or_else(|| anyhow::anyhow!("failed to create Silero VAD detector"))?;

            Ok(Self {
                detector,
                pending_samples: Vec::new(),
                window_size,
            })
        }

        pub fn accept_waveform(&mut self, samples: &[f32]) -> Vec<VadSpeechSegment> {
            self.pending_samples.extend_from_slice(samples);
            while self.pending_samples.len() >= self.window_size {
                let chunk: Vec<f32> = self.pending_samples.drain(..self.window_size).collect();
                self.detector.accept_waveform(&chunk);
            }
            self.drain_segments()
        }

        pub fn flush(&mut self) -> Vec<VadSpeechSegment> {
            if !self.pending_samples.is_empty() {
                self.detector.accept_waveform(&self.pending_samples);
                self.pending_samples.clear();
            }
            self.detector.flush();
            self.drain_segments()
        }

        fn drain_segments(&mut self) -> Vec<VadSpeechSegment> {
            let mut segments = Vec::new();

            while let Some(segment) = self.detector.front() {
                segments.push(VadSpeechSegment {
                    start: segment.start(),
                    samples: segment.samples().to_vec(),
                });
                self.detector.pop();
            }

            segments
        }
    }

    fn silero_window_size(sample_rate: i32) -> anyhow::Result<usize> {
        match sample_rate {
            8000 => Ok(256),
            16000 => Ok(512),
            _ => anyhow::bail!("Silero VAD only supports 8000 Hz and 16000 Hz input"),
        }
    }

    fn path_to_string(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }
}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
pub struct SileroVadSegmenter {}

#[cfg(not(all(feature = "android-asr", any(target_os = "android", target_os = "ios"))))]
impl SileroVadSegmenter {
    pub fn new(_paths: &AsrModelPaths, _sample_rate: i32) -> anyhow::Result<Self> {
        anyhow::bail!("Silero VAD is only available in Android ASR builds")
    }

    pub fn accept_waveform(&mut self, _samples: &[f32]) -> Vec<VadSpeechSegment> {
        Vec::new()
    }

    pub fn flush(&mut self) -> Vec<VadSpeechSegment> {
        Vec::new()
    }
}

#[cfg(all(feature = "android-asr", any(target_os = "android", target_os = "ios")))]
pub use platform::SileroVadSegmenter;

#[cfg(all(
    test,
    feature = "android-asr",
    any(target_os = "android", target_os = "ios")
))]
mod tests {
    use super::*;

    #[test]
    fn silero_vad_segments_fixture_speech() {
        let Some(model_dir) = option_env!("VERBATIM_ANDROID_ASR_TEST_MODEL_DIR") else {
            eprintln!("set VERBATIM_ANDROID_ASR_TEST_MODEL_DIR to run this test");
            return;
        };
        let wav_path = option_env!("VERBATIM_ANDROID_ASR_TEST_VAD_WAV")
            .or(option_env!("VERBATIM_ANDROID_ASR_TEST_WAV"));
        let Some(wav_path) = wav_path else {
            eprintln!(
                "set VERBATIM_ANDROID_ASR_TEST_VAD_WAV or VERBATIM_ANDROID_ASR_TEST_WAV to run this test"
            );
            return;
        };

        let paths = AsrModelPaths::for_dir(std::path::Path::new(model_dir));
        let mut segmenter = SileroVadSegmenter::new(&paths, 16000).unwrap();
        let wave = sherpa_onnx::Wave::read(wav_path).unwrap();

        assert_eq!(wave.sample_rate(), 16000);

        for chunk in wave.samples().chunks(1600) {
            segmenter.accept_waveform(chunk);
        }

        let segments = segmenter.flush();
        assert!(!segments.is_empty(), "expected at least one speech segment");
        assert!(segments.iter().any(|segment| !segment.samples.is_empty()));
    }
}
