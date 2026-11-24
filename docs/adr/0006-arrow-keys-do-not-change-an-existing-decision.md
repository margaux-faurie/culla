---
status: accepted
date: 2026-11-24
---

# Arrow keys don't change an existing decision (undo first)

## Context and problem statement

When the user comes back to a photo that already has a decision, what should ← and → do?

## Considered options

* Overwrite the decision and record it in the undo history (proposed by the critique)
* Do nothing, with feedback that explains how to change it

## Decision outcome

Chosen option: "Do nothing, with feedback", because it is the author's choice: undo first, then re-decide. A stray arrow press on a decided photo can never flip it.

* ←/→ on a decided photo trigger a short shake animation and a toast saying "Press U to undo".
* U makes the current photo undecided (an appended event). Then ←/→ work again.
* Shift+←/→ navigate without deciding.

### Consequences

* Good, because past decisions are stable while the user browses back through them.
* Good, because the reducer stays simple: a decision event only ever starts from `undecided`.
* Bad, because changing a decision takes two keystrokes instead of one.
* Bad, because the critique warned that dead keys can feel like a bug; the shake and toast must be clear, and are checked in usability testing.

Source: [SPEC.md §3.2](../SPEC.md), [critique.md §1](../research/critique.md).
