import { describe, expect, it } from "vitest";
import type { DestinationSummary } from "../backend/types";
import { keptPlace, leftInPlaceNote } from "./places";

const selects: DestinationSummary = {
  id: "d1",
  folder: { path: "D:\\Photos\\Selects 2026", volume: "Backup drive (D:)" },
  available: true,
};

describe("short route places", () => {
  it("words the Kept branch's place with the verb chosen", () => {
    expect(keptPlace(selects, "move")).toBe("Moved to D:\\Photos\\Selects 2026");
    expect(keptPlace(selects, "copy")).toBe("Copied to D:\\Photos\\Selects 2026");
    expect(keptPlace(selects, null)).toBe("To D:\\Photos\\Selects 2026");
    expect(keptPlace(null, "move")).toBe("Moved to the destination you choose");
    expect(keptPlace({ ...selects, available: false }, "copy")).toBe(
      "Copied to the destination you choose",
    );
  });

  it("says which photos stay where they are", () => {
    expect(leftInPlaceNote(0)).toBe("Undecided photos are not touched.");
    expect(leftInPlaceNote(2)).toBe("Undecided and held-back photos are not touched.");
  });
});
