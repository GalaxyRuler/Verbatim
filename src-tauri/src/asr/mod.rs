#[cfg(target_os = "android")]
pub mod jni_bridge;
pub mod llm_models;
pub mod models;
pub mod offline;
pub mod streaming;
pub mod vad;
pub mod wer;

use std::path::{Path, PathBuf};

pub struct AsrModelPaths {
    pub streaming_encoder: PathBuf,
    pub streaming_decoder: PathBuf,
    pub streaming_joiner: PathBuf,
    pub streaming_tokens: PathBuf,
    pub whisper_encoder: PathBuf,
    pub whisper_decoder: PathBuf,
    pub whisper_tokens: PathBuf,
    pub sense_voice_model: PathBuf,
    pub sense_voice_tokens: PathBuf,
    pub vad: PathBuf,
}

impl AsrModelPaths {
    pub fn for_dir(dir: &Path) -> Self {
        let join = |name: &str| dir.join(name);

        Self {
            streaming_encoder: join("streaming/encoder.onnx"),
            streaming_decoder: join("streaming/decoder.onnx"),
            streaming_joiner: join("streaming/joiner.onnx"),
            streaming_tokens: join("streaming/tokens.txt"),
            whisper_encoder: join("whisper/encoder.onnx"),
            whisper_decoder: join("whisper/decoder.onnx"),
            whisper_tokens: join("whisper/tokens.txt"),
            sense_voice_model: join("sense_voice/model.onnx"),
            sense_voice_tokens: join("sense_voice/tokens.txt"),
            vad: join("silero_vad_v4.onnx"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asr_paths_resolve_from_models_dir() {
        let p = AsrModelPaths::for_dir(std::path::Path::new("/data/models/verbatim-asr"));
        assert!(p.whisper_encoder.ends_with("encoder.onnx"));
        assert!(p.streaming_joiner.ends_with("joiner.onnx"));
        assert!(p.vad.ends_with("silero_vad_v4.onnx"));
        assert!(p.sense_voice_model.ends_with("sense_voice/model.onnx"));
        assert!(p.sense_voice_tokens.ends_with("sense_voice/tokens.txt"));
    }
}
