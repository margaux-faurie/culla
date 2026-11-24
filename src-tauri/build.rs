//! Generates the Tauri context and the app's permissions.
//!
//! Only the commands listed here can be called from the webview, and each one also needs its
//! `allow-<command>` permission in `capabilities/main.json` (threat model T7).

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&["version"]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("tauri-build failed");
}
