import type {
  AccountFallback,
  AccountResolution,
  Agent,
  DirectoryChoiceKind,
  DocumentAgentSettings,
  ImportedAccounts,
  ImportedConsole,
  InspectedAccount,
  InspectedAccounts,
  InspectedConsole,
  InspectedProject,
  ProjectResolution,
  ProviderInfo,
  SettingsSection,
} from "../protocol";
import type { ReplyOf } from "../requests";
import { isKnownAgent } from "../agents/providers";
import { isAbsolute, isHomeRelative, type PathStyle } from "../format/hostPath";
import type { PlainMessageKey } from "../i18n/catalog";
import { sameKeys, type Binding, type ModifierHost } from "../shortcuts/binding";
import { REBINDABLE_COMMANDS } from "../shortcuts/commands";
import { effectiveBindings, sanitizeOverrides, type Overrides } from "../shortcuts/store";
import type { Region } from "./RegionTabs";
import { SETTINGS_SECTIONS } from "./settingsSections";

/** What the daemon answers to inspecting a configuration document. */
export type Inspection = ReplyOf<"inspect_configuration">;

/** The choices a project's offers can name: the three directories the daemon works out. The fourth,
 * a directory the user points at, is never offered, and leaving the project out is no directory. */
export type OfferedChoice = Exclude<DirectoryChoiceKind, "directory">;

const OFFERED_CHOICES: readonly OfferedChoice[] = ["exported_path", "default_clone_dir", "clone"];

/** What the review holds for one project: which way it is placed and, for `directory`, the path the
 * user typed. `skip` leaves the project out of the import. */
export interface RowState {
  choice: OfferedChoice | "directory" | "skip";
  /** What the field handed the form, which is already trimmed. */
  directory: string;
  /** Set once the user has made a choice for the row (see `withRow`, `applyToAll`); a row without it still follows the
   * preselection. */
  chosen?: true;
}

/** The review's state, indexed as the document is: `[console][project]`, which is also how a
 * resolution names a project. */
export type ReviewState = RowState[][];

/** The choices a project's state allows, in the order the review lists them. Pointing at a
 * directory and leaving the project out are open to every project. */
export function choicesFor(project: InspectedProject): RowState["choice"][] {
  return [...OFFERED_CHOICES.filter((choice) => project.offers[choice] !== null), "directory", "skip"];
}

/** Every project on the choice the daemon preselected, and left out where it offered nothing. */
export function initialReview(consoles: InspectedConsole[]): ReviewState {
  return consoles.map((console) =>
    console.projects.map((project) => ({ choice: project.default ?? "skip", directory: "" })),
  );
}

/** The consoles with their projects' clone-dependent offers (`default_clone_dir`, `clone`) and preselected choice as they
 * come to under the document's own default clone directory where `documentDir` says the import takes it, else as they
 * stand under this machine's. A project the document gives no second set for is as it was. */
export function withCloneDirOffers(consoles: InspectedConsole[], documentDir: boolean): InspectedConsole[] {
  if (!documentDir) return consoles;
  return consoles.map((console) => ({
    ...console,
    projects: console.projects.map((project) =>
      project.in_document_clone_dir ? { ...project, offers: project.in_document_clone_dir.offers, default: project.in_document_clone_dir.default } : project,
    ),
  }));
}

/** The review's rows after the offers changed to `after` (see `withCloneDirOffers`): a row the user never touched
 * follows the new preselected choice, or is left out where there is none. A row the user chose for keeps the choice,
 * unless it is on a clone-dependent choice that the new offers lack, which goes to the new preselected choice too. */
export function reviewForOffers(review: ReviewState, after: InspectedConsole[]): ReviewState {
  return review.map((rows, c) =>
    rows.map((row, p) => {
      const now = after[c].projects[p];
      const fallback = now.default ?? "skip";
      if (!row.chosen) return row.choice === fallback ? row : { ...row, choice: fallback };
      if ((row.choice === "default_clone_dir" || row.choice === "clone") && now.offers[row.choice] === null) return { ...row, choice: fallback };
      return row;
    }),
  );
}

/** Replaces one project's row, which the user has now chosen for; every other row is the same object as before. */
export function withRow(review: ReviewState, console: number, project: number, row: RowState): ReviewState {
  return review.map((rows, c) => (c === console ? rows.map((existing, p) => (p === project ? { ...row, chosen: true } : existing)) : rows));
}

/** How many projects `choice` can be applied to: those whose state offers it, and every project for
 * leaving it out. */
