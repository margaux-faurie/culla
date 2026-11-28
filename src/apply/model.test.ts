import { describe, expect, it } from "vitest";
import type {
  ApplyProgress,
  ApplyReport,
  CurrentFile,
  ReportFile,
  ReportGroup,
} from "../backend/types";
import {
  bytesLine,
  canContinue,
  doneLine,
  fileText,
  groupReason,
  haltText,
  interruptedText,
  outcomeOf,
  pausedText,
  percent,
  photoLine,
  progressAnnouncement,
  reportActions,
  reportLead,
  reportSections,
  reportTitle,
  runningTitle,
  STOP_LABEL,
  stepLines,
  summaryParts,
  timeLeft,
} from "./model";

const progress = (p: Partial<ApplyProgress> = {}): ApplyProgress => ({
  photosDone: 61,
  photos: 138,
  bytesDone: 1_000_000_000,
  bytes: 2_100_000_000,
  secondsLeft: 120,
  measured: false,
  mode: "move",
  ...p,
});

const current = (c: Partial<CurrentFile> = {}): CurrentFile => ({
  photoId: "p1",
  photoName: "MGF_7502",
  jpeg: 1,
  nef: 1,
  edit: 1,
  fileName: "MGF_7502.NEF",
  index: 2,
  count: 3,
  kind: "xmove",
  step: "verifying",
  action: "keep",
  to: "D:\\Photos\\Selects 2026\\Barcelona",
  from: "E:\\2026\\Barcelona",
  cameraProtected: false,
  ...c,
});

const file = (
  state: ReportFile["state"],
  reason: ReportFile["reason"] = null,
  name = "A.JPG",
): ReportFile => ({
  fileName: name,
  from: `E:\\2026\\${name}`,
  to: `D:\\Selects\\${name}`,
  state,
  reason,
  code: null,
});

const group = (name: string, files: ReportFile[], g: Partial<ReportGroup> = {}): ReportGroup => ({
  photoId: name,
  name,
  action: "keep",
  kind: "xmove",
  cameraProtected: false,
  retryable: false,
  files,
  ...g,
});

const report = (groups: ReportGroup[], r: Partial<ApplyReport> = {}): ApplyReport => ({
  runId: "run-1",
  end: "finished",
  groups,
  held: [],
  partialsKept: [],
  sourceVolume: "Photos drive (E:)",
  destinationVolume: "Backup drive (D:)",
  ...r,
});

