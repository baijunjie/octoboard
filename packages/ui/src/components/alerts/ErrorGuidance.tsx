import { providerPresentation } from "../../providers/registry";

import type { ReactElement } from "react";

import { defaultAccountConfigDir, offersLoginFlow, providerLabel, servedProvider } from "../../agents/providers";
import { SignInHint } from "../text/SignInHint";
import { CODE } from "../../errors/codes";
import { CodedError } from "../../errors/codedError";
import { daemonParams, NO_RECORDS } from "../../errors/daemonMessage";
import { classifyError, isProviderRemedy, type ErrorRecovery, type ProviderRemedy } from "../../errors/errorRecovery";
import { CORE_PARAM_NAMES, globalParam, PROVIDER_PARAM_NAMES, type MessageKey, type Translate } from "../../i18n/catalog";
import { Message, useCurrentLanguage, useT } from "../../i18n/react";
import type { Language } from "../../i18n/languages";
import { InlinePath, pathText } from "../../i18n/InlinePath";
import { InlineReference } from "../../i18n/InlineReference";
import { useDaemonStoreOrEmpty } from "../../store";
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
export function RecoveryLink({ action, id, size }: { action: RecoveryAction | undefined; id?: string; size?: "xs" | "sm" }): ReactElement | null {
  if (action?.kind !== "link") return null;
  return <ExternalLink id={id} href={action.href} appearance="button" size={size} variant="secondary">{action.label}</ExternalLink>;
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

/** The product names a remedy can name: the application, Git, Tauri, the operating system and the providers. */
const PRODUCT_PARAM_NAMES: readonly string[] = ["appName", "git", "tauri", "osName", ...PROVIDER_PARAM_NAMES];

/** The product names as a remedy shows them in rich text: plain text isolated as an element, with no chip, as the
 * daemon's message shows them (`daemonParams`), in place of the chip `Message` would otherwise draw for a global name. */
function plainProductNames(language: Language): Record<string, ReactElement> {
  return Object.fromEntries(PRODUCT_PARAM_NAMES.flatMap((name) => {
    const value = globalParam(name, language);
    return value === undefined ? [] : [[name, <bdi key={name} dir="auto">{value}</bdi>]];
  }));
}

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

/** A remedy a provider's module words, with `{agent}` the provider named by the id in the key: it has a module, so its
 * name is the module's, with no served label needed. With the `error` it is drawn for, the params the daemon sent with
 * the code (a path, a version) fill it as they fill the code's message (`daemonParams`), the record's `codeParams` drawn
 * as references. A failure that nests another carries the nested failure's params beside its own, so the remedy's code
 * (named by the key) reads them from the same `error`. Where names collide the module's own constants win over a code's
 * param (the code's are spread first, then the recovery params, the references and `agent`): a code param named like one
 * of them is the module's mistake, which the providers' test catches. */
function ProviderRemedyGuidance({ remedy, error }: { remedy: ProviderRemedy; error?: unknown }): ReactElement {
  // Kept out of the React Compiler, as `CodedDaemonMessage` is: the params depend on the daemon host's operating system,
  // state outside React that `useT` re-renders this for.
  "use no memo";
  const t = useT();
  const language = useCurrentLanguage();
  const [, id] = remedy.split(".");
  const references = providerPresentation(id)?.recoveryReferences ?? {};
  const marked = Object.fromEntries(Object.entries(references).map(([key, value]) => [key, RICH_MARKS.reference(value, true)]));
  const code = remedy.slice(0, -".remedy".length);
  // No records: a provider's code names none, so a param is shown as it came.
  // `daemonParams` also returns the global placeholders (`osName`, the products' names), which `Message` fills itself as
  // references in any sentence, so they are dropped: its values would override that fill. The record's `uiParams` fill
  // with or without an error, which only the code's own params need.
  const own = Object.fromEntries(
    Object.entries(daemonParams<ReactElement>(language, code, error instanceof CodedError ? error.params : {}, NO_RECORDS, (value, monospace) => RICH_MARKS.reference(value, monospace)))
      .filter(([name]) => !CORE_PARAM_NAMES.includes(name) && !PROVIDER_PARAM_NAMES.includes(name)),
  );
  // The agent and every other product name is plain text, as in the failure's own message; account names and paths keep
  // their chips.
  const params = { ...own, ...recoveryParams(t, RICH_MARKS), ...plainProductNames(language), ...marked, agent: <bdi dir="auto">{providerLabel(t, [], id)}</bdi> };
  return <Message id={remedy as MessageKey} params={params as never} />;
}

/** Which provider's own way to log in an `error` asks for in place of the advice to log in: the agent of an
 * `account_not_signed_in` (the one code whose remedy is the account's own, `accountNotSignedIn`) that names a served provider (`agent_id`) with no login flow (`offersLoginFlow`), with the
 * account's configuration directory the daemon names (`config_dir`) or, for the default account, for which the daemon
 * names none until it knows it, the one the agent's availability gives, else the provider's usual one. `undefined` for
 * any other failure, any provider with a flow, a provider this client does not serve, and an older daemon's error that
 * names no `agent_id`: they keep the advice to log in and its way to Settings. */
export function useSignInHintOf(error: unknown): { agent: string; dir: string } | undefined {
  const providers = useDaemonStoreOrEmpty((state) => state.providers);
  const availability = useDaemonStoreOrEmpty((state) => state.agentAvailability);
  if (!(error instanceof CodedError) || error.code !== CODE.account_not_signed_in) return undefined;
  const agent = error.params.agent_id;
  if (!agent || servedProvider(providers, agent) === undefined || offersLoginFlow(providers, agent)) return undefined;
  const dir = error.params.config_dir || defaultAccountConfigDir(availability, providers, agent);
  return dir ? { agent, dir } : undefined;
}

/** A recovery's words; the control it may offer besides them (see `recoveryAction`) stands among the actions of
 * whatever shows them, an alert's footer or a mark's popover, never in the text. Nothing for the plain alert that has
 * no remedy. With the `error` an `account_not_signed_in` came from, a provider that has no login flow says how to log in
 * itself (`useSignInHintOf`). */
export function RecoveryGuidance({ recovery, error }: { recovery: ErrorRecovery; error?: unknown }): ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const providers = useDaemonStoreOrEmpty((state) => state.providers);
  const hint = useSignInHintOf(recovery === "accountNotSignedIn" ? error : undefined);
  if (recovery === "none") return null;
  // The provider has no login flow, so what to do is its own way to log in (the advice here would send to Log in).
  if (hint) return <SignInHint providers={providers} {...hint} />;
  if (isProviderRemedy(recovery)) return <ProviderRemedyGuidance remedy={recovery} error={error} />;
  const id = `error.recovery.${recovery === "trash" ? "trashRetry" : recovery}` as const;
  // Product names are plain text, as in the failure's own message.
  const params = { ...recoveryParams(t, RICH_MARKS), ...plainProductNames(language) };
  return <Message id={id} params={params as never} />;
}

/** A batch keeps each failure's remedy beside the item it applies to. */
export function ErrorGuidance({ error, recovery }: { error: unknown; recovery?: ErrorRecovery }): ReactElement {
  const t = useT();
  const classification = classifyError(error, recovery);
  if (classification.kind === "actionable" && classification.recovery === "none") return <></>;
  return <p>{classification.kind === "actionable" ? <RecoveryGuidance recovery={classification.recovery} error={error} /> : t("error.unexpected.item")}</p>;
}
