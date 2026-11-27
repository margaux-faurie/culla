/**
 * Everything the UI asks of the outside world goes through this interface (ADR 0014).
 * `backend/tauri.ts` talks to the Rust side (one `version` command so far); `backend/demo.ts`
 * simulates it in the browser.
 * The UI only ever sees opaque IDs, never file paths.
 */
export interface Backend {
  /** Version of the Rust side, or of the demo build. */
  version(): Promise<string>;
}
