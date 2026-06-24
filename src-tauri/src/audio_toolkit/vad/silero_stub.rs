use super::{VadFrame, VoiceActivityDetector};
use anyhow::Result;

const ENGINE_DISABLED_ERROR: &str = "Silero VAD engine is disabled in this build";

pub struct SileroVad;

impl SileroVad {
    pub fn new(_model_path: &str, _threshold: f32) -> Result<Self> {
        Err(anyhow::anyhow!(ENGINE_DISABLED_ERROR))
    }
}

impl VoiceActivityDetector for SileroVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        Ok(VadFrame::Speech(frame))
    }
}
