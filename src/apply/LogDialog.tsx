import { useEffect, useRef } from "react";

export interface LogDialogProps {
  /** The log's text, or null when the dialog is closed. */
  readonly text: string | null;
  readonly onClose: () => void;
}

/**
 * The web demo's "Open log" (US-36): the app opens the log in the default text editor by its run
 * id and the webview never sees its path (threat model T7); the demo has no editor, so it shows
 * the text the backend rendered, as the core renders it (file-engine §9.8). Esc closes it and
 * focus goes back to where it was.
 */
export function LogDialog({ text, onClose }: LogDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const returnTo = useRef<Element | null>(null);
  const open = text !== null;

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null) {
      return;
    }
    if (open && !dialog.open) {
      returnTo.current = document.activeElement;
      dialog.showModal();
      closeRef.current?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  const handleClose = () => {
    const target = returnTo.current;
    returnTo.current = null;
    onClose();
    if (target instanceof HTMLElement) {
      target.focus();
    }
  };

  return (
    <dialog
      ref={ref}
      className="confirm log-dialog"
      aria-labelledby="log-title"
      aria-describedby="log-note"
      onClose={handleClose}
    >
      <h2 id="log-title" className="panel-title">
        Apply log
      </h2>
      <p id="log-note" className="small">
        In the app this opens in your text editor. The web demo moved nothing: this log is
        simulated.
      </p>
      {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a named region that scrolls on its own takes focus so the keyboard can scroll it (WCAG 2.1.1, axe scrollable-region-focusable). */}
      <section className="log-text" tabIndex={0} aria-label="Log text">
        <pre>{text ?? ""}</pre>
      </section>
      <div className="confirm-actions">
        <button
          ref={closeRef}
          type="button"
          className="button button-outline"
          onClick={() => ref.current?.close()}
        >
          Close
        </button>
      </div>
    </dialog>
  );
}
