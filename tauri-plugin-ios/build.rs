const COMMANDS: &[&str] = &["tauri_start_ios_proxy", "tauri_stop_ios_proxy"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .ios_path("ios")
    .build();
}
