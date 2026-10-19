import type { ReactElement } from "react";

import { classifyError, type ErrorRecovery } from "../errorRecovery";
import type { Translate } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import { InlineChip } from "./InlineChip";

/** The button a recovery offers besides its words: it opens Settings on Agent accounts, with the Add agent
 * dialog in front when `addAgent` is set. None for a recovery with nothing to open. */
export function recoveryAction(t: Translate, recovery: ErrorRecovery): { label: string; addAgent?: boolean } | undefined {
  switch (recovery) {
    case "account":
    case "addedAgents": return { label: t("sidebar.session.manageAccounts") };
    case "agentNotAdded": return { label: t("agents.addAction"), addAgent: true };
    default: return undefined;
  }
}

/** A recovery's words; the button it may offer besides them is the alert's (see `recoveryAction`), never part
 * of the body. Nothing for the plain alert that has no remedy. */
export function RecoveryGuidance({ recovery }: { recovery: ErrorRecovery }): ReactElement | null {
  const t = useT();
  switch (recovery) {
    case "connection": return <Message id="error.recovery.connection" params={{ retry: <InlineChip size="inherit">{t("connection.retry")}</InlineChip> }} />;
    case "path": return <Message id="error.recovery.path" params={{ browse: <InlineChip size="inherit">{t("common.browse")}</InlineChip> }} />;
    case "unlock": return <Message id="error.recovery.unlock" params={{ unlock: <InlineChip size="inherit">{t("sidebar.session.unlock")}</InlineChip> }} />;
    case "projectBranches": return <Message id="error.recovery.projectBranches" params={{ worktreeList: <InlineChip monospace size="inherit">git worktree list</InlineChip>, branchList: <InlineChip monospace size="inherit">git branch</InlineChip> }} />;
    case "none": return null;
    default: return <>{t(`error.recovery.${recovery}`)}</>;
  }
}

/** A batch keeps each failure's remedy beside the item it applies to. */
export function ErrorGuidance({ error, recovery }: { error: unknown; recovery?: ErrorRecovery }): ReactElement {
  const t = useT();
  const classification = classifyError(error, recovery);
  if (classification.kind === "actionable" && classification.recovery === "none") return <></>;
  return <p>{classification.kind === "actionable" ? <RecoveryGuidance recovery={classification.recovery} /> : t("error.unexpected.item")}</p>;
}
