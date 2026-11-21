import React, { useState } from "react";

import { alertFailure } from "../components/alerts/raiseError";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { useT } from "../i18n/react";
import { useDaemon } from "../store";
import type { SettingsSearchId } from "./settingsSearch";
import { SettingRow, SettingSwitch } from "./SettingRow";

/**
 * The row of an Allow switch of the Access section: Allow remote access (`set_remote_access`) or Allow browser access
 * (`set_browser_access`). These are not settings of the configuration document: the daemon keeps them apart and each
 * travels as a request of its own. Turning it on is asked first, with `confirmMessage`, since whoever reaches the address
 * with its key can do everything the owner can, and the dialog sends it; the switch shows the daemon's own state
 * meanwhile. Turning it off is sent at once. A failure is an alert titled `failedTitle`.
 */
export function AccessSwitchRow({ searchId, label, description, allowed, setting, failedTitle, confirmTitle, confirmMessage }: {
  searchId: SettingsSearchId;
  label: string;
  description: React.ReactNode;
  allowed: boolean;
  setting: "set_remote_access" | "set_browser_access";
  failedTitle: string;
  confirmTitle: string;
  confirmMessage: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [confirming, setConfirming] = useState(false);

  const change = (value: boolean) => {
    if (value) {
      setConfirming(true);
      return undefined;
    }
    return request({ type: setting, allowed: false }).catch(alertFailure({ title: failedTitle }));
  };

  return (
    <>
      <SettingRow compact searchId={searchId} label={label} description={description}>
        <SettingSwitch label={label} isSelected={allowed} onChange={change} />
      </SettingRow>
      {confirming && (
        <ConfirmDialog
          title={confirmTitle}
          errorTitle={failedTitle}
          message={confirmMessage}
          confirmLabel={t("settings.remoteAccess.confirm.confirm")}
          destructive
          onConfirm={async () => {
            await request({ type: setting, allowed: true });
            setConfirming(false);
          }}
          onCancel={() => setConfirming(false)}
        />
      )}
    </>
  );
}
