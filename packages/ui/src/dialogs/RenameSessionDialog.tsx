import React, { useState } from "react";

import { useT } from "../i18n/react";
import type { Session } from "../protocol";
import { useDaemon } from "../store";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { TextInput } from "./TextInput";

export function RenameSessionDialog({ session, onClose }: { session: Session; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [title, setTitle] = useState(session.title);
  const { error, setError, busy, run } = useDialogAction();

  const submit = () => {
    if (!title.trim()) {
      setError(t("dialog.rename.titleRequired"));
      return;
    }
    void run(async () => {
      await request({ type: "rename_session", session: session.id, title });
      onClose();
    });
  };

  return (
    <Dialog title={t("dialog.rename.title")} onClose={onClose} submitLabel={t("common.save")} busy={busy} onSubmit={submit} size="sm">
      <TextInput label={t("dialog.rename.field")} value={title} onChange={setTitle} autoFocus />
      <DialogError message={error} />
    </Dialog>
  );
}
