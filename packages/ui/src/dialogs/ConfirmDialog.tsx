import { Button, Input, Label, TextField } from "@heroui/react";
import React, { useState } from "react";

import { TitledControl } from "../components/TitledControl";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
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
  cancelLabel,
  extraAction,
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
  const word = typeToConfirm?.toLocaleUpperCase(language);
  const confirmed = word === undefined || typed.trim().toLocaleUpperCase(language) === word;

  const footer = (
    <>
      <Button type="button" variant="secondary" onPress={onCancel} isDisabled={busy}>
        {cancelLabel ?? t("common.cancel")}
      </Button>
      {extraAction && (
        <TitledControl title={extraAction.title}>
          <Button type="button" variant="secondary" onPress={() => void run(extraAction.onClick)} isDisabled={busy}>
            {extraAction.label}
          </Button>
        </TitledControl>
      )}
      <Button type="submit" variant={destructive ? "danger" : "primary"} isDisabled={busy || !confirmed}>
        {confirmLabel ?? t("common.confirm")}
      </Button>
    </>
  );

  return (
    <Dialog title={title} onClose={onCancel} footer={footer} resetKey={resetKey} size={size} onSubmit={() => confirmed && void run(onConfirm)} alert>
      {typeof message === "string" ? (
        <p className="text-sm">{message}</p>
      ) : (
        <div className="flex flex-col gap-3 text-sm text-foreground">{message}</div>
      )}
      {word !== undefined && (
        <TextField
          fullWidth
          variant="secondary"
          value={typed}
          onChange={setTyped}
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
