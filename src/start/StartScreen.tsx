import { useEffect, useId, useState } from "react";
import { useBackend } from "../backend/context";
import type {
  DestinationSummary,
  KeepMode,
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

/**
 * Start (SPEC §4.2, US-21, US-44): pick the folder, the options and the destination on one screen,
 * with the route view showing where kept and rejected photos go as the options are set. Move or
 * copy is chosen fresh each session: nothing is preselected and Start stays disabled until then.
 */
export function StartScreen({ onStarted }: StartScreenProps) {
  const backend = useBackend();
  const hintId = useId();
  const [recents, setRecents] = useState<Recents>({ kind: "loading" });
  const [source, setSource] = useState<SourceSummary | null>(null);
  const [details, setDetails] = useState<SourceDetails | null>(null);
  const [includeSubfolders, setIncludeSubfolders] = useState(true);
  const [rawFollows, setRawFollows] = useState(true);
  const [keepMode, setKeepMode] = useState<KeepMode | null>(null);
  const [destination, setDestination] = useState<DestinationSummary | null>(null);
  const [sameAsLastTime, setSameAsLastTime] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    backend.recentSources().then(
      (sources) => active && setRecents({ kind: "ready", sources }),
      () => active && setRecents({ kind: "error" }),
    );
    return () => {
      active = false;
    };
  }, [backend]);

  const sourceId = source?.id ?? null;

  useEffect(() => {
    if (sourceId === null) {
      return;
    }
    let active = true;
    setDestination(null);
    setSameAsLastTime(false);
    backend.proposeDestination(sourceId).then(
      (proposal) => {
        if (active && proposal !== null) {
          setDestination(proposal.destination);
          setSameAsLastTime(true);
        }
      },
      () => undefined,
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
    setDetails(null);
    backend.describeSource(sourceId, includeSubfolders).then(
      (value) => active && setDetails(value),
      () => active && setError("Could not read this folder."),
    );
    return () => {
      active = false;
    };
  }, [backend, sourceId, includeSubfolders]);

  const chooseFolder = async () => {
    setError(null);
    try {
      const picked = await backend.chooseSource();
      if (picked !== null) {
        setSource(picked);
      }
    } catch {
      setError("Could not open the folder dialog.");
    }
  };

  const chooseDestination = async () => {
    setError(null);
    try {
      const picked = await backend.chooseDestination();
      if (picked !== null) {
        setDestination(picked);
        setSameAsLastTime(false);
      }
    } catch {
      setError("Could not open the folder dialog.");
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
    } catch {
      setError("Could not start the session.");
      setStarting(false);
    }
  };

  return (
    <div className="start">
      <header className="start-header">
        <span className="brand">Culla</span>
      </header>
      <main className="start-main">
        <section className="start-pick" aria-labelledby="start-title">
          <h1 id="start-title" className="title">
            What do you want to sort?
          </h1>
          <button type="button" className="button button-primary" onClick={chooseFolder}>
            Choose a folder
          </button>
          <h2 className="subtitle">Recent folders</h2>
          <RecentList recents={recents} selectedId={sourceId} onSelect={setSource} />
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
              example={details?.example ?? null}
              rejectedFolderExists={details?.rejectedFolderExists ?? false}
              destination={destination}
              keepMode={keepMode}
              rawFollows={rawFollows}
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
              {hint !== null && (
                <span id={hintId} className="small">
                  {hint}
                </span>
              )}
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
    </div>
  );
}

function startHint(
  source: SourceSummary | null,
  details: SourceDetails | null,
  keepMode: KeepMode | null,
  destination: DestinationSummary | null,
): string | null {
  if (source === null) {
    return "Choose a folder first";
  }
  if (!source.connected) {
    return "Connect this folder's drive to sort it";
  }
  if (details === null) {
    return "Reading the folder";
  }
  if (details.photoCount === 0) {
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

function photoSummary(source: SourceSummary, details: SourceDetails | null): string {
  if (!source.connected) {
    return `${source.photoCount} photos · not connected`;
  }
  if (details === null) {
    return "Counting photos";
  }
  const photos = `${details.photoCount} ${details.photoCount === 1 ? "photo" : "photos"}`;
  if (!details.includeSubfolders) {
    return `${photos} · subfolders left out`;
  }
  return details.subfolders.length === 0
    ? photos
    : `${photos}, also in ${details.subfolders.join(" and ")}`;
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
