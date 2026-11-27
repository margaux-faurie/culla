import type { DecisionEvent } from "../decisions/events";
import {
  type DemoPhotoSpec,
  demoGroup,
  demoPhoto,
  source2026,
  sourceAlps,
  sourceCard,
} from "./demo-photos";
import { type DemoGroup, type RouteOptions, rejectedRoot, routeOf } from "./demo-routes";
import {
  type DestinationMemory,
  parseMemory,
  proposedDestinationId,
  remember,
} from "./destinations";
import type {
  Backend,
  DestinationSummary,
  PhotoSummary,
  RouteExample,
  SessionInfo,
  SessionOptions,
  SourceDetails,
  SourceSummary,
} from "./types";

/** Where the demo keeps the destination memory between visits. Demo only, display data only. */
export const MEMORY_KEY = "culla-demo:destinations";

/** The subset of `Storage` the demo needs; absent in Node, where memory lasts one backend. */
export interface DemoStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

interface DemoSource {
  readonly summary: SourceSummary;
  readonly specs: readonly DemoPhotoSpec[];
  readonly rejectedFolderExists: boolean;
}

const SOURCES: readonly DemoSource[] = [
  {
    summary: {
      id: "src-alps",
      name: "Alps",
      folder: { path: "E:\\2025\\Alps", volume: "Photos drive (E:)" },
      photoCount: sourceAlps().length,
      status: { kind: "applied", on: "12 Nov" },
      connected: true,
    },
    specs: sourceAlps(),
    rejectedFolderExists: true,
  },
  {
    summary: {
      id: "src-card",
      name: "100NZ6_3",
      folder: { path: "F:\\DCIM\\100NZ6_3", volume: "SD card (F:)" },
      photoCount: sourceCard().length,
      status: { kind: "inProgress", decided: 3 },
      connected: false,
    },
    specs: sourceCard(),
    rejectedFolderExists: false,
  },
];

/** The folder "Choose a folder" opens in the demo, as if picked in the native dialog. */
const PICKED: DemoSource = {
  summary: {
    id: "src-2026",
    name: "2026",
    folder: { path: "E:\\2026", volume: "Photos drive (E:)" },
    photoCount: source2026().length,
    status: { kind: "new" },
    connected: true,
  },
  specs: source2026(),
  rejectedFolderExists: false,
};

const DESTINATIONS: readonly DestinationSummary[] = [
  {
    id: "dst-selects",
    folder: { path: "D:\\Photos\\Selects 2026", volume: "Backup drive (D:)" },
    available: true,
  },
  {
    id: "dst-archive",
    folder: { path: "G:\\Archive\\Alps", volume: "Archive drive (G:)" },
    available: false,
  },
];

/** Seeded so the demo shows both cases: Alps remembers an unplugged drive, others get the last one. */
const SEED: DestinationMemory = {
  bySource: { "src-alps": "dst-archive" },
  lastUsed: "dst-selects",
};

function defaultStorage(): DemoStorage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export interface DemoOptions {
  /** Destination memory to start from when storage holds none. */
  readonly seed?: DestinationMemory;
  /** Null keeps the memory in this backend only. Defaults to `localStorage` when there is one. */
  readonly storage?: DemoStorage | null;
}

/** Reads the stored memory; a storage that throws (private mode, quota, policy) counts as none. */
function readStored(storage: DemoStorage | null): string | null {
  try {
    return storage?.getItem(MEMORY_KEY) ?? null;
  } catch {
    return null;
  }
}

/** Writes the memory; when the storage refuses it, the memory lasts this backend only. */
function writeStored(storage: DemoStorage | null, memory: DestinationMemory): void {
  try {
    storage?.setItem(MEMORY_KEY, JSON.stringify(memory));
  } catch {
    // Kept in this backend's memory only, as with no storage at all.
  }
}

interface DemoEntry {
  readonly photo: PhotoSummary;
  readonly group: DemoGroup;
}

/** The route view's example: a photo in a subfolder with a NEF and a sidecar, if there is one. */
function exampleOf(entries: readonly DemoEntry[]): DemoEntry | null {
  const rich = (e: DemoEntry) => e.group.raw !== null && e.group.sidecars.length > 0;
  return (
    entries.find((e) => e.group.folder.length > 0 && rich(e)) ??
    entries.find(rich) ??
    entries[0] ??
    null
  );
}

