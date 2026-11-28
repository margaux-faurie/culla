import { useEffect, useRef, useState } from "react";

export interface StartOverDialogProps {
  readonly open: boolean;
  /** Photos decided so far, for the question. */
  readonly decided: number;
  /** Esc or "Keep my decisions": closes, nothing changes. */
  readonly onCancel: () => void;
  /** Puts the decisions aside and starts the folder again; rejects when the backend refuses. */
  readonly onConfirm: () => Promise<void>;
}

/**
 * Asks before "Start over" on a resumed session (US-37, proposed in SPEC §10 "Resume"). Focus
 * starts on "Keep my decisions", so a stray Enter changes nothing. It sits outside the swipe view,
 * so its keys never reach the swipe shortcuts; Esc closes it.
 */
export function StartOverDialog({ open, decided, onCancel, onConfirm }: StartOverDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const keepRef = useRef<HTMLButtonElement>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null) {
      return;
    }
    if (open && !dialog.open) {
      setFailed(false);
      dialog.showModal();
      keepRef.current?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  const confirm = async () => {
    if (busy) {
      return;
    }
    setBusy(true);
    setFailed(false);
    try {
      await onConfirm();
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };

  const photos = decided === 1 ? "1 photo" : `${decided} photos`;
  return (
    <dialog
      ref={ref}
      className="confirm"
      aria-labelledby="start-over-title"
      aria-describedby="start-over-body"
      onCancel={(event: { preventDefault: () => void }) => {
        event.preventDefault();
        if (!busy) onCancel();
      }}
      onClose={() => {
        // A close the browser made on its own (see ConfirmDialog): keep the state in step.
        if (ref.current?.open !== true && open) onCancel();
      }}
    >
      <h2 id="start-over-title" className="panel-title">
        Start this folder over?
      </h2>
      <div id="start-over-body" className="confirm-body">
        <p>
          Your decisions on {photos} are put aside, and every photo starts undecided. No file
          changes.
        </p>
      </div>
      <div className="confirm-actions">
        <button
          ref={keepRef}
          type="button"
          className="button button-outline"
          disabled={busy}
          onClick={onCancel}
        >
          Keep my decisions
        </button>
        <button type="button" className="button button-primary" disabled={busy} onClick={confirm}>
          Start over
        </button>
      </div>
      {failed && (
        <p className="error" role="alert">
          Could not start over. Your decisions are kept.
        </p>
      )}
    </dialog>
  );
}
