import { useEffect, useId, useRef, useState } from "react";
import { useBackend } from "../backend/context";
import { isNotReady } from "../backend/errors";
import type {
  DestinationSummary,
  KeepMode,
  RouteExample,
  SessionInfo,
  SourceDetails,
  SourceStatus,
  SourceSummary,
} from "../backend/types";
import { RouteView } from "../route/RouteView";

export interface StartScreenProps {
  readonly onStarted: (session: SessionInfo) => void;
}

type Recents =
  | { readonly kind: "loading" }
  | { readonly kind: "ready"; readonly sources: readonly SourceSummary[] }
  | { readonly kind: "error" };

type Details =
  | { readonly kind: "none" }
  | { readonly kind: "loading" }
  | { readonly kind: "ready"; readonly value: SourceDetails }
  | { readonly kind: "failed" };

type LastDetails = { readonly sourceId: string; readonly value: SourceDetails };

/** The alert shown when describing the folder fails; cleared once a later describe succeeds. */
const READ_FAILED = "Could not read this folder.";

/**
 * Start (SPEC §4.2, US-21, US-44): pick the folder, the options and the destination on one screen,
 * with the route view showing where kept and rejected photos go as the options are set. Move or
 * copy is chosen fresh each session: nothing is preselected and Start stays disabled until then.
 * Resuming or starting fresh on a folder with a session (SPEC §4.2) is not built yet (SPEC §10).
 */
