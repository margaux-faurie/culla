---
status: accepted
date: 2026-11-27
---

# A bare `X.xmp` follows the NEF, and stays with it when "RAW follows JPEG" is off

Refines [ADR 0007](0007-pair-files-by-basename-within-one-folder.md) (which sidecars follow what). Pairing by basename within one folder is unchanged.

## Context and problem statement

ADR 0007 says `X.xmp` and `X.pp3` follow the group, so a bare `X.xmp` moves with the JPEG when "RAW follows JPEG" is off. Lightroom (and Adobe Camera Raw) writes a bare `X.xmp` for a RAW file; for a JPEG it writes the edits into the file itself. With "RAW follows" off, the NEF's Lightroom edits would then leave without the NEF. Where does a bare `X.xmp` go?

## Considered options

* It follows the group's primary, whatever "RAW follows" says (ADR 0007 as written)
* It follows the NEF when the group has one, otherwise the primary
* It is copied to both places

## Decision outcome

Chosen option: "It follows the NEF when the group has one, otherwise the primary", because a bare `X.xmp` describes the RAW in Lightroom's convention, and every sidecar follows the file it describes.

* With "RAW follows JPEG" on, the whole group moves together, as before.
* With it off, the NEF, `X.NEF.xmp`, `NKSC_PARAM/X.NEF.nksc` and `X.xmp` stay; the JPEG, `X.JPG.xmp` and `NKSC_PARAM/X.JPG.nksc` move.
* In a JPEG-only group, `X.xmp` follows the JPEG. In a lone-NEF group the NEF is the primary, so nothing changes.
* `X.pp3` still follows the group's primary.
* Decided by Margaux on 27 Nov 2026 (option (a) of the file-engine open question). Details: [design/file-engine.md](../design/file-engine.md) §3.3 and §7.1.

### Consequences

* Good, because Lightroom edits of a RAW never get separated from it.
* Good, because the rule stays "a sidecar follows the file it describes", with one convention for a bare `.xmp`.
* Bad, because an `X.xmp` written by another tool for the JPEG stays with the NEF when "RAW follows" is off; the review lists where each sidecar goes.

Source: [SPEC.md §3.1](../SPEC.md), [design/file-engine.md §3.3](../design/file-engine.md).
