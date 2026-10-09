import React from "react";

import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { useT } from "../i18n/react";
import type { Session } from "../protocol";

/**
 * A bound project session's binding badge: a dot in its owner console session's colour, on the
 * project session's sidebar row. A focus mode's cards carry none: one lists only sessions not
 * bound to a console session, the other only sessions bound to the console session it is for.
 * `owner` is that console session — its `colour` is what is drawn, and its `title` is what the
 * tooltip names (through `sidebar.session.boundTo`, the same message the row's own accessible name
 * folds the fact into), since the badge carries no information its tooltip does not: colour alone
 * never distinguishes two owners for a user who cannot tell the colours apart. A console session
 * normally has a `colour` assigned on creation and keeps it, but a `Session` carries
 * `colour?: ConsoleSessionColour | null` (`protocol.ts`), so one built without it is reachable from
 * a fixture or a future caller; drawn with none, nothing renders, rather than a `background-color`
 * of `var(--console-session-undefined)` that WCAG would have no way to flag as missing.
 *
 * The tooltip is an `IndicatorTooltip`: the badge is a non-interactive dot, so it shows on pointer
 * hover only and adds no tab stop to the row; a screen-reader user has the row's own `aria-label`
 * ("Bound to …"), and there is today no keyboard route to the owner's name for a sighted user who
 * cannot tell the colours apart.
 *
 * `decorative` drops the tooltip where the owner's name is already visible text beside the dot: a
 * console session's own row and its focus mode's header, which show its colour and its full name
 * side by side, an owner option in the new-session dialog's select, that dialog's fixed-owner
 * line, and the owner chips of a project's focus mode. The tooltip would only repeat that label.
 */
export function BindingBadge({ owner, decorative }: { owner: Session; decorative?: boolean }): React.ReactElement | null {
  const t = useT();
  if (!owner.colour) return null;
  const dot = (
    <span
      aria-hidden="true"
      className="inline-block size-2.5 shrink-0 rounded-full"
      style={{ backgroundColor: `var(--console-session-${owner.colour})` }}
    />
  );
  if (decorative) return dot;
  const label = t("sidebar.session.boundTo", { name: owner.title });
  return (
    <IndicatorTooltip tooltip={label} label={label} className="inline-flex">
      {dot}
    </IndicatorTooltip>
  );
}
