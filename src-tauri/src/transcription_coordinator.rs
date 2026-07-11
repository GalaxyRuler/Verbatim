use crate::actions::ACTION_MAP;
use crate::managers::audio::AudioRecordingManager;
use crate::runtime_settings::ShortcutRuntime;
use log::{debug, error, info, warn};
use serde::Serialize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const MAX_COORDINATOR_RESTARTS: usize = 1;
const COORDINATOR_HEALTH_EVENT: &str = "transcription-coordinator-health";

/// Commands processed sequentially by the coordinator thread.
enum Command {
    Input {
        binding_id: String,
        hotkey_string: String,
        is_pressed: bool,
        runtime: ShortcutRuntime,
    },
    Cancel {
        recording_was_active: bool,
    },
    TapWindowExpired {
        binding_id: String,
        released_at: Instant,
    },
    ProcessingFinished {
        generation: u64,
    },
    ProcessingWatchdog {
        generation: u64,
    },
    InjectWorkerPanicForSmoke,
}

enum SupervisorMessage {
    Command(Command),
    WorkerPanicked { generation: u64 },
}

#[derive(Clone, Debug, Serialize)]
struct CoordinatorHealthEvent {
    status: CoordinatorHealthStatus,
    restart_count: usize,
    reason: String,
}

#[derive(Clone, Debug, Serialize)]
struct RecordingErrorPayload {
    error_type: String,
    detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CoordinatorHealthSnapshot {
    pub status: String,
    pub restart_count: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CoordinatorHealthStatus {
    Restarted,
    Disabled,
}

#[derive(Debug, PartialEq, Eq)]
enum SupervisorDecision {
    IgnoreStaleExit,
    Restart {
        generation: u64,
        restart_count: usize,
    },
    Disable {
        restart_count: usize,
    },
}

#[derive(Debug)]
struct SupervisorRuntimeState {
    active_generation: u64,
    restart_count: usize,
    disabled: bool,
}

impl Default for SupervisorRuntimeState {
    fn default() -> Self {
        Self {
            active_generation: 1,
            restart_count: 0,
            disabled: false,
        }
    }
}

impl SupervisorRuntimeState {
    fn record_worker_panic(&mut self, generation: u64) -> SupervisorDecision {
        if generation != self.active_generation {
            return SupervisorDecision::IgnoreStaleExit;
        }

        self.restart_count += 1;
        if self.restart_count <= MAX_COORDINATOR_RESTARTS {
            self.active_generation += 1;
            SupervisorDecision::Restart {
                generation: self.active_generation,
                restart_count: self.restart_count,
            }
        } else {
            self.disabled = true;
            SupervisorDecision::Disable {
                restart_count: self.restart_count,
            }
        }
    }

    fn can_accept_commands(&self) -> bool {
        !self.disabled
    }
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
    Processing {
        generation: u64,
    },
}

/// A toggle press that arrived while the pipeline was busy processing.
/// Replayed as a fresh start when processing finishes, so rapid stop->start
/// toggles are deferred instead of silently dropped.
#[derive(Clone, Debug)]
struct PendingToggle {
    binding_id: String,
    hotkey_string: String,
    stored_at: Instant,
}

/// A press older than this is user-abandoned; don't surprise-start recording.
const PENDING_TOGGLE_MAX_AGE: Duration = Duration::from_secs(3);
const PROCESSING_WATCHDOG: Duration = Duration::from_secs(300);

#[derive(Debug, PartialEq, Eq)]
enum PendingDecision {
    Stored,
    Cancelled,
}

fn pending_toggle_on_press(
    pending: &mut Option<PendingToggle>,
    binding_id: &str,
    hotkey_string: &str,
    at: Instant,
) -> PendingDecision {
    if pending.take().is_some() {
        PendingDecision::Cancelled
    } else {
        *pending = Some(PendingToggle {
            binding_id: binding_id.to_string(),
            hotkey_string: hotkey_string.to_string(),
            stored_at: at,
        });
        PendingDecision::Stored
    }
}

fn take_replayable_pending(pending: Option<PendingToggle>, now: Instant) -> Option<PendingToggle> {
    pending.filter(|p| now.duration_since(p.stored_at) <= PENDING_TOGGLE_MAX_AGE)
}

fn processing_finish_matches(stage: &Stage, generation: u64) -> bool {
    matches!(stage, Stage::Processing { generation: active } if *active == generation)
}

fn watchdog_should_recover(stage: &Stage, generation: u64) -> bool {
    processing_finish_matches(stage, generation)
}

/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline.
pub struct TranscriptionCoordinator {
    tx: Sender<SupervisorMessage>,
    health_events: Arc<Mutex<Vec<CoordinatorHealthSnapshot>>>,
}

pub fn is_transcribe_binding(id: &str) -> bool {
    id == "transcribe" || id == "transcribe_with_post_process"
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        let supervisor_tx = tx.clone();
        let health_events = Arc::new(Mutex::new(Vec::new()));
        let supervisor_health_events = Arc::clone(&health_events);

        thread::spawn(move || run_supervisor(app, rx, supervisor_tx, supervisor_health_events));

        Self { tx, health_events }
    }

    /// Send a keyboard/signal input event for a transcribe binding.
    /// External press-only triggers should pass `is_pressed: true` with the
    /// user shortcut runtime converted through [`ShortcutRuntime::as_toggle`].
    pub fn send_input(
        &self,
        binding_id: &str,
        hotkey_string: &str,
        is_pressed: bool,
        runtime: ShortcutRuntime,
    ) {
        self.send_command(Command::Input {
            binding_id: binding_id.to_string(),
            hotkey_string: hotkey_string.to_string(),
            is_pressed,
            runtime,
        });
    }

    pub fn notify_cancel(&self, recording_was_active: bool) {
        self.send_command(Command::Cancel {
            recording_was_active,
        });
    }

    pub fn notify_processing_finished(&self, generation: u64) {
        self.send_command(Command::ProcessingFinished { generation });
    }

    pub fn health_snapshot(&self) -> Vec<CoordinatorHealthSnapshot> {
        self.health_events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn inject_worker_panic_for_smoke(&self) {
        self.send_command(Command::InjectWorkerPanicForSmoke);
    }

    fn send_command(&self, command: Command) {
        if self.tx.send(SupervisorMessage::Command(command)).is_err() {
            warn!("Transcription coordinator channel closed");
        }
    }
}

fn run_supervisor(
    app: AppHandle,
    rx: Receiver<SupervisorMessage>,
    tx: Sender<SupervisorMessage>,
    health_events: Arc<Mutex<Vec<CoordinatorHealthSnapshot>>>,
) {
    let mut state = SupervisorRuntimeState::default();
    let mut worker_tx = Some(spawn_worker(
        app.clone(),
        tx.clone(),
        state.active_generation,
    ));

    while let Ok(message) = rx.recv() {
        match message {
            SupervisorMessage::Command(command) => {
                if !state.can_accept_commands() {
                    warn!("Transcription coordinator is disabled; ignoring command");
                    continue;
                }

                let Some(current_worker_tx) = worker_tx.as_ref() else {
                    warn!("Transcription coordinator has no active worker");
                    continue;
                };

                if let Err(send_error) = current_worker_tx.send(command) {
                    let command = send_error.0;
                    match handle_worker_panic(
                        &app,
                        &tx,
                        &health_events,
                        &mut state,
                        &mut worker_tx,
                        "worker channel closed",
                    ) {
                        SupervisorDecision::Restart { .. } => {
                            if let Some(next_worker_tx) = worker_tx.as_ref() {
                                if next_worker_tx.send(command).is_err() {
                                    warn!("Failed to forward command to restarted coordinator");
                                }
                            }
                        }
                        SupervisorDecision::Disable { .. }
                        | SupervisorDecision::IgnoreStaleExit => {}
                    }
                }
            }
            SupervisorMessage::WorkerPanicked { generation } => {
                if generation != state.active_generation {
                    let _ = state.record_worker_panic(generation);
                    continue;
                }
                let _ = handle_worker_panic(
                    &app,
                    &tx,
                    &health_events,
                    &mut state,
                    &mut worker_tx,
                    "worker panic",
                );
            }
        }
    }

    debug!("Transcription coordinator supervisor exited");
}

fn handle_worker_panic(
    app: &AppHandle,
    supervisor_tx: &Sender<SupervisorMessage>,
    health_events: &Arc<Mutex<Vec<CoordinatorHealthSnapshot>>>,
    state: &mut SupervisorRuntimeState,
    worker_tx: &mut Option<Sender<Command>>,
    reason: &str,
) -> SupervisorDecision {
    let decision = state.record_worker_panic(state.active_generation);
    match decision {
        SupervisorDecision::Restart {
            generation,
            restart_count,
        } => {
            warn!(
                "Transcription coordinator worker failed ({reason}); restarting once ({restart_count}/{MAX_COORDINATOR_RESTARTS})"
            );
            *worker_tx = Some(spawn_worker(app.clone(), supervisor_tx.clone(), generation));
            emit_health_event(
                app,
                health_events,
                CoordinatorHealthStatus::Restarted,
                restart_count,
                reason,
            );
        }
        SupervisorDecision::Disable { restart_count } => {
            error!(
                "Transcription coordinator worker failed repeatedly ({reason}); disabling dictation commands"
            );
            *worker_tx = None;
            emit_health_event(
                app,
                health_events,
                CoordinatorHealthStatus::Disabled,
                restart_count,
                reason,
            );
        }
        SupervisorDecision::IgnoreStaleExit => {}
    }

    decision
}

fn spawn_worker(
    app: AppHandle,
    supervisor_tx: Sender<SupervisorMessage>,
    generation: u64,
) -> Sender<Command> {
    let (worker_tx, worker_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_worker(app, supervisor_tx.clone(), worker_rx);
        }));
        if let Err(e) = result {
            error!("Transcription coordinator worker panicked: {e:?}");
            let _ = supervisor_tx.send(SupervisorMessage::WorkerPanicked { generation });
        }
    });
    worker_tx
}

