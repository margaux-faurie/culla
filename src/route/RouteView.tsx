import type { ReactNode } from "react";
import type { DestinationSummary, KeepMode, RouteExample, SourceSummary } from "../backend/types";

export interface RouteViewProps {
  readonly source: SourceSummary;
  readonly photoSummary: string;
  /** One real photo and its route, computed by the backend. Null while loading or when none. */
  readonly example: RouteExample | null;
  /** `E:\2026\_rejected` from the backend, or null while it loads. */
  readonly rejectedFolder: string | null;
  /** Null while the folder's details load: the "new folder" mark waits for them. */
  readonly rejectedFolderExists: boolean | null;
  readonly destination: DestinationSummary | null;
  readonly keepMode: KeepMode | null;
  /** Controls placed in the From branch (change folder, subfolders). */
  readonly originControls?: ReactNode;
  /** Controls placed in the Kept branch (moved or copied, destination). */
  readonly keepControls?: ReactNode;
}

/**
 * "Where photos go" (SPEC §2): the origin, a Kept branch, a Rejected branch and the photos left in
 * place, with one real example path per branch. The same view heads the start, review and apply
 * screens. Every path is a display string from the backend, printed as it is.
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
          {props.originControls}
        </div>
      </li>

      <li className="route-step">
        <span className="route-mark route-mark-keep" aria-hidden="true">
          <CheckIcon />
        </span>
        <div className="route-body">
          <span className="route-label route-label-keep">Kept photos →</span>
          {props.keepControls ?? (
            <span className="route-place">{keptPlace(destination, keepMode)}</span>
          )}
          <span className="route-note">Subfolders are kept as they are.</span>
          {example !== null && (
            <Example example={example} to={example.route.keptTo ?? "the destination you choose"} />
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
            Moved to {props.rejectedFolder ?? "_rejected"}
            {props.rejectedFolderExists === false && (
              <>
                <span className="visually-hidden">, </span>
                <span className="pill pill-outline">new folder</span>
              </>
            )}
          </span>
          <span className="route-note">
            One folder, inside the folder you sort. Nothing is deleted.
          </span>
          {example !== null && <Example example={example} to={example.route.rejectedTo} />}
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

/** The Kept branch's place when no controls replace it: no verb until moved or copied is chosen. */
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
}

/** One real example path: the files that go, where they go, and the files that stay. */
function Example({ example, to }: ExampleProps) {
  const { photo, route } = example;
  return (
    <span className="route-example">
      <span>
        <span className="visually-hidden">Example: </span>
        {route.origin} <b>{route.going.extensions}</b> → {to}
        {photo.cameraProtected && " (camera-protected: copied, original kept)"}
      </span>
      {route.staying !== null && (
        <span>
          {route.staying.name} <b>{route.staying.extensions}</b> stay in {route.folder}
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
