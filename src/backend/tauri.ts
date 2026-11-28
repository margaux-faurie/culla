import { invoke } from "@tauri-apps/api/core";
import { NotReadyError } from "./errors";
import type { Backend } from "./types";

/**
 * The Rust commands behind these calls arrive one slice at a time (CLAUDE.md, "How to add a
 * feature"); until then the UI shows "Not ready yet" for them.
 */
const notYet = (name: string) => (): Promise<never> => Promise.reject(new NotReadyError(name));

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
    startSession: notYet("startSession"),
    startOver: notYet("startOver"),
    updateSession: notYet("updateSession"),
    chooseDestination: notYet("chooseDestination"),
    fullImage: notYet("fullImage"),
    photoExif: notYet("photoExif"),
    recordEvent: notYet("recordEvent"),
    reviewPlan: notYet("reviewPlan"),
    chooseVolume: notYet("chooseVolume"),
    confirmPlan: notYet("confirmPlan"),
    startApply: notYet("startApply"),
    applyStatus: notYet("applyStatus"),
    pauseApply: notYet("pauseApply"),
    continueApply: notYet("continueApply"),
    stopApply: notYet("stopApply"),
    retryFailed: notYet("retryFailed"),
    finishMoves: notYet("finishMoves"),
    closeApply: notYet("closeApply"),
    openLog: notYet("openLog"),
    interruptedApply: notYet("interruptedApply"),
  };
}
