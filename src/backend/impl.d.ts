// Contract of `#backend-impl`. Vite resolves the module to tauri.ts (desktop) or demo.ts (demo,
// test); tsconfig.json maps it here so tsc checks callers against the shared shape. Both files
// export a `createBackend` with this signature.
import type { Backend } from "./types";

export declare function createBackend(): Backend;
