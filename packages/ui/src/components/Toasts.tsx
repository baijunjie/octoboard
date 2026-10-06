import { Toast, toast, type ToastContentValue } from "@heroui/react";
import React, { useCallback, useEffect, useRef } from "react";
import type { QueuedToast } from "react-aria-components";

import { sessionLocation } from "../sessionLabel";
import { useDaemon, type ToastRequest } from "../store";
import { TitledControl } from "./TitledControl";

/** How long a toast stays before it dismisses itself. An error gets longer: it says something went
 * wrong, where a notice only says something happened. */
const NOTICE_DURATION_MS = 5000;
const ERROR_DURATION_MS = 8000;

/** The toasts on screen, by what makes two of them identical (kind, message, session), to the
 * queue's key for each. Module-level because the queue is: the stack unmounts and remounts when
 * the app switches between its connecting screen and the main one, while the toasts stay. */
const shownToasts = new Map<string, string>();

/**
 * Surfaces the daemon's errors and notices, and what callers hand to `toastError`, in HeroUI's own
 * toast stack (`Toast.Provider` over its queue), floating over the bottom right so it does not move
 * the layout. An error is the danger variant; a notice, which is something the user has to know
 * rather than something that went wrong, is the accent one. The stack dismisses toasts by itself
 * after a few seconds and pauses while the pointer is over it or focus is inside it, which is what
 * WCAG 2.2.1 (timing adjustable) asks for. The stack's own Alt+T hotkey is off (`hotkey={[]}`): it
 * would swallow Option+T typed into the terminal and pull focus into the stack. react-aria still
 * registers the region as a landmark that F6 cycles to while a toast is shown; that has no switch.
 *
 * A toast that carries a session is titled with where that session is and carries the message as
 * its description: the daemon's own messages say "this session" without naming it, since it has no
 * notion of what the client calls that session, so the title is the only thing that tells the user
 * which one is meant. Every notice carries one; an error carries one when its call site knows which
 * session the daemon was talking about.
 *
 * An identical toast arriving while one is still shown replaces it with a fresh one at the front
 * of the stack and a restarted countdown, instead of stacking another or updating the old one in
 * place, which could sit hidden behind newer toasts.
 *
 * Dismissing keeps keyboard focus where it is: the close button does not take focus on a press,
 * and neither does a press on the toast's padding or icon (the region cancels mousedown there),
 * since a focused toast that then unmounts would drop focus to `<body>`, away from the terminal.
 * A press on the title or description is not cancelled, so the text can be selected and copied;
 * it focuses the toast, and focus is handed back as soon as the press ends with nothing selected
 * (a plain click) or right after a `copy` from inside the region, while a selection is left alone
 * so Cmd+C still reaches it. Focus goes back to the element it came from when that is still
 * connected and takes it (under a modal it is the dialog's field, since everything outside the
 * dialog, the terminal included, is inert while the toast region, a top layer, is not), else to
 * `focusTerminal`, and if the toast would still hold it (no terminal to take it), it is blurred so
 * it does not pin the toast open. Focus that leaves the region for nowhere (Escape blurs it, the
 * last focused toast exiting blurs it) is handed back the same way instead of resting on `<body>`,
 * unless it went into the report panel's iframe: that reports no `relatedTarget` either, but the
 * iframe is the active element by the time the check runs a task later. The region portals itself
 * to `<body>`, so its toast-layer z-index is not capped by a stacking context somewhere in the
 * app's tree; react-aria already keeps it out of the set made inert while a dialog or menu is open.
 *
 * The stack always sits at the bottom right, so it never moves with what is open. The top of the
 * window is where the controls are — the top bar, the report panel's pager, the settings dialog's
 * right column and its close button — while the bottom right is terminal or page content, or the
 * settings dialog's empty padding; the smaller dialogs are centred, well clear of it.
 */
