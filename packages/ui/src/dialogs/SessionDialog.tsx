import { Checkbox, Description, Label, TextArea, TextField } from "@heroui/react";
import React, { useState } from "react";

import { AGENT_OPTIONS } from "../agents";
import type { Agent, Console, Project } from "../protocol";
import { useDaemon } from "../store";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { OptionSelect } from "./OptionSelect";
import { TextInput } from "./TextInput";

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
}: {
  console: Console;
  project: Project;
  onClose: () => void;
  onOpened: (sessionId: string) => void;
}): React.ReactElement {
  const { request } = useDaemon();
  const [agent, setAgent] = useState<Agent>(project.default_agent ?? parentConsole.default_agent);
  const [title, setTitle] = useState("");
  const [task, setTask] = useState("");
  const [includeInHub, setIncludeInHub] = useState(false);
  const { error, busy, run } = useDialogAction();

  const submit = () =>
    void run(async () => {
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
    });

  return (
    <Dialog
      title={`Open session in ${project.name}`}
      onClose={onClose}
      submitLabel="Open"
      busy={busy}
      onSubmit={submit}
    >
      <OptionSelect label="Agent" options={AGENT_OPTIONS} value={agent} onChange={setAgent} />
      <TextInput label="Title (optional)" value={title} onChange={setTitle} />
      <TextField fullWidth value={task} onChange={setTask}>
        <Label>Initial task (optional)</Label>
        <TextArea rows={4} />
      </TextField>
      <Checkbox isSelected={includeInHub} onChange={setIncludeInHub}>
        {/* `Checkbox.Content` is the pressable part, so the box goes inside it with the label;
            the description is the field's, a sibling of it. */}
        <Checkbox.Content>
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          <Label>Include in hub</Label>
        </Checkbox.Content>
        <Description>
          Reports this session's results to the console's hub, instead of staying outside the orchestration.
        </Description>
      </Checkbox>
      <DialogError message={error} />
    </Dialog>
  );
}
