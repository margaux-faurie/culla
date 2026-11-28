import { describe, expect, it } from "vitest";
import type { DecisionEvent, Verdict } from "../decisions/events";
import { createBackend } from "./demo";
import {
  DEFAULT_READ_RATE,
  draftPlan,
  fileBytes,
  folded,
  NORMAL_FACTS,
  type PlanSettings,
} from "./demo-plan";
import type { DemoGroup } from "./demo-routes";
import type { ScenarioName } from "./demo-scenarios";
import type { Backend, KeepMode, ReviewPlan, SessionInfo } from "./types";

/** Photo positions (1-based, display order) of the demo folder `E:\2026`. */
const MGF_7001 = 1;
const MGF_7004 = 4;
const MGF_7270 = 5;
const MGF_7271 = 6;
const MGF_7273 = 8;
const MGF_7276 = 11;
const MGF_8040 = 15;
const MGF_8042 = 17;

interface Setup {
  readonly backend: Backend;
  readonly session: SessionInfo;
  readonly seq: number;
}

async function decide(
  decisions: readonly [number, Verdict][],
  options: { keepMode?: KeepMode; rawFollows?: boolean; scenario?: ScenarioName | null } = {},
): Promise<Setup> {
  const backend = createBackend({ storage: null, scenario: options.scenario ?? null });
  const sourceId = (await backend.chooseSource())?.id ?? "";
  const started = await backend.startSession({ sourceId, includeSubfolders: true });
  const session = await backend.updateSession(started.id, {
    rawFollows: options.rawFollows ?? true,
    keepMode: options.keepMode ?? "move",
    destinationId: "dst-selects",
  });
  let seq = 0;
  for (const [group, verdict] of decisions) {
    seq += 1;
    const event: DecisionEvent = {
      seq,
      ts: seq,
      cause: "user",
      type: "decided",
      group_id: group,
      verdict,
    };
    await backend.recordEvent(session.id, event);
  }
  return { backend, session, seq };
}

async function planFor(
  decisions: readonly [number, Verdict][],
  options: Parameters<typeof decide>[1] = {},
): Promise<ReviewPlan & { session: SessionInfo }> {
  const { backend, session, seq } = await decide(decisions, options);
  return { ...(await backend.reviewPlan(session.id, seq)), session };
}

const DEST = "D:\\Photos\\Selects 2026";
const REJECTED = "E:\\2026\\_rejected";

