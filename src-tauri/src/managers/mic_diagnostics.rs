use std::time::Duration;

const DEFAULT_LEVEL_THRESHOLD: f32 = 0.03;
const DEFAULT_SILENCE_AFTER: Duration = Duration::from_secs(2);
const DEFAULT_MIC_FAILED_AFTER: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicDiagnosticState {
    Recording,
    Silence,
    MicFailed,
}

impl MicDiagnosticState {
    pub fn overlay_state(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Silence => "silence",
            Self::MicFailed => "mic_failed",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SilenceDiagnosticConfig {
    pub level_threshold: f32,
    pub silence_after: Duration,
    pub mic_failed_after: Duration,
}

impl Default for SilenceDiagnosticConfig {
    fn default() -> Self {
        Self {
            level_threshold: DEFAULT_LEVEL_THRESHOLD,
            silence_after: DEFAULT_SILENCE_AFTER,
            mic_failed_after: DEFAULT_MIC_FAILED_AFTER,
        }
    }
}

#[derive(Debug)]
pub struct SilenceDiagnostic {
    config: SilenceDiagnosticConfig,
    quiet_since: Option<Duration>,
    state: MicDiagnosticState,
    observed_active_signal: bool,
}

impl SilenceDiagnostic {
    pub fn new(config: SilenceDiagnosticConfig) -> Self {
        Self {
            config,
            quiet_since: None,
            state: MicDiagnosticState::Recording,
            observed_active_signal: false,
        }
    }

    pub fn reset(&mut self) {
        self.quiet_since = None;
        self.state = MicDiagnosticState::Recording;
        self.observed_active_signal = false;
    }

    pub fn has_observed_active_signal(&self) -> bool {
        self.observed_active_signal
    }

    pub fn observe_at(&mut self, levels: &[f32], at: Duration) -> MicDiagnosticState {
        if self.has_active_signal(levels) {
            self.quiet_since = None;
            self.state = MicDiagnosticState::Recording;
            self.observed_active_signal = true;
            return self.state;
        }

        let quiet_since = *self.quiet_since.get_or_insert(at);
        let quiet_for = at.saturating_sub(quiet_since);

        self.state = if quiet_for >= self.config.mic_failed_after {
            MicDiagnosticState::MicFailed
        } else if quiet_for >= self.config.silence_after {
            MicDiagnosticState::Silence
        } else {
            MicDiagnosticState::Recording
        };

        self.state
    }

    fn has_active_signal(&self, levels: &[f32]) -> bool {
        levels
            .iter()
            .any(|level| level.is_finite() && *level >= self.config.level_threshold)
    }
}

impl Default for SilenceDiagnostic {
    fn default() -> Self {
        Self::new(SilenceDiagnosticConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{MicDiagnosticState, SilenceDiagnostic, SilenceDiagnosticConfig};
    use std::time::Duration;

    #[test]
    fn sustained_low_levels_escalate_to_silence_then_mic_failed() {
        let mut diagnostic = SilenceDiagnostic::new(SilenceDiagnosticConfig {
            level_threshold: 0.03,
            silence_after: Duration::from_secs(2),
            mic_failed_after: Duration::from_secs(8),
        });

        assert_eq!(
            diagnostic.observe_at(&[0.2, 0.1, 0.05], Duration::from_millis(0)),
            MicDiagnosticState::Recording
        );
        assert_eq!(
            diagnostic.observe_at(&[0.0, 0.01, 0.0], Duration::from_secs(1)),
            MicDiagnosticState::Recording
        );
        assert_eq!(
            diagnostic.observe_at(&[0.0, 0.01, 0.0], Duration::from_secs(3)),
            MicDiagnosticState::Silence
        );
        assert_eq!(
            diagnostic.observe_at(&[0.0, 0.01, 0.0], Duration::from_secs(9)),
            MicDiagnosticState::MicFailed
        );
        assert_eq!(
            diagnostic.observe_at(&[0.1, 0.08, 0.04], Duration::from_secs(10)),
            MicDiagnosticState::Recording
        );
    }

    #[test]
    fn empty_level_updates_count_as_silence() {
        let mut diagnostic = SilenceDiagnostic::new(SilenceDiagnosticConfig {
            level_threshold: 0.03,
            silence_after: Duration::from_secs(1),
            mic_failed_after: Duration::from_secs(4),
        });

        assert_eq!(
            diagnostic.observe_at(&[], Duration::from_secs(0)),
            MicDiagnosticState::Recording
        );
        assert_eq!(
            diagnostic.observe_at(&[], Duration::from_secs(2)),
            MicDiagnosticState::Silence
        );
    }

    #[test]
    fn active_observation_is_recorded_until_reset() {
        let mut diagnostic = SilenceDiagnostic::default();

        assert!(!diagnostic.has_observed_active_signal());
        diagnostic.observe_at(&[0.0, 0.04], Duration::from_secs(1));
        assert!(diagnostic.has_observed_active_signal());

        diagnostic.observe_at(&[0.0, 0.0], Duration::from_secs(2));
        assert!(diagnostic.has_observed_active_signal());

        diagnostic.reset();
        assert!(!diagnostic.has_observed_active_signal());
    }
}
