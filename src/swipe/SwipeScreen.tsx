import { type Dispatch, useEffect, useMemo, useRef, useState } from "react";
import type { SessionInfo, SessionPhoto } from "../backend/types";
import type { GroupState, Verdict } from "../decisions/events";
import { CheckIcon, CrossIcon } from "../route/RouteView";
import { stayVerb } from "../route/staying";
import { HelpDialog } from "./HelpDialog";
import { type Command, commandFor } from "./keys";
import {
  counts,
  currentPhoto,
  type Feedback,
  type SwipeAction,
  type SwipeState,
  stateAt,
} from "./session";

export interface SwipeScreenProps {
  readonly session: SessionInfo;
  readonly state: SwipeState;
  readonly dispatch: Dispatch<SwipeAction>;
  readonly onReview: () => void;
  /** Shown when autosave failed; the decision stays in the reducer either way. */
  readonly saveError: boolean;
}

/** How long a toast stays, and how long the exit card stays mounted (CSS animates within it). */
const TOAST_MS = 4000;
const EXIT_MS = 320;

/**
 * Swipe screen (SPEC §4.2, US-22 to US-32, US-42) on the Night theme. Every decision is committed to
 * the reducer at once; the exit animation catches up and never blocks input, and at most 3 cards
 * are mounted: the one leaving, the current one and the next one underneath (ADR 0015).
 *
 * Drag is not wired yet: it arrives with Motion (ADR 0015), which feeds `drag.ts` from `onDragEnd`.
 * The Keep and Reject buttons and the keyboard drive the same path the drag will.
 */
