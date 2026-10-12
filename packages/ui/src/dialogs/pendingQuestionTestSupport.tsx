// Shared by the tests of the two questions `PendingQuestionDialog` serves — the trust prompts and
// the requests for a console session — which answer the same dialog and so change together.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";

import type { Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { PendingQuestionDialog } from "./PendingQuestionDialog";

export interface MountedDialog {
  store: Daemon["store"];
  /** The bodies of the requests sent to the daemon, in order. */
  sent: () => unknown[];
  /** The arguments of each `toastError` call. */
  toasts: () => unknown[][];
  /** The console sessions the dialog handed on as started. */
  started: () => unknown[];
  /** Whether a dialog is on screen. */
  onScreen: () => boolean;
}

/**
 * Renders `PendingQuestionDialog` over a store holding `state` and a daemon whose every request
 * answers with `reply`. The tree is taken down when the test finishes rather than at the end of
 * the caller's own steps, so a failure in the middle of them still leaves nothing mounted.
 */
export function mountPendingQuestionDialog(state: Partial<State>, reply: () => Promise<unknown>): MountedDialog {
  const started: unknown[] = [];
  const daemon: Daemon = {
    store: createStateStore(state),
    request: vi.fn(reply) as never,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    // What the real one does, so a prompt answered from the dialog leaves the queue and the next
    // one comes up, as it would in the application.
    dismissTrustPrompt: (session) =>
      act(() => {
        daemon.store.setState((s) => ({ trustPrompts: s.trustPrompts.filter((p) => p.session !== session) }));
      }),
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <PendingQuestionDialog onConsoleSessionStarted={(session: Session) => started.push(session)} />
      </DaemonProvider>,
    ),
  );
  return {
    store: daemon.store,
    sent: () => vi.mocked(daemon.request).mock.calls.map(([body]) => body),
    toasts: () => vi.mocked(daemon.toastError).mock.calls,
    started: () => started,
    onScreen: () => document.body.querySelector("[role=alertdialog]") !== null,
  };
}

/** Presses the dialog's button with this label, if it is there. */
export async function pressButton(label: string): Promise<void> {
  const button = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === label);
  await act(async () => button?.click());
}
