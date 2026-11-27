/**
 * Keyboard map of the swipe screen (SPEC §4.1), as a pure function so it can be tested without a DOM.
 *
 * Letters and Ctrl shortcuts match the character on the key (`KeyboardEvent.key`, case-insensitive),
 * so Z and Ctrl+Z stay on the Z key on AZERTY and QWERTZ. Arrows, Enter and Esc match by position
 * (`KeyboardEvent.code`); `?` matches by character. Zoom (Z, hold Space) and the EXIF overlay (I)
 * arrive in M4 (US-39, US-40) and map to nothing yet.
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
  | "escape";

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

export function commandFor(input: KeyInput): Command | null {
  const command = match(input);
  if (command === null || (input.repeat && (CHANGES_LOG.has(command) || command === "review"))) {
    return null;
  }
  return command;
}

function match(input: KeyInput): Command | null {
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
      return input.shiftKey ? "next" : "keep";
    case "ArrowLeft":
      return input.shiftKey ? "prev" : "reject";
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
  { keys: ["Enter"], action: "Open the review" },
  { keys: ["?"], action: "Show this list" },
  { keys: ["Esc"], action: "Close this list" },
];
