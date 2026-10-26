import { DaemonRequestError } from "../daemon-client";
import { consoleAccount, inheritedAccount } from "../accountChoices";
import { AGENT_LABEL, isAgentAdded, isAgentUnavailable, projectSessionAgent } from "../agents";
import { daemonMessage } from "../daemonMessage";
import { errorPresentation, noticeErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { currentLanguage } from "../i18n/language";
import { CONSOLE_SESSION_COLOURS, DEFAULT_ACCOUNT_NAME, isLive, type Account, type AccountLogin, type AccountSharing, type Agent, type ConsoleSessionColour, type BrowseEntry, type Console, type DirEntry, type Event, type PathLinkKind, type PathLinkView, type Project, type QuotaWindow, type RequestBody, type Session, type SharingCause, type SessionRecordsPresence, type ShellTerminal, type WorktreeOverride, type WorktreeOverrideChanges } from "../protocol";
import { accountQuotaOf } from "../quota/accountQuotaOf";
import { exhaustedQuotaWindow } from "../quota/quotaFormat";
import { applyDaemonEvent, createStateStore, type Daemon, type ToastRequest } from "../store";
import { FOLLOWING_WORKTREE_SETTINGS } from "../worktreeSettings";
import type { Scenario } from "./scenario";
import { terminalFixtureUrl } from "./fakeTerminal";
import { createConfigurationReplies } from "./fixtureConfiguration";
import { createGitReplies } from "./fixtureGit";
import { refuseUnfillable } from "./banner";
import { SAMPLE_MODELS } from "./fixtures/agentModels";
import { SAMPLE_FILES, type FixtureError, type FixtureFile, type FixtureFiles } from "./fixtures/projectFiles";
import { SAMPLE_GIT } from "./fixtures/projectGit";

/** How long after the app subscribes to toasts the scenario's own are raised, for the stack to be
 * on screen to receive them. */
const TOAST_DELAY_MS = 300;

const HOME = "/Users/dev";

/** What the CLI said when a login the scenario ends as `failed` did not go through. */
const LOGIN_FAILURE_DETAIL = [
  "Login failed: Request failed with status code 400",
  "    authorization endpoint rejected the device code",
  "    /Users/dev/.claude-work/مرحبا/credentials.json: permission denied",
].join("\n");

/** The names the real daemon's file name search skips at every depth, with everything beneath them:
 * Git's metadata and the files an operating system leaves in a folder, matched ignoring ASCII case
 * (stored folded), and `._` starts AppleDouble. */
const LEFT_OUT_NAMES = new Set([
  ".git",
  ".ds_store",
  ".appledouble",
  ".spotlight-v100",
  ".trashes",
  ".fseventsd",
  ".temporaryitems",
  "thumbs.db",
  "desktop.ini",
]);

/** The failure the daemon keeps of a change that failed part-way, from the `account_sharing_failed` error a scenario stages. */
function failedSharing(error: FixtureError, share: boolean): NonNullable<AccountSharing["failure"]> {
  return {
    share,
    ...(error.params?.cause && { cause: error.params.cause as SharingCause }),
    items: JSON.parse(error.params?.items ?? "[]") as NonNullable<AccountSharing["failure"]>["items"],
    detail: error.params?.detail ?? error.message,
  };
}

/** Only A to Z are folded, as the daemon's `eq_ignore_ascii_case` does; `toLowerCase` would also fold
 * characters like the Kelvin sign into a letter. */
function isLeftOut(name: string): boolean {
  return LEFT_OUT_NAMES.has(name.replace(/[A-Z]/g, (letter) => letter.toLowerCase())) || name.startsWith("._");
}

/** A path as the daemon compares one: `~` and `~/` expanded against the home directory, `.` and
 * `..` and repeated or trailing slashes resolved lexically, symlinks not followed. */
function normalizedPath(path: string): string {
  const rest = path.startsWith("~/") ? path.slice(2) : undefined;
  const expanded = path === "~" ? HOME : rest === undefined ? path : rest.startsWith("/") ? rest : `${HOME}/${rest}`;
  const parts: string[] = [];
  for (const part of expanded.split("/")) {
    if (part === "..") parts.pop();
    else if (part !== "" && part !== ".") parts.push(part);
  }
  return `${expanded.startsWith("/") ? "/" : ""}${parts.join("/")}`;
}

/** The refusal to open a session, or hand one off, on an agent the user has not added. */
const agentNotAdded = (agent: Agent) => ({
  code: "agent_not_added",
  params: { agent: AGENT_LABEL[agent] },
  message: `${AGENT_LABEL[agent]} is not added. Add it in Settings → Agent accounts to open a session with it.`,
});

/** A text as the daemon stores one that may be left blank: trimmed, and none when nothing is left
 * (`trimmed_or_none` of `launch_settings.rs`). */
const trimmedOrNull = (text: string | null | undefined): string | null => text?.trim() || null;

/** What a `create_console` or `update_console` request sets of the console, each field only where
 * the request names it; an `update_console` that leaves a field out leaves it alone, and one that
 * names it as `null` clears it, as `update_console` of `coordinator.rs` does. Account and model
 * references are taken as given, not checked against the settings or an agent's list. */
function consoleFields(body: Extract<RequestBody, { type: "create_console" | "update_console" }>): Partial<Console> {
  const fields: Partial<Console> = {};
  if (body.name !== undefined) fields.name = body.name.trim();
  if (body.default_agent !== undefined) fields.default_agent = body.default_agent;
  if (body.project_session_agent !== undefined) fields.project_session_agent = body.project_session_agent;
  for (const agent of ["claude", "codex", "grok"] as const) {
    const account = body[`${agent}_account_id`];
    if (account !== undefined) fields[`${agent}_account_id`] = account;
    const model = body[`${agent}_console_session_model`];
    if (model !== undefined) fields[`${agent}_console_session_model`] = trimmedOrNull(model);
    const effort = body[`${agent}_console_session_effort`];
    if (effort !== undefined) fields[`${agent}_console_session_effort`] = trimmedOrNull(effort);
    const projectModel = body[`${agent}_project_session_model`];
    if (projectModel !== undefined) fields[`${agent}_project_session_model`] = trimmedOrNull(projectModel);
    const projectEffort = body[`${agent}_project_session_effort`];
    if (projectEffort !== undefined) fields[`${agent}_project_session_effort`] = trimmedOrNull(projectEffort);
  }
  if (body.icon !== undefined) fields.icon = body.icon;
  if (body.console_session_prompt !== undefined) fields.console_session_prompt = trimmedOrNull(body.console_session_prompt);
  return fields;
}

/** Mutable project account and launch settings, retaining absent-versus-cleared values. */
function projectFields(body: Extract<RequestBody, { type: "add_project" | "update_project" }>): Partial<Project> {
  const fields: Partial<Project> = {};
  if (body.default_agent !== undefined) fields.default_agent = body.default_agent;
  for (const agent of ["claude", "codex", "grok"] as const) {
    const account = body[`${agent}_account_id`];
    if (account !== undefined) fields[`${agent}_account_id`] = account ?? { kind: "inherit" };
    const model = body[`${agent}_session_model`];
    if (model !== undefined) fields[`${agent}_session_model`] = trimmedOrNull(model);
    const effort = body[`${agent}_session_effort`];
    if (effort !== undefined) fields[`${agent}_session_effort`] = trimmedOrNull(effort);
  }
  return fields;
}

/** A project's own worktree settings after a request's: per item, absent keeps it, `null` goes back to following the
 * application-wide value, and a value becomes the project's own. */
function projectWorktrees(current: WorktreeOverride, changes: WorktreeOverrideChanges | undefined): WorktreeOverride {
  return { ...current, ...changes };
}

function normalizeTags(tags: string[]): string[] {
  const seen = new Set<string>();
  return tags.map((tag) => tag.trim()).filter((tag) => {
    if (!tag || seen.has(tag.toLowerCase())) return false;
    seen.add(tag.toLowerCase());
    return true;
  });
}

/** A listing for a path the scenario gave none for: a few directories, some of them repositories. */
function genericListing(path: string): { path: string; entries: DirEntry[] } {
  const resolved = path === "~" ? HOME : path;
  const base = resolved === "/" ? "" : resolved;
  const entries = ["code", "Documents", "Downloads", "work", "repos"].map((name, i) => ({
    name,
    path: `${base}/${name}`,
    is_git_repo: i % 2 === 0,
  }));
  return { path: resolved, entries };
}

/** Whether Git ignores `path`: the fixture marks it, or a folder above it, `ignored`. */
function isIgnored(files: FixtureFiles, path: string): boolean {
  const segments = path.split("/");
  return segments.some((_, i) => files[segments.slice(0, i + 1).join("/")]?.ignored === true);
}

/** The entries directly under `dir` among `files`, as `list_project_dir` sends them. */
function entriesUnder(files: FixtureFiles, dir: string): BrowseEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  return Object.entries(files)
    .filter(([path]) => path !== dir && path.startsWith(prefix) && !path.slice(prefix.length).includes("/"))
    .map(([path, file]): BrowseEntry => ({
      name: path.slice(prefix.length),
      kind: "dir" in file ? "directory" : "file",
      size: "text" in file ? new TextEncoder().encode(file.text).length : "image" in file ? Math.floor((file.image.data.length * 3) / 4) : "binary" in file ? file.binary.size : null,
      version: "dir" in file ? null : "1",
      target: null,
      ...(isIgnored(files, path) && { ignored: true }),
    }))
    .sort((a, b) => (a.name < b.name ? -1 : 1));
}

