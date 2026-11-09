import { Unplug } from "lucide-react";
import React, { useEffect, useId, type RefObject } from "react";

import { EmptyPanel } from "../components/panel/EmptyPanel";
import { LoadingState } from "../components/panel/LoadingState";
import { useT } from "../i18n/react";
import { Spinner, useFocusVisibleProps } from "../primitives";

/**
 * What covers a terminal that has nothing live to show, over the view's `--terminal-background`:
 * the disconnected state with its Reconnect (offered only when `canReconnect`), or a spinner and
 * a sentence, hidden from assistive technology (the caller speaks `text` from a `StatusAnnouncer`;
 * the disconnected one is not hidden, so that its button stays reachable).
 *
 * `held` is the caller's `useFocusHandoff` for the Reconnect button, which goes away on its own.
 * Typing into a terminal that is not connected goes nowhere, so when Reconnect appears while focus
 * is in the view (`rootRef`, on the terminal), it takes focus. Focus elsewhere in the window is
 * left alone: a background failure must not pull it.
 *
 * `pending` is the cover of a terminal whose process cannot be reached: a spinner, a title and a line
 * saying what it means, with no button, since the daemon alone retries. Its element is a region that
 * can take focus (`tabIndex={-1}`) and draws its own ring, which the caller's `usePendingFocus` hands focus
 * to. It is named by what it is, `label` (the session's or terminal's name), not by the title it holds,
 * which VoiceOver would then read twice (`docs/voiceover-cases.md` KNW-9), and described by the title
 * and the line, so focusing it still says it is reconnecting; the copy is on screen to assistive technology too, as
 * the disconnected state's is, and the caller's `StatusAnnouncer` speaks the one sentence only while
 * focus is elsewhere, since the focused region already reads the copy.
 */
export function TerminalCover({
  ref,
  rootRef,
  held,
  text,
  spinner,
  disconnected,
  canReconnect,
  onReconnect,
  pending,
}: {
  /** The caller's handle on the cover, to find its Reconnect button by. */
  ref: RefObject<HTMLDivElement | null>;
  rootRef: RefObject<HTMLElement | null>;
  held: { onFocus: (event: React.FocusEvent) => void; onBlur: (event: React.FocusEvent) => void };
  text: string;
  /** The spinner above `text`, outside the disconnected state. */
  spinner: boolean;
  disconnected: boolean;
  canReconnect: boolean;
  onReconnect: () => void;
  pending?: { label: string; title: string; body: string };
}): React.ReactElement {
  const t = useT();
  const titleId = useId();
  const bodyId = useId();
  const focusVisible = useFocusVisibleProps();
  useEffect(() => {
    if (!canReconnect || !rootRef.current?.contains(document.activeElement)) return;
    ref.current?.querySelector("button")?.focus();
  }, [canReconnect]);
  return (
    <div
      ref={ref}
      {...held}
      {...(pending && {
        ...focusVisible,
        onFocus: (event: React.FocusEvent) => {
          focusVisible.onFocus?.(event);
          held.onFocus(event);
        },
        onBlur: (event: React.FocusEvent) => {
          focusVisible.onBlur?.(event);
          held.onBlur(event);
        },
        role: "region",
        tabIndex: -1,
        "aria-label": pending.label,
        "aria-describedby": `${titleId} ${bodyId}`,
      })}
      className={`absolute inset-0 flex flex-col items-center justify-center-safe text-sm text-app-muted ${pending ? "outline-none ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-inset ui-keyboard-focus:ring-focus" : ""}`}
      style={{ backgroundColor: "var(--terminal-background)" }}
    >
      {pending ? (
        <div className="flex max-w-sm flex-col items-center gap-3 px-6 text-center">
          <div aria-hidden="true">
            <Spinner size="md" color="current" />
          </div>
          <h2 id={titleId} className="text-base font-semibold text-foreground">{pending.title}</h2>
          <p id={bodyId}>{pending.body}</p>
        </div>
      ) : disconnected ? (
        // The reconnect its button starts moves focus to the terminal, and `held` covers the
        // button vanishing some other way.
        <EmptyPanel icon={Unplug} message={text} action={canReconnect ? { label: t("terminal.reconnect"), onPress: onReconnect } : undefined} />
      ) : (
        <LoadingState size="md" label={text} spinner={spinner} />
      )}
    </div>
  );
}
