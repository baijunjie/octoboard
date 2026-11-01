import React from "react";

import { useT } from "../i18n/react";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";

/** How sessions behave towards the user: what they may do without asking first (whether their
 * requests to start a session and to resume a conversation, and their account switches and
 * handoffs, are approved on the spot, each by its own switch) and how they ask (a question as
 * options to pick from). */
export function AgentBehaviorSection(): React.ReactElement {
  const t = useT();
  return (
    <>
      <p className="pb-2 text-sm text-app-muted">{t("settings.agentPermissions.lead")}</p>
      {/* `SettingRow` rules between its rows but not above the first, and the lead paragraph above would
          otherwise run straight into a description in the same muted text. The wrapper draws that rule. */}
      <div className="groove-t">
        <DaemonSwitchSetting field="auto_approve_session_requests" />
        <DaemonSwitchSetting field="auto_approve_resume_requests" />
        <DaemonSwitchSetting field="auto_approve_account_switches" />
        <DaemonSwitchSetting field="auto_approve_handoffs" />
        <DaemonSwitchSetting field="ask_questions_as_choices" />
      </div>
    </>
  );
}
