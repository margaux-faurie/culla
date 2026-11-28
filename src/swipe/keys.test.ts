import { describe, expect, it } from "vitest";
import { commandFor, type KeyInput } from "./keys";

const press = (key: string, code: string, extra: Partial<KeyInput> = {}): KeyInput => ({
  key,
  code,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  repeat: false,
  ...extra,
});

describe("swipe keyboard map (SPEC §4.1)", () => {
  it("decides with the arrows and browses with Shift", () => {
    expect(commandFor(press("ArrowRight", "ArrowRight"))).toBe("keep");
    expect(commandFor(press("ArrowLeft", "ArrowLeft"))).toBe("reject");
    expect(commandFor(press("ArrowRight", "ArrowRight", { shiftKey: true }))).toBe("next");
    expect(commandFor(press("ArrowLeft", "ArrowLeft", { shiftKey: true }))).toBe("prev");
  });

  it("ignores auto-repeat on decisions and undo, not on browsing (US-23)", () => {
    expect(commandFor(press("ArrowRight", "ArrowRight", { repeat: true }))).toBeNull();
    expect(commandFor(press("ArrowLeft", "ArrowLeft", { repeat: true }))).toBeNull();
    expect(commandFor(press("u", "KeyU", { repeat: true }))).toBeNull();
    expect(commandFor(press("z", "KeyZ", { ctrlKey: true, repeat: true }))).toBeNull();
    expect(commandFor(press("Enter", "Enter", { repeat: true }))).toBeNull();
    expect(commandFor(press("ArrowRight", "ArrowRight", { shiftKey: true, repeat: true }))).toBe(
      "next",
    );
  });

  it("matches letters by character, case-insensitive", () => {
    expect(commandFor(press("u", "KeyU"))).toBe("clear");
    expect(commandFor(press("U", "KeyU", { shiftKey: true }))).toBe("clear");
    expect(commandFor(press("n", "KeyN"))).toBe("nextUndecided");
    expect(commandFor(press("f", "KeyF"))).toBe("filter");
  });

  it("keeps Z and Ctrl+Z on the key labelled Z on AZERTY and QWERTZ (US-22)", () => {
    // AZERTY: the key labelled Z sits where QWERTY has W.
    expect(commandFor(press("z", "KeyW", { ctrlKey: true }))).toBe("undo");
    expect(commandFor(press("Z", "KeyW", { ctrlKey: true, shiftKey: true }))).toBe("redo");
    // The QWERTY Z position types W on AZERTY: not undo.
    expect(commandFor(press("w", "KeyZ", { ctrlKey: true }))).toBeNull();
    // QWERTZ: Z sits where QWERTY has Y.
    expect(commandFor(press("z", "KeyY", { ctrlKey: true }))).toBe("undo");
    // AZERTY: U and N are where QWERTY has them; A sits on KeyQ and is not a shortcut.
    expect(commandFor(press("u", "KeyU"))).toBe("clear");
    expect(commandFor(press("a", "KeyQ"))).toBeNull();
  });

  it("matches the arrows, Enter and Esc by position", () => {
    expect(commandFor(press("Enter", "Enter"))).toBe("review");
    expect(commandFor(press("Enter", "NumpadEnter"))).toBe("review");
    expect(commandFor(press("Escape", "Escape"))).toBe("escape");
  });

  it("opens the overlay on ? whatever key types it", () => {
    expect(commandFor(press("?", "Slash", { shiftKey: true }))).toBe("help");
    expect(commandFor(press("?", "KeyM", { shiftKey: true }))).toBe("help"); // AZERTY: Shift+,
    expect(commandFor(press("?", "Minus", { shiftKey: true }))).toBe("help"); // QWERTZ: Shift+ß
  });

  it("leaves AltGr, the Windows key and other Ctrl combinations alone", () => {
    // AltGr arrives as Ctrl+Alt on Windows.
    expect(commandFor(press("z", "KeyZ", { ctrlKey: true, altKey: true }))).toBeNull();
    expect(commandFor(press("u", "KeyU", { altKey: true }))).toBeNull();
    expect(commandFor(press("ArrowRight", "ArrowRight", { metaKey: true }))).toBeNull();
    expect(commandFor(press("u", "KeyU", { ctrlKey: true }))).toBeNull();
    expect(commandFor(press("ArrowRight", "ArrowRight", { ctrlKey: true }))).toBeNull();
  });

  it("zooms with Z on the key labelled Z, and with Space by position (US-39)", () => {
    expect(commandFor(press("z", "KeyZ"))).toBe("zoom");
    expect(commandFor(press("Z", "KeyZ"))).toBe("zoom"); // Caps Lock
    // AZERTY: Z sits where QWERTY has W; the QWERTY Z position types W.
    expect(commandFor(press("z", "KeyW"))).toBe("zoom");
    expect(commandFor(press("w", "KeyZ"))).toBeNull();
    expect(commandFor(press(" ", "Space"))).toBe("zoomHold");
    // A held Z or Space acts once.
    expect(commandFor(press("z", "KeyZ", { repeat: true }))).toBeNull();
    expect(commandFor(press(" ", "Space", { repeat: true }))).toBeNull();
    // Shift+Z is not zoom; Ctrl+Z stays undo.
    expect(commandFor(press("Z", "KeyZ", { shiftKey: true }))).toBeNull();
    expect(commandFor(press("z", "KeyZ", { ctrlKey: true }))).toBe("undo");
  });

  it("pans with the arrows while zoomed, never deciding; Shift still browses", () => {
    expect(commandFor(press("ArrowRight", "ArrowRight"), true)).toBe("panRight");
    expect(commandFor(press("ArrowLeft", "ArrowLeft"), true)).toBe("panLeft");
    expect(commandFor(press("ArrowUp", "ArrowUp"), true)).toBe("panUp");
    expect(commandFor(press("ArrowDown", "ArrowDown"), true)).toBe("panDown");
    // A held arrow pans on.
    expect(commandFor(press("ArrowRight", "ArrowRight", { repeat: true }), true)).toBe("panRight");
    expect(commandFor(press("ArrowRight", "ArrowRight", { shiftKey: true }), true)).toBe("next");
    expect(commandFor(press("ArrowLeft", "ArrowLeft", { shiftKey: true }), true)).toBe("prev");
    // Not zoomed: up and down do nothing.
    expect(commandFor(press("ArrowUp", "ArrowUp"))).toBeNull();
    expect(commandFor(press("ArrowDown", "ArrowDown"))).toBeNull();
  });

  it("maps the EXIF overlay to nothing yet (US-40)", () => {
    expect(commandFor(press("i", "KeyI"))).toBeNull();
  });
});
