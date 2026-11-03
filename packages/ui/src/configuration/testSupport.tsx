// Shared by the tests of the export and import dialogs, which mount the same way: over a daemon
// whose replies the test gives, and a platform whose configuration-file capability is a stub; and
// a reader of the import's lists of settings.
import { act } from "react";
import { onTestFinished, vi } from "vitest";

import { ErrorAlerts } from "../shell/ErrorAlerts";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { trackMountRefocus } from "../components/dialog/dialogTestSupport";
import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import type { ConfigurationFileCapability } from "../platform";
import type { DocumentSettings, Event, ClientRequestBody } from "../protocol";
import type { State } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";

/** A document's settings with each switch off and no Claude compatibility choice, except what `carried` sets. */
export function documentSettings(carried: Partial<DocumentSettings> = {}): DocumentSettings {
  return {
    auto_pull_repositories: false,
    auto_delete_unresumable_archives: false,
    auto_select_account: false,
    auto_switch_on_usage_limit: false,
    sidebar_order_by_status: false,
    auto_start_quota_windows: false,
    auto_approve_session_requests: false,
    auto_approve_resume_requests: false,
    auto_approve_account_switches: false,
    auto_approve_handoffs: false,
    ask_questions_as_choices: false,
    git_workflow: DEFAULT_GIT_WORKFLOW,
    claude_compatibility: {},
    agent_tags: {},
    ...carried,
  };
}

export interface Mounted {
  /** The bodies of the requests sent to the daemon, in order. */
  sent: () => ClientRequestBody[];
  /** The messages given to `showNotice`. */
  notices: () => string[];
  closed: () => number;
  /** Awaited before a step whose focus the test asserts (see `trackMountRefocus`). */
  pastMountRefocus: () => Promise<void>;
}

/** Renders `dialog(onClose)` over a daemon answering each request with `reply`, and a platform
 * whose configuration-file capability is `configurationFile`. Taken down when the test finishes. */
export function mountDialog(
  dialog: (onClose: () => void) => React.ReactElement,
  reply: (body: ClientRequestBody) => Promise<Event>,
  configurationFile: ConfigurationFileCapability,
  state: Partial<State> = {},
): Mounted {
  const closed = vi.fn();
  const request = vi.fn(reply);
  const daemon = testDaemon({ state, request, showNotice: vi.fn() });
  // Registered before the mount, so it runs after the unmount.
  onTestFinished(() => {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  mountHost(<>{dialog(closed)}<ErrorAlerts daemon={daemon} /></>, { daemon, platform: { kind: "browser", configurationFile } });
  return {
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
      const note = rest.find((child) => child.tagName === "SPAN");
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

/** Selects the tab with this label, whether or not it is marked as needing attention. */
export async function openTab(label: string): Promise<void> {
  const tab = Array.from(document.body.querySelectorAll<HTMLElement>("[role=tab]")).find((t) => t.textContent === label);
  if (!tab) throw new Error(`no tab labelled ${label}`);
  await act(async () => tab.click());
}
