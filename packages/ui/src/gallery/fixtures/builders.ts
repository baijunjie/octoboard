import type { AccountQuota, Agent, Console, FileContent, GitStatus, Host, Page, Project, Session, SessionBranchItem, SessionBranchState, SessionStatus, Settings, ShellTerminal } from "../../protocol";
import type { SessionRequest, State, TrustPrompt } from "../../store";

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
    ...extra,
  };
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
}): Partial<State> {
  return {
    connectionState: parts.connectionState ?? "open",
    hosts: new Map([[LOCAL_HOST.id, LOCAL_HOST]]),
    consoles: new Map((parts.consoles ?? []).map((c) => [c.id, c])),
    projects: new Map((parts.projects ?? []).map((p) => [p.id, p])),
    sessions: new Map((parts.sessions ?? []).map((s) => [s.id, s])),
    pages: new Map(Object.entries(parts.pages ?? {})),
    shellTerminals: new Map((parts.shellTerminals ?? []).map((terminal) => [terminal.id, terminal])),
    snapshotEpoch: 1,
    trustPrompts: parts.trustPrompts ?? [],
    trustedDirectories: parts.trustedDirectories ?? [],
    sessionRequests: parts.sessionRequests ?? [],
    gitStatuses: new Map((parts.gitStatuses ?? []).map((g) => [g.project, g])),
    settings: parts.settings ?? { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/Users/dev/Projects", accounts: [] },
    accountQuotas: parts.accountQuotas ?? [],
    homeDir: "/Users/dev",
  };
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
