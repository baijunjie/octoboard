import React, { useState } from "react";

import { useT } from "../i18n/react";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "./Dialog";
import { TextInput } from "./TextInput";

/** The one-field dialog that renames a session or a project: the caller says what the field is
 * called, what a blank value is told, and how the new value is sent. */
export function RenameDialog({
  title,
  label,
  initialValue,
  requiredMessage,
  onSubmit,
  onClose,
}: {
  title: string;
  label: string;
  initialValue: string;
  requiredMessage: string;
  /** Sends the new value; the dialog closes when it resolves and shows its rejection otherwise. */
  onSubmit: (value: string) => Promise<void>;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const [value, setValue] = useState(initialValue);
  const { error, busy, run } = useDialogAction();
  const { shown, attempt } = useSubmitValidation();
  const valueError = value.trim() ? undefined : requiredMessage;

  const submit = () => {
    if (!attempt(valueError)) return;
    void run(async () => {
      await onSubmit(value);
      onClose();
    });
  };

  return (
    <Dialog title={title} onClose={onClose} submitLabel={t("common.save")} busy={busy} onSubmit={submit} size="sm">
      <TextInput
        label={label}
        value={value}
        onChange={setValue}
        errorMessage={shown(valueError)}
        autoFocus
      />
      <DialogError message={error} />
    </Dialog>
  );
}
