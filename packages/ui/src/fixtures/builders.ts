import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { FOLLOWING_GIT_WORKFLOW, GIT_WORKFLOW_ITEM_KEYS } from "../git/gitWorkflow";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { PROTOCOL_GENERATION, type Account, type AccountQuota, type AccountSharing, type Agent, type AgentCliStatus, type AgentCliUpdate, type Console, type DocumentAgentSettings, type FileContent, type Launch, type GitStatus, type Host, type HostInfo, type InspectedAgentSettings, type Page, type PageContent, type PerAgent, type ProcessTree, type Project, type ProviderInfo, type Session, type SessionBranchItem, type SessionBranchState, type SessionContext, type SessionStatus, type Settings, type ShellTerminal } from "../protocol";
import { type MessageParams } from "../errors/codedError";
import type { SessionRequest, State, TrustPrompt } from "../store";
import type { NotificationFixture } from "./fixture";
import { FIXTURE_AGENTS, FIXTURE_PROVIDERS, availabilityOf } from "./fixtureProviders";

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

const LOCAL_HOST: Host = { id: "local", name: "This Mac", kind: "local", ssh_config: null };

const NO_LAUNCH: Launch = { model: null, effort: null, compatibility: {} };

/** One group per agent with the settings an extra names laid over the ones it has. */
function layered<T extends object>(base: PerAgent<T>, over: Partial<PerAgent<Partial<T>>> = {}): PerAgent<T> {
  return Object.fromEntries(FIXTURE_AGENTS.map((agent) => [agent, { ...base[agent], ...over[agent] }])) as PerAgent<T>;
}

/** Every path at version 1: what a record built here holds for a checked write to be based on. */
const versionsOf = (paths: string[]): Record<string, number> => Object.fromEntries(paths.map((path) => [path, 1]));
const forEachAgent = (paths: (agent: Agent) => string[]): string[] => FIXTURE_AGENTS.flatMap(paths);

/** The `versions` of a console, a project, an account and the settings a builder here gives: one for every path the
 * daemon versions. */
export const CONSOLE_VERSIONS = versionsOf([
  "name", "default_agent", "project_session_agent", "icon", "console_session_prompt",
  ...FIXTURE_PROVIDERS.flatMap(({ id, settings }) => settings.filter((setting) => setting.compatibility_with !== null).flatMap((setting) => [`launch.${id}.compatibility.${setting.id}`, `project_session_launch.${id}.compatibility.${setting.id}`])),
  ...forEachAgent((agent) => [`accounts.${agent}`, `launch.${agent}.model`, `launch.${agent}.effort`, `project_session_launch.${agent}.model`, `project_session_launch.${agent}.effort`]),
]);
export const PROJECT_VERSIONS = versionsOf([
  "name", "default_agent", "pinned", "tags", "description",
  ...forEachAgent((agent) => [`accounts.${agent}`, `launch.${agent}.model`, `launch.${agent}.effort`]),
  ...GIT_WORKFLOW_ITEM_KEYS.map((item) => `git_workflow.${item}`),
  ...FIXTURE_PROVIDERS.flatMap(({ id, settings }) => settings.filter((setting) => setting.compatibility_with !== null).map((setting) => `launch.${id}.compatibility.${setting.id}`)),
]);
export const ACCOUNT_VERSIONS = versionsOf(["name", "config_dir", "disabled", "share_default_config"]);
export const SETTINGS_VERSIONS = versionsOf([
  "auto_close_idle_shell_terminals", "auto_pull_repositories", "auto_delete_unresumable_archives", "auto_approve_session_requests",
  "auto_approve_resume_requests", "auto_approve_account_switches", "auto_approve_handoffs", "ask_questions_as_choices",
  "interface_language", "keep_awake", "keep_awake_when", "auto_select_account", "auto_switch_on_usage_limit",
  "auto_start_quota_windows", "default_clone_dir", "resume_sessions_on_start",
  ...GIT_WORKFLOW_ITEM_KEYS.map((item) => `git_workflow.${item}`),
  ...forEachAgent((agent) => [`agent_tags.${agent}`]),
  ...FIXTURE_PROVIDERS.flatMap(({ id, settings }) => settings.map((setting) => `provider_settings.${id}.${setting.id}`)),
]);

