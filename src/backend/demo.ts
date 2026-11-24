import type { Backend } from "./types";

/** Simulated backend for the web demo and for tests. There is no Rust side in the browser. */
export function createBackend(): Backend {
  return {
    version: () => Promise.resolve("demo"),
  };
}