fn run_worker(app: AppHandle, supervisor_tx: Sender<SupervisorMessage>, rx: Receiver<Command>) {
    let mut stage = Stage::Idle;
    let mut last_press: Option<Instant> = None;
    let mut active_press: Option<(String, Instant)> = None;
    let mut pending_toggle: Option<PendingToggle> = None;
    let mut next_generation: u64 = 0;

    while let Ok(cmd) = rx.recv() {
        handle_command(
            &app,
            supervisor_tx.clone(),
            &mut stage,
            &mut next_generation,
            &mut last_press,
            &mut active_press,
            &mut pending_toggle,
            cmd,
        );
    }
    debug!("Transcription coordinator worker exited");
}

fn emit_health_event(
    app: &AppHandle,
    health_events: &Arc<Mutex<Vec<CoordinatorHealthSnapshot>>>,
    status: CoordinatorHealthStatus,
    restart_count: usize,
    reason: &str,
) {
    health_events
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(CoordinatorHealthSnapshot {
            status: match status {
                CoordinatorHealthStatus::Restarted => "restarted".to_string(),
                CoordinatorHealthStatus::Disabled => "disabled".to_string(),
            },
            restart_count,
            reason: reason.to_string(),
        });

    let _ = app.emit(
        COORDINATOR_HEALTH_EVENT,
        CoordinatorHealthEvent {
            status,
            restart_count,
            reason: reason.to_string(),
        },
    );
}

