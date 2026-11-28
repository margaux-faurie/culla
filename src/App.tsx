import { useEffect, useState } from "react";
import { ApplyScreen } from "./apply/ApplyScreen";
import { useBackend } from "./backend/context";
import type { InterruptedApply, SessionInfo } from "./backend/types";
import { SessionView } from "./session/SessionView";
import { StartScreen } from "./start/StartScreen";

export function App() {
  const backend = useBackend();
  const [session, setSession] = useState<SessionInfo | null>(null);
  // An apply found interrupted at launch comes before the rest (US-38, file-engine §10.1);
  // undefined while the backend is asked.
  const [interrupted, setInterrupted] = useState<InterruptedApply | null | undefined>(undefined);

  useEffect(() => {
    let current = true;
    backend.interruptedApply().then(
      (found) => {
        if (current) setInterrupted(found);
      },
      // Not available (the app before its command lands): there is nothing to finish.
      () => {
        if (current) setInterrupted(null);
      },
    );
    return () => {
      current = false;
    };
  }, [backend]);

  // The start screen shows while the backend answers (a moment): keys typed meanwhile still land.
  if (interrupted !== null && interrupted !== undefined) {
    return (
      <ApplyScreen
        key={interrupted.planId}
        session={interrupted.session}
        planId={interrupted.planId}
        recovery
        onDone={() => setInterrupted(null)}
      />
    );
  }
  return session === null ? (
    <StartScreen onStarted={setSession} />
  ) : (
    <SessionView
      key={session.id}
      session={session}
      onExit={() => setSession(null)}
      onRestarted={setSession}
    />
  );
}
