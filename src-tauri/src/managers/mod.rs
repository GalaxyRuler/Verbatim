pub mod audio;
pub mod history;
mod mic_diagnostics;
pub mod model;
mod model_catalog;
#[cfg(feature = "transcribe-rs-engine")]
pub mod transcription;
#[cfg(not(feature = "transcribe-rs-engine"))]
#[path = "transcription_mock.rs"]
pub mod transcription;