/** What a test or scenario may set on a console: the per-agent settings may name only the agents (and launch fields) they set. */
export type ConsoleExtra = Partial<Omit<Console, "accounts" | "launch" | "project_session_launch">> & {
  accounts?: Partial<PerAgent<string | null>>;
  launch?: Partial<PerAgent<Partial<Launch>>>;
  project_session_launch?: Partial<PerAgent<Partial<Launch>>>;
};

/** What a test or scenario may set on a project, likewise. */
export type ProjectExtra = Partial<Omit<Project, "accounts" | "launch">> & {
  accounts?: Partial<Project["accounts"]>;
  launch?: Partial<PerAgent<Partial<Launch>>>;
};

/** `base` with what `extra` sets laid over it: a per-agent group changes only the agents (and launch fields) it names. */
export function consoleWith(base: Console, extra: ConsoleExtra): Console {
  const { accounts, launch, project_session_launch, ...rest } = extra;
  return {
    ...base,
    ...rest,
    accounts: { ...base.accounts, ...accounts } as Console["accounts"],
    launch: layered(base.launch, launch),
    project_session_launch: layered(base.project_session_launch, project_session_launch),
  };
}

/** `base` with what `extra` sets laid over it, as `consoleWith` does for a console. */
export function projectWith(base: Project, extra: ProjectExtra): Project {
  const { accounts, launch, ...rest } = extra;
  return {
    ...base,
    ...rest,
    accounts: { ...base.accounts, ...accounts } as Project["accounts"],
    launch: layered(base.launch, launch),
  };
}

/** An account of `agent` in `config_dir` that is not shared with the default configuration and not turned off; `extra`
 * sets what a scenario shows otherwise. */
export function accountOf(id: string, agent: Agent, name: string, config_dir: string, extra: Partial<Account> = {}): Account {
  return { id, agent, name, config_dir, share_default_config: false, share_default_config_incomplete: false, disabled: false, versions: ACCOUNT_VERSIONS, ...extra };
}

/** One agent's CLI as the daemon last checked it: installed through the CLI's own installer at 2.1.319, which is also the
 * latest version published, so up to date; `extra` sets what a scenario shows otherwise. */
export function agentCliStatusOf(agent: Agent, extra: Partial<AgentCliStatus> = {}): AgentCliStatus {
  return { agent, installed_version: "2.1.319", install_method: "native", latest_version: "2.1.319", latest_error: null, latest_detail: null, checked_at: minutesAgo(1), update: null, ...extra };
}

/** An update of an agent's CLI, running since a minute before the page loaded; `extra` ends it or says why it failed. */
export function agentCliUpdateOf(extra: Partial<AgentCliUpdate> = {}): AgentCliUpdate {
  return { state: "running", started_at: minutesAgo(1), finished_at: null, error: null, detail: null, ...extra };
}

/** The statuses a snapshot holds, keyed by agent as the store keeps them. */
export function agentCliStatusesOf(statuses: readonly AgentCliStatus[]): Map<Agent, AgentCliStatus> {
  return new Map<Agent, AgentCliStatus>(statuses.map((status) => [status.agent, status]));
}

export function consoleOf(id: string, name: string, extra: ConsoleExtra = {}): Console {
  return consoleWith({
    id,
    name,
    workdir: `/Users/dev/octoboard/${id}`,
    default_agent: "claude",
    project_session_agent: null,
    accounts: { claude: null, codex: null, grok: null },
    launch: { claude: NO_LAUNCH, codex: NO_LAUNCH, grok: NO_LAUNCH },
    project_session_launch: { claude: NO_LAUNCH, codex: NO_LAUNCH, grok: NO_LAUNCH },
    icon: null,
    console_session_prompt: null,
    created_at: minutesAgo(60 * 24 * 30),
    versions: CONSOLE_VERSIONS,
  }, extra);
}

