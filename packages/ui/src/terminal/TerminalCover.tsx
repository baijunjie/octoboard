import { Unplug } from "lucide-react";
import React, { useEffect, type RefObject } from "react";

import { EmptyPanel } from "../components/panel/EmptyPanel";
import { LoadingState } from "../components/panel/LoadingState";
import { useT } from "../i18n/react";

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
}): React.ReactElement {
  const t = useT();
  useEffect(() => {
    if (!canReconnect || !rootRef.current?.contains(document.activeElement)) return;
    ref.current?.querySelector("button")?.focus();
  }, [canReconnect]);
  return (
    <div
      ref={ref}
      {...held}
      className="absolute inset-0 flex flex-col items-center justify-center-safe text-sm text-app-muted"
      style={{ backgroundColor: "var(--terminal-background)" }}
    >
      {disconnected ? (
        // The reconnect its button starts moves focus to the terminal, and `held` covers the
        // button vanishing some other way.
        <EmptyPanel icon={Unplug} message={text} action={canReconnect ? { label: t("terminal.reconnect"), onPress: onReconnect } : undefined} />
      ) : (
        <LoadingState size="md" label={text} spinner={spinner} />
      )}
    </div>
  );
}
