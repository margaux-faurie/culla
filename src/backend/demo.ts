import type { DecisionEvent } from "../decisions/events";
import { fold, stateOf } from "../decisions/log";
import {
  advance,
  appliedNames,
  stateOf as applyStateOf,
  closeSim,
  crash,
  createSim,
  currentOf,
  finishMoves,
  logOf,
  progressOf,
  reportOf,
  requestHalt,
  resume,
  retryFailed,
  runIdOf,
  type Sim,
} from "./demo-apply";
import {
  type DemoPhotoSpec,
  demoFullImage,
  demoGroup,
  demoPhoto,
  source2026,
  sourceAlps,
  sourceCard,
} from "./demo-photos";
import {
  DEFAULT_READ_RATE,
  type DemoFacts,
  folded,
  keptNeedingRoute,
  type PlanGroup,
  type PlanSettings,
  reviewPlanOf,
} from "./demo-plan";
import { type DemoGroup, joinPath, type RouteOptions, rejectedRoot, routeOf } from "./demo-routes";
import { type Scenario, type ScenarioName, scenarioFromSearch, scenarioOf } from "./demo-scenarios";
import {
  appliedDay,
  parseSessions,
  renderSessions,
  resumable,
  SESSIONS_KEY,
  type StoredSession,
  storedStatus,
} from "./demo-sessions";
import {
  type DestinationMemory,
  parseMemory,
  proposedDestinationId,
  remember,
  rememberKeepMode,
} from "./destinations";
import { NoKeepRouteError } from "./errors";
import type {
  ApplyStatus,
  Backend,
  ConfirmResult,
  DestinationSummary,
  FullImage,
  InterruptedApply,
  OtherFile,
  PhotoSummary,
  ReviewPlan,
  SessionChange,
  SessionInfo,
  SessionOptions,
  SourceSummary,
  StartOptions,
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

/** What "Choose a folder" opens in the `empty-folder` scenario: a folder with no photos. */
const EMPTY: DemoSource = {
  summary: {
    id: "src-empty",
    name: "Scans",
    folder: { path: "E:\\Scans", volume: "Photos drive (E:)" },
    photoCount: 0,
    status: { kind: "new" },
    connected: true,
  },
  specs: [],
  rejectedFolderExists: false,
  others: [],
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

/**
 * Seeded so the demo shows both cases: Alps remembers an unplugged drive, others get the last one.
 * No move or copy is remembered, so the first review asks for it, as on a first run.
 */
const SEED: DestinationMemory = {
  bySource: { "src-alps": "dst-archive" },
  lastUsed: "dst-selects",
  keepMode: null,
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
  /** Milliseconds, for the simulated apply; defaults to `performance.now()`. Tests pass their own. */
  readonly now?: () => number;
  /** The wall clock for the apply log's times. */
  readonly wall?: () => Date;
}

/** Reads a stored value; a storage that throws (private mode, quota, policy) counts as none. */
function readStored(storage: DemoStorage | null, key: string): string | null {
  try {
    return storage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

/** Writes a value; when the storage refuses it, the value lasts this backend only. */
function writeStored(storage: DemoStorage | null, key: string, value: string): void {
  try {
    storage?.setItem(key, value);
  } catch {
    // Kept in this backend's memory only, as with no storage at all.
  }
}

interface DemoEntry {
  readonly photo: PhotoSummary;
  readonly group: DemoGroup;
  /** The full image zoom asks for (US-39). */
  readonly full: () => FullImage;
}

/**
 * A session as the demo keeps it: its options (changed from the review screen), its events, the
 * facts preflight sees, the latest draft.
 */
interface DemoSession {
  readonly id: string;
  readonly source: DemoSource;
  destination: DestinationSummary | null;
  sameAsLastTime: boolean;
  options: SessionOptions;
  readonly entries: readonly DemoEntry[];
  readonly events: DecisionEvent[];
  existing: Set<string>;
  facts: DemoFacts;
  latest: ReviewPlan | null;
  confirmChecked: boolean;
  confirmed: string | null;
  /** It existed before this start (US-37). */
  readonly resumed: boolean;
  /** "Start over" archived it: it records nothing more. */
  archived: boolean;
  /** The day its last plan was applied, until a new decision. */
  appliedOn: string | null;
}

/** A plan being applied: its simulated run and the time it was last advanced. */
interface DemoApply {
  readonly session: DemoSession;
  readonly sim: Sim;
  last: number;
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
  const storedMemory = readStored(storage, MEMORY_KEY);
  let memory = storedMemory === null ? (options.seed ?? SEED) : parseMemory(storedMemory);
  // Sessions in progress, kept between visits so a reload can resume them (US-37).
  const stored = parseSessions(readStored(storage, SESSIONS_KEY));
  const known = new Map<string, DemoSource>(SOURCES.map((s) => [s.summary.id, s]));
  // The folder picked on an earlier visit is a recent folder now, so it can be opened from there.
  if (stored.has(PICKED.summary.id)) known.set(PICKED.summary.id, PICKED);

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
        full: () => demoFullImage(spec, n),
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

  /**
   * Once a plan is confirmed its groups are locked (design/file-engine.md §5.4): the session builds
   * no new draft and confirms nothing more.
   */
  const unlocked = (session: DemoSession): DemoSession => {
    if (session.confirmed !== null) {
      throw new Error("plan already confirmed");
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
    const groups: PlanGroup[] = session.entries.map((e, i) => ({
      photoId: e.photo.id,
      group: e.group,
      // Group ids are display positions until the backend lists groups (SPEC §10, M3).
      state: stateOf(decisions, i + 1),
    }));
    const route = keepRouteOf(session);
    // As the core's plan (SPEC §10, plan() (c)): kept photos that would get operations need move
    // or copy and an available destination; rejects alone, or kept photos held back, need neither.
    if (route === null && keptNeedingRoute(groups, session.facts) > 0) {
      throw new NoKeepRouteError();
    }
    const settings: PlanSettings = {
      sourcePath,
      sourceVolume: session.source.summary.folder.volume,
      destinationPath: route?.destination.folder.path ?? "",
      destinationVolume: route?.destination.folder.volume ?? "",
      keepMode: route?.keepMode ?? "move",
      rawFollows: session.options.rawFollows,
      sameVolume: session.source.summary.folder.volume === route?.destination.folder.volume,
      destinationFat32: false,
    };
    planCount += 1;
    const draft = reviewPlanOf({
      planId: `demo-plan-${planCount}`,
      uptoSeq,
      groups,
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
  const existingFor = (source: DemoSource, destination: DestinationSummary | null): Set<string> => {
    const root = destination?.folder.path;
    const entries = [
      ...(destination === null || root === undefined
        ? []
        : [root, ...(DESTINATION_CONTENT[destination.id] ?? []).map((rel) => joinPath(root, rel))]),
      ...(source.rejectedFolderExists ? [rejectedRoot(source.summary.folder.path)] : []),
    ];
    return new Set(entries.map(folded));
  };

  /** Move or copy and an available destination, or null while either is missing. */
  const keepRouteOf = (
    session: DemoSession,
  ): { keepMode: "move" | "copy"; destination: DestinationSummary } | null => {
    const { keepMode } = session.options;
    const { destination } = session;
    return keepMode === null || destination === null || !destination.available
      ? null
      : { keepMode, destination };
  };

  /** The session as the UI sees it, every route computed under its current options. */
  const infoOf = (session: DemoSession): SessionInfo => {
    const { source, destination, options, entries } = session;
    const routeOptions: RouteOptions = {
      sourcePath: source.summary.folder.path,
      rawFollows: options.rawFollows,
      keepMode: options.keepMode,
      destinationPath: destination?.available === true ? destination.folder.path : null,
    };
    return {
      id: session.id,
      source: source.summary,
      destination,
      sameAsLastTime: session.sameAsLastTime,
      options,
      rejectedFolder: rejectedRoot(source.summary.folder.path),
      rejectedFolderExists: source.rejectedFolderExists,
      photos: entries.map((e) => ({ ...e.photo, route: routeOf(e.group, routeOptions) })),
      exampleId: exampleOf(entries)?.photo.id ?? null,
      resumed: session.resumed,
      events: [...session.events],
    };
  };

  const openSession = (
    source: DemoSource,
    destination: DestinationSummary | null,
    sameAsLastTime: boolean,
    options: SessionOptions,
    from?: StoredSession,
  ): SessionInfo => {
    sessionCount += 1;
    const id = `demo-session-${sessionCount}`;
    const session: DemoSession = {
      id,
      source,
      destination,
      sameAsLastTime,
      options,
      entries: entriesOf(source, options.includeSubfolders),
      events: [...(from?.events ?? [])],
      existing: existingFor(source, destination),
      facts: scenario.facts,
      latest: null,
      confirmChecked: false,
      confirmed: null,
      resumed: from !== undefined,
      archived: false,
      appliedOn: null,
    };
    sessions.set(id, session);
    return infoOf(session);
  };

  /**
   * Remembers move or copy and the destination for next time. Memory is replaced whole once the
   * write was tried: a storage that refuses it leaves this backend's memory updated and the stored
   * one as it was, never half of each.
   */
  const rememberRoute = (session: DemoSession) => {
    let next = memory;
    if (session.options.keepMode !== null) next = rememberKeepMode(next, session.options.keepMode);
    if (session.destination?.available === true) {
      next = remember(next, session.source.summary.id, session.destination.id);
    }
    if (next === memory) return;
    writeStored(storage, MEMORY_KEY, JSON.stringify(next));
    memory = next;
  };

  /** The source as the start screen lists it, its status from its stored session. */
  const summaryOf = (source: DemoSource): SourceSummary => {
    const status = storedStatus(stored.get(source.summary.id));
    return status === null ? source.summary : { ...source.summary, status };
  };

  /** Keeps a folder's session for the next visit, or forgets it with `null`. */
  const persist = (sourceId: string, record: StoredSession | null) => {
    if (record === null) {
      stored.delete(sourceId);
    } else {
      stored.set(sourceId, record);
    }
    writeStored(storage, SESSIONS_KEY, renderSessions(stored));
  };
  const recordOf = (session: DemoSession): StoredSession => ({
    events: [...session.events],
    rawFollows: session.options.rawFollows,
    applied: [...session.facts.applied],
    appliedOn: session.appliedOn,
  });

  /** A new session with the defaults (SPEC §4.2); a resumed one also gets its stored session. */
  const begin = (source: DemoSource, includeSubfolders: boolean, from?: StoredSession) => {
    const sourceId = source.summary.id;
    const destination = destinationOf(proposedDestinationId(memory, sourceId)) ?? null;
    const info = openSession(
      source,
      destination,
      destination !== null,
      {
        sourceId,
        includeSubfolders,
        rawFollows: from?.rawFollows ?? true,
        keepMode: memory.keepMode,
        destinationId: destination?.id ?? null,
      },
      from,
    );
    // Photos applied by an earlier plan stay held as applied, in a fresh session too (INV-04).
    const applied = stored.get(sourceId)?.applied ?? [];
    if (applied.length > 0) {
      const session = sessionOf(info.id);
      session.facts = {
        ...session.facts,
        applied: new Set([...session.facts.applied, ...applied]),
      };
    }
    return info;
  };

  // --- The simulated apply (US-36, US-38). The demo never touches a file (SPEC §5.5). ---
  const now =
    options.now ?? (() => (typeof performance === "undefined" ? Date.now() : performance.now()));
  const wall = options.wall ?? (() => new Date());
  const applies = new Map<string, DemoApply>();

  /** The apply of a plan, its simulated time brought up to now. */
  const applyOf = (planId: string): DemoApply => {
    const apply = applies.get(planId);
    if (apply === undefined) {
      throw new Error("no apply for this plan");
    }
    const t = now();
    advance(apply.sim, t - apply.last);
    apply.last = t;
    return apply;
  };

  const statusOf = ({ sim }: DemoApply): ApplyStatus => ({
    planId: sim.cfg.planId,
    runId: runIdOf(sim),
    state: applyStateOf(sim),
    progress: progressOf(sim),
    current: currentOf(sim),
    report: reportOf(sim),
  });

  const beginApply = (session: DemoSession, confirmed: ReviewPlan): DemoApply => {
    const summary = session.source.summary;
    const groupIds = new Map(session.entries.map((e, i) => [e.photo.id, i + 1]));
    const sim = createSim(
      confirmed,
      {
        planId: confirmed.planId,
        planNumber: Number(confirmed.planId.replace(/\D+/g, "")) || 1,
        sourcePath: summary.folder.path,
        rejectedPath: rejectedRoot(summary.folder.path),
        destinationPath: session.destination?.folder.path ?? "",
        sourceVolume: summary.folder.volume,
        destinationVolume: session.destination?.folder.volume ?? "",
        rate: DEFAULT_READ_RATE,
        scenario: scenario.apply ?? {},
        locked: session.facts.locked,
        held: confirmed.held,
        wall,
        version: "demo",
      },
      (photoId) => groupIds.get(photoId) ?? 0,
    );
    const apply = { session, sim, last: now() };
    applies.set(confirmed.planId, apply);
    return apply;
  };

  /** The apply found interrupted at launch, made once for the `interrupted` scenarios. */
  let interrupted: { info: SessionInfo; planId: string } | null = null;
  const makeInterrupted = (cardOut: boolean): { info: SessionInfo; planId: string } => {
    known.set(PICKED.summary.id, PICKED);
    const destination = DESTINATIONS[0] as DestinationSummary;
    const info = openSession(PICKED, destination, true, {
      sourceId: PICKED.summary.id,
      includeSubfolders: true,
      rawFollows: true,
      keepMode: "move",
      destinationId: destination.id,
    });
    const session = sessionOf(info.id);
    const verdicts = ["keep", "keep", "keep", "reject", "reject"] as const;
    for (const [i, verdict] of verdicts.entries()) {
      session.events.push({
        seq: i + 1,
        ts: 0,
        cause: "user",
        type: "decided",
        group_id: i + 1,
        verdict,
      });
    }
    const draft = plan(session, verdicts.length);
    session.confirmChecked = true;
    session.confirmed = draft.planId;
    const { sim } = beginApply(session, draft);
    // Cut part-way through the first kept photo: its JPEG committed, its NEF being copied.
    for (let i = 0; i < 10_000; i += 1) {
      const midGroup =
        sim.files.some((f) => f.kind === "xmove" && f.state === "committed") &&
        sim.work?.step === "copying";
      if (midGroup) break;
      advance(sim, 20);
    }
    crash(sim, cardOut ? PICKED.summary.folder.volume : null);
    return { info, planId: draft.planId };
  };

  return {
    version: () => Promise.resolve("demo"),

    // The folder the dialog picks joins the list, first, once it has a session (US-44).
    recentSources: () =>
      Promise.resolve(
        [...(stored.has(PICKED.summary.id) ? [PICKED] : []), ...SOURCES].map(summaryOf),
      ),

    chooseSource: () => {
      const picked = scenario.emptyFolder === true ? EMPTY : PICKED;
      known.set(picked.summary.id, picked);
      return Promise.resolve(summaryOf(picked));
    },

    startSession: async ({ sourceId, includeSubfolders }: StartOptions): Promise<SessionInfo> => {
      const source = sourceOf(sourceId);
      if (!source.summary.connected) {
        throw new Error("source not connected");
      }
      // Nothing is asked at the start (decided by Margaux 28 Nov 2026): RAW follows JPEG, move or
      // copy as last chosen, the destination remembered for this folder or else the last one used.
      // A session in progress resumes, its decisions and "RAW follows JPEG" as it left them (US-37).
      const previous = stored.get(sourceId);
      return begin(source, includeSubfolders, resumable(previous) ? previous : undefined);
    },

    startOver: async (sessionId) => {
      const session = unlocked(sessionOf(sessionId));
      // The app archives the old session with its events (FE §6.1 step 4); the demo forgets it.
      session.archived = true;
      const sourceId = session.source.summary.id;
      // Photos applied by an earlier plan stay applied: only the decisions are put aside.
      const applied = stored.get(sourceId)?.applied ?? [];
      persist(
        sourceId,
        applied.length === 0 ? null : { events: [], rawFollows: true, applied, appliedOn: null },
      );
      return begin(session.source, session.options.includeSubfolders);
    },

    updateSession: async (sessionId, change: SessionChange): Promise<SessionInfo> => {
      const session = unlocked(sessionOf(sessionId));
      let { destination, sameAsLastTime } = session;
      if (change.destinationId !== undefined) {
        const chosen = destinationOf(change.destinationId);
        if (chosen === undefined) {
          throw new Error("unknown destination");
        }
        destination = chosen;
        sameAsLastTime = false;
      }
      session.options = {
        ...session.options,
        ...(change.keepMode === undefined ? {} : { keepMode: change.keepMode }),
        ...(change.rawFollows === undefined ? {} : { rawFollows: change.rawFollows }),
        destinationId: destination?.id ?? null,
      };
      if (destination !== session.destination) {
        // What exists under the old destination no longer matters; `_rejected` (applied rejects
        // included) still does.
        const old = session.destination === null ? null : folded(session.destination.folder.path);
        const outside = [...session.existing].filter(
          (p) => old === null || (p !== old && !p.startsWith(`${old}\\`)),
        );
        session.destination = destination;
        session.existing = new Set([...outside, ...existingFor(session.source, destination)]);
      }
      session.sameAsLastTime = sameAsLastTime;
      // A draft made under the old options can no longer be confirmed (§7.4).
      session.latest = null;
      rememberRoute(session);
      // A resumed session gets "RAW follows JPEG" back as it was left (US-37, proposed).
      if (session.events.length > 0) persist(session.source.summary.id, recordOf(session));
      return infoOf(session);
    },

    chooseDestination: () => Promise.resolve(DESTINATIONS[0] ?? null),

    fullImage: async (photoId) => {
      if (scenario.fullImageUnreadable === true) {
        throw new Error("full image unreadable");
      }
      if (scenario.fullImageDelayMs !== undefined) {
        await new Promise((resolve) => setTimeout(resolve, scenario.fullImageDelayMs));
      }
      for (const session of sessions.values()) {
        const entry = session.entries.find((e) => e.photo.id === photoId);
        if (entry !== undefined) return entry.full();
      }
      // Only ids of the current scan are served (US-33).
      throw new Error("unknown photo");
    },

    recordEvent: async (sessionId, event) => {
      const session = sessionOf(sessionId);
      if (session.archived) {
        throw new Error("session archived");
      }
      session.events.push(event);
      session.appliedOn = null;
      persist(session.source.summary.id, recordOf(session));
    },

    reviewPlan: async (sessionId, uptoSeq) => {
      const session = unlocked(sessionOf(sessionId));
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
      const session = unlocked(sessionOfPlan(planId));
      const choices = session.facts.sourceChoices;
      if (choices === null || !choices.some((c) => c.id === volumeId)) {
        throw new Error("unknown volume");
      }
      session.facts = { ...session.facts, sourceChoices: null };
      return plan(session, session.latest?.uptoSeq ?? 0);
    },

    confirmPlan: async (planId): Promise<ConfirmResult> => {
      const session = unlocked(sessionOfPlan(planId));
      const previous = session.latest as ReviewPlan;
      // What changes on disk between review and confirm, once (US-35).
      if (!session.confirmChecked) {
        session.confirmChecked = true;
        session.facts = { ...session.facts, ...scenario.atConfirm };
        const root = session.destination?.folder.path;
        if (root !== undefined) {
          for (const rel of scenario.takenAtConfirm ?? []) {
            session.existing.add(folded(joinPath(root, [rel])));
          }
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
      rememberRoute(session);
      return { kind: "confirmed", planId: again.planId };
    },

    startApply: async (planId) => {
      if (applies.has(planId)) {
        const apply = applyOf(planId);
        // An interrupted plan rolls forward from the journal once its volumes are present.
        if (apply.sim.phase === "interrupted") resume(apply.sim);
        return statusOf(apply);
      }
      const session = [...sessions.values()].find((s) => s.confirmed === planId);
      if (session === undefined || session.latest?.planId !== planId) {
        throw new Error("plan not confirmed");
      }
      return statusOf(beginApply(session, session.latest));
    },

    applyStatus: async (planId) => statusOf(applyOf(planId)),

    pauseApply: async (planId) => {
      const apply = applyOf(planId);
      requestHalt(apply.sim, "pause");
      return statusOf(apply);
    },

    continueApply: async (planId) => {
      const apply = applyOf(planId);
      resume(apply.sim);
      return statusOf(apply);
    },

    stopApply: async (planId) => {
      const apply = applyOf(planId);
      requestHalt(apply.sim, "cancel");
      return statusOf(apply);
    },

    retryFailed: async (planId) => {
      const apply = applyOf(planId);
      retryFailed(apply.sim);
      return statusOf(apply);
    },

    finishMoves: async (planId) => {
      const apply = applyOf(planId);
      finishMoves(apply.sim);
      return statusOf(apply);
    },

    closeApply: async (planId) => {
      const { session, sim } = applyOf(planId);
      if (!closeSim(sim)) {
        throw new Error("the apply has not ended");
      }
      // The plan's applied groups are locked for good (§5.4); a new plan holds them as applied
      // and sees the files where they now are.
      const applied = new Set([...session.facts.applied, ...appliedNames(sim)]);
      session.facts = { ...session.facts, applied };
      for (const f of sim.files) {
        if (f.state === "done" || f.state === "copiedSourceKept")
          session.existing.add(folded(f.to));
      }
      session.confirmed = null;
      session.latest = null;
      session.appliedOn = appliedDay(wall());
      persist(session.source.summary.id, recordOf(session));
      if (interrupted?.planId === planId) interrupted = null;
    },

    openLog: async (runId) => {
      for (const { sim } of applies.values()) {
        const text = logOf(sim, runId);
        if (text !== null) return { kind: "text", text };
      }
      throw new Error("no log for this run");
    },

    interruptedApply: async (): Promise<InterruptedApply | null> => {
      if (interrupted === null && scenario.interrupted !== undefined && applies.size === 0) {
        interrupted = makeInterrupted(scenario.interrupted.cardOut);
      }
      if (interrupted === null) return null;
      const apply = applies.get(interrupted.planId);
      if (apply === undefined || apply.sim.phase === "closed") return null;
      return { session: interrupted.info, planId: interrupted.planId };
    },
  };
}
