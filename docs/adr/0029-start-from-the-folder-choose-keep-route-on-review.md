---
status: accepted
date: 2026-11-28
decision-makers: Margaux Faurie (decided 28 Nov 2026)
---

# Start sorting from the folder; choose move or copy and the destination on review

Decided by Margaux on 28 Nov 2026. It replaces the rule of 27 Nov 2026, recorded in SPEC §2 ("Keep") and US-21, that move or copy is chosen fresh each session with nothing preselected and that Start stays disabled until it is chosen. No earlier ADR recorded that rule.

## Context and problem statement

The start screen asked for the folder, include subfolders, RAW follows JPEG, move or copy and the destination, with the "Where your photos will go" panel beside them, before any photo was shown. It was too heavy for the moment it serves: picking what to sort. Most of those choices only matter when the files are applied. Where should each choice live, and what does a session start with?

## Considered options

* Keep every choice on the start screen (as built in M3)
* Start screen with the folder only; move or copy and the destination on review, right before Apply; defaults for the rest
* Ask move or copy and the destination in a dialog after choosing the folder

## Decision outcome

Chosen option: "Start screen with the folder only; move or copy and the destination on review", because the choices are made where they take effect, and sorting starts in one click.

* The start screen shows "What do you want to sort?", "Choose a folder" and the recent folders. Choosing a folder, or clicking a connected recent folder, starts sorting at once.
* Defaults, without asking each time: include subfolders on, RAW follows JPEG on, the destination last used ("same as last time"), move or copy as last chosen.
* On review the Kept branch reads "Kept photos → Move | Copy to <destination>", rejected photos go to `_rejected`, and the route view stays. Apply stays disabled, with a hint, until kept photos have both; the first time there is no last choice, so the user picks them there.
* Include subfolders and RAW follows JPEG stay changeable but out of the way. Where exactly, and how a change of include subfolders is handled once swiping has started, is proposed by the agent in SPEC §10 ("Simple start").
* The swipe screen copes with no destination yet: "You choose where on Review". The webview still prints backend display strings only.

### Consequences

* Good, because starting takes one click and the screen asks one question.
* Good, because move or copy is chosen next to the plan it changes, with the counts and the route in view.
* Bad, because the first time the swipe screen cannot say where kept photos go, and the review asks one more thing before Apply.
* Bad, because a remembered move or copy is applied without being asked again; the review shows it before Apply.

Source: SPEC §2 ("Start screen", "Keep"), §4.2, §10 ("Simple start"); stories.md US-21, US-30, US-34, US-44.
