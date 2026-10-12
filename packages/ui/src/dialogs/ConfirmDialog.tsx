import { Button, Input, Label, TextField } from "@heroui/react";
import React, { useState } from "react";

import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { useTrimmedField } from "./useTrimmedField";

export interface ConfirmDialogProps {
  title: string;
  /** What the dialog says: a string is one paragraph; a node lays out its own structure. */
  message: React.ReactNode;
  confirmLabel?: string;
  /** What the confirm button reads while `onConfirm` is running, for an action that takes seconds. */
  pendingLabel?: string;
  /** What the button that declines is called. */
  cancelLabel?: string;
  destructive?: boolean;
  /** A control shown under the message that settles what Confirm does, such as a checkbox widening
   * what the action covers; the caller holds its state and reads it in `onConfirm`. */
  extraControl?: React.ReactNode;
  /** Names what the dialog currently asks about, for one reused across subjects: a change clears
   * the failure shown for the previous one. */
  resetKey?: string;
  size?: "sm" | "md" | "lg";
  /** For an action that cannot be undone and loses a lot: the word the user has to type before
   * Confirm is enabled, shown in capitals. What is typed shows in capitals too, and is compared
   * ignoring case, so a script without case (Chinese) works unchanged. */
  typeToConfirm?: string;
  /** May reject — the dialog shows the failure inline and stays open instead of closing, so the
   * caller does not need its own try/catch around the request. */
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  pendingLabel,
  cancelLabel,
  extraControl,
  destructive,
  resetKey,
  size,
  typeToConfirm,
  onConfirm,
  onCancel,
}: ConfirmDialogProps): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { error, busy, run } = useDialogAction(resetKey);
  const [typed, setTyped] = useState("");
  const typedField = useTrimmedField(typed, setTyped);
  const word = typeToConfirm?.toLocaleUpperCase(language);
  const confirmed = word === undefined || typed.toLocaleUpperCase(language) === word;

  const footer = (
    <>
      <Button type="button" variant="secondary" onPress={onCancel} isDisabled={busy}>
        {cancelLabel ?? t("common.cancel")}
      </Button>
      {/* Pending rather than disabled while it runs: a natively disabled button drops the focus the
          press gave it, and the dialog's Escape and Tab containment go with it. */}
      <Button type="submit" variant={destructive ? "danger" : "primary"} isPending={busy} isDisabled={!confirmed}>
        {busy && pendingLabel ? pendingLabel : (confirmLabel ?? t("common.confirm"))}
      </Button>
    </>
  );

  return (
    <Dialog title={title} onClose={busy ? () => {} : onCancel} footer={footer} resetKey={resetKey} size={size} onSubmit={() => confirmed && !busy && void run(onConfirm)} alert>
      {typeof message === "string" ? (
        <p className="text-sm">{message}</p>
      ) : (
        <div className="flex flex-col gap-3 text-sm text-foreground">{message}</div>
      )}
      {extraControl}
      {word !== undefined && (
        <TextField
          fullWidth
          variant="secondary"
          {...typedField}
          autoFocus
        >
          <Label>
            <Message id="dialog.typeToConfirm.label" params={{ word: <strong className="font-semibold">{word}</strong> }} />
          </Label>
          {/* Capitals by styling rather than by rewriting the value: rewriting a controlled value
              mid-composition breaks an input method's composition. */}
          <Input className="uppercase" autoComplete="off" spellCheck={false} />
        </TextField>
      )}
      <DialogError message={error} />
    </Dialog>
  );
}
