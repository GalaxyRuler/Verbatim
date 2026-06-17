use crate::actions::ACTION_MAP;
use crate::managers::audio::AudioRecordingManager;
use log::{debug, error, warn};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const DEBOUNCE: Duration = Duration::from_millis(30);
const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(500);
const MAX_TAP_DURATION: Duration = Duration::from_millis(280);

/// Commands processed sequentially by the coordinator thread.
enum Command {
    Input {
        binding_id: String,
        hotkey_string: String,
        is_pressed: bool,
        push_to_talk: bool,
    },
    Cancel {
        recording_was_active: bool,
    },
    TapWindowExpired {
        binding_id: String,
        released_at: Instant,
    },
    ProcessingFinished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordingMode {
    PushToTalk,
    PendingLatch { released_at: Instant },
    Latched,
}

/// Pipeline lifecycle, owned exclusively by the coordinator thread.
enum Stage {
    Idle,
    Recording {
        binding_id: String,
        mode: RecordingMode,
    },
    Processing,
}

/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline.
pub struct TranscriptionCoordinator {
    tx: Sender<Command>,
}

pub fn is_transcribe_binding(id: &str) -> bool {
    id == "transcribe" || id == "transcribe_with_post_process"
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        let timer_tx = tx.clone();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut stage = Stage::Idle;
                let mut last_press: Option<Instant> = None;
                let mut active_press: Option<(String, Instant)> = None;

                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        Command::Input {
                            binding_id,
                            hotkey_string,
                            is_pressed,
                            push_to_talk,
                        } => {
                            let event_at = Instant::now();
                            // Debounce rapid-fire press events (key repeat / double-tap).
                            // Releases always pass through for push-to-talk.
                            if is_pressed {
                                if last_press
                                    .map_or(false, |t| event_at.duration_since(t) < DEBOUNCE)
                                {
                                    debug!("Debounced press for '{binding_id}'");
                                    continue;
                                }
                                last_press = Some(event_at);
                            }

                            if push_to_talk {
                                if is_pressed {
                                    if can_latch_pending_recording(&stage, &binding_id, event_at) {
                                        set_recording_mode(&mut stage, RecordingMode::Latched);
                                        active_press = None;
                                        debug!("Latched hands-free recording for '{binding_id}'");
                                    } else if let Some(active_binding_id) =
                                        stop_binding_for_latched_press(&stage, &binding_id)
                                            .map(str::to_string)
                                    {
                                        active_press = None;
                                        stop(&app, &mut stage, &active_binding_id, &hotkey_string);
                                    } else if let Some(active_binding_id) =
                                        stop_binding_for_expired_pending_press(
                                            &stage,
                                            &binding_id,
                                            event_at,
                                        )
                                        .map(str::to_string)
                                    {
                                        active_press = None;
                                        stop(&app, &mut stage, &active_binding_id, &hotkey_string);
                                    } else if matches!(stage, Stage::Idle) {
                                        start(
                                            &app,
                                            &mut stage,
                                            &binding_id,
                                            &hotkey_string,
                                            RecordingMode::PushToTalk,
                                        );
                                        if matches!(stage, Stage::Recording { .. }) {
                                            active_press = Some((binding_id, event_at));
                                        }
                                    } else {
                                        debug!("Ignoring press for '{binding_id}': pipeline busy");
                                    }
                                } else if !is_pressed {
                                    if let Some(active_binding_id) =
                                        stop_binding_for_push_to_talk_release(&stage, &binding_id)
                                            .map(str::to_string)
                                    {
                                        let release_can_latch = active_press.take().map_or(
                                            false,
                                            |(pressed_binding_id, pressed_at)| {
                                                bindings_match(&pressed_binding_id, &binding_id)
                                                    && is_latch_candidate_release(
                                                        pressed_at, event_at,
                                                    )
                                            },
                                        );

                                        if release_can_latch {
                                            set_recording_mode(
                                                &mut stage,
                                                RecordingMode::PendingLatch {
                                                    released_at: event_at,
                                                },
                                            );
                                            schedule_tap_window_expiry(
                                                timer_tx.clone(),
                                                active_binding_id,
                                                event_at,
                                            );
                                        } else {
                                            stop(
                                                &app,
                                                &mut stage,
                                                &active_binding_id,
                                                &hotkey_string,
                                            );
                                        }
                                    }
                                }
                            } else if is_pressed {
                                active_press = None;
                                match &stage {
                                    Stage::Idle => {
                                        start(
                                            &app,
                                            &mut stage,
                                            &binding_id,
                                            &hotkey_string,
                                            RecordingMode::Latched,
                                        );
                                    }
                                    Stage::Recording { .. } => {
                                        if let Some(active_binding_id) =
                                            stop_binding_for_input(&stage, &binding_id)
                                                .map(str::to_string)
                                        {
                                            stop(
                                                &app,
                                                &mut stage,
                                                &active_binding_id,
                                                &hotkey_string,
                                            );
                                        } else {
                                            debug!(
                                                "Ignoring press for '{binding_id}': pipeline busy"
                                            );
                                        }
                                    }
                                    _ => {
                                        debug!("Ignoring press for '{binding_id}': pipeline busy")
                                    }
                                }
                            }
                        }
                        Command::Cancel {
                            recording_was_active,
                        } => {
                            active_press = None;
                            // Don't reset during processing — wait for the pipeline to finish.
                            if !matches!(stage, Stage::Processing)
                                && (recording_was_active
                                    || matches!(stage, Stage::Recording { .. }))
                            {
                                stage = Stage::Idle;
                            }
                        }
                        Command::TapWindowExpired {
                            binding_id,
                            released_at,
                        } => {
                            if let Some(active_binding_id) =
                                stop_binding_for_expired_pending_release(
                                    &stage,
                                    &binding_id,
                                    released_at,
                                )
                                .map(str::to_string)
                            {
                                active_press = None;
                                stop(&app, &mut stage, &active_binding_id, "");
                            }
                        }
                        Command::ProcessingFinished => {
                            active_press = None;
                            stage = Stage::Idle;
                        }
                    }
                }
                debug!("Transcription coordinator exited");
            }));
            if let Err(e) = result {
                error!("Transcription coordinator panicked: {e:?}");
            }
        });

        Self { tx }
    }

    /// Send a keyboard/signal input event for a transcribe binding.
    /// For signal-based toggles, use `is_pressed: true` and `push_to_talk: false`.
    pub fn send_input(
        &self,
        binding_id: &str,
        hotkey_string: &str,
        is_pressed: bool,
        push_to_talk: bool,
    ) {
        if self
            .tx
            .send(Command::Input {
                binding_id: binding_id.to_string(),
                hotkey_string: hotkey_string.to_string(),
                is_pressed,
                push_to_talk,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_cancel(&self, recording_was_active: bool) {
        if self
            .tx
            .send(Command::Cancel {
                recording_was_active,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_processing_finished(&self) {
        if self.tx.send(Command::ProcessingFinished).is_err() {
            warn!("Transcription coordinator channel closed");
        }
    }
}

fn start(
    app: &AppHandle,
    stage: &mut Stage,
    binding_id: &str,
    hotkey_string: &str,
    mode: RecordingMode,
) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.start(app, binding_id, hotkey_string);
    if app
        .try_state::<Arc<AudioRecordingManager>>()
        .map_or(false, |a| a.is_recording())
    {
        *stage = Stage::Recording {
            binding_id: binding_id.to_string(),
            mode,
        };
    } else {
        debug!("Start for '{binding_id}' did not begin recording; staying idle");
    }
}

fn stop(app: &AppHandle, stage: &mut Stage, binding_id: &str, hotkey_string: &str) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.stop(app, binding_id, hotkey_string);
    *stage = Stage::Processing;
}

fn stop_binding_for_input<'a>(stage: &'a Stage, incoming_binding_id: &str) -> Option<&'a str> {
    let Stage::Recording {
        binding_id: active_binding_id,
        ..
    } = stage
    else {
        return None;
    };

    if bindings_match(active_binding_id, incoming_binding_id) {
        Some(active_binding_id)
    } else {
        None
    }
}

fn stop_binding_for_push_to_talk_release<'a>(
    stage: &'a Stage,
    incoming_binding_id: &str,
) -> Option<&'a str> {
    let Stage::Recording {
        binding_id: active_binding_id,
        mode: RecordingMode::PushToTalk,
    } = stage
    else {
        return None;
    };

    if bindings_match(active_binding_id, incoming_binding_id) {
        Some(active_binding_id)
    } else {
        None
    }
}