describe("demo plan (US-34, design/file-engine.md §7)", () => {
  it("plans decided photos only, rejects under _rejected and keeps under the destination", async () => {
    const plan = await planFor([
      [MGF_7001, "keep"],
      [MGF_7270, "keep"],
      [MGF_8040, "reject"],
    ]);
    expect(plan.counts).toEqual({ kept: 2, rejected: 1, undecided: 21, held: 0 });
    const ids = new Set(plan.operations.map((op) => op.photoId));
    const decided = [MGF_7001, MGF_7270, MGF_8040].map((n) => plan.session.photos[n - 1]?.id);
    expect([...ids].sort()).toEqual([...decided].sort());
    const reject = plan.operations.filter((op) => op.action === "reject");
    expect(reject.map((op) => [op.from, op.to, op.kind])).toEqual([
      ["E:\\2026\\Lyon\\MGF_8040.JPG", `${REJECTED}\\Lyon\\MGF_8040.JPG`, "move"],
      ["E:\\2026\\Lyon\\MGF_8040.NEF", `${REJECTED}\\Lyon\\MGF_8040.NEF`, "move"],
    ]);
    // Photos drive (E:) to Backup drive (D:): a keep by move changes drive.
    const keep = plan.operations.filter((op) => op.action === "keep");
    expect(keep.every((op) => op.kind === "moveAcrossDrives")).toBe(true);
    expect(keep[0]?.to).toBe(`${DEST}\\MGF_7001.JPG`);
    expect(plan.rejected?.folders).toEqual([
      { folder: `${REJECTED}\\Lyon`, photos: 1, newFolder: true },
    ]);
    expect(plan.rejected?.toExists).toBe(false);
    expect(plan.kept?.folders).toEqual([
      { folder: DEST, photos: 1, newFolder: false },
      { folder: `${DEST}\\Barcelona`, photos: 1, newFolder: false },
    ]);
  });

  it("never removes, never touches undecided photos, keeps every target under its root (INV-04, INV-05)", async () => {
    const decisions: [number, Verdict][] = Array.from({ length: 24 }, (_, i) => [
      i + 1,
      i % 3 === 0 ? "reject" : "keep",
    ]);
    decisions.splice(4, 3);
    const plan = await planFor(decisions);
    const undecided = new Set([5, 6, 7].map((n) => plan.session.photos[n - 1]?.id));
    for (const op of plan.operations) {
      expect(undecided.has(op.photoId)).toBe(false);
      const root = op.action === "keep" ? `${DEST}\\` : `${REJECTED}\\`;
      expect(op.to.startsWith(root)).toBe(true);
      expect(op.from).not.toBe(op.to);
    }
    const targets = plan.operations.map((op) => folded(op.to));
    expect(new Set(targets).size).toBe(targets.length);
    expect(plan.counts.undecided).toBe(3);
  });

  it("gives every member of a group with a taken name the same suffix (US-15, INV-07)", async () => {
    const plan = await planFor([[MGF_7271, "keep"]]);
    expect(plan.operations.map((op) => op.to)).toEqual([
      `${DEST}\\Barcelona\\MGF_7271_1.JPG`,
      `${DEST}\\Barcelona\\MGF_7271_1.NEF`,
    ]);
    expect(plan.collisions).toEqual([
      { where: DEST, photos: 1, suffix: "_1", example: "MGF_7271_1" },
    ]);
    // A reject of the same photo goes to _rejected, where the name is free.
    const rejected = await planFor([[MGF_7271, "reject"]]);
    expect(rejected.collisions).toEqual([]);
  });

  it("takes the smallest suffix free on disk and in the plan (§7.3)", () => {
    const group = (name: string, folder: string[]): DemoGroup => ({
      name,
      folder,
      primary: ".JPG",
      raw: ".NEF",
      sidecars: [],
      readOnly: [],
    });
    const settings: PlanSettings = {
      sourcePath: "E:\\2026",
      sourceVolume: "Photos drive (E:)",
      destinationPath: "D:\\Selects",
      destinationVolume: "Backup drive (D:)",
      keepMode: "move",
      rawFollows: true,
      sameVolume: false,
      destinationFat32: false,
    };
    const existing = new Set(["d:\\selects\\x.jpg", "d:\\selects\\x_1.nef"]);
    const draft = draftPlan(
      [{ photoId: "a", group: group("X", []), state: "kept" }],
      settings,
      existing,
      NORMAL_FACTS,
    );
    expect(draft.groups[0]?.suffix).toBe(2);
    expect(draft.groups[0]?.ops.map((op) => op.to)).toEqual([
      "D:\\Selects\\X_2.JPG",
      "D:\\Selects\\X_2.NEF",
    ]);
  });

  it("leaves the NEF and its sidecars in place when RAW does not follow (ADR 0023)", async () => {
    const plan = await planFor([[MGF_7273, "keep"]], { rawFollows: false });
    expect(plan.operations.map((op) => op.from)).toEqual(["E:\\2026\\Barcelona\\MGF_7273.JPG"]);
    expect(plan.staying.map((f) => f.path)).toEqual([
      "E:\\2026\\Barcelona\\MGF_7273.NEF",
      "E:\\2026\\Barcelona\\MGF_7273.xmp",
    ]);
    expect(plan.kept?.nef).toBe(0);
    const follows = await planFor([[MGF_7273, "keep"]]);
    expect(follows.operations.map((op) => op.to)).toEqual([
      `${DEST}\\Barcelona\\MGF_7273.JPG`,
      `${DEST}\\Barcelona\\MGF_7273.NEF`,
      `${DEST}\\Barcelona\\MGF_7273.xmp`,
    ]);
    expect(follows.kept?.edit).toBe(1);
  });

  it("copies a camera-protected group whole, for a reject too (P13, SPEC §2)", async () => {
    const plan = await planFor([[MGF_7276, "reject"]]);
    expect(plan.operations.every((op) => op.kind === "copy" && op.cameraProtected)).toBe(true);
    expect(plan.rejected?.protectedCopies).toBe(1);
    expect(plan.estimate.bytesToWrite).toBeGreaterThan(0);
    // With RAW not following, only the read-only JPEG moves: still copied.
    const jpegOnly = await planFor([[MGF_7276, "keep"]], { rawFollows: false });
    expect(jpegOnly.operations.map((op) => op.kind)).toEqual(["copy"]);
  });

  it("plans copies for a keep by copy, with no camera-protected note (SPEC §10, plan() (f))", async () => {
    const plan = await planFor(
      [
        [MGF_7004, "keep"],
        [MGF_7276, "keep"],
      ],
      { keepMode: "copy" },
    );
    expect(plan.operations.every((op) => op.kind === "copy")).toBe(true);
    expect(plan.operations.some((op) => op.cameraProtected)).toBe(false);
    expect(plan.kept?.copied).toBe(2);
    expect(plan.estimate.changesDrive).toBe(false);
  });

  it("estimates bytes exactly and time with the second read of each source (SPEC §9)", async () => {
    const plan = await planFor([
      [MGF_7001, "keep"],
      [MGF_8040, "reject"],
    ]);
    const bytes = fileBytes("MGF_7001.JPG") + fileBytes("MGF_7001.NEF");
    expect(plan.estimate.bytesToWrite).toBe(bytes);
    expect(plan.estimate.bytesToRead).toBe(bytes * 3);
    expect(plan.estimate.seconds).toBe(Math.ceil((bytes * 3) / DEFAULT_READ_RATE));
    expect(plan.estimate.measured).toBe(false);
    expect(plan.estimate.changesDrive).toBe(true);
    const renames = await planFor([[MGF_8040, "reject"]]);
    expect(renames.estimate.bytesToWrite).toBe(0);
    expect(renames.estimate.seconds).toBe(0);
  });

  it("lists the files it does not handle and plans nothing for them (US-04)", async () => {
    const plan = await planFor([[MGF_7001, "keep"]]);
    expect(plan.otherFiles.map((f) => f.path)).toEqual([
      "E:\\2026\\Barcelona\\MGF_7290.MOV",
      "E:\\2026\\Barcelona\\MGF_7291.MOV",
      "E:\\2026\\notes.txt",
    ]);
    const froms = new Set(plan.operations.map((op) => op.from));
    expect(plan.otherFiles.some((f) => froms.has(f.path))).toBe(false);
  });

  it("follows the latest decisions: undo and clear give a new draft (§7.4)", async () => {
    const { backend, session } = await decide([[MGF_7001, "keep"]]);
    const first = await backend.reviewPlan(session.id, 1);
    await backend.recordEvent(session.id, {
      seq: 2,
      ts: 2,
      cause: "user",
      type: "cleared",
      group_id: 1,
    });
    const second = await backend.reviewPlan(session.id, 2);
    expect(second.planId).not.toBe(first.planId);
    expect(second.operations).toEqual([]);
    expect(second.counts.undecided).toBe(24);
    // The first draft is superseded.
    await expect(backend.confirmPlan(first.planId)).rejects.toThrow();
  });

  it("waits for decisions the backend has not received yet", async () => {
    const { backend, session } = await decide([[MGF_7001, "keep"]]);
    await expect(backend.reviewPlan(session.id, 2)).rejects.toThrow();
  });
});

