import { useEffect, useReducer, useRef, useState } from "react";
import { ApplyScreen } from "../apply/ApplyScreen";
import { useBackend } from "../backend/context";
import type { SessionInfo } from "../backend/types";
import { ReviewScreen } from "../review/ReviewScreen";
import { SwipeScreen } from "../swipe/SwipeScreen";
import { resumedState, swipeReducer } from "../swipe/session";

/**
 * One session: the swipe reducer, autosave, and the switch between swipe, review and apply. The reducer
 * commits first; each new event is then mirrored to the backend in order (ADR 0005, architecture
 * §6.2). A failed save keeps the decision and is retried, from the first unsaved event, on the next
 * change.
 */
export interface SessionViewProps {
  readonly session: SessionInfo;
  /** Leaves the session for the start screen. */
  readonly onExit: () => void;
  /** "Start over" made a new session on the same folder (US-37): it replaces this one. */
  readonly onRestarted: (session: SessionInfo) => void;
}

export function SessionView({ session: started, onExit, onRestarted }: SessionViewProps) {
  const backend = useBackend();
  // The session as the backend last answered it: the review screen changes its options (move or
  // copy, destination, RAW follows), and every photo's route with them. The photos stay the same.
  const [session, setSession] = useState(started);
  // A resumed session starts from its stored log (US-37), which the backend already has.
  const [state, dispatch] = useReducer(swipeReducer, started, (s) =>
    resumedState(
      s.photos.map((p) => p.id),
      s.events,
    ),
  );
  const [screen, setScreen] = useState<"swipe" | "review" | "apply">("swipe");
  // The confirmed plan the apply screen runs (US-36).
  const [planId, setPlanId] = useState<string | null>(null);
  const [saveError, setSaveError] = useState(false);
  // How many events the backend has; the review plans only once it has them all.
  const [savedCount, setSavedCount] = useState(state.log.length);
  const saved = useRef(state.log.length);
  const saving = useRef(false);
  const latest = useRef(state.log);
  latest.current = state.log;

  const { log } = state;
  useEffect(() => {
    if (saving.current || saved.current >= log.length) {
      return;
    }
    saving.current = true;
    (async () => {
      try {
        // Events committed while a save is in flight are picked up by the same loop, in order.
        for (let event = latest.current[saved.current]; event !== undefined; ) {
          await backend.recordEvent(session.id, event);
          saved.current += 1;
          setSavedCount(saved.current);
          event = latest.current[saved.current];
        }
        setSaveError(false);
      } catch {
        setSaveError(true);
      } finally {
        saving.current = false;
      }
    })();
  }, [backend, session.id, log]);

  if (screen === "apply" && planId !== null) {
    return (
      <ApplyScreen
        key={planId}
        session={session}
        planId={planId}
        onDone={onExit}
        onApplyRemaining={() => {
          setPlanId(null);
          setScreen("review");
        }}
      />
    );
  }
  if (screen === "review") {
    return (
      <ReviewScreen
        session={session}
        state={state}
        saved={savedCount >= state.log.length}
        saveError={saveError}
        onSessionChanged={setSession}
        onBack={() => setScreen("swipe")}
        onShow={(photoId) => {
          dispatch({ type: "show", photoId });
          setScreen("swipe");
        }}
        onSortUndecided={() => {
          dispatch({ type: "firstUndecided" });
          setScreen("swipe");
        }}
        onConfirmed={(confirmed) => {
          setPlanId(confirmed);
          setScreen("apply");
        }}
      />
    );
  }
  return (
    <SwipeScreen
      session={session}
      state={state}
      dispatch={dispatch}
      onReview={() => setScreen("review")}
      saveError={saveError}
      onStartOver={
        session.resumed ? async () => onRestarted(await backend.startOver(session.id)) : undefined
      }
    />
  );
}