describe("apply progress words (US-36)", () => {
  it("says what the canvas says", () => {
    expect(runningTitle(progress())).toBe("Moving your photos");
    expect(runningTitle(progress({ mode: "copy" }))).toBe("Copying your photos");
    expect(runningTitle(progress({ mode: "mixed" }))).toBe("Moving and copying your photos");
    expect(doneLine(progress())).toBe("61 of 138 photos done. You can keep using your computer.");
    expect(timeLeft(progress())).toBe("About 2 minutes left");
    expect(timeLeft(progress({ secondsLeft: 20 }))).toBe("Under a minute left");
    expect(timeLeft(progress({ secondsLeft: 0 }))).toBe("A few seconds left");
    expect(bytesLine(progress())).toBe("1.0 of 2.1 GB");
    expect(bytesLine(progress({ bytesDone: 4_200_000, bytes: 36_000_000 }))).toBe("4.2 of 36.0 MB");
    expect(percent(progress())).toBe(48);
    expect(percent(progress({ bytes: 0, photos: 4, photosDone: 1 }))).toBe(25);
  });

  it("names the photo and its files, and the current file", () => {
    expect(photoLine(current())).toBe("MGF_7502 · JPEG, NEF and 1 edit file");
    expect(photoLine(current({ jpeg: 0, edit: 0 }))).toBe("MGF_7502 · NEF");
    expect(photoLine(current({ nef: 0, edit: 2 }))).toBe("MGF_7502 · JPEG and 2 edit files");
  });

  it("words each step of a move to another drive in order", () => {
    expect(stepLines(current({ step: "copying" }))).toEqual([
      { text: "Copying to D:\\Photos\\Selects 2026\\Barcelona", state: "current" },
      { text: "Reading the copy back and checking it matches", state: "next" },
      { text: "Then the original leaves E:\\2026\\Barcelona", state: "next" },
    ]);
    expect(stepLines(current()).map((s) => s.state)).toEqual(["done", "current", "next"]);
    expect(stepLines(current())[0]?.text).toBe("Copied to D:\\Photos\\Selects 2026\\Barcelona");
    expect(stepLines(current({ step: "removingSource" }))).toContainEqual({
      text: "The original leaves E:\\2026\\Barcelona",
      state: "current",
    });
    expect(stepLines(current({ step: "retrying" }))[0]?.text).toBe(
      "Another app has MGF_7502.NEF open. Trying again",
    );
    expect(stepLines(current({ step: "reverifying" }))[1]?.text).toMatch(/again, from scratch/);
  });

  it("keeps the original for a copy, and says a rename stays on its drive", () => {
    const copy = stepLines(current({ kind: "copy", cameraProtected: true }));
    expect(copy[2]).toEqual({
      text: "The original stays in E:\\2026\\Barcelona: the camera protected it",
      state: "next",
    });
    expect(
      stepLines(current({ kind: "rename", step: "renaming", to: "E:\\2026\\_rejected" })),
    ).toEqual([{ text: "Moving to E:\\2026\\_rejected, on the same drive", state: "current" }]);
  });

  it("acknowledges a pause and a stop at once, and says why it is paused", () => {
    expect(haltText({ kind: "running", halt: null })).toBeNull();
    expect(haltText({ kind: "running", halt: "pause" })).toMatch(/^Pausing after this file/);
    expect(haltText({ kind: "running", halt: "cancel" })).toMatch(/^Stopping after this file/);
    expect(STOP_LABEL).toBe("Stop after this file");
    const out = {
      kind: "paused",
      reason: "volumeLost",
      volume: "Photos drive (E:)",
      back: false,
    } as const;
    expect(pausedText(out)).toBe(
      "Photos drive (E:) is not connected. Connect it again: nothing is lost, and the apply carries on from where it stopped.",
    );
    expect(canContinue(out)).toBe(false);
    expect(canContinue({ ...out, back: true })).toBe(true);
    expect(pausedText({ ...out, back: true })).toMatch(/is back/);
    expect(
      pausedText({ kind: "paused", reason: "diskFull", volume: "Backup drive (D:)", back: true }),
    ).toMatch(/^Backup drive \(D:\) is full/);
    expect(pausedText({ kind: "paused", reason: "user", volume: null, back: true })).toMatch(
      /^Paused/,
    );
  });

  it("words the launch notice with or without rolling forward on its own (US-38)", () => {
    expect(interruptedText(true)).toMatch(/needs attention\. Culla is finishing it\.$/);
    expect(interruptedText(false)).toMatch(/needs attention\.$/);
  });

  it("announces progress at most every 10 photos or 5 s, and only when it changed", () => {
    const p = progress({ photosDone: 5 });
    expect(progressAnnouncement({ at: 0, done: 0 }, 1000, p)).toBeNull();
    expect(progressAnnouncement({ at: 0, done: 0 }, 5000, p)).toBe("5 of 138 photos done.");
    expect(progressAnnouncement({ at: 0, done: 0 }, 1000, progress({ photosDone: 10 }))).toBe(
      "10 of 138 photos done.",
    );
    expect(progressAnnouncement({ at: 0, done: 5 }, 9000, p)).toBeNull();
  });
});

