import { useEffect, useReducer, useRef, useState } from "react";
import { useBackend } from "../backend/context";
import type { SessionInfo } from "../backend/types";
import { ReviewPlaceholder } from "../review/ReviewPlaceholder";
import { SwipeScreen } from "../swipe/SwipeScreen";
import { initialState, swipeReducer } from "../swipe/session";

/**
 * One session: the swipe reducer, autosave, and the switch between swipe and review. The reducer
 * commits first; each new event is then mirrored to the backend in order (ADR 0005, architecture
 * §6.2). A failed save keeps the decision and is retried, from the first unsaved event, on the next
 * change.
 */
export function SessionView({ session }: { readonly session: SessionInfo }) {
  const backend = useBackend();
  const [state, dispatch] = useReducer(swipeReducer, session.photos, (photos) =>
    initialState(photos.map((p) => p.id)),
  );
  const [screen, setScreen] = useState<"swipe" | "review">("swipe");
  const [saveError, setSaveError] = useState(false);
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

  if (screen === "review") {
    return <ReviewPlaceholder session={session} state={state} onBack={() => setScreen("swipe")} />;
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
