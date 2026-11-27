import type { ReactNode } from "react";
import type { DestinationSummary, KeepMode, PhotoSummary, SourceSummary } from "../backend/types";
import {
  filesOf,
  joinPath,
  keepVerb,
  keptFolder,
  photoFolder,
  rejectedFolder,
  rejectVerb,
  suffixes,
} from "./route";

export interface RouteViewProps {
  readonly source: SourceSummary;
  readonly photoSummary: string;
  readonly example: PhotoSummary | null;
  readonly rejectedFolderExists: boolean;
  readonly destination: DestinationSummary | null;
  readonly keepMode: KeepMode | null;
  readonly rawFollows: boolean;
  /** Controls placed in the From branch (change folder, subfolders). */
  readonly originControls?: ReactNode;
  /** Controls placed in the Kept branch (moved or copied, destination). */
  readonly keepControls?: ReactNode;
}

/**
 * "Where photos go" (SPEC §2): the origin, a Kept branch, a Rejected branch and the photos left in
 * place, with one real example path per branch. The same view heads the start, review and apply
 * screens.
 */
export function RouteView(props: RouteViewProps) {
  const { source, example, destination, keepMode, rawFollows } = props;
  const rejectedRoot = joinPath(source.folder.path, ["_rejected"]);

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
            <span className="route-place">
              {keepMode === "copy" ? "Copied" : "Moved"} to {destination?.folder.path ?? "—"}
            </span>
          )}
          <span className="route-note">Subfolders are kept as they are.</span>
          {example !== null && (
            <Example
              sourcePath={source.folder.path}
              photo={example}
              rawFollows={rawFollows}
              to={keptTarget(example, destination, keepMode)}
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
            Moved to {rejectedRoot}
            {!props.rejectedFolderExists && <span className="pill pill-outline">new folder</span>}
          </span>
          <span className="route-note">
            One folder, inside the folder you sort. Nothing is deleted.
          </span>
          {example !== null && (
            <Example
              sourcePath={source.folder.path}
              photo={example}
              rawFollows={rawFollows}
              to={`${rejectVerb(example)} to ${rejectedFolder(source.folder.path, example)}`}
            />
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

function keptTarget(
  photo: PhotoSummary,
  destination: DestinationSummary | null,
  keepMode: KeepMode | null,
): string {
  if (destination === null) {
    return "the destination you choose";
  }
  const folder = keptFolder(destination.folder.path, photo);
  return keepMode === null ? folder : `${keepVerb(keepMode, photo)} to ${folder}`;
}

interface ExampleProps {
  readonly sourcePath: string;
  readonly photo: PhotoSummary;
  readonly rawFollows: boolean;
  readonly to: string;
}

/** One real example path: the files that go, where they go, and the files that stay. */
function Example({ sourcePath, photo, rawFollows, to }: ExampleProps) {
  const files = filesOf(photo, rawFollows);
  const from = photoFolder(sourcePath, photo);
  return (
    <span className="route-example">
      <span>
        <span className="visually-hidden">Example: </span>
        {joinPath(from, [photo.name])} <b>{suffixes(photo, files.going)}</b> → {to}
        {photo.cameraProtected && " (camera-protected: copied, original kept)"}
      </span>
      {files.staying.length > 0 && (
        <span>
          {photo.name} <b>{suffixes(photo, files.staying)}</b> stay in {from}
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
