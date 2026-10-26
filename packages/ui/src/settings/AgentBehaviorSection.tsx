import React from "react";

import { useT } from "../i18n/react";
import { AskQuestionsAsChoicesSetting } from "./AskQuestionsAsChoicesSetting";
import { AutoApproveAccountSwitchesSetting } from "./AutoApproveAccountSwitchesSetting";
import { AutoApproveHandoffsSetting } from "./AutoApproveHandoffsSetting";
import { AutoApproveResumeRequestsSetting } from "./AutoApproveResumeRequestsSetting";
import { AutoApproveSessionRequestsSetting } from "./AutoApproveSessionRequestsSetting";
import { ReplyInInterfaceLanguageSetting } from "./ReplyInInterfaceLanguageSetting";

/** How sessions behave towards the user: what they may do without asking first (whether their
 * requests to start a session and to resume a conversation, and their account switches and
 * handoffs, are approved on the spot, each by its own switch) and how they ask (a question as
 * options to pick from) and which language they talk to the user in. */
export function AgentBehaviorSection(): React.ReactElement {
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
        <AskQuestionsAsChoicesSetting />
        <ReplyInInterfaceLanguageSetting />
      </div>
    </>
  );
}
