---
status: accepted
date: 2026-11-27
---

# Bare `X.xmp` and `X.pp3` follow the NEF, and stay with it when "RAW follows JPEG" is off

Refines [ADR 0007](0007-pair-files-by-basename-within-one-folder.md) (which sidecars follow what). Pairing by basename within one folder is unchanged.

## Context and problem statement

ADR 0007 says `X.xmp` and `X.pp3` follow the group, so with "RAW follows JPEG" off they move with the JPEG. Lightroom (and Adobe Camera Raw) writes a bare `X.xmp` for a RAW file; for a JPEG it writes the edits into the file itself. A bare `X.pp3` (RawTherapee) is also, in practice, a RAW edit. With "RAW follows" off, the NEF's edits would then leave without the NEF. Where do bare sidecars go?

## Considered options

* They follow the group's primary, whatever "RAW follows" says (ADR 0007 as written)
* They follow the NEF when the group has one, otherwise the primary
* They are copied to both places

## Decision outcome

Chosen option: "They follow the NEF when the group has one, otherwise the primary", because a bare `X.xmp` or `X.pp3` describes the RAW, and every sidecar follows the file it describes.

* With "RAW follows JPEG" on, the whole group moves together, as before.
* With it off, the NEF, `X.NEF.xmp`, `NKSC_PARAM/X.NEF.nksc`, `X.xmp` and `X.pp3` stay; the JPEG, `X.JPG.xmp` and `NKSC_PARAM/X.JPG.nksc` move.
* In a JPEG-only group, `X.xmp` and `X.pp3` follow the JPEG. In a lone-NEF group the NEF is the primary, so nothing changes.
* Decided by Margaux on 27 Nov 2026 (`X.xmp` first, then `X.pp3` the same day). Details: [design/file-engine.md](../design/file-engine.md) §3.3 and §7.1.

### Consequences

* Good, because RAW edits from Lightroom or RawTherapee never get separated from the RAW.
* Good, because the rule stays "a sidecar follows the file it describes", with one convention for bare sidecars.
* Bad, because a bare sidecar that another tool wrote for the JPEG stays with the NEF when "RAW follows" is off; the review lists where each sidecar goes.

Source: [SPEC.md §3.1](../SPEC.md), [design/file-engine.md §3.3](../design/file-engine.md).