fn stop_binding_for_latched_press<'a>(
    stage: &'a Stage,
    incoming_binding_id: &str,
) -> Option<&'a str> {
    let Stage::Recording {
        binding_id: active_binding_id,
        mode: RecordingMode::Latched,
    } = stage
    else {
        return None;
    };

    if bindings_match(active_binding_id, incoming_binding_id) {
        Some(active_binding_id)
    } else {
        None
    }
}

fn stop_binding_for_expired_pending_press<'a>(
    stage: &'a Stage,
    incoming_binding_id: &str,
    now: Instant,
) -> Option<&'a str> {
    let Stage::Recording {
        binding_id: active_binding_id,
        mode: RecordingMode::PendingLatch { released_at },
    } = stage
    else {
        return None;
    };

    if bindings_match(active_binding_id, incoming_binding_id)
        && now
            .checked_duration_since(*released_at)
            .map_or(false, |elapsed| elapsed > DOUBLE_TAP_WINDOW)
    {
        Some(active_binding_id)
    } else {
        None
    }
}

fn stop_binding_for_expired_pending_release<'a>(
    stage: &'a Stage,
    incoming_binding_id: &str,
    released_at: Instant,
) -> Option<&'a str> {
    let Stage::Recording {
        binding_id: active_binding_id,
        mode: RecordingMode::PendingLatch {
            released_at: active_released_at,
        },
    } = stage
    else {
        return None;
    };

    if bindings_match(active_binding_id, incoming_binding_id) && *active_released_at == released_at
    {
        Some(active_binding_id)
    } else {
        None
    }
}

