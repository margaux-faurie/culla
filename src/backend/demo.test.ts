import { describe, expect, it } from "vitest";
import { createBackend, type DemoStorage, MEMORY_KEY } from "./demo";
import { EMPTY_MEMORY } from "./destinations";

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

  it("lists recent folders with their status (US-44)", async () => {
    const sources = await createBackend({ storage: null }).recentSources();
    expect(sources.map((s) => [s.name, s.status.kind, s.connected])).toEqual([
      ["Alps", "applied", true],
      ["100NZ6_3", "inProgress", false],
    ]);
  });

  it("gives opaque ids and display data only", async () => {
    const backend = createBackend({ storage: null });
    const source = await backend.chooseSource();
    expect(source).not.toBeNull();
    const details = await backend.describeSource(source?.id ?? "", true);
    expect(details.photoCount).toBe(24);
    expect(details.subfolders).toEqual(["Barcelona", "Lyon"]);
    expect(details.rejectedFolder).toBe("E:\\2026\\_rejected");
    expect(details.rejectedFolderExists).toBe(false);
    const top = await backend.describeSource(source?.id ?? "", false);
    expect(top.photoCount).toBe(4);
  });

  it("computes the example route as display strings, for the options as set", async () => {
    const backend = createBackend({ storage: null });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const query = {
      sourceId,
      includeSubfolders: true,
      rawFollows: true,
      keepMode: null,
      destinationId: null,
    };
    const none = await backend.exampleRoute(query);
    expect(none?.photo.name).toBe("MGF_7273");
    expect(none?.photo.kind).toBe("JPEG + NEF");
    expect(none?.route.folder).toBe("E:\\2026\\Barcelona");
    expect(none?.route.going).toEqual({ name: "MGF_7273", extensions: ".JPG .NEF .xmp" });
    expect(none?.route.keptTo).toBeNull();
    expect(none?.route.rejectedTo).toBe("moved to E:\\2026\\_rejected\\Barcelona");
    const chosen = await backend.exampleRoute({
      ...query,
      rawFollows: false,
      keepMode: "copy",
      destinationId: "dst-selects",
    });
    expect(chosen?.route.keptTo).toBe("copied to D:\\Photos\\Selects 2026\\Barcelona");
    expect(chosen?.route.staying).toEqual({ name: "MGF_7273", extensions: ".NEF .xmp" });
    // A destination on a missing drive is not a place photos can go.
    const missing = await backend.exampleRoute({ ...query, destinationId: "dst-archive" });
    expect(missing?.route.keptTo).toBeNull();
    const top = await backend.exampleRoute({ ...query, includeSubfolders: false });
    expect(top?.route.folder).toBe("E:\\2026");
  });

  it("starts a session whose photos carry their routes", async () => {
    const backend = createBackend({ storage: null });
    const sourceId = (await backend.chooseSource())?.id ?? "";
    const session = await backend.startSession({
      sourceId,
      includeSubfolders: true,
      rawFollows: true,
      keepMode: "move",
      destinationId: "dst-selects",
    });
    const first = session.photos[0];
    expect(first?.route.keptTo).toBe("moved to D:\\Photos\\Selects 2026");
    expect(session.rejectedFolder).toBe("E:\\2026\\_rejected");
    const example = session.photos.find((p) => p.id === session.exampleId);
    expect(example?.name).toBe("MGF_7273");
    const camera = session.photos.find((p) => p.cameraProtected);
    expect(camera?.route.keptTo).toBe("copied to D:\\Photos\\Selects 2026\\Barcelona");
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
    expect(await backend.proposeDestination(sourceId)).toBeNull();
    const session = await backend.startSession({
      sourceId,
      includeSubfolders: true,
      rawFollows: true,
      keepMode: "move",
      destinationId: "dst-selects",
    });
    expect(session.photos).toHaveLength(24);
    expect(writes).toBe(1);
    // The memory of this backend is updated whole, though the write failed.
    const proposal = await backend.proposeDestination(sourceId);
    expect(proposal?.reason).toBe("thisSource");
    expect(proposal?.destination.id).toBe("dst-selects");
  });

  it("proposes, restores and remembers destinations (US-21)", async () => {
    const storage = memoryStorage();
    const first = createBackend({ storage, seed: EMPTY_MEMORY });
    const picked = await first.chooseSource();
    const sourceId = picked?.id ?? "";
    expect(await first.proposeDestination(sourceId)).toBeNull();
    const destination = await first.chooseDestination();
    const session = await first.startSession({
      sourceId,
      includeSubfolders: true,
      rawFollows: true,
      keepMode: "copy",
      destinationId: destination?.id ?? "",
    });
    expect(session.photos).toHaveLength(24);
    expect(storage.data.has(MEMORY_KEY)).toBe(true);

    // A later visit reads the stored memory.
    const later = createBackend({ storage });
    await later.chooseSource();
    const proposal = await later.proposeDestination(sourceId);
    expect(proposal?.reason).toBe("thisSource");
    expect(proposal?.destination.id).toBe(destination?.id);
    // A new folder gets the last destination used anywhere.
    expect((await later.proposeDestination("src-card"))?.reason).toBe("lastUsed");
  });

  it("marks a remembered destination on a missing drive unavailable and refuses it", async () => {
    const backend = createBackend({ storage: null });
    const proposal = await backend.proposeDestination("src-alps");
    expect(proposal?.reason).toBe("thisSource");
    expect(proposal?.destination.available).toBe(false);
    await expect(
      backend.startSession({
        sourceId: "src-alps",
        includeSubfolders: true,
        rawFollows: true,
        keepMode: "move",
        destinationId: proposal?.destination.id ?? "",
      }),
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
});
