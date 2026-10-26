import React, { useState } from "react";

import { useT } from "../i18n/react";
import { Dialog, useDialogAction, useSubmitValidation } from "./Dialog";
import { TextInput } from "./TextInput";
import { exceedsTextLimit } from "./textLength";

/** The one-field dialog that renames a session or a project: the caller says what the field is
 * called, what a blank value is told, and how the new value is sent. */
export function RenameDialog({
  title,
  errorTitle,
  label,
  initialValue,
  placeholder,
  requiredMessage,
  onSubmit,
  onClose,
  characterLimit,
  tooLongMessage,
}: {
  title: string;
  /** What a failed rename is titled: a plain name for what failed, with no name in it. */
  errorTitle: string;
  label: string;
  initialValue: string;
  placeholder: string;
  requiredMessage: string;
  /** Sends the new value; the dialog closes when it resolves and shows its rejection otherwise. */
  onSubmit: (value: string) => Promise<void>;
  onClose: () => void;
  characterLimit?: number;
  tooLongMessage?: string;
}): React.ReactElement {
  const t = useT();
  const [value, setValue] = useState(initialValue);
  const { busy, run } = useDialogAction(undefined, errorTitle);
  const { shown, attempt } = useSubmitValidation();
  const valueError = !value.trim() ? requiredMessage
    : characterLimit !== undefined && exceedsTextLimit(value, characterLimit, initialValue) ? tooLongMessage : undefined;

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
        placeholder={placeholder}
        onChange={setValue}
        errorMessage={shown(valueError)}
        characterLimit={characterLimit}
        autoFocus
      />
    </Dialog>
  );
}