describe("demo preflight scenarios (US-34, design/file-engine.md §8)", () => {
  it("blocks on a write-protected card", async () => {
    const plan = await planFor([[MGF_8040, "reject"]], { scenario: "write-protected" });
    expect(plan.blockers).toEqual([{ kind: "sourceWriteProtected", volume: "Photos drive (E:)" }]);
  });

  it("does not need the card writable for keeps by copy only (P3)", async () => {
    const plan = await planFor([[MGF_7001, "keep"]], {
      scenario: "write-protected",
      keepMode: "copy",
    });
    expect(plan.blockers).toEqual([]);
  });

  it("holds groups back with their reason and plans the rest (P9, P10, INV-04)", async () => {
    const plan = await planFor(
      [
        [1, "keep"],
        [2, "keep"],
        [3, "reject"],
        [MGF_7004, "keep"],
      ],
      { scenario: "held" },
    );
    expect(plan.held.map((h) => [h.name, h.reason])).toEqual([
      ["MGF_7002", "applied"],
      ["MGF_7001", "changedSinceScan"],
      ["MGF_7003", "linkOrPlaceholder"],
    ]);
    expect(plan.counts).toEqual({ kept: 1, rejected: 0, undecided: 20, held: 3 });
    const held = new Set(plan.held.map((h) => h.photoId));
    expect(plan.operations.some((op) => held.has(op.photoId))).toBe(false);
    expect(plan.blockers).toEqual([]);
  });

  it("blocks on free space with the shortfall (P5)", async () => {
    const plan = await planFor(
      [
        [MGF_7001, "keep"],
        [MGF_8042, "keep"],
      ],
      { scenario: "no-space" },
    );
    const blocker = plan.blockers[0];
    expect(blocker?.kind).toBe("notEnoughSpace");
    if (blocker?.kind === "notEnoughSpace") {
      expect(blocker.needed).toBeGreaterThan(blocker.free);
      expect(blocker.volume).toBe("Backup drive (D:)");
    }
  });

  it("warns and still allows apply (P12, P15, P17)", async () => {
    const plan = await planFor(
      [
        [1, "keep"],
        [2, "keep"],
      ],
      { scenario: "warnings" },
    );
    expect(plan.blockers).toEqual([]);
    expect(plan.warnings.map((w) => w.kind)).toEqual(["cloudSynced", "locked", "longPaths"]);
  });

  it("asks which volume when two share the card's serial, then plans again (P1, §6.3)", async () => {
    const { backend, session, seq } = await decide([[1, "keep"]], { scenario: "two-volumes" });
    const plan = await backend.reviewPlan(session.id, seq);
    const blocker = plan.blockers[0];
    expect(blocker?.kind).toBe("volumeAmbiguous");
    const choice = blocker?.kind === "volumeAmbiguous" ? blocker.choices[1]?.id : undefined;
    const again = await backend.chooseVolume(plan.planId, choice ?? "");
    expect(again.blockers).toEqual([]);
  });

  it("waits for the previews, then clears on the next check (P16)", async () => {
    const { backend, session, seq } = await decide([[1, "keep"]], { scenario: "previews-busy" });
    expect((await backend.reviewPlan(session.id, seq)).blockers).toEqual([
      { kind: "previewsBusy" },
    ]);
    expect((await backend.reviewPlan(session.id, seq)).blockers).toEqual([]);
  });

  it("blocks on an unplugged destination (P2)", async () => {
    const plan = await planFor([[1, "keep"]], { scenario: "drive-unplugged" });
    expect(plan.blockers).toEqual([
      { kind: "volumeMissing", role: "destination", volume: "Backup drive (D:)" },
    ]);
    // Rejects alone do not need the destination.
    const rejects = await planFor([[1, "reject"]], { scenario: "drive-unplugged" });
    expect(rejects.blockers).toEqual([]);
  });
});

