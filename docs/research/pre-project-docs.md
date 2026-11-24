# Pre-implementation documents for Culla

Research note, 24 Nov 2026. Question: which documents should exist before coding starts, and which ones are worth it for a solo OSS project of Culla's size.

## Key findings from sources

- **Design docs (Google):** the usual shape is context and scope, goals and non-goals, the design, alternatives considered, and cross-cutting concerns (security, privacy, observability). A 1–3 page "mini design doc" is fine. Skip the doc when there are no real trade-offs. ([industrialempathy.com](https://www.industrialempathy.com/posts/design-docs-at-google/))
- **arc42** has 12 sections, including quality requirements, risks and a glossary. It has a "lean" mode built on the motto "travel light", but quality goals should always be written down. ([docs.arc42.org](https://docs.arc42.org/home/))
- **C4:** "you don't need to use all 4 levels". The context and container diagrams "are sufficient for most software development teams". ([c4model.com](https://c4model.com/diagrams))
- **ADRs:** Thoughtworks rated lightweight ADRs "Adopt" and wants them kept in source control next to the code ([radar](https://www.thoughtworks.com/radar/techniques/lightweight-architecture-decision-records)). MADR 4.0.0 offers full, minimal and bare templates. Only context, options and outcome are required; the rest is optional. ([adr.github.io/madr](https://adr.github.io/madr/), [github.com/adr/madr](https://github.com/adr/madr))
- **Threat modeling:** OWASP builds it on four questions (what are we working on, what can go wrong, what will we do, did we do enough). It uses STRIDE as prompts, starts early and keeps the model updated ([OWASP cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Threat_Modeling_Cheat_Sheet.html)). Fowler recommends "bite-sized chunks" tied to the current work ([martinfowler.com](https://martinfowler.com/articles/agile-threat-modelling.html)).
- **Product framing:** a Shape Up pitch covers problem, appetite, solution, rabbit holes and no-gos ([basecamp.com](https://basecamp.com/shapeup/1.5-chapter-06)). A Rust RFC adds drawbacks, prior art, unresolved questions and future possibilities ([rust-lang/rfcs](https://github.com/rust-lang/rfcs/blob/master/0000-template.md)). Both are leaner than a classic PRD.
- **OSS hygiene:** every project should ship a LICENSE, README, CONTRIBUTING and CODE_OF_CONDUCT ([opensource.guide](https://opensource.guide/starting-a-project/)). OpenSSF Best Practices (passing level) asks for a published vulnerability-reporting process, release notes, unique versions and a test policy ([bestpractices.dev](https://www.bestpractices.dev/en/criteria/0)). Scorecard looks for `SECURITY.md` ([checks](https://github.com/ossf/scorecard/blob/main/docs/checks.md)). GitHub reads health files from the root, `.github/` or `docs/`, and does not list GOVERNANCE among them ([GitHub docs](https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/creating-a-default-community-health-file)).
- **Definition of Done:** "a formal description of the state of the Increment when it meets the quality measures" ([Scrum Guide](https://scrumguides.org/scrum-guide.html)).
- **Accessibility:** WCAG 2.2 has a criterion that matters directly for a swipe app: 2.5.7 Dragging Movements (AA), which requires a non-drag alternative. Also relevant are 2.1.1 Keyboard (A), 2.4.7 Focus Visible (AA) and 2.3.3 Animation from Interactions (AAA, reduced motion) ([W3C](https://www.w3.org/TR/WCAG22/)).

**Verdict:** a slim set works best. A pitch-style PRD, an NFR doc with real numbers, a lean arc42 architecture doc with two C4 diagrams, MADR-minimal ADRs, a data-model and state-machine doc for the file engine, a lightweight STRIDE pass, a test strategy with a DoD, and a milestone roadmap. Skip a separate risk register, a governance doc, wireframe suites and a formal test plan.

## 1. Recommended document set

| Document | Purpose | Template / source | Path | When | Already in SPEC.md |
|---|---|---|---|---|---|
| PRD (pitch style) | Problem, user, appetite, goals and non-goals, success criteria | Shape Up pitch + Google goals/non-goals | `docs/PRD.md` | **Before coding** | §1, §2, §8 (scope, v2), §10 |
| User stories + acceptance criteria | Testable behaviour, input to Playwright and Vitest tests | Given/When/Then, one file | `docs/stories.md` | **Before coding** (M1 stories at least) | §4.1 rules, §4.2 screens, implicitly |
| Non-functional requirements | Performance budgets (first photo < 1 s, swipe-to-next < 50 ms from cache, memory cap), data safety (zero overwrite or loss), accessibility (WCAG 2.2 AA, keyboard-complete, `prefers-reduced-motion`) | arc42 §10 quality scenarios | `docs/nfr.md` | **Before coding** | §5.1 numbers, §9 scattered; no explicit budgets or a11y |
| Architecture doc | Context, containers, crate boundaries, runtime flows (scan, prefetch, apply), deployment, cross-cutting concerns | arc42 lean (§1–5, 6, 8, 11, 12) + C4 context + container (Mermaid) | `docs/architecture.md` | **Before coding** | §5, §5.3, §5.4, §6 |
| ADRs | One decision per file, with alternatives | MADR 4 minimal | `docs/adr/NNNN-title.md` | **Before coding** (top 8), then continuously | §2 table, §5 rationale (e.g. rawler/LGPL) |
| Data model + state machines | Group, event log, session and journal schemas; journal file states; apply resume rules | arc42 §8 concept, Mermaid `stateDiagram` | `docs/design/file-engine.md` | **Before coding** (riskiest area) | §3, §5.2 |
| Threat model | STRIDE on the IPC boundary, custom URI scheme, CSP, updater signing, path traversal and symlinks, demo `webkitdirectory`, supply chain | OWASP 4 questions + STRIDE table, 1–2 pages | `docs/threat-model.md` | **Before coding** (light), update per milestone | §5.2 preflight, §5.3 CSP, §7 cargo-deny |
| Privacy note | No telemetry; the only network call is the updater check; the demo uploads nothing | Plain text | `PRIVACY.md` (linked from README) | Before the first public build | §5.5 only |
| Test strategy + DoD | Test pyramid, fault-injection scenarios for apply, fixture corpus, coverage gates; DoD checklist per PR | Short prose + checklist | `docs/testing.md` (DoD also in the PR template) | **Before coding** | §7 tests line |
| Benchmark plan | Method, hardware (G14 + real SD card), datasets, metrics, pass/fail tied to the NFRs | Custom, 1 page | `docs/benchmarks.md` | **Before M1** | §9 ("benchmark in milestone 1") |
| Roadmap & milestones | M0 skeleton → M1 scan/preview benchmarks → M2 swipe → M3 apply engine → M4 demo → v1 | Milestones, with GitHub Milestones mirrored | `docs/roadmap.md` | **Before coding** | §8 partly; no milestones yet |
| Risk register | Not separate: a "Risks & tech debt" section in the architecture doc | arc42 §11 | in `architecture.md` | — | §9 |
| Glossary | group, primary, sidecar, session, plan, journal, preflight | arc42 §12 | in `architecture.md` | Before coding (5 min) | Terms in §3, undefined |
| UX flows + tokens | Screen flow diagram, states (empty, error, end-of-list), motion spec; design tokens as CSS variables | Mermaid flow; tokens in code (`src/styles/tokens.css`) | `docs/ux.md` | Before UI work (M2) | §4 |
| Release & versioning | SemVer, release-please, updater key custody, SmartScreen note, changelog | Conventional Commits + release-please | `docs/releasing.md` | Before the first release | §6, §7 |
| README, LICENSE, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY.md | OSS hygiene, OpenSSF and Scorecard | opensource.guide, Contributor Covenant, GitHub private vulnerability reporting | repo root | LICENSE + README stub at repo creation; the rest before going public | §7 (listed) |
| GOVERNANCE | Skip: one line in CONTRIBUTING ("maintained by one author, BDFL") | — | — | — | — |
| Agent guide | Conventions for Claude Code agents (structure, commands, rules) | `AGENTS.md` / `CLAUDE.md` | repo root | **Before coding** | §7 points to `practices.md` |

**Split, don't duplicate:** once the split is done, SPEC.md becomes a 1-page index (or is retired). §1–2, §8 and §10 go to the PRD; §3 and §5.2 to `file-engine.md`; §4 to `ux.md` and `stories.md`; §5 and §9 to `architecture.md`; §6–7 to `releasing.md`, `testing.md` and CONTRIBUTING. The "Decisions" table and its rationale become ADRs.

## 2. First ADRs (MADR minimal)

0. Record decisions as MADR ADRs in `docs/adr/`
1. Tauri 2 + Rust + React instead of Electron or a native UI
2. Pure Rust `core` crate with no Tauri dependency
3. Stage decisions in a queue; nothing touches the disk until an explicit apply
4. Never delete or overwrite: rejects go to `_rejected/`, collisions get a suffix per group
5. Decision state as an append-only event log (undo, redo, resume)
6. Arrow keys don't change an existing decision (undo first)
7. Pair files by case-insensitive basename within the same folder only
8. Key sessions and caches by volume serial + relative path
9. SQLite (rusqlite) per source for sessions and the journal
10. Journaled apply: copy, hash, fsync, rename; roll forward only, no rollback in v1
11. Own TIFF/IFD walker for NEF previews instead of rawler (LGPL); kamadak-exif for EXIF
12. turbojpeg scaled decode + fast_image_resize preview pipeline; no mmap on removable media
13. Deliver images through a custom URI scheme, not IPC or the asset protocol
14. `Backend` interface with build-time Tauri and Demo implementations
15. Motion for swipe physics; commit first, animation catches up, at most 3 cards mounted
16. Distribution: per-user NSIS + signed updater; Windows code signing deferred (SignPath later)
17. MIT licence + cargo-deny licence allowlist
18. Conventional Commits + release-please for versioning

## 3. Drafting order

1. `AGENTS.md`, plus ADR-0000 so agents follow the conventions from the first commit
2. `PRD.md`, which fixes goals, non-goals and appetite
3. `nfr.md`: budgets and quality goals drive every later decision
4. `architecture.md` (C4 context + container, glossary, risks), then ADRs 1–5 and 8–10
5. `design/file-engine.md`: data model + state machines, then ADRs 6–7 and 11
6. `threat-model.md`, a STRIDE pass on the architecture from step 4, then ADR 13
7. `testing.md` + DoD, and `benchmarks.md`, so M1 has pass/fail criteria
8. `roadmap.md` + `stories.md` for M1–M2
9. Before M2 (UI): `ux.md`, tokens, ADRs 12, 14 and 15
10. Before going public or the first release: README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY.md, PRIVACY.md, `releasing.md`, ADRs 16–18