export function projectOf(id: string, consoleId: string, name: string, extra: ProjectExtra = {}): Project {
  return projectWith({
    id,
    console_id: consoleId,
    host_id: LOCAL_HOST.id,
    name,
    path: `/Users/dev/code/${name.toLowerCase().replaceAll(" ", "-")}`,
    default_agent: null,
    source: "local",
    remote_url: null,
    trust_consent: false,
    pinned: false,
    tags: [],
    description: null,
    accounts: { claude: { kind: "inherit" }, codex: { kind: "inherit" }, grok: { kind: "inherit" } },
    launch: { claude: NO_LAUNCH, codex: NO_LAUNCH, grok: NO_LAUNCH },
    git_workflow: FOLLOWING_GIT_WORKFLOW,
    versions: PROJECT_VERSIONS,
  }, extra);
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
    agent_session_id: null,
    console_id: consoleId,
    project_id: projectId ?? null,
    host_id: LOCAL_HOST.id,
    kind: projectId ? "project" : "console",
    origin: projectId ? "user" : "console",
    title,
    status,
    has_conversation: true,
    compatibility: {},
    pinned: false,
    locked: false,
    started_at: minutesAgo(90),
    bound_to: null,
    colour: null,
    ordinal: null,
    account_id: null,
    config_dir: null,
    ended_at: status === "archived" ? minutesAgo(30) : null,
    ...extra,
    // A session with a parent it reports to is a worker; one without is a coordinator.
    role: extra.role ?? (extra.bound_to ? "worker" : "coordinator"),
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
  return { id, session_id: session.id, kept: false, keepable: true, command, cwd: "/Users/dev/code/website", cwd_missing: false, started_at: minutesAgo(minutes), process_count: 1, memory_bytes: 48_000_000, ports: [], ...extra };
}

/** A project process left behind in a removed directory of a project, which belongs to no session. */
export function leftoverOf(
  id: string,
  command: string,
  minutes: number,
  extra: Partial<Omit<ProcessTree, "id" | "session_id" | "command" | "started_at">> = {},
): Omit<ProcessTree, "session_id"> {
  return { id, kept: false, keepable: true, command, cwd: "/Users/dev/code/website/.worktrees/old-nav", cwd_missing: true, started_at: minutesAgo(minutes), process_count: 1, memory_bytes: 48_000_000, ports: [], ...extra };
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

/** A page's content that is one Markdown block, for a test or a scenario that needs a page and not its components. */
export function markdownContent(text: string, title?: string): PageContent {
  return { ...(title === undefined ? {} : { title }), body: [{ type: "markdown", text }] };
}

/** A page pushed `minutes` ago, not yet submitted unless a `submission` is given, which the daemon would have marked
 * once, so it is revision 1 (a page never marked is 0). */
export function pageOf(id: string, sessionId: string, content: PageContent, minutes: number, submission: Page["submission"] = null): Page {
  return { id, session_id: sessionId, content, submission, submission_revision: submission === null ? 0 : 1, created_at: minutesAgo(minutes) };
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

/** The release version the fixtures' daemon says it is in its hello. */
export const FIXTURE_DAEMON_VERSION = "0.1.0";

/** A connected window's state after its first snapshot, over which a scenario lays what it differs
 * in. Only what is given is set; the rest is the empty state. The daemon greeted it with a hello of the window's own
 * generation unless `daemon` says otherwise. */
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
  daemon?: State["daemon"];
  /** The daemon host's own facts and home directory, for a scenario on a machine that is not the Unix one the rest assumes. */
  host?: HostInfo;
  homeDir?: string;
}): Partial<State> {
  return {
    connectionState: parts.connectionState ?? "open",
    daemon: parts.daemon === undefined ? { version: FIXTURE_DAEMON_VERSION, generation: PROTOCOL_GENERATION } : parts.daemon,
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
    providers: FIXTURE_PROVIDERS,
    agentAvailability: availabilityOf(FIXTURE_PROVIDERS),
    // The daemon's settings always carry the version of every field, so a scenario's own settings are completed with them.
    settings: { ...(parts.settings ?? { ...DEFAULT_SETTINGS, default_clone_dir: "/Users/dev/Projects" }), versions: { ...SETTINGS_VERSIONS, ...parts.settings?.versions } },
    accountQuotas: parts.accountQuotas ?? [],
    accountSharing: accountSharingOf(parts.accountSharing ?? []),
    homeDir: parts.homeDir ?? "/Users/dev",
    ...(parts.host && { host: parts.host }),
  };
}

/** What a snapshot says of a Mac that can be held awake (Prevent sleep): the daemon host's facts and its home
 * directory, for `snapshotState`, so a scenario shows the Prevent sleep setting whichever client views the daemon. */
export const MACOS_MACHINE = {
  host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: true },
  homeDir: "/Users/dev",
} as const satisfies { host: HostInfo; homeDir: string };

/** What a snapshot says of a Windows machine: the daemon host's facts and its home directory, for `snapshotState`, so a
 * scenario about a Windows host shows its paths as the daemon spells them (`C:\Users\dev`, never shortened to `~`). */
