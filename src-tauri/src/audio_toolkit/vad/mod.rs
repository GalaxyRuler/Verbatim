use anyhow::Result;

pub enum VadFrame<'a> {
    /// Speech – may aggregate several frames (prefill + current + hangover)
    Speech(&'a [f32]),
    /// Non-speech (silence, noise). Down-stream code can ignore it.
    Noise,
}

impl<'a> VadFrame<'a> {
    #[inline]
    pub fn is_speech(&self) -> bool {
        matches!(self, VadFrame::Speech(_))
    }
}

pub trait VoiceActivityDetector: Send + Sync {
    /// Primary streaming API: feed one 30-ms frame, get keep/drop decision.
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;

    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        Ok(self.push_frame(frame)?.is_speech())
    }

    fn reset(&mut self) {}
}

#[cfg(feature = "silero-vad-engine")]
mod silero;
#[cfg(not(feature = "silero-vad-engine"))]
mod silero_stub;
mod smoothed;

#[cfg(feature = "silero-vad-engine")]
pub use silero::SileroVad;
#[cfg(not(feature = "silero-vad-engine"))]
pub use silero_stub::SileroVad;
pub use smoothed::SmoothedVad;
