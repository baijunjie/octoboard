import { DaemonRequestError } from "../daemon-client";
import { consoleAccount, inheritedAccount } from "../accountChoices";
import { daemonMessage } from "../daemonMessage";
import { errorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { currentLanguage } from "../i18n/language";
import type { BrowseEntry, Console, DirEntry, Event, RequestBody, Session, SessionRecordsPresence } from "../protocol";
import { createStateStore, type Daemon, type ToastRequest } from "../store";
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
  for (const agent of ["claude", "codex", "grok"] as const) {
    const account = body[`${agent}_account_id`];
    if (account !== undefined) fields[`${agent}_account_id`] = account;
    const model = body[`${agent}_console_session_model`];
    if (model !== undefined) fields[`${agent}_console_session_model`] = trimmedOrNull(model);
    const effort = body[`${agent}_console_session_effort`];
    if (effort !== undefined) fields[`${agent}_console_session_effort`] = trimmedOrNull(effort);
  }
  if (body.icon !== undefined) fields.icon = body.icon;
  if (body.console_session_prompt !== undefined) fields.console_session_prompt = trimmedOrNull(body.console_session_prompt);
  return fields;
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

/** The entries directly under `dir` among `files`, as `list_project_dir` sends them. */
function entriesUnder(files: FixtureFiles, dir: string): BrowseEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  return Object.entries(files)
    .filter(([path]) => path !== dir && path.startsWith(prefix) && !path.slice(prefix.length).includes("/"))
    .map(([path, file]): BrowseEntry => ({
      name: path.slice(prefix.length),
      kind: "dir" in file ? "directory" : "file",
      size: "text" in file ? new TextEncoder().encode(file.text).length : "image" in file ? Math.floor((file.image.data.length * 3) / 4) : null,
      version: "dir" in file ? null : "1",
      target: null,
    }))
    .sort((a, b) => (a.name < b.name ? -1 : 1));
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
      emitToast({
        ...toast,
        ...(code && { daemonError: new DaemonRequestError(message, code, params ?? {}) }),
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
        case "resume_session": {
          const current = store.getState();
          const session = current.sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
          if (session.status !== "archived" && session.status !== "interrupted") return fail({ code: "session_already_running", params: { session: body.session }, message: "this session is not interrupted or archived" });
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
          const reply = scenario.sessionAccountSwitches?.[body.session];
          if (reply === "pending") return new Promise<Event>(() => {});
          if (reply) return fail(reply);
          const updated = { ...session, account_id: account?.id, config_dir: account?.config_dir };
          store.setState((state) => ({ sessions: new Map(state.sessions).set(session.id, updated) }));
          return { type: "ack" };
        }
        case "open_project_directory": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
          if (scenario.projectDirectoryOpenError) return fail(scenario.projectDirectoryOpenError);
          return { type: "ack" };
        }
        case "delete_project": {
          if (!store.getState().projects.has(body.project)) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
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
        case "open_session": {
          if (scenario.sessionOpenError) return fail(scenario.sessionOpenError);
          const current = store.getState();
          const owner = current.consoles.get(body.console_id);
          if (!owner) return fail({ code: "unknown_console", params: { console: body.console_id }, message: `unknown console ${body.console_id}` });
          const project = body.project_id ? current.projects.get(body.project_id) : undefined;
          if (body.project_id && !project) return fail({ code: "unknown_project", params: { project: body.project_id }, message: `unknown project ${body.project_id}` });
          const agent = body.agent ?? project?.default_agent ?? owner.default_agent;
          const accountId = body.account === undefined
            ? project ? inheritedAccount(owner, project, agent, current.settings.accounts) : consoleAccount(owner, agent, current.settings.accounts)
            : body.account;
          const account = current.settings.accounts.find((candidate) => candidate.id === accountId && candidate.agent === agent);
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
            started_at: Date.now(),
            ...(project ? { bound_to: body.bound_to } : { ordinal, colour: "teal" }),
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
            const expanded = given === "~" ? HOME : given.startsWith("~/") ? `${HOME}/${given.slice(2)}` : given;
            if (given.trim() === "") cloneDir = `${HOME}/Projects`;
            else {
              if (!expanded.startsWith("/")) return fail({ code: "path_not_absolute", params: { path: given }, message: `\`${given}\` is not an absolute path; give the full path or start it with \`~/\`` });
              const parts: string[] = [];
              for (const part of expanded.split("/")) {
                if (part === "..") parts.pop();
                else if (part !== "" && part !== ".") parts.push(part);
              }
              cloneDir = `/${parts.join("/")}`;
            }
          }
          store.setState({ settings: { ...current.settings, auto_pull_repositories: body.auto_pull_repositories ?? current.settings.auto_pull_repositories, default_clone_dir: cloneDir } });
          return { type: "ack" };
        }
        case "remove_trusted_directory": {
          const expanded = body.path === "~" ? HOME : body.path.startsWith("~/") ? `${HOME}/${body.path.slice(2)}` : body.path;
          const parts: string[] = [];
          for (const part of expanded.split("/")) {
            if (part === "..") parts.pop();
            else if (part !== "" && part !== ".") parts.push(part);
          }
          const path = `${expanded.startsWith("/") ? "/" : ""}${parts.join("/")}`;
          store.setState((current) => ({ trustedDirectories: current.trustedDirectories.filter((given) => given !== path) }));
          return { type: "ack" };
        }
        case "list_project_dir": {
          const files = projectFiles(body.project);
          const dir = browsed(files, body.path);
          const complete = "dir" in dir ? dir.complete !== false : true;
          return { type: "project_dir", project: body.project, worktree: null, path: body.path, root_id: "fixture", entries: entriesUnder(files, body.path), complete };
        }
        case "read_project_file": {
          const file = browsed(projectFiles(body.project), body.path);
          const source = { kind: "live" as const, root_id: "fixture", version: "1" };
          const content =
            "text" in file
              ? { size: new TextEncoder().encode(file.text).length, kind: "text" as const, media_type: null, text: file.text, data: null }
              : "image" in file
                ? { size: Math.floor((file.image.data.length * 3) / 4), kind: "binary" as const, media_type: file.image.mediaType, text: null, data: file.image.data }
                : { size: 0, kind: "binary" as const, media_type: null, text: null, data: "" };
          return { type: "project_file", project: body.project, worktree: null, path: body.path, source, file: content };
        }
        case "list_dir": {
          // `~` is answered with the home directory it resolves to, as the real daemon does, so a
          // scenario names the listing it gives by that path.
          const path = body.path === "~" ? HOME : body.path;
          const listed = scenario.directories?.[path];
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
        case "update_console": {
          // Broadcast before the answer, as the daemon does, so a dialog reopened on the console
          // reads what was saved.
          const current = store.getState().consoles.get(body.console);
          if (current) store.setState((state) => ({ consoles: new Map(state.consoles).set(current.id, { ...current, ...consoleFields(body) }) }));
          return { type: "ack" };
        }
        case "inspect_session_records":
          return { type: "session_records_inspected", presence: recordsPresence(body.sessions) };
        case "delete_session": {
          const all = [...store.getState().sessions.values()];
          const session = store.getState().sessions.get(body.session);
          if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: "Unknown session" });
          const unused = !session.has_conversation && !session.bound_to && !all.some((s) => s.bound_to === session.id);
          if ((body.only_if_unused_without_agent_records && (!unused || recordsPresence([session.id]) !== "absent"))
            || (session.status !== "archived" && !unused)) {
            return fail({ code: "session_not_deletable", params: { session: session.id }, message: "Session is no longer unused or its agent records cannot be proved absent" });
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
          store.setState((state) => ({
            sessions: new Map(Array.from(state.sessions).filter(([id]) => !doomed.has(id))),
            pages: new Map(Array.from(state.pages).filter(([id]) => !doomed.has(id))),
          }));
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
    toastNotice: (message) => emitToast({ kind: "notice", message }),
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
    terminalUrl: (session) => terminalFixtureUrl(scenario.terminal ?? "output", session),
    terminalReconnectDelay: scenario.terminalRetriesImmediate ? () => 0 : undefined,
  };
}
