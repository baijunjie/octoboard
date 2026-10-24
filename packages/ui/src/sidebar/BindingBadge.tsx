import React from "react";

import { InlineReference } from "../components/InlineReference";
import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Session } from "../protocol";
import { sessionColourVars } from "../sessionColours";
import { sessionDisplayTitle } from "../sessionLabel";

/**
 * A bound project session's binding badge: a dot in its owner console session's colour, on the
 * project session's sidebar row and on cards in the combined project focus view. The other focus
 * views identify the console session through their header or their separate summary chips.
 * `owner` is that console session — its `colour` is what is drawn, and its `title` is what the
 * tooltip names (through `sidebar.session.boundTo`, the same message the row's own accessible name
 * folds the fact into), since the badge carries no information its tooltip does not: colour alone
 * never distinguishes two owners for a user who cannot tell the colours apart. A console session
 * normally has a `colour` (assigned on creation, changed only by the user), but a `Session` carries
 * `colour?: ConsoleSessionColour | null` (`protocol.ts`), so one built without it is reachable from
 * a fixture or a future caller; drawn with none, nothing renders.
 *
 * It has no touch popover (`touchPopover={false}`): wherever it is not decorative it sits in a session's
 * row, whose press (selecting the session) a tap on the dot would share with the popover, so a touch
 * has no route to the owner's name beyond the row's own label.
 *
 * The tooltip is an `IndicatorTooltip`: the badge is a non-interactive dot, so it shows on pointer
 * hover only and adds no tab stop to the row; a screen-reader user has the row's own `aria-label`
 * ("Bound to …"), and there is today no keyboard route to the owner's name for a sighted user who
 * cannot tell the colours apart.
 *
 * `decorative` drops the tooltip where the owner's name is already visible text beside the dot: a
 * console session's own row and its own card in its focus mode, which show its colour and its
 * full name side by side, an owner option in the new-session dialog's select, that dialog's
 * fixed-owner line, and the owner chips of a project's focus mode. The tooltip would only repeat
 * that label.
 */
export function BindingBadge({ owner, decorative }: { owner: Session; decorative?: boolean }): React.ReactElement | null {
  const t = useT();
  const vars = owner.colour ? sessionColourVars(owner.colour) : undefined;
  if (!vars) return null;
  const dot = (
    <span
      aria-hidden="true"
      className="session-colour-dot inline-block size-2.5 shrink-0 rounded-full"
      style={vars}
    />
  );
  if (decorative) return dot;
  const label = t("sidebar.session.boundTo", { name: isolate(sessionDisplayTitle(t, owner)) });
  return (
    <IndicatorTooltip tooltip={<Message id="sidebar.session.boundTo" params={{ name: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, owner)}</InlineReference> }} />} label={label} className="inline-flex" touchPopover={false}>
      {dot}
    </IndicatorTooltip>
  );
}
