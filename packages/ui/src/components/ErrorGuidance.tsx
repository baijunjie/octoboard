import type { ReactElement } from "react";

import { classifyError, type ErrorRecovery } from "../errorRecovery";
import { Message, useT } from "../i18n/react";
import { InlineChip } from "./InlineChip";

export function RecoveryGuidance({ recovery }: { recovery: ErrorRecovery }): ReactElement {
  const t = useT();
  const settingsAccounts = { settings: <InlineChip size="inherit">{t("settings.title")}</InlineChip>, accounts: <InlineChip size="inherit">{t("settings.section.accounts")}</InlineChip> };
  switch (recovery) {
    case "connection": return <Message id="error.recovery.connection" params={{ retry: <InlineChip size="inherit">{t("connection.retry")}</InlineChip> }} />;
    case "path": return <Message id="error.recovery.path" params={{ browse: <InlineChip size="inherit">{t("common.browse")}</InlineChip> }} />;
    case "account": return <Message id="error.recovery.account" params={settingsAccounts} />;
    case "addedAgents": return <Message id="error.recovery.addedAgents" params={settingsAccounts} />;
    case "unlock": return <Message id="error.recovery.unlock" params={{ unlock: <InlineChip size="inherit">{t("sidebar.session.unlock")}</InlineChip> }} />;
    case "projectBranches": return <Message id="error.recovery.projectBranches" params={{ worktreeList: <InlineChip monospace size="inherit">git worktree list</InlineChip>, branchList: <InlineChip monospace size="inherit">git branch</InlineChip> }} />;
    default: return <>{t(`error.recovery.${recovery}`)}</>;
  }
}

/** A batch keeps each failure's remedy beside the item it applies to. */
export function ErrorGuidance({ error, recovery }: { error: unknown; recovery?: ErrorRecovery }): ReactElement {
  const t = useT();
  const classification = classifyError(error, recovery);
  return <p>{classification.kind === "actionable" ? <RecoveryGuidance recovery={classification.recovery} /> : t("error.unexpected.item")}</p>;
}
