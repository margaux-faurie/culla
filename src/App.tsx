import { useState } from "react";
import type { SessionInfo } from "./backend/types";
import { SessionView } from "./session/SessionView";
import { StartScreen } from "./start/StartScreen";

export function App() {
  const [session, setSession] = useState<SessionInfo | null>(null);
  return session === null ? (
    <StartScreen onStarted={setSession} />
  ) : (
    <SessionView key={session.id} session={session} />
  );
}
