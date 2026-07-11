use rubato::{FftFixedIn, Resampler, ResamplerConstructionError};
use std::time::Duration;

// Make this a constant you can tweak
const RESAMPLER_CHUNK_SIZE: usize = 1024;

pub struct FrameResampler {
    resampler: Option<FftFixedIn<f32>>,
    chunk_in: usize,
    in_buf: Vec<f32>,
    frame_samples: usize,
    pending: Vec<f32>,
    dropped_chunks: usize,
}

impl FrameResampler {
    pub fn new(
        in_hz: usize,
        out_hz: usize,
        frame_dur: Duration,
    ) -> Result<Self, ResamplerConstructionError> {
        let frame_samples = ((out_hz as f64 * frame_dur.as_secs_f64()).round()) as usize;
        assert!(frame_samples > 0, "frame duration too short");

        // Use fixed chunk size instead of GCD-based
        let chunk_in = RESAMPLER_CHUNK_SIZE;

        let resampler = if in_hz != out_hz {
            Some(FftFixedIn::<f32>::new(in_hz, out_hz, chunk_in, 1, 1)?)
        } else {
            None
        };

        Ok(Self {
            resampler,
            chunk_in,
            in_buf: Vec::with_capacity(chunk_in),
            frame_samples,
            pending: Vec::with_capacity(frame_samples),
            dropped_chunks: 0,
        })
    }

    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        let Some(mut resampler) = self.resampler.take() else {
            self.emit_frames(src, &mut emit);
            return;
        };

        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];

            if self.in_buf.len() == self.chunk_in {
                match resampler.process(&[&self.in_buf[..]], None) {
                    Ok(out) => self.emit_frames(&out[0], &mut emit),
                    Err(err) => {
                        log::warn!(
                            "Audio resampler failed to process input chunk: {err}; dropping chunk"
                        );
                        self.dropped_chunks = self.dropped_chunks.saturating_add(1);
                    }
                }
                self.in_buf.clear();
            }
        }

        self.resampler = Some(resampler);
    }

    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        // Process any remaining input samples
        if let Some(mut resampler) = self.resampler.take() {
            if !self.in_buf.is_empty() {
                // Pad with zeros to reach chunk size
                self.in_buf.resize(self.chunk_in, 0.0);
                match resampler.process(&[&self.in_buf[..]], None) {
                    Ok(out) => self.emit_frames(&out[0], &mut emit),
                    Err(err) => {
                        log::warn!(
                            "Audio resampler failed to process final input chunk: {err}; dropping chunk"
                        );
                        self.dropped_chunks = self.dropped_chunks.saturating_add(1);
                    }
                }
                self.in_buf.clear();
            }
            self.resampler = Some(resampler);
        }

        // Emit any remaining pending frame (padded with zeros)
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }

    pub(crate) fn take_dropped_chunks(&mut self) -> usize {
        std::mem::take(&mut self.dropped_chunks)
    }

    fn emit_frames(&mut self, mut data: &[f32], emit: &mut impl FnMut(&[f32])) {
        while !data.is_empty() {
            let space = self.frame_samples - self.pending.len();
            let take = space.min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];

            if self.pending.len() == self.frame_samples {
                emit(&self.pending);
                self.pending.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_input_sample_rate_returns_error() {
        let result = FrameResampler::new(0, 16_000, Duration::from_millis(30));

        assert!(result.is_err());
    }
}
