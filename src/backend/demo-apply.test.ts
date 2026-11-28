import { describe, expect, it } from "vitest";
import { createBackend } from "./demo";
import { RENAME_MS, RETRY_MS, RETURN_MS } from "./demo-apply";
import type { ScenarioName } from "./demo-scenarios";
import type { ApplyStatus, Backend, ReportGroup } from "./types";

const WALL = new Date(2026, 10, 28, 9, 30, 0);

/** A demo backend on a clock the test moves by hand. */
function setup(scenario: ScenarioName | null = null) {
  const clock = { t: 0 };
  const backend = createBackend({
    storage: null,
    scenario,
    now: () => clock.t,
    wall: () => WALL,
  });
  return { backend, clock };
}

/** Starts a session on the demo folder, records the decisions (group id → verdict), confirms. */
async function confirmed(
  backend: Backend,
  decisions: readonly [number, "keep" | "reject"][],
  keepMode: "move" | "copy" = "move",
): Promise<string> {
  const source = await backend.chooseSource();
  const session = await backend.startSession({
    sourceId: source?.id ?? "",
    includeSubfolders: true,
    rawFollows: true,
    keepMode,
    destinationId: "dst-selects",
  });
  let seq = 0;
  for (const [group_id, verdict] of decisions) {
    seq += 1;
    await backend.recordEvent(session.id, {
      seq,
      ts: 0,
      cause: "user",
      type: "decided",
      group_id,
      verdict,
    });
  }
  const plan = await backend.reviewPlan(session.id, seq);
  const result = await backend.confirmPlan(plan.planId);
  if (result.kind !== "confirmed") throw new Error(`not confirmed: ${result.kind}`);
  return result.planId;
}

/** Polls until the status matches, moving the clock 20 ms at a time (at most 10 minutes). */
async function until(
  backend: Backend,
  clock: { t: number },
  planId: string,
  done: (s: ApplyStatus) => boolean,
): Promise<ApplyStatus> {
  for (let i = 0; i < 30_000; i += 1) {
    const status = await backend.applyStatus(planId);
    if (done(status)) return status;
    clock.t += 20;
  }
  throw new Error("timed out");
}

const ended = (s: ApplyStatus) => s.state.kind === "ended";
const states = (g: ReportGroup | undefined) => g?.files.map((f) => [f.state, f.reason]);
const group = (s: ApplyStatus, name: string) => s.report?.groups.find((g) => g.name === name);

