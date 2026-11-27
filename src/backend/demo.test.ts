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
    expect(details.example?.folder).toEqual(["Barcelona"]);
    expect(details.rejectedFolderExists).toBe(false);
    const top = await backend.describeSource(source?.id ?? "", false);
    expect(top.photoCount).toBe(4);
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
      body: { type: "cleared", groupId: "g" },
    } as const;
    await expect(backend.recordEvent("nope", event)).rejects.toThrow();
  });
});
