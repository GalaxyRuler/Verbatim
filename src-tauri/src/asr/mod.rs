#[cfg(target_os = "android")]
pub mod jni_bridge;
pub mod llm_models;
pub mod models;
pub mod offline;
pub mod streaming;
pub mod vad;
pub mod wer;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrEngineKind {
    ZipformerWhisper,
    SenseVoice,
    Canary,
}

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
    pub canary_encoder: PathBuf,
    pub canary_decoder: PathBuf,
    pub canary_tokens: PathBuf,
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
            canary_encoder: join("canary/encoder.onnx"),
            canary_decoder: join("canary/decoder.onnx"),
            canary_tokens: join("canary/tokens.txt"),
            vad: join("silero_vad_v4.onnx"),
        }
    }

    pub fn engine_kind(&self) -> AsrEngineKind {
        let has_sense_voice_tier =
            self.sense_voice_model.is_file() && self.sense_voice_tokens.is_file();
        let has_canary_tier = self.canary_encoder.is_file()
            && self.canary_decoder.is_file()
            && self.canary_tokens.is_file();

        if has_sense_voice_tier {
            AsrEngineKind::SenseVoice
        } else if has_canary_tier {
            AsrEngineKind::Canary
        } else {
            AsrEngineKind::ZipformerWhisper
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
        assert!(p.canary_encoder.ends_with("canary/encoder.onnx"));
        assert!(p.canary_decoder.ends_with("canary/decoder.onnx"));
        assert!(p.canary_tokens.ends_with("canary/tokens.txt"));
    }

    #[test]
    fn session_kind_uses_streaming_when_streaming_layout_exists() {
        let temp = tempfile::tempdir().unwrap();
        write_file(temp.path().join("streaming/encoder.onnx"));
        write_file(temp.path().join("streaming/decoder.onnx"));
        write_file(temp.path().join("streaming/joiner.onnx"));
        write_file(temp.path().join("streaming/tokens.txt"));
        write_file(temp.path().join("whisper/encoder.onnx"));
        write_file(temp.path().join("whisper/decoder.onnx"));
        write_file(temp.path().join("whisper/tokens.txt"));
        write_file(temp.path().join("silero_vad_v4.onnx"));

        let paths = AsrModelPaths::for_dir(temp.path());
        assert_eq!(paths.engine_kind(), AsrEngineKind::ZipformerWhisper);
    }

    #[test]
    fn session_kind_uses_sensevoice_when_no_streaming_tier_exists() {
        let temp = tempfile::tempdir().unwrap();
        write_file(temp.path().join("sense_voice/model.onnx"));
        write_file(temp.path().join("sense_voice/tokens.txt"));
        write_file(temp.path().join("silero_vad_v4.onnx"));

        let paths = AsrModelPaths::for_dir(temp.path());
        assert_eq!(paths.engine_kind(), AsrEngineKind::SenseVoice);
    }

    #[test]
    fn session_kind_uses_canary_when_canary_layout_exists() {
        let temp = tempfile::tempdir().unwrap();
        write_file(temp.path().join("canary/encoder.onnx"));
        write_file(temp.path().join("canary/decoder.onnx"));
        write_file(temp.path().join("canary/tokens.txt"));
        write_file(temp.path().join("silero_vad_v4.onnx"));

        let paths = AsrModelPaths::for_dir(temp.path());
        assert_eq!(paths.engine_kind(), AsrEngineKind::Canary);
    }

    fn write_file(path: std::path::PathBuf) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"fixture").unwrap();
    }
}
