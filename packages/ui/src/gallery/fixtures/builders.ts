import { DEFAULT_GIT_WORKFLOW, FOLLOWING_GIT_WORKFLOW } from "../../gitWorkflow";
import type { PlainMessageKey, Translate } from "../../i18n/catalog";
import { hasClaudeCompatibility } from "../../settings/claudeCompatibility";
import type { AccountQuota, AccountSharing, Agent, Console, FileContent, GitStatus, Host, Page, ProcessTree, Project, Session, SessionBranchItem, SessionBranchState, SessionContext, SessionStatus, Settings, ShellTerminal } from "../../protocol";
import type { SessionRequest, State, TrustPrompt } from "../../store";

/** Every agent, as the list of added agents of a user who has added them all (in the default order). */
export const ALL_AGENTS: Agent[] = ["claude", "codex", "grok"];

const NOW = Date.now();

/** An instant `minutes` before the page loaded, in the milliseconds the protocol uses. */
export function minutesAgo(minutes: number): number {
  return NOW - minutes * 60_000;
}

/** An instant `minutes` after the page loaded: a reset time or deadline that is still ahead. */
export function minutesFromNow(minutes: number): number {
  return NOW + minutes * 60_000;
}

/** A text file's content as the daemon reads it. */
export const text = (body: string): FileContent => ({ size: new TextEncoder().encode(body).length, kind: "text", media_type: null, text: body, data: null });

const LOCAL_HOST: Host = { id: "local", name: "This Mac", kind: "local" };

export function consoleOf(id: string, name: string, extra: Partial<Console> = {}): Console {
  return {
    id,
    name,
    workdir: `/Users/dev/octoboard/${id}`,
    default_agent: "claude",
    created_at: minutesAgo(60 * 24 * 30),
    ...extra,
  };
}

export function projectOf(id: string, consoleId: string, name: string, extra: Partial<Project> = {}): Project {
  return {
    id,
    console_id: consoleId,
    host_id: LOCAL_HOST.id,
    name,
    path: `/Users/dev/code/${name.toLowerCase().replaceAll(" ", "-")}`,
    source: "local",
    trust_consent: false,
    pinned: false,
    tags: [],
    claude_account_id: { kind: "inherit" },
    codex_account_id: { kind: "inherit" },
    grok_account_id: { kind: "inherit" },
    git_workflow: FOLLOWING_GIT_WORKFLOW,
    ...extra,
  };
}

/** A context size with its window and the share of it the daemon reports. */
export function contextOf(percent: number, window = 200_000): SessionContext {
  return { tokens: Math.round((window * percent) / 100), window, used_percent: percent };
}

export function sessionOf(
  id: string,
  consoleId: string,
  projectId: string | undefined,
  title: string,
  status: SessionStatus,
  extra: Partial<Session> = {},
): Session {
  return {
    id,
    agent: "claude" satisfies Agent,
    console_id: consoleId,
    project_id: projectId,
    host_id: LOCAL_HOST.id,
    role: projectId ? "project" : "console",
    origin: projectId ? "user" : "console",
    title,
    status,
    has_conversation: true,
    pinned: false,
    locked: false,
    started_at: minutesAgo(90),
    ...(status === "archived" ? { ended_at: minutesAgo(30) } : {}),
    ...extra,
  };
}

/** A shell terminal of `project`, running and idle in the project's own directory, titled `title`
 * as the shell last set it. `created_at` follows `order`, so terminals are listed in the tab strip
 * in the order of `order`. */
export function shellTerminalOf(id: string, project: Project, title: string, order: number, extra: Partial<ShellTerminal> = {}): ShellTerminal {
  return {
    id,
    project_id: project.id,
    worktree: null,
    cwd: project.path,
    title,
    renamed: false,
    state: "running",
    busy: false,
    created_at: minutesAgo(60 - order),
    ...extra,
  };
}

/** A branch bound to `session` of `project` that git refuses to delete as it stands. */
export function branchItemOf(project: Project, session: Session, branch: string, state: SessionBranchState = "unmerged"): SessionBranchItem {
  return { kind: "branch", repository: `${project.path}/.git`, branch, path: null, project: project.id, session: session.id, state, uncommitted: false, detail: null };
}

