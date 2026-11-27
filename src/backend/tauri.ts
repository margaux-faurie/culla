import { invoke } from "@tauri-apps/api/core";
import type { Backend } from "./types";

/** The Rust commands behind these calls arrive one slice at a time (CLAUDE.md, "How to add a feature"). */
const notYet = (name: string) => (): Promise<never> =>
  Promise.reject(new Error(`${name} is not available in the app yet`));

/**
 * Backend for the desktop app. Commands are typed by hand until tauri-specta generates
 * src/bindings.ts (ADR 0019, M3); each one needs `allow-<command>` in the main capability.
 * Only `version` exists on the Rust side so far: the start and swipe screens run against the
 * demo backend until their commands land.
 */
export function createBackend(): Backend {
  return {
    version: () => invoke<string>("version"),
    recentSources: notYet("recentSources"),
    chooseSource: notYet("chooseSource"),
    describeSource: notYet("describeSource"),
    proposeDestination: notYet("proposeDestination"),
    chooseDestination: notYet("chooseDestination"),
    startSession: notYet("startSession"),
    recordEvent: notYet("recordEvent"),
  };
}