describe("demo confirm (US-35)", () => {
  it("confirms the latest draft and touches nothing", async () => {
    const { backend, session, seq } = await decide([[1, "keep"]]);
    const plan = await backend.reviewPlan(session.id, seq);
    const result = await backend.confirmPlan(plan.planId);
    expect(result.kind).toBe("confirmed");
  });

  it("locks the session once a plan is confirmed (design/file-engine.md §5.4)", async () => {
    const { backend, session, seq } = await decide([[1, "keep"]]);
    const plan = await backend.reviewPlan(session.id, seq);
    const result = await backend.confirmPlan(plan.planId);
    expect(result.kind).toBe("confirmed");
    // A second confirm, a repeated one included, and a new draft are refused.
    await expect(backend.confirmPlan(plan.planId)).rejects.toThrow();
    await expect(backend.reviewPlan(session.id, seq)).rejects.toThrow();
  });

  it("rebuilds the plan when a name was taken since the review (P11)", async () => {
    const { backend, session, seq } = await decide([[MGF_7001, "keep"]], {
      scenario: "taken-at-confirm",
    });
    const plan = await backend.reviewPlan(session.id, seq);
    const result = await backend.confirmPlan(plan.planId);
    expect(result.kind).toBe("replanned");
    if (result.kind === "replanned") {
      expect(result.plan.replanned).toEqual([
        { name: "MGF_7001", newName: "MGF_7001_1", suffix: "_1", where: DEST },
      ]);
      expect(result.plan.operations[0]?.to).toBe(`${DEST}\\MGF_7001_1.JPG`);
      const again = await backend.confirmPlan(result.plan.planId);
      expect(again.kind).toBe("confirmed");
    }
  });

  it("refuses to start when the card was locked after the review", async () => {
    const { backend, session, seq } = await decide([[MGF_8040, "reject"]], {
      scenario: "locked-at-confirm",
    });
    const plan = await backend.reviewPlan(session.id, seq);
    expect(plan.blockers).toEqual([]);
    const result = await backend.confirmPlan(plan.planId);
    expect(result.kind).toBe("blocked");
    if (result.kind === "blocked") {
      expect(result.plan.blockers.map((b) => b.kind)).toEqual(["sourceWriteProtected"]);
    }
  });
});
