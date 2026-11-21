import React, { useState } from "react";

import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { useT } from "../i18n/react";
import { Button } from "../primitives";
import { useDaemon } from "../store";
import { SettingRow } from "./SettingRow";

/**
 * The Rotate key row of the Access section, shown while either way in serves: the daemon holds one
 * owner key for both, so rotating it disconnects the browser pages and the other computers' apps that use the old one
 * (`rotate_owner_key`). Asks first, since the pages have to be opened again at the new address; the apps carry on with
 * the new key by themselves. Where the daemon has no browser port (`browser` is false) there are no pages, and the
 * row speaks of the apps alone.
 */
export function RotateKeyRow({ browser }: { browser: boolean }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [confirming, setConfirming] = useState(false);
  return (
    <>
      <SettingRow compact searchId="remote-access.rotate" label={t("settings.remoteAccess.rotate.label")} description={t(browser ? "settings.remoteAccess.rotate.descriptionShared" : "settings.remoteAccess.rotate.descriptionApp")}>
        <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => setConfirming(true)}>
          {t("settings.remoteAccess.rotate.label")}
        </Button>
      </SettingRow>
      {confirming && (
        <ConfirmDialog
          title={t("settings.remoteAccess.rotate.confirmTitle")}
          errorTitle={t("settings.remoteAccess.rotate.failedTitle")}
          message={t(browser ? "settings.remoteAccess.rotate.confirmMessageShared" : "settings.remoteAccess.rotate.confirmMessageApp")}
          confirmLabel={t("settings.remoteAccess.rotate.confirm")}
          onConfirm={async () => {
            await request({ type: "rotate_owner_key" });
            setConfirming(false);
          }}
          onCancel={() => setConfirming(false)}
        />
      )}
    </>
  );
}
