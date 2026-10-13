import { DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import { currentLanguage } from "../i18n/language";
import type { BrowseEntry, DirEntry, Event, Session } from "../protocol";
import { createStateStore, type Daemon, type ToastRequest } from "../store";
import type { Scenario } from "./scenario";
import { terminalFixtureUrl } from "./fakeTerminal";
import { createConfigurationReplies } from "./fixtureConfiguration";
import { createGitReplies } from "./fixtureGit";
import { SAMPLE_MODELS } from "./fixtures/agentModels";
import { SAMPLE_FILES, type FixtureError, type FixtureFile, type FixtureFiles } from "./fixtures/projectFiles";
import { SAMPLE_GIT } from "./fixtures/projectGit";

/** How long after the app subscribes to toasts the scenario's own are raised, for the stack to be
 * on screen to receive them. */
const TOAST_DELAY_MS = 300;

const HOME = "/Users/dev";

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
 * accepted and ignored (a new console excepted, which is added, and the deletion of a scope's
 * archived sessions, which removes them), and the scenario's toasts are raised once the app is
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

  const raiseScenarioToasts = () => {
    if (toastsRaised) return;
    toastsRaised = true;
    for (const { code, params, message, ...toast } of scenario.toasts ?? []) {
      emitToast({
        ...toast,
        message: code ? daemonMessage(currentLanguage(), code, params ?? {}, message, store.getState()) : message,
      });
    }
  };

  const files = scenario.files ?? SAMPLE_FILES;
  const git = scenario.git === undefined ? SAMPLE_GIT : scenario.git;
  const fail = ({ code, params, message }: FixtureError): never => {
    throw new DaemonRequestError(daemonMessage(currentLanguage(), code, params, message, store.getState()), code, params);
  };
  const gitReply = createGitReplies(git, fail);
  const configurationReply = createConfigurationReplies(scenario.configuration, store.getState, fail);
  /** A browse request's path, or the daemon error it is answered with, worded as the store words one. */
  const browsed = (path: string): FixtureFile => {
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
      const fromGit = gitReply(body);
      if (fromGit !== undefined) return fromGit;
      const fromConfiguration = configurationReply(body);
      if (fromConfiguration !== undefined) return fromConfiguration;
      switch (body.type) {
        case "list_project_dir": {
          const dir = browsed(body.path);
          const complete = "dir" in dir ? dir.complete !== false : true;
          return { type: "project_dir", project: body.project, worktree: null, path: body.path, root_id: "fixture", entries: entriesUnder(files, body.path), complete };
        }
        case "read_project_file": {
          const file = browsed(body.path);
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
          const { name, default_agent, icon } = body;
          const id = `console-${Date.now()}`;
          store.setState((state) => ({
            consoles: new Map(state.consoles).set(id, {
              id,
              name,
              workdir: `${HOME}/.octoboard/consoles/${id}`,
              default_agent,
              icon,
              created_at: Date.now(),
            }),
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
    toastError: (message, session) => emitToast({ kind: "error", message, session }),
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
