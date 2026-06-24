#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictationTransactionTerminal {
    Cancelled,
    NoRecording,
    NoUsableSpeech,
    EmptyOutput,
    TranscriptionFailed,
    InsertionCompleted,
    InsertionSchedulingFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DictationCleanupPlan {
    pub hide_overlay: bool,
    pub restore_idle_tray: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingStopDecision<T> {
    Continue(T),
    Terminal(DictationTransactionTerminal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinalTextDecision {
    Continue(String),
    Terminal(DictationTransactionTerminal),
}

impl DictationTransactionTerminal {
    pub fn cleanup_plan(self) -> DictationCleanupPlan {
        DictationCleanupPlan {
            hide_overlay: true,
            restore_idle_tray: true,
        }
    }

    pub fn should_save_failed_history(self, wav_saved: bool) -> bool {
        matches!(self, Self::TranscriptionFailed) && wav_saved
    }
}

pub fn classify_recording_stop<T, F>(
    stop_result: Option<T>,
    has_usable_speech: F,
) -> RecordingStopDecision<T>
where
    F: FnOnce(&T) -> bool,
{
    let Some(stop_result) = stop_result else {
        return RecordingStopDecision::Terminal(DictationTransactionTerminal::NoRecording);
    };

    if has_usable_speech(&stop_result) {
        RecordingStopDecision::Continue(stop_result)
    } else {
        RecordingStopDecision::Terminal(DictationTransactionTerminal::NoUsableSpeech)
    }
}

pub fn classify_final_text(final_text: String) -> FinalTextDecision {
    if final_text.is_empty() {
        FinalTextDecision::Terminal(DictationTransactionTerminal::EmptyOutput)
    } else {
        FinalTextDecision::Continue(final_text)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub trait DictationTransactionAdapter {
    type Recording;

    fn stop_recording(&mut self) -> Option<Self::Recording>;
    fn has_usable_speech(&self, recording: &Self::Recording) -> bool;
    fn run_speech_transaction(
        &mut self,
        recording: Self::Recording,
    ) -> DictationTransactionTerminal;
    fn finish_transaction(&mut self, terminal: DictationTransactionTerminal);
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn run_dictation_transaction<A>(adapter: &mut A) -> DictationTransactionTerminal
where
    A: DictationTransactionAdapter,
{
    let terminal = match classify_recording_stop(adapter.stop_recording(), |recording| {
        adapter.has_usable_speech(recording)
    }) {
        RecordingStopDecision::Continue(recording) => adapter.run_speech_transaction(recording),
        RecordingStopDecision::Terminal(terminal) => terminal,
    };

    adapter.finish_transaction(terminal);
    terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states_require_idle_cleanup() {
        for terminal in [
            DictationTransactionTerminal::Cancelled,
            DictationTransactionTerminal::NoRecording,
            DictationTransactionTerminal::NoUsableSpeech,
            DictationTransactionTerminal::EmptyOutput,
            DictationTransactionTerminal::TranscriptionFailed,
            DictationTransactionTerminal::InsertionCompleted,
            DictationTransactionTerminal::InsertionSchedulingFailed,
        ] {
            let plan = terminal.cleanup_plan();
            assert!(plan.hide_overlay, "{terminal:?} should hide overlay");
            assert!(
                plan.restore_idle_tray,
                "{terminal:?} should restore idle tray"
            );
        }
    }

    #[test]
    fn failed_history_is_saved_only_for_transcription_failure_with_wav() {
        assert!(DictationTransactionTerminal::TranscriptionFailed.should_save_failed_history(true));
        assert!(
            !DictationTransactionTerminal::TranscriptionFailed.should_save_failed_history(false)
        );
        assert!(!DictationTransactionTerminal::NoUsableSpeech.should_save_failed_history(true));
        assert!(!DictationTransactionTerminal::Cancelled.should_save_failed_history(true));
        assert!(!DictationTransactionTerminal::NoRecording.should_save_failed_history(true));
        assert!(!DictationTransactionTerminal::EmptyOutput.should_save_failed_history(true));
        assert!(!DictationTransactionTerminal::InsertionCompleted.should_save_failed_history(true));
        assert!(!DictationTransactionTerminal::InsertionSchedulingFailed
            .should_save_failed_history(true));
    }

    #[test]
    fn recording_stop_decision_continues_only_when_recording_has_usable_speech() {
        assert_eq!(
            classify_recording_stop::<u8, _>(None, |_| true),
            RecordingStopDecision::Terminal(DictationTransactionTerminal::NoRecording)
        );
        assert_eq!(
            classify_recording_stop(Some(7_u8), |_| false),
            RecordingStopDecision::Terminal(DictationTransactionTerminal::NoUsableSpeech)
        );
        assert_eq!(
            classify_recording_stop(Some(7_u8), |_| true),
            RecordingStopDecision::Continue(7)
        );
    }

    #[test]
    fn final_text_decision_inserts_only_non_empty_output() {
        assert_eq!(
            classify_final_text(String::new()),
            FinalTextDecision::Terminal(DictationTransactionTerminal::EmptyOutput)
        );
        assert_eq!(
            classify_final_text("hello".to_string()),
            FinalTextDecision::Continue("hello".to_string())
        );
    }

    struct FakeTransactionAdapter {
        recording: Option<u8>,
        usable_speech: bool,
        speech_terminal: DictationTransactionTerminal,
        speech_runs: usize,
        finished: Vec<DictationTransactionTerminal>,
    }

    impl Default for FakeTransactionAdapter {
        fn default() -> Self {
            Self {
                recording: None,
                usable_speech: false,
                speech_terminal: DictationTransactionTerminal::InsertionCompleted,
                speech_runs: 0,
                finished: Vec::new(),
            }
        }
    }

    impl DictationTransactionAdapter for FakeTransactionAdapter {
        type Recording = u8;

        fn stop_recording(&mut self) -> Option<Self::Recording> {
            self.recording.take()
        }

        fn has_usable_speech(&self, _recording: &Self::Recording) -> bool {
            self.usable_speech
        }

        fn run_speech_transaction(
            &mut self,
            _recording: Self::Recording,
        ) -> DictationTransactionTerminal {
            self.speech_runs += 1;
            self.speech_terminal
        }

        fn finish_transaction(&mut self, terminal: DictationTransactionTerminal) {
            self.finished.push(terminal);
        }
    }

    #[test]
    fn transaction_runner_skips_speech_work_without_recording() {
        let mut adapter = FakeTransactionAdapter {
            recording: None,
            usable_speech: true,
            speech_terminal: DictationTransactionTerminal::InsertionCompleted,
            ..Default::default()
        };

        let terminal = run_dictation_transaction(&mut adapter);

        assert_eq!(terminal, DictationTransactionTerminal::NoRecording);
        assert_eq!(adapter.speech_runs, 0);
        assert_eq!(
            adapter.finished,
            vec![DictationTransactionTerminal::NoRecording]
        );
    }

    #[test]
    fn transaction_runner_skips_speech_work_for_unusable_recording() {
        let mut adapter = FakeTransactionAdapter {
            recording: Some(7),
            usable_speech: false,
            speech_terminal: DictationTransactionTerminal::InsertionCompleted,
            ..Default::default()
        };

        let terminal = run_dictation_transaction(&mut adapter);

        assert_eq!(terminal, DictationTransactionTerminal::NoUsableSpeech);
        assert_eq!(adapter.speech_runs, 0);
        assert_eq!(
            adapter.finished,
            vec![DictationTransactionTerminal::NoUsableSpeech]
        );
    }

    #[test]
    fn transaction_runner_finishes_speech_terminal_once() {
        let mut adapter = FakeTransactionAdapter {
            recording: Some(7),
            usable_speech: true,
            speech_terminal: DictationTransactionTerminal::InsertionSchedulingFailed,
            ..Default::default()
        };

        let terminal = run_dictation_transaction(&mut adapter);

        assert_eq!(
            terminal,
            DictationTransactionTerminal::InsertionSchedulingFailed
        );
        assert_eq!(adapter.speech_runs, 1);
        assert_eq!(
            adapter.finished,
            vec![DictationTransactionTerminal::InsertionSchedulingFailed]
        );
    }
}