export function availableFor(consoles: InspectedConsole[], choice: OfferedChoice | "skip"): number {
  return consoles.reduce(
    (count, console) =>
      count + console.projects.filter((project) => choice === "skip" || project.offers[choice] !== null).length,
    0,
  );
}

/** `choice` on every project it is available for, which the user has now chosen for; a project it is not available
 * for keeps its row. */
export function applyToAll(
  consoles: InspectedConsole[],
  review: ReviewState,
  choice: OfferedChoice | "skip",
): ReviewState {
  return review.map((rows, c) =>
    rows.map((row, p) => {
      const project = consoles[c].projects[p];
      return choice === "skip" || project.offers[choice] !== null ? { ...row, choice, chosen: true } : row;
    }),
  );
}

/** Whether the path a project was exported with is a path and not only the directory's name, which
 * is all an export made without full paths records. */
export function hasRecordedPath(path: string): boolean {
  return /[\\/]/.test(path);
}

/** Whether `path` is one the daemon takes as a directory the user pointed at: absolute on its host, or starting
 * with `~` and a separator, which it expands. Only the shape is judged. */
export function isUsableDirectory(path: string, style: PathStyle = "unix"): boolean {
  return isAbsolute(path, style) || (path !== "~" && isHomeRelative(path, style));
}

/** What is wrong with a directory typed for a project or an account. */
export type DirectoryProblem = "empty" | "not_absolute";

/** What is wrong with the directory typed for a project that is placed in one, if anything. */
export function directoryProblem(row: Pick<RowState, "directory"> & { choice: string }, style: PathStyle = "unix"): DirectoryProblem | undefined {
  if (row.choice !== "directory") return undefined;
  const path = row.directory;
  if (path === "") return "empty";
  return isUsableDirectory(path, style) ? undefined : "not_absolute";
}

/** Whether any project's row still has a directory the daemon would refuse. */
export function hasDirectoryProblem(review: ReviewState, style: PathStyle = "unix"): boolean {
  return review.some((rows) => rows.some((row) => directoryProblem(row, style) !== undefined));
}

/** The directory a row would place its project in, for the review to show before anything is
 * applied; none for a project left out and for a directory not typed yet. */
export function targetOf(project: InspectedProject, row: RowState): string | undefined {
  switch (row.choice) {
    case "skip":
      return undefined;
    case "directory":
      return row.directory || undefined;
    default:
      return project.offers[row.choice] ?? undefined;
  }
}

/** The resolutions to send: one per project that is placed, none for a project left out. */
export function resolutionsOf(review: ReviewState): ProjectResolution[] {
  return review.flatMap((rows, console) =>
    rows.flatMap((row, project): ProjectResolution[] => {
      if (row.choice === "skip") return [];
      return [
        {
          console,
          project,
          choice: row.choice === "directory" ? { kind: "directory", path: row.directory } : { kind: row.choice },
        },
      ];
    }),
  );
}

/** What the review holds for one account of the document: `create` takes it as the daemon proposes it (a `new`
 * account; an `existing` one is used as it is and has no choice), `directory` puts it in the path the user typed, and
 * `leave_out` skips it. */
export interface AccountRowState {
  choice: "create" | "directory" | "leave_out";
  /** What the field handed the form, which is already trimmed. */
  directory: string;
}

/** The review's state of the accounts: `rows` indexed as the inspection lists them, `[agent][account]`, the account being
 * its index among the document's accounts of that agent, which is also how a resolution names it; and the agents the
 * user has unchecked (`leftOut`), whose part of the document is not imported at all. */
export interface AccountReviewState {
  rows: AccountRowState[][];
  leftOut: Agent[];
}

/** Whether an account is invalid for its name (taken, or blank), which no other directory fixes. `field_required`
 * names the field that is blank, which is the directory as often as the name. */
export function hasNameProblem(account: InspectedAccount): boolean {
  const error = account.status === "invalid" ? account.error : null;
  if (error === null) return false;
  return error.code === "account_name_taken" || (error.code === "field_required" && error.params.field === "name");
}

/** Whether an account cannot be created because the document's directory for it was written on another operating
 * system, which no reason of the daemon's says better (it only says the directory is not absolute). An account that is
 * invalid for its name is not: another directory would not fix that. */
export function hasForeignDirectoryProblem(account: InspectedAccount): boolean {
  return account.status === "invalid" && account.foreign_directory && !hasNameProblem(account);
}

/** The choices an account's status allows, in the order the review lists them; none for an account that exists
 * already, which is used as it is. A directory in use or a directory the form would refuse can be put right with
 * another directory; an account whose name is the problem can only be left out. */
