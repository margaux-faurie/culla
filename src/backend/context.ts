import { createContext, useContext } from "react";
import type { Backend } from "./types";

/** The backend chosen at build time, handed to the UI through context (ADR 0014). */
export const BackendContext = createContext<Backend | null>(null);

export function useBackend(): Backend {
  const backend = useContext(BackendContext);
  if (backend === null) {
    throw new Error("useBackend needs a <BackendContext> above it");
  }
  return backend;
}
