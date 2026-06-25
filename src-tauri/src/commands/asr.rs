//! Android on-device ASR command surface.

use crate::asr::offline::OfflineRecognizer;
use crate::asr::streaming::StreamingRecognizer;
use crate::asr::AsrModelPaths;
use once_cell::sync::Lazy;
use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

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
    streaming: StreamingRecognizer,
    offline: OfflineRecognizer,
    buffered_samples: Vec<f32>,
    last_partial: String,
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
        Ok(Self {
            streaming: StreamingRecognizer::new(&paths)?,
            offline: OfflineRecognizer::new(&paths, lang)?,
            buffered_samples: Vec::new(),
            last_partial: String::new(),
        })
    }

    pub fn feed_pcm(&mut self, frames: &[f32]) -> anyhow::Result<Vec<AsrCommandEvent>> {
        self.buffered_samples.extend_from_slice(frames);
        if !self.streaming.accept_waveform(SAMPLE_RATE, frames)? {
            return Ok(Vec::new());
        }

        let text = self.streaming.partial_text()?;
        if text.trim().is_empty() || text == self.last_partial {
            return Ok(Vec::new());
        }

        self.last_partial.clone_from(&text);
        Ok(vec![AsrCommandEvent::Partial { text }])
    }

    pub fn stop(&mut self) -> anyhow::Result<Vec<AsrCommandEvent>> {
        let mut text = self
            .offline
            .transcribe(SAMPLE_RATE, &self.buffered_samples)?;
        if text.trim().is_empty() {
            text = self.streaming.finish()?;
        }

        Ok(vec![AsrCommandEvent::Final { text }])
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

    crate::portable::app_data_dir(app)
        .map(|dir| dir.join("models").join("android-asr").join(model_id))
        .map_err(|error| format!("Failed to resolve Android ASR model directory: {error}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn asr_permission_ids_are_kebab_case() {
        assert_eq!(
            super::permission_ids(),
            ["allow-asr-start", "allow-asr-feed-pcm", "allow-asr-stop"]
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