/** A linked worktree of `project`'s repository at `path`, with `branch` checked out (`null` for a
 * detached `HEAD`), bound to `session` (`null` for one bound to none). `uncommitted` is whether it
 * has modified or untracked files, which a locked one can have as well. */
export function worktreeItemOf(
  project: Project,
  session: Session | null,
  branch: string | null,
  path: string,
  state: SessionBranchState = "uncommitted_changes",
  uncommitted: boolean = state === "uncommitted_changes",
): SessionBranchItem {
  return { kind: "worktree", repository: `${project.path}/.git`, branch, path, project: project.id, session: session?.id ?? null, state, uncommitted, detail: null };
}

/** A linked worktree of `project`'s repository at `path` that the removal of the project with its folder holds back:
 * whether it is or holds another project's folder could not be told, so it stays (`failed`, with that `cause`). */
export function unknownFolderWorktreeOf(project: Project, session: Session | null, branch: string | null, path: string): SessionBranchItem {
  return {
    ...worktreeItemOf(project, session, branch, path, "failed", false),
    detail: "whether it is or holds another project's folder could not be told",
    cause: "project_folder_unknown",
  };
}

/** A process tree the daemon follows for the launch of `session`: one process started `minutes` ago in `cwd`, with
 * no ports; `extra` sets what the view needs (its count, memory, ports, or a working directory that is unknown or
 * deleted). */
export function processTreeOf(
  id: string,
  session: Session,
  command: string,
  minutes: number,
  extra: Partial<Omit<ProcessTree, "id" | "session_id" | "command" | "started_at">> = {},
): ProcessTree & { session_id: string } {
  return { id, session_id: session.id, kept: false, command, cwd: "/Users/dev/code/website", cwd_missing: false, started_at: minutesAgo(minutes), process_count: 1, memory_bytes: 48_000_000, ports: [], ...extra };
}

/** A project process left behind in a removed directory of a project, which belongs to no session. */
export function leftoverOf(
  id: string,
  command: string,
  minutes: number,
  extra: Partial<Omit<ProcessTree, "id" | "session_id" | "command" | "started_at">> = {},
): Omit<ProcessTree, "session_id"> {
  return { id, kept: false, command, cwd: "/Users/dev/code/website/.worktrees/old-nav", cwd_missing: true, started_at: minutesAgo(minutes), process_count: 1, memory_bytes: 48_000_000, ports: [], ...extra };
}

/** A project process the user kept past the end of its session (or a failed account switch left): it belongs to no
 * session, and its directory is still there. */
export function keptOf(
  id: string,
  command: string,
  minutes: number,
  extra: Partial<Omit<ProcessTree, "id" | "session_id" | "command" | "started_at">> = {},
): Omit<ProcessTree, "session_id"> {
  return leftoverOf(id, command, minutes, { kept: true, cwd: "/Users/dev/code/website", cwd_missing: false, ...extra });
}

export function pageOf(id: string, consoleSessionId: string, html: string, minutes: number): Page {
  return { id, console_session_id: consoleSessionId, html, created_at: minutesAgo(minutes) };
}

export function gitStatusOf(projectId: string, extra: Partial<GitStatus> = {}): GitStatus {
  return {
    project: projectId,
    repository: true,
    branch: "main",
    detached: false,
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    activity: "idle",
    error: null,
    ...extra,
  };
}

/** The sharing states the daemon reports (`snapshot.account_sharing`), by account; an account left out is free. */
export function accountSharingOf(states: AccountSharing[]): Map<string, AccountSharing> {
  return new Map(states.map((sharing) => [sharing.account, sharing]));
}

/** A connected window's state after its first snapshot, over which a scenario lays what it differs
 * in. Only what is given is set; the rest is the empty state. */
