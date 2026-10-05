import React, { useState } from "react";

import { Modal } from "./Modal";

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel?: string;
  /** What the button that declines is called. */
  cancelLabel?: string;
  destructive?: boolean;
  /** A third action beside Cancel and Confirm, run and reported on the same way as Confirm. */
  extraAction?: { label: string; title?: string; onClick: () => Promise<void> };
  /** May reject — the dialog shows the failure inline and stays open instead of closing, so the
   * caller does not need its own try/catch around the request. */
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  extraAction,
  destructive,
  onConfirm,
  onCancel,
}: ConfirmDialogProps): React.ReactElement {
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(undefined);
    try {
      await action();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const handleConfirm = () => run(onConfirm);

  const footer = (
    <>
      <button type="button" onClick={onCancel} disabled={busy}>
        {cancelLabel}
      </button>
      {extraAction && (
        <button type="button" title={extraAction.title} onClick={() => run(extraAction.onClick)} disabled={busy}>
          {extraAction.label}
        </button>
      )}
      <button type="submit" className={destructive ? "button-destructive" : undefined} disabled={busy}>
        {confirmLabel}
      </button>
    </>
  );
  return (
    <Modal title={title} onClose={onCancel} footer={footer} onSubmit={handleConfirm}>
      <p>{message}</p>
      {error && <p className="error-text">{error}</p>}
    </Modal>
  );
}
