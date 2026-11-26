import { useEffect, useRef, useState } from "react";

import { raiseError } from "../components/alerts/raiseError";
import { onWindowFocusReturn } from "../components/hooks/windowFocus";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { LaunchAtLoginCapability, LaunchAtLoginStatus } from "../platform";
import { usePlatform } from "../platform/react";

/** The launch-at-login state of the system as the Settings dialog holds it (`useLaunchAtLogin`). */
export interface LaunchAtLoginRead {
  /** What the system says, or last said (see `useLaunchAtLogin`): `undefined` until it has answered once, and where
   * the shell cannot register the app. */
  status: LaunchAtLoginStatus | undefined;
  /** The row is on screen: the shell can register the app and its state is known. The row and the search
   * both go by this. */
  shown: boolean;
  /** Asks the system to register or unregister the app and takes what it answers; a refusal is raised as an error alert. */
  change: (value: boolean) => Promise<void>;
}

/** The last state each shell's system answered with, kept across openings of the Settings dialog: the dialog starts
 * from it, so the row is there from the dialog's first render. A row that came in once a read answered would arrive
 * after the section's list motion began watching, and slide in as an added row each time the dialog opened. */
const lastAnswered = new WeakMap<LaunchAtLoginCapability, LaunchAtLoginStatus>();
// The capabilities a preload has been started for, so that it reads once, even while its first read is unanswered.
const preloading = new WeakSet<LaunchAtLoginCapability>();

/** Keeps what the system answered for the next opening and hands it on to be shown. */
function remember(launchAtLogin: LaunchAtLoginCapability, answer: LaunchAtLoginStatus): LaunchAtLoginStatus {
  lastAnswered.set(launchAtLogin, answer);
  return answer;
}

/** Reads the system's state once, ahead of the first opening of the Settings dialog (`useSettingsDialog` calls it as
 * the window starts), so that opening finds it already known. An answer the dialog has meanwhile replaced is dropped;
 * a failure leaves nothing, and the dialog then reads as it opens. */
export function preloadLaunchAtLogin(launchAtLogin: LaunchAtLoginCapability): void {
  if (preloading.has(launchAtLogin) || lastAnswered.has(launchAtLogin)) return;
  preloading.add(launchAtLogin);
  void launchAtLogin.status().then(
    (answer) => {
      if (!lastAnswered.has(launchAtLogin)) lastAnswered.set(launchAtLogin, answer);
    },
    () => {},
  );
}

/** Whether the system opens the app when the user logs in, read from the system when the Settings dialog opens and
 * again whenever the window gets focus back, since the user can change it in the system's list of login items. Until
 * the first of those reads answers, the dialog shows the last state the system answered with (`lastAnswered`), and a
 * read that finds it changed moves the switch. It is read here, in the dialog, and not in the row
 * (`LaunchAtLoginSetting`), because the search counts the row only once a state is known, and a section the search
 * takes out of the list holds no row to ask. A read that overlaps a change is dropped, so it cannot show the state from
 * before the change over its answer. */
export function useLaunchAtLogin(): LaunchAtLoginRead {
  const t = useT();
  const { launchAtLogin } = usePlatform();
  const [status, setStatus] = useState<LaunchAtLoginStatus | undefined>(() => (launchAtLogin ? lastAnswered.get(launchAtLogin) : undefined));
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
        (answer) => shown() && setStatus(remember(launchAtLogin, answer)),
        // A system that cannot say keeps the state last known, or shows the switch off where none is, which a press
        // then tries to change; the failure is kept for no later opening.
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
      setStatus(remember(launchAtLogin, await launchAtLogin.set(value)));
    } catch (error) {
      // An unexpected failure: the alert offers Report issue and keeps what the system said behind Error details.
      raiseError(error instanceof Error ? error : new Error(String(error)), {
        title: t("settings.launchAtLogin.failedTitle"),
        content: <Message id="settings.launchAtLogin.failed" params={{ setting: <InlineReference dir="auto" wrap>{t("settings.launchAtLogin.label")}</InlineReference> }} />,
      });
      // The system may have changed its state before it refused, so the switch shows what it says now.
      await launchAtLogin.status().then((answer) => setStatus(remember(launchAtLogin, answer)), () => {});
    }
    settle();
  };

  return { status, shown: launchAtLogin !== undefined && status !== undefined, change };
}
