import React, { useState } from "react";

import type { Session } from "../protocol";
import { useDaemon } from "../store";
import { Modal } from "./Modal";

export function RenameSessionDialog({ session, onClose }: { session: Session; onClose: () => void }): React.ReactElement {
  const { request } = useDaemon();
  const [title, setTitle] = useState(session.title);
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

  const submit = async () => {
    if (!title.trim()) {
      setError("Title is required.");
      return;
    }
    setSaving(true);
    setError(undefined);
    try {
      await request({ type: "rename_session", session: session.id, title });
      onClose();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const footer = (
    <>
      <button type="button" onClick={onClose} disabled={saving}>
        Cancel
      </button>
      <button type="submit" disabled={saving}>
        Save
      </button>
    </>
  );

  return (
    <Modal title="Rename session" onClose={onClose} footer={footer} onSubmit={submit}>
      <label className="field">
        <span>Title</span>
        <input type="text" value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
      </label>
      {error && <p className="error-text">{error}</p>}
    </Modal>
  );
}
