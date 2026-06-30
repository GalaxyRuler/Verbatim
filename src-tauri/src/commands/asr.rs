//! Android on-device ASR command surface.

use crate::asr::models::{
    installed_pack_dir_for_app, AndroidAsrModelManager, AndroidAsrModelPackState,
};
use crate::asr::offline::OfflineRecognizer;
use crate::asr::streaming::StreamingRecognizer;
use crate::asr::vad::SileroVadSegmenter;
use crate::asr::{AsrEngineKind, AsrModelPaths};
use once_cell::sync::Lazy;
use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

const SAMPLE_RATE: i32 = 16_000;
static GLOBAL_ASR_SESSION: Lazy<Mutex<Option<AsrCommandSession>>> = Lazy::new(|| Mutex::new(None));

#[cfg_attr(not(test), allow(dead_code))]
pub fn permission_ids() -> [&'static str; 3] {
    ["allow-asr-start", "allow-asr-feed-pcm", "allow-asr-stop"]
}

#[derive(Default)]
pub struct AsrCommandState;

#[derive(Clone, Debug, PartialEq)]
pub enum AsrCommandEvent {
    Partial { text: String },
    Final { text: String },
}

pub struct AsrCommandSession {
    mode: AsrCommandSessionMode,
    buffered_samples: Vec<f32>,
    vad: Option<SileroVadSegmenter>,
}

enum AsrCommandSessionMode {
    ZipformerWhisper {
        streaming: StreamingRecognizer,
        offline: OfflineRecognizer,
        last_partial: String,
    },
    FinalOnlyOffline {
        offline: OfflineRecognizer,
        finalized_segments: Vec<String>,
    },
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
struct AsrCommandEventShape {
    emits_partials: bool,
    emits_final: bool,
}

#[cfg(test)]
impl AsrCommandSessionMode {
    fn event_shape_for_engine(engine_kind: AsrEngineKind) -> AsrCommandEventShape {
        match engine_kind {
            AsrEngineKind::ZipformerWhisper => AsrCommandEventShape {
                emits_partials: true,
                emits_final: true,
            },
            AsrEngineKind::SenseVoice => AsrCommandEventShape {
                emits_partials: false,
                emits_final: true,
            },
            AsrEngineKind::Canary => AsrCommandEventShape {
                emits_partials: false,
                emits_final: true,
            },
            AsrEngineKind::Moonshine => AsrCommandEventShape {
                emits_partials: false,
                emits_final: true,
            },
        }
    }
}

pub fn global_start(paths: AsrModelPaths, lang: &str) -> anyhow::Result<()> {
    let session = AsrCommandSession::start(paths, lang)?;
    let mut guard = GLOBAL_ASR_SESSION
        .lock()
        .map_err(|_| anyhow::anyhow!("ASR session lock is poisoned"))?;
    *guard = Some(session);
    Ok(())
}

pub fn global_feed_pcm(frames: &[f32]) -> anyhow::Result<Vec<AsrCommandEvent>> {
    let mut guard = GLOBAL_ASR_SESSION
        .lock()
        .map_err(|_| anyhow::anyhow!("ASR session lock is poisoned"))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("ASR session has not been started"))?;

    session.feed_pcm(frames)
}

pub fn global_stop() -> anyhow::Result<Vec<AsrCommandEvent>> {
    let mut guard = GLOBAL_ASR_SESSION
        .lock()
        .map_err(|_| anyhow::anyhow!("ASR session lock is poisoned"))?;
    let mut session = guard
        .take()
        .ok_or_else(|| anyhow::anyhow!("ASR session has not been started"))?;

    session.stop()
}

impl AsrCommandSession {
    pub fn start(paths: AsrModelPaths, lang: &str) -> anyhow::Result<Self> {
        match paths.engine_kind() {
            AsrEngineKind::ZipformerWhisper => Ok(Self {
                mode: AsrCommandSessionMode::ZipformerWhisper {
                    streaming: StreamingRecognizer::new(&paths)?,
                    offline: OfflineRecognizer::new(&paths, lang)?,
                    last_partial: String::new(),
                },
                buffered_samples: Vec::new(),
                vad: None,
            }),
            AsrEngineKind::SenseVoice => {
                let offline = OfflineRecognizer::new_sense_voice(&paths)?;
                Self::new_final_only_offline(paths, offline)
            }
            AsrEngineKind::Canary => {
                let offline = OfflineRecognizer::new_canary_for_language(&paths, lang)?;
                Self::new_final_only_offline(paths, offline)
            }
            AsrEngineKind::Moonshine => {
                let offline = OfflineRecognizer::new_moonshine(&paths)?;
                Self::new_final_only_offline(paths, offline)
            }
        }
    }

