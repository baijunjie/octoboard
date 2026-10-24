import React from "react";

import { useT } from "../i18n/react";
import { AutoApproveAccountSwitchesSetting } from "./AutoApproveAccountSwitchesSetting";
import { AutoApproveHandoffsSetting } from "./AutoApproveHandoffsSetting";
import { AutoApproveResumeRequestsSetting } from "./AutoApproveResumeRequestsSetting";
import { AutoApproveSessionRequestsSetting } from "./AutoApproveSessionRequestsSetting";

/** What a session may do without asking the user first: whether its requests to start a session and
 * to resume a conversation, and its account switches and handoffs, are approved on the spot, each
 * by its own switch. */
export function AgentPermissionsSection(): React.ReactElement {
  const t = useT();
  return (
    <>
      <p className="pb-2 text-sm text-app-muted">{t("settings.agentPermissions.lead")}</p>
      {/* `SettingRow` rules between its rows but not above the first, and the lead paragraph above would
          otherwise run straight into a description in the same muted text. The wrapper draws that rule. */}
      <div className="groove-t">
        <AutoApproveSessionRequestsSetting />
        <AutoApproveResumeRequestsSetting />
        <AutoApproveAccountSwitchesSetting />
        <AutoApproveHandoffsSetting />
      </div>
    </>
  );
}
