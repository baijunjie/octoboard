import { Toast, toast, type ToastContentValue } from "@heroui/react";
import React, { useCallback, useEffect, useSyncExternalStore } from "react";
import type { QueuedToast } from "react-aria-components";

import { CodeToneContext } from "./codeTone";
import { InlineReference } from "./InlineReference";
import { DaemonErrorMessage, DaemonErrorTitle } from "./DaemonErrorMessage";
import { errorAlertParts } from "../errorAlertParts";
import { errorAlerts } from "../errorAlerts";
import { hasCodeParams, hasShellTimeoutDetails } from "../errorPresentation";
import { t } from "../i18n/language";
import { Message, useT } from "../i18n/react";
import { sessionLocationContent, sessionWithLocationContent } from "../sessionCopy";
import { useDaemon, type ToastRequest } from "../store";
import { TOAST_BREAKPOINT } from "./toastGeometry";
import { TitledControl } from "./TitledControl";
import { useLatest } from "./useLatest";

/** How long a notice stays before it dismisses itself. */
const NOTICE_DURATION_MS = 5000;

/** Matches a window at least as wide as HeroUI's toast breakpoint (its `sm:`); the narrow window is
 * the one it does not match. */
const WIDE_QUERY = `(min-width: ${TOAST_BREAKPOINT}px)`;

function subscribeNarrow(onChange: () => void): () => void {
  const query = window.matchMedia(WIDE_QUERY);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

const isNarrow = () => !window.matchMedia(WIDE_QUERY).matches;

/** The toasts on screen, by what makes two of them identical (kind, message, session), to the
 * queue's key for each. Module-level because the queue is: the stack unmounts and remounts when
 * the app switches between its connecting screen and the main one, while the toasts stay. */
const shownToasts = new Map<string, string>();

/**
 * Routes errors into the acknowledgement queue and displays notices in HeroUI's timed toast
 * stack. An error alert is titled for what failed, with no name in the title, and names its session in the body; a session
 * notice keeps its location and structured daemon message. Notices keep keyboard focus where it
 * was, remain selectable, and move clear of dialog footers.
 */
export function Toasts({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement {
  const { store, onToast } = useDaemon();

  useEffect(
    () =>
      onToast((request) => {
        const { sessions, consoles, projects } = store.getState();
        const session = request.session ? sessions.get(request.session) : undefined;
        const location = session && sessionLocationContent(t, session, consoles, projects);
        if (request.kind === "error") {
          const { title, body, details } = errorAlertParts(request);
          // Which session the alert is about goes in the body, since the daemon words its message as "this session"
          // without naming it: from the session's record, or, when it is gone, from the name the request carries.
          const goneName = request.daemonTitle?.params.title;
          const sessionLine = session ? sessionWithLocationContent(t, session, consoles, projects) : goneName ? <InlineReference dir="auto" wrap>{goneName}</InlineReference> : undefined;
          errorAlerts.report({
            message: request.message,
            session: request.session,
            error: request.error ?? request.daemonError ?? request.message,
            title,
            content: (
              <>
                {sessionLine && <div dir="auto" className="mb-2"><Message id="error.alertSession" params={{ session: sessionLine }} /></div>}
                {body}
              </>
            ),
            details,
          });
        } else showToast(request, location);
      }),
    [store, onToast],
  );

  // `keepFocus` has to stay the same function, or HeroUI re-runs it and re-attaches its listeners.
  const focusTerminalRef = useLatest(focusTerminal);

  // A toast is a tab stop, so a mouse press on it would focus it and pull focus off whatever had
  // it. The press is cancelled on everything but the text, which has to stay selectable; a press on
  // the text focuses the toast, and `handBack` returns focus unless the press left a selection to
  // copy. The listeners live and die with the node, which HeroUI never hands back to a cleanup.
  const keepFocus = useCallback((node: HTMLDivElement | null) => {
    if (!node) return;
    const region = node;
    region.dataset.region = "toast";
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
  }, [focusTerminalRef]);

  const narrow = useSyncExternalStore(subscribeNarrow, isNarrow);

  return (
    <Toast.Provider ref={keepFocus} hotkey={[]} placement={narrow ? "top" : "bottom end"}>
      {renderToast}
    </Toast.Provider>
  );
}

function showToast({ kind, message, session, daemonError, daemonTitle, content }: ToastRequest, location: React.ReactNode): void {
  const signature = JSON.stringify([kind, message, session]);
  const body = content ?? message;
  const shell = daemonError && hasShellTimeoutDetails(daemonError) ? daemonError : undefined;
  // A message with chips is worded from the catalog here, preserving each marked parameter.
  const coded = daemonError && hasCodeParams(daemonError) ? daemonError : undefined;
  const structured = shell ?? coded;
  // A notice about a session that no longer exists (a launch that failed) has no location, and its
  // worded message is the only place that names the session, so it stays the title above the
  // details instead of giving way to the details' own.
  const named = !!daemonTitle || (!!shell && !location && kind === "notice");
  // With a session's location as the title the message is the description; without one the title
  // is the message itself; a structured launch or shell timeout lays its details beneath it.
  const title = location ?? (daemonTitle ? <DaemonErrorMessage error={daemonTitle} /> : named ? body : shell ? <DaemonErrorTitle /> : coded ? <DaemonErrorMessage error={coded} /> : body);
  const options = {
    variant: kind === "error" ? ("danger" as const) : ("accent" as const),
    description: location
      ? structured ? <DaemonErrorMessage error={structured} showTitle /> : body
      : (shell || named) && structured ? <DaemonErrorMessage error={structured} showTitle={false} /> : undefined,
    timeout: NOTICE_DURATION_MS,
  };

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
        {/* The title is the toast's own tone, so a literal in it takes that tone; the description stays neutral. */}
        {!!title && (
          <CodeToneContext.Provider value={variant === "danger" ? "danger" : "accent"}>
            <Toast.Title>{title}</Toast.Title>
          </CodeToneContext.Provider>
        )}
        {!!description && <Toast.Description elementType="div">{description}</Toast.Description>}
      </Toast.Content>
      <ToastClose />
    </Toast>
  );
}

function ToastClose(): React.ReactElement {
  const t = useT();
  return (
    <TitledControl title={t("common.close")}>
      <Toast.CloseButton aria-label={t("common.close")} preventFocusOnPress />
    </TitledControl>
  );
}
