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
* U makes the current photo undecided (a `Cleared` event, see [ADR 0005](0005-decision-state-as-append-only-event-log.md)). Then ←/→ work again.
* The same rule applies to the visible Keep and Reject buttons and to drag. Extension of SPEC, to confirm with the author.
* Shift+←/→ navigate without deciding.

### Consequences

* Good, because past decisions are stable while the user browses back through them.
* Good, because the reducer stays simple: a `Decided` event only ever applies to an undecided group.
* Bad, because changing a decision takes two keystrokes instead of one.
* Bad, because the critique warned that dead keys can feel like a bug; the shake and toast must be clear, and are checked in usability testing.

Source: [SPEC.md §3.2](../SPEC.md), [critique.md §1](../research/critique.md).