fn can_latch_pending_recording(stage: &Stage, incoming_binding_id: &str, now: Instant) -> bool {
    let Stage::Recording {
        binding_id: active_binding_id,
        mode: RecordingMode::PendingLatch { released_at },
    } = stage
    else {
        return false;
    };

    bindings_match(active_binding_id, incoming_binding_id)
        && now
            .checked_duration_since(*released_at)
            .map_or(false, |elapsed| elapsed <= DOUBLE_TAP_WINDOW)
}

fn set_recording_mode(stage: &mut Stage, next_mode: RecordingMode) {
    if let Stage::Recording { mode, .. } = stage {
        *mode = next_mode;
    }
}

fn is_latch_candidate_release(pressed_at: Instant, released_at: Instant) -> bool {
    released_at
        .checked_duration_since(pressed_at)
        .map_or(false, |duration| duration <= MAX_TAP_DURATION)
}

fn schedule_tap_window_expiry(tx: Sender<Command>, binding_id: String, released_at: Instant) {
    thread::spawn(move || {
        thread::sleep(DOUBLE_TAP_WINDOW);
        let _ = tx.send(Command::TapWindowExpired {
            binding_id,
            released_at,
        });
    });
}

fn bindings_match(active_binding_id: &str, incoming_binding_id: &str) -> bool {
    active_binding_id == incoming_binding_id
        || (is_transcribe_binding(active_binding_id) && is_transcribe_binding(incoming_binding_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternate_transcribe_binding_can_stop_active_recording() {
        let stage = Stage::Recording {
            binding_id: "transcribe_with_post_process".to_string(),
            mode: RecordingMode::Latched,
        };

        assert_eq!(
            stop_binding_for_input(&stage, "transcribe"),
            Some("transcribe_with_post_process")
        );
    }

    #[test]
    fn non_transcribe_binding_cannot_stop_active_recording() {
        let stage = Stage::Recording {
            binding_id: "transcribe_with_post_process".to_string(),
            mode: RecordingMode::Latched,
        };

        assert_eq!(stop_binding_for_input(&stage, "cancel"), None);
    }

    #[test]
    fn transcribe_binding_does_not_interrupt_processing() {
        let stage = Stage::Processing;

        assert_eq!(stop_binding_for_input(&stage, "transcribe"), None);
    }

    #[test]
    fn latched_recording_ignores_push_to_talk_release() {
        let stage = Stage::Recording {
            binding_id: "transcribe".to_string(),
            mode: RecordingMode::Latched,
        };

        assert_eq!(
            stop_binding_for_push_to_talk_release(&stage, "transcribe"),
            None
        );
    }

    #[test]
    fn latched_recording_stops_on_next_press() {
        let stage = Stage::Recording {
            binding_id: "transcribe".to_string(),
            mode: RecordingMode::Latched,
        };

        assert_eq!(
            stop_binding_for_latched_press(&stage, "transcribe_with_post_process"),
            Some("transcribe")
        );
    }

    #[test]
    fn pending_latch_accepts_second_press_inside_window() {
        let released_at = Instant::now() - Duration::from_millis(120);
        let stage = Stage::Recording {
            binding_id: "transcribe".to_string(),
            mode: RecordingMode::PendingLatch { released_at },
        };

        assert!(can_latch_pending_recording(
            &stage,
            "transcribe_with_post_process",
            Instant::now()
        ));
    }

    #[test]
    fn pending_latch_expires_after_window() {
        let released_at = Instant::now() - (DOUBLE_TAP_WINDOW + Duration::from_millis(1));
        let stage = Stage::Recording {
            binding_id: "transcribe".to_string(),
            mode: RecordingMode::PendingLatch { released_at },
        };

        assert!(!can_latch_pending_recording(
            &stage,
            "transcribe",
            Instant::now()
        ));
    }

    #[test]
    fn only_short_push_to_talk_releases_are_latch_candidates() {
        let pressed_at = Instant::now();

        assert!(is_latch_candidate_release(
            pressed_at,
            pressed_at + MAX_TAP_DURATION
        ));
        assert!(!is_latch_candidate_release(
            pressed_at,
            pressed_at + MAX_TAP_DURATION + Duration::from_millis(1)
        ));
    }
}
