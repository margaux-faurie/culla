---
status: accepted
date: 2026-11-24
---

# Record architecture decisions with MADR

## Context and problem statement

Culla is built mostly by AI coding agents working in short sessions, with one human author. Decisions made in chat or in a PR description get lost, and agents re-open questions that were settled. We need a durable, reviewable record of why the system is the way it is. The record also feeds the blog enablement guide.

## Considered options

- Lightweight ADRs in the repo, MADR 4 minimal template
- Nygard-style ADRs (context, decision, status, consequences)
- Decisions kept only in the spec and the work log
- An external wiki

## Decision outcome

Chosen option: lightweight ADRs in `docs/adr/NNNN-title.md`, using the MADR 4 minimal template (context and problem, considered options, decision outcome with consequences). Thoughtworks rates lightweight ADRs "Adopt" and recommends keeping them in source control next to the code.

Rules:
- One decision per file, numbered in order, never renumbered.
- An accepted ADR is not edited in substance. A new ADR supersedes it, and the old one gets `status: superseded by NNNN`.
- A PR that changes a recorded decision must include the superseding ADR.
- `docs/adr/README.md` keeps the index.

### Consequences

- Good: decisions are versioned and reviewed with the code, and agents can read them.
- Good: a ready-made source for the blog guide.
- Bad: one more thing to write per decision, so ADRs stay short (20–50 lines).

## More information

- MADR: https://adr.github.io/madr/
- Thoughtworks Tech Radar, lightweight ADRs: https://www.thoughtworks.com/radar/techniques/lightweight-architecture-decision-records
