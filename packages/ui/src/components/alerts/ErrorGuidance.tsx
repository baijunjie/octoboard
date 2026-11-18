import { providerPresentation } from "../../providers/registry";

import type { ReactElement } from "react";

import { providerLabel } from "../../agents/providers";
import { classifyError, type ErrorRecovery, type ProviderRemedy } from "../../errors/errorRecovery";
import { globalParam, type MessageKey, type Translate } from "../../i18n/catalog";
import { Message, useT } from "../../i18n/react";
import { InlinePath, pathText } from "../../i18n/InlinePath";
import { InlineReference } from "../../i18n/InlineReference";
import { ExternalLink } from "../text/ExternalLink";

/** Where Git is downloaded from, for the recovery of a missing `git`. */
const GIT_DOWNLOAD_URL = "https://git-scm.com/downloads";

/** What the control a recovery offers besides its words opens: Settings on Agent accounts, with the Add agent dialog in
 * front when `addAgent` is set; Change model for the session the alert is about; or the address `href`, as a link. */
export type RecoveryAction =
  | { kind: "settings"; label: string; addAgent?: boolean }
  | { kind: "sessionModel"; label: string }
  | { kind: "link"; label: string; href: string };

/** The control a recovery offers besides its words, or none for a recovery with nothing to open. */
export function recoveryAction(t: Translate, recovery: ErrorRecovery): RecoveryAction | undefined {
  switch (recovery) {
    case "account":
    case "accountDisabled":
    case "accountNotSignedIn":
    case "resetCreditLogin":
    case "addedAgents": return { kind: "settings", label: t("sidebar.session.manageAccounts") };
    case "changeModelSetting":
    case "modelChange": return { kind: "sessionModel", label: t("sidebar.session.changeModel") };
    case "installGit":
    case "installGitNoRetry": return { kind: "link", label: t("error.recovery.installGit.action"), href: GIT_DOWNLOAD_URL };
    case "agentNotAdded": return { kind: "settings", label: t("agents.addAction"), addAgent: true };
    default: return undefined;
  }
}

/** The control the recovery of `error` offers besides its words (see `recoveryAction`), or none for a failure that is
 * not actionable or whose recovery has nothing to open. */
export function failureRecoveryAction(t: Translate, error: unknown): RecoveryAction | undefined {
  const classification = classifyError(error);
  return classification.kind === "actionable" ? recoveryAction(t, classification.recovery) : undefined;
}

/** The button of a recovery's link out (Download Git), for the footer or popover that holds its other controls, at
 * their `size`; nothing for a recovery whose control is not a link. */
export function RecoveryLink({ action, size }: { action: RecoveryAction | undefined; size?: "xs" | "sm" }): ReactElement | null {
  if (action?.kind !== "link") return null;
  return <ExternalLink href={action.href} appearance="button" size={size} variant="secondary">{action.label}</ExternalLink>;
}

/** How the rich boundary marks a recovery parameter: a name or literal as one reference, a path as a path. */
interface RecoveryMarks<V> {
  reference: (value: string, monospace: boolean) => V;
  path: (steps: string[]) => V;
}

const RICH_MARKS: RecoveryMarks<ReactElement> = {
  reference: (value, monospace) => <InlineReference dir={monospace ? "ltr" : "auto"} monospace={monospace} wrap>{value}</InlineReference>,
  path: (steps) => <InlinePath steps={steps} />,
};

/** Plain recovery parameters stay suitable for accessibility and system text. The same values are
 * marked at the rich boundary, keeping control labels and command literals distinct. */
export function recoveryParams<V = never>(t: Translate, mark?: RecoveryMarks<V>): Record<string, string | V> {
  const accountsSteps = [t("settings.title"), t("settings.section.accounts")];
  const values: Record<string, string> = {
    appName: globalParam("appName")!, git: "Git", pathVar: "PATH", enter: "Enter",
    terminal: t("common.terminal"), reopen: t("sidebar.session.reopen"), viewArchive: t("sidebar.archive.consoleSessions"), newSession: t("sidebar.focus.newSession"), retry: t("connection.retry"), browse: t("common.browse"), unlock: t("sidebar.session.unlock"), changeModel: t("sidebar.session.changeModel"),
    shareDefaultConfig: t("settings.accounts.sharing.label"), accountsPath: pathText(accountsSteps),
    worktreeList: "git worktree list", branchList: "git branch",
  };
  if (!mark) return values;
  return Object.fromEntries(Object.entries(values).map(([name, value]) => [name, name === "accountsPath" ? mark.path(accountsSteps) : mark.reference(value, ["pathVar", "enter", "worktreeList", "branchList"].includes(name))]));
}

/** Whether `recovery` is worded by a provider's module (`provider.<id>.<code>.remedy`). */
function isProviderRemedy(recovery: ErrorRecovery): recovery is ProviderRemedy {
  return recovery.startsWith("provider.");
}

/** A remedy a provider's module words, with `{agent}` the provider named by the id in the key: it has a module, so its
 * name is the module's, with no served label needed. */
function ProviderRemedyGuidance({ remedy }: { remedy: ProviderRemedy }): ReactElement {
  const t = useT();
  const references = providerPresentation(remedy.split(".")[1])?.recoveryReferences ?? {};
  const marked = Object.fromEntries(Object.entries(references).map(([key, value]) => [key, RICH_MARKS.reference(value, true)]));
  const params = { ...recoveryParams(t, RICH_MARKS), ...marked, agent: <InlineReference dir="auto" wrap>{providerLabel(t, [], remedy.split(".")[1])}</InlineReference> };
  return <Message id={remedy as MessageKey} params={params as never} />;
}

/** A recovery's words; the control it may offer besides them (see `recoveryAction`) stands among the actions of
 * whatever shows them, an alert's footer or a mark's popover, never in the text. Nothing for the plain alert that has
 * no remedy. */
export function RecoveryGuidance({ recovery }: { recovery: ErrorRecovery }): ReactElement | null {
  const t = useT();
  if (recovery === "none") return null;
  if (isProviderRemedy(recovery)) return <ProviderRemedyGuidance remedy={recovery} />;
  const id = `error.recovery.${recovery === "trash" ? "trashRetry" : recovery}` as const;
  const params = recoveryParams(t, RICH_MARKS);
  return <Message id={id} params={params as never} />;
}

/** A batch keeps each failure's remedy beside the item it applies to. */
export function ErrorGuidance({ error, recovery }: { error: unknown; recovery?: ErrorRecovery }): ReactElement {
  const t = useT();
  const classification = classifyError(error, recovery);
  if (classification.kind === "actionable" && classification.recovery === "none") return <></>;
  return <p>{classification.kind === "actionable" ? <RecoveryGuidance recovery={classification.recovery} /> : t("error.unexpected.item")}</p>;
}
