import { describe, expect, it } from "vitest";
import type { PhotoRoute } from "../backend/types";
import { alreadyDecidedToast, decidedNote, keepLine } from "./route-text";

const route = (keptTo: string | null): PhotoRoute => ({
  folder: "E:\\2026",
  origin: "E:\\2026\\MGF_7001",
  going: { name: "MGF_7001", extensions: ".JPG .NEF" },
  staying: null,
  keptTo,
  rejectedTo: "moved to E:\\2026\\_rejected",
  cameraProtected: false,
});

describe("swipe route words (SPEC §4.2)", () => {
  it("prints the backend's kept route when there is one", () => {
    const r = route("moved to D:\\Photos\\Selects 2026");
    expect(keepLine(r)).toBe("moved to D:\\Photos\\Selects 2026");
    expect(decidedNote(r, "kept")).toBe("Kept: will be moved to D:\\Photos\\Selects 2026.");
    expect(alreadyDecidedToast(r, "kept")).toBe(
      "Already kept. Will be moved to D:\\Photos\\Selects 2026. Press U to undo.",
    );
  });

  it("says where is chosen on Review while no destination is set", () => {
    const r = route(null);
    expect(keepLine(r)).toBe("You choose where on Review");
    expect(decidedNote(r, "kept")).toBe("Kept: you choose where on Review.");
    expect(alreadyDecidedToast(r, "kept")).toBe(
      "Already kept. You choose where on Review. Press U to undo.",
    );
  });

  it("words rejects the same way whatever the kept route", () => {
    expect(decidedNote(route(null), "rejected")).toBe(
      "Rejected: will be moved to E:\\2026\\_rejected.",
    );
    expect(alreadyDecidedToast(route(null), "rejected")).toBe(
      "Already rejected. Will be moved to E:\\2026\\_rejected. Press U to undo.",
    );
  });
});