export function snapshotState(parts: {
  consoles?: Console[];
  projects?: Project[];
  sessions?: Session[];
  pages?: Record<string, Page[]>;
  shellTerminals?: ShellTerminal[];
  trustPrompts?: TrustPrompt[];
  trustedDirectories?: string[];
  sessionRequests?: SessionRequest[];
  connectionState?: State["connectionState"];
  gitStatuses?: GitStatus[];
  settings?: Settings;
  accountQuotas?: AccountQuota[];
  accountSharing?: AccountSharing[];
}): Partial<State> {
  return {
    connectionState: parts.connectionState ?? "open",
    hosts: new Map([[LOCAL_HOST.id, LOCAL_HOST]]),
    consoles: new Map((parts.consoles ?? []).map((c) => [c.id, c])),
    projects: new Map((parts.projects ?? []).map((p) => [p.id, p])),
    sessions: new Map((parts.sessions ?? []).map((s) => [s.id, s])),
    // The sessions the daemon's snapshot says are switching their account, kept apart from what the window adds for its own batch.
    daemonSwitching: new Set((parts.sessions ?? []).filter((s) => s.switching_account).map((s) => s.id)),
    pages: new Map(Object.entries(parts.pages ?? {})),
    shellTerminals: new Map((parts.shellTerminals ?? []).map((terminal) => [terminal.id, terminal])),
    snapshotEpoch: 1,
    trustPrompts: parts.trustPrompts ?? [],
    trustedDirectories: parts.trustedDirectories ?? [],
    sessionRequests: parts.sessionRequests ?? [],
    gitStatuses: new Map((parts.gitStatuses ?? []).map((g) => [g.project, g])),
    settings: parts.settings ?? { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/Users/dev/Projects", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [] },
    accountQuotas: parts.accountQuotas ?? [],
    accountSharing: accountSharingOf(parts.accountSharing ?? []),
    homeDir: "/Users/dev",
  };
}

/** `state` as the daemon reports it when the account switches of the sessions `ids` are under way
 * (`Session.switching_account`), whatever those sessions were doing. */
export function switchingAccount(state: Partial<State>, ids: string[]): Partial<State> {
  const sessions = new Map(state.sessions);
  for (const id of ids) {
    const session = sessions.get(id);
    if (!session) throw new Error(`switchingAccount: no session ${id} in the state`);
    sessions.set(id, { ...session, switching_account: true });
  }
  return { ...state, sessions, daemonSwitching: new Set([...(state.daemonSwitching ?? []), ...ids]) };
}

/** A small, ordinary console to build a scenario on: one console, two projects, a console session
 * and a few sessions. */
export const SAMPLE = (() => {
  const console_ = consoleOf("c-main", "Main");
  const web = projectOf("p-web", console_.id, "Website");
  const api = projectOf("p-api", console_.id, "Search API");
  const consoleSession = sessionOf("s-console", console_.id, undefined, "Hub", "idle", { colour: "teal" });
  const sessions = [
    consoleSession,
    sessionOf("s-web-1", console_.id, web.id, "Fix the summary layout", "working"),
    sessionOf("s-web-2", console_.id, web.id, "Update the dependencies", "idle"),
    sessionOf("s-api-1", console_.id, api.id, "Add idempotency keys", "waiting_user"),
  ];
  return { console: console_, web, api, consoleSession, sessions };
})();

/** The title the action that raises an error gives its alert (`ToastFixture.title`), for a scenario that raises the
 * error itself rather than through the action: the message at `key`, worded in the viewing language, so a title the
 * app renames or drops breaks the scenario at type checking. */
export const failureTitle = (key: PlainMessageKey) => (t: Translate) => t(key);

/** The Claude compatibility the daemon puts in `Settings.claude_compatibility` and in an import's
 * `current_claude_compatibility`: the user's explicit choice for an agent when there is one, otherwise Grok Build on and
 * Codex on only while Claude Code is among the added agents (an undecided list counts as not). The gallery's daemon keeps
 * a scenario's `settings.claude_compatibility` as the explicit choices and reports this. */
export function effectiveClaudeCompatibility(choices: Partial<Record<Agent, boolean>>, added: Agent[] | null): Partial<Record<Agent, boolean>> {
  const defaults: Record<Exclude<Agent, "claude">, boolean> = { codex: added?.includes("claude") ?? false, grok: true };
  return Object.fromEntries(ALL_AGENTS.filter(hasClaudeCompatibility).map((agent) => [agent, choices[agent] ?? defaults[agent]]));
}
