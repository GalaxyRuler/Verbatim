// registerListener/removeListener are base-class @Commands (app.tauri.plugin.Plugin) used by
// JS addPluginListener; they need ACL permissions generated even though they have no Rust handler.
const COMMANDS: &[&str] = &["ping", "registerListener", "removeListener"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .ios_path("ios")
    .build();
}
