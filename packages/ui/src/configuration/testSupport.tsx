// Shared by the tests of the export and import dialogs, which mount the same way: over a daemon
// whose replies the test gives, and a platform whose configuration-file capability is a stub; and
// a reader of the import's lists of settings.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";

import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { errorAlerts } from "../errorAlerts";
import { trackMountRefocus } from "../dialogs/dialogTestSupport";
import type { ConfigurationFileCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { Event, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";

export interface Mounted {
  /** The bodies of the requests sent to the daemon, in order. */
  sent: () => RequestBody[];
  /** The messages given to `toastNotice`. */
  notices: () => string[];
  closed: () => number;
  /** Awaited before a step whose focus the test asserts (see `trackMountRefocus`). */
  pastMountRefocus: () => Promise<void>;
}

/** Renders `dialog(onClose)` over a daemon answering each request with `reply`, and a platform
 * whose configuration-file capability is `configurationFile`. Taken down when the test finishes. */
export function mountDialog(
  dialog: (onClose: () => void) => React.ReactElement,
  reply: (body: RequestBody) => Promise<Event>,
  configurationFile: ConfigurationFileCapability,
  state: Partial<State> = {},
): Mounted {
  const closed = vi.fn();
  const daemon: Daemon = {
    store: createStateStore(state),
    request: vi.fn(reply),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  act(() =>
    root.render(
      <PlatformProvider value={{ kind: "browser", configurationFile }}>
        <DaemonProvider value={daemon}>{dialog(closed)}<ErrorAlerts daemon={daemon} /></DaemonProvider>
      </PlatformProvider>,
    ),
  );
  return {
    sent: () => vi.mocked(daemon.request).mock.calls.map(([body]) => body),
    notices: () => vi.mocked(daemon.toastNotice).mock.calls.map(([message]) => message),
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

/** The lines of a settings list, by setting name: the badge, and the muted note under it where
 * there is one. */
export function settingLinesOf(list: Element): Record<string, { badge: string; note?: string }> {
  return Object.fromEntries(
    Array.from(list.querySelectorAll(":scope > li")).map((li) => {
      const [head, note] = Array.from(li.children);
      return [head.firstElementChild!.textContent, { badge: head.lastElementChild!.textContent!, ...(note && { note: note.textContent! }) }];
    }),
  );
}

/** Selects the tab with this label, whether or not it is marked as needing attention. */
export async function openTab(label: string): Promise<void> {
  const tab = Array.from(document.body.querySelectorAll<HTMLElement>("[role=tab]")).find((t) => t.textContent === label);
  if (!tab) throw new Error(`no tab labelled ${label}`);
  await act(async () => tab.click());
}
