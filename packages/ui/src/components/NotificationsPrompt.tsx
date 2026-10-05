import { Button } from "@heroui/react";
import React, { useState } from "react";

import { usePlatform } from "../platform/react";

/**
 * Asks for notification permission where the platform wants the ask to come from a user gesture (a
 * browser), as one quiet row at the foot of the sidebar. It disappears once the answer is given,
 * whichever it is, or when the user dismisses it for this window; a notification that cannot be
 * shown leaves the tree's own raised-hand marker as the signal (see "The raised hand" in
 * `docs/product/sessions.md`). Hidden where the platform needs no ask from a gesture.
 */
export function NotificationsPrompt(): React.ReactElement | null {
  const { notifications } = usePlatform();
  const prompt = notifications?.permissionPrompt;
  // Re-read after each ask: the browser's answer is not an event this component is told about.
  const [status, setStatus] = useState(() => prompt?.status());
  const [dismissed, setDismissed] = useState(false);
  if (!prompt || status !== "undecided" || dismissed) return null;
  return (
    <div className="flex shrink-0 items-center gap-1 border-t border-separator px-3 py-1 text-xs text-muted">
      <span className="min-w-0 flex-1">Get notified when a session is waiting for you.</span>
      <Button
        size="sm"
        variant="outline"
        preventFocusOnPress
        onPress={() => void prompt.request().then(() => setStatus(prompt.status()))}
      >
        Enable
      </Button>
      <Button isIconOnly size="sm" variant="ghost" preventFocusOnPress aria-label="Not now" onPress={() => setDismissed(true)}>
        ×
      </Button>
    </div>
  );
}