export const WINDOWS_MACHINE = {
  host: { os: "windows", path_style: "windows", file_links: true, trash: "recycle_bin", settings: "settings", keep_awake: true },
  homeDir: "C:\\Users\\dev",
} as const satisfies { host: HostInfo; homeDir: string };

/** `state` as the daemon reports it when the account switches of the sessions `ids` are under way
 * (`Session.switching_account`), whatever those sessions were doing. */
export function switchingAccount(state: Partial<State>, ids: string[]): Partial<State> {
  const sessions = new Map(state.sessions);
  for (const id of ids) {
    const session = sessions.get(id);
    if (!session) throw new Error(`switchingAccount: no session ${id} in the state`);
    sessions.set(id, { ...session, switching_account: true });
  }
  return { ...state, sessions };
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

/** A notice the daemon sends about a session (`session_notice`), shown as the app shows the real one: a failure, or a
 * notice carrying raw text, is an error alert and any other a toast. `message` is the daemon's English fallback for a
 * code the catalog lacks. */
export const sessionNoticeNotification = (session: string, code: string, params: MessageParams = {}, message = ""): NotificationFixture =>
  ({ kind: "broadcast", event: { type: "session_notice", session, code, params, message } });

/** An error the daemon sends that answers no request and names no session, shown as the app shows the real one. */
export const broadcastErrorNotification = (code: string, params: MessageParams = {}, message = ""): NotificationFixture =>
  ({ kind: "broadcast", event: { type: "error", id: null, code, params, message } });

/** The title the action that raises an error gives its alert (`NotificationFixture.title`), for a scenario that raises
 * the error itself rather than through the action: the message at `key`, worded in the viewing language, so a title the
 * app renames or drops breaks the scenario at type checking. */
export const failureTitle = (key: PlainMessageKey) => (t: Translate) => t(key);

/** The value in effect of every setting of every provider, as the daemon puts them in `Settings.provider_settings`: the
 * user's explicit choice (`choices`, the same shape) when there is one, otherwise what the setting follows without one.
 * A choice setting without a choice is `null`; a switch without a choice is its default. The gallery's daemon keeps a
 * scenario's `settings.provider_settings` as the explicit choices and reports this. By default the providers are the
 * registered ones; a scenario that serves another beside them passes the providers it serves. */
export function effectiveProviderSettings(choices: Settings["provider_settings"], providers: readonly ProviderInfo[] = FIXTURE_PROVIDERS): Settings["provider_settings"] {
  return Object.fromEntries(
    providers.map(({ id, settings }) => [
      id,
      Object.fromEntries(
        settings.map(({ id: setting, kind }): [string, boolean | string | null] => {
          const chosen = choices[id]?.[setting] ?? null;
          if (kind.type === "choice" || chosen !== null) return [setting, chosen];
          return [setting, kind.default === "on"];
        }),
      ),
    ]),
  );
}

/** What a configuration export carries of the user's explicit choices (`choices`, as `effectiveProviderSettings` takes
 * them): only the settings their descriptors export, only those the user chose for. A provider with none has no entry.
 * `providers` are the ones the machine serves, the registered ones by default. */
export function exportedProviderSettings(choices: Settings["provider_settings"], providers: readonly ProviderInfo[] = FIXTURE_PROVIDERS): DocumentAgentSettings["provider_settings"] {
  const exported: DocumentAgentSettings["provider_settings"] = {};
  for (const { id, settings } of providers) {
    const chosen = settings.flatMap(({ id: setting, exported: carried }) => {
      const value = choices[id]?.[setting];
      return carried && value !== undefined && value !== null ? [[setting, value] as const] : [];
    });
    if (chosen.length > 0) exported[id] = Object.fromEntries(chosen);
  }
  return exported;
}

/** The values in effect (`effectiveProviderSettings`) of the exported settings, as an import's review reports them for
 * this machine in `InspectedAgentSettings.current_provider_settings`. A provider that exports none has no entry. `providers`
 * are the ones the machine serves, the registered ones by default. */
export function currentExportedProviderSettings(effective: Settings["provider_settings"], providers: readonly ProviderInfo[] = FIXTURE_PROVIDERS): InspectedAgentSettings["current_provider_settings"] {
  const current: InspectedAgentSettings["current_provider_settings"] = {};
  for (const { id, settings } of providers) {
    const carried = settings.filter(({ exported }) => exported).map(({ id: setting }) => [setting, effective[id]?.[setting] ?? null] as const);
    if (carried.length > 0) current[id] = Object.fromEntries(carried);
  }
  return current;
}
