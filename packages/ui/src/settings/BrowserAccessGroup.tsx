import React, { useState } from "react";

import { CopyCommand } from "../components/controls/CopyBox";
import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction, useSubmitValidation } from "../components/dialog/dialogHooks";
import { TextInput } from "../components/form/TextInput";
import { useExternalLinkAction } from "../components/text/ExternalLink";
import { ExternalLinkIcon } from "../components/text/ExternalLinkIcon";
import { Message, useT } from "../i18n/react";
import { Button } from "../primitives";
import { parsePort } from "../remoteDaemons/port";
import { useDaemon } from "../store";
import { AccessFailureAlert } from "./AccessFailureAlert";
import { AccessSwitchRow } from "./AccessSwitchRow";
import { servedBrowserAddress, type BrowserPortAccess } from "./accessState";
import { SettingRow } from "./SettingRow";
import { SettingsGroup } from "./SettingsGroup";

/**
 * The Access section's This computer's browser group, only where the daemon has a browser port (`hasBrowserPort`): the
 * Allow browser access switch (its confirmation adds that another user of the computer can take the browser port while
 * the app is not running), why the port could not be opened, while it could not, the browser port with the action that
 * changes it (taking effect at once and disconnecting the pages open at the old port, which says so when the port is
 * already in use), and, where the daemon serves the browser at its port (`servedBrowserAddress`), the local address with
 * Copy and an action that opens it in the default browser. `announce` is for a failure that appears after the section
 * has painted.
 */
export function BrowserAccessGroup({ remoteAccess, announce }: { remoteAccess: BrowserPortAccess; announce: boolean }): React.ReactElement {
  const t = useT();
  const [changingPort, setChangingPort] = useState(false);
  const { browser_allowed: browserAllowed, browser_failure: browserFailure, browser_port: browserPort } = remoteAccess;
  const localAddress = servedBrowserAddress(remoteAccess);

  return (
    <SettingsGroup title="settings.remoteAccess.group.browser">
      <AccessSwitchRow
        searchId="remote-access.browser-allow"
        label={t("settings.remoteAccess.browserAccess.label")}
        description={t(browserAllowed ? "settings.remoteAccess.browserAccess.on" : "settings.remoteAccess.browserAccess.off")}
        allowed={browserAllowed}
        setting="set_browser_access"
        failedTitle={t("settings.remoteAccess.browserAccess.failedTitle")}
        confirmTitle={t("settings.remoteAccess.browserAccess.confirmTitle")}
        confirmMessage={
          <>
            <p>{t("settings.remoteAccess.confirm.access")}</p>
            <p>{t("settings.remoteAccess.confirm.key")}</p>
            <p>{t("settings.remoteAccess.confirm.port")}</p>
            <p>{t("settings.remoteAccess.confirm.portKey")}</p>
          </>
        }
      />
      {browserFailure && <AccessFailureAlert id="remote-access.browser-failed" title={t("settings.remoteAccess.failed.titleBrowser")} failure={browserFailure} way="browser" announce={announce} />}
      <SettingRow
        compact
        searchId="remote-access.browser-port"
        label={t("settings.remoteAccess.port.label")}
        description={
          <>
            {t("settings.browserAccess.port.description")}
            <span className="block"><Message id="settings.remoteAccess.port.current" params={{ port: String(browserPort) }} /></span>
          </>
        }
      >
        <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => setChangingPort(true)}>
          {t("settings.remoteAccess.port.change")}
        </Button>
      </SettingRow>
      {localAddress !== null && (
        <SettingRow
          searchId="remote-access.browser-local"
          label={t("settings.remoteAccess.local.label")}
          description={t("settings.remoteAccess.local.description")}
          below={
            <CopyCommand
              text={localAddress}
              copyText={t("settings.remoteAccess.local.copy")}
              failedText={t("settings.remoteAccess.local.copyFailed")}
              action={<OpenLocalAddress address={localAddress} />}
            />
          }
        />
      )}
      {changingPort && <PortDialog initialPort={browserPort} onClose={() => setChangingPort(false)} />}
    </SettingsGroup>
  );
}

/** The icon button that opens the local address in the default browser, through the external link's own open: a second
 * press while one is out is ignored, and a failure to open is the link's alert, naming the address to open by hand. */
function OpenLocalAddress({ address }: { address: string }): React.ReactElement {
  const t = useT();
  const action = useExternalLinkAction({ href: address });
  return (
    <Button size="xs" variant="ghost" iconOnly label={t("settings.remoteAccess.local.open")} tooltip={t("settings.remoteAccess.local.open")} pending={action.busy} focusOnPress={false} onPress={action.activate}>
      <ExternalLinkIcon className="size-3.5" />
    </Button>
  );
}

/** The dialog that changes the browser port (Windows, macOS): one field, a whole number from 1 to 65535, sent on Save; the
 * dialog closes once the daemon has taken it, and shows the failure otherwise. A port already in use is no failure of
 * the request: the section says so once the daemon has tried it. */
function PortDialog({ initialPort, onClose }: { initialPort: number; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [value, setValue] = useState(String(initialPort));
  const { busy, run } = useDialogAction(undefined, t("settings.remoteAccess.port.failedTitle"));
  const { shown, attempt } = useSubmitValidation();
  const port = parsePort(value);
  const valueError = port === undefined ? t("settings.remoteAccess.port.invalid") : undefined;

  const submit = () => {
    if (!attempt(valueError) || port === undefined) return;
    void run(async () => {
      await request({ type: "set_remote_access_port", port });
      onClose();
    });
  };

  return (
    <Dialog title={t("settings.remoteAccess.port.dialogTitle")} onClose={onClose} submitLabel={t("common.save")} busy={busy} onSubmit={submit} size="sm">
      <TextInput
        label={t("settings.remoteAccess.port.field")}
        description={t("settings.remoteAccess.port.fieldDescription")}
        value={value}
        onChange={setValue}
        placeholder={t("common.example", { example: String(initialPort) })}
        errorMessage={shown(valueError)}
        dir="ltr"
        autoFocus
      />
    </Dialog>
  );
}