describe("demo apply: a run over the confirmed plan (US-36, file-engine §9)", () => {
  it("applies rejects first, then keeps, through the journal states, and ends with a log", async () => {
    const { backend, clock } = setup();
    const planId = await confirmed(backend, [
      [1, "keep"],
      [2, "keep"],
      [4, "reject"],
    ]);
    const first = await backend.startApply(planId);
    expect(first.state).toEqual({ kind: "running", halt: null });
    expect(first.progress).toMatchObject({ photosDone: 0, photos: 3, bytesDone: 0, mode: "move" });
    expect(first.progress.secondsLeft).toBeGreaterThan(0);
    // Starting again answers the same apply.
    expect((await backend.startApply(planId)).runId).toBe(first.runId);

    clock.t += RENAME_MS / 2;
    const renaming = await backend.applyStatus(planId);
    expect(renaming.current).toMatchObject({
      photoName: "MGF_7004",
      kind: "rename",
      step: "renaming",
      action: "reject",
      to: "E:\\2026\\_rejected",
    });

    const seen = new Set<string>();
    const end = await until(backend, clock, planId, (s) => {
      if (s.current !== null) seen.add(`${s.current.kind} ${s.current.step}`);
      return ended(s);
    });
    expect([...seen].sort()).toEqual([
      "rename renaming",
      "xmove copying",
      "xmove removingSource",
      "xmove verifying",
    ]);
    expect(end.state).toEqual({ kind: "ended", end: "finished" });
    expect(end.progress).toMatchObject({ photosDone: 3, photos: 3, secondsLeft: 0 });
    expect(end.progress.bytesDone).toBe(end.progress.bytes);
    expect(end.report?.groups.map((g) => g.name)).toEqual(["MGF_7004", "MGF_7001", "MGF_7002"]);
    // A rename stays committed until the plan closes (§9.3 step 4).
    expect(states(group(end, "MGF_7004"))).toEqual([["committed", null]]);
    expect(states(group(end, "MGF_7002"))).toEqual([
      ["done", null],
      ["done", null],
      ["done", null],
    ]);
    const log = await backend.openLog(end.report?.runId ?? "");
    expect(log.kind).toBe("text");
    const text = log.kind === "text" ? log.text : "";
    expect(text).toContain("plan         ");
    expect(text).toContain("(apply, first run)");
    expect(text).toContain(
      'op 1  group 4  reject  rename  source "MGF_7004.JPG" → rejected "MGF_7004.JPG"  committed',
    );
    expect(text).toMatch(
      /op 2 {2}group 1 {2}keep {2}xmove {2}source "MGF_7001\.JPG" → destination "MGF_7001\.JPG" {2}done {2}blake3 [0-9a-f]{64}\n/,
    );
    expect(text).toContain("end          finished\n");

    await backend.closeApply(planId);
    const closed = await backend.applyStatus(planId);
    expect(states(group(closed, "MGF_7004"))).toEqual([["done", null]]);
  });

  it("acknowledges a pause at once and takes it after the current file", async () => {
    const { backend, clock } = setup("slow");
    const planId = await confirmed(backend, [[1, "keep"]]);
    await backend.startApply(planId);
    clock.t += 100;
    const asked = await backend.pauseApply(planId);
    expect(asked.state).toEqual({ kind: "running", halt: "pause" });
    expect(asked.current).toMatchObject({ fileName: "MGF_7001.JPG", step: "copying" });
    const paused = await until(backend, clock, planId, (s) => s.state.kind === "paused");
    expect(paused.state).toEqual({ kind: "paused", reason: "user", volume: null, back: true });
    // The JPEG's whole operation finished (copy, verify, commit); the NEF has not started.
    const status = await backend.applyStatus(planId);
    expect(status.current).toBeNull();
    const run1 = status.runId;
    const log1 = await backend.openLog(run1);
    expect(log1.kind === "text" && log1.text).toContain("end          paused (by the user)\n");
    expect(log1.kind === "text" && log1.text).toContain(
      'source "MGF_7001.JPG" → destination "MGF_7001.JPG"  committed',
    );
    expect(log1.kind === "text" && log1.text).toContain(
      'source "MGF_7001.NEF" → destination "MGF_7001.NEF"  pending',
    );

    const resumed = await backend.continueApply(planId);
    expect(resumed.state.kind).toBe("running");
    expect(resumed.runId).not.toBe(run1);
    const end = await until(backend, clock, planId, ended);
    expect(states(group(end, "MGF_7001"))).toEqual([
      ["done", null],
      ["done", null],
    ]);
    const log2 = await backend.openLog(end.runId);
    expect(log2.kind === "text" && log2.text).toContain("(apply, resume)");
  });

  it("a stop part-way through a cross-volume group leaves it copied, source kept; Finish moves completes it", async () => {
    const { backend, clock } = setup("slow");
    const planId = await confirmed(backend, [
      [1, "keep"],
      [2, "keep"],
    ]);
    await backend.startApply(planId);
    clock.t += 100;
    const asked = await backend.stopApply(planId);
    expect(asked.state).toEqual({ kind: "running", halt: "cancel" });
    const end = await until(backend, clock, planId, ended);
    expect(end.state).toEqual({ kind: "ended", end: "cancelled" });
    expect(states(group(end, "MGF_7001"))).toEqual([
      ["copiedSourceKept", null],
      ["failed", "groupFailed"],
    ]);
    // Never started: no file of MGF_7002 moved.
    expect(states(group(end, "MGF_7002"))).toEqual([
      ["pending", null],
      ["pending", null],
      ["pending", null],
    ]);
    expect(group(end, "MGF_7001")?.retryable).toBe(false);
    expect(end.progress.photosDone).toBe(1);

    const finishing = await backend.finishMoves(planId);
    expect(finishing.state.kind).toBe("running");
    clock.t += 20;
    expect((await backend.applyStatus(planId)).current?.step).toBe("reverifying");
    const after = await until(backend, clock, planId, ended);
    expect(states(group(after, "MGF_7001"))).toEqual([
      ["done", null],
      ["done", null],
    ]);
    // Finish moves runs only that group; the unstarted one is for "Apply remaining".
    expect(states(group(after, "MGF_7002"))?.every(([s]) => s === "pending")).toBe(true);
  });

  it("a stop part-way through a rename group can leave it split; Apply remaining plans again", async () => {
    const { backend, clock } = setup("slow");
    const planId = await confirmed(backend, [
      [1, "reject"],
      [4, "reject"],
    ]);
    await backend.startApply(planId);
    clock.t += 10;
    await backend.stopApply(planId);
    const end = await until(backend, clock, planId, ended);
    expect(states(group(end, "MGF_7001"))).toEqual([
      ["committed", null],
      ["pending", null],
    ]);
    expect(states(group(end, "MGF_7004"))).toEqual([["pending", null]]);
    const log = await backend.openLog(end.runId);
    expect(log.kind === "text" && log.text).toContain("end          cancelled\n");

    await backend.closeApply(planId);
    const sessionPlan = await backend.reviewPlan("demo-session-1", 2);
    // The split group is applied (a file reached committed, §5.4): held. The other is planned.
    expect(sessionPlan.held.map((h) => [h.name, h.reason])).toEqual([["MGF_7001", "applied"]]);
    expect(sessionPlan.counts.rejected).toBe(1);
  });

  it("a card pulled mid-copy pauses until it is back, then starts that copy again", async () => {
    const { backend, clock } = setup("card-pulled");
    const planId = await confirmed(backend, [[1, "keep"]]);
    await backend.startApply(planId);
    const paused = await until(backend, clock, planId, (s) => s.state.kind === "paused");
    expect(paused.state).toEqual({
      kind: "paused",
      reason: "volumeLost",
      volume: "Photos drive (E:)",
      back: false,
    });
    // Continue does nothing while the card is out.
    expect((await backend.continueApply(planId)).state.kind).toBe("paused");
    clock.t += RETURN_MS;
    const back = await backend.applyStatus(planId);
    expect(back.state).toMatchObject({ kind: "paused", back: true });
    await backend.continueApply(planId);
    const end = await until(backend, clock, planId, ended);
    expect(states(group(end, "MGF_7001"))).toEqual([
      ["done", null],
      ["done", null],
    ]);
    const log = await backend.openLog(end.runId);
    expect(log.kind === "text" && log.text).toContain("attempts 2");
    const first = await backend.openLog(`${planId}-run-1`);
    expect(first.kind === "text" && first.text).toContain("paused (a volume went away)");
  });

  it("a file lost while the card was out is an alarm, and its group is not retried", async () => {
    const { backend, clock } = setup("lost");
    const planId = await confirmed(backend, [[1, "reject"]]);
    await backend.startApply(planId);
    await until(backend, clock, planId, (s) => s.state.kind === "paused");
    clock.t += RETURN_MS;
    await backend.applyStatus(planId);
    await backend.continueApply(planId);
    const end = await until(backend, clock, planId, ended);
    expect(states(group(end, "MGF_7001"))).toEqual([
      ["failed", "lost"],
      ["failed", "groupFailed"],
    ]);
    expect(group(end, "MGF_7001")?.retryable).toBe(false);
    const log = await backend.openLog(end.runId);
    expect(log.kind === "text" && log.text).toContain(
      "alarms: a file is not where the journal says",
    );
  });

  it("a full destination sends the copy back to pending and pauses", async () => {
    const { backend, clock } = setup("disk-full");
    const planId = await confirmed(backend, [[1, "keep"]]);
    await backend.startApply(planId);
    const paused = await until(backend, clock, planId, (s) => s.state.kind === "paused");
    expect(paused.state).toEqual({
      kind: "paused",
      reason: "diskFull",
      volume: "Backup drive (D:)",
      back: true,
    });
    const log = await backend.openLog(paused.runId);
    expect(log.kind === "text" && log.text).toContain(
      'source "MGF_7001.JPG" → destination "MGF_7001.JPG"  pending',
    );
    await backend.continueApply(planId);
    const end = await until(backend, clock, planId, ended);
    expect(states(group(end, "MGF_7001"))?.every(([s]) => s === "done")).toBe(true);
  });

  it("a verify mismatch fails the group with its source kept; Retry failed runs it again", async () => {
    const { backend, clock } = setup("verify-mismatch");
    const planId = await confirmed(backend, [
      [1, "keep"],
      [4, "keep"],
    ]);
    await backend.startApply(planId);
    const end = await until(backend, clock, planId, ended);
    expect(end.state).toEqual({ kind: "ended", end: "finished" });
    expect(states(group(end, "MGF_7001"))).toEqual([
      ["failed", "verifyMismatch"],
      ["failed", "groupFailed"],
    ]);
    expect(group(end, "MGF_7001")?.retryable).toBe(true);
    expect(states(group(end, "MGF_7004"))).toEqual([["done", null]]);
    await backend.retryFailed(planId);
    const again = await until(backend, clock, planId, ended);
    expect(states(group(again, "MGF_7001"))?.every(([s]) => s === "done")).toBe(true);
  });

  it("a file another app holds is retried with backoff, then failed as locked", async () => {
    const { backend, clock } = setup("locked-file");
    const planId = await confirmed(backend, [[1, "keep"]]);
    await backend.startApply(planId);
    clock.t += 20;
    expect((await backend.applyStatus(planId)).current?.step).toBe("retrying");
    clock.t += RETRY_MS;
    const end = await until(backend, clock, planId, ended);
    expect(states(group(end, "MGF_7001"))).toEqual([
      ["failed", "locked"],
      ["failed", "groupFailed"],
    ]);
    await backend.retryFailed(planId);
    const again = await until(backend, clock, planId, ended);
    expect(states(group(again, "MGF_7001"))?.every(([s]) => s === "done")).toBe(true);
  });

  it("copies by copy when keep is copy, and the source of a copy is kept", async () => {
    const { backend, clock } = setup();
    const planId = await confirmed(backend, [[1, "keep"]], "copy");
    const first = await backend.startApply(planId);
    expect(first.progress.mode).toBe("copy");
    const end = await until(backend, clock, planId, ended);
    expect(group(end, "MGF_7001")?.kind).toBe("copy");
    expect(states(group(end, "MGF_7001"))?.every(([s]) => s === "done")).toBe(true);
  });

  it("refuses an apply for a plan that is not confirmed, and a close before the run ends", async () => {
    const { backend, clock } = setup("slow");
    await expect(backend.startApply("demo-plan-99")).rejects.toThrow();
    const planId = await confirmed(backend, [[1, "keep"]]);
    await backend.startApply(planId);
    clock.t += 10;
    await expect(backend.closeApply(planId)).rejects.toThrow();
    await expect(backend.openLog("nope")).rejects.toThrow();
  });
});

