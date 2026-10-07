import { Checkbox, Description, Label } from "@heroui/react";
import React, { useState } from "react";

import { AGENT_ICON_OPTIONS } from "../components/AgentIcon";
import { useT } from "../i18n/react";
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
  const t = useT();
  const { request } = useDaemon();
  const [agent, setAgent] = useState<Agent>(project.default_agent ?? parentConsole.default_agent);
  const [title, setTitle] = useState("");
  const [reportToConsoleSession, setReportToConsoleSession] = useState(false);
  const { error, busy, run } = useDialogAction();

  const submit = () =>
    void run(async () => {
      const reply = await request({
        type: "open_session",
        console_id: parentConsole.id,
        project_id: project.id,
        agent,
        title: title || undefined,
        include_in_hub: reportToConsoleSession || undefined,
      });
      if (reply.type === "session_opened") onOpened(reply.session.id);
      onClose();
    });

  return (
    <Dialog
      title={t("dialog.session.title", { project: project.name })}
      onClose={onClose}
      submitLabel={t("dialog.session.open")}
      busy={busy}
      onSubmit={submit}
    >
      <OptionSelect label={t("dialog.session.agent")} options={AGENT_ICON_OPTIONS} value={agent} onChange={setAgent} />
      <TextInput label={t("dialog.session.titleOptional")} value={title} onChange={setTitle} />
      {/* HeroUI's variant for a control on a surface (the dialog), whose unselected box the default
          variant would leave to blend into it. */}
      <Checkbox variant="secondary" isSelected={reportToConsoleSession} onChange={setReportToConsoleSession}>
        {/* `Checkbox.Content` is the pressable part, so the box goes inside it with the label;
            the description is the field's, a sibling of it. */}
        <Checkbox.Content>
          <Checkbox.Control>
            <Checkbox.Indicator />
          </Checkbox.Control>
          <Label>{t("dialog.session.reportToConsoleSession")}</Label>
        </Checkbox.Content>
        <Description>{t("dialog.session.reportToConsoleSessionDescription")}</Description>
      </Checkbox>
      <DialogError message={error} />
    </Dialog>
  );
}
