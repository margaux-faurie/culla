import { describe, expect, it } from "vitest";
import { createBackend } from "../backend/demo";
import type { BranchPlan, KeepMode, ReviewPlan } from "../backend/types";
import type { Verdict } from "../decisions/events";
import {
  blockerText,
  branchFiles,
  branchTitle,
  canApply,
  collisionText,
  confirmText,
  disabledReason,
  duration,
  estimateText,
  formatBytes,
  otherFilesText,
  primaryLabel,
  protectedText,
} from "./model";

/** A review plan from the demo backend for these decisions (group positions, 1-based). */
async function reviewOf(
  decisions: readonly [number, Verdict][],
  keepMode: KeepMode = "move",
): Promise<ReviewPlan> {
  const backend = createBackend({ storage: null, scenario: null });
  const sourceId = (await backend.chooseSource())?.id ?? "";
  const session = await backend.startSession({
    sourceId,
    includeSubfolders: true,
    rawFollows: true,
    keepMode,
    destinationId: "dst-selects",
  });
  let seq = 0;
  for (const [group, verdict] of decisions) {
    seq += 1;
    await backend.recordEvent(session.id, {
      seq,
      ts: seq,
      cause: "user",
      type: "decided",
      group_id: group,
      verdict,
    });
  }
  return backend.reviewPlan(session.id, seq);
}

const branch = (extra: Partial<BranchPlan> = {}): BranchPlan => ({
  photos: 96,
  moved: 96,
  copied: 0,
  protectedCopies: 0,
  to: "D:\\Photos\\Selects 2026",
  toExists: true,
  jpeg: 96,
  nef: 96,
  edit: 12,
  bytes: 2_104_000_000,
  folders: [],
  photoIds: [],
  ...extra,
});

describe("review model (US-34)", () => {
  it("formats bytes in decimal units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(8_400)).toBe("8 KB");
    expect(formatBytes(36_200_000)).toBe("36.2 MB");
    expect(formatBytes(2_104_000_000)).toBe("2.1 GB");
  });

  it("words a branch as on the canvas", () => {
    expect(branchTitle(branch(), "kept")).toBe("96 kept · moved");
    expect(branchTitle(branch({ moved: 95, copied: 1 }), "kept")).toBe(
      "96 kept · 95 moved, 1 copied",
    );
    expect(branchTitle(branch({ moved: 0, copied: 96 }), "kept")).toBe("96 kept · copied");
    expect(branchFiles(branch())).toBe("96 JPEG · 96 NEF · 12 edit files · 2.1 GB");
    expect(branchFiles(branch({ nef: 0, edit: 1 }))).toBe("96 JPEG · 1 edit file · 2.1 GB");
  });

  it("words collisions, camera protection and other files", () => {
    expect(
      collisionText({ where: "Selects 2026", photos: 2, suffix: "_1", example: "MGF_7101_1" }),
    ).toBe(
      "2 names are taken in Selects 2026. Those photos get _1 (MGF_7101_1). Nothing is replaced.",
    );
    expect(protectedText(branch({ protectedCopies: 1 }), "kept", "Photos drive (E:)")).toBe(
      "1 kept photo is protected on the camera. It is copied, and the original stays on Photos drive (E:).",
    );
    expect(protectedText(branch(), "kept", "E:")).toBeNull();
    expect(
      otherFilesText([
        { path: "E:\\a.MOV", extension: ".MOV", reason: "unknownType" },
        { path: "E:\\b.MOV", extension: ".MOV", reason: "unknownType" },
        { path: "E:\\notes.txt", extension: ".txt", reason: "unknownType" },
      ]),
    ).toBe("3 other files (.mov, .txt) are left alone.");
    expect(otherFilesText([])).toBeNull();
  });

  it("names the lock switch and the shortfall in blockers", () => {
    expect(blockerText({ kind: "sourceWriteProtected", volume: "SD card (F:)" })).toContain(
      "lock switch",
    );
    expect(
      blockerText({
        kind: "notEnoughSpace",
        volume: "Backup drive (D:)",
        needed: 3_000_000_000,
        free: 1_000_000_000,
      }),
    ).toContain("Free 2.0 GB");
  });

  it("says renames take seconds and moves across drives are read back", async () => {
    const renames = await reviewOf([[15, "reject"]]);
    expect(estimateText(renames)).toBe(
      "Every photo stays on its drive, so this takes a few seconds.",
    );
    const across = await reviewOf([[1, "keep"]]);
    expect(estimateText(across)).toMatch(
      /^Kept photos change drive, so each copy is read back and checked before its original leaves Photos drive \(E:\)\. (Under a minute|About \d+ minutes?)\.$/,
    );
    expect(duration(0)).toBe("a few seconds");
    expect(duration(30)).toBe("under a minute");
    expect(duration(240)).toBe("about 4 minutes");
  });

  it("enables Apply only with something to apply and no blocker", async () => {
    const none = await reviewOf([]);
    expect(canApply(none)).toBe(false);
    expect(disabledReason(none)).toBe("Nothing to apply yet. Keep or reject some photos first.");
    const some = await reviewOf([[1, "keep"]]);
    expect(canApply(some)).toBe(true);
    expect(disabledReason(some)).toBeNull();
    const blocked: ReviewPlan = { ...some, blockers: [{ kind: "previewsBusy" }] };
    expect(canApply(blocked)).toBe(false);
    expect(disabledReason(blocked)).toBe("Apply waits until the problem above is solved.");
  });
});