describe("demo apply: an apply interrupted at launch (US-38)", () => {
  it("finds it, rolls it forward from the journal as a resume, and closes it", async () => {
    const { backend, clock } = setup("interrupted");
    const found = await backend.interruptedApply();
    expect(found).not.toBeNull();
    const planId = found?.planId ?? "";
    const before = await backend.applyStatus(planId);
    expect(before.state).toEqual({ kind: "interrupted", missing: null });
    // The crashed run wrote no log.
    await expect(backend.openLog(before.runId)).rejects.toThrow();
    const started = await backend.startApply(planId);
    expect(started.state.kind).toBe("running");
    const end = await until(backend, clock, planId, ended);
    expect(
      end.report?.groups.every((g) =>
        g.files.every((f) => f.state === "done" || f.state === "committed"),
      ),
    ).toBe(true);
    const log = await backend.openLog(end.runId);
    expect(log.kind === "text" && log.text).toContain("(apply, resume)");
    // The NEF cut mid-copy was copied again.
    expect(log.kind === "text" && log.text).toContain("attempts 2");
    await backend.closeApply(planId);
    expect(await backend.interruptedApply()).toBeNull();
  });

  it("waits for a card that is out and touches nothing meanwhile", async () => {
    const { backend, clock } = setup("interrupted-card-out");
    const found = await backend.interruptedApply();
    const planId = found?.planId ?? "";
    const waiting = await backend.startApply(planId);
    expect(waiting.state).toEqual({ kind: "interrupted", missing: "Photos drive (E:)" });
    clock.t += RETURN_MS;
    expect((await backend.applyStatus(planId)).state).toEqual({
      kind: "interrupted",
      missing: null,
    });
    expect((await backend.startApply(planId)).state.kind).toBe("running");
  });

  it("finds nothing without the scenario", async () => {
    expect(await setup().backend.interruptedApply()).toBeNull();
  });
});