describe("apply report (US-36, file-engine §9.6)", () => {
  const done = group("DONE", [file("done"), file("done", null, "A.NEF")]);
  const renamed = group("REN", [file("committed")], { kind: "rename", action: "reject" });
  const kept = group("CSK", [file("copiedSourceKept"), file("failed", "groupFailed", "A.NEF")]);
  const split = group("SPLIT", [file("committed"), file("pending", null, "A.NEF")], {
    kind: "rename",
    action: "reject",
  });
  const lost = group("LOST", [file("failed", "lost"), file("failed", "groupFailed", "A.NEF")], {
    kind: "rename",
    action: "reject",
  });
  const mismatch = group("MIS", [file("failed", "verifyMismatch")], { retryable: true });
  const locked = group("LOCK", [file("failed", "locked"), file("failed", "groupFailed", "A.NEF")], {
    retryable: true,
  });
  const unstarted = group("NOT", [file("pending")]);
  const all = report([done, renamed, kept, split, lost, mismatch, locked, unstarted], {
    end: "cancelled",
    held: [
      {
        photoId: "h",
        name: "HELD",
        folder: "E:\\2026",
        decision: "kept",
        reason: "changedSinceScan",
      },
    ],
  });

  it("classifies each photo by its files' journal states", () => {
    expect([done, renamed, kept, split, lost, mismatch, locked, unstarted].map(outcomeOf)).toEqual([
      "done",
      "done",
      "copiedSourceKept",
      "split",
      "alarm",
      "mismatch",
      "failed",
      "notStarted",
    ]);
  });

  it("puts alarms first, then mismatches, failures, both places, split, not started, done", () => {
    expect(reportSections(all).map((s) => [s.outcome, s.groups.map((g) => g.name)])).toEqual([
      ["alarm", ["LOST"]],
      ["mismatch", ["MIS"]],
      ["failed", ["LOCK"]],
      ["copiedSourceKept", ["CSK"]],
      ["split", ["SPLIT"]],
      ["notStarted", ["NOT"]],
      ["done", ["DONE", "REN"]],
    ]);
  });

  it("counts per outcome, held back included", () => {
    expect(summaryParts(all)).toEqual([
      "1 photo not where expected",
      "1 copy did not match",
      "1 photo not applied",
      "1 photo copied, source kept",
      "1 photo split",
      "1 photo not started",
      "2 photos done",
      "1 photo held back",
    ]);
    expect(reportActions(all)).toEqual({ retry: 2, finish: 1, remaining: 1 });
  });

  it("titles and leads the report by how it ended", () => {
    expect(reportTitle(all)).toBe("Apply stopped");
    expect(reportLead(all)).toBe(
      "Stopped after the current file. 2 of 8 photos done. Nothing was deleted.",
    );
    expect(reportTitle(report([done]))).toBe("Apply finished");
    expect(reportTitle(report([done, kept]))).toBe("Apply finished, with photos to check");
    expect(reportLead(report([done]))).toBe("1 of 1 photo done. Nothing was deleted.");
  });

  it("words each file, with the reason a group failed", () => {
    expect(groupReason(locked, "finished")).toBe("locked");
    expect(groupReason(kept, "cancelled")).toBe("stopped");
    expect(groupReason(done, "finished")).toBeNull();
    expect(fileText(file("copiedSourceKept"), kept, "cancelled")).toBe(
      "A.JPG: copied, source kept. Checked at D:\\Selects\\A.JPG, and the original is still in place",
    );
    expect(fileText(file("failed", "groupFailed", "A.NEF"), kept, "cancelled")).toBe(
      "A.NEF: the apply stopped before it. Still at E:\\2026\\A.NEF.",
    );
    expect(fileText(file("failed", "groupFailed", "A.NEF"), locked, "finished")).toMatch(
      /^A\.NEF: another file of this photo failed/,
    );
    expect(fileText(file("failed", "lost"), lost, "finished")).toBe(
      "A.JPG: it is in neither place Culla expected. Nothing was touched.",
    );
    expect(fileText(file("committed"), renamed, "finished")).toBe(
      "A.JPG: moved to D:\\Selects\\A.JPG",
    );
    expect(fileText(file("done"), done, "finished")).toBe(
      "A.JPG: moved to D:\\Selects\\A.JPG, checked",
    );
    expect(
      fileText(file("done"), group("P", [], { kind: "copy", cameraProtected: true }), "finished"),
    ).toBe("A.JPG: copied to D:\\Selects\\A.JPG, original kept (protected on the camera)");
    expect(fileText(file("pending"), unstarted, "cancelled")).toBe(
      "A.JPG: not started, still at E:\\2026\\A.JPG",
    );
  });
});
