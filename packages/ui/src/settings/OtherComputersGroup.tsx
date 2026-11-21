import React, { useState } from "react";
import { CircleCheck } from "lucide-react";

import { raiseError } from "../components/alerts/raiseError";
import { CopyCommand } from "../components/controls/CopyBox";
import { QrCode } from "../components/qr/QrCode";
import { StatusBadge } from "../components/status/StatusBadge";
import { TextTabStrip } from "../components/tabs/TextTabStrip";
import { ActionableError } from "../errors/actionableError";
import type { PlainMessageKey } from "../i18n/catalog";
import { InlinePath, pathText } from "../i18n/InlinePath";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import { daemonOs } from "../i18n/platformTerms";
import { usePlatform } from "../platform/react";
import { Button, Tabs } from "../primitives";
import type { RemoteAccess } from "../protocol";
import { AccessFailureAlert } from "./AccessFailureAlert";
import { AccessSwitchRow } from "./AccessSwitchRow";
import { browserPortFailed, hasBrowserPort, servedBrowserAddress, servedConnection, servedForwardCommand } from "./accessState";
import { ForwardCommand } from "./ForwardCommand";
import { SettingRow } from "./SettingRow";
import { SettingsGroup } from "./SettingsGroup";
import { SettingsTabPanel } from "./SettingsTabPanel";
import { useSettingsSearchFilter } from "./settingsSearchFilter";
import type { SSH_SERVER_NAMES } from "./sshServerWords";
import { sshServerNames, sshServerWords } from "./sshServerWords";

/** What the SSH server sentences take (see `SSH_SERVER_NAMES`), as `V`. */
type SshParams<V> = Record<"path" | keyof typeof SSH_SERVER_NAMES, V>;

/** The two ways in from another computer, each a tab: its application and a browser. */
type WayIn = "app" | "browser";

/**
 * The Access section's Other computers group: the computer's SSH server (macOS Remote Login, Windows OpenSSH Server),
 * which both ways in from another computer need, with whether it is on (`loginOn`; a change of it, which the user makes
 * outside the app, is announced by the section). Under it, where the daemon has a browser port, a tab for each way in,
 * the application's first (`WayIn`); without a browser port there are no tabs, just the application's rows. The
 * application's tab: the Allow remote access switch for another computer's application, why remote access could not
 * serve (while it could not), and, while it serves, the connection details another device's application fills its Add
 * remote daemon form from (the text to copy and its QR code). The Browser tab: while browser access serves, step 1 the
 * forward command to copy (its address is a token that opens a menu of the ones the daemon lists for this computer, its
 * name first, with a last item that leaves `<server>` to fill in; with none listed the command is plain text and the
 * owner fills it in) and step 2 pointing at the local address in the browser group; while it is off, a line that says to
 * turn it on; while it is on but the port could not be opened, a line that says the port is not open and points at the
 * alert in the browser group. A search that finds rows in one tab only shows that tab. `announce` is for a failure that
 * appears after the section has painted.
 */
