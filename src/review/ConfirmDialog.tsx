import { useEffect, useRef } from "react";
import type { ReviewPlan } from "../backend/types";
import { confirmText } from "./model";

export interface ConfirmDialogProps {
  readonly open: boolean;
  readonly plan: ReviewPlan;
  /** Preflight is running again at confirm (§7.4): both buttons wait. */
  readonly busy: boolean;
  /** Esc or "Go back": closes, nothing is recorded or moved. */
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
  /**
   * The browser closed the dialog on its own (Chromium closes a modal on a second Esc even when
   * `cancel` is prevented): the parent syncs its state so Apply can open it again.
   */
  readonly onClosed: () => void;
}

/**
 * The confirm dialog (US-35, ADR 0003): a modal that states the counts and where they go. Focus
 * starts on "Go back" (proposed), so a stray Enter closes it rather than confirming; Tab stays inside
 * while it is open; Esc closes it. It sits outside the review's main element.
 */
export function ConfirmDialog({
  open,
  plan,
  busy,
  onCancel,
  onConfirm,
  onClosed,
}: ConfirmDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const backRef = useRef<HTMLButtonElement>(null);
  const text = confirmText(plan);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null) {
      return;
    }
    if (open && !dialog.open) {
      dialog.showModal();
      backRef.current?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  // Native Esc fires `cancel`; it is handled here so the state, not the browser, closes it.
  const onCancelEvent = (event: { preventDefault: () => void }) => {
    event.preventDefault();
    if (!busy) {
      onCancel();
    }
  };

  // Keeps Tab inside the dialog: from the last button to the first and back (NFR-A11Y-01).
  const onKeyDown = (event: { key: string; shiftKey: boolean; preventDefault: () => void }) => {
    if (event.key !== "Tab") {
      return;
    }
    const dialog = ref.current;
    if (dialog === null) {
      return;
    }
    const buttons = [...dialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    if (first === undefined || last === undefined) {
      event.preventDefault();
      return;
    }
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !dialog.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !dialog.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <dialog
      ref={ref}
      className="confirm"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-body"
      onCancel={onCancelEvent}
      onClose={onClosed}
      onKeyDown={onKeyDown}
    >
      <h2 id="confirm-title" className="panel-title">
        {text.title}
      </h2>
      <div id="confirm-body" className="confirm-body">
        {text.body.map((line) => (
          <p key={line}>{line}</p>
        ))}
      </div>
      <div className="confirm-actions">
        <button
          ref={backRef}
          type="button"
          className="button button-outline"
          disabled={busy}
          onClick={onCancel}
        >
          {text.cancel}
        </button>
        <button type="button" className="button button-primary" disabled={busy} onClick={onConfirm}>
          {busy ? "Checking again" : text.confirm}
        </button>
      </div>
      <p className="small" role="status">
        {busy ? "Checking the drives once more before anything moves." : ""}
      </p>
    </dialog>
  );
}
