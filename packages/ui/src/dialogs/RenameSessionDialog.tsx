import React, { useState } from "react";

import type { Session } from "../protocol";
import { useDaemon } from "../store";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { TextInput } from "./TextInput";

export function RenameSessionDialog({ session, onClose }: { session: Session; onClose: () => void }): React.ReactElement {
  const { request } = useDaemon();
  const [title, setTitle] = useState(session.title);
  const { error, setError, busy, run } = useDialogAction();

  const submit = () => {
    if (!title.trim()) {
      setError("Title is required.");
      return;
    }
    void run(async () => {
      await request({ type: "rename_session", session: session.id, title });
      onClose();
    });
  };

  return (
    <Dialog title="Rename session" onClose={onClose} submitLabel="Save" busy={busy} onSubmit={submit} size="sm">
      <TextInput label="Title" value={title} onChange={setTitle} autoFocus />
      <DialogError message={error} />
    </Dialog>
  );
}
