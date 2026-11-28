import { useEffect, useRef, useState } from "react";
import { useBackend } from "../backend/context";
import { isNotReady } from "../backend/errors";
import type { SessionInfo, SourceStatus, SourceSummary } from "../backend/types";

export interface StartScreenProps {
  readonly onStarted: (session: SessionInfo) => void;
}

type Recents =
  | { readonly kind: "loading" }
  | { readonly kind: "ready"; readonly sources: readonly SourceSummary[] }
  | { readonly kind: "error" };

/**
 * Start (SPEC §4.2, US-21, US-44; decided by Margaux 28 Nov 2026): "What do you want to sort?",
 * the folder dialog and the recent folders. Choosing a folder starts sorting at once; the backend
 * fills in the defaults, and move or copy and the destination are set on the review screen. No
 * options here: subfolders are always included (decided by Margaux 28 Nov 2026).
 * Resuming or starting fresh on a folder with a session (SPEC §4.2) is not built yet (SPEC §10).
 */
export function StartScreen({ onStarted }: StartScreenProps) {
  const backend = useBackend();
  const [recents, setRecents] = useState<Recents>({ kind: "loading" });
  const [error, setError] = useState<string | null>(null);
  // Desktop mode until the Tauri commands land: a notice instead of error messages.
  const [notReady, setNotReady] = useState(false);
  // A second activation while a start is in flight does nothing.
  const starting = useRef(false);

  useEffect(() => {
    let active = true;
    backend.recentSources().then(
      (sources) => active && setRecents({ kind: "ready", sources }),
      (cause: unknown) => {
        if (active) {
          setRecents(isNotReady(cause) ? { kind: "ready", sources: [] } : { kind: "error" });
          if (isNotReady(cause)) {
            setNotReady(true);
          }
        }
      },
    );
    return () => {
      active = false;
    };
  }, [backend]);

  const fail = (cause: unknown, message: string) => {
    if (isNotReady(cause)) {
      setNotReady(true);
    } else {
      setError(message);
    }
  };

  const start = async (source: SourceSummary) => {
    if (!source.connected) {
      setError("Connect this folder's drive to sort it.");
      return;
    }
    try {
      const session = await backend.startSession({ sourceId: source.id, includeSubfolders: true });
      if (session.photos.length === 0) {
        setError("This folder has no photos.");
        return;
      }
      onStarted(session);
    } catch (cause) {
      fail(cause, "Could not start sorting this folder.");
    }
  };

  const run = async (task: () => Promise<void>) => {
    if (starting.current) {
      return;
    }
    starting.current = true;
    setError(null);
    try {
      await task();
    } finally {
      starting.current = false;
    }
  };

  const chooseFolder = () =>
    run(async () => {
      let picked: SourceSummary | null;
      try {
        picked = await backend.chooseSource();
      } catch (cause) {
        fail(cause, "Could not open the folder dialog.");
        return;
      }
      if (picked !== null) {
        await start(picked);
      }
    });

  return (
    <div className="start">
      <header className="start-header">
        <span className="brand">Culla</span>
      </header>
      <main className="start-main">
        {/* Mounted from the start and filled later, so screen readers announce it. */}
        <output className="notice">
          {notReady && (
            <span>
              Not ready yet. Sorting in the app arrives with its next versions; the web demo shows
              how it will work.
            </span>
          )}
        </output>
        <section className="start-pick" aria-labelledby="start-title">
          <h1 id="start-title" className="title">
            What do you want to sort?
          </h1>
          <p className="lead">Nothing moves until you review and apply.</p>
          <button type="button" className="button button-primary" onClick={chooseFolder}>
            Choose a folder
          </button>
          {error !== null && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
          <h2 className="subtitle">Recent folders</h2>
          <RecentList recents={recents} onSelect={(source) => run(() => start(source))} />
        </section>
      </main>
      {/* Demo only, after the main content so it is not the first tab stop. */}
      {import.meta.env.MODE === "demo" && (
        <footer className="start-footer">
          <a href="/install">Download and install</a>
        </footer>
      )}
    </div>
  );
}

function RecentList(props: {
  readonly recents: Recents;
  readonly onSelect: (source: SourceSummary) => void;
}) {
  const { recents } = props;
  if (recents.kind === "loading") {
    return <p className="small">Loading</p>;
  }
  if (recents.kind === "error") {
    return <p className="small">Could not load recent folders.</p>;
  }
  if (recents.sources.length === 0) {
    return <p className="small">Folders you sort appear here.</p>;
  }
  return (
    <ul className="recents">
      {recents.sources.map((s) => (
        <li key={s.id}>
          <button type="button" className="recent" onClick={() => props.onSelect(s)}>
            <span className="recent-top">
              <span className="recent-name">{s.name}</span>
              <StatusPill status={s.status} connected={s.connected} />
            </span>
            <span className="small">
              {s.folder.path} · {s.photoCount} photos
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function StatusPill({ status, connected }: { status: SourceStatus; connected: boolean }) {
  if (!connected) {
    return <span className="pill pill-outline">Not connected</span>;
  }
  switch (status.kind) {
    case "inProgress":
      return <span className="pill pill-filled">In progress</span>;
    case "applied":
      return <span className="pill pill-outline">Applied {status.on}</span>;
    case "new":
      return null;
  }
}