export function StartScreen({ onStarted }: StartScreenProps) {
  const backend = useBackend();
  const hintId = useId();
  const [recents, setRecents] = useState<Recents>({ kind: "loading" });
  const [source, setSource] = useState<SourceSummary | null>(null);
  const [details, setDetails] = useState<Details>({ kind: "none" });
  // The last details read for a source, kept while they load again after a subfolder toggle, so
  // the rejected folder and its "new folder" mark do not flash.
  const [lastDetails, setLastDetails] = useState<LastDetails | null>(null);
  const [example, setExample] = useState<RouteExample | null>(null);
  const [includeSubfolders, setIncludeSubfolders] = useState(true);
  const [rawFollows, setRawFollows] = useState(true);
  const [keepMode, setKeepMode] = useState<KeepMode | null>(null);
  const [destination, setDestination] = useState<DestinationSummary | null>(null);
  const [sameAsLastTime, setSameAsLastTime] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Desktop mode until the Tauri commands land: a notice instead of error messages.
  const [notReady, setNotReady] = useState(false);
  // Set once the user picks a destination for this folder, so a late proposal never replaces it.
  const destinationChosen = useRef(false);

  const fail = (cause: unknown, message: string) => {
    if (isNotReady(cause)) {
      setNotReady(true);
    } else {
      setError(message);
    }
  };
  const failRef = useRef(fail);
  failRef.current = fail;

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

  const sourceId = source?.id ?? null;
  const destinationId = destination?.id ?? null;

  useEffect(() => {
    if (sourceId === null) {
      return;
    }
    let active = true;
    destinationChosen.current = false;
    setDestination(null);
    setSameAsLastTime(false);
    setExample(null);
    backend.proposeDestination(sourceId).then(
      (proposal) => {
        if (active && proposal !== null && !destinationChosen.current) {
          setDestination(proposal.destination);
          setSameAsLastTime(true);
        }
      },
      (cause: unknown) => active && isNotReady(cause) && setNotReady(true),
    );
    return () => {
      active = false;
    };
  }, [backend, sourceId]);

  useEffect(() => {
    if (sourceId === null) {
      return;
    }
    let active = true;
    setDetails({ kind: "loading" });
    backend.describeSource(sourceId, includeSubfolders).then(
      (value) => {
        if (active) {
          setDetails({ kind: "ready", value });
          setLastDetails({ sourceId, value });
          // A folder read that failed before (toggling subfolders, say) no longer applies.
          setError((shown) => (shown === READ_FAILED ? null : shown));
        }
      },
      (cause: unknown) => {
        if (active) {
          setDetails({ kind: "failed" });
          failRef.current(cause, READ_FAILED);
        }
      },
    );
    return () => {
      active = false;
    };
  }, [backend, sourceId, includeSubfolders]);

  // The example keeps showing while a new one loads, so the route does not flash as options change.
  useEffect(() => {
    if (sourceId === null) {
      return;
    }
    let active = true;
    const query = { sourceId, includeSubfolders, rawFollows, keepMode, destinationId };
    backend.exampleRoute(query).then(
      (value) => active && setExample(value),
      (cause: unknown) => {
        if (active) {
          setExample(null);
          if (isNotReady(cause)) {
            setNotReady(true);
          }
        }
      },
    );
    return () => {
      active = false;
    };
  }, [backend, sourceId, includeSubfolders, rawFollows, keepMode, destinationId]);

  const chooseFolder = async () => {
    setError(null);
    try {
      const picked = await backend.chooseSource();
      if (picked !== null) {
        setSource(picked);
      }
    } catch (cause) {
      fail(cause, "Could not open the folder dialog.");
    }
  };

  const chooseDestination = async () => {
    setError(null);
    try {
      const picked = await backend.chooseDestination();
      if (picked !== null) {
        destinationChosen.current = true;
        setDestination(picked);
        setSameAsLastTime(false);
      }
    } catch (cause) {
      fail(cause, "Could not open the folder dialog.");
    }
  };

  const hint = startHint(source, details, keepMode, destination);
  const canStart = hint === null && !starting;

  const start = async () => {
    if (!canStart || source === null || keepMode === null || destination === null) {
      return;
    }
    setStarting(true);
    setError(null);
    try {
      onStarted(
        await backend.startSession({
          sourceId: source.id,
          includeSubfolders,
          rawFollows,
          keepMode,
          destinationId: destination.id,
        }),
      );
    } catch (cause) {
      fail(cause, "Could not start the session.");
      setStarting(false);
    }
  };

  const ready =
    details.kind === "ready"
      ? details.value
      : details.kind === "loading" && lastDetails?.sourceId === sourceId
        ? lastDetails.value
        : null;

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
          <button type="button" className="button button-primary" onClick={chooseFolder}>
            Choose a folder
          </button>
          <h2 className="subtitle">Recent folders</h2>
          <RecentList
            recents={recents}
            selectedId={sourceId}
            onSelect={(picked) => {
              setError(null);
              setSource(picked);
            }}
          />
        </section>

        <section className="panel" aria-labelledby="route-title">
          <div className="panel-head">
            <h2 id="route-title" className="panel-title">
              Where your photos will go
            </h2>
            <span className="small">Nothing moves until you review and apply</span>
          </div>

          {source === null ? (
            <p className="lead">Choose a folder to see where your photos will go.</p>
          ) : (
            <RouteView
              source={source}
              photoSummary={photoSummary(source, details)}
              example={example}
              rejectedFolder={ready?.rejectedFolder ?? null}
              rejectedFolderExists={ready?.rejectedFolderExists ?? null}
              destination={destination}
              keepMode={keepMode}
              originControls={
                <label className="check">
                  <input
                    type="checkbox"
                    checked={includeSubfolders}
                    onChange={(e) => setIncludeSubfolders(e.currentTarget.checked)}
                  />
                  Include subfolders
                </label>
              }
              keepControls={
                <KeepControls
                  keepMode={keepMode}
                  onKeepMode={setKeepMode}
                  destination={destination}
                  sameAsLastTime={sameAsLastTime}
                  onChooseDestination={chooseDestination}
                />
              }
            />
          )}

          <div className="panel-foot">
            <label className="check">
              <input
                type="checkbox"
                checked={rawFollows}
                onChange={(e) => setRawFollows(e.currentTarget.checked)}
              />
              RAW and edit files follow their JPEG
            </label>
            <span className="start-go">
              <output id={hintId} className="small">
                {hint ?? ""}
              </output>
              <button
                type="button"
                className="button button-primary button-large"
                disabled={!canStart}
                aria-describedby={hint === null ? undefined : hintId}
                onClick={start}
              >
                Start sorting
              </button>
            </span>
          </div>
          {error !== null && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
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

function startHint(
  source: SourceSummary | null,
  details: Details,
  keepMode: KeepMode | null,
  destination: DestinationSummary | null,
): string | null {
  if (source === null) {
    return "Choose a folder first";
  }
  if (!source.connected) {
    return "Connect this folder's drive to sort it";
  }
  if (details.kind === "failed") {
    return "Could not read this folder";
  }
  if (details.kind !== "ready") {
    return "Reading the folder";
  }
  if (details.value.photoCount === 0) {
    return "This folder has no photos";
  }
  if (keepMode === null) {
    return "Choose moved or copied first";
  }
  if (destination === null) {
    return "Choose a destination first";
  }
  if (!destination.available) {
    return "Choose a destination that is connected";
  }
  return null;
}

function photoSummary(source: SourceSummary, details: Details): string {
  if (!source.connected) {
    return `${source.photoCount} photos · not connected`;
  }
  if (details.kind === "failed") {
    return "could not count the photos";
  }
  if (details.kind !== "ready") {
    return "Counting photos";
  }
  const { value } = details;
  const photos = `${value.photoCount} ${value.photoCount === 1 ? "photo" : "photos"}`;
  if (!value.includeSubfolders) {
    return `${photos} · subfolders left out`;
  }
  return value.subfolders.length === 0
    ? photos
    : `${photos}, also in ${value.subfolders.join(" and ")}`;
}

interface KeepControlsProps {
  readonly keepMode: KeepMode | null;
  readonly onKeepMode: (mode: KeepMode) => void;
  readonly destination: DestinationSummary | null;
  readonly sameAsLastTime: boolean;
  readonly onChooseDestination: () => void;
}

function KeepControls(props: KeepControlsProps) {
  const { keepMode, destination } = props;
  return (
    <div className="keep-controls">
      <fieldset className="choice">
        <legend className="choice-legend">Kept photos are</legend>
        {(["move", "copy"] as const).map((mode) => (
          <label key={mode} className="choice-option">
            <input
              type="radio"
              name="keep-mode"
              value={mode}
              checked={keepMode === mode}
              onChange={() => props.onKeepMode(mode)}
            />
            <span>{mode === "move" ? "Moved" : "Copied"}</span>
          </label>
        ))}
      </fieldset>
      <div className="destination">
        <span className="small">to</span>
        {destination === null ? (
          <button
            type="button"
            className="button button-outline"
            onClick={props.onChooseDestination}
          >
            Choose a destination
          </button>
        ) : (
          <>
            <span className={destination.available ? "route-place" : "route-place unavailable"}>
              {destination.folder.path}
            </span>
            <button
              type="button"
              className="button button-outline"
              onClick={props.onChooseDestination}
            >
              Change
            </button>
          </>
        )}
      </div>
      {destination !== null && (
        <span className="route-note">
          {destination.folder.volume}
          {props.sameAsLastTime && (
            <>
              {" · "}
              <b>same as last time</b>
            </>
          )}
          {!destination.available && (
            <>
              {" · "}
              <b>not available</b>: the drive is not connected. Choose another destination.
            </>
          )}
        </span>
      )}
      <span className="route-note">Choose moved or copied each time.</span>
    </div>
  );
}

function RecentList(props: {
  readonly recents: Recents;
  readonly selectedId: string | null;
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
          <button
            type="button"
            className="recent"
            aria-pressed={s.id === props.selectedId}
            onClick={() => props.onSelect(s)}
          >
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
