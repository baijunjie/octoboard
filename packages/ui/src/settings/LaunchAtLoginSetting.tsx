import React, { useRef } from "react";

import { raiseError } from "../components/alerts/raiseError";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { ActionableError } from "../errors/actionableError";
import { InlinePath, pathText } from "../i18n/InlinePath";
import { Message, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { Button } from "../primitives";
import { SettingRow, SettingSwitch } from "./SettingRow";
import { useSettingsReads } from "./settingsReads";

/** Whether the system opens the app when the user logs in. Rendered only where the shell can register it
 * (`GeneralSection`), and once the system's state is known to the dialog (`shown` from `useLaunchAtLogin`, which
 * the search counts the row by too): read ahead of the first opening and kept from the last answer, so the row is
 * there as the dialog opens. What the switch shows is the system's own state, and its description
 * also says what the switch adds to the system and takes away again, the entry in the system's list of what opens at
 * login (P9 in `docs/conventions/principles.md`). A registration the
 * system holds for the user's approval shows as on, with the description saying where to allow it and a button that
 * opens that list. */
export function LaunchAtLoginSetting(): React.ReactElement | null {
  const t = useT();
  const { launchAtLogin } = usePlatform();
  const { status, shown, change } = useSettingsReads().launchAtLogin;
  const controlsRef = useRef<HTMLDivElement>(null);
  // The button goes once the user has approved the registration, which can be while focus is on it.
  const handoff = useFocusHandoff(() => controlsRef.current?.querySelector<HTMLElement>('input[role="switch"]')?.focus());

  if (!launchAtLogin || !shown) return null;
  const on = status !== "disabled";
  const requiresApproval = status === "requiresApproval";
  const description = requiresApproval
    ? <Message id="settings.launchAtLogin.description.requiresApproval" />
    : `${t(on ? "settings.launchAtLogin.description.on" : "settings.launchAtLogin.description.off")}${t("common.sentenceSeparator")}${t(on ? "settings.launchAtLogin.description.entry.on" : "settings.launchAtLogin.description.entry.off")}`;

  const openSettings = async () => {
    try {
      await launchAtLogin.openSystemSettings();
    } catch (cause) {
      const steps = [t("settings.launchAtLogin.step.general"), t("settings.launchAtLogin.step.loginItems")];
      raiseError(new ActionableError(t("settings.launchAtLogin.openFailed", { path: pathText(steps) }), "none", { cause }), {
        title: t("settings.launchAtLogin.openFailedTitle"),
        content: <Message id="settings.launchAtLogin.openFailed" params={{ path: <InlinePath steps={steps} /> }} />,
      });
    }
  };

  return (
    <SettingRow compact={!requiresApproval} searchId="launch-at-login" label={t("settings.launchAtLogin.label")} description={description}>
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
