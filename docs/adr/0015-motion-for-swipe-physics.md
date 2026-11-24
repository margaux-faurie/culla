---
status: accepted
date: 2026-11-24
---

# Motion for swipe physics; commit first, animation catches up, at most 3 cards mounted

## Context and problem statement

Animation is first-class in Culla, but culling is fast and keyboard-heavy. A decision must never wait for an animation, and held keys must not decide dozens of photos. Which animation library, and how do animation and state relate?

## Considered options

* Motion (`motion/react`)
* `@use-gesture/react` + react-spring
* Hand-written CSS transitions and pointer events

## Decision outcome

Chosen option: "Motion", because it provides drag, motion values and velocity out of the box and is actively maintained.

* `drag="x"`, `useMotionValue`/`useTransform` for rotation and opacity tied to x, and `onDragEnd` with `info.offset`/`info.velocity` for the commit threshold; otherwise the card springs back ([drag docs](https://motion.dev/docs/react-drag)).
* `@use-gesture/react` is still at 10.3.1 and react-spring is slow-moving.
* Each decision is committed to the reducer immediately. The exit animation catches up and never blocks input.
* At most 3 cards are mounted. The DOM is not reordered; only `transform`, `opacity` and `zIndex` animate.
* Keyboard decisions and the visible Keep and Reject buttons drive the same exit animation as a drag.
* Auto-repeat (`event.repeat`) is ignored on decision keys.
* `prefers-reduced-motion` shortens or removes the motion.

### Consequences

* Good, because fast keyboard culling stays correct whatever the frame rate.
* Good, because drag, keys and buttons share one code path.
* Bad, because Motion's card-stack tutorial is partly behind Motion+, so the stack is built from the public drag API.
* Bad, because it adds a runtime dependency to the UI bundle.

Source: [research-stack-repo.md §2](../research/research-stack-repo.md), [SPEC.md §4.1, §5.4](../SPEC.md), [critique.md §1](../research/critique.md).
