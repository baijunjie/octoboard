import { useEffect, useRef, useState } from "react";

import { raiseError } from "../components/alerts/raiseError";
import { onWindowFocusReturn } from "../components/hooks/windowFocus";
import { useT } from "../i18n/react";
import type { LaunchAtLoginStatus } from "../platform";
import { usePlatform } from "../platform/react";

/** The launch-at-login state of the system as the Settings dialog holds it (`useLaunchAtLogin`). */
export interface LaunchAtLoginRead {
  /** What the system says: `undefined` until the first read is in, and where the shell cannot register the app. */
  status: LaunchAtLoginStatus | undefined;
  /** The row is on screen: the shell can register the app and the first read has answered. The row and the search
   * both go by this. */
  shown: boolean;
  /** Asks the system to register or unregister the app and takes what it answers; a refusal is raised as an error alert. */
  change: (value: boolean) => Promise<void>;
}

/** Whether the system opens the app when the user logs in, read from the system when the Settings dialog opens and
 * again whenever the window gets focus back, since the user can change it in the system's list of login items;
 * nothing else is kept. It is read here, in the dialog, and not in the row (`LaunchAtLoginSetting`), because the search
 * counts the row only once this has answered, and a section the search takes out of the list holds no row to ask. A
 * read that overlaps a change is dropped, so it cannot show the state from before the change over its answer. */
export function useLaunchAtLogin(): LaunchAtLoginRead {
  const t = useT();
  const { launchAtLogin } = usePlatform();
  const [status, setStatus] = useState<LaunchAtLoginStatus | undefined>();
  // Whether a change is out, and which stretch of changes a read began in: a read that began before the change, or
  // answers while one is out, shows a state the change is about to replace.
  const changing = useRef(false);
  const epoch = useRef(0);

  useEffect(() => {
    if (!launchAtLogin) return;
    let current = true;
    const read = () => {
      if (changing.current) return;
      const began = epoch.current;
      const shown = () => current && !changing.current && epoch.current === began;
      void launchAtLogin.status().then(
        (answer) => shown() && setStatus(answer),
        // A system that cannot say shows the switch off, which a press then tries to change.
        () => shown() && setStatus((before) => before ?? "disabled"),
      );
    };
    read();
    const undoFocus = onWindowFocusReturn(read);
    return () => {
      current = false;
      undoFocus();
    };
  }, [launchAtLogin]);

  // A change ends whichever way it went: the epoch moves on again, so a read begun meanwhile is still dropped.
  const settle = () => {
    changing.current = false;
    epoch.current++;
  };
  const change = async (value: boolean) => {
    if (!launchAtLogin) return;
    changing.current = true;
    epoch.current++;
    try {
      setStatus(await launchAtLogin.set(value));
    } catch (error) {
      // An unexpected failure: the alert offers Report issue and keeps what the system said behind Error details.
      raiseError(error instanceof Error ? error : new Error(String(error)), {
        title: t("settings.launchAtLogin.failedTitle"),
        content: t("settings.launchAtLogin.failed"),
      });
      // The system may have changed its state before it refused, so the switch shows what it says now.
      await launchAtLogin.status().then(setStatus, () => {});
    }
    settle();
  };

  return { status, shown: launchAtLogin !== undefined && status !== undefined, change };
}
