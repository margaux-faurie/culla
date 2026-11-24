---
status: accepted
date: 2026-11-24
---

# Use Tauri 2 with a Rust backend and a React UI

## Context and problem statement

Culla is a Windows-first desktop app with heavy file and image work (thousands of 24–45 MP JPEGs and NEFs, often on SD cards) and a swipe UI where animation matters. The same UI must also run as a browser demo. Which desktop stack?

## Considered options

* Tauri 2 + Rust + React
* Electron
* Wails (Go)
* Flutter
* Electrobun or Deno desktop
* Native UI (WinUI)

## Decision outcome

Chosen option: "Tauri 2 + Rust + React", because it gives a small installer, a Rust process for the decode and file-engine work, and a web UI that the demo can reuse as is.

* Tauri 2 has been stable since October 2024. Installers are typically 3–15 MB against 50–150 MB+ for Electron ([digitalapplied 2026](https://www.digitalapplied.com/blog/desktop-apps-web-stack-tauri-electron-deno-wails-2026)).
* On Windows 10/11 the webview is WebView2 (evergreen Chromium), so the usual "different webview per OS" drawback barely applies to a Windows-first app. The NSIS `downloadBootstrapper` adds 0 MB ([Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)).
* Wails v3 is still alpha. Deno desktop is experimental with no Windows auto-update. Electrobun is young and Bun-first.
* Flutter and a native UI would break UI sharing with the web demo.
* React over Svelte or Solid: frame cost is compositor work and image decoding, not diffing, and React reads better in a portfolio.

### Consequences

* Good, because heavy work stays in Rust and only screen-sized previews reach the webview.
* Good, because one UI codebase serves the app and the demo ([ADR 0014](0014-backend-interface-with-build-time-implementations.md)).
* Bad, because the IPC boundary needs care: typed bindings ([ADR 0019](0019-generate-typescript-bindings-with-tauri-specta.md)) and a custom image scheme ([ADR 0013](0013-deliver-images-through-a-custom-uri-scheme.md)).
* Bad, because macOS and Linux builds (v2) will meet WebKit-based webviews with different behaviour.

Source: [research-stack-repo.md §1–2](../research/research-stack-repo.md).
