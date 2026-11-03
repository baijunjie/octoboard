import { Lock, LockOpen, Pin, PinOff } from "lucide-react";
import React, { type RefObject } from "react";

import { RowMarkButton } from "../components/rows/RowMarkButton";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Session } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { ShortcutTooltip } from "./sessionShortcuts";
import type { SidebarHandlers } from "./types";

/** The pin mark of a pinned project or session, which is also the way to unpin it: a pin glyph that
 * becomes the unpin one while the button is hovered or holds keyboard focus (see `RowMarkButton`).
 * `name` is what it unpins, for the accessible name, as the tooltip is the bare verb.
 * `returnFocusTo` is the enclosing row, or an element holding the first button that stays (a
 * header, which is no tab stop itself). */
export function PinButton({
  name,
  onUnpin,
  shortcut = false,
  returnFocusTo,
}: {
  name: string;
  onUnpin: () => void;
  shortcut?: boolean;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  return (
    <RowMarkButton
      icon={Pin}
      hoverIcon={PinOff}
      tooltip={shortcut ? <ShortcutTooltip action="pin">{t("sidebar.unpin")}</ShortcutTooltip> : t("sidebar.unpin")}
      label={t("sidebar.unpinItem", { name: isolate(name) })}
      tone="text-app-muted"
      onPress={onUnpin}
      returnFocusTo={returnFocusTo}
    />
  );
}

/** The lock mark of a locked session, which is also the way to unlock it (see `RowMarkButton`). */
export function LockButton({ name, onUnlock, returnFocusTo }: {
  name: string;
  onUnlock: () => void;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  return (
    <RowMarkButton
      icon={Lock}
      hoverIcon={LockOpen}
      tooltip={<ShortcutTooltip action="lock">{t("sidebar.session.unlock")}</ShortcutTooltip>}
      label={t("sidebar.session.unlockItem", { name: isolate(name) })}
      tone="text-warning-soft-foreground"
      onPress={onUnlock}
      returnFocusTo={returnFocusTo}
    />
  );
}

/** Whether `SessionMarkButtons` draws anything for `session` (with its pin mark left out for `hidePin`), so that a row
 * or card opens its fixed controls only when it does. */
export function hasSessionMarks(session: Session, hidePin = false): boolean {
  return session.locked || (session.pinned && !hidePin);
}

/** A session's lock mark and, unless `hidePin` is set, its pin mark, each shown only while the session
 * has it, for a row's or card's fixed controls. Draws nothing for a session with neither. */
export function SessionMarkButtons({
  handlers,
  session,
  hidePin,
  returnFocusTo,
}: {
  handlers: Pick<SidebarHandlers, "onSetLocked" | "onSetPinned">;
  session: Session;
  hidePin?: boolean;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const title = sessionDisplayTitle(t, session);
  return (
    <>
      {session.locked && <LockButton name={title} onUnlock={() => handlers.onSetLocked(session, false)} returnFocusTo={returnFocusTo} />}
      {!hidePin && session.pinned && <PinButton shortcut name={title} onUnpin={() => handlers.onSetPinned({ session }, false)} returnFocusTo={returnFocusTo} />}
    </>
  );
}
