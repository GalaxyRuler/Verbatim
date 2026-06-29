use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "verbatim", about = "Verbatim - Speech to Text")]
pub struct CliArgs {
    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to running instance)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to running instance)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation (sent to running instance)
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,

    /// Internal: load and run a tiny Whisper GPU inference, then exit.
    #[arg(long, hide = true, value_name = "MODEL_PATH")]
    pub whisper_gpu_preflight: Option<PathBuf>,

    /// Internal: GPU device used with --whisper-gpu-preflight.
    #[arg(long, hide = true, default_value_t = 0)]
    pub whisper_gpu_preflight_device: i32,
}
