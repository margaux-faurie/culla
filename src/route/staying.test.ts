import { describe, expect, it } from "vitest";
import { fileCount, protectedNote, stayVerb } from "./staying";

describe("files labels", () => {
  it("counts one file per extension", () => {
    expect(fileCount({ name: "MGF_7001", extensions: ".NEF" })).toBe(1);
    expect(fileCount({ name: "MGF_7273", extensions: ".NEF .xmp" })).toBe(2);
  });

  it("agrees the verb with the number of files", () => {
    expect(stayVerb({ name: "MGF_7001", extensions: ".NEF" })).toBe("stays");
    expect(stayVerb({ name: "MGF_7273", extensions: ".NEF .NEF.xmp" })).toBe("stay");
  });

  it("notes camera protection only where it changed the kind", () => {
    const note = " (camera-protected: copied, original kept)";
    expect(protectedNote(true, "move")).toBe(note);
    expect(protectedNote(true, null)).toBe(note);
    // A keep by copy is a copy anyway, so the attribute changed nothing.
    expect(protectedNote(true, "copy")).toBe("");
    expect(protectedNote(false, "move")).toBe("");
  });
});
