import React from "react";

import { useEscapeStack } from "../hooks/useEscapeStack";

interface ModalProps {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  footer?: React.ReactNode;
  /** When set, the body and footer render inside a `<form>` so Enter in a text field submits it —
   * the dialog decides what "submit" means (create/save/confirm). Omit for a dialog with nothing
   * to submit. */
  onSubmit?: () => void;
}

/** A plain in-page modal. Used instead of `window.confirm`/`window.alert` so every dialog in the
 * app — including the exit confirmation — is driven by the same DOM and styling. */
export function Modal({ title, onClose, children, footer, onSubmit }: ModalProps): React.ReactElement {
  // Registered as a layer rather than listening for Escape directly, so only the topmost open
  // layer closes: a picker opened from a form, or the exit confirmation over an open dialog, would
  // otherwise both react to the same keypress.
  useEscapeStack(true, onClose);

  const body = (
    <>
      <div className="modal-body">{children}</div>
      {footer && <div className="modal-footer">{footer}</div>}
    </>
  );

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title}>
        <div className="modal-header">
          <h2>{title}</h2>
          <button type="button" className="modal-close" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        {onSubmit ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              onSubmit();
            }}
          >
            {body}
          </form>
        ) : (
          body
        )}
      </div>
    </div>
  );
}
