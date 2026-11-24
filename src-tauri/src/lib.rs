//! Culla desktop shell: wiring only (SPEC §5, architecture §5.3).
//!
//! Commands are thin wrappers over `culla-adapters`. Each one is listed in `build.rs`
//! (`AppManifest::commands`) and allowed in `capabilities/main.json`.
//!
//! Open (SPEC §10): `bundle.windows.webviewInstallMode` is left at Tauri's default, the download
//! bootstrapper, until the minimum WebView2 version and the installer mode are decided.
#![forbid(unsafe_code)]

/// Version of the Rust side, shared by every workspace crate.
#[tauri::command]
fn version() -> &'static str {
    culla_adapters::VERSION
}

/// Builds and runs the app. Called from `main.rs`.
///
/// # Panics
///
/// If Tauri cannot start, for example when WebView2 is missing.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![version])
        .run(tauri::generate_context!())
        .expect("Culla could not start");
}
