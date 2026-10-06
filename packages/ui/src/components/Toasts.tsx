import { Alert, CloseButton } from "@heroui/react";
import React from "react";
import { createPortal } from "react-dom";

import { sessionLocation } from "../sessionLabel";
import { useDaemon, useDaemonStore } from "../store";

/**
 * Surfaces every undismissed toast in one stack, in arrival order, floating over the top right so
 * it does not move the layout. An error is the danger colour; a notice, which is something the user
 * has to know rather than something that went wrong, is the accent colour.
 *
 * A toast that carries a session is prefixed with where that session is: the daemon's own messages
 * say "this session" without naming it, since it has no notion of what the client calls that
 * session, so this prefix is the only thing that tells the user which one is meant. Every notice
 * carries one; an error carries one when its call site knows which session the daemon was talking
 * about.
 *
 * The stack is marked `data-react-aria-top-layer`, which keeps it out of the set react-aria makes
 * inert while a dialog or menu is open and out of its outside-press detection; without that a toast
 * would be unclickable then, its dismiss press falling through to the backdrop and closing the
 * dialog. It is portalled to `<body>` so its toast-layer z-index is not capped by a stacking context
 * somewhere in the app's tree, and it stays above the modal backdrop, which is portalled there too.
 *
 * Dismissing keeps keyboard focus where it is: the button unmounts with its toast, and a press that
 * focused it would drop focus to `<body>`, away from the terminal.
 *
 * The stack's top offset clears whatever top chrome the current screen actually has, so a toast
 * only ever covers passive page content and never a control: `--connection-banner-height`, which
 * `ConnectionBanner` publishes (0 when it is not mounted), plus the pane header's
 * `--pane-header-height` (via `style.css`'s combined `--top-chrome-height`) when
 * `belowPaneHeader` says this screen has one.
 */
export function Toasts({ belowPaneHeader = false }: { belowPaneHeader?: boolean } = {}): React.ReactElement {
  const { dismissToast } = useDaemon();
  const toasts = useDaemonStore((s) => s.toasts);
  const sessions = useDaemonStore((s) => s.sessions);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const top = belowPaneHeader
    ? "calc(0.75rem + var(--top-chrome-height))"
    : "calc(0.75rem + var(--connection-banner-height, 0px))";
  return createPortal(
    <div
      data-react-aria-top-layer
      style={{ top }}
      className="fixed right-3 z-(--z-index-toast) flex w-full max-w-sm flex-col gap-2"
    >
      {toasts.map((toast) => {
        const session = toast.session ? sessions.get(toast.session) : undefined;
        return (
          <Alert key={toast.id} status={toast.kind === "error" ? "danger" : "accent"} className="shadow-lg">
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Description className="break-words">
                {session && <strong>{sessionLocation(session, consoles, projects)}: </strong>}
                {toast.message}
              </Alert.Description>
            </Alert.Content>
            <CloseButton aria-label="Dismiss" preventFocusOnPress onPress={() => dismissToast(toast.id)} />
          </Alert>
        );
      })}
    </div>,
    document.body,
  );
}
