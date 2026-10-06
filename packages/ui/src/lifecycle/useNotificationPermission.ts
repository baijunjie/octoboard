import { useEffect, useState } from "react";

import type { NotificationPermissionStatus } from "../platform";
import { usePlatform } from "../platform/react";

// A browser's answer is not an event, so every hook instance is told when any of them asked: the
// top bar's bell goes away when the Settings dialog's button is pressed.
const askedListeners = new Set<() => void>();

/**
 * The notification permission as the platform reports it, and how to ask for it. `status` is
 * `undefined` where there is no way to read it (no notifications at all, or a platform that asks
 * by itself when it first notifies), and `request` is `undefined` where asking needs no user
 * gesture or is not possible. Re-read when the window regains focus, since the browser's own site
 * settings can change it behind the page.
 */
export function useNotificationPermission(): {
  status: NotificationPermissionStatus | undefined;
  request: (() => Promise<void>) | undefined;
} {
  const { notifications } = usePlatform();
  const prompt = notifications?.permissionPrompt;
  const [status, setStatus] = useState(() => prompt?.status());

  useEffect(() => {
    if (!prompt) return;
    const refresh = () => setStatus(prompt.status());
    askedListeners.add(refresh);
    window.addEventListener("focus", refresh);
    return () => {
      askedListeners.delete(refresh);
      window.removeEventListener("focus", refresh);
    };
  }, [prompt]);

  return {
    status,
    request: prompt
      ? async () => {
          await prompt.request();
          for (const listener of askedListeners) listener();
        }
      : undefined,
  };
}