fn handle_command(
    app: &AppHandle,
    supervisor_tx: Sender<SupervisorMessage>,
    stage: &mut Stage,
    next_generation: &mut u64,
    last_press: &mut Option<Instant>,
    active_press: &mut Option<(String, Instant)>,
    pending_toggle: &mut Option<PendingToggle>,
    cmd: Command,
) {
    match cmd {
        Command::Input {
            binding_id,
            hotkey_string,
            is_pressed,
            runtime,
        } => {
            let event_at = Instant::now();
            // Debounce rapid-fire press events (key repeat / double-tap).
            // Releases always pass through for push-to-talk.
            if is_pressed {
                if last_press.map_or(false, |t| event_at.duration_since(t) < runtime.debounce()) {
                    debug!("Debounced press for '{binding_id}'");
                    return;
                }
                *last_press = Some(event_at);
            }

            if runtime.push_to_talk() {
                if is_pressed {
                    if can_latch_pending_recording(stage, &binding_id, event_at, runtime) {
                        set_recording_mode(stage, RecordingMode::Latched);
                        *active_press = None;
                        debug!("Latched hands-free recording for '{binding_id}'");
                    } else if let Some(active_binding_id) =
                        stop_binding_for_latched_press(stage, &binding_id).map(str::to_string)
                    {
                        *active_press = None;
                        stop(
                            app,
                            stage,
                            next_generation,
                            supervisor_tx.clone(),
                            &active_binding_id,
                            &hotkey_string,
                        );
                    } else if let Some(active_binding_id) = stop_binding_for_expired_pending_press(
                        stage,
                        &binding_id,
                        event_at,
                        runtime,
                    )
                    .map(str::to_string)
                    {
                        *active_press = None;
                        stop(
                            app,
                            stage,
                            next_generation,
                            supervisor_tx.clone(),
                            &active_binding_id,
                            &hotkey_string,
                        );
                    } else if matches!(stage, Stage::Idle) {
                        start(
                            app,
                            stage,
                            &binding_id,
                            &hotkey_string,
                            RecordingMode::PushToTalk,
                        );
                        if matches!(stage, Stage::Recording { .. }) {
                            *active_press = Some((binding_id, event_at));
                        }
                    } else {
                        debug!("Ignoring press for '{binding_id}': pipeline busy");
                    }
                } else if let Some(active_binding_id) =
                    stop_binding_for_push_to_talk_release(stage, &binding_id).map(str::to_string)
                {
                    let release_can_latch =
                        active_press
                            .take()
                            .map_or(false, |(pressed_binding_id, pressed_at)| {
                                bindings_match(&pressed_binding_id, &binding_id)
                                    && is_latch_candidate_release(pressed_at, event_at, runtime)
                            });

                    if release_can_latch {
                        set_recording_mode(
                            stage,
                            RecordingMode::PendingLatch {
                                released_at: event_at,
                            },
                        );
                        schedule_tap_window_expiry(
                            supervisor_tx,
                            active_binding_id,
                            event_at,
                            runtime,
                        );
                    } else {
                        stop(
                            app,
                            stage,
                            next_generation,
                            supervisor_tx.clone(),
                            &active_binding_id,
                            &hotkey_string,
                        );
                    }
                }
            } else if is_pressed {
                *active_press = None;
                match &stage {
                    Stage::Idle => {
                        start(
                            app,
                            stage,
                            &binding_id,
                            &hotkey_string,
                            RecordingMode::Latched,
                        );
                    }
                    Stage::Recording { .. } => {
                        if let Some(active_binding_id) =
                            stop_binding_for_input(stage, &binding_id).map(str::to_string)
                        {
                            stop(
                                app,
                                stage,
                                next_generation,
                                supervisor_tx.clone(),
                                &active_binding_id,
                                &hotkey_string,
                            );
                        } else {
                            debug!("Ignoring press for '{binding_id}': pipeline busy");
                        }
                    }
                    Stage::Processing { .. } => {
                        match pending_toggle_on_press(
                            pending_toggle,
                            &binding_id,
                            &hotkey_string,
                            event_at,
                        ) {
                            PendingDecision::Stored => {
                                info!(
                                    "Deferred toggle press for '{binding_id}' until processing finishes"
                                );
                            }
                            PendingDecision::Cancelled => {
                                info!("Cancelled pending toggle press for '{binding_id}'");
                            }
                        }
                    }
                }
            }
        }
        Command::Cancel {
            recording_was_active,
        } => {
            *active_press = None;
            *pending_toggle = None;
            // Don't reset during processing — wait for the pipeline to finish.
            if !matches!(stage, Stage::Processing { .. })
                && (recording_was_active || matches!(stage, Stage::Recording { .. }))
            {
                *stage = Stage::Idle;
            }
        }
        Command::TapWindowExpired {
            binding_id,
            released_at,
        } => {
            if let Some(active_binding_id) =
                stop_binding_for_expired_pending_release(stage, &binding_id, released_at)
                    .map(str::to_string)
            {
                *active_press = None;
                stop(
                    app,
                    stage,
                    next_generation,
                    supervisor_tx.clone(),
                    &active_binding_id,
                    "",
                );
            }
        }
        Command::ProcessingFinished { generation } => {
            if !processing_finish_matches(stage, generation) {
                debug!("Ignoring stale ProcessingFinished (generation {generation})");
                return;
            }
            *active_press = None;
            *stage = Stage::Idle;
            if let Some(pending) = take_replayable_pending(pending_toggle.take(), Instant::now()) {
                info!(
                    "Replaying deferred toggle press for '{}'",
                    pending.binding_id
                );
                start(
                    app,
                    stage,
                    &pending.binding_id,
                    &pending.hotkey_string,
                    RecordingMode::Latched,
                );
            }
        }
        Command::ProcessingWatchdog { generation } => {
            if watchdog_should_recover(stage, generation) {
                error!(
                    "Pipeline stuck in Processing for {}s (generation {generation}); force-recovering",
                    PROCESSING_WATCHDOG.as_secs()
                );
                *active_press = None;
                *pending_toggle = None;
                *stage = Stage::Idle;
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorPayload {
                        error_type: "pipeline_watchdog_recovered".to_string(),
                        detail: Some(format!("generation {generation}")),
                    },
                );
            }
        }
        Command::InjectWorkerPanicForSmoke => {
            panic!("forced coordinator panic for packaged smoke drill");
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

fn stop(
    app: &AppHandle,
    stage: &mut Stage,
    next_generation: &mut u64,
    supervisor_tx: Sender<SupervisorMessage>,
    binding_id: &str,
    hotkey_string: &str,
) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        *stage = Stage::Idle;
        return;
    };
    *next_generation += 1;
    let generation = *next_generation;
    *stage = Stage::Processing { generation };
    action.stop(app, binding_id, hotkey_string, generation);
    schedule_processing_watchdog(supervisor_tx, generation);
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
    runtime: ShortcutRuntime,
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
            .map_or(false, |elapsed| elapsed > runtime.double_tap_window())
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

fn can_latch_pending_recording(
    stage: &Stage,
    incoming_binding_id: &str,
    now: Instant,
    runtime: ShortcutRuntime,
) -> bool {
    let Stage::Recording {
        binding_id: active_binding_id,
        mode: RecordingMode::PendingLatch { released_at },
    } = stage
    else {
        return false;
    };

    runtime.latch_enabled()
        && bindings_match(active_binding_id, incoming_binding_id)
        && now
            .checked_duration_since(*released_at)
            .map_or(false, |elapsed| elapsed <= runtime.double_tap_window())
}

fn set_recording_mode(stage: &mut Stage, next_mode: RecordingMode) {
    if let Stage::Recording { mode, .. } = stage {
        *mode = next_mode;
    }
}

fn is_latch_candidate_release(
    pressed_at: Instant,
    released_at: Instant,
    runtime: ShortcutRuntime,
) -> bool {
    runtime.latch_enabled()
        && released_at
            .checked_duration_since(pressed_at)
            .map_or(false, |duration| {
                duration <= runtime.max_latch_tap_duration()
            })
}

fn schedule_tap_window_expiry(
    tx: Sender<SupervisorMessage>,
    binding_id: String,
    released_at: Instant,
    runtime: ShortcutRuntime,
) {
    thread::spawn(move || {
        thread::sleep(runtime.double_tap_window());
        let _ = tx.send(SupervisorMessage::Command(Command::TapWindowExpired {
            binding_id,
            released_at,
        }));
    });
}

fn schedule_processing_watchdog(tx: Sender<SupervisorMessage>, generation: u64) {
    thread::spawn(move || {
        thread::sleep(PROCESSING_WATCHDOG);
        let _ = tx.send(SupervisorMessage::Command(Command::ProcessingWatchdog {
            generation,
        }));
    });
}

fn bindings_match(active_binding_id: &str, incoming_binding_id: &str) -> bool {
    active_binding_id == incoming_binding_id
        || (is_transcribe_binding(active_binding_id) && is_transcribe_binding(incoming_binding_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        let stage = Stage::Processing { generation: 1 };

        assert_eq!(stop_binding_for_input(&stage, "transcribe"), None);
    }

    #[test]
    fn press_during_processing_stores_pending_toggle() {
        let mut pending: Option<PendingToggle> = None;
        let decision =
            pending_toggle_on_press(&mut pending, "transcribe", "ctrl+space", Instant::now());

        assert_eq!(decision, PendingDecision::Stored);
        assert!(pending.is_some());
        assert_eq!(pending.as_ref().unwrap().binding_id, "transcribe");
    }

    #[test]
    fn second_press_during_processing_cancels_pending_toggle() {
        let mut pending: Option<PendingToggle> = None;
        let now = Instant::now();

        pending_toggle_on_press(&mut pending, "transcribe", "ctrl+space", now);
        let decision = pending_toggle_on_press(&mut pending, "transcribe", "ctrl+space", now);

        assert_eq!(decision, PendingDecision::Cancelled);
        assert!(pending.is_none());
    }

    #[test]
    fn fresh_pending_toggle_is_replayable_on_finish() {
        let now = Instant::now();
        let pending = Some(PendingToggle {
            binding_id: "transcribe".into(),
            hotkey_string: "ctrl+space".into(),
            stored_at: now,
        });

        assert!(take_replayable_pending(pending, now).is_some());
    }

    #[test]
    fn stale_pending_toggle_is_dropped_on_finish() {
        let stored_at = Instant::now() - PENDING_TOGGLE_MAX_AGE - Duration::from_millis(1);
        let pending = Some(PendingToggle {
            binding_id: "transcribe".into(),
            hotkey_string: "ctrl+space".into(),
            stored_at,
        });

        assert!(take_replayable_pending(pending, Instant::now()).is_none());
    }

    #[test]
    fn stale_processing_finished_is_ignored() {
        let stage = Stage::Processing { generation: 2 };

        assert!(!processing_finish_matches(&stage, 1));
        assert!(processing_finish_matches(&stage, 2));
    }

    #[test]
    fn watchdog_only_fires_for_matching_generation() {
        let stage = Stage::Processing { generation: 5 };

        assert!(watchdog_should_recover(&stage, 5));
        assert!(!watchdog_should_recover(&stage, 4));
        assert!(!watchdog_should_recover(&Stage::Idle, 5));
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
        let released_at =
            Instant::now() - ShortcutRuntime::push_to_talk_mode().double_tap_window() / 4;
        let stage = Stage::Recording {
            binding_id: "transcribe".to_string(),
            mode: RecordingMode::PendingLatch { released_at },
        };

        assert!(can_latch_pending_recording(
            &stage,
            "transcribe_with_post_process",
            Instant::now(),
            ShortcutRuntime::push_to_talk_mode()
        ));
    }

    #[test]
    fn pending_latch_expires_after_window() {
        let runtime = ShortcutRuntime::push_to_talk_mode();
        let released_at =
            Instant::now() - (runtime.double_tap_window() + std::time::Duration::from_millis(1));
        let stage = Stage::Recording {
            binding_id: "transcribe".to_string(),
            mode: RecordingMode::PendingLatch { released_at },
        };

        assert!(!can_latch_pending_recording(
            &stage,
            "transcribe",
            Instant::now(),
            runtime
        ));
    }

    #[test]
    fn only_short_push_to_talk_releases_are_latch_candidates() {
        let pressed_at = Instant::now();

        assert!(is_latch_candidate_release(
            pressed_at,
            pressed_at + ShortcutRuntime::push_to_talk_mode().max_latch_tap_duration(),
            ShortcutRuntime::push_to_talk_mode()
        ));
        assert!(!is_latch_candidate_release(
            pressed_at,
            pressed_at
                + ShortcutRuntime::push_to_talk_mode().max_latch_tap_duration()
                + std::time::Duration::from_millis(1),
            ShortcutRuntime::push_to_talk_mode()
        ));
    }

    #[test]
    fn injected_worker_panic_restarts_once() {
        let mut state = SupervisorRuntimeState::default();

        let decision = state.record_worker_panic(state.active_generation);

        assert_eq!(
            decision,
            SupervisorDecision::Restart {
                generation: 2,
                restart_count: 1
            }
        );
        assert!(state.can_accept_commands());
        assert_eq!(state.active_generation, 2);
    }

    #[test]
    fn repeated_worker_panic_disables_coordinator() {
        let mut state = SupervisorRuntimeState::default();

        assert!(matches!(
            state.record_worker_panic(state.active_generation),
            SupervisorDecision::Restart { .. }
        ));
        let decision = state.record_worker_panic(state.active_generation);

        assert_eq!(decision, SupervisorDecision::Disable { restart_count: 2 });
        assert!(!state.can_accept_commands());
    }

    #[test]
    fn stale_worker_panic_after_restart_is_ignored() {
        let mut state = SupervisorRuntimeState::default();

        assert!(matches!(
            state.record_worker_panic(state.active_generation),
            SupervisorDecision::Restart { .. }
        ));
        let decision = state.record_worker_panic(1);

        assert_eq!(decision, SupervisorDecision::IgnoreStaleExit);
        assert!(state.can_accept_commands());
        assert_eq!(state.restart_count, 1);
        assert_eq!(state.active_generation, 2);
    }

    #[test]
    fn restarted_supervisor_accepts_next_operation() {
        let mut state = SupervisorRuntimeState::default();

        assert!(matches!(
            state.record_worker_panic(state.active_generation),
            SupervisorDecision::Restart { .. }
        ));

        assert!(state.can_accept_commands());
    }
}
