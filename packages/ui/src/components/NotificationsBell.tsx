import { Badge } from "@heroui/react";
import { Bell } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { ChromeButton } from "./ChromeButton";
import { useFocusHandoff } from "./useFocusHandoff";

/** Where the bell's dot badge sits: centred on the glyph's top right corner, which stays the
 * physical top right under a right-to-left language, since HeroUI's `Badge` has only physical
 * placements. Its `top-right` placement only pushes a badge a quarter of its size past the corner;
 * `translate` stacks with that placement's own `transform`, adding the other quarter. */
const BADGE_PLACEMENT = "pointer-events-none translate-x-1/4 -translate-y-1/4";

/** The dot is cut out of the 16px glyph under it: masked away in a 1.5px ring around the dot, so it
 * reads as separate from the bell and the ring shows whatever is behind (the native material, the
 * browser's chrome colour, a hover tint) rather than a colour of its own. */
const DOT_CUT_OUT = "[mask-image:radial-gradient(circle_at_16px_0,transparent_6.5px,black_7px)]";

/** The invitation to turn notifications on, where the platform wants the ask to come from a user
 * gesture (a browser) and the answer is still undecided; gone once the answer is given, whichever
 * it is. The dot only draws the eye, the label carries the meaning. The Settings dialog's
 * Notifications section is the other place to see and change the answer.
 *
 * Pressed from the keyboard, the bell holds focus until the answer removes it, and focus would then
 * fall to `<body>`; so once the answer is in, focus that was lost is handed on (`useFocusHandoff`).
 * Not before the answer: while the browser's own prompt is up, keys must not reach the agent. */
export function NotificationsBell({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement | null {
  const t = useT();
  const { status, request } = useNotificationPermission();
  const held = useFocusHandoff(focusTerminal);
  if (!request || status !== "undecided") return null;
  return (
    <span className="contents" {...held}>
      <ChromeButton label={t("rail.notifications.enable")} onPress={() => void request()} tooltipPlacement="end">
        <Badge.Anchor>
          <Bell aria-hidden="true" className={`size-4 ${DOT_CUT_OUT}`} />
          <Badge aria-hidden="true" color="accent" variant="primary" size="sm" className={`${BADGE_PLACEMENT} min-h-2.5 min-w-2.5 border-0`} />
        </Badge.Anchor>
      </ChromeButton>
    </span>
  );
}
