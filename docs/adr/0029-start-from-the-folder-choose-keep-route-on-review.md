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

Decided by Margaux on 28 Nov 2026:

* The start screen shows "What do you want to sort?", "Choose a folder" and the recent folders. Choosing a folder starts sorting at once: no route panel, no options and no Start button.
* No options at all on the start screen, not even folded: subfolders are always included. A setting may come later.
* Defaults, without asking each time: subfolders included, RAW follows JPEG on, move or copy as last chosen, the destination last used: the one used for this folder, otherwise the last one used anywhere.
* On review, right before Apply: "Kept → Move | Copy to <destination>", rejected photos to `_rejected`, with the route view. Kept photos cannot be applied until both are set; the first time there is no last choice, so the user picks them there.
* Rejects alone can be applied with neither move or copy nor a destination chosen.
* RAW follows JPEG sits in a folded "Options" on review, on by default, changeable until Apply.
* The swipe screen copes with no destination yet. The webview still prints backend display strings only.

### Proposed by the agent (SPEC §10)

Awaiting Margaux, listed in SPEC §10 ("Simple start"): when a choice is remembered (on choice and again at confirm), the wording of the review and swipe screens ("same as last time", the Apply hints, "You choose where on Review"), that only kept photos that would get operations need a route (as the core's `plan()` (c)), that every change on review plans again, the backend shape (`startSession`, `updateSession`, `NoKeepRouteError`), and what a click on a recent folder that is not connected, or on a folder with no photos, does.

### Consequences

* Good, because starting takes one click and the screen asks one question.
* Good, because move or copy is chosen next to the plan it changes, with the counts and the route in view.
* Bad, because the first time the swipe screen cannot say where kept photos go, and the review asks one more thing before Apply.
* Bad, because a remembered move or copy is applied without being asked again; the review shows it before Apply.
* Bad, because a folder cannot be sorted without its subfolders until a setting exists.

Source: SPEC §2 ("Start screen", "Keep"), §4.2, §10 ("Simple start"); stories.md US-21, US-30, US-34, US-44.
