// registerListener/removeListener are base-class @Commands (app.tauri.plugin.Plugin) used by
// JS addPluginListener; they need ACL permissions generated even though they have no Rust handler.
const COMMANDS: &[&str] = &[
  "ping",
  "registerListener",
  "removeListener",
  "permission_snapshot",
  "native_transcript_history",
  "delete_history_entry",
  "sync_text_formatter",
  "bubble_corner_snapshot",
  "set_bubble_corner",
  "start_bubble",
  "stop_bubble",
  "request_microphone",
  "open_overlay_settings",
  "open_accessibility_settings",
  "request_speech_model_download",
  "open_external_url",
];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .ios_path("ios")
    .build();
}