    fn new_final_only_offline(
        paths: AsrModelPaths,
        offline: OfflineRecognizer,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            mode: AsrCommandSessionMode::FinalOnlyOffline {
                offline,
                finalized_segments: Vec::new(),
            },
            buffered_samples: Vec::new(),
            vad: Some(SileroVadSegmenter::new(&paths, SAMPLE_RATE)?),
        })
    }

    pub fn feed_pcm(&mut self, frames: &[f32]) -> anyhow::Result<Vec<AsrCommandEvent>> {
        self.buffered_samples.extend_from_slice(frames);
        match &mut self.mode {
            AsrCommandSessionMode::ZipformerWhisper {
                streaming,
                last_partial,
                ..
            } => {
                if !streaming.accept_waveform(SAMPLE_RATE, frames)? {
                    return Ok(Vec::new());
                }

                let text = streaming.partial_text()?;
                if text.trim().is_empty() || text == *last_partial {
                    return Ok(Vec::new());
                }

                last_partial.clone_from(&text);
                Ok(vec![AsrCommandEvent::Partial { text }])
            }
            AsrCommandSessionMode::FinalOnlyOffline {
                offline,
                finalized_segments,
            } => {
                if let Some(vad) = &mut self.vad {
                    for segment in vad.accept_waveform(frames) {
                        let text = offline.transcribe(SAMPLE_RATE, &segment.samples)?;
                        if !text.trim().is_empty() {
                            finalized_segments.push(text);
                        }
                    }
                }
                Ok(Vec::new())
            }
        }
    }

    pub fn stop(&mut self) -> anyhow::Result<Vec<AsrCommandEvent>> {
        match &mut self.mode {
            AsrCommandSessionMode::ZipformerWhisper {
                streaming, offline, ..
            } => {
                let mut text = offline.transcribe(SAMPLE_RATE, &self.buffered_samples)?;
                if text.trim().is_empty() {
                    text = streaming.finish()?;
                }

                Ok(vec![AsrCommandEvent::Final { text }])
            }
            AsrCommandSessionMode::FinalOnlyOffline {
                offline,
                finalized_segments,
            } => {
                if let Some(vad) = &mut self.vad {
                    for segment in vad.flush() {
                        let text = offline.transcribe(SAMPLE_RATE, &segment.samples)?;
                        if !text.trim().is_empty() {
                            finalized_segments.push(text);
                        }
                    }
                }

                let mut text = finalized_segments.join(" ").trim().to_string();
                if text.is_empty() {
                    text = offline.transcribe(SAMPLE_RATE, &self.buffered_samples)?;
                }

                Ok(vec![AsrCommandEvent::Final { text }])
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AsrTextPayload {
    pub text: String,
}

#[tauri::command]
#[specta::specta]
pub fn asr_start(app: AppHandle, model_id: String, lang: String) -> Result<(), String> {
    let paths = AsrModelPaths::for_dir(&resolve_model_dir(&app, &model_id)?);
    global_start(paths, &lang).map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn asr_feed_pcm(app: AppHandle, frames: Vec<f32>) -> Result<(), String> {
    let events = global_feed_pcm(&frames).map_err(|error| error.to_string())?;
    emit_events(&app, events)
}

#[tauri::command]
#[specta::specta]
pub fn asr_stop(app: AppHandle) -> Result<(), String> {
    let events = global_stop().map_err(|error| error.to_string())?;
    emit_events(&app, events)
}

#[tauri::command]
#[specta::specta]
pub fn asr_list_model_packs(
    app: AppHandle,
    manager: State<'_, AndroidAsrModelManager>,
) -> Result<Vec<AndroidAsrModelPackState>, String> {
    manager
        .list_for_app(&app)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn asr_get_model_pack_state(
    app: AppHandle,
    manager: State<'_, AndroidAsrModelManager>,
    model_id: String,
) -> Result<AndroidAsrModelPackState, String> {
    manager
        .pack_state_for_app(&app, &model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn asr_download_model_pack(
    app: AppHandle,
    manager: State<'_, AndroidAsrModelManager>,
    model_id: String,
) -> Result<(), String> {
    let result = manager
        .download_pack(&app, &model_id)
        .await
        .map_err(|error| error.to_string());

    if let Err(ref error) = result {
        let _ = app.emit(
            "android-asr-model-failed",
            serde_json::json!({ "modelId": &model_id, "error": error }),
        );
    }

    result
}

#[tauri::command]
#[specta::specta]
pub fn asr_cancel_model_download(
    manager: State<'_, AndroidAsrModelManager>,
    model_id: String,
) -> Result<(), String> {
    manager
        .cancel_download(&model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn asr_select_model_pack(
    app: AppHandle,
    manager: State<'_, AndroidAsrModelManager>,
    model_id: String,
) -> Result<AndroidAsrModelPackState, String> {
    manager
        .select_pack(&app, &model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn asr_delete_model_pack(
    app: AppHandle,
    manager: State<'_, AndroidAsrModelManager>,
    model_id: String,
) -> Result<(), String> {
    manager
        .delete_pack(&app, &model_id)
        .map_err(|error| error.to_string())
}

fn emit_events(app: &AppHandle, events: Vec<AsrCommandEvent>) -> Result<(), String> {
    for event in events {
        match event {
            AsrCommandEvent::Partial { text } => app
                .emit("asr-partial", AsrTextPayload { text })
                .map_err(|error| error.to_string())?,
            AsrCommandEvent::Final { text } => app
                .emit("asr-final", AsrTextPayload { text })
                .map_err(|error| error.to_string())?,
        }
    }

    Ok(())
}

fn resolve_model_dir(app: &AppHandle, model_id: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(model_id);
    if path.is_absolute() || path.components().count() > 1 {
        return Ok(path);
    }

    installed_pack_dir_for_app(app, model_id)
        .map_err(|error| format!("Failed to resolve Android ASR model directory: {error}"))
}

#[cfg(test)]
mod tests {
    use crate::asr::AsrEngineKind;

    #[test]
    fn asr_permission_ids_are_kebab_case() {
        assert_eq!(
            super::permission_ids(),
            ["allow-asr-start", "allow-asr-feed-pcm", "allow-asr-stop"]
        );
    }

    #[test]
    fn asr_command_event_shape_matches_engine_kind() {
        assert_eq!(
            super::AsrCommandSessionMode::event_shape_for_engine(AsrEngineKind::ZipformerWhisper),
            super::AsrCommandEventShape {
                emits_partials: true,
                emits_final: true,
            }
        );
        assert_eq!(
            super::AsrCommandSessionMode::event_shape_for_engine(AsrEngineKind::SenseVoice),
            super::AsrCommandEventShape {
                emits_partials: false,
                emits_final: true,
            }
        );
        assert_eq!(
            super::AsrCommandSessionMode::event_shape_for_engine(AsrEngineKind::Canary),
            super::AsrCommandEventShape {
                emits_partials: false,
                emits_final: true,
            }
        );
        assert_eq!(
            super::AsrCommandSessionMode::event_shape_for_engine(AsrEngineKind::Moonshine),
            super::AsrCommandEventShape {
                emits_partials: false,
                emits_final: true,
            }
        );
    }
}

#[cfg(all(
    test,
    feature = "android-asr",
    any(target_os = "android", target_os = "ios")
))]
mod android_tests {
    use super::*;

    #[test]
    fn asr_command_session_emits_final_for_fixture_frames() {
        let Some(model_dir) = option_env!("VERBATIM_ANDROID_ASR_TEST_MODEL_DIR") else {
            eprintln!("set VERBATIM_ANDROID_ASR_TEST_MODEL_DIR to run this test");
            return;
        };
        let Some(wav_path) = option_env!("VERBATIM_ANDROID_ASR_TEST_WAV") else {
            eprintln!("set VERBATIM_ANDROID_ASR_TEST_WAV to run this test");
            return;
        };
        let expected = option_env!("VERBATIM_ANDROID_ASR_TEST_EXPECTED").unwrap_or("nightfall");

        let paths = AsrModelPaths::for_dir(std::path::Path::new(model_dir));
        let mut session = AsrCommandSession::start(paths, "en").unwrap();
        let wave = sherpa_onnx::Wave::read(wav_path).unwrap();

        for chunk in wave.samples().chunks(1600) {
            session.feed_pcm(chunk).unwrap();
        }

        let events = session.stop().unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            AsrCommandEvent::Final { text } if text.to_lowercase().contains(expected)
        )));
    }
}
