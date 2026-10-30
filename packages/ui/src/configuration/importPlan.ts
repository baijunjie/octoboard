import type {
  AccountFallback,
  AccountResolution,
  Agent,
  ImportChoiceKind,
  ImportedAccounts,
  Event,
  ImportedConsole,
  InspectedAccount,
  InspectedAccounts,
  InspectedConsole,
  InspectedProject,
  ProjectResolution,
} from "../protocol";
import { hasClaudeCompatibility } from "../settings/claudeCompatibility";
import { overridesEqual, sanitizeOverrides, type Overrides } from "../shortcuts/store";
import type { Region } from "./RegionTabs";

/** What the daemon answers to inspecting a configuration document. */
export type Inspection = Extract<Event, { type: "configuration_inspected" }>;

/** The choices a project's offers can name: the three directories the daemon works out. The fourth,
 * a directory the user points at, is never offered, and leaving the project out is no directory. */
export type OfferedChoice = Exclude<ImportChoiceKind, "directory">;

const OFFERED_CHOICES: readonly OfferedChoice[] = ["exported_path", "default_clone_dir", "clone"];

/** What the review holds for one project: which way it is placed and, for `directory`, the path the
 * user typed. `skip` leaves the project out of the import. */
export interface RowState {
  choice: OfferedChoice | "directory" | "skip";
  /** What the field handed the form, which is already trimmed. */
  directory: string;
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

/** Replaces one project's row; every other row is the same object as before. */
export function withRow(review: ReviewState, console: number, project: number, row: RowState): ReviewState {
  return review.map((rows, c) => (c === console ? rows.map((existing, p) => (p === project ? row : existing)) : rows));
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

/** `choice` on every project it is available for; a project it is not available for keeps its
 * row. */
export function applyToAll(
  consoles: InspectedConsole[],
  review: ReviewState,
  choice: OfferedChoice | "skip",
): ReviewState {
  return review.map((rows, c) =>
    rows.map((row, p) => {
      const project = consoles[c].projects[p];
      return choice === "skip" || project.offers[choice] !== null ? { ...row, choice } : row;
    }),
  );
}

/** Whether the path a project was exported with is a path and not only the directory's name, which
 * is all an export made without full paths records. */
export function hasRecordedPath(path: string): boolean {
  return /[\\/]/.test(path);
}

/** Whether `path` is one the daemon takes as a directory the user pointed at: absolute, or starting
 * with `~/`, which it expands. Only the shape is judged. */
export function isUsableDirectory(path: string): boolean {
  return path.startsWith("/") || path.startsWith("~/");
}

/** What is wrong with a directory typed for a project or an account. */
export type DirectoryProblem = "empty" | "not_absolute";

/** What is wrong with the directory typed for a project that is placed in one, if anything. */
export function directoryProblem(row: Pick<RowState, "directory"> & { choice: string }): DirectoryProblem | undefined {
  if (row.choice !== "directory") return undefined;
  const path = row.directory;
  if (path === "") return "empty";
  return isUsableDirectory(path) ? undefined : "not_absolute";
}

/** Whether any project's row still has a directory the daemon would refuse. */
export function hasDirectoryProblem(review: ReviewState): boolean {
  return review.some((rows) => rows.some((row) => directoryProblem(row) !== undefined));
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

/** The review's state of the accounts, indexed as the inspection lists them: `[agent][account]`, the account being
 * its index among the document's accounts of that agent, which is also how a resolution names it. */
export type AccountReviewState = AccountRowState[][];

/** Whether an account is invalid for its name (taken, or blank), which no other directory fixes. `field_required`
 * names the field that is blank, which is the directory as often as the name. */
export function hasNameProblem(account: InspectedAccount): boolean {
  const error = account.status === "invalid" ? account.error : null;
  if (error === null) return false;
  return error.code === "account_name_taken" || (error.code === "field_required" && error.params.field === "name");
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

/** Every account that can be created as proposed on `create`, and every one that cannot left out. */
export function initialAccountReview(accounts: InspectedAccounts | null | undefined): AccountReviewState {
  return (accounts?.agents ?? []).map((agent) =>
    agent.accounts.map((account) => ({ choice: account.status === "new" || account.status === "existing" ? "create" : "leave_out", directory: "" })),
  );
}

/** Replaces one account's row; every other row is the same object as before. */
export function withAccountRow(review: AccountReviewState, agent: number, account: number, row: AccountRowState): AccountReviewState {
  return review.map((rows, a) => (a === agent ? rows.map((existing, i) => (i === account ? row : existing)) : rows));
}

/** Whether any account that would be created has a typed directory the daemon would refuse. */
export function hasAccountDirectoryProblem(review: AccountReviewState): boolean {
  return review.some((rows) => rows.some((row) => directoryProblem(row) !== undefined));
}

/** The resolutions to send: one per account the user moved to another directory or left out; an account created as
 * proposed, and one that exists already, has none. */
export function accountResolutionsOf(accounts: InspectedAccounts, review: AccountReviewState): AccountResolution[] {
  return accounts.agents.flatMap((agent, a) =>
    agent.accounts.flatMap((account, i): AccountResolution[] => {
      const row = review[a][i];
      if (account.status === "existing" || row.choice === "create") return [];
      return [row.choice === "leave_out" ? { agent: agent.agent, account: i, leave_out: true } : { agent: agent.agent, account: i, directory: row.directory }];
    }),
  );
}

/** Whether the accounts panel has something that needs attention: an account that cannot be created as proposed. */
export function accountsNeedAttention(accounts: InspectedAccounts): boolean {
  return accounts.agents.some((agent) => agent.accounts.some((account) => account.status === "directory_in_use" || account.status === "invalid"));
}

/** An account's agent and name as the daemon compares names: trimmed and without regard to case. */
function accountKey(agent: Agent, name: string): string {
  return `${agent}:${name.trim().toLowerCase()}`;
}

/** The accounts of the document a console or project reference would bind to once the import has run: the ones that
 * exist already, the new ones created as proposed and the ones given another directory. An account left out is not
 * among them. The daemon binds a reference by name after it has created the accounts, whichever directory they got. */
function boundAccounts(accounts: InspectedAccounts, review: AccountReviewState): Set<string> {
  const bound = new Set<string>();
  accounts.agents.forEach((agent, a) =>
    agent.accounts.forEach((account, i) => {
      if (account.status === "existing" || (review[a]?.[i]?.choice !== "leave_out" && !hasNameProblem(account))) bound.add(accountKey(agent.agent, account.name));
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

/** The regions a document carries, in the order the dialog lists them. A document with nothing in it at all still
 * counts as carrying its (empty) consoles. */
export function carriedRegions(document: { consoles: InspectedConsole[]; accounts?: unknown; settings?: unknown }): Region[] {
  const regions: Region[] = [];
  if (document.consoles.length > 0 || (!document.accounts && !document.settings)) regions.push("consoles");
  if (document.accounts) regions.push("accounts");
  if (document.settings) regions.push("settings");
  return regions;
}

/** The regions an import takes, of the ones the document `carried`: the checked ones, or the one region a document
 * carries alone, which has no checkbox because importing that file is importing that region. */
export function importedRegions(carried: Region[], checked: Record<Region, boolean>): Region[] {
  return carried.length === 1 ? carried : carried.filter((region) => checked[region]);
}

/** What an import's accounts came to, counted from the daemon's result. */
export interface AccountsOutcome {
  created: number;
  existing: number;
  leftOut: number;
  failed: number;
  sharingWarnings: number;
}

export function accountsOutcomeOf(accounts: ImportedAccounts): AccountsOutcome {
  const outcome: AccountsOutcome = { created: 0, existing: 0, leftOut: 0, failed: 0, sharingWarnings: 0 };
  for (const account of accounts.agents.flatMap((agent) => agent.accounts)) {
    if (account.status === "created") outcome.created += 1;
    else if (account.status === "existing") outcome.existing += 1;
    else if (account.status === "left_out") outcome.leftOut += 1;
    else if (account.status === "failed") outcome.failed += 1;
    if (account.sharing_warning) outcome.sharingWarnings += 1;
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
 * this client, and whether they differ from the `current` ones. */
export function shortcutsReview(shortcuts: Record<string, unknown>, current: Overrides): { count: number; changes: boolean } {
  const { overrides } = sanitizeOverrides(shortcuts);
  return { count: Object.keys(overrides).length, changes: !overridesEqual(overrides, current) };
}

/** Whether a document's Claude compatibility choices say anything: an empty object is a machine where the user
 * chose for no agent, and a key for an agent without the setting counts for nothing. */
export function hasClaudeCompatibilityChoices(choices: Partial<Record<Agent, boolean>>): boolean {
  return Object.keys(choices).some(hasClaudeCompatibility);
}

/** Whether applying a document's Claude compatibility `wanted` would change a value here. Only an agent that has
 * the setting and that this machine reports in `current` counts: the daemon skips the others, and for an agent it
 * does not report nothing is known to compare with. */
export function claudeCompatibilityChanges(wanted: Partial<Record<Agent, boolean>>, current: Partial<Record<Agent, boolean>>): boolean {
  return (Object.entries(wanted) as [Agent, boolean][]).some(([agent, on]) => hasClaudeCompatibility(agent) && current[agent] !== undefined && current[agent] !== on);
}
