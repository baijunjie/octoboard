import type { ReactElement } from "react";

import { classifyError, type ErrorRecovery } from "../errorRecovery";
import { globalParam, type Translate } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import { InlineReference } from "./InlineReference";

/** The button a recovery offers besides its words: it opens Settings on Agent accounts, with the Add agent
 * dialog in front when `addAgent` is set, or, when `sessionModel` is set, Change model for the session the alert is
 * about. None for a recovery with nothing to open. */
export function recoveryAction(t: Translate, recovery: ErrorRecovery): { label: string; addAgent?: boolean; sessionModel?: boolean } | undefined {
  switch (recovery) {
    case "account":
    case "accountDisabled":
    case "accountNotSignedIn":
    case "addedAgents": return { label: t("sidebar.session.manageAccounts") };
    case "changeModelSetting":
    case "modelChange": return { label: t("sidebar.session.changeModel"), sessionModel: true };
    case "agentNotAdded": return { label: t("agents.addAction"), addAgent: true };
    default: return undefined;
  }
}

/** Plain recovery parameters stay suitable for accessibility and system text. The same values are
 * marked at the rich boundary, keeping control labels and command literals distinct. */
export function recoveryParams<V = never>(t: Translate, mark?: (value: string, monospace: boolean) => V): Record<string, string | V> {
  const values: Record<string, string> = {
    appName: globalParam("appName")!, codex: "Codex", git: "Git", pathVar: "PATH", enter: "Enter",
    terminal: t("common.terminal"), reopen: t("sidebar.session.reopen"), viewArchive: t("sidebar.archive.consoleSessions"), newSession: t("sidebar.focus.newSession"), retry: t("connection.retry"), browse: t("common.browse"), unlock: t("sidebar.session.unlock"), changeModel: t("sidebar.session.changeModel"),
    worktreeList: "git worktree list", branchList: "git branch",
    configToml: "config.toml", sqliteHome: "sqlite_home", codexSqliteHome: "CODEX_SQLITE_HOME", tilde: "~", home: "HOME",
  };
  if (!mark) return values;
  return Object.fromEntries(Object.entries(values).map(([name, value]) => [name, mark(value, ["pathVar", "enter", "worktreeList", "branchList", "configToml", "sqliteHome", "codexSqliteHome", "tilde", "home"].includes(name))]));
}

/** A recovery's words; the button it may offer besides them is the alert's (see `recoveryAction`), never part
 * of the body. Nothing for the plain alert that has no remedy. */
export function RecoveryGuidance({ recovery }: { recovery: ErrorRecovery }): ReactElement | null {
  const t = useT();
  if (recovery === "none") return null;
  const id = `error.recovery.${recovery}` as const;
  const params = recoveryParams(t, (value, monospace) => <InlineReference dir={monospace ? "ltr" : "auto"} monospace={monospace} wrap>{value}</InlineReference>);
  return <Message id={id} params={params as never} />;
}

/** A batch keeps each failure's remedy beside the item it applies to. */
export function ErrorGuidance({ error, recovery }: { error: unknown; recovery?: ErrorRecovery }): ReactElement {
  const t = useT();
  const classification = classifyError(error, recovery);
  if (classification.kind === "actionable" && classification.recovery === "none") return <></>;
  return <p>{classification.kind === "actionable" ? <RecoveryGuidance recovery={classification.recovery} /> : t("error.unexpected.item")}</p>;
}
