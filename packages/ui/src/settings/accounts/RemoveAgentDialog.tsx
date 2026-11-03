import React from "react";

import { AGENT_LABEL } from "../../agents/agents";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { appDataRecovery } from "../../errors/errorRecovery";
import { InlineReference } from "../../i18n/InlineReference";
import { joinNodes } from "../../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { useDaemon, useDaemonStoreEqual } from "../../store";
import { agentUsage } from "./agentUsage";

/** The confirmation of taking an agent off the added list. It says the agent's accounts are kept and, when consoles,
 * projects or sessions still use the agent, names them and says they keep their setting. Each line says how its
 * kind uses the agent: a console or project has it as its default agent, which is not the same as running sessions of
 * it there. */
export function RemoveAgentDialog({ agent, onRemoved, onCancel }: {
  agent: Agent;
  onRemoved: () => void;
  onCancel: () => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { request } = useDaemon();
  const usage = useDaemonStoreEqual((s) => agentUsage(agent, s.consoles, s.projects, s.sessions, (session) => sessionDisplayTitle(t, session)));
  // Names are listed while they are few enough to read in a line, else only counted.
  const phrase = (names: string[], count: string): React.ReactNode => (names.length > 0 && names.length <= 5
    ? <Message id="settings.accounts.removeAgentNamed" params={{ what: count, names: joinNodes(language, names, (name, key) => <InlineReference key={key} dir="auto" wrap>{name}</InlineReference>) }} />
    : count);
  const uses = [
    usage.consoles.length > 0 && phrase(usage.consoles, t("settings.accounts.removeAgentConsolesDefault", { count: usage.consoles.length })),
    usage.projects.length > 0 && phrase(usage.projects, t("settings.accounts.removeAgentProjectsDefault", { count: usage.projects.length })),
    usage.sessions.length > 0 && phrase(usage.sessions, t("settings.accounts.removeAgentSessionsUsing", { count: usage.sessions.length })),
  ].filter((use): use is React.ReactNode => use !== false);
  return (
    <ConfirmDialog
      title={t("settings.accounts.removeAgent")}
      errorTitle={t("settings.accounts.removeAgentFailedTitle")}
      errorRecovery={appDataRecovery}
      message={
        <>
          <p><Message id="settings.accounts.removeAgentMessage" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference> }} /></p>
          {uses.length > 0 && (
            <>
              <p><Message id="settings.accounts.removeAgentInUse" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference> }} /></p>
              <ul className="list-disc ps-5">
                {uses.map((use, at) => <li key={at}>{use}</li>)}
              </ul>
            </>
          )}
        </>
      }
      confirmLabel={t("common.remove")}
      destructive
      resetKey={agent}
      onConfirm={async () => {
        await request({ type: "remove_agent", agent });
        onRemoved();
      }}
      onCancel={onCancel}
    />
  );
}
