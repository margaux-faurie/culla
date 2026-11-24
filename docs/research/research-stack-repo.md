> Historical input, written before the decisions were settled. Where it disagrees with `docs/SPEC.md` or an ADR, those win.

# Photo Swipe Sorter: stack and repo research (Nov 2026)

## 1. Desktop shell: keep Tauri 2
- Tauri 2 has been stable since Oct 2024. Typical installers are about 3–15 MB, against 50–150 MB+ for Electron 42. Wails is at v2.12 with v3 still in alpha, and Deno desktop is experimental with no Windows auto-update ([digitalapplied 2026](https://www.digitalapplied.com/blog/desktop-apps-web-stack-tauri-electron-deno-wails-2026)). Electrobun is young and Bun-first; its Windows target is x64 only (ARM via emulation) ([compat](https://blackboard.sh/electrobun/docs/guides/compatability)).
- On Windows 10 and 11, WebView2 is evergreen Chromium, so CSS transform animations perform the same as in Chrome. The usual "renders differently on each OS" drawback of system webviews barely matters for a Windows-first app. Use the NSIS default `downloadBootstrapper` (+0 MB) ([Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)).
- **Recommendation:** Tauri 2. The real performance risk is decoding 24–45 MP JPEGs in the webview, and the Rust downscaling pipeline in the spec already handles it. Flutter would break UI sharing with the web demo.

## 2. Animation: Motion + React
- Motion (`motion/react`) gives you `drag="x"`, `useMotionValue`/`useTransform` (rotation and opacity tied to x) and `onDragEnd` with `info.offset`/`info.velocity` for threshold + flick. The official [card stack tutorial](https://motion.dev/tutorials/react-card-stack) is partly behind Motion+, but the [drag docs](https://motion.dev/docs/react-drag) are enough. Its main tip: don't reorder the DOM; keep an index and animate only `transform`/`opacity`/`zIndex`.
- `@use-gesture/react` is still at 10.3.1 and react-spring is slow-moving, so there's no reason to pick them over Motion.
- Svelte/Solid would not make a noticeable difference. Frame cost is compositor work and image decoding, not framework diffing. React is also easier to recognise in a portfolio.
- **Recommendation:** Motion. Keep at most 3 card nodes mounted, call `await img.decode()` before promoting the next card, serve screen-sized previews only (never the originals), and drive keyboard decisions through the same `animate()` exit as drag.

## 3. Web demo
- **Adapter:** define a `Backend` interface in TS (`scan`, `preview(id)`, `exif`, `saveSession`, `plan`, `apply`) with `TauriBackend` (invoke + custom protocol URLs) and `DemoBackend` (in-memory, simulated apply). Pick one at build time with `import.meta.env` so the web bundle never includes `@tauri-apps/*`.
- **File System Access:** `showDirectoryPicker` works only in Chrome/Edge. Firefox and Safari have negative standards positions, and it is "very unlikely to ever become Baseline" ([web-features](https://web-platform-dx.github.io/web-features-explorer/features/file-system-access)). Fallback: `<input type="file" webkitdirectory>` works everywhere for **read-only** sorting. In the demo, apply stays simulated (or exports a list of decisions). Lone NEFs are skipped in the web demo; an optional later step is compiling the `crates/core` preview extractor to WASM.
- **Hosting:** use Cloudflare Workers static assets, consistent with the author's other sites. Cloudflare is steering new projects to Workers ([migration guide](https://developers.cloudflare.com/workers/static-assets/migrate-from-pages/)). GitHub Pages also works. Neither needs Vercel.
- **Sample photos:** 20–30 of the author's own photos (no licensing issue), 1600 px long edge, AVIF/WebP at about 150–250 KB each (about 5 MB total), EXIF pre-extracted to a JSON manifest, lazy-loaded with the same prefetch window.

## 4. Distribution
- **CI:** [`tauri-apps/tauri-action@v1`](https://v2.tauri.app/distribute/pipelines/github/) on `windows-latest`, `contents: write`, `releaseDraft: true`, triggered by the release-please tag.
- **Installer:** NSIS only. It installs per-user into `%LOCALAPPDATA%` without admin rights, which suits the updater. MSI needs WiX on Windows and adds little here.
- **Auto-update:** add [`tauri-plugin-updater`](https://v2.tauri.app/plugin/updater/) in v1. It uses its own mandatory signing key (separate from Authenticode, so it works unsigned) and `createUpdaterArtifacts: true`. tauri-action generates `latest.json` for GitHub Releases. Back up the private key: losing it strands every installed user.
- **Code signing:**
  - Azure Artifact Signing ($9.99/month) is open to **individuals only in the US/Canada**; EU access is for organisations only ([quickstart](https://learn.microsoft.com/azure/artifact-signing/quickstart), [MS options](https://learn.microsoft.com/windows/apps/package-and-deploy/code-signing-options)), so it is out for a French individual.
  - OV certificates cost $150–300/year and require an HSM or token.
  - EV no longer gives instant SmartScreen reputation since 2024 ([ToDesktop](https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore)).
  - [SignPath Foundation](https://signpath.org/terms) is free for OSI-licensed projects, but requires the project to be **already released**, MFA, a "Code signing policy" section and manual approval per release.
  - **Plan:** ship v1 unsigned, then apply to SignPath. Reputation builds per publisher over downloads either way.
- **Store:** a free alternative is the Microsoft Store. Tauri can only list an EXE/MSI there, and it must be signed ([Tauri Store](https://v2.tauri.app/distribute/microsoft-store/)). The alternative is MSIX via Microsoft's [winapp CLI](https://learn.microsoft.com/et-ee/windows/apps/dev-tools/winapp-cli/guides/tauri), and the Store then signs it for free. Worth trying in v2.
- **winget:** make the first manifest PR by hand, then automate with `vedantmgoyal9/winget-releaser` or Komac. As far as I know (not verified this session), unsigned installers are accepted after Defender validation.

## 5. Repo best practice
- **Layout:** pnpm workspaces + root Cargo workspace (`crates/core`, `apps/desktop/src-tauri`). Have `tauri.conf.json` `version` point at `../package.json`, so release-please bumps a single source (plus Cargo.toml via `extra-files`).
- **Lint:** Biome v2 (lint + format, one tool, includes React hooks rules) instead of ESLint + Prettier, unless you need typescript-eslint's type-aware rules. Rust side: `rustfmt`, `clippy -D warnings`, `cargo-deny` (licences/advisories).
- **Tests:** Vitest (state machine, adapters), Playwright against the web demo (same UI, so it covers most of the desktop flows), `cargo test` on core with `tempfile` fixtures for the apply/journal tests. nextest is optional at this size.
- **Releases:** release-please + Conventional Commits (commitlint optional). Changesets is built for multi-package npm publishing, which doesn't apply here.
- **Dependencies:** Dependabot for npm, cargo and github-actions, with grouped updates, is enough. Renovate is optional.
- **Security:** OpenSSF Scorecard action + badge, pinned action SHAs, `permissions:` set per job.
- **Exemplars:**
  - [readest/readest](https://github.com/readest/readest): Tauri 2, pnpm workspace, desktop + web from one codebase.
  - [mountain-loop/yaak](https://github.com/mountain-loop/yaak): MIT, `crates/` kept separate from `crates-tauri/`, the same core-without-Tauri split the spec proposes.
  - [tauri-apps/plugins-workspace](https://github.com/tauri-apps/plugins-workspace): reference pnpm + Cargo workspace and CI.

## Spec issues
1. **Over-engineered:** `apps/web-demo` + `packages/ui` + `apps/desktop` is three JS packages for one UI. Use a single `apps/app` Vite frontend with two build modes (tauri/web) + `src-tauri` + `crates/core`.
2. **Over-engineered:** apply journal "resumed/rolled back". Same-volume moves are atomic renames; an append-only JSONL journal with **resume** is enough for v1. Rollback is v2.
3. **Missing:** the auto-updater, and a decision on JSON vs SQLite (pick one; SQLite via `rusqlite` scales better to thousands of decisions per folder with autosave).
4. **Missing:** on Windows, custom protocols are served as `http(s)://<scheme>.localhost`, so CSP `img-src` must allow it. Scope the asset protocol to the cache dir only.
5. **Imprecise:** the "Chromium only" FSA mode should be paired with a `webkitdirectory` read-only fallback.
6. **Missing:** Prettier is redundant if you adopt Biome. Add `SECURITY.md` and Scorecard together, plus a third-party licences file for the bundled binary.
7. **Fine as is:** signing postponed. But Azure is not available to individuals in France, and SignPath needs a released v1 first, so the README SmartScreen note is the right call.
