---
status: accepted
date: 2026-11-24
---

# A `Backend` interface with build-time Tauri and Demo implementations

Implementation note (M0, PR #12): the build-time selection is a Vite alias, `#backend-impl`, pointing at `src/backend/tauri.ts` in `--mode desktop` and `src/backend/demo.ts` otherwise, with a shared declaration in `src/backend/impl.d.ts`. `tests/demo-bundle.test.ts` enforces that the demo bundle contains no `@tauri-apps` module.

## Context and problem statement

The same UI must run in the desktop app and as a browser demo on the author's site. The demo cannot call Rust, and its bundle should not carry Tauri code. How does one UI serve both?

## Considered options

* Separate apps: `apps/desktop`, `apps/web-demo`, shared `packages/ui`
* One Vite app with a runtime check (`window.__TAURI__`)
* One Vite app, a TS `Backend` interface, implementation chosen at build time

## Decision outcome

Chosen option: "One Vite app, `Backend` interface, implementation chosen at build time", because it keeps one UI codebase and keeps `@tauri-apps/*` out of the demo bundle.

* The interface covers scanning, previews (as URLs), EXIF, recording decision events, planning and applying with progress.
* `TauriBackend` uses the generated bindings and `culla://` URLs. `DemoBackend` uses about 25 bundled photos (1600 px, AVIF/WebP, ~5 MB) with pre-extracted EXIF JSON, and simulates apply; the review shows the plan it would have run.
* Selection through `import.meta.env` at build time. The UI receives the backend through context.
* Optional "your folder" demo mode with `<input webkitdirectory>`: read-only, works in every browser, uploads nothing. `showDirectoryPicker` was rejected: Chrome/Edge only and "very unlikely to ever become Baseline" ([web-features](https://web-platform-dx.github.io/web-features-explorer/features/file-system-access)).
* Three JS packages for one UI was judged over-engineered.

### Consequences

* Good, because Playwright against the demo covers most desktop flows cheaply in CI.
* Good, because UI work can proceed against `DemoBackend` before the Rust side exists.
* Bad, because `DemoBackend` must keep up with the real behaviour; every new command adds a demo implementation.
* Bad, because demo-only gaps (no lone NEF previews in "your folder" mode, to decide) differ from the app.

Source: [research-stack-repo.md §3, spec issues 1 and 5](../research/research-stack-repo.md), [practices.md §3](../research/practices.md), [SPEC.md §5.4–5.5](../SPEC.md).
