import { describe, expect, it } from "vitest";
import { createBackend } from "../backend/demo";
import type { BranchPlan, DestinationSummary, KeepMode, ReviewPlan } from "../backend/types";
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
  findingsCount,
  formatBytes,
  heldLine,
  keepRouteHint,
  otherFilesText,
  primaryLabel,
  protectedText,
  rawNote,
  readySummary,
  replannedSummary,
  worthKnowing,
} from "./model";

/** A review plan from the demo backend for these decisions (group positions, 1-based). */
async function reviewOf(
  decisions: readonly [number, Verdict][],
  keepMode: KeepMode = "move",
): Promise<ReviewPlan> {
  const backend = createBackend({ storage: null, scenario: null });
  const sourceId = (await backend.chooseSource())?.id ?? "";
  const started = await backend.startSession({ sourceId, includeSubfolders: true });
  const session = await backend.updateSession(started.id, {
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

describe("keep route on review (decided by Margaux 28 Nov 2026)", () => {
  const selects: DestinationSummary = {
    id: "d1",
    folder: { path: "D:\\Photos\\Selects 2026", volume: "Backup drive (D:)" },
    available: true,
  };
  const unplugged: DestinationSummary = { ...selects, available: false };

  it("asks for move or copy and a destination before kept photos can be applied", () => {
    expect(keepRouteHint(null, null)).toBe(
      "Choose move or copy and a destination for the kept photos first.",
    );
    expect(keepRouteHint(null, selects)).toBe("Choose move or copy for the kept photos first.");
    expect(keepRouteHint("move", null)).toBe("Choose a destination for the kept photos first.");
    expect(keepRouteHint("copy", unplugged)).toBe(
      "The destination's drive is not connected. Connect it, or choose another destination.",
    );
    expect(keepRouteHint(null, unplugged)).toBe(
      "Choose move or copy, and a destination that is connected, for the kept photos first.",
    );
    expect(keepRouteHint("move", selects)).toBe(
      "Choose move or copy and a destination for the kept photos first.",
    );
  });
});

describe("lean review (decided by Margaux 28 Nov 2026: essentials visible, details folded)", () => {
  it("lists what is worth knowing: taken names, camera protection, warnings", async () => {
    const plan = await reviewOf([
      [5, "keep"],
      [6, "keep"],
      [11, "keep"],
    ]);
    expect(worthKnowing(plan)).toEqual([
      "1 name is taken in D:\\Photos\\Selects 2026. That photo gets _1 (MGF_7271_1). Nothing is replaced.",
      "1 kept photo is protected on the camera. It is copied, and the original stays on Photos drive (E:).",
    ]);
    const warned: ReviewPlan = {
      ...plan,
      collisions: [],
      warnings: [{ kind: "syncRootUnknown" }],
    };
    expect(worthKnowing(warned)).toEqual([
      "1 kept photo is protected on the camera. It is copied, and the original stays on Photos drive (E:).",
      "Culla could not check whether the destination is in a cloud-synced folder.",
    ]);
  });

  it("counts every line the folded findings list", async () => {
    const plan = await reviewOf([[5, "keep"]]);
    // The demo folder has three other files: one line for them.
    expect(findingsCount({ ...plan, collisions: [] })).toBe(1);
    expect(findingsCount({ ...plan, collisions: [], otherFiles: [] })).toBe(0);
    const held: ReviewPlan = {
      ...plan,
      collisions: [],
      otherFiles: [],
      held: [
        { photoId: "a", name: "A", decision: "kept", folder: "E:\\2026", reason: "missing" },
        { photoId: "b", name: "B", decision: "kept", folder: "E:\\2026", reason: "applied" },
      ],
      replanned: [{ name: "C", where: "Selects", suffix: "_1", newName: "C_1" }],
    };
    expect(findingsCount(held)).toBe(3);
  });

  it("says in one line that nothing stops the apply, and whether there is more to read", async () => {
    const plan = await reviewOf([[5, "keep"]]);
    const quiet: ReviewPlan = { ...plan, collisions: [], otherFiles: [] };
    expect(readySummary(quiet)).toBe("Checked. Nothing stops the apply.");
    expect(readySummary({ ...quiet, otherFiles: plan.otherFiles })).toBe(
      "Checked. Nothing stops the apply. 1 thing is worth knowing.",
    );
    expect(readySummary({ ...plan, warnings: [{ kind: "syncRootUnknown" }] })).toBe(
      "Checked. Nothing stops the apply. 2 things are worth knowing.",
    );
    expect(readySummary({ ...plan, blockers: [{ kind: "previewsBusy" }] })).toBeNull();
    expect(readySummary(await reviewOf([]))).toBeNull();
  });

  it("words a re-plan and a held photo in short", () => {
    expect(replannedSummary(1)).toBe(
      "1 name changed since you checked. Look at it again, then apply.",
    );
    expect(replannedSummary(3)).toBe(
      "3 names changed since you checked. Look at them again, then apply.",
    );
    expect(
      heldLine({
        photoId: "a",
        name: "MGF_7001",
        decision: "kept",
        folder: "E:\\2026",
        reason: "changedSinceScan",
      }),
    ).toBe("MGF_7001 (kept, in E:\\2026): changed since scan, rescan.");
  });

  it("says whether RAW and edit files go with their JPEG", () => {
    expect(rawNote(true)).toBe("RAW and edit files go with their JPEG.");
    expect(rawNote(false)).toBe("RAW and edit files stay where they are.");
  });
});
