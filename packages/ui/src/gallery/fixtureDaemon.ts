import { DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import { currentLanguage } from "../i18n/language";
import type { BrowseEntry, DirEntry, Event } from "../protocol";
import { createStateStore, type Daemon, type ToastRequest } from "../store";
import type { Scenario } from "./scenario";
import { terminalFixtureUrl } from "./fakeTerminal";
import { SAMPLE_FILES, type FixtureFile, type FixtureFiles } from "./fixtures/projectFiles";

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
 * it is, requests are answered from the scenario (`list_dir`, `list_pages`, the browse requests) or accepted and
 * ignored, and the scenario's toasts are raised once the app is listening.
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
          const listed = scenario.directories?.[body.path];
          return { type: "dir_listing", ...(listed ? { path: body.path, entries: listed } : genericListing(body.path)) };
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
        default:
          return { type: "ack" };
      }
    },
    toastError: (message, session) => emitToast({ kind: "error", message, session }),
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
  };
}
