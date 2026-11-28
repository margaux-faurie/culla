import type { DecisionEvent } from "../decisions/events";
import { fold, stateOf } from "../decisions/log";
import {
  type DemoPhotoSpec,
  demoGroup,
  demoPhoto,
  source2026,
  sourceAlps,
  sourceCard,
} from "./demo-photos";
import { type DemoFacts, folded, type PlanSettings, reviewPlanOf } from "./demo-plan";
import { type DemoGroup, joinPath, type RouteOptions, rejectedRoot, routeOf } from "./demo-routes";
import { type Scenario, type ScenarioName, scenarioFromSearch, scenarioOf } from "./demo-scenarios";
import {
  type DestinationMemory,
  parseMemory,
  proposedDestinationId,
  remember,
} from "./destinations";
import type {
  Backend,
  ConfirmResult,
  DestinationSummary,
  OtherFile,
  PhotoSummary,
  ReviewPlan,
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

/** A file the scan does not handle (US-04), relative to the source root. */
interface DemoOther {
  readonly relative: readonly string[];
  readonly extension: string;
  readonly reason: OtherFile["reason"];
}

interface DemoSource {
  readonly summary: SourceSummary;
  readonly specs: readonly DemoPhotoSpec[];
  readonly rejectedFolderExists: boolean;
  readonly others: readonly DemoOther[];
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
    others: [],
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
    others: [],
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
  others: [
    { relative: ["Barcelona", "MGF_7290.MOV"], extension: ".MOV", reason: "unknownType" },
    { relative: ["Barcelona", "MGF_7291.MOV"], extension: ".MOV", reason: "unknownType" },
    { relative: ["notes.txt"], extension: ".txt", reason: "unknownType" },
  ],
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

/**
 * What already exists under each destination, relative to its root (the root itself exists): an
 * earlier `MGF_7271` in Selects 2026, so the demo shows a collision suffix (§7.3, US-15).
 */
const DESTINATION_CONTENT: Readonly<Record<string, readonly (readonly string[])[]>> = {
  "dst-selects": [["Barcelona"], ["Barcelona", "MGF_7271.JPG"]],
};

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

/** The scenario named in the page's query string (`?scenario=held`), demo only. */
function defaultScenario(): ScenarioName | null {
  try {
    return typeof location === "undefined" ? null : scenarioFromSearch(location.search);
  } catch {
    return null;
  }
}

export interface DemoOptions {
  /** Destination memory to start from when storage holds none. */
  readonly seed?: DestinationMemory;
  /** Null keeps the memory in this backend only. Defaults to `localStorage` when there is one. */
  readonly storage?: DemoStorage | null;
  /** The review and confirm scenario; defaults to `?scenario=` in the page's address. */
  readonly scenario?: ScenarioName | null;
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

/** A session as the demo keeps it: its events, the facts preflight sees, the latest draft. */
interface DemoSession {
  readonly id: string;
  readonly source: DemoSource;
  readonly destination: DestinationSummary;
  readonly options: SessionOptions;
  readonly entries: readonly DemoEntry[];
  readonly events: DecisionEvent[];
  readonly existing: Set<string>;
  facts: DemoFacts;
  latest: ReviewPlan | null;
  confirmChecked: boolean;
  confirmed: string | null;
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
  const sessions = new Map<string, DemoSession>();
  const scenario: Scenario = scenarioOf(
    options.scenario === undefined ? defaultScenario() : options.scenario,
  );
  let sessionCount = 0;
  let planCount = 0;

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

  const sessionOf = (id: string): DemoSession => {
    const session = sessions.get(id);
    if (session === undefined) {
      throw new Error("unknown session");
    }
    return session;
  };

  const sessionOfPlan = (planId: string): DemoSession => {
    for (const session of sessions.values()) {
      if (session.latest?.planId === planId) {
        return session;
      }
    }
    // Only the last draft of a session can be confirmed (design/file-engine.md §7.4).
    throw new Error("superseded plan");
  };

  /** Plans and preflights the session's decisions up to `uptoSeq`; the new draft is the latest. */
  const plan = (session: DemoSession, uptoSeq: number, previous?: ReviewPlan): ReviewPlan => {
    const { folded: decisions } = fold(session.events.filter((e) => e.seq <= uptoSeq));
    const sourcePath = session.source.summary.folder.path;
    const settings: PlanSettings = {
      sourcePath,
      sourceVolume: session.source.summary.folder.volume,
      destinationPath: session.destination.folder.path,
      destinationVolume: session.destination.folder.volume,
      keepMode: session.options.keepMode,
      rawFollows: session.options.rawFollows,
      sameVolume: session.source.summary.folder.volume === session.destination.folder.volume,
      destinationFat32: false,
    };
    planCount += 1;
    const draft = reviewPlanOf({
      planId: `demo-plan-${planCount}`,
      uptoSeq,
      groups: session.entries.map((e, i) => ({
        photoId: e.photo.id,
        group: e.group,
        // Group ids are display positions until the backend lists groups (SPEC §10, M3).
        state: stateOf(decisions, i + 1),
      })),
      settings,
      existing: session.existing,
      facts: session.facts,
      otherFiles: session.source.others
        .filter((o) => session.options.includeSubfolders || o.relative.length === 1)
        .map((o) => ({
          path: joinPath(sourcePath, o.relative),
          extension: o.extension,
          reason: o.reason,
        })),
      ...(previous === undefined ? {} : { previous }),
    });
    session.latest = draft;
    return draft;
  };

  /** Folded display paths of what exists under the destination and `_rejected` (§7.3). */
  const existingFor = (source: DemoSource, destination: DestinationSummary): Set<string> => {
    const root = destination.folder.path;
    const entries = [
      root,
      ...(DESTINATION_CONTENT[destination.id] ?? []).map((rel) => joinPath(root, rel)),
      ...(source.rejectedFolderExists ? [rejectedRoot(source.summary.folder.path)] : []),
    ];
    return new Set(entries.map(folded));
  };

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
      sessionCount += 1;
      const id = `demo-session-${sessionCount}`;
      const routeOptions: RouteOptions = {
        sourcePath: source.summary.folder.path,
        rawFollows: options.rawFollows,
        keepMode: options.keepMode,
        destinationPath: destination.folder.path,
      };
      const entries = entriesOf(source, options.includeSubfolders);
      sessions.set(id, {
        id,
        source,
        destination,
        options,
        entries,
        events: [],
        existing: existingFor(source, destination),
        facts: scenario.facts,
        latest: null,
        confirmChecked: false,
        confirmed: null,
      });
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
      sessionOf(sessionId).events.push(event);
    },

    reviewPlan: async (sessionId, uptoSeq) => {
      const session = sessionOf(sessionId);
      const saved = session.events.at(-1)?.seq ?? 0;
      if (saved < uptoSeq) {
        throw new Error("decisions not saved yet");
      }
      // A check after the first one sees what has finished meanwhile (previews, US-34 P16).
      if (session.latest !== null && scenario.clearsOnRecheck !== undefined) {
        session.facts = { ...session.facts, ...scenario.clearsOnRecheck };
      }
      return plan(session, uptoSeq);
    },

    chooseVolume: async (planId, volumeId) => {
      const session = sessionOfPlan(planId);
      const choices = session.facts.sourceChoices;
      if (choices === null || !choices.some((c) => c.id === volumeId)) {
        throw new Error("unknown volume");
      }
      session.facts = { ...session.facts, sourceChoices: null };
      return plan(session, session.latest?.uptoSeq ?? 0);
    },

    confirmPlan: async (planId): Promise<ConfirmResult> => {
      const session = sessionOfPlan(planId);
      const previous = session.latest as ReviewPlan;
      // What changes on disk between review and confirm, once (US-35).
      if (!session.confirmChecked) {
        session.confirmChecked = true;
        session.facts = { ...session.facts, ...scenario.atConfirm };
        for (const rel of scenario.takenAtConfirm ?? []) {
          session.existing.add(folded(joinPath(session.destination.folder.path, [rel])));
        }
      }
      // Preflight runs again at confirm (design/file-engine.md §7.4); the demo never touches a file.
      const again = plan(session, previous.uptoSeq, previous);
      if (again.blockers.length > 0) {
        return { kind: "blocked", plan: again };
      }
      const heldBefore = previous.held.map((h) => h.photoId).join(" ");
      const heldNow = again.held.map((h) => h.photoId).join(" ");
      if (again.replanned.length > 0 || heldBefore !== heldNow) {
        return { kind: "replanned", plan: again };
      }
      session.confirmed = again.planId;
      return { kind: "confirmed", planId: again.planId };
    },
  };
}
