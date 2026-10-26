import { Alert } from "@heroui/react";
import React from "react";

import { AlertStatusContext } from "./AlertButton";
import { AlertFrame } from "./AlertFrame";
import { DetailsButton } from "./DetailsButton";
import { useAnnouncement } from "./useAnnouncement";

/**
 * Durable failure or warning content shown among other content: one that blocks (a lookup a form makes), one about a
 * whole area laid out top down, or one about the whole view; never a whole subject that failed (`EmptyPanel`) or one
 * at a point (`StatusMark`). It is shown here and nowhere else. The Alert is compact ("Errors shown among other
 * content" in `docs/ui-accessibility.md`): the `title`, required, saying what happened; `children`, the one short
 * description, which says why rather than repeating the title (inline content only, since `Alert.Description` is a
 * `span`); then `remedy`, the one main thing to do about it, also always visible and also inline content, so the
 * visible copy is at most two sentences in the application's own words; then at its end, where HeroUI's own Alert puts
 * its action, the buttons. `actions` is the recovery button (Try again) and `details` everything else, which a Details
 * button opens in a popover: first further explanation in the application's own words (a reason per cause, each
 * cause's own fix, further guidance), then the raw text not written for the reader (the system's own message, a
 * diagnostic block, raw entries); with no `details` there is no Details button. The buttons wrap beneath the text when
 * the Alert is too narrow to hold both.
 * `status` is `warning` for a warning, or a failure that leaves the subject usable.
 *
 * `announce` makes the title and the description (the first sentence, not the `remedy`), and nothing else, an alert,
 * so the failure is spoken once as it appears; it is read as the Alert mounts. A caller passes it for a failure that
 * arrives after its view has painted (`useHasPainted`), and not for one that is there as the view opens. The buttons
 * and the details are never part of the announcement. `announceKey` is the failure's attempt: a new one remounts the
 * announcing part, so a failure that repeats with the same words is spoken again, while the actions around it stay
 * mounted and keep focus. `announceDelayMs` holds the announcement back for a failure that mounts together with a
 * focus move (a dialog opening onto its field, focus handed to Try again): VoiceOver drops what is announced within
 * about 300 ms of one, so the announcing part is replaced by an alert after the delay (`AFTER_FOCUS_MOVE_DELAY_MS`),
 * and until then the text is only shown. Like `announce`, it is read as the Alert mounts.
 */
export function ErrorAlert({
  id,
  title,
  announce = false,
  announceKey,
  announceDelayMs,
  status = "danger",
  remedy,
  details,
  actions,
  children,
  className = "",
}: {
  /** For a field that is described by the alert. */
  id?: string;
  title: React.ReactNode;
  announce?: boolean;
  announceKey?: number;
  announceDelayMs?: number;
  status?: "danger" | "warning";
  /** The one main thing to do, shown under the description and not announced. */
  remedy?: React.ReactNode;
  details?: React.ReactNode;
  actions?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
}): React.ReactElement {
  const announcement = useAnnouncement(announce, announceKey, announceDelayMs);
  return (
    <div id={id} className={`min-w-0 shrink-0 text-start ${className}`}>
      <AlertFrame status={status} className="flex-wrap">
        <Alert.Indicator aria-hidden="true" />
        <Alert.Content className="min-w-40 basis-0">
          <div key={announcement.key} role={announcement.role} className="flex min-w-0 max-w-full flex-col items-start">
            <Alert.Title>{title}</Alert.Title>
            {children && <Alert.Description>{children}</Alert.Description>}
          </div>
          {remedy && <Alert.Description>{remedy}</Alert.Description>}
        </Alert.Content>
        {(actions || details) && (
          <div className="ms-auto flex shrink-0 items-center gap-2">
            <AlertStatusContext.Provider value={status}>
              {/* The recovery action is the primary one, so it takes the trailing edge, as a dialog's
                  confirming button does; Details stands before it. */}
              {details && <DetailsButton inAlert>{details}</DetailsButton>}
              {actions}
            </AlertStatusContext.Provider>
          </div>
        )}
      </AlertFrame>
    </div>
  );
}
