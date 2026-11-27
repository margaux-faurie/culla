import { useEffect, useRef } from "react";
import type { SessionInfo } from "../backend/types";
import { RouteView } from "../route/RouteView";
import { counts, type SwipeState } from "../swipe/session";

export interface ReviewPlaceholderProps {
  readonly session: SessionInfo;
  readonly state: SwipeState;
  readonly onBack: () => void;
}

/**
 * Stand-in for the review screen (M4, US-34): the route and the counts, so Enter and the Review
 * button lead somewhere. The plan, preflight and Apply come with the review and apply slices.
 */
export function ReviewPlaceholder({ session, state, onBack }: ReviewPlaceholderProps) {
  const ref = useRef<HTMLElement>(null);
  const tally = counts(state);
  const photo = session.photos.find((p) => p.id === session.exampleId);
  const example = photo === undefined ? null : { photo, route: photo.route };

  useEffect(() => {
    const root = ref.current;
    if (root === null) {
      return;
    }
    root.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code === "Escape") {
        event.preventDefault();
        onBack();
      }
    };
    root.addEventListener("keydown", onKeyDown);
    return () => root.removeEventListener("keydown", onKeyDown);
  }, [onBack]);

  return (
    <main ref={ref} className="review" tabIndex={-1} aria-labelledby="review-title">
      <h1 id="review-title" className="title">
        Review
      </h1>
      <p className="lead">
        {tally.kept} kept, {tally.rejected} rejected, {tally.undecided} undecided. No files have
        moved yet.
      </p>
      <p className="small">The full plan and Apply come in a later version of this screen.</p>
      {/* The route list carries the name "Where your photos will go"; the panel adds none. */}
      <div className="panel">
        <RouteView
          source={session.source}
          photoSummary={`${tally.total} photos`}
          example={example}
          rejectedFolder={session.rejectedFolder}
          rejectedFolderExists={session.rejectedFolderExists}
          destination={session.destination}
          keepMode={session.options.keepMode}
        />
      </div>
      <div>
        <button type="button" className="button button-outline" onClick={onBack}>
          Back to sorting
        </button>
      </div>
    </main>
  );
}
