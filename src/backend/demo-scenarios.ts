/**
 * Named demo scenarios for the states a demo folder cannot reach on its own (stories.md, M4 tests,
 * proposed): a write-protected card, held groups, no space, warnings, two volumes with one serial,
 * previews still running, an unplugged drive, the two things preflight at confirm can find, and
 * for the apply (US-36, US-38): a slow reader, a card pulled mid-apply (cleanly, or leaving a lost
 * file), a full destination, a verify mismatch, a file another app holds, and an apply found
 * interrupted at launch, with its card in or out; for the start screen, a folder with no photos; and,
 * for zoom (US-39), a full image that cannot be read.
 *
 * Chosen by a query parameter, `?scenario=<name>`, read by the demo backend only (`demo.ts`): the
 * desktop backend never looks at it, and the demo bundle is the only one with this module (ADR
 * 0014). Proposed by the agent, awaiting Margaux (SPEC §10): a query parameter rather than a
 * demo-only build flag or another route. An unknown name is ignored.
 */
import type { ApplyScenario } from "./demo-apply";
import { type DemoFacts, NORMAL_FACTS } from "./demo-plan";

export const SCENARIOS = [
  "write-protected",
  "held",
  "no-space",
  "warnings",
  "two-volumes",
  "previews-busy",
  "drive-unplugged",
  "taken-at-confirm",
  "locked-at-confirm",
  "slow",
  "card-pulled",
  "lost",
  "disk-full",
  "verify-mismatch",
  "locked-file",
  "interrupted",
  "interrupted-card-out",
  "empty-folder",
  "full-image-unreadable",
] as const;

export type ScenarioName = (typeof SCENARIOS)[number];

export interface Scenario {
  /** The facts preflight sees at review. */
  readonly facts: DemoFacts;
  /** Facts that change between review and confirm; they stay changed afterwards. */
  readonly atConfirm?: Partial<DemoFacts>;
  /**
   * Target paths (display form, under the session's destination) that appear between review and
   * confirm, relative to the destination root: a name taken meanwhile (P11).
   */
  readonly takenAtConfirm?: readonly string[];
  /** Facts that clear once the user checks again (the previews finish). */
  readonly clearsOnRecheck?: Partial<DemoFacts>;
  /** What goes wrong during the apply. */
  readonly apply?: ApplyScenario;
  /**
   * At launch, an apply cut by a crash waits to be finished (US-38); `cardOut` leaves its source
   * unplugged for a moment first.
   */
  readonly interrupted?: { readonly cardOut: boolean };
  /** "Choose a folder" picks a folder with no photos (start screen). */
  readonly emptyFolder?: boolean;
  /** Zoom cannot read the full image (card pulled, damaged file): the preview stays. */
  readonly fullImageUnreadable?: boolean;
}

const names = (...list: string[]): ReadonlySet<string> => new Set(list);

const TABLE: Readonly<Record<ScenarioName, Scenario>> = {
  "write-protected": { facts: { ...NORMAL_FACTS, sourceReadOnly: true } },
  held: {
    facts: {
      ...NORMAL_FACTS,
      changed: names("MGF_7001"),
      applied: names("MGF_7002"),
      links: names("MGF_7003"),
    },
  },
  "no-space": { facts: { ...NORMAL_FACTS, destinationFree: 900_000_000 } },
  warnings: {
    facts: {
      ...NORMAL_FACTS,
      syncRoot: true,
      locked: names("MGF_7001"),
      longPaths: names("MGF_7002"),
    },
  },
  "two-volumes": {
    facts: {
      ...NORMAL_FACTS,
      sourceChoices: [
        { id: "vol-e", label: "Photos drive (E:)" },
        { id: "vol-h", label: "Photos drive (H:)" },
      ],
    },
  },
  "previews-busy": {
    facts: { ...NORMAL_FACTS, previewsBusy: true },
    clearsOnRecheck: { previewsBusy: false },
  },
  "drive-unplugged": { facts: { ...NORMAL_FACTS, destinationMounted: false } },
  "taken-at-confirm": { facts: NORMAL_FACTS, takenAtConfirm: ["MGF_7001.JPG"] },
  "locked-at-confirm": { facts: NORMAL_FACTS, atConfirm: { sourceReadOnly: true } },
  slow: { facts: NORMAL_FACTS, apply: { speed: 0.2 } },
  "card-pulled": { facts: NORMAL_FACTS, apply: { pull: "clean" } },
  lost: { facts: NORMAL_FACTS, apply: { pull: "lost" } },
  "disk-full": { facts: NORMAL_FACTS, apply: { diskFull: true } },
  "verify-mismatch": { facts: NORMAL_FACTS, apply: { verifyMismatch: true } },
  "locked-file": { facts: { ...NORMAL_FACTS, locked: names("MGF_7001") } },
  interrupted: { facts: NORMAL_FACTS, interrupted: { cardOut: false } },
  "interrupted-card-out": { facts: NORMAL_FACTS, interrupted: { cardOut: true } },
  "empty-folder": { facts: NORMAL_FACTS, emptyFolder: true },
  "full-image-unreadable": { facts: NORMAL_FACTS, fullImageUnreadable: true },
};

const NORMAL: Scenario = { facts: NORMAL_FACTS };

const isScenario = (name: string): name is ScenarioName =>
  (SCENARIOS as readonly string[]).includes(name);

/** The scenario a query string names (`?scenario=held`), or null. */
export function scenarioFromSearch(search: string): ScenarioName | null {
  const name = new URLSearchParams(search).get("scenario");
  return name !== null && isScenario(name) ? name : null;
}

export const scenarioOf = (name: ScenarioName | null): Scenario =>
  name === null ? NORMAL : TABLE[name];
