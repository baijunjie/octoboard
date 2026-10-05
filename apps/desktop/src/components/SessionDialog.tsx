import React, { useState } from "react";

import { AGENT_OPTIONS } from "../agents";
import type { Agent, Console, Project } from "../protocol";
import { useDaemon } from "../store";
import { Dropdown } from "./Dropdown";
import { Modal } from "./Modal";

interface SessionDialogProps {
  console: Console;
  project: Project;
  onClose: () => void;
  onOpened: (sessionId: string) => void;
}

/**
 * Opens a session manually under a project. The agent picker defaults per the "Which agent a
 * session uses" section of docs/product/sessions.md (project default, then console default) but
 * the user can override it for this one session, matching `open_session`'s own `agent` field.
 *
 * `open_session` answers with `session_opened`, which names the session it started — the broadcast
 * that puts it in the tree carries no request id, so this reply is the only way to tell which of
 * the sessions appearing there is ours to select.
 */
export function SessionDialog({
  console: parentConsole,
  project,
  onClose,
  onOpened,
}: SessionDialogProps): React.ReactElement {
  const { request } = useDaemon();
  const [agent, setAgent] = useState<Agent>(project.default_agent ?? parentConsole.default_agent);
  const [title, setTitle] = useState("");
  const [task, setTask] = useState("");
  const [includeInHub, setIncludeInHub] = useState(false);
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

  const submit = async () => {
    setSaving(true);
    setError(undefined);
    try {
      const reply = await request({
        type: "open_session",
        console_id: parentConsole.id,
        project_id: project.id,
        agent,
        task: task || undefined,
        title: title || undefined,
        include_in_hub: includeInHub || undefined,
      });
      if (reply.type === "session_opened") onOpened(reply.session.id);
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
        Open
      </button>
    </>
  );

  return (
    <Modal title={`Open session in ${project.name}`} onClose={onClose} footer={footer} onSubmit={submit}>
      <label className="field">
        <span>Agent</span>
        <Dropdown options={AGENT_OPTIONS} value={agent} onChange={setAgent} aria-label="Agent" />
      </label>
      <label className="field">
        <span>Title (optional)</span>
        <input type="text" value={title} onChange={(e) => setTitle(e.target.value)} />
      </label>
      <label className="field">
        <span>Initial task (optional)</span>
        <textarea value={task} onChange={(e) => setTask(e.target.value)} rows={4} />
      </label>
      <label className="field field-checkbox">
        <input type="checkbox" checked={includeInHub} onChange={(e) => setIncludeInHub(e.target.checked)} />
        <span className="field-checkbox-text">
          <span>Include in hub</span>
          <span className="field-help">
            Reports this session's results to the console's hub, instead of staying outside the orchestration.
          </span>
        </span>
      </label>
      {error && <p className="error-text">{error}</p>}
    </Modal>
  );
}