export function accountChoicesFor(account: InspectedAccount): AccountRowState["choice"][] {
  switch (account.status) {
    case "new":
      return ["create", "leave_out"];
    case "existing":
      return [];
    default:
      return hasNameProblem(account) ? ["leave_out"] : ["directory", "leave_out"];
  }
}

/** Every account that can be created as proposed on `create`, every one that cannot left out, and every agent
 * imported. */
export function initialAccountReview(accounts: InspectedAccounts | null | undefined): AccountReviewState {
  return {
    rows: (accounts?.agents ?? []).map((agent) =>
      agent.accounts.map((account) => ({ choice: account.status === "new" || account.status === "existing" ? "create" : "leave_out", directory: "" })),
    ),
    leftOut: [],
  };
}

/** Replaces one account's row; every other row is the same object as before. */
export function withAccountRow(review: AccountReviewState, agent: number, account: number, row: AccountRowState): AccountReviewState {
  return { ...review, rows: review.rows.map((rows, a) => (a === agent ? rows.map((existing, i) => (i === account ? row : existing)) : rows)) };
}

/** Whether `agent`'s part of the document is imported. */
export function agentImported(review: AccountReviewState, agent: Agent): boolean {
  return !review.leftOut.includes(agent);
}

/** The review with `agent` imported or left out; the rows of its accounts keep their choices. */
export function withAgentImported(review: AccountReviewState, agent: Agent, imported: boolean): AccountReviewState {
  return { ...review, leftOut: imported ? review.leftOut.filter((left) => left !== agent) : [...review.leftOut, agent] };
}

/** Whether any account that would be created, of an agent that is imported, has a typed directory the daemon would
 * refuse. */
export function hasAccountDirectoryProblem(accounts: InspectedAccounts, review: AccountReviewState, style: PathStyle = "unix"): boolean {
  return accounts.agents.some((agent, a) => agentImported(review, agent.agent) && review.rows[a].some((row) => directoryProblem(row, style) !== undefined));
}

/** The resolutions to send: one per account of an imported agent that the user moved to another directory or left out;
 * an account created as proposed, one that exists already and any of an agent left out has none. */
export function accountResolutionsOf(accounts: InspectedAccounts, review: AccountReviewState): AccountResolution[] {
  return accounts.agents.flatMap((agent, a) =>
    agent.accounts.flatMap((account, i): AccountResolution[] => {
      const row = review.rows[a][i];
      if (!agentImported(review, agent.agent) || account.status === "existing" || row.choice === "create") return [];
      return [row.choice === "leave_out" ? { agent: agent.agent, account: i, leave_out: true } : { agent: agent.agent, account: i, directory: row.directory }];
    }),
  );
}

/** Whether the accounts panel has something that needs attention: an account that cannot be created as proposed, of an
 * agent that is imported. */
export function accountsNeedAttention(accounts: InspectedAccounts, review: AccountReviewState): boolean {
  return accounts.agents.some((agent) => agentImported(review, agent.agent) && agent.accounts.some((account) => account.status === "directory_in_use" || account.status === "invalid"));
}

/** An account's agent and name as the daemon compares names: trimmed and without regard to case. */
function accountKey(agent: Agent, name: string): string {
  return `${agent}:${name.trim().toLowerCase()}`;
}

/** The accounts of the document a console or project reference would bind to once the import has run: the ones that
 * exist already, the new ones created as proposed and the ones given another directory. An account left out, and a new
 * one of an agent left out, is not among them. The daemon binds a reference by name after it has created the accounts,
 * whichever directory they got. */
function boundAccounts(accounts: InspectedAccounts, review: AccountReviewState): Set<string> {
  const bound = new Set<string>();
  accounts.agents.forEach((agent, a) =>
    agent.accounts.forEach((account, i) => {
      if (account.status === "existing" || (agentImported(review, agent.agent) && review.rows[a]?.[i]?.choice !== "leave_out" && !hasNameProblem(account))) bound.add(accountKey(agent.agent, account.name));
    }),
  );
  return bound;
}

/** The account references that name no account to bind to: the ones that name none of `bound` (see `boundAccounts`). */
function effectiveFallbacks(fallbacks: AccountFallback[], bound: Set<string>): AccountFallback[] {
  return fallbacks.filter((fallback) => !bound.has(accountKey(fallback.agent, fallback.name)));
}

/** The inspected consoles with their account references reduced to the ones that fall back to a default account. While
 * the accounts are imported (`importing`), a reference binds to an account the import creates, by whichever choice it
 * is created (as proposed or in another directory), so only a reference to an account left out still falls back; while
 * they are not, the references stand as the daemon reported them. */