describe("confirm text (US-35)", () => {
  it("says Move when every planned photo moves", async () => {
    const plan = await reviewOf([
      [1, "keep"],
      [2, "keep"],
      [15, "reject"],
    ]);
    expect(primaryLabel(plan)).toBe("Move 3 photos");
    const text = confirmText(plan);
    expect(text.title).toBe("Move 3 photos?");
    expect(text.confirm).toBe("Move 3 photos");
    expect(text.cancel).toBe("Go back");
    expect(text.body.slice(0, 4)).toEqual([
      "2 kept photos will be moved to D:\\Photos\\Selects 2026.",
      "1 rejected photo will be moved to E:\\2026\\_rejected.",
      "Nothing is deleted.",
      "21 undecided photos stay where they are.",
    ]);
    expect(text.body[4]).toMatch(/^7 files, [\d.]+ MB, (under a minute|about \d+ minutes?)\.$/);
  });

  it("says Apply to when some photos are copied", async () => {
    const copy = await reviewOf([[1, "keep"]], "copy");
    expect(primaryLabel(copy)).toBe("Apply to 1 photo");
    expect(confirmText(copy).body[0]).toBe(
      "1 kept photo will be copied to D:\\Photos\\Selects 2026.",
    );
    // MGF_7276 is protected on the camera: copied, while MGF_7277 moves.
    const mixed = await reviewOf([
      [11, "keep"],
      [12, "keep"],
    ]);
    expect(primaryLabel(mixed)).toBe("Apply to 2 photos");
    expect(confirmText(mixed).body[0]).toBe(
      "2 kept photos go to D:\\Photos\\Selects 2026: 1 moved, 1 copied.",
    );
  });

  it("states rejects only, and the held-back photos left out", async () => {
    const rejects = await reviewOf([[15, "reject"]]);
    const text = confirmText(rejects);
    expect(text.body[0]).toBe("1 rejected photo will be moved to E:\\2026\\_rejected.");
    expect(text.body).not.toContain("kept");
    const held: ReviewPlan = { ...rejects, counts: { ...rejects.counts, held: 2, undecided: 1 } };
    expect(confirmText(held).body).toContain("2 held-back photos are left out.");
    expect(confirmText(held).body).toContain("1 undecided photo stays where it is.");
  });
});
