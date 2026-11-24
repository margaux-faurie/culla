---
status: accepted
date: 2026-11-24
---

# MIT licence + cargo-deny licence allowlist

## Context and problem statement

Culla is open source and part of the author's portfolio. Its Rust dependencies are linked statically into one binary, so a copyleft dependency would change what the binary's users may do. Which licence, and how is dependency licensing kept clean?

## Considered options

* MIT
* Apache-2.0, or MIT OR Apache-2.0
* GPL-3.0
* Licence checks: manual review, `cargo-deny` licences check, `cargo-deny` full checks plus `cargo-audit`

## Decision outcome

Chosen option: "MIT, enforced by a `cargo-deny` licence allowlist", because it is the simplest permissive licence and the allowlist makes a copyleft dependency fail CI.

* `deny.toml` allows permissive licences only (exact list to fix in M0). LGPL and GPL are not allowed, which is why `rawler`, exiv2, exiftool-rs and libraw are out ([ADR 0011](0011-own-ifd-walker-for-nef-previews-and-kamadak-exif.md)).
* `cargo-deny` also covers advisories, bans and sources ([checks](https://embarkstudios.github.io/cargo-deny/checks/index.html)), so a separate `cargo-audit` is not needed. Its bans keep `tauri` out of `culla-core` and `culla-adapters`.
* A third-party licences file ships with the binary (bundled libjpeg-turbo and SQLite included).
* JS dependencies are bundled into the UI; their licences go into the same file. Tooling for that is to decide.

### Consequences

* Good, because anyone can reuse the code and the parsers, including in commercial tools.
* Good, because licence drift is caught in CI, not in review.
* Bad, because good copyleft libraries (rawler) cannot be used, so we write and fuzz our own NEF preview walker.
* Bad, because MIT has no explicit patent grant (Apache-2.0 has one).

Source: [research-raw-perf.md §1](../research/research-raw-perf.md), [practices.md §2](../research/practices.md), [SPEC.md §7](../SPEC.md).
