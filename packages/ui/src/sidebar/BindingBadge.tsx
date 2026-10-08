import React from "react";

import { useT } from "../i18n/react";
import type { Session } from "../protocol";

/**
 * A bound project session's binding badge: a dot in its owner console session's colour, wherever a
 * project session is listed (a sidebar row, a focus-mode card). `owner` is that console session —
 * its `colour` is what is drawn, and its `title` is what the tooltip names (through
 * `sidebar.session.boundTo`, the same message the row's own accessible name folds the fact into),
 * since the badge carries no information its tooltip does not: colour alone never distinguishes two
 * owners for a user who cannot tell the colours apart. A console session normally has a `colour`
 * assigned on creation and keeps it, but a `Session` carries `colour?: ConsoleSessionColour | null`
 * (`protocol.ts`), so one built without it is reachable from a fixture or a future caller; drawn
 * with none, nothing renders, rather than a `background-color` of `var(--console-session-undefined)`
 * that WCAG would have no way to flag as missing.
 *
 * A plain `title` attribute rather than the shared `Tooltip`-based `TitledControl`: the badge is a
 * non-interactive dot, not a react-aria-components control, which `TitledControl` requires (see its
 * own comment); `GitBadge`'s error marker takes the same route for the same reason. This is a known,
 * kept gap for a sighted keyboard-only user who cannot tell the dot's colours apart: a `title`
 * attribute shows on pointer hover only, never on keyboard focus, and the badge is not itself a tab
 * stop that could carry one — a screen-reader user has the row's own `aria-label` ("Bound to …")
 * instead, but there is today no keyboard route to the owner's name for a sighted user. Do not add
 * one by turning the badge into a tab stop of its own; see "Rows, names and keyboard focus" in
 * `docs/product/sidebar.md` for why an extra one inside a sidebar row is unwanted.
 *
 * `decorative` drops the tooltip where the owner's name is already visible text beside the dot: a
 * console session's own row, which shows its colour and its full name side by side, and an owner
 * option in the new-session dialog's select. The tooltip would only repeat that label.
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
  return <span title={t("sidebar.session.boundTo", { name: owner.title })}>{dot}</span>;
}
