import { useEffect, useReducer, useRef, useState } from "react";
import { useBackend } from "../backend/context";
import type { SessionInfo } from "../backend/types";
import { ApplyPlaceholder } from "../review/ApplyPlaceholder";
import { ReviewScreen } from "../review/ReviewScreen";
import { SwipeScreen } from "../swipe/SwipeScreen";
import { initialState, swipeReducer } from "../swipe/session";

/**
 * One session: the swipe reducer, autosave, and the switch between swipe and review. The reducer
 * commits first; each new event is then mirrored to the backend in order (ADR 0005, architecture
 * §6.2). A failed save keeps the decision and is retried, from the first unsaved event, on the next
 * change.
 */
export interface SessionViewProps {
  readonly session: SessionInfo;
  /** Leaves the session for the start screen. */
  readonly onExit: () => void;
}

export function SessionView({ session, onExit }: SessionViewProps) {
  const backend = useBackend();
  const [state, dispatch] = useReducer(swipeReducer, session.photos, (photos) =>
    initialState(photos.map((p) => p.id)),
  );
  const [screen, setScreen] = useState<"swipe" | "review" | "confirmed">("swipe");
  const [saveError, setSaveError] = useState(false);
  // How many events the backend has; the review plans only once it has them all.
  const [savedCount, setSavedCount] = useState(0);
  const saved = useRef(0);
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

  if (screen === "confirmed") {
    return <ApplyPlaceholder onDone={onExit} />;
  }
  if (screen === "review") {
    return (
      <ReviewScreen
        session={session}
        state={state}
        saved={savedCount >= state.log.length}
        saveError={saveError}
        onBack={() => setScreen("swipe")}
        onShow={(photoId) => {
          dispatch({ type: "show", photoId });
          setScreen("swipe");
        }}
        onSortUndecided={() => {
          dispatch({ type: "firstUndecided" });
          setScreen("swipe");
        }}
        onConfirmed={() => setScreen("confirmed")}
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
    />
  );
}
