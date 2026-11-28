import type { DestinationSummary, KeepMode, RouteExample, SourceSummary } from "../backend/types";
import { protectedNote, stayVerb } from "./staying";

export interface RouteViewProps {
  readonly source: SourceSummary;
  readonly photoSummary: string;
  /** One real photo and its route, computed by the backend. Null when the session has none. */
  readonly example: RouteExample | null;
  /** `E:\2026\_rejected` from the backend. */
  readonly rejectedFolder: string;
  /** `_rejected\` is marked as a new folder only while it does not exist yet (SPEC §2). */
  readonly rejectedFolderExists: boolean;
  readonly destination: DestinationSummary | null;
  readonly keepMode: KeepMode | null;
}

/**
 * "Where photos go" (SPEC §2): the origin, a Kept branch, a Rejected branch and the photos left in
 * place, with one real example path per branch, on the apply screen. The review shows the same
 * route in short, one sentence per branch (lean review, 28 Nov 2026); the swipe screen shows each
 * photo's route under its buttons. Every path is a display string from
 * the backend, printed as it is.
 */
export function RouteView(props: RouteViewProps) {
  const { source, example, destination, keepMode } = props;

  return (
    <ol className="route" aria-label="Where your photos will go">
      <li className="route-step">
        <span className="route-mark route-mark-origin" aria-hidden="true" />
        <div className="route-body">
          <span className="route-label">From</span>
          <span className="route-place">{source.folder.path}</span>
          <span className="route-note">
            {source.folder.volume} · {props.photoSummary}
          </span>
        </div>
      </li>

      <li className="route-step">
        <span className="route-mark route-mark-keep" aria-hidden="true">
          <CheckIcon />
        </span>
        <div className="route-body">
          <span className="route-label route-label-keep">Kept photos →</span>
          <span className="route-place">{keptPlace(destination, keepMode)}</span>
          <span className="route-note">Subfolders are kept as they are.</span>
          {example !== null && (
            <Example
              example={example}
              to={example.route.keptTo ?? "the destination you choose"}
              mode={keepMode}
            />
          )}
        </div>
      </li>

      <li className="route-step">
        <span className="route-mark route-mark-reject" aria-hidden="true">
          <CrossIcon />
        </span>
        <div className="route-body">
          <span className="route-label">← Rejected photos</span>
          <span className="route-place">
            Moved to {props.rejectedFolder}
            {!props.rejectedFolderExists && (
              <>
                <span className="visually-hidden">, </span>
                <span className="pill pill-outline">new folder</span>
              </>
            )}
          </span>
          <span className="route-note">
            One folder, inside the folder you sort. Nothing is deleted.
          </span>
          {example !== null && (
            <Example example={example} to={example.route.rejectedTo} mode="move" />
          )}
        </div>
      </li>

      <li className="route-step route-step-last">
        <span className="route-mark route-mark-stay" aria-hidden="true" />
        <div className="route-body">
          <span className="route-label route-label-muted">Photos you don't decide</span>
          <span className="route-note">Stay exactly where they are.</span>
        </div>
      </li>
    </ol>
  );
}

/** The Kept branch's place: no verb until moved or copied is chosen. */
function keptPlace(destination: DestinationSummary | null, keepMode: KeepMode | null): string {
  const where =
    destination === null || !destination.available
      ? "the destination you choose"
      : destination.folder.path;
  if (keepMode === null) {
    return `To ${where}`;
  }
  return `${keepMode === "copy" ? "Copied" : "Moved"} to ${where}`;
}

interface ExampleProps {
  readonly example: RouteExample;
  readonly to: string;
  /** The branch's kind before camera protection: the keep mode for Kept, "move" for Rejected. */
  readonly mode: KeepMode | null;
}

/**
 * One real example path: the files that go, where they go, and the files that stay. The
 * camera-protected note shows only where the read-only attribute changed the kind, so not on a
 * keep by copy.
 */
function Example({ example, to, mode }: ExampleProps) {
  const { route } = example;
  return (
    <span className="route-example">
      <span>
        <span className="visually-hidden">Example: </span>
        {route.origin} <b>{route.going.extensions}</b> → {to}
        {protectedNote(route.cameraProtected, mode)}
      </span>
      {route.staying !== null && (
        <span>
          {route.staying.name} <b>{route.staying.extensions}</b> {stayVerb(route.staying)} in{" "}
          {route.folder}
        </span>
      )}
    </span>
  );
}

export function CheckIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 18 18" fill="none" aria-hidden="true">
      <path
        d="M3.5 9.5l3.5 3.5 7.5-8"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function CrossIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 18 18" fill="none" aria-hidden="true">
      <path d="M4 4l10 10M14 4L4 14" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}
