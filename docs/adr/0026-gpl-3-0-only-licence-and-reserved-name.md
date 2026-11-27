---
status: accepted
date: 2026-11-27
decision-makers: Margaux Faurie
---

# GPL-3.0-only licence, reserved name and logo, no outside pull requests

Supersedes the licence choice in [ADR 0017](0017-mit-licence-and-cargo-deny-licence-allowlist.md). Its `cargo-deny` allowlist stays.

## Context and problem statement

The repository went public on 27 Nov 2026. The author wants the project open source, as a portfolio piece, but does not want others to take it, close it and sell it, or ship it under her name. MIT allows all of that as long as the copyright line is kept. Which licence, what about the name, and who may contribute code?

## Considered options

* GPL-3.0-only
* GPL-3.0-or-later
* AGPL-3.0
* PolyForm Noncommercial (source-available)
* Keep MIT

## Decision outcome

Chosen option: "GPL-3.0-only", decided by Margaux on 27 Nov 2026.

* Anyone may use, study, change and share Culla. Whoever distributes a copy or a modified version must license it under GPL-3.0, publish its source and keep the copyright and licence notices. A closed, paid copy is not allowed.
* "Only", not "or later": future GPL versions published by the FSF do not apply unless the author chooses them.
* Copyright © 2026 Margaux Faurie. The name "Culla" and its logo are not licensed: forks are welcome under the GPL but must use another name ([TRADEMARKS.md](../../TRADEMARKS.md)). The name is not a registered trademark yet; registration (INPI or EUIPO) is the author's call.
* Issues and ideas are welcome; outside pull requests are not merged for now ([CONTRIBUTING.md](../../CONTRIBUTING.md)). The code keeps a single author, so it can be relicensed or dual-licensed later without asking anyone.
* Dependencies: every licence on the `deny.toml` allowlist (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, Unlicense, CC0-1.0) is GPL-3.0-compatible, and the per-crate MPL-2.0 exceptions are too (MPL-2.0 §3.3). Culla's own crates are unpublished, so `cargo-deny` skips their licence (`[licenses.private] ignore = true`).
* Copies taken while the repository was public under MIT (a few minutes on 27 Nov 2026) keep MIT for that snapshot.

### Consequences

* Good, because nobody can ship a closed or paid copy, and every fork must say it is based on Culla.
* Good, because the project stays OSI open source, which matters for the portfolio and the blog guide.
* Good, because a single author keeps every option open, including a commercial licence later.
* Bad, because some companies avoid GPL code, so reuse of the parsers in other tools drops.
* Bad, because the GPL is not AGPL: someone could host a modified web demo without publishing its source. The demo is a UI shell with no file access, so this is accepted.
* Bad, because enforcement is up to the author, and the protection of AI-written code is legally unsettled. This is not legal advice.
* Neutral: `rawler` (LGPL-2.1) is now licence-compatible, but ADR 0011 still rules it out for `memmap2` and the own walker is in place.

Source: [GPL-3.0 text](../../LICENSE), [ADR 0017](0017-mit-licence-and-cargo-deny-licence-allowlist.md).
