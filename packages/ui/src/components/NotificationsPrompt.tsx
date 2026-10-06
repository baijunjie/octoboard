import { Button } from "@heroui/react";
import { X } from "lucide-react";
import React, { useState } from "react";

import { useNotificationPermission } from "../lifecycle/useNotificationPermission";

/**
 * Asks for notification permission where the platform wants the ask to come from a user gesture (a
 * browser), as one quiet row at the foot of the sidebar. It disappears once the answer is given,
 * whichever it is, or when the user dismisses it for this window; a notification that cannot be
 * shown leaves the tree's own raised-hand marker as the signal (see "The raised hand" in
 * `docs/product/sessions.md`). Hidden where the platform needs no ask from a gesture. The Settings
 * dialog's Notifications section is the other place to see and change the answer.
 */
export function NotificationsPrompt(): React.ReactElement | null {
  const { status, request } = useNotificationPermission();
  const [dismissed, setDismissed] = useState(false);
  if (!request || status !== "undecided" || dismissed) return null;
  return (
    <div className="flex shrink-0 items-center gap-1 border-t border-separator px-3 py-1 text-xs text-muted">
      <span className="min-w-0 flex-1">Get notified when a session is waiting for you.</span>
      <Button
        size="sm"
        variant="outline"
        preventFocusOnPress
        onPress={() => void request()}
      >
        Enable
      </Button>
      <Button isIconOnly size="sm" variant="ghost" preventFocusOnPress aria-label="Not now" onPress={() => setDismissed(true)}>
        <X aria-hidden="true" className="size-4" />
      </Button>
    </div>
  );
}
