import { Button } from "@heroui/react";
import React from "react";

import { TitledControl } from "../components/TitledControl";
import { Dialog, DialogError, useDialogAction } from "./Dialog";

interface ConfirmDialogProps {
  title: string;
  /** What the dialog says: a string is one paragraph; a node lays out its own structure. */
  message: React.ReactNode;
  confirmLabel?: string;
  /** What the button that declines is called. */
  cancelLabel?: string;
  destructive?: boolean;
  /** A third action beside Cancel and Confirm, run and reported on the same way as Confirm. */
  extraAction?: { label: string; title?: string; onClick: () => Promise<void> };
  /** Names what the dialog currently asks about, for one reused across subjects: a change clears
   * the failure shown for the previous one. */
  resetKey?: string;
  size?: "sm" | "md" | "lg";
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
  resetKey,
  size,
  onConfirm,
  onCancel,
}: ConfirmDialogProps): React.ReactElement {
  const { error, busy, run } = useDialogAction(resetKey);

  const footer = (
    <>
      <Button type="button" variant="secondary" onPress={onCancel} isDisabled={busy}>
        {cancelLabel}
      </Button>
      {extraAction && (
        <TitledControl title={extraAction.title}>
          <Button type="button" variant="secondary" onPress={() => void run(extraAction.onClick)} isDisabled={busy}>
            {extraAction.label}
          </Button>
        </TitledControl>
      )}
      <Button type="submit" variant={destructive ? "danger" : "primary"} isDisabled={busy}>
        {confirmLabel}
      </Button>
    </>
  );

  return (
    <Dialog title={title} onClose={onCancel} footer={footer} resetKey={resetKey} size={size} onSubmit={() => void run(onConfirm)} alert>
      {typeof message === "string" ? (
        <p className="text-sm">{message}</p>
      ) : (
        <div className="flex flex-col gap-3 text-sm text-foreground">{message}</div>
      )}
      <DialogError message={error} />
    </Dialog>
  );
}
