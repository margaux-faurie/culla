import { useEffect, useRef } from "react";

export interface ApplyPlaceholderProps {
  /** Back to the start screen; the confirmed plan stays as the backend recorded it. */
  readonly onDone: () => void;
}

/**
 * Stand-in for the apply screen (US-36, the next M4 slice): the confirm dialog leads somewhere. In
 * the web demo the apply is simulated and no file is touched (SPEC §5.5).
 */
export function ApplyPlaceholder({ onDone }: ApplyPlaceholderProps) {
  const titleRef = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    titleRef.current?.focus();
  }, []);
  return (
    <main className="review" aria-labelledby="apply-title">
      <h1 id="apply-title" ref={titleRef} className="title" tabIndex={-1}>
        Plan confirmed
      </h1>
      <p className="lead">
        The apply screen, with progress, pause and stop, comes in a later version. Nothing has
        moved: the web demo never touches a file.
      </p>
      <div>
        <button type="button" className="button button-outline" onClick={onDone}>
          Back to the start
        </button>
      </div>
    </main>
  );
}
