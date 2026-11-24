import { invoke } from "@tauri-apps/api/core";
import type { Backend } from "./types";

/**
 * Backend for the desktop app. Commands are typed by hand until tauri-specta generates
 * src/bindings.ts (ADR 0019, M1); each one needs `allow-<command>` in the main capability.
 */
export function createBackend(): Backend {
  return {
    version: () => invoke<string>("version"),
  };
}
