// Shared by the tests of the questions `PendingQuestionDialog` serves — the trust prompts and the
// session requests — which answer the same dialog and so change together.
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { PendingQuestionDialog } from "./PendingQuestionDialog";

export interface MountedDialog {
  store: Daemon["store"];
  /** The bodies of the requests sent to the daemon, in order. */
  sent: () => unknown[];
  /** The arguments of each `toastError` call. */
  toasts: () => unknown[][];
  /** Whether a dialog is on screen. */
  onScreen: () => boolean;
}

/**
 * Renders `PendingQuestionDialog` over a store holding `state` and a daemon whose every request
 * answers with `reply`, after `under` (a dialog the question comes up over) and before `after` (what the window mounts
 * after the application, as the error alerts are), in `StrictMode` when `strict` is set. The
 * tree is taken down when the test finishes rather than at the end of the caller's own steps, so a failure in the
 * middle of them still leaves nothing mounted.
 */
export function mountPendingQuestionDialog(
  state: Partial<State>,
  reply: () => Promise<unknown>,
  { under, after, strict = false }: { under?: React.ReactNode; after?: React.ReactNode; strict?: boolean } = {},
): MountedDialog {
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
  const tree = (
    <DaemonProvider value={daemon}>
      {under}
      <PendingQuestionDialog />
      {after}
    </DaemonProvider>
  );
  act(() => root.render(strict ? <StrictMode>{tree}</StrictMode> : tree));
  return {
    store: daemon.store,
    sent: () => vi.mocked(daemon.request).mock.calls.map(([body]) => body),
    toasts: () => vi.mocked(daemon.toastError).mock.calls,
    onScreen: () => document.body.querySelector("[role=alertdialog]") !== null,
  };
}

/** The dialog's button with this label, if it is there. */
export function findButton(label: string): HTMLButtonElement | undefined {
  return Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === label);
}

/** Presses the dialog's button with this label, if it is there. */
export async function pressButton(label: string): Promise<void> {
  const button = findButton(label);
  await act(async () => button?.click());
}
