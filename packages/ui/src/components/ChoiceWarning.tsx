import React from "react";

import { ErrorAlert } from "./ErrorAlert";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "./StatusAnnouncer";

/**
 * A warning about the consequence of a choice the user is making ("On the screen: an Alert when it matters, a mark at
 * its point otherwise" in `docs/ui-accessibility.md`). The caller mounts it only while the option is selected and
 * puts its `id` in the option's `aria-describedby`. It is announced as it mounts, after
 * `AFTER_FOCUS_MOVE_DELAY_MS`, because the tick has VoiceOver speaking the checkbox's new state at that moment.
 */
export function ChoiceWarning({
  id,
  title,
  remedy,
  children,
}: {
  id: string;
  title: React.ReactNode;
  remedy: React.ReactNode;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <ErrorAlert id={id} status="warning" announce announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} title={title} remedy={remedy}>
      {children}
    </ErrorAlert>
  );
}