export function OtherComputersGroup({ remoteAccess, loginOn, announce }: { remoteAccess: RemoteAccess; loginOn: boolean | undefined; announce: boolean }): React.ReactElement {
  const t = useT();
  const { remoteLogin } = usePlatform();
  // The section is in the local window only, so the daemon whose section it is runs on this computer.
  const sshServer = sshServerWords(daemonOs());
  const [tab, setTab] = useState<WayIn>("app");
  const { shows } = useSettingsSearchFilter();
  const { allowed, failure, browser_allowed: browserAllowed } = remoteAccess;
  const browserPort = hasBrowserPort(remoteAccess);
  // A daemon that predates the list sends none.
  const addresses: readonly string[] = remoteAccess.server_addresses ?? [];
  const connection = servedConnection(remoteAccess);
  const forwardCommand = servedForwardCommand(remoteAccess);
  const portServed = servedBrowserAddress(remoteAccess) !== null;
  const portFailed = browserPortFailed(remoteAccess);

  // The SSH server sentences' names and path, as chips and a breadcrumb where they show, as text where they are read
  // out (the error's message). Every sentence is given the same names, which the one that has no use for a name ignores.
  const chip = (key: PlainMessageKey) => <InlineReference dir="auto" wrap>{t(key)}</InlineReference>;
  function sshParams(sentence: keyof typeof sshServer.paths, rich: true): SshParams<React.ReactNode>;
  function sshParams(sentence: keyof typeof sshServer.paths, rich: false): SshParams<string>;
  function sshParams(sentence: keyof typeof sshServer.paths, rich: boolean): SshParams<React.ReactNode> {
    const steps = sshServer.paths[sentence].map((key) => t(key));
    return { path: rich ? <InlinePath steps={steps} /> : pathText(steps), ...(rich ? sshServerNames(chip) : sshServerNames((key) => t(key))) };
  }

  const openRemoteLoginSettings = async () => {
    if (!remoteLogin) return;
    try {
      await remoteLogin.openSystemSettings();
    } catch (cause) {
      raiseError(new ActionableError(t(sshServer.openFailed, sshParams("openFailed", false)), "none", { cause }), {
        title: t("settings.remoteAccess.remoteLogin.openFailedTitle"),
        content: <Message id={sshServer.openFailed} params={sshParams("openFailed", true)} />,
      });
    }
  };

  // The rows of each way in from another computer, as the tabs hold them: another computer's application (the switch, its
  // failure, the connection details) and a browser (the steps, or the line to turn browser access on).
  const appRows = (
    <>
      <AccessSwitchRow
        searchId="remote-access.allow"
        label={t("settings.remoteAccess.allow.label")}
        description={t(allowed ? "settings.remoteAccess.allow.ways.on" : "settings.remoteAccess.allow.ways.off")}
        allowed={allowed}
        setting="set_remote_access"
        failedTitle={t("settings.remoteAccess.allow.failedTitle")}
        confirmTitle={t("settings.remoteAccess.confirm.title")}
        confirmMessage={
          <>
            <p>{t("settings.remoteAccess.confirm.access")}</p>
            <p>{t("settings.remoteAccess.confirm.keyApp")}</p>
            <p>{t("settings.remoteAccess.confirm.ssh")}</p>
          </>
        }
      />
      {failure && <AccessFailureAlert id="remote-access.failed" title={t("settings.remoteAccess.failed.titleWays")} failure={failure} way="remote" announce={announce} />}
      {connection && (
        <SettingRow
          searchId="remote-access.connection"
          label={t("settings.remoteAccess.connection.label")}
          description={
            <Message
              id="settings.remoteAccess.connection.description"
              params={{ addRemoteDaemon: <InlineReference>{t("remoteDaemons.add")}</InlineReference>, connectionDetails: <InlineReference>{t("remoteDaemons.form.details")}</InlineReference> }}
            />
          }
          below={
            <div className="flex flex-col gap-3">
              <CopyCommand text={connection.text} copyText={t("settings.remoteAccess.connection.copy")} failedText={t("settings.remoteAccess.connection.copyFailed")} />
              <QrCode rows={connection.qr} label={t("settings.remoteAccess.connection.qr")} />
            </div>
          }
        />
      )}
    </>
  );
  const browserRows = (
    <>
      {forwardCommand !== null && (
        <SettingRow
          searchId="remote-access.browser-forward"
          label={t("settings.remoteAccess.forward.label")}
          description={
            addresses.length === 0
              ? <Message id="settings.remoteAccess.forward.description" params={{ server: <InlineReference monospace>{"<server>"}</InlineReference> }} />
              : (
                <Message
                  id="settings.remoteAccess.forward.descriptionPick"
                  params={{ manual: <InlineReference>{t("settings.remoteAccess.forward.address.manual")}</InlineReference>, server: <InlineReference monospace>{"<server>"}</InlineReference> }}
                />
              )
          }
          below={
            <ForwardCommand
              command={forwardCommand}
              addresses={addresses}
              copyText={t("settings.remoteAccess.forward.copy")}
              failedText={t("settings.remoteAccess.forward.copyFailed")}
            />
          }
        />
      )}
      {portServed && (
        <SettingRow
          searchId="remote-access.browser-address"
          label={t("settings.remoteAccess.address.labelAbove")}
          description={<Message id="settings.remoteAccess.address.descriptionAbove" params={{ localAddress: <InlineReference>{t("settings.remoteAccess.local.label")}</InlineReference> }} />}
        />
      )}
      {/* While browser access is on but its port is not open, the alert in the browser group says why, and this line points at it. */}
      {portFailed && (
        <SettingRow
          searchId="remote-access.browser-closed"
          label={t("settings.remoteAccess.browserClosed.label")}
          description={<Message id="settings.remoteAccess.browserClosed.description" params={{ allowBrowserAccess: <InlineReference>{t("settings.remoteAccess.browserAccess.label")}</InlineReference> }} />}
        />
      )}
      {!browserAllowed && (
        <SettingRow
          searchId="remote-access.browser-off"
          label={t("settings.remoteAccess.openInBrowser.label")}
          description={<Message id="settings.remoteAccess.openInBrowser.off" params={{ allowBrowserAccess: <InlineReference>{t("settings.remoteAccess.browserAccess.label")}</InlineReference> }} />}
        />
      )}
    </>
  );
  // What each way in shows, so that a search can tell which tab holds what it found.
  const appShown = shows("remote-access.allow") || (failure != null && shows("remote-access.failed")) || (connection !== null && shows("remote-access.connection"));
  const browserShown = (forwardCommand !== null && shows("remote-access.browser-forward")) || (portServed && shows("remote-access.browser-address")) || (portFailed && shows("remote-access.browser-closed")) || (browserPort && !browserAllowed && shows("remote-access.browser-off"));
  // The tabs are there only where the daemon has a browser port, and while a search finds something in one of them.
  const tabbed = browserPort && (appShown || browserShown);
  // A search that finds rows in one tab only shows that one, whichever the user left selected.
  const shownTab: WayIn = tab === "app" && !appShown && browserShown ? "browser" : tab === "browser" && !browserShown && appShown ? "app" : tab;
  // A tab with nothing to show cannot be chosen: while a search leaves it empty (it can be chosen again once the search
  // is over, or finds something in both).
  const disabledTabs: WayIn[] = appShown === browserShown ? [] : appShown ? ["browser"] : ["app"];

  return (
    <SettingsGroup title="settings.remoteAccess.group.others">
      {/* First, since the way in logs in over SSH. */}
      <SettingRow
        compact
        searchId="remote-access.remote-login"
        label={t(sshServer.label)}
        status={loginOn !== true && (
          <Message id={loginOn === false ? sshServer.off : sshServer.unknown} params={sshParams(loginOn === false ? "off" : "unknown", true)} />
        )}
        description={<Message id={browserPort ? sshServer.descriptionBoth : sshServer.description} params={sshServerNames(chip)} />}
      >
        {loginOn === true ? (
          <StatusBadge size="lg" color="success" variant="soft" separated={false} className="inline-flex items-center gap-1">
            <CircleCheck size={16} />
            {t("settings.remoteAccess.remoteLogin.on")}
          </StatusBadge>
        ) : (
          <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => void openRemoteLoginSettings()}>
            {t("settings.remoteAccess.remoteLogin.openSettings")}
          </Button>
        )}
      </SettingRow>
      {tabbed ? (
        <Tabs variant="underline" selectedKey={shownTab} disabledKeys={disabledTabs} onSelectionChange={(key) => setTab(key as WayIn)} className="gap-0">
          <TextTabStrip
            underline="hairline"
            label={t("settings.remoteAccess.tabs.label")}
            tabs={[
              { id: "app", label: t("settings.remoteAccess.tabs.app") },
              { id: "browser", label: t("settings.remoteAccess.tabs.browser") },
            ]}
          />
          <SettingsTabPanel id="app">{appRows}</SettingsTabPanel>
          <SettingsTabPanel id="browser">{browserRows}</SettingsTabPanel>
        </Tabs>
      ) : appRows}
    </SettingsGroup>
  );
}
