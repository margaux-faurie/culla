import { useEffect, useRef } from "react";
import { SHORTCUTS } from "./keys";

export interface HelpDialogProps {
  readonly open: boolean;
  readonly onClose: () => void;
}

/**
 * Shortcut overlay (US-31): a modal dialog listing every key; Esc closes it and focus goes back to
 * where it was. It sits outside the swipe view, so its keys never reach the swipe shortcuts.
 */
export function HelpDialog({ open, onClose }: HelpDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const returnTo = useRef<Element | null>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null) {
      return;
    }
    if (open && !dialog.open) {
      returnTo.current = document.activeElement;
      dialog.showModal();
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
    <dialog ref={ref} className="help" aria-labelledby="help-title" onClose={handleClose}>
      <h2 id="help-title" className="panel-title">
        Keyboard shortcuts
      </h2>
      <p className="small">Letters follow your keyboard layout: press the key labelled so.</p>
      <table className="help-table">
        <thead>
          <tr>
            <th scope="col">Keys</th>
            <th scope="col">Action</th>
          </tr>
        </thead>
        <tbody>
          {SHORTCUTS.map((s) => (
            <tr key={s.action}>
              <td>
                {s.keys.map((k, i) => (
                  <span key={k}>
                    {i > 0 && " + "}
                    <kbd>{k}</kbd>
                  </span>
                ))}
              </td>
              <td>{s.action}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <button type="button" className="button button-outline" onClick={() => ref.current?.close()}>
        Close
      </button>
    </dialog>
  );
}
