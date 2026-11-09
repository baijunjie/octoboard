// Shared by the tests of the export and import dialogs, which mount the same way: over a daemon
// whose replies the test gives, and a platform whose configuration-file capability is a stub; and
// a reader of the import's lists of settings.
import { act } from "react";
import { onTestFinished, vi } from "vitest";

import { ErrorAlerts } from "../shell/ErrorAlerts";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { trackMountRefocus } from "../components/dialog/dialogTestSupport";
import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import type { ConfigurationFileCapability, PlatformAdapter } from "../platform";
import { ThemeProvider } from "../preferences/theme";
import type { DocumentSettings, Event, ClientRequestBody } from "../protocol";
import type { Daemon, State } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { BROWSER_HOST } from "../test/hosts";

/** A document's settings with each switch off and no default clone directory, except what `carried` sets. */
export function documentSettings(carried: Partial<DocumentSettings> = {}): DocumentSettings {
  return {
    auto_pull_repositories: false,
    auto_close_idle_shell_terminals: false,
    auto_delete_unresumable_archives: false,
    auto_approve_session_requests: false,
    auto_approve_resume_requests: false,
    auto_approve_account_switches: false,
    auto_approve_handoffs: false,
    ask_questions_as_choices: false,
    git_workflow: DEFAULT_GIT_WORKFLOW,
    ...carried,
  };
}

export interface Mounted {
  /** The daemon's state store, for a test that changes the state under the dialog. */
  store: Daemon["store"];
  /** The bodies of the requests sent to the daemon, in order. */
  sent: () => ClientRequestBody[];
  /** The messages given to `showNotice`. */
  notices: () => string[];
  closed: () => number;
  /** Awaited before a step whose focus the test asserts (see `trackMountRefocus`). */
  pastMountRefocus: () => Promise<void>;
}

/** Renders `dialog(onClose)` over a daemon answering each request with `reply`, and a platform
 * whose configuration-file capability is `configurationFile` and which has what `capabilities` adds; `daemonMembers`
 * replace the daemon's other members. The dialogs read the appearance, so they are under the theme provider. Taken
 * down when the test finishes. */
export function mountDialog(
  dialog: (onClose: () => void) => React.ReactElement,
  reply: (body: ClientRequestBody) => Promise<Event>,
  configurationFile: ConfigurationFileCapability,
  state: Partial<State> = {},
  capabilities: Pick<PlatformAdapter, "backgroundRun" | "statusItem"> = {},
  daemonMembers: Partial<Omit<Daemon, "request">> = {},
): Mounted {
  const closed = vi.fn();
  const request = vi.fn(reply);
  const daemon = testDaemon({ state, request, showNotice: vi.fn(), ...daemonMembers });
  // The theme provider asks the window for the system's colour scheme.
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  // Registered before the mount, so it runs after the unmount.
  onTestFinished(() => {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    vi.unstubAllGlobals();
  });
  mountHost(<ThemeProvider>{dialog(closed)}<ErrorAlerts daemon={daemon} /></ThemeProvider>, { daemon, platform: { kind: "browser", host: BROWSER_HOST, configurationFile, ...capabilities } });
  return {
    store: daemon.store,
    sent: () => request.mock.calls.map(([body]) => body),
    notices: () => vi.mocked(daemon.showNotice).mock.calls.map(([message]) => message),
    closed: () => closed.mock.calls.length,
    pastMountRefocus: trackMountRefocus(),
  };
}

/** Presses the button with this label. */
export async function press(label: string): Promise<void> {
  const button = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === label);
  if (!button) throw new Error(`no button labelled ${label}`);
  await act(async () => button.click());
}

/** The lines of a settings list, by setting name: the badge, the muted note under it where there is one and the
 * values listed under that where there are any. */
export function settingLinesOf(list: Element): Record<string, { badge: string; note?: string; values?: string[] }> {
  return Object.fromEntries(
    Array.from(list.querySelectorAll(":scope > li")).map((li) => {
      const [head, ...rest] = Array.from(li.children);
      const note = rest.find((child) => child.tagName === "DIV");
      const values = rest.find((child) => child.tagName === "UL");
      return [
        head.firstElementChild!.textContent,
        {
          badge: head.lastElementChild!.textContent!,
          ...(note && { note: note.textContent! }),
          ...(values && { values: Array.from(values.children).map((value) => value.textContent!) }),
        },
      ];
    }),
  );
}

/** The card titled `title` in `root`, by its heading. */
export function cardTitled(root: Element, title: string): Element | undefined {
  return Array.from(root.querySelectorAll("section[aria-labelledby]")).find((card) => card.querySelector(":scope > div h3")?.textContent === title);
}

/** The lines of settings in an agent's card, by name (see `settingLinesOf`): the list of lines among the card's lists,
 * which is the one whose items start with a name, where an account's row starts with a block. */
export function agentSettingLinesOf(card: Element): ReturnType<typeof settingLinesOf> {
  const list = Array.from(card.querySelectorAll(":scope > ul, :scope > div > ul")).find((ul) => ul.querySelector(":scope > li > span:first-child"));
  return list ? settingLinesOf(list) : {};
}

/** The titled cards of the settings in `root`, each with the names of its lines in the order they are drawn. */
export function settingGroupsOf(root: Element): { title: string; lines: string[] }[] {
  return Array.from(root.querySelectorAll("section[aria-labelledby]")).map((card) => ({
    title: card.querySelector("h3")!.textContent!,
    // A card collapsed to its title has no rows.
    lines: Object.keys(card.querySelector(":scope > ul") ? settingLinesOf(card.querySelector(":scope > ul")!) : {}),
  }));
}

/** The lines of every settings card in `root`, by setting name. */
export function settingLinesAcrossGroups(root: Element): ReturnType<typeof settingLinesOf> {
  return Object.assign({}, ...Array.from(root.querySelectorAll("section[aria-labelledby] > ul")).map(settingLinesOf));
}

/** Selects the tab with this label, whether or not it is marked as needing attention. */
export async function openTab(label: string): Promise<void> {
  const tab = Array.from(document.body.querySelectorAll<HTMLElement>("[role=tab]")).find((t) => t.textContent === label);
  if (!tab) throw new Error(`no tab labelled ${label}`);
  await act(async () => tab.click());
}