export function withEffectiveFallbacks(
  consoles: InspectedConsole[],
  accounts: InspectedAccounts | null | undefined,
  review: AccountReviewState,
  importing: boolean,
): InspectedConsole[] {
  if (!importing || !accounts) return consoles;
  const bound = boundAccounts(accounts, review);
  return consoles.map((console) => ({
    ...console,
    account_fallbacks: effectiveFallbacks(console.account_fallbacks, bound),
    projects: console.projects.map((project) => ({ ...project, account_fallbacks: effectiveFallbacks(project.account_fallbacks, bound) })),
  }));
}

/** Whether the consoles panel has something that needs attention: a project found nowhere, or a reference that
 * falls back to a default account. */
export function consolesNeedAttention(consoles: InspectedConsole[]): boolean {
  return consoles.some((console) => console.account_fallbacks.length > 0 || console.projects.some((project) => project.default === null || project.account_fallbacks.length > 0));
}

/** One thing of a document the user chooses to import or not: its consoles and projects, its Settings (within which
 * each carried section is chosen on its own), or its accounts. */
export type ImportPart = "consoles" | "settings" | "accounts";

/** The parts a document carries, in the order the dialog lists them: the consoles and projects, the settings, then the
 * accounts; the settings count as carried when the document carries at least one Settings section. A document with nothing in
 * it at all still counts as carrying its (empty) consoles. */
export function carriedParts(document: { consoles: InspectedConsole[]; accounts?: unknown; settings_sections: readonly SettingsSection[] }): ImportPart[] {
  const parts: ImportPart[] = [];
  const settings = document.settings_sections.length > 0;
  if (document.consoles.length > 0 || (!document.accounts && !settings)) parts.push("consoles");
  if (settings) parts.push("settings");
  if (document.accounts) parts.push("accounts");
  return parts;
}

/** The parts an import takes, of the ones the document `carried`: the checked ones, or the one part a document carries
 * alone, which has no checkbox because importing that file is importing that part. */
export function importedParts(carried: ImportPart[], checked: Record<ImportPart, boolean>): ImportPart[] {
  return carried.length === 1 ? carried : carried.filter((part) => checked[part]);
}

/** The Settings sections an import applies: none unless the settings are among the `parts` imported, else the sections
 * the document `carries` that are checked, in Settings order. */
export function importedSections(parts: ImportPart[], carries: readonly SettingsSection[], checked: Record<SettingsSection, boolean>): SettingsSection[] {
  return parts.includes("settings") ? SETTINGS_SECTIONS.filter((section) => carries.includes(section) && checked[section]) : [];
}

/** The parts and Settings sections an import takes for the choices made (`checked`, `sectionsChecked`) on `inspection`. */
export function chosen(
  inspection: Pick<Inspection, "consoles" | "accounts" | "settings_sections">,
  checked: Record<ImportPart, boolean>,
  sectionsChecked: Record<SettingsSection, boolean>,
): { parts: ImportPart[]; sections: SettingsSection[] } {
  const parts = importedParts(carriedParts(inspection), checked);
  return { parts, sections: importedSections(parts, inspection.settings_sections, sectionsChecked) };
}

/** Whether the import takes the document's own default clone directory: the box for it (`use`) is checked and the Git
 * section is among the sections the choices import. The review's clone-dependent offers follow this. */
export function takesDocumentCloneDir(
  inspection: Pick<Inspection, "consoles" | "accounts" | "settings_sections">,
  checked: Record<ImportPart, boolean>,
  sectionsChecked: Record<SettingsSection, boolean>,
  use: boolean,
): boolean {
  return use && chosen(inspection, checked, sectionsChecked).sections.includes("git");
}

/** Whether an import of `parts` with the Settings `sections` applied would do nothing: no consoles and no accounts, and
 * the settings either not imported or with every section unchecked. */
export function nothingToImport(parts: ImportPart[], sections: SettingsSection[]): boolean {
  return !parts.includes("consoles") && !parts.includes("accounts") && sections.length === 0;
}

/** The regions (tabs) `parts` come to: the consoles, the settings, which keep their tab while checked
 * even with every section unchecked, and the accounts. */
export function regionsOf(parts: ImportPart[]): Region[] {
  return (["consoles", "settings", "accounts"] as const).filter((region) => parts.includes(region));
}

/** What an import's accounts came to, counted from the daemon's result. */
export interface AccountsOutcome {
  created: number;
  existing: number;
  leftOut: number;
  failed: number;
}

