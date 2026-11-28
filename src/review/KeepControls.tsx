import type { DestinationSummary, KeepMode } from "../backend/types";

export interface KeepControlsProps {
  readonly keepMode: KeepMode | null;
  readonly destination: DestinationSummary | null;
  readonly sameAsLastTime: boolean;
  readonly onKeepMode: (mode: KeepMode) => void;
  readonly onChooseDestination: () => void;
}

/**
 * The Kept branch's controls on the review screen, right before Apply (decided by Margaux 28 Nov
 * 2026, ADR 0029). Lean review decided by Margaux; this layout proposed, SPEC §10: "→ Move | Copy
 * to <destination> Change" after the branch's "12 kept". Both start as last chosen; the first
 * time neither is set and Apply waits for them. The destination is a display string from the
 * backend, printed as it is.
 */
export function KeepControls(props: KeepControlsProps) {
  const { keepMode, destination } = props;
  return (
    <div className="keep-controls">
      <div className="destination">
        <span className="branch-arrow" aria-hidden="true">
          →
        </span>
        <fieldset className="choice">
          <legend className="visually-hidden">Move or copy the kept photos</legend>
          {(["move", "copy"] as const).map((mode) => (
            <label key={mode} className="choice-option">
              <input
                type="radio"
                name="keep-mode"
                value={mode}
                checked={keepMode === mode}
                onChange={() => props.onKeepMode(mode)}
              />
              <span>{mode === "move" ? "Move" : "Copy"}</span>
            </label>
          ))}
        </fieldset>
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
              className="button button-quiet button-small"
              onClick={props.onChooseDestination}
            >
              Change
            </button>
          </>
        )}
      </div>
      {destination !== null && (
        <span className="branch-note">
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
              <b>not available</b>: the drive is not connected. Connect it, or choose another
              destination.
            </>
          )}
        </span>
      )}
      {keepMode === null && (
        <span className="branch-note">
          Choose move or copy. Culla keeps your choice for next time.
        </span>
      )}
    </div>
  );
}
