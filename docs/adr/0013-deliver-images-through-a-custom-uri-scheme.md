---
status: accepted
date: 2026-11-24
---

# Deliver images through a custom URI scheme, not IPC or the asset protocol

## Context and problem statement

The webview must show screen-sized previews quickly, one or more per swipe, plus full-size images for 100% zoom. The user picks any folder at runtime, on any drive. How do image bytes reach the webview?

## Considered options

* Base64 or byte arrays over Tauri IPC
* Tauri asset protocol pointing at the original files
* Raw RGBA frames
* Async custom URI scheme `culla://` serving pre-encoded JPEGs

## Decision outcome

Chosen option: "Async custom URI scheme `culla://`", because it serves small pre-encoded previews from memory with HTTP caching, and needs no file-system scope in the webview.

* Registered with `register_asynchronous_uri_scheme_protocol`. On Windows it appears as `http://culla.localhost`. Routes like `culla://preview/<id>/<size>`; the full-size route for zoom is to decide in M1.
* Only IDs from the current scan are served. Unknown IDs get 404. Responses carry `Cache-Control: immutable`.
* CSP `img-src` allows only `'self'` and the scheme's origin.
* IPC is string-serialized ([tauri#5690](https://github.com/tauri-apps/tauri/discussions/5690)). Raw RGBA is ~20 MB per frame.
* The asset protocol would make Chromium decode the 20 MB originals itself (180 MB RGBA for 45 MP), and its scope would have to be widened at runtime to every folder the user picks ([asset protocol scope](https://v2.tauri.app/security/asset-protocol/)). The asset protocol is not used for originals.
* The UI calls `img.decode()` before animating a card in.

### Consequences

* Good, because the webview only ever sees opaque IDs, never paths.
* Good, because cached previews cost a memory lookup.
* Bad, because the handler is a new attack surface (ID parsing, path mapping); it is covered in [threat-model.md](../threat-model.md).
* Bad, because the `http://<scheme>.localhost` mapping is Windows-specific and will differ on macOS and Linux.

Source: [research-raw-perf.md §3](../research/research-raw-perf.md), [practices.md §2](../research/practices.md), [research-stack-repo.md, spec issue 4](../research/research-stack-repo.md).