export function Toasts({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement {
  const { store, onToast } = useDaemon();

  useEffect(
    () =>
      onToast((request) => {
        const { sessions, consoles, projects } = store.getState();
        const session = request.session ? sessions.get(request.session) : undefined;
        showToast(request, session && sessionLocation(session, consoles, projects));
      }),
    [store, onToast],
  );

  // `keepFocus` has to stay the same function, or HeroUI re-runs it and re-attaches its listeners.
  const focusTerminalRef = useRef(focusTerminal);
  focusTerminalRef.current = focusTerminal;

  // A toast is a tab stop, so a mouse press on it would focus it and pull focus off whatever had
  // it. The press is cancelled on everything but the text, which has to stay selectable; a press on
  // the text focuses the toast, and `handBack` returns focus unless the press left a selection to
  // copy. The listeners live and die with the node, which HeroUI never hands back to a cleanup.
  const keepFocus = useCallback((node: HTMLDivElement | null) => {
    if (!node) return;
    const region = node;
    // Where focus was before it entered the region.
    let origin: HTMLElement | null = null;
    let press: AbortController | undefined;

    const rememberOrigin = (element: Element | null) => {
      if (element instanceof HTMLElement && element !== document.body && !region.contains(element)) {
        origin = element;
      }
    };

    region.addEventListener("mousedown", (event) => {
      const target = event.target as Element;
      if (!target.closest('[data-slot="toast-title"], [data-slot="toast-description"]')) {
        event.preventDefault();
        return;
      }
      // Only a primary press selects text; others (a context menu, which swallows the release)
      // are left to do what they do.
      if (event.button !== 0 || event.ctrlKey) return;
      rememberOrigin(document.activeElement);
      // The press may end outside the toast (a drag), so the release is listened for on the
      // window; a drag of already selected text ends in a cancel instead. The selection settles
      // after the release, hence the timeout.
      press?.abort();
      const listeners = new AbortController();
      press = listeners;
      const end = () => {
        listeners.abort();
        setTimeout(() => {
          if (!window.getSelection()?.toString()) handBack();
        });
      };
      window.addEventListener("pointerup", end, { signal: listeners.signal });
      window.addEventListener("pointercancel", end, { signal: listeners.signal });
    });
    region.addEventListener("focusin", (event) => rememberOrigin(event.relatedTarget as Element | null));
    // After a copy the selection has done its job and focus goes back, which may collapse it.
    region.addEventListener("copy", () => setTimeout(handBack));
    region.addEventListener("focusout", (event) => {
      const next = event.relatedTarget as Element | null;
      if (next) {
        if (!region.contains(next)) origin = null;
        return;
      }
      // No `relatedTarget` means focus went nowhere (to `<body>`), into another document (the
      // report panel's iframe, whose focus `document.activeElement` shows as the iframe element),
      // or the window itself lost focus. Only the first is answered, once focus has settled; going
      // into the iframe is a move out of the region like any other, so it forgets the origin.
      setTimeout(() => {
        const active = document.activeElement;
        if (active === document.body && document.hasFocus()) handBack();
        else if (active instanceof HTMLIFrameElement) origin = null;
      });
    });

    function handBack(): void {
      const target = origin;
      origin = null;
      if (target?.isConnected) {
        target.focus();
        if (document.activeElement === target) return;
      }
      focusTerminalRef.current();
      const active = document.activeElement;
      if (active instanceof HTMLElement && region.contains(active)) active.blur();
    }
  }, []);

  return (
    <Toast.Provider ref={keepFocus} hotkey={[]} placement="bottom end">
      {renderToast}
    </Toast.Provider>
  );
}

function showToast({ kind, message, session }: ToastRequest, location: string | undefined): void {
  const signature = JSON.stringify([kind, message, session]);
  const options = {
    variant: kind === "error" ? ("danger" as const) : ("accent" as const),
    description: location ? message : undefined,
    timeout: kind === "error" ? ERROR_DURATION_MS : NOTICE_DURATION_MS,
  };
  const title = location ?? message;

  const existing = shownToasts.get(signature);
  if (existing) toast.close(existing);
  // Closing the old copy fires its `onClose` synchronously, which forgets the signature, before
  // the new copy is registered below, so a copy's `onClose` never sees a newer copy's entry.
  const key: string = toast(title, { ...options, onClose: () => shownToasts.delete(signature) });
  shownToasts.set(signature, key);
}

/** HeroUI's default toast, with a close button that does not take focus on a press. */
function renderToast({ toast: queued }: { toast: QueuedToast<ToastContentValue> }): React.ReactElement {
  const { title, description, variant } = queued.content;
  return (
    <Toast toast={queued} variant={variant}>
      <Toast.Indicator variant={variant} />
      <Toast.Content>
        {!!title && <Toast.Title>{title}</Toast.Title>}
        {!!description && <Toast.Description>{description}</Toast.Description>}
      </Toast.Content>
      <TitledControl title="Close" contents>
        <Toast.CloseButton preventFocusOnPress />
      </TitledControl>
    </Toast>
  );
}
