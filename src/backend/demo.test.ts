import { describe, expect, it } from "vitest";
import { createBackend, type DemoStorage, MEMORY_KEY } from "./demo";
import { EMPTY_MEMORY } from "./destinations";
import { isNoKeepRoute } from "./errors";

const memoryStorage = (): DemoStorage & { data: Map<string, string> } => {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value);
    },
  };
};

describe("demo backend", () => {
  it("reports the demo version", async () => {
    await expect(createBackend({ storage: null }).version()).resolves.toBe("demo");
  });

  it("serves the full image for zoom by the photo's id, 24 MP, a lone NEF's embedded JPEG (US-39)", async () => {
    const backend = createBackend({ storage: null, scenario: null });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    const [first, , lone] = session.photos;
    const full = await backend.fullImage(first?.id ?? "");
    expect([full.width, full.height, full.embeddedPreview]).toEqual([6048, 4032, false]);
    expect(full.url === first?.previewUrl).toBe(false);
    expect(lone?.kind).toBe("NEF");
    expect((await backend.fullImage(lone?.id ?? "")).embeddedPreview).toBe(true);
    await expect(backend.fullImage("not-a-photo")).rejects.toThrow();
    const broken = createBackend({ storage: null, scenario: "full-image-unreadable" });
    await broken.chooseSource();
    const other = await broken.startSession({ sourceId, includeSubfolders: true });
    await expect(broken.fullImage(other.photos[0]?.id ?? "")).rejects.toThrow();
  });

  it("answers the EXIF overlay's strings by photo id, pending until the pass reaches it (US-40)", async () => {
    const backend = createBackend({ storage: null, scenario: null });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    const exif = await backend.photoExif(session.photos[0]?.id ?? "");
    expect(exif).toEqual({
      kind: "ready",
      camera: "NIKON Z 6_3",
      lens: "NIKKOR Z 24-70mm f/4 S",
      shutter: "1/250 s",
      aperture: "f/4",
      iso: "ISO 100",
      focalLength: "24 mm",
      exposureCompensation: "0 EV",
      captured: "14 Nov 2026, 09:00:00",
      capturedFromFile: false,
    });
    const jpegOnly = session.photos.find((p) => p.name === "MGF_7004");
    const fromFile = await backend.photoExif(jpegOnly?.id ?? "");
    expect(fromFile.kind === "ready" && [fromFile.lens, fromFile.capturedFromFile]).toEqual([
      null,
      true,
    ]);
    await expect(backend.photoExif("not-a-photo")).rejects.toThrow();
    const slow = createBackend({ storage: null, scenario: "exif-pending" });
    await slow.chooseSource();
    const other = await slow.startSession({ sourceId, includeSubfolders: true });
    const id = other.photos[1]?.id ?? "";
    expect((await slow.photoExif(id)).kind).toBe("pending");
    expect((await slow.photoExif(id)).kind).toBe("ready");
    const broken = createBackend({ storage: null, scenario: "exif-unreadable" });
    await broken.chooseSource();
    const third = await broken.startSession({ sourceId, includeSubfolders: true });
    await expect(broken.photoExif(third.photos[0]?.id ?? "")).rejects.toThrow();
  });

  it("lists recent folders with their status (US-44)", async () => {
    const sources = await createBackend({ storage: null }).recentSources();
    expect(sources.map((s) => [s.name, s.status.kind, s.connected])).toEqual([
      ["Alps", "applied", true],
      ["100NZ6_3", "inProgress", false],
    ]);
  });

  it("starts sorting at once with the defaults: subfolders as asked, RAW follows (SPEC §4.2)", async () => {
    const backend = createBackend({ storage: null });
    const source = await backend.chooseSource();
    const session = await backend.startSession({
      sourceId: source?.id ?? "",
      includeSubfolders: true,
    });
    expect(session.photos).toHaveLength(24);
    expect(session.options.includeSubfolders).toBe(true);
    expect(session.options.rawFollows).toBe(true);
    // Seeded memory: a destination used before, no move or copy chosen yet.
    expect(session.options.keepMode).toBeNull();
    expect(session.destination?.id).toBe("dst-selects");
    expect(session.sameAsLastTime).toBe(true);
    expect(session.rejectedFolder).toBe("E:\\2026\\_rejected");
    expect(session.rejectedFolderExists).toBe(false);
    const top = await backend.startSession({
      sourceId: source?.id ?? "",
      includeSubfolders: false,
    });
    expect(top.photos).toHaveLength(4);
  });

  it("gives every photo its route as display strings, before and after the keep route is set", async () => {
    const backend = createBackend({ storage: null });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    const example = session.photos.find((p) => p.name === "MGF_7273");
    expect(example?.name).toBe("MGF_7273");
    expect(example?.kind).toBe("JPEG + NEF");
    expect(example?.route.folder).toBe("E:\\2026\\Barcelona");
    expect(example?.route.going).toEqual({ name: "MGF_7273", extensions: ".JPG .NEF .xmp" });
    expect(example?.route.keptTo).toBe("moved or copied to D:\\Photos\\Selects 2026\\Barcelona");
    expect(example?.route.rejectedTo).toBe("moved to E:\\2026\\_rejected\\Barcelona");

    const moved = await backend.updateSession(session.id, { keepMode: "move" });
    expect(moved.photos[0]?.route.keptTo).toBe("moved to D:\\Photos\\Selects 2026");
    const camera = moved.photos.find((p) => p.cameraProtected);
    expect(camera?.route.keptTo).toBe("copied to D:\\Photos\\Selects 2026\\Barcelona");

    const copied = await backend.updateSession(session.id, { keepMode: "copy", rawFollows: false });
    const again = copied.photos.find((p) => p.name === "MGF_7273");
    expect(again?.route.keptTo).toBe("copied to D:\\Photos\\Selects 2026\\Barcelona");
    expect(again?.route.staying).toEqual({ name: "MGF_7273", extensions: ".NEF .xmp" });
    expect(copied.options.rawFollows).toBe(false);
  });

  it("has no destination on a first run, and no kept route until one is chosen", async () => {
    const backend = createBackend({ storage: null, seed: EMPTY_MEMORY });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    expect(session.destination).toBeNull();
    expect(session.sameAsLastTime).toBe(false);
    expect(session.options.keepMode).toBeNull();
    expect(session.photos[0]?.route.keptTo).toBeNull();
    const destination = await backend.chooseDestination();
    const chosen = await backend.updateSession(session.id, {
      destinationId: destination?.id ?? "",
    });
    expect(chosen.destination?.id).toBe("dst-selects");
    expect(chosen.sameAsLastTime).toBe(false);
    await expect(backend.updateSession(session.id, { destinationId: "nope" })).rejects.toThrow();
  });

  it("plans rejects without a keep route, and refuses kept photos until it is set", async () => {
    const backend = createBackend({ storage: null, seed: EMPTY_MEMORY, scenario: null });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    const decide = (seq: number, verdict: "keep" | "reject") =>
      backend.recordEvent(session.id, {
        seq,
        ts: seq,
        cause: "user",
        type: "decided",
        group_id: seq,
        verdict,
      });
    await decide(1, "reject");
    const rejects = await backend.reviewPlan(session.id, 1);
    expect(rejects.counts.rejected).toBe(1);
    expect(rejects.kept).toBeNull();
    expect(rejects.blockers).toEqual([]);
    await decide(2, "keep");
    const noRoute = async () => {
      try {
        await backend.reviewPlan(session.id, 2);
      } catch (error) {
        return isNoKeepRoute(error);
      }
      return false;
    };
    expect(await noRoute()).toBe(true);
    await backend.updateSession(session.id, { keepMode: "move" });
    expect(await noRoute()).toBe(true);
    await backend.updateSession(session.id, { destinationId: "dst-selects" });
    const both = await backend.reviewPlan(session.id, 2);
    expect(both.counts.kept).toBe(1);
    expect(both.kept?.to).toBe("D:\\Photos\\Selects 2026");
  });

  it("needs no keep route for kept photos held back as applied, as the core's plan (c)", async () => {
    const backend = createBackend({ storage: null, seed: EMPTY_MEMORY, scenario: "held" });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    // Group 2 is MGF_7002, applied by an earlier plan in this scenario.
    await backend.recordEvent(session.id, {
      seq: 1,
      ts: 1,
      cause: "user",
      type: "decided",
      group_id: 2,
      verdict: "keep",
    });
    const plan = await backend.reviewPlan(session.id, 1);
    expect(plan.kept).toBeNull();
    expect(plan.held.map((h) => [h.name, h.reason])).toEqual([["MGF_7002", "applied"]]);
  });

  it("opens a folder with no photos in the empty-folder scenario", async () => {
    const backend = createBackend({ storage: null, scenario: "empty-folder" });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    expect(session.photos).toHaveLength(0);
  });

  it("a draft made before the options changed cannot be confirmed", async () => {
    const backend = createBackend({ storage: null, scenario: null });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    await backend.updateSession(session.id, { keepMode: "move" });
    await backend.recordEvent(session.id, {
      seq: 1,
      ts: 1,
      cause: "user",
      type: "decided",
      group_id: 1,
      verdict: "keep",
    });
    const draft = await backend.reviewPlan(session.id, 1);
    await backend.updateSession(session.id, { keepMode: "copy" });
    await expect(backend.confirmPlan(draft.planId)).rejects.toThrow();
  });

  it("falls back to memory when the storage throws", async () => {
    let writes = 0;
    const storage: DemoStorage = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        writes += 1;
        throw new Error("quota");
      },
    };
    const backend = createBackend({ storage, seed: EMPTY_MEMORY });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({ sourceId, includeSubfolders: true });
    expect(session.destination).toBeNull();
    expect(writes).toBe(0);
    await backend.updateSession(session.id, { keepMode: "move", destinationId: "dst-selects" });
    expect(writes).toBe(1);
    // The memory of this backend is updated whole, though the write failed.
    const next = await backend.startSession({ sourceId, includeSubfolders: true });
    expect(next.destination?.id).toBe("dst-selects");
    expect(next.options.keepMode).toBe("move");
  });

  it("remembers move or copy and the destination for next time (US-21)", async () => {
    const storage = memoryStorage();
    const first = createBackend({ storage, seed: EMPTY_MEMORY });
    const sourceId = (await first.chooseSource())?.id ?? "";
    const session = await first.startSession({ sourceId, includeSubfolders: true });
    const destination = await first.chooseDestination();
    await first.updateSession(session.id, {
      keepMode: "copy",
      destinationId: destination?.id ?? "",
    });
    expect(storage.data.has(MEMORY_KEY)).toBe(true);

    // A later visit reads the stored memory: the same folder and a new one both get them.
    const later = createBackend({ storage });
    await later.chooseSource();
    const again = await later.startSession({ sourceId, includeSubfolders: true });
    expect(again.destination?.id).toBe(destination?.id);
    expect(again.sameAsLastTime).toBe(true);
    expect(again.options.keepMode).toBe("copy");
    const alps = await later.startSession({ sourceId: "src-alps", includeSubfolders: true });
    expect(alps.destination?.id).toBe(destination?.id);
    expect(alps.options.keepMode).toBe("copy");
  });

  it("starts with a remembered destination on a missing drive, marked unavailable, with no kept route", async () => {
    const backend = createBackend({ storage: null });
    const session = await backend.startSession({ sourceId: "src-alps", includeSubfolders: true });
    expect(session.destination?.id).toBe("dst-archive");
    expect(session.destination?.available).toBe(false);
    expect(session.photos[0]?.route.keptTo).toBeNull();
  });

  it("refuses a folder whose drive is not connected", async () => {
    const backend = createBackend({ storage: null });
    await expect(
      backend.startSession({ sourceId: "src-card", includeSubfolders: true }),
    ).rejects.toThrow();
  });

  it("records events per session", async () => {
    const backend = createBackend({ storage: null });
    const event = {
      seq: 1,
      ts: 0,
      cause: "user",
      type: "cleared",
      group_id: 1,
    } as const;
    await expect(backend.recordEvent("nope", event)).rejects.toThrow();
  });

  describe("resume (US-37)", () => {
    const keep = (seq: number, group: number) =>
      ({ seq, ts: seq, cause: "user", type: "decided", group_id: group, verdict: "keep" }) as const;

    it("resumes a folder with a session in progress after a reload, RAW follows as it was", async () => {
      const storage = memoryStorage();
      const first = createBackend({ storage });
      const sourceId = (await first.chooseSource())?.id ?? "";
      const session = await first.startSession({ sourceId, includeSubfolders: true });
      expect(session.resumed).toBe(false);
      expect(session.events).toEqual([]);
      await first.recordEvent(session.id, keep(1, 1));
      await first.recordEvent(session.id, keep(2, 3));
      await first.updateSession(session.id, { rawFollows: false });

      const again = createBackend({ storage });
      const recents = await again.recentSources();
      expect(recents.map((s) => [s.name, s.status])).toEqual([
        ["2026", { kind: "inProgress", decided: 2 }],
        ["Alps", { kind: "applied", on: "12 Nov" }],
        ["100NZ6_3", { kind: "inProgress", decided: 3 }],
      ]);
      expect((await again.chooseSource())?.status).toEqual({ kind: "inProgress", decided: 2 });
      const resumed = await again.startSession({ sourceId, includeSubfolders: true });
      expect(resumed.resumed).toBe(true);
      expect(resumed.events).toEqual([keep(1, 1), keep(2, 3)]);
      expect(resumed.options.rawFollows).toBe(false);
      // Its decisions are the backend's too: a plan sees them.
      const plan = await again
        .updateSession(resumed.id, { keepMode: "move" })
        .then(() => again.reviewPlan(resumed.id, 2));
      expect(plan.counts.kept).toBe(2);
    });

    it("starts over: decisions put aside, a new session with every photo undecided", async () => {
      const storage = memoryStorage();
      const first = createBackend({ storage });
      const sourceId = (await first.chooseSource())?.id ?? "";
      const session = await first.startSession({ sourceId, includeSubfolders: true });
      await first.recordEvent(session.id, keep(1, 1));
      const again = createBackend({ storage });
      const resumed = await again.startSession({ sourceId, includeSubfolders: true });
      const fresh = await again.startOver(resumed.id);
      expect(fresh.id === resumed.id).toBe(false);
      expect(fresh.resumed).toBe(false);
      expect(fresh.events).toEqual([]);
      expect(fresh.options.rawFollows).toBe(true);
      // The old session records nothing more, and a reload starts the folder new.
      await expect(again.recordEvent(resumed.id, keep(2, 2))).rejects.toThrow();
      const third = createBackend({ storage });
      expect((await third.recentSources()).map((s) => s.name)).toEqual(["Alps", "100NZ6_3"]);
      expect((await third.chooseSource())?.status).toEqual({ kind: "new" });
      expect((await third.startSession({ sourceId, includeSubfolders: true })).resumed).toBe(false);
    });

    /** Plans, confirms and applies the session's decisions up to `upto` on a simulated clock. */
    async function applyUpTo(
      backend: ReturnType<typeof createBackend>,
      clock: { t: number },
      sessionId: string,
      upto: number,
      stop = false,
    ) {
      await backend.updateSession(sessionId, { keepMode: "move", destinationId: "dst-selects" });
      const plan = await backend.reviewPlan(sessionId, upto);
      const confirmed = await backend.confirmPlan(plan.planId);
      const planId = confirmed.kind === "confirmed" ? confirmed.planId : "";
      await backend.startApply(planId);
      if (stop) await backend.stopApply(planId);
      let status = await backend.applyStatus(planId);
      for (let i = 0; i < 1000 && status.state.kind !== "ended"; i += 1) {
        clock.t += 100;
        status = await backend.applyStatus(planId);
      }
      expect(status.state.kind).toBe("ended");
      await backend.closeApply(planId);
      return status;
    }

    const options = (storage: DemoStorage, clock: { t: number }) => ({
      storage,
      wall: () => new Date(2026, 10, 28, 15),
      now: () => clock.t,
      seed: { ...EMPTY_MEMORY, keepMode: "move" as const },
      scenario: null,
    });

    it("a folder applied since its last decision shows as applied and starts fresh", async () => {
      const storage = memoryStorage();
      const clock = { t: 0 };
      const first = createBackend(options(storage, clock));
      const sourceId = (await first.chooseSource())?.id ?? "";
      const session = await first.startSession({ sourceId, includeSubfolders: true });
      await first.recordEvent(session.id, keep(1, 1));
      await applyUpTo(first, clock, session.id, 1);

      const again = createBackend({ storage });
      expect((await again.chooseSource())?.status).toEqual({ kind: "applied", on: "28 Nov" });
      const next = await again.startSession({ sourceId, includeSubfolders: true });
      expect(next.resumed).toBe(false);
      // The applied photo is held back as applied in the new session's plan (INV-04).
      await again.updateSession(next.id, { keepMode: "move" });
      await again.recordEvent(next.id, keep(1, 1));
      const replan = await again.reviewPlan(next.id, 1);
      expect(replan.held.map((h) => h.reason)).toEqual(["applied"]);
    });

    it("a stopped apply leaves the folder in progress: a reload resumes every decision", async () => {
      const storage = memoryStorage();
      const clock = { t: 0 };
      const first = createBackend(options(storage, clock));
      const sourceId = (await first.chooseSource())?.id ?? "";
      const session = await first.startSession({ sourceId, includeSubfolders: true });
      for (const [i, group] of [1, 2, 3].entries()) {
        await first.recordEvent(session.id, keep(i + 1, group));
      }
      const status = await applyUpTo(first, clock, session.id, 3, true);
      expect(status.state.kind === "ended" && status.state.end).toBe("cancelled");

      const again = createBackend({ storage });
      expect((await again.chooseSource())?.status).toEqual({ kind: "inProgress", decided: 3 });
      const resumed = await again.startSession({ sourceId, includeSubfolders: true });
      expect(resumed.resumed).toBe(true);
      expect(resumed.events).toHaveLength(3);
    });

    it("a closed plan that applied nothing gives the folder no applied day", async () => {
      const storage = memoryStorage();
      const clock = { t: 0 };
      // The destination fills up during the first copy: the run pauses with nothing applied.
      const first = createBackend({ ...options(storage, clock), scenario: "disk-full" });
      const sourceId = (await first.chooseSource())?.id ?? "";
      const session = await first.startSession({ sourceId, includeSubfolders: true });
      await first.recordEvent(session.id, keep(1, 1));
      await first.updateSession(session.id, { keepMode: "move", destinationId: "dst-selects" });
      const plan = await first.reviewPlan(session.id, 1);
      const confirmed = await first.confirmPlan(plan.planId);
      const planId = confirmed.kind === "confirmed" ? confirmed.planId : "";
      await first.startApply(planId);
      let status = await first.applyStatus(planId);
      for (let i = 0; i < 100 && status.state.kind !== "paused"; i += 1) {
        clock.t += 100;
        status = await first.applyStatus(planId);
      }
      expect(status.state.kind === "paused" && status.state.reason).toBe("diskFull");
      // From the pause, Stop ends the run at once.
      status = await first.stopApply(planId);
      for (let i = 0; i < 100 && status.state.kind !== "ended"; i += 1) {
        clock.t += 100;
        status = await first.applyStatus(planId);
      }
      const states = status.report?.groups.flatMap((g) => g.files.map((f) => f.state)) ?? [];
      expect(states.length > 0 && states.every((st) => st === "failed" || st === "pending")).toBe(
        true,
      );
      await first.closeApply(planId);

      const again = createBackend({ storage });
      const resumed = await again.startSession({ sourceId, includeSubfolders: true });
      await again.startOver(resumed.id);
      // Nothing was ever applied: the folder is new again, with no "Applied <day>".
      expect((await again.chooseSource())?.status).toEqual({ kind: "new" });
    });

    it("stores only what plans applied, never a scenario's seeded facts", async () => {
      const storage = memoryStorage();
      // `held` seeds MGF_7002 as applied by an earlier plan.
      const held = createBackend({ storage, scenario: "held" });
      const sourceId = (await held.chooseSource())?.id ?? "";
      const session = await held.startSession({ sourceId, includeSubfolders: true });
      await held.recordEvent(session.id, keep(1, 2));
      expect(storage.data.get("culla-demo:sessions")?.includes("MGF_7002")).toBe(false);

      const plain = createBackend({ storage, scenario: null });
      const next = await plain.startSession({ sourceId, includeSubfolders: true });
      await plain.updateSession(next.id, { keepMode: "move" });
      const plan = await plain.reviewPlan(next.id, 1);
      expect(plan.held).toEqual([]);
      expect(plan.counts.kept).toBe(1);
    });

    it("Start over keeps the folder's applied day; an archived session takes no more changes", async () => {
      const storage = memoryStorage();
      const clock = { t: 0 };
      const first = createBackend(options(storage, clock));
      const sourceId = (await first.chooseSource())?.id ?? "";
      const session = await first.startSession({ sourceId, includeSubfolders: true });
      await first.recordEvent(session.id, keep(1, 1));
      await applyUpTo(first, clock, session.id, 1);
      await first.recordEvent(session.id, keep(2, 2));

      const again = createBackend({ storage });
      expect((await again.chooseSource())?.status).toEqual({ kind: "inProgress", decided: 2 });
      const resumed = await again.startSession({ sourceId, includeSubfolders: true });
      const fresh = await again.startOver(resumed.id);
      expect((await again.chooseSource())?.status).toEqual({ kind: "applied", on: "28 Nov" });
      await expect(again.updateSession(resumed.id, { rawFollows: false })).rejects.toThrow();
      await expect(again.reviewPlan(resumed.id, 0)).rejects.toThrow();
      await expect(again.startOver(resumed.id)).rejects.toThrow();
      // The new session still holds the applied photo back.
      await again.updateSession(fresh.id, { keepMode: "move" });
      await again.recordEvent(fresh.id, keep(1, 1));
      const plan = await again.reviewPlan(fresh.id, 1);
      expect(plan.held.map((h) => h.reason)).toEqual(["applied"]);
    });
  });
});
