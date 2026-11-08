import React, { useEffect, useRef, useState } from "react";

import { raiseError } from "../components/alerts/raiseError";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { onWindowFocusReturn } from "../components/hooks/windowFocus";
import { ActionableError } from "../errors/actionableError";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { LaunchAtLoginStatus } from "../platform";
import { usePlatform } from "../platform/react";
import { Button } from "../primitives";
import { SettingRow, SettingSwitch } from "./SettingRow";

/** Whether the system opens the app when the user logs in. Rendered only where the shell can register it
 * (`AppBehaviorSection`). What the switch shows is the system's own state, read when the row appears and again whenever
 * the window gets focus back, since the user can change it in the system's list of login items; nothing is kept here. A
 * read that overlaps a change is dropped, so it cannot show the state from before the change over its answer. A
 * registration the system holds for the user's approval shows as on, with the description saying where to allow it and
 * a button that opens that list. */
export function LaunchAtLoginSetting(): React.ReactElement | null {
  const t = useT();
  const { launchAtLogin, windowFocus } = usePlatform();
  const [status, setStatus] = useState<LaunchAtLoginStatus | undefined>();
  // Whether a change is out, and which stretch of changes a read began in: a read that began before the change, or
  // answers while one is out, shows a state the change is about to replace.
  const changing = useRef(false);
  const epoch = useRef(0);
  const controlsRef = useRef<HTMLDivElement>(null);
  // The button goes once the user has approved the registration, which can be while focus is on it.
  const handoff = useFocusHandoff(() => controlsRef.current?.querySelector<HTMLElement>('input[role="switch"]')?.focus());

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
    const undoFocus = onWindowFocusReturn(windowFocus, read);
    return () => {
      current = false;
      undoFocus();
    };
  }, [launchAtLogin, windowFocus]);

  if (!launchAtLogin || status === undefined) return null;
  const on = status !== "disabled";
  const requiresApproval = status === "requiresApproval";
  const description = requiresApproval
    ? <Message id="settings.launchAtLogin.description.requiresApproval" />
    : t(on ? "settings.launchAtLogin.description.on" : "settings.launchAtLogin.description.off");

  // A change ends whichever way it went: the epoch moves on again, so a read begun meanwhile is still dropped.
  const settle = () => {
    changing.current = false;
    epoch.current++;
  };
  const change = async (value: boolean) => {
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

  const openSettings = async () => {
    try {
      await launchAtLogin.openSystemSettings();
    } catch (cause) {
      const pane = t("settings.launchAtLogin.pane");
      raiseError(new ActionableError(t("settings.launchAtLogin.openFailed", { pane }), "none", { cause }), {
        title: t("settings.launchAtLogin.openFailedTitle"),
        content: <Message id="settings.launchAtLogin.openFailed" params={{ pane: <InlineReference dir="auto" wrap>{pane}</InlineReference> }} />,
      });
    }
  };

  return (
    <SettingRow compact={!requiresApproval} label={t("settings.launchAtLogin.label")} description={description}>
      <div ref={controlsRef} className="flex items-center gap-3" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        {requiresApproval && (
          <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => void openSettings()}>
            {t("settings.launchAtLogin.openSettings")}
          </Button>
        )}
        <SettingSwitch label={t("settings.launchAtLogin.label")} isSelected={on} onChange={change} />
      </div>
    </SettingRow>
  );
}