export function accountsOutcomeOf(accounts: ImportedAccounts): AccountsOutcome {
  const outcome: AccountsOutcome = { created: 0, existing: 0, leftOut: 0, failed: 0 };
  for (const account of accounts.agents.flatMap((agent) => agent.accounts)) {
    if (account.status === "created") outcome.created += 1;
    else if (account.status === "existing") outcome.existing += 1;
    else if (account.status === "left_out") outcome.leftOut += 1;
    else if (account.status === "failed") outcome.failed += 1;
  }
  return outcome;
}

/** What an import came to, counted from the daemon's result. */
export interface ImportOutcome {
  consolesCreated: number;
  consolesFailed: number;
  projectsCreated: number;
  projectsLeftOut: number;
  projectsFailed: number;
  projectsNotAttempted: number;
}

export function outcomeOf(consoles: ImportedConsole[]): ImportOutcome {
  const outcome: ImportOutcome = {
    consolesCreated: 0,
    consolesFailed: 0,
    projectsCreated: 0,
    projectsLeftOut: 0,
    projectsFailed: 0,
    projectsNotAttempted: 0,
  };
  for (const console of consoles) {
    if (console.status === "created") outcome.consolesCreated += 1;
    else outcome.consolesFailed += 1;
    for (const project of console.projects) {
      if (project.status === "created") outcome.projectsCreated += 1;
      else if (project.status === "left_out") outcome.projectsLeftOut += 1;
      else if (project.status === "failed") outcome.projectsFailed += 1;
      else outcome.projectsNotAttempted += 1;
    }
  }
  return outcome;
}

/** The keyboard shortcut changes a `keyboard_shortcuts` value holds, as the client that exported it put them there
 * (an empty object is every shortcut at its default); `undefined` when the value is not an object, which leaves this
 * client's shortcuts as they are. */
export function shortcutsRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;
}

/** The keyboard shortcut changes a document carries in its settings; `undefined` when it carries none. */
export function documentShortcuts(document: unknown): Record<string, unknown> | undefined {
  return shortcutsRecord((document as { settings?: { keyboard_shortcuts?: unknown } | null } | null)?.settings?.keyboard_shortcuts);
}

/** What applying a document's shortcuts would do here: how many changed shortcuts it holds that can stand on
 * this client, and the commands whose binding would change from the `current` ones, in the order the commands are
 * listed, each with the binding it would get and the one it has now: none when applying would change nothing. */
export function shortcutsReview(
  shortcuts: Record<string, unknown>,
  current: Overrides,
  host: ModifierHost,
): { count: number; changed: { label: PlainMessageKey; binding: Binding; current: Binding }[] } {
  const { overrides } = sanitizeOverrides(shortcuts, host);
  const wanted = effectiveBindings(overrides);
  const now = effectiveBindings(current);
  const changed = REBINDABLE_COMMANDS
    .filter((command) => !sameKeys(wanted[command.id], now[command.id], host))
    .map((command) => ({ label: command.label, binding: wanted[command.id], current: now[command.id] }));
  return { count: Object.keys(overrides).length, changed };
}

/** The agents the Agent accounts tab and its result draw a card for: `listed` (the agents the document gives accounts
 * or default-account changes, in its order), then each agent of `added` (the agents added to the list) not among them,
 * and then each other agent the daemon serves that the document's provider settings or tags name. A key for an agent
 * the daemon does not serve counts for nothing. */
export function cardAgents(
  providers: readonly ProviderInfo[],
  settings: Pick<DocumentAgentSettings, "provider_settings" | "agent_tags"> | null | undefined,
  listed: readonly Agent[],
  added: readonly Agent[] = [],
): Agent[] {
  const first = [...listed, ...added.filter((agent) => !listed.includes(agent))];
  const named = [...Object.keys(settings?.provider_settings ?? {}), ...Object.keys(settings?.agent_tags ?? {})];
  const others = named.filter((agent, i) => isKnownAgent(providers, agent) && !first.includes(agent as Agent) && named.indexOf(agent) === i);
  return [...first, ...(others as Agent[])];
}

/** The agents of the document's accounts that are imported: those of its cards (see `cardAgents`) the user has not
 * unchecked, which are what the import is told to apply. */
export function importedAgents(providers: readonly ProviderInfo[], accounts: InspectedAccounts, review: AccountReviewState): Agent[] {
  return cardAgents(providers, accounts.agent_settings, accounts.agents.map(({ agent }) => agent), accounts.agents_to_add).filter((agent) => agentImported(review, agent));
}
