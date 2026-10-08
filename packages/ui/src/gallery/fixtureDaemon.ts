import { daemonMessage } from "../daemonMessage";
import { currentLanguage } from "../i18n/language";
import type { DirEntry, Event } from "../protocol";
import { createStateStore, type Daemon, type ToastRequest } from "../store";
import type { Scenario } from "./scenario";
import { terminalFixtureUrl } from "./fakeTerminal";

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

/**
 * A `Daemon` with no connection behind it, for the gallery: the store holds the scenario's state as
 * it is, requests are answered from the scenario (`list_dir`, `list_pages`) or accepted and
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

  return {
    store,
    request: async (body): Promise<Event> => {
      switch (body.type) {
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
