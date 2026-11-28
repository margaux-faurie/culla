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

/**
 * The plan cannot be built: kept photos that would get operations have no move or copy, or no
 * connected destination (the core's `PlanError::NoDestination`, SPEC §10 plan() (c)). A kept photo
 * held back (applied, missing) does not count, and rejects alone never need one. The review asks
 * for the missing choice instead of showing a plan.
 */
export class NoKeepRouteError extends Error {
  constructor() {
    super("kept photos have no move or copy or no destination");
    this.name = "NoKeepRouteError";
  }
}

export const isNoKeepRoute = (error: unknown): boolean => error instanceof NoKeepRouteError;