/** The window's name in a quota refusal; mirrors the daemon's English `window_name`. */
function quotaWindowName(window: QuotaWindow): string {
  if (window.label) return window.label;
  if (window.kind === "session") return "5-hour";
  if (window.kind !== "credit_period") return "Weekly";
  const periodDays = (window.window_minutes ?? 0) / (24 * 60);
  if (periodDays >= 6 && periodDays <= 8) return "Weekly";
  return periodDays >= 28 && periodDays <= 31 ? "Monthly" : "Credits";
}

/**
 * A `Daemon` with no connection behind it, for the gallery: the store holds the scenario's state as
 * it is, requests are answered from the scenario (`list_dir`, `list_pages`, the browse requests) or
 * accepted and ignored (settings and trusted folders are updated, a console is added or saved, and
 * session-file inspections are answered explicitly and deleting sessions removes them), and the scenario's toasts are raised once the app is
 * listening.
 *
 * A project pane's Git-mode requests are answered by `fixtureGit.ts` instead, and the export and import
 * of configuration by `fixtureConfiguration.ts`; each is asked before anything here and says with
 * `undefined` that a request is not one of its own.
 */
export function createFixtureDaemon(scenario: Scenario): Daemon {
  const store = createStateStore(scenario.state);
  const toastListeners = new Set<(toast: ToastRequest) => void>();
  const emitToast = (toast: ToastRequest) => toastListeners.forEach((listener) => listener(toast));
  let toastsRaised = false;
  let openedSessionCount = 0;
  let createdShellTerminalCount = 0;
  let createdProjectCount = 0;
  // The daemon remembers its ordinal counter independently of session history. Seed it from
  // fixture ordinals once, so renaming or deleting a session never reuses an ordinal.
  const initial = store.getState();
  const nextConsoleOrdinals = new Map([...initial.consoles.keys()].map((id) => {
    const sessions = [...initial.sessions.values()].filter((session) => session.console_id === id && session.role === "console");
    return [id, Math.max(0, ...sessions.map((session, index) => session.ordinal ?? index + 1)) + 1];
  }));

  const raiseScenarioToasts = () => {
    if (toastsRaised) return;
    toastsRaised = true;
    for (const { code, params, message, ...toast } of scenario.toasts ?? []) {
      const failure = code ? noticeErrorPresentation(code, params ?? {}, message) : undefined;
      emitToast({
        ...toast,
        // Mirror the store: keep the full plain message and structured launch title separately.
        ...(code && {
          daemonTitle: failure ? new DaemonRequestError(message, `${code}.title`, params ?? {}) : undefined,
          daemonError: failure ?? new DaemonRequestError(message, code, params ?? {}),
        }),
        message: code ? daemonMessage(currentLanguage(), code, params ?? {}, message, store.getState()) : message,
      });
    }
  };

  const restoredFiles = new Map<string, FixtureFiles>();
  const restoredGit = new Map<string, ReturnType<typeof createGitReplies>>();
  const projectFiles = (project: string) => restoredFiles.get(project) ?? scenario.filesByProject?.[project] ?? scenario.files ?? SAMPLE_FILES;
  const git = scenario.git === undefined ? SAMPLE_GIT : scenario.git;
  const fail = ({ code, params, message }: FixtureError): never => {
    throw new DaemonRequestError(daemonMessage(currentLanguage(), code, params, message, store.getState()), code, params);
  };
  /** The account a new session of `agent` launches under: the one named, or, when none is, the one
   * the project (else the console) resolves to. */
  const launchAccount = (owner: Console, project: Project | undefined, agent: Agent, account: string | null | undefined) => {
    const current = store.getState();
    const accountId = account === undefined
      ? project ? inheritedAccount(owner, project, agent, current.settings.accounts) : consoleAccount(owner, agent, current.settings.accounts)
      : account;
    return current.settings.accounts.find((candidate) => candidate.id === accountId && candidate.agent === agent);
  };
  /** Refuses a session on, or a switch to, an account whose last reading in the store has a session, weekly or
   * credit-period window used up (100% or more) that has not reset, as the daemon does with the reading it holds:
   * a model's weekly window never counts, an account not read yet is never refused, and of several used-up
   * windows the one that resets last (one with no reset time after all) is named, in English, with its kind, length
   * and whether the reading also has a model's weekly window, from which the app words it. */
  const refuseIfQuotaExhausted = (agent: Agent, account: Account | undefined) => {
    const current = store.getState();
    const reading = accountQuotaOf(current.accountQuotas, agent, account?.id ?? null);
    const blocking = exhaustedQuotaWindow(reading, Date.now());
    if (!blocking) return;
    const windowName = quotaWindowName(blocking);
    const modelWeekly = typeof reading === "object" && reading.windows.some((window) => window.kind === "weekly_model");
    const name = account?.name ?? current.settings.default_account_names?.[agent] ?? DEFAULT_ACCOUNT_NAME;
    return fail({
      code: "account_quota_exhausted",
      params: {
        agent: AGENT_LABEL[agent],
        account: name,
        window: windowName,
        window_kind: blocking.kind,
        ...(blocking.window_minutes != null && { window_minutes: String(blocking.window_minutes) }),
        ...(modelWeekly && { model_weekly: "true" }),
        ...(blocking.resets_at != null && { resets_at: String(blocking.resets_at) }),
      },
      message: `${name} has reached its ${AGENT_LABEL[agent]} usage limit (${windowName})`,
    });
  };
  /** Refuses a session on, a handoff to, or a switch to an account the user turned off, as the daemon does before it
   * looks at the quota: `account` is the account the launch resolves to, `undefined` the agent's default account. The
   * name in the error is the account's, the default account's being the one the user gave it (else the reserved word). */
  const refuseIfDisabled = (agent: Agent, account: Account | undefined) => {
    const current = store.getState();
    const disabled = account ? account.disabled === true : current.settings.disabled_default_accounts?.includes(agent) === true;
    if (!disabled) return;
    const name = account?.name ?? current.settings.default_account_names?.[agent] ?? DEFAULT_ACCOUNT_NAME;
    return fail({
      code: "account_disabled",
      params: { agent: AGENT_LABEL[agent], account: name },
      message: `${name} is disabled for ${AGENT_LABEL[agent]}. Choose another account, or turn it back on in Settings → Agent accounts.`,
    });
  };
  /** Refuses what the daemon turns away while a change of the sharing of `accountId` runs: a session opened on it, one
   * of its sessions resumed, a switch onto or away from it, a handoff from it. The default account (`null`) has no sharing. */
  const refuseIfSharingChanges = (accountId: string | null | undefined) => {
    if (!accountId) return;
    const current = store.getState();
    if (!current.accountSharing.get(accountId)?.progress) return;
    const name = current.settings.accounts.find((candidate) => candidate.id === accountId)?.name ?? accountId;
    return fail({ code: "account_sharing_in_progress", params: { account: accountId, name }, message: `a change of the sharing of account \`${name}\` is running` });
  };
  /** A console session's colour as the daemon assigns one: the first of the palette that no session of
   * the console that is not archived wears (an interrupted one still holds its colour), else the first. */
  const freshColour = (consoleId: string): ConsoleSessionColour => {
    const used = new Set([...store.getState().sessions.values()].filter((session) => session.console_id === consoleId && session.role === "console" && session.status !== "archived").map((session) => session.colour));
    return CONSOLE_SESSION_COLOURS.find((colour) => !used.has(colour)) ?? CONSOLE_SESSION_COLOURS[0];
  };
  const refuseLocked = (sessions: Iterable<Session>) => {
    const locked = [...sessions].find((session) => session.locked);
    if (locked) return fail({ code: "session_locked", params: { session: locked.id }, message: "This session is locked" });
  };
  const gitReply = createGitReplies(git, fail);
  const configurationReply = createConfigurationReplies(scenario.configuration, store.getState, fail);
  const recordsPresence = (ids: string[]): SessionRecordsPresence => {
    let presence: SessionRecordsPresence = "absent";
    for (const id of ids) {
      const item = store.getState().sessions.has(id) ? scenario.sessionRecords?.[id] ?? "unknown" : "unknown";
      if (item === "present") return "present";
      if (item === "unknown") presence = "unknown";
    }
    return presence;
  };
  /** A browse request's path, or the daemon error it is answered with, worded as the store words one. */
  const browsed = (files: FixtureFiles, path: string): FixtureFile => {
    const file = files[path];
    if (file?.error) {
      const { code, params, message } = file.error;
      throw new DaemonRequestError(daemonMessage(currentLanguage(), code, params, message, store.getState()), code, params);
    }
    if (!file) {
      const params = { path };
      throw new DaemonRequestError(daemonMessage(currentLanguage(), "file_not_found", params, `${path} was not found`, store.getState()), "file_not_found", params);
    }
    return file;
  };

  return {
    store,
    request: async (body): Promise<Event> => {
      if (body.type === "list_project_branches") {
        const result = scenario.projectBranches?.[body.project];
        if (result === "pending") return new Promise<Event>(() => {});
        if (result) return fail(result);
      }
      const fromGit = ("project" in body && typeof body.project === "string" ? restoredGit.get(body.project) ?? gitReply : gitReply)(body);
      if (fromGit !== undefined) return fromGit;
      const fromConfiguration = configurationReply(body);
      if (fromConfiguration !== undefined) return fromConfiguration;
      switch (body.type) {
        case "archive_session": {
          const current = store.getState();
          const session = current.sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: "Unknown session" });
          if (scenario.sessionArchiveError) return fail(scenario.sessionArchiveError);
          // Match coordinator::team over the valid fixture hierarchy: owners reach two levels.
          const childrenOf = (owner: Session) => [...current.sessions.values()].filter((child) => child.bound_to === owner.id);
          const direct = childrenOf(session);
          const team = [...direct.flatMap(childrenOf), ...direct];
          refuseLocked([session, ...team]);
          const descendants = team.filter((child) => child.status !== "archived");
          const sessions = new Map(current.sessions);
          const ended_at = Date.now();
          for (const record of [...descendants, session]) sessions.set(record.id, { ...record, status: "archived", ended_at });
          store.setState({ sessions });
          return { type: "ack" };
        }
        case "resume_session": {
          if (scenario.sessionResumes?.[body.session] === "pending") return new Promise<Event>(() => {});
          const current = store.getState();
          const session = current.sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          if (session.status !== "archived" && session.status !== "interrupted") return fail({ code: "session_already_running", params: { session: body.session }, message: "this session is not interrupted or archived" });
          refuseIfSharingChanges(session.account_id);
          // Reopening a child brings back archived owners, while siblings stay archived.
          const sessions = new Map(current.sessions);
          const reopen = (record: Session) => sessions.set(record.id, { ...record, status: "idle", ended_at: undefined });
          reopen(session);
          let owner = session.bound_to;
          while (owner) {
            const record = sessions.get(owner);
            if (!record || record.status !== "archived") break;
            reopen(record);
            owner = record.bound_to;
          }
          store.setState({ sessions });
          return { type: "ack" };
        }
        case "switch_project_branch": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          const result = scenario.projectBranchSwitches?.[body.project]?.[body.branch];
          if (!result) return refuseUnfillable(`switch_project_branch (${body.project}, ${body.branch}): no post-switch repository, files and badge supplied`);
          if ("code" in result) return fail(result);
          restoredFiles.set(body.project, result.files);
          restoredGit.set(body.project, createGitReplies(result.git, fail));
          store.setState((state) => ({ gitStatuses: new Map(state.gitStatuses).set(body.project, result.status) }));
          return { type: "ack" };
        }
        case "restore_project_changes": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          const result = scenario.projectRestores?.[body.project];
          if (!result) return refuseUnfillable(`restore_project_changes (${body.project}): no post-restore repository and files supplied`);
          if ("code" in result) return fail(result);
          restoredFiles.set(body.project, result.files);
          restoredGit.set(body.project, createGitReplies(result.git, fail));
          return { type: "ack" };
        }
        case "switch_session_account": {
          const current = store.getState();
          const session = current.sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          if (session.status === "archived") return fail({ code: "session_archived", params: { session: body.session }, message: "An archived session cannot switch accounts" });
          const account = body.account === null ? undefined : current.settings.accounts.find((account) => account.id === body.account && account.agent === session.agent);
          if (body.account !== null && !account) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
          if ((session.account_id ?? null) === body.account) return fail({ code: "session_already_on_account", params: { session: body.session }, message: "this session is on that account already" });
          refuseIfSharingChanges(session.account_id);
          refuseIfSharingChanges(account?.id);
          refuseIfDisabled(session.agent, account);
          refuseIfQuotaExhausted(session.agent, account);
          // The daemon broadcasts the session as switching while the switch holds its claim, and the
          // session as it is once the switch ends. What the window adds for its own batch is not the daemon's to send.
          const { switching_account: _, ...resting } = session;
          applyDaemonEvent(store, { type: "session_upserted", session: { ...resting, switching_account: true } });
          const reply = scenario.sessionAccountSwitches?.[body.session];
          if (reply === "pending") return new Promise<Event>(() => {});
          if (reply) {
            applyDaemonEvent(store, { type: "session_upserted", session: resting });
            return fail(reply);
          }
          applyDaemonEvent(store, { type: "session_upserted", session: { ...resting, account_id: account?.id, config_dir: account?.config_dir } });
          return { type: "ack" };
        }
        case "hand_off_session": {
          const current = store.getState();
          const session = current.sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          // A refusal the scenario stages: one the window could not have seen coming.
          const staged = scenario.sessionHandoffs?.[body.session];
          if (staged === "pending") return new Promise<Event>(() => {});
          if (staged) return fail(staged);
          if (session.status === "archived") return fail({ code: "session_archived", params: { session: session.id }, message: "An archived session cannot be handed off" });
          if (session.status === "working" || session.status === "starting") return fail({ code: "session_working", params: { session: session.id }, message: "The session is in the middle of a turn" });
          refuseLocked([session]);
          refuseIfSharingChanges(session.account_id);
          if (body.agent === session.agent) return fail({ code: "handoff_to_same_agent", params: { session: session.id, agent: AGENT_LABEL[session.agent] }, message: "The session already runs on that agent" });
          if (isAgentUnavailable(current.agentAvailability, body.agent)) return fail({ code: "agent_not_available", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} is not available` });
          if (!isAgentAdded(current.settings.added_agents, body.agent)) return fail(agentNotAdded(body.agent));
          if (body.account != null && !current.settings.accounts.some((account) => account.id === body.account && account.agent === body.agent)) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
          const owner = current.consoles.get(session.console_id);
          if (!owner) return fail({ code: "unknown_console", params: { console: session.console_id }, message: `unknown console ${session.console_id}` });
          const project = session.project_id ? current.projects.get(session.project_id) : undefined;
          const account = launchAccount(owner, project, body.agent, body.account);
          refuseIfDisabled(body.agent, account);
          refuseIfQuotaExhausted(body.agent, account);
          const ordinal = nextConsoleOrdinals.get(owner.id) ?? 1;
          if (!project) nextConsoleOrdinals.set(owner.id, ordinal + 1);
          // The receiving session takes the original's place; the original is archived and what it
          // owned is bound to the new one.
          const received: Session = {
            id: `s-gallery-opened-${++openedSessionCount}`,
            agent: body.agent,
            console_id: session.console_id,
            project_id: session.project_id,
            host_id: session.host_id,
            role: session.role,
            origin: session.origin,
            title: session.title,
            status: "working",
            has_conversation: false,
            pinned: session.pinned,
            locked: false,
            started_at: Date.now(),
            ...(project ? { bound_to: session.bound_to } : { ordinal, colour: freshColour(owner.id) }),
            ...(account && { account_id: account.id, config_dir: account.config_dir }),
            ...(body.model !== undefined && { model: body.model }),
            ...(body.effort !== undefined && { effort: body.effort }),
          };
          const sessions = new Map(current.sessions);
          for (const owned of current.sessions.values()) if (owned.bound_to === session.id) sessions.set(owned.id, { ...owned, bound_to: received.id });
          sessions.set(session.id, { ...session, status: "archived", ended_at: Date.now() });
          sessions.set(received.id, received);
          store.setState({ sessions });
          return { type: "session_opened", session: received };
        }
        case "release_session": {
          const session = store.getState().sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          // The real daemon refuses a console session and an unbound one in prose, which reaches the window as an internal error.
          if (session.role === "console" || !session.bound_to) {
            const detail = session.role === "console"
              ? "a console session is never bound, so it has no owner to be released from"
              : `\`${session.id}\` is not bound to any owner, so there is nothing to release`;
            return fail({ code: "internal_error", params: { detail }, message: detail });
          }
          // Whatever the session owns stays bound to it, and so do its branches; only its own binding goes.
          store.setState((state) => ({ sessions: new Map(state.sessions).set(session.id, { ...session, bound_to: undefined }) }));
          return { type: "ack" };
        }
        // Native path operations are acknowledged without launching anything on the host.
        case "open_project_path":
        case "reveal_project_path": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          return { type: "ack" };
        }
        case "open_path_link":
          return { type: "ack" };
        case "open_project_directory": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          if (scenario.projectDirectoryOpenError) return fail(scenario.projectDirectoryOpenError);
          return { type: "ack" };
        }
        case "delete_project": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          refuseLocked([...store.getState().sessions.values()].filter((session) => session.project_id === body.project));
          if (body.trash_directory && scenario.projectDeleteError) return fail(scenario.projectDeleteError);
          return { type: "ack" };
        }
        case "set_session_pinned": {
          const session = store.getState().sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          const updated = { ...session, pinned: body.pinned };
          store.setState((state) => ({ sessions: new Map(state.sessions).set(body.session, updated) }));
          return { type: "ack" };
        }
        case "set_session_locked": {
          const session = store.getState().sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          store.setState((state) => ({ sessions: new Map(state.sessions).set(body.session, { ...session, locked: body.locked }) }));
          return { type: "ack" };
        }
        case "set_session_colour": {
          const session = store.getState().sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          const updated = { ...session, colour: body.colour };
          store.setState((state) => ({ sessions: new Map(state.sessions).set(body.session, updated) }));
          return { type: "ack" };
        }
        case "preview_console_session_title": {
          const owner = store.getState().consoles.get(body.console);
          if (!owner) return fail({ code: "unknown_console", params: { console: body.console }, message: `unknown console ${body.console}` });
          return { type: "console_session_title", console: body.console, title: owner.name };
        }
        case "locate_conversation_to_resume": {
          const located = scenario.conversationLocation ?? { kind: "not_found" as const };
          if (located === "pending") return new Promise<Event>(() => {});
          if ("code" in located) return fail(located);
          return { type: "conversation_located", location: located };
        }
        case "open_session": {
          if (scenario.sessionOpenError) return fail(scenario.sessionOpenError);
          const current = store.getState();
          const owner = current.consoles.get(body.console_id);
          if (!owner) return fail({ code: "unknown_console", params: { console: body.console_id }, message: `unknown console ${body.console_id}` });
          const project = body.project_id ? current.projects.get(body.project_id) : undefined;
          if (body.project_id && !project) return fail({ code: "unknown_project", params: { project: body.project_id }, message: `unknown project ${body.project_id}` });
          const agent = body.agent ?? (project ? project.default_agent ?? projectSessionAgent(owner) : owner.default_agent);
          if (!isAgentAdded(current.settings.added_agents, agent)) return fail(agentNotAdded(agent));
          const account = launchAccount(owner, project, agent, body.account);
          refuseIfSharingChanges(account?.id);
          refuseIfDisabled(agent, account);
          refuseIfQuotaExhausted(agent, account);
          const ordinal = nextConsoleOrdinals.get(owner.id) ?? 1;
          if (!project) nextConsoleOrdinals.set(owner.id, ordinal + 1);
          const session: Session = {
            id: `s-gallery-opened-${++openedSessionCount}`,
            agent,
            console_id: owner.id,
            project_id: project?.id,
            host_id: project?.host_id ?? "local",
            role: project ? "project" : "console",
            origin: "user",
            title: (body.title?.trim() || undefined) ?? project?.name ?? owner.name,
            status: body.task == null ? "idle" : "working",
            has_conversation: false,
            pinned: false,
            locked: false,
            started_at: Date.now(),
            ...(project ? { bound_to: body.bound_to } : { ordinal, colour: freshColour(owner.id) }),
            ...(account && { account_id: account.id, config_dir: account.config_dir }),
            ...(body.model !== undefined && { model: body.model }),
            ...(body.effort !== undefined && { effort: body.effort }),
          };
          store.setState({ sessions: new Map(current.sessions).set(session.id, session) });
          return { type: "session_opened", session };
        }
        case "update_settings": {
          const current = store.getState();
          let cloneDir = current.settings.default_clone_dir;
          if (body.default_clone_dir !== undefined) {
            const given = body.default_clone_dir;
            if (given.trim() === "") cloneDir = `${HOME}/Projects`;
            else {
              if (!given.startsWith("/") && given !== "~" && !given.startsWith("~/")) return fail({ code: "path_not_absolute", params: { path: given }, message: `\`${given}\` is not an absolute path; give the full path or start it with \`~/\`` });
              cloneDir = normalizedPath(given);
            }
          }
          store.setState({ settings: { ...current.settings, auto_pull_repositories: body.auto_pull_repositories ?? current.settings.auto_pull_repositories, auto_delete_unresumable_archives: body.auto_delete_unresumable_archives ?? current.settings.auto_delete_unresumable_archives, ...(body.auto_approve_session_requests !== undefined && { auto_approve_session_requests: body.auto_approve_session_requests }), ...(body.auto_approve_resume_requests !== undefined && { auto_approve_resume_requests: body.auto_approve_resume_requests }), ...(body.auto_approve_account_switches !== undefined && { auto_approve_account_switches: body.auto_approve_account_switches }), ...(body.auto_approve_handoffs !== undefined && { auto_approve_handoffs: body.auto_approve_handoffs }), ...(body.ask_questions_as_choices !== undefined && { ask_questions_as_choices: body.ask_questions_as_choices }), ...(body.auto_select_account !== undefined && { auto_select_account: body.auto_select_account }), ...(body.sidebar_order_by_status !== undefined && { sidebar_order_by_status: body.sidebar_order_by_status }), ...(body.auto_start_quota_windows !== undefined && { auto_start_quota_windows: body.auto_start_quota_windows }), ...(body.worktrees !== undefined && { worktrees: { ...current.settings.worktrees, ...body.worktrees } }), default_clone_dir: cloneDir } });
          return { type: "ack" };
        }
        case "set_codex_approval_reviewer": {
          if (scenario.accountApprovalError) return fail(scenario.accountApprovalError);
          store.setState({ settings: { ...store.getState().settings, codex_approval_reviewer: body.reviewer } });
          return { type: "ack" };
        }
        case "set_claude_approval_mode": {
          if (scenario.accountApprovalError) return fail(scenario.accountApprovalError);
          store.setState({ settings: { ...store.getState().settings, claude_approval_mode: body.mode } });
          return { type: "ack" };
        }
        case "set_account_disabled": {
          // `null` is the agent's default account, kept by agent in `disabled_default_accounts`; an owned account carries
          // the flag itself. Running sessions are not touched, and a request that changes nothing records nothing.
          const current = store.getState();
          if (body.account === null) {
            const held = current.settings.disabled_default_accounts ?? [];
            if (held.includes(body.agent) === body.disabled) return { type: "ack" };
            const disabled_default_accounts = body.disabled ? [...held, body.agent] : held.filter((agent) => agent !== body.agent);
            store.setState({ settings: { ...current.settings, disabled_default_accounts } });
            return { type: "ack" };
          }
          const account = current.settings.accounts.find((item) => item.id === body.account && item.agent === body.agent);
          if (!account) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
          if ((account.disabled === true) === body.disabled) return { type: "ack" };
          store.setState({ settings: { ...current.settings, accounts: current.settings.accounts.map((item) => item.id === account.id ? { ...item, disabled: body.disabled } : item) } });
          return { type: "ack" };
        }
        case "set_account_sharing": {
          // The default account is the directory the others share, so it has no choice. Turning
          // sharing on for an owned account whose directory is that one, under another spelling, is
          // refused the same way, and nothing is recorded. The spellings are compared lexically:
          // the fixture has no filesystem, so it does not resolve a symlink the way the daemon does.
          const current = store.getState();
          if (body.account === null) {
            return fail({ code: "default_account_not_shareable", params: {}, message: "the default account is the configuration the other accounts share" });
          }
          const account = current.settings.accounts.find((item) => item.id === body.account);
          if (!account) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
          const defaultDir = current.agentAvailability.get(account.agent)?.default_account_dir;
          const shares = account.share_default_config === true;
          if (body.share && !shares && defaultDir != null && normalizedPath(account.config_dir) === normalizedPath(defaultDir)) {
            return fail({ code: "default_account_not_shareable", params: { account: account.id, name: account.name }, message: `account \`${account.name}\`'s directory is the default account's own` });
          }
          // A change runs at once or is refused: the scenario's `accountSharing` marks an account whose change runs
          // or whose sessions or sign-in are in the way, and nothing is recorded for either.
          const state = current.accountSharing.get(account.id);
          if (state?.progress) return fail({ code: "account_sharing_in_progress", params: { account: account.id, name: account.name }, message: `a change of the sharing of account \`${account.name}\` is running` });
          if (state?.in_use && (state.in_use.sessions > 0 || state.in_use.signing_in)) {
            const { sessions, signing_in } = state.in_use;
            return fail({ code: "account_in_use", params: { account: account.id, name: account.name, sessions: String(sessions), signing_in: String(signing_in) }, message: `account \`${account.name}\` is in use` });
          }
          // Asking again for the recorded value does nothing, unless the last change did not finish: then it retries.
          if (shares === body.share && account.share_default_config_incomplete !== true) return { type: "ack" };
          // A refusal other than a failed change leaves nothing recorded.
          const error = scenario.accountSharingError;
          if (error && error.code !== "account_sharing_failed") return fail(error);
          // Otherwise the choice is recorded, and the account is complete unless the scenario fails the change; a
          // failure is not rolled back, and the daemon keeps what it left until the next change starts.
          store.setState((now) => {
            const { failure: _, ...rest } = now.accountSharing.get(account.id) ?? { account: account.id };
            const accountSharing = new Map(now.accountSharing);
            const failure = error && failedSharing(error, body.share);
            const next: AccountSharing = { ...rest, ...(failure && { failure }) };
            if (next.in_use !== undefined || next.progress !== undefined || next.failure !== undefined) accountSharing.set(account.id, next);
            else accountSharing.delete(account.id);
            return {
              accountSharing,
              settings: { ...now.settings, accounts: now.settings.accounts.map((item) => item.id === account.id ? { ...item, share_default_config: body.share, share_default_config_incomplete: error !== undefined } : item) },
            };
          });
          if (error) return fail(error);
          return { type: "ack" };
        }
        case "set_default_account_name": {
          const current = store.getState();
          const name = body.name.trim();
          const taken = name && current.settings.accounts.find((account) => account.agent === body.agent && account.name.trim().toLowerCase() === name.toLowerCase());
          if (taken) return fail({ code: "account_name_taken", params: { agent: AGENT_LABEL[body.agent], name: taken.name }, message: `${AGENT_LABEL[body.agent]} already has an account named ${taken.name}` });
          const { [body.agent]: _, ...others } = current.settings.default_account_names ?? {};
          // A blank name, or the reserved word in any letter case, clears the custom name.
          const clears = !name || name.toLowerCase() === DEFAULT_ACCOUNT_NAME.toLowerCase();
          store.setState({ settings: { ...current.settings, default_account_names: clears ? others : { ...others, [body.agent]: name } } });
          return { type: "ack" };
        }
        case "start_account_login": {
          const current = store.getState();
          if (scenario.loginStartError) return fail(scenario.loginStartError);
          if (current.accountLogins.some((login) => login.agent === body.agent && login.account === (body.account ?? null) && (login.status === "starting" || login.status === "waiting"))) {
            return fail({ code: "login_already_running", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} is already being signed in for this account` });
          }
          const account = body.account ?? null;
          const publish = (login: Partial<AccountLogin>) => store.setState((state) => ({
            accountLogins: [
              ...state.accountLogins.filter((held) => held.agent !== body.agent || held.account !== account),
              { agent: body.agent, account, status: "starting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null, ...login },
            ],
          }));
          publish({});
          // What the CLIs hand back: Claude Code and Codex a page to authorize on, Grok Build its device page and code.
          const grok = body.agent === "grok";
          const page = grok ? "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234" : "https://auth.example.com/authorize";
          const waiting = { status: "waiting" as const, authorization_url: page, device_code: grok ? "ABCD-1234" : null, opened_by_agent: grok };
          setTimeout(() => {
            publish(waiting);
            if (!scenario.loginOutcome) return;
            setTimeout(() => {
              const outcome = scenario.loginOutcome!;
              publish(outcome === "failed"
                ? { status: "failed", failure: { code: "login_failed", params: { agent: AGENT_LABEL[body.agent], detail: LOGIN_FAILURE_DETAIL }, message: `${AGENT_LABEL[body.agent]} reported: ${LOGIN_FAILURE_DETAIL}` } }
                : { status: outcome });
            }, 300);
          }, 50);
          return { type: "ack" };
        }
        case "cancel_account_login": {
          const account = body.account ?? null;
          const running = store.getState().accountLogins.find((login) => login.agent === body.agent && login.account === account && (login.status === "starting" || login.status === "waiting"));
          if (!running) return fail({ code: "login_not_running", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} is not being signed in for this account` });
          store.setState((state) => ({ accountLogins: state.accountLogins.map((login) => login === running ? { ...login, status: "cancelled", authorization_url: null, device_code: null } : login) }));
          return { type: "ack" };
        }
        case "create_account":
        case "update_account": {
          const current = store.getState();
          const editing = body.type === "update_account" ? current.settings.accounts.find((account) => account.id === body.account) : undefined;
          if (body.type === "update_account" && !editing) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
          const agent = body.type === "create_account" ? body.agent : editing!.agent;
          const customDefault = current.settings.default_account_names?.[agent];
          if (body.name !== undefined && customDefault && body.name.trim().toLowerCase() === customDefault.toLowerCase()) {
            return fail({ code: "account_name_taken", params: { agent: AGENT_LABEL[agent], name: customDefault }, message: `${AGENT_LABEL[agent]} already has an account named ${customDefault}` });
          }
          if (body.config_dir !== undefined) {
            const dir = normalizedPath(body.config_dir.trim());
            // The daemon refuses nothing for an update that leaves the account's own directory as it is.
            const defaultDir = current.agentAvailability.get(agent)?.default_account_dir;
            const owner = dir === editing?.config_dir
              ? undefined
              : current.settings.accounts.find((account) => account.agent === agent && account.id !== editing?.id && normalizedPath(account.config_dir) === dir)?.name
                ?? (defaultDir != null && normalizedPath(defaultDir) === dir ? DEFAULT_ACCOUNT_NAME : undefined);
            if (owner !== undefined) return fail({ code: "config_dir_taken", params: { agent: AGENT_LABEL[agent], name: owner }, message: `${AGENT_LABEL[agent]} account ${owner} already uses this config directory` });
          }
          return { type: "ack" };
        }
        case "delete_account": {
          const current = store.getState();
          if (!current.settings.accounts.some((account) => account.id === body.account)) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
          const fields = ["claude_account_id", "codex_account_id", "grok_account_id"] as const;
          const consoles = new Map([...current.consoles].map(([id, console]) => {
            const updated = { ...console };
            for (const field of fields) if (updated[field] === body.account) updated[field] = null;
            return [id, updated];
          }));
          const projects = new Map([...current.projects].map(([id, project]) => {
            const updated = { ...project };
            for (const field of fields) if (updated[field].kind === "account" && updated[field].id === body.account) updated[field] = { kind: "inherit" };
            return [id, updated];
          }));
          // An account that shares has its sharing turned off first, at once; when that does not finish, the account stays,
          // incomplete, with the failure kept, as `set_account_sharing` leaves it.
          const target = current.settings.accounts.find((account) => account.id === body.account)!;
          const error = scenario.accountSharingError;
          if (error?.code === "account_sharing_failed" && (target.share_default_config === true || target.share_default_config_incomplete === true)) {
            store.setState((now) => ({
              accountSharing: new Map(now.accountSharing).set(target.id, { account: target.id, failure: failedSharing(error, false) }),
              settings: { ...now.settings, accounts: now.settings.accounts.map((item) => item.id === target.id ? { ...item, share_default_config: false, share_default_config_incomplete: true } : item) },
            }));
            return fail(error);
          }
          store.setState({ consoles, projects, settings: { ...current.settings, accounts: current.settings.accounts.filter((account) => account.id !== body.account) } });
          return { type: "ack" };
        }
        case "add_agent": {
          if (scenario.agentListError) return fail(scenario.agentListError);
          const current = store.getState();
          const added = current.settings.added_agents;
          const state = current.agentAvailability.get(body.agent)?.availability;
          if (state === "unavailable") return fail({ code: "agent_not_installed", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} is not installed on this machine` });
          if (state !== "available") return fail({ code: "agent_availability_pending", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} cannot be added until it is known which agents are installed` });
          if (added?.includes(body.agent)) return fail({ code: "agent_already_added", params: { agent: AGENT_LABEL[body.agent] }, message: `${AGENT_LABEL[body.agent]} is already added` });
          store.setState({ settings: { ...current.settings, added_agents: [...(added ?? []), body.agent] } });
          return { type: "ack" };
        }
        case "remove_agent":
        case "reorder_agents": {
          if (scenario.agentListError) return fail(scenario.agentListError);
          const current = store.getState();
          const added = current.settings.added_agents;
          if (added === null) return fail({ code: "added_agents_undecided", params: {}, message: "the added agents cannot be changed until it is known which agents are installed" });
          if (body.type === "remove_agent") {
            if (added.includes(body.agent)) store.setState({ settings: { ...current.settings, added_agents: added.filter((agent) => agent !== body.agent) } });
          } else {
            if (body.agents.length !== added.length || !added.every((agent) => body.agents.filter((given) => given === agent).length === 1)) {
              return fail({ code: "agent_order_invalid", params: {}, message: "the new order must name each added agent exactly once" });
            }
            store.setState({ settings: { ...current.settings, added_agents: body.agents } });
          }
          return { type: "ack" };
        }
        case "remove_trusted_directory": {
          const path = normalizedPath(body.path);
          store.setState((current) => ({ trustedDirectories: current.trustedDirectories.filter((given) => given !== path) }));
          return { type: "ack" };
        }
        case "probe_path_links": {
          const root = body.project === undefined ? undefined : store.getState().projects.get(body.project)?.path;
          const existing = new Map(Object.entries(scenario.pathLinks ?? {}).map(([path, found]) => [normalizedPath(path), found]));
          const links = body.paths.map((path): { path: string; kind: PathLinkKind; view?: PathLinkView; viewer_only?: boolean } => {
            const absolute = path.startsWith("/") || path.startsWith("~") || root === undefined ? path : `${root}/${path}`;
            const found = existing.get(normalizedPath(absolute));
            if (found === undefined) return { path, kind: "none" };
            if (typeof found === "string") return { path, kind: found };
            if ("kind" in found) return { path, kind: found.kind, view: found.view };
            return { path, kind: "file", view: found.view, ...(found.viewerOnly && { viewer_only: true }) };
          });
          return { type: "path_links", links };
        }
        case "list_project_dir": {
          const files = projectFiles(body.project);
          const dir = browsed(files, body.path);
          const complete = "dir" in dir ? dir.complete !== false : true;
          return { type: "project_dir", project: body.project, worktree: null, path: body.path, root_id: "fixture", entries: entriesUnder(files, body.path), complete };
        }
        case "search_project_files": {
          const query = body.query.trim().toLowerCase();
          const files = projectFiles(body.project);
          const paths = Object.entries(files)
            .filter(([path, file]) => !("dir" in file) && !path.split("/").some(isLeftOut) && path.slice(path.lastIndexOf("/") + 1).toLowerCase().includes(query))
            .map(([path]) => path)
            .filter((path) => body.include_ignored === true || !isIgnored(files, path))
            .sort();
          const ignored = body.include_ignored === true ? paths.filter((path) => isIgnored(files, path)) : [];
          return { type: "project_file_matches", project: body.project, query: body.query, paths, ...(ignored.length > 0 && { ignored }), complete: true };
        }
        case "read_project_file": {
          const files = body.worktree === undefined ? projectFiles(body.project) : scenario.worktreeFiles?.[body.worktree] ?? {};
          const file = browsed(files, body.path);
          const source = { kind: "live" as const, root_id: "fixture", version: "1" };
          const content =
            "text" in file
              ? { size: new TextEncoder().encode(file.text).length, kind: "text" as const, media_type: null, text: file.text, data: null }
              : "image" in file
                ? { size: Math.floor((file.image.data.length * 3) / 4), kind: "binary" as const, media_type: file.image.mediaType, text: null, data: file.image.data }
                : { size: "binary" in file ? file.binary.size : 0, kind: "binary" as const, media_type: null, text: null, data: "" };
          return { type: "project_file", project: body.project, worktree: body.worktree ?? null, path: body.path, source, file: content };
        }
        case "list_dir": {
          // `~` is answered with the home directory it resolves to, as the real daemon does, so a
          // scenario names the listing it gives by that path.
          const path = body.path === "~" ? HOME : body.path;
          const listed = scenario.directories?.[path];
          if (listed && !Array.isArray(listed)) return fail(listed);
          return { type: "dir_listing", ...(listed ? { path, entries: listed } : genericListing(path)) };
        }
        case "detect_directory_agent":
        case "probe_git_remote": {
          const detected = scenario.detections?.[body.type === "probe_git_remote" ? body.remote_url : body.path];
          if (detected === "pending") return new Promise<Event>(() => {});
          if (detected && "code" in detected) return fail(detected);
          return { type: "agent_detected", agent: detected?.agent ?? null };
        }
        case "list_agent_models": {
          const listed = scenario.agentModels?.[body.agent];
          if (listed === "pending") return new Promise<Event>(() => {});
          const { models, unavailable = null } = listed ?? { models: SAMPLE_MODELS[body.agent] };
          return { type: "agent_models", agent: body.agent, account: body.account ?? null, models, unavailable };
        }
        case "list_pages": {
          const refused = scenario.pageListErrors?.[body.console_session];
          if (refused) return fail(refused);
          // As the real client's reply reaches the reducer, so a console session with no fixture
          // pages ends up listed as empty rather than not listed yet.
          const pages = store.getState().pages.get(body.console_session) ?? [];
          store.setState((state) =>
            state.pages.has(body.console_session)
              ? state
              : { pages: new Map(state.pages).set(body.console_session, pages) },
          );
          return { type: "page_list", console_session_id: body.console_session, pages };
        }
        case "create_console": {
          // As the daemon does, the new console reaches the store before the request is answered
          // (`coordinator.rs` broadcasts first), so a scenario can reach the first console's arrival.
          const id = `console-${Date.now()}`;
          store.setState((state) => ({
            consoles: new Map(state.consoles).set(id, {
              id,
              workdir: `${HOME}/.octoboard/consoles/${id}`,
              created_at: Date.now(),
              ...consoleFields(body),
              name: body.name.trim(),
              default_agent: body.default_agent,
            }),
          }));
          return { type: "ack" };
        }
        case "delete_console": {
          const current = store.getState();
          if (!current.consoles.has(body.console)) return fail({ code: "unknown_console", params: { console: body.console }, message: `unknown console ${body.console}` });
          refuseLocked([...current.sessions.values()].filter((session) => session.console_id === body.console));
          if ([...current.sessions.values()].some((session) => session.console_id === body.console && isLive(session.status))) return fail({ code: "console_has_running_sessions", params: {}, message: "This console still has running sessions" });
          const consoles = new Map(current.consoles);
          consoles.delete(body.console);
          const projects = new Map([...current.projects].filter(([, project]) => project.console_id !== body.console));
          const sessions = new Map([...current.sessions].filter(([, session]) => session.console_id !== body.console));
          const pages = new Map([...current.pages].filter(([id]) => sessions.has(id)));
          const gitStatuses = new Map([...current.gitStatuses].filter(([id]) => projects.has(id)));
          const shellTerminals = new Map([...current.shellTerminals].filter(([, terminal]) => projects.has(terminal.project_id)));
          store.setState({ consoles, projects, sessions, pages, gitStatuses, shellTerminals });
          return { type: "ack" };
        }
        case "add_project": {
          const current = store.getState();
          if (!current.consoles.has(body.console_id)) return fail({ code: "unknown_console", params: { console: body.console_id }, message: `unknown console ${body.console_id}` });
          const given = body.path ?? (body.source === "git" ? current.settings.default_clone_dir : "");
          const parent = normalizedPath(given.trim());
          if (!parent.startsWith("/")) return fail({ code: "path_not_absolute", params: { path: given }, message: `${given} is not an absolute path` });
          const remote = body.remote_url?.trim().replace(/\/+$/, "");
          const repository = remote?.split(/[/:]/).at(-1)?.replace(/\.git$/, "");
          if (body.source === "git" && !repository) return fail({ code: "field_required", params: { field: "remote_url" }, message: "remote_url is required" });
          const listedParent = scenario.directories?.[parent];
          if (listedParent && !Array.isArray(listedParent)) return fail(listedParent);
          const directories = body.source === "parent"
            ? (listedParent ?? genericListing(parent).entries).filter((entry) => entry.is_git_repo).map((entry) => normalizedPath(entry.path))
            : [body.source === "git" ? normalizedPath(`${parent}/${repository}`) : parent];
          if (!directories.length) return fail({ code: "no_repositories_found", params: { path: parent }, message: `no repositories found in ${parent}` });
          const projects = new Map(current.projects);
          let added = 0;
          for (const path of directories) {
            if ([...projects.values()].some((project) => project.console_id === body.console_id && project.path === path)) continue;
            const detection = scenario.detections?.[body.source === "git" ? body.remote_url! : path] ?? scenario.detections?.[given];
            const project: Project = {
              id: `project-gallery-${++createdProjectCount}`,
              console_id: body.console_id,
              host_id: "local",
              source: body.source,
              path,
              name: directories.length === 1 && body.name?.trim() || path.split("/").at(-1) || path,
              default_agent: body.default_agent ?? (body.detect_default_agent !== false && detection && typeof detection === "object" && "agent" in detection ? detection.agent : null),
              remote_url: body.source === "git" ? body.remote_url : null,
              trust_consent: false,
              pinned: false,
              tags: normalizeTags(body.tags ?? []),
              description: directories.length === 1 ? trimmedOrNull(body.description) : null,
              claude_account_id: { kind: "inherit" },
              codex_account_id: { kind: "inherit" },
              grok_account_id: { kind: "inherit" },
              worktrees: projectWorktrees(FOLLOWING_WORKTREE_SETTINGS, body.worktrees),
              ...projectFields(body),
            };
            projects.set(project.id, project);
            added++;
          }
          if (!added) return fail({ code: "all_projects_already_added", params: {}, message: "every directory found is already associated with this console" });
          store.setState({ projects });
          return { type: "ack" };
        }
        case "update_project": {
          const current = store.getState().projects.get(body.project);
          if (!current) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          if (body.name !== undefined && !body.name.trim()) return fail({ code: "field_required", params: { field: "name" }, message: "name is required" });
          const project = { ...current, ...projectFields(body), worktrees: projectWorktrees(current.worktrees, body.worktrees),
            ...(body.name !== undefined && body.name.trim() !== current.name.trim() && { name: body.name.trim() }),
            ...(body.description !== undefined && trimmedOrNull(body.description) !== trimmedOrNull(current.description) && { description: trimmedOrNull(body.description) }),
            ...(body.tags !== undefined && { tags: normalizeTags(body.tags) }),
            ...(body.pinned !== undefined && { pinned: body.pinned }),
          };
          store.setState((state) => ({ projects: new Map(state.projects).set(project.id, project) }));
          return { type: "ack" };
        }
        case "rename_session": {
          const current = store.getState().sessions.get(body.session);
          if (!current) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          const title = body.title.trim();
          if (!title) return fail({ code: "field_required", params: { field: "title" }, message: "title is required" });
          store.setState((state) => ({ sessions: new Map(state.sessions).set(current.id, { ...current, title }) }));
          return { type: "ack" };
        }
        case "update_console": {
          // Broadcast before the answer, as the daemon does, so a dialog reopened on the console
          // reads what was saved.
          const current = store.getState().consoles.get(body.console);
          if (current) store.setState((state) => ({ consoles: new Map(state.consoles).set(current.id, { ...current, ...consoleFields(body) }) }));
          return { type: "ack" };
        }
        case "read_session_history": {
          if (!store.getState().sessions.has(body.session)) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          const history = scenario.sessionHistory?.[body.session] ?? [];
          if (history === "pending") return new Promise<Event>(() => {});
          if (history === "unavailable") return { type: "session_history", session: body.session, messages: [], unavailable: true };
          if (!Array.isArray(history)) return fail(history);
          return { type: "session_history", session: body.session, messages: history };
        }
        case "inspect_session_records":
          return { type: "session_records_inspected", presence: recordsPresence(body.sessions) };
        case "inspect_session_branches": {
          if (scenario.sessionBranches === "pending") return new Promise<Event>(() => {});
          if (scenario.sessionBranches && !Array.isArray(scenario.sessionBranches)) return fail(scenario.sessionBranches);
          const sessions = store.getState().sessions;
          const asked = new Set(body.sessions.filter((id) => sessions.has(id)));
          return { type: "session_branches_inspected", items: (scenario.sessionBranches ?? []).filter((item) => item.session !== null && asked.has(item.session)) };
        }
        case "inspect_project_removal": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          const found = scenario.projectRemovalBranches?.[body.project] ?? [];
          if ("code" in found) return fail(found);
          return { type: "project_removal_inspected", items: found };
        }
        case "delete_session": {
          const all = [...store.getState().sessions.values()];
          const session = store.getState().sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: "Unknown session" });
          refuseLocked([session]);
          // As `delete_session_if_unused` of `store.rs`, repeated here and not taken from the sidebar's
          // `isUnusedSession`: no conversation, nothing bound to it, and either no owner or opened by
          // the user (one an agent started was dispatched by its owner, which expects it to exist,
          // so it is archived instead).
          const unused = !session.has_conversation && !all.some((s) => s.bound_to === session.id)
            && (!session.bound_to || session.origin === "user");
          if (session.status === "starting" || (body.only_if_unused && !unused)
            || (session.status !== "archived" && !unused)) {
            return fail({ code: "session_not_deletable", params: { session: session.id }, message: "Session is no longer unused or is still starting" });
          }
          const doomed = new Set([session.id]);
          const visit = (owner: string) => {
            for (const child of all.filter((s) => s.bound_to === owner && s.status === "archived")) {
              if (doomed.has(child.id)) continue;
              doomed.add(child.id);
              visit(child.id);
            }
          };
          if (session.status === "archived") visit(session.id);
          refuseLocked(all.filter((record) => doomed.has(record.id)));
          store.setState((state) => ({
            sessions: new Map([...state.sessions].filter(([id]) => !doomed.has(id))),
            pages: new Map([...state.pages].filter(([id]) => !doomed.has(id))),
          }));
          return { type: "ack" };
        }
        case "delete_archived_sessions": {
          // What the sessions' deletion comes to, written to the store directly (the reducer that
          // answers `session_deleted` by dropping the session and its pages is not reachable from
          // here) by the time the request is answered, as the daemon's deletes are, so the archive
          // view a scenario has open empties as it would.
          const all = Array.from(store.getState().sessions.values());
          const doomed = new Set<string>();
          // As `delete_archived_group` of `coordinator.rs`: an archived session goes, and with it
          // the archived sessions bound to it, each with its own; one that is not archived stays
          // and keeps what is bound to it. The daemon's own rule is repeated here and not taken
          // from the sidebar's `boundArchivedSessions`, so that a change to what the sidebar lists
          // does not change what a deletion removes.
          const deleteGroup = (session: Session) => {
            if (doomed.has(session.id) || session.status !== "archived") return;
            doomed.add(session.id);
            all.filter((s) => s.bound_to === session.id).forEach(deleteGroup);
          };
          // The scope is the project's, the console session's, or with neither the console's
          // console sessions.
          all
            .filter(
              (s) =>
                s.console_id === body.console &&
                (body.project !== undefined
                  ? s.project_id === body.project
                  : body.console_session !== undefined
                    ? s.bound_to === body.console_session
                    : s.role === "console"),
            )
            .forEach(deleteGroup);
          refuseLocked(all.filter((record) => doomed.has(record.id)));
          store.setState((state) => ({
            sessions: new Map(Array.from(state.sessions).filter(([id]) => !doomed.has(id))),
            pages: new Map(Array.from(state.pages).filter(([id]) => !doomed.has(id))),
          }));
          return { type: "ack" };
        }
        case "create_shell_terminal": {
          const project = store.getState().projects.get(body.project);
          if (!project) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          if (scenario.shellTerminalStart === "pending") return new Promise<Event>(() => {});
          // The project's folder is the first worktree of the repository (as `get_project_source` says),
          // and its place in another is that worktree's root: a fixture project's scope is the whole worktree.
          const target = body.worktree === undefined ? undefined : git?.worktrees.find((worktree) => worktree.id === body.worktree);
          if (body.worktree !== undefined && !target) return fail({ code: "worktree_unavailable", params: { worktree: body.worktree }, message: `worktree ${body.worktree} is gone` });
          if (target && !target.scope_present) return fail({ code: "source_unavailable", params: { path: target.root, detail: "No such file or directory (os error 2)" }, message: `The directory ${target.root} is not available: No such file or directory (os error 2)` });
          const shellTerminal: ShellTerminal = {
            id: `shell-gallery-${++createdShellTerminalCount}`,
            project_id: project.id,
            worktree: body.worktree ?? null,
            cwd: target?.root ?? project.path,
            title: "zsh",
            renamed: false,
            state: "running",
            busy: false,
            created_at: Date.now(),
          };
          store.setState((state) => ({ shellTerminals: new Map(state.shellTerminals).set(shellTerminal.id, shellTerminal) }));
          return { type: "shell_terminal_created", shell_terminal: shellTerminal };
        }
        case "rename_shell_terminal": {
          const terminal = store.getState().shellTerminals.get(body.shell_terminal);
          if (!terminal) return fail({ code: "unknown_shell_terminal", params: { shell_terminal: body.shell_terminal }, message: `unknown shell terminal ${body.shell_terminal}` });
          const title = body.title.trim();
          if (!title) return fail({ code: "field_required", params: { field: "title" }, message: "title is required" });
          store.setState((state) => ({ shellTerminals: new Map(state.shellTerminals).set(terminal.id, { ...terminal, title, renamed: true }) }));
          return { type: "ack" };
        }
        case "close_shell_terminal": {
          if (!store.getState().shellTerminals.has(body.shell_terminal)) return fail({ code: "unknown_shell_terminal", params: { shell_terminal: body.shell_terminal }, message: `unknown shell terminal ${body.shell_terminal}` });
          store.setState((state) => ({ shellTerminals: new Map([...state.shellTerminals].filter(([id]) => id !== body.shell_terminal)) }));
          return { type: "ack" };
        }
        default:
          return { type: "ack" };
      }
    },
    toastError: (message, session) => {
      const presentation = typeof message === "string" ? message : errorPresentation(message);
      emitToast({ kind: "error", message: errorText(message), session,
        ...(typeof presentation !== "string" && { daemonError: presentation }) });
    },
    toastNotice: (message, content) => emitToast({ kind: "notice", message, content }),
    onToast: (listener) => {
      toastListeners.add(listener);
      // StrictMode subscribes, unsubscribes and subscribes again; by the time this fires only the
      // last subscription is left.
      setTimeout(raiseScenarioToasts, TOAST_DELAY_MS);
      return () => void toastListeners.delete(listener);
    },
    dismissTrustPrompt: (session) =>
      store.setState((state) => ({ trustPrompts: state.trustPrompts.filter((p) => p.session !== session) })),
    reconnect: () => {},
    // A shell terminal follows `shellTerminal` rather than `terminal`, which is the sessions'; one
    // whose shell has exited shows its final screen and closes, as the daemon's socket does.
    terminalUrl: (id) => {
      const shell = store.getState().shellTerminals.get(id);
      if (!shell) return terminalFixtureUrl(scenario.terminal ?? "output", id);
      return terminalFixtureUrl(shell.state === "exited" ? "exited" : scenario.shellTerminal ?? "shell", id);
    },
    terminalReconnectDelay: scenario.terminalRetriesImmediate ? () => 0 : undefined,
  };
}
