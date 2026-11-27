/**
 * A backend call whose Rust command has not landed yet (desktop mode, until each slice arrives).
 * The UI shows a "Not ready yet" notice for it instead of an error message.
 */
export class NotReadyError extends Error {
  constructor(name: string) {
    super(`${name} is not available in the app yet`);
    this.name = "NotReadyError";
  }
}

export const isNotReady = (error: unknown): boolean => error instanceof NotReadyError;
