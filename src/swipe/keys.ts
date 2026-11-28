/**
 * Keyboard map of the swipe screen (SPEC §4.1), as a pure function so it can be tested without a DOM.
 *
 * Letters and Ctrl shortcuts match the character on the key (`KeyboardEvent.key`, case-insensitive),
 * so Z and Ctrl+Z stay on the Z key on AZERTY and QWERTZ. Arrows, Enter and Esc match by position
 * (`KeyboardEvent.code`), as does Space; `?` matches by character. Z toggles zoom and a held
 * Space zooms until released (US-39); while zoomed the arrows pan and never decide, and Shift with
 * an arrow still browses. The EXIF overlay (I) arrives with US-40 and maps to nothing yet.
 */

export type Command =
  | "keep"
  | "reject"
  | "next"
  | "prev"
  | "nextUndecided"
  | "clear"
  | "undo"
  | "redo"
  | "filter"
  | "help"
  | "review"
  | "escape"
  /** Z: zoom to 100 % or back (US-39). */
  | "zoom"
  /** Space down: zoom while it is held; the screen ends it on Space up. */
  | "zoomHold"
  | "panLeft"
  | "panRight"
  | "panUp"
  | "panDown";

/** The fields of a `KeyboardEvent` the map reads. */
export interface KeyInput {
  readonly key: string;
  readonly code: string;
  readonly ctrlKey: boolean;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
  readonly metaKey: boolean;
  readonly repeat: boolean;
}

/**
 * Commands that change the decision log. Auto-repeat is ignored for them, so holding → keeps one
 * photo (US-23). Navigation repeats as the OS repeats the key.
 */
const CHANGES_LOG: ReadonlySet<Command> = new Set(["keep", "reject", "clear", "undo", "redo"]);

/** Toggles: a held key acts once (a held Z does not zoom in and out, proposed in US-39). */
const ONCE: ReadonlySet<Command> = new Set(["review", "zoom", "zoomHold"]);

/** `zoomed`: the photo is at 100 %, so the arrows pan instead of deciding. */
export function commandFor(input: KeyInput, zoomed = false): Command | null {
  const command = match(input, zoomed);
  if (command === null || (input.repeat && (CHANGES_LOG.has(command) || ONCE.has(command)))) {
    return null;
  }
  return command;
}

function match(input: KeyInput, zoomed: boolean): Command | null {
  // `?` is a character: Shift+, on AZERTY, Shift+ß on QWERTZ, Shift+/ on QWERTY.
  if (input.key === "?" && !input.ctrlKey && !input.metaKey) {
    return "help";
  }
  // Meta (the Windows key) belongs to the OS. Ctrl+Alt is AltGr on Windows layouts: it types a
  // character, so it is never a Ctrl shortcut.
  if (input.metaKey || input.altKey) {
    return null;
  }
  const letter = input.key.length === 1 ? input.key.toLowerCase() : "";
  if (input.ctrlKey) {
    if (letter === "z") {
      return input.shiftKey ? "redo" : "undo";
    }
    return null;
  }
  switch (input.code) {
    case "ArrowRight":
      return input.shiftKey ? "next" : zoomed ? "panRight" : "keep";
    case "ArrowLeft":
      return input.shiftKey ? "prev" : zoomed ? "panLeft" : "reject";
    case "ArrowUp":
      return zoomed && !input.shiftKey ? "panUp" : null;
    case "ArrowDown":
      return zoomed && !input.shiftKey ? "panDown" : null;
    case "Space":
      return input.shiftKey ? null : "zoomHold";
    case "Enter":
    case "NumpadEnter":
      return input.shiftKey ? null : "review";
    case "Escape":
      return "escape";
  }
  switch (letter) {
    case "u":
      return "clear";
    case "n":
      return "nextUndecided";
    case "f":
      return "filter";
    case "z":
      return input.shiftKey ? null : "zoom";
    default:
      return null;
  }
}

/** One line of the shortcut overlay and the key hints, with each key as labelled on the keyboard. */
export interface Shortcut {
  readonly keys: readonly string[];
  readonly action: string;
}

export const SHORTCUTS: readonly Shortcut[] = [
  { keys: ["→"], action: "Keep, then show the next photo" },
  { keys: ["←"], action: "Reject, then show the next photo" },
  { keys: ["Shift", "→"], action: "Next photo, no decision" },
  { keys: ["Shift", "←"], action: "Previous photo, no decision" },
  { keys: ["N"], action: "Next undecided photo" },
  { keys: ["U"], action: "Undo the decision on this photo" },
  { keys: ["Ctrl", "Z"], action: "Undo the last action, on any photo" },
  { keys: ["Ctrl", "Shift", "Z"], action: "Redo" },
  { keys: ["F"], action: "Filter: all, undecided, kept, rejected" },
  { keys: ["Z"], action: "Zoom to 100 % at the pointer, or back" },
  { keys: ["Space"], action: "Hold to zoom to 100 %" },
  { keys: ["←", "↑", "→", "↓"], action: "While zoomed: move around the photo" },
  { keys: ["Enter"], action: "Open the review" },
  { keys: ["?"], action: "Show this list" },
  { keys: ["Esc"], action: "Close this list, or leave zoom" },
];