/** Simulated backend for the web demo and for tests. There is no Rust side in the browser. */
export function createBackend(options: DemoOptions = {}): Backend {
  const storage = options.storage === undefined ? defaultStorage() : options.storage;
  const stored = readStored(storage);
  let memory = stored === null ? (options.seed ?? SEED) : parseMemory(stored);
  const known = new Map<string, DemoSource>(SOURCES.map((s) => [s.summary.id, s]));
  const events = new Map<string, DecisionEvent[]>();
  let sessions = 0;

  const sourceOf = (id: string): DemoSource => {
    const source = known.get(id);
    if (source === undefined) {
      throw new Error("unknown source");
    }
    return source;
  };

  const entriesOf = (source: DemoSource, includeSubfolders: boolean): DemoEntry[] =>
    source.specs
      .map((spec, n) => ({ spec, n }))
      .filter(({ spec }) => includeSubfolders || spec.folder.length === 0)
      .map(({ spec, n }) => ({
        photo: demoPhoto(`${source.summary.id}-${n + 1}`, spec, n),
        group: demoGroup(spec),
      }));

  const destinationOf = (id: string | null): DestinationSummary | undefined =>
    DESTINATIONS.find((d) => d.id === id);

  return {
    version: () => Promise.resolve("demo"),

    recentSources: () => Promise.resolve(SOURCES.map((s) => s.summary)),

    chooseSource: () => {
      known.set(PICKED.summary.id, PICKED);
      return Promise.resolve(PICKED.summary);
    },

    describeSource: async (sourceId, includeSubfolders): Promise<SourceDetails> => {
      const source = sourceOf(sourceId);
      const entries = source.summary.connected ? entriesOf(source, includeSubfolders) : [];
      const subfolders = [...new Set(entries.flatMap((e) => e.group.folder.slice(0, 1)))];
      return {
        sourceId,
        includeSubfolders,
        photoCount: source.summary.connected ? entries.length : source.summary.photoCount,
        subfolders,
        rejectedFolder: rejectedRoot(source.summary.folder.path),
        rejectedFolderExists: source.rejectedFolderExists,
      };
    },

    exampleRoute: async (query): Promise<RouteExample | null> => {
      const source = sourceOf(query.sourceId);
      if (!source.summary.connected) {
        return null;
      }
      const example = exampleOf(entriesOf(source, query.includeSubfolders));
      if (example === null) {
        return null;
      }
      const destination = destinationOf(query.destinationId);
      return {
        photo: example.photo,
        route: routeOf(example.group, {
          sourcePath: source.summary.folder.path,
          rawFollows: query.rawFollows,
          keepMode: query.keepMode,
          destinationPath: destination?.available === true ? destination.folder.path : null,
        }),
      };
    },

    proposeDestination: async (sourceId) => {
      const proposal = proposedDestinationId(memory, sourceId);
      const destination = destinationOf(proposal?.id ?? null);
      return proposal === null || destination === undefined
        ? null
        : { destination, reason: proposal.reason };
    },

    chooseDestination: () => Promise.resolve(DESTINATIONS[0] ?? null),

    startSession: async (options: SessionOptions): Promise<SessionInfo> => {
      const source = sourceOf(options.sourceId);
      const destination = destinationOf(options.destinationId);
      if (!source.summary.connected) {
        throw new Error("source not connected");
      }
      if (destination === undefined || !destination.available) {
        throw new Error("destination not available");
      }
      // Memory is replaced whole once the write was tried: a storage that refuses it leaves this
      // backend's memory updated and the stored one as it was, never half of each.
      const next = remember(memory, options.sourceId, options.destinationId);
      writeStored(storage, next);
      memory = next;
      sessions += 1;
      const id = `demo-session-${sessions}`;
      events.set(id, []);
      const routeOptions: RouteOptions = {
        sourcePath: source.summary.folder.path,
        rawFollows: options.rawFollows,
        keepMode: options.keepMode,
        destinationPath: destination.folder.path,
      };
      const entries = entriesOf(source, options.includeSubfolders);
      return {
        id,
        source: source.summary,
        destination,
        options,
        rejectedFolder: rejectedRoot(source.summary.folder.path),
        rejectedFolderExists: source.rejectedFolderExists,
        photos: entries.map((e) => ({ ...e.photo, route: routeOf(e.group, routeOptions) })),
        exampleId: exampleOf(entries)?.photo.id ?? null,
      };
    },

    recordEvent: async (sessionId, event) => {
      const log = events.get(sessionId);
      if (log === undefined) {
        throw new Error("unknown session");
      }
      log.push(event);
    },
  };
}