export function SwipeScreen(props: SwipeScreenProps) {
  const { session, state, dispatch, onReview } = props;
  const rootRef = useRef<HTMLElement>(null);
  const [helpOpen, setHelpOpen] = useState(false);
  const byId = useMemo(() => new Map(session.photos.map((p) => [p.id, p])), [session.photos]);
  const photo = byId.get(currentPhoto(state) ?? "");
  const next = byId.get(state.photos[state.index + 1] ?? "");
  const decision: GroupState = photo === undefined ? "undecided" : stateAt(state, state.index);
  const tally = counts(state);

  const run = (command: Command) => {
    const now = Date.now();
    switch (command) {
      case "keep":
      case "reject":
        dispatch({ type: "decide", verdict: command, now });
        return;
      case "clear":
      case "undo":
      case "redo":
        dispatch({ type: command, now });
        return;
      case "next":
      case "prev":
      case "nextUndecided":
        dispatch({ type: command });
        return;
      case "filter":
        dispatch({ type: "cycleFilter" });
        return;
      case "help":
        setHelpOpen(true);
        return;
      case "review":
        onReview();
        return;
      case "escape":
        dispatch({ type: "dismissFeedback" });
        return;
    }
  };

  // Shortcuts listen on the swipe view only, so single keys act only while it has focus (2.1.4).
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const root = rootRef.current;
    if (root === null) {
      return;
    }
    root.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      // A held Enter on a focused Keep, Reject or Undo button would press it again and again.
      const enter = event.code === "Enter" || event.code === "NumpadEnter";
      if (
        event.repeat &&
        enter &&
        event.target instanceof HTMLButtonElement &&
        root.contains(event.target)
      ) {
        event.preventDefault();
        return;
      }
      const command = commandFor(event);
      if (command === null) {
        return;
      }
      // Enter on a focused button presses that button, not the review shortcut.
      if (command === "review" && event.target !== root) {
        return;
      }
      event.preventDefault();
      runRef.current(command);
    };
    root.addEventListener("keydown", onKeyDown);
    return () => root.removeEventListener("keydown", onKeyDown);
  }, []);

  // A focused button can unmount with the state (Undo once the photo is undecided, Jump to
  // undecided once the end card goes, Keep or Reject once the end card comes). Focus would fall to
  // the body, where the shortcuts do not listen, so it goes back to the swipe view. Runs after
  // every render.
  useEffect(() => {
    const root = rootRef.current;
    if (helpOpen || root === null) {
      return;
    }
    if (!root.contains(document.activeElement)) {
      root.focus();
    }
  });

  // The toast hides itself after a while; a newer one restarts the clock.
  const { feedback, nonce } = state;
  useEffect(() => {
    if (feedback === null) {
      return;
    }
    const timer = window.setTimeout(() => dispatch({ type: "dismissFeedback" }), TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [feedback, dispatch]);

  const exit = useExit(state, byId);
  const folderPath = photo === undefined ? session.source.folder.path : photo.route.folder;
  const position = Math.min(state.index + 1, tally.total);
  const progress = tally.total === 0 ? 0 : ((tally.kept + tally.rejected) / tally.total) * 100;

  return (
    <>
      <main
        ref={rootRef}
        className="swipe"
        data-theme="night"
        tabIndex={-1}
        aria-label="Sort photos"
      >
        <div className="progress" aria-hidden="true">
          <div className="progress-fill" style={{ width: `${progress}%` }} />
        </div>
        <header className="swipe-header">
          <div className="swipe-where">
            <h1 className="brand">Culla</h1>
            <span className="muted">{folderPath}</span>
            <span className="visually-hidden">. </span>
            <span className="pill pill-quiet">No files have moved yet</span>
          </div>
          <div className="swipe-tally">
            <span className="strong">
              {position} / {tally.total}
            </span>
            <span className="tally-item">
              <span className="dot dot-keep" aria-hidden="true" />
              {tally.kept} kept
            </span>
            <span className="tally-item">
              <span className="dot dot-reject" aria-hidden="true" />
              {tally.rejected} rejected
            </span>
            <span className="muted">{tally.undecided} undecided</span>
            <button
              type="button"
              className="button button-quiet button-small"
              aria-keyshortcuts="F"
              onClick={() => run("filter")}
            >
              Showing {state.filter === "all" ? "all" : state.filter}
            </button>
            <button
              type="button"
              className="button button-primary button-small"
              aria-keyshortcuts="Enter"
              onClick={onReview}
            >
              Review
            </button>
          </div>
        </header>

        {props.saveError && (
          <p className="save-error" role="alert">
            Could not save the last decision. It is kept on screen; keep sorting and it is saved
            again with the next one.
          </p>
        )}
        <div className="toast-slot">
          {feedback !== null && (
            <p className="toast" key={nonce}>
              {toastText(feedback, byId)}
            </p>
          )}
        </div>

        <div className="stage">
          {photo === undefined ? (
            <EndCard
              undecided={tally.undecided}
              kept={tally.kept}
              rejected={tally.rejected}
              onJump={() => run("nextUndecided")}
              onReview={onReview}
            />
          ) : (
            <>
              {next !== undefined && (
                <Card photo={next} state="undecided" className="card card-next" hidden />
              )}
              <Card
                key={photo.id}
                photo={photo}
                state={decision}
                className={cardClass(decision, feedback, photo.id, nonce)}
              />
            </>
          )}
          {exit !== null && (
            <Card
              key={`exit-${exit.nonce}`}
              photo={exit.photo}
              state={exit.verdict === "keep" ? "kept" : "rejected"}
              className={`card card-exit card-exit-${exit.verdict}`}
              hidden
            />
          )}
        </div>

        {photo !== undefined && <Actions photo={photo} decision={decision} onCommand={run} />}

        <p className="hints">
          <span>
            <kbd>U</kbd> undo
          </span>
          <span>
            <kbd>Shift</kbd> + <kbd>←</kbd> <kbd>→</kbd> browse
          </span>
          <span>
            <kbd>N</kbd> next undecided
          </span>
          <span>
            <kbd>F</kbd> filter
          </span>
          <span>
            <kbd>?</kbd> all keys
          </span>
        </p>

        {/* A repeated refusal has the same words; the alternating no-break space makes it a
            change, so it is announced again (NFR-A11Y-09). The nonce moves with every refusal. */}
        <output className="visually-hidden">
          {state.announcement}
          {state.nonce % 2 === 1 ? "\u00a0" : ""}
        </output>
      </main>
      <HelpDialog open={helpOpen} onClose={() => setHelpOpen(false)} />
    </>
  );
}

function cardClass(
  decision: GroupState,
  feedback: Feedback | null,
  photoId: string,
  nonce: number,
): string {
  const classes = ["card", "card-current", `card-${decision}`];
  if (feedback?.kind === "already-decided" && feedback.photoId === photoId) {
    classes.push(nonce % 2 === 0 ? "card-shake-a" : "card-shake-b");
  }
  return classes.join(" ");
}

/** The card that just got a decision, kept mounted for its exit animation only. */
function useExit(state: SwipeState, byId: ReadonlyMap<string, SessionPhoto>) {
  const [exit, setExit] = useState<{ photo: SessionPhoto; verdict: Verdict; nonce: number } | null>(
    null,
  );
  const { exit: leaving, nonce } = state;
  useEffect(() => {
    const photo = leaving === null ? undefined : byId.get(leaving.photoId);
    if (leaving === null || photo === undefined) {
      setExit(null);
      return;
    }
    setExit({ photo, verdict: leaving.verdict, nonce });
    const timer = window.setTimeout(() => setExit(null), EXIT_MS);
    return () => window.clearTimeout(timer);
  }, [leaving, nonce, byId]);
  return exit;
}

interface CardProps {
  readonly photo: SessionPhoto;
  readonly state: GroupState;
  readonly className: string;
  /** Hidden cards (the next one, the one leaving) are decoration for assistive tech. */
  readonly hidden?: boolean;
}

function Card({ photo, state, className, hidden = false }: CardProps) {
  return (
    <figure className={className} aria-hidden={hidden ? true : undefined}>
      <img
        src={photo.previewUrl}
        alt={hidden ? "" : photo.alt}
        draggable={false}
        decoding="async"
      />
      {!hidden && (
        <figcaption className="card-caption">
          <Badge state={state} />
          <span className="pill pill-photo">
            {photo.name} · {photo.kind}
          </span>
        </figcaption>
      )}
    </figure>
  );
}

/** Label and shape as well as colour (NFR-A11Y-08): filled for kept, outline for rejected. */
function Badge({ state }: { state: GroupState }) {
  switch (state) {
    case "kept":
      return (
        <span className="badge badge-kept">
          <CheckIcon />
          Kept
        </span>
      );
    case "rejected":
      return (
        <span className="badge badge-rejected">
          <CrossIcon />
          Rejected
        </span>
      );
    case "undecided":
      return <span className="badge badge-undecided">Undecided</span>;
  }
}

interface ActionsProps {
  readonly photo: SessionPhoto;
  readonly decision: GroupState;
  readonly onCommand: (command: Command) => void;
}

/**
 * Visible Keep and Reject buttons (US-42, WCAG 2.5.7), each with where the photo would go. On a
 * decided photo they stay clickable but only explain (ADR 0006), and the route says where the photo
 * will go instead of what the buttons would do.
 */
function Actions({ photo, decision, onCommand }: ActionsProps) {
  const decided = decision !== "undecided";
  const keepTo = keepRoute(photo);
  const rejectTo = photo.route.rejectedTo;
  const { staying } = photo.route;
  return (
    <div className={decided ? "actions actions-decided" : "actions"}>
      <div className="action-row">
        <div className="action">
          <button
            type="button"
            className="button button-reject"
            aria-label="Reject"
            aria-keyshortcuts="ArrowLeft"
            onClick={() => onCommand("reject")}
          >
            <CrossIcon />
            Reject
            <kbd>←</kbd>
          </button>
          {!decided && <span className="action-route">{rejectTo}</span>}
        </div>
        <div className="action">
          <button
            type="button"
            className="button button-keep"
            aria-label="Keep"
            aria-keyshortcuts="ArrowRight"
            onClick={() => onCommand("keep")}
          >
            <CheckIcon />
            Keep
            <kbd>→</kbd>
          </button>
          {!decided && <span className="action-route">{keepTo}</span>}
        </div>
        {decided && (
          <button
            type="button"
            className="button button-undo"
            aria-keyshortcuts="U"
            onClick={() => onCommand("clear")}
          >
            Undo
            <kbd>U</kbd>
          </button>
        )}
      </div>
      {decided && (
        <p className="action-note">
          {decision === "kept" ? `Kept: will be ${keepTo}.` : `Rejected: will be ${rejectTo}.`}{" "}
          Arrows never change a decision. Undo first, then decide again.
        </p>
      )}
      {staying !== null && (
        <p className="action-note">
          RAW and edit files do not follow: {staying.name} {staying.extensions} {stayVerb(staying)}{" "}
          in {photo.route.folder}.
        </p>
      )}
    </div>
  );
}

function EndCard(props: {
  readonly undecided: number;
  readonly kept: number;
  readonly rejected: number;
  readonly onJump: () => void;
  readonly onReview: () => void;
}) {
  return (
    <section className="end-card" aria-labelledby="end-title">
      <h2 id="end-title" className="panel-title">
        End of the list
      </h2>
      <p className="muted">
        {props.kept} kept, {props.rejected} rejected, {props.undecided} undecided.
      </p>
      <p className="muted">No files have moved yet.</p>
      <div className="action-row">
        {props.undecided > 0 && (
          <button type="button" className="button button-quiet" onClick={props.onJump}>
            Jump to undecided ({props.undecided})
          </button>
        )}
        <button type="button" className="button button-primary" onClick={props.onReview}>
          Review
        </button>
      </div>
    </section>
  );
}

function toastText(feedback: Feedback, byId: ReadonlyMap<string, SessionPhoto>): string {
  switch (feedback.kind) {
    case "already-decided": {
      const photo = byId.get(feedback.photoId);
      if (photo === undefined || feedback.state === "undecided") {
        return "Press U to undo.";
      }
      const where = feedback.state === "kept" ? keepRoute(photo) : photo.route.rejectedTo;
      return `Already ${feedback.state}. Will be ${where}. Press U to undo.`;
    }
    case "already-undecided":
      return "This photo has no decision to undo.";
    case "nothing-to-undo":
      return "Nothing to undo.";
    case "nothing-to-redo":
      return "Nothing to redo.";
    case "no-undecided":
      return "No undecided photos left.";
  }
}

/** The session's destination is available, so `keptTo` is set; the fallback is defensive. */
const keepRoute = (photo: SessionPhoto): string => photo.route.keptTo ?? "kept";
