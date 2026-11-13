import React from "react";

import { useT } from "../i18n/react";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { LeadRows, SettingsLead } from "./SettingsLead";

/** How sessions behave towards the user: what they may do without asking first (whether their
 * requests to start a session and to resume a conversation, and their account switches and
 * handoffs, are approved on the spot, each by its own switch), how they ask (a question as
 * options to pick from) and whether they are kept to their own project. */
export function AgentBehaviorSection(): React.ReactElement {
  const t = useT();
  return (
    <>
      <SettingsLead>{t("settings.agentPermissions.lead")}</SettingsLead>
      <LeadRows>
        <DaemonSwitchSetting field="auto_approve_session_requests" />
        <DaemonSwitchSetting field="auto_approve_resume_requests" />
        <DaemonSwitchSetting field="auto_approve_account_switches" />
        <DaemonSwitchSetting field="auto_approve_handoffs" />
        <DaemonSwitchSetting field="ask_questions_as_choices" />
        <DaemonSwitchSetting field="keep_sessions_in_own_project" />
      </LeadRows>
    </>
  );
}
