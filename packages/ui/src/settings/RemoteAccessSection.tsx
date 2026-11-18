import React, { useState } from "react";
import { CircleCheck } from "lucide-react";

import { AttendedStatusAnnouncer } from "../components/announce/AttendedStatusAnnouncer";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { useHasPainted } from "../components/motion/useHasPainted";
import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { alertFailure, raiseError } from "../components/alerts/raiseError";
import { CopyCommand } from "../components/controls/CopyBox";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction, useSubmitValidation } from "../components/dialog/dialogHooks";
import { TextInput } from "../components/form/TextInput";
import { QrCode } from "../components/qr/QrCode";
import { StatusBadge } from "../components/status/StatusBadge";
import { ActionableError } from "../errors/actionableError";
import type { PlainMessageKey } from "../i18n/catalog";
import { InlinePath, pathText } from "../i18n/InlinePath";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import { daemonOs } from "../i18n/platformTerms";
import { usePlatform } from "../platform/react";
import { TextTabStrip } from "../components/tabs/TextTabStrip";
import { Button, Tabs } from "../primitives";
import type { RemoteAccessFailureReason } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { ForwardCommand } from "./ForwardCommand";
import { SettingRow, SettingSwitch } from "./SettingRow";
import { SettingsTabPanel } from "./SettingsTabPanel";
import { LeadRows, SettingsLead } from "./SettingsLead";
import { SettingsSearchBlock, useSettingsSearchFilter } from "./settingsSearchFilter";
import type { SSH_SERVER_NAMES } from "./sshServerWords";
import { sshServerNames, sshServerWords } from "./sshServerWords";
import { useRemoteLogin } from "./useRemoteLogin";
import { useSectionRefocus } from "./useSectionRefocus";
import { parsePort } from "../remoteDaemons/port";

/** The client's own sentence for why the owner socket is not served, per `RemoteAccessFailure.reason`; the daemon's
 * `detail` stands beside it. */
const FAILURE_REASON: Record<RemoteAccessFailureReason, PlainMessageKey> = {
  no_ui: "settings.remoteAccess.failed.noUi",
  not_private: "settings.remoteAccess.failed.notPrivate",
  socket_failed: "settings.remoteAccess.failed.socketFailed",
  owner_key_failed: "settings.remoteAccess.failed.ownerKeyFailed",
  port_in_use: "settings.remoteAccess.failed.portInUse",
  port_failed: "settings.remoteAccess.failed.portFailed",
  pipe_failed: "settings.remoteAccess.failed.pipeFailed",
};

/** What the SSH server sentences take (see `SSH_SERVER_NAMES`), as `V`. */
type SshParams<V> = Record<"path" | keyof typeof SSH_SERVER_NAMES, V>;

/** The two ways in, each a tab: another computer's application (the connection details) and a browser (the SSH forward
 * and the address). */
type WayIn = "app" | "browser";

/**
 * Remote access to this computer's own daemon from another computer's application or from a browser, each logging in
 * over SSH. Listed only in the application's window (`remoteAccessOffered`). In order: the computer's SSH server (macOS
 * Remote Login, Windows OpenSSH Server), which both ways need, with whether it is on (a change of it, which the user
 * makes outside the app, is announced); the Allow remote access switch, which is not a setting of the configuration
 * document: the daemon keeps it apart, and it travels as its own requests (`set_remote_access`, `rotate_owner_key`);
 * turning it on asks first, since whoever reaches the address with its key can do everything the owner can, and turning
 * it off does not; and, while it could not serve, why. While the daemon serves, the two ways in are tabs, the
 * application's first: the connection details another device's application fills its Add remote daemon form from (the
 * text to copy and its QR code), and the browser's: step 1 the forward command to copy (its address is a token that
 * opens a menu of the ones the daemon lists for this computer, its name first, with a last item that leaves `<server>`
 * to fill in; with none listed the command is plain text and the owner fills it in), step 2 the browser address to copy,
 * the browser port on Windows, and the action that rotates the key (asking first, since it disconnects the pages open
 * with the old one). A search that finds rows in one tab only shows that tab.
 *
 * On Windows, whose SSH server cannot forward to a socket, the daemon serves the browser at a loopback port of its own
 * instead (`browser_port` is set): the section shows the port with the action that changes it, which takes effect at
 * once and disconnects the pages open at the old port, says so when the port is already in use, and the confirmation
 * names the cost of a port another user of the computer can take while the daemon is not holding it. The port is the
 * user's setting whether or not the daemon serves, so while it does not serve the row sits under the switch, and while
 * it does, in the Browser tab.
 */
export function RemoteAccessSection(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const { remoteLogin } = usePlatform();
  // The section is in the local window only, so the daemon whose section it is runs on this computer.
  const sshServer = sshServerWords(daemonOs());
  const remoteAccess = useDaemonStore((s) => s.remoteAccess);
  const loginOn = useRemoteLogin();
  const [confirmingAllow, setConfirmingAllow] = useState(false);
  const [confirmingRotate, setConfirmingRotate] = useState(false);
  const [changingPort, setChangingPort] = useState(false);
  const [tab, setTab] = useState<WayIn>("app");
  const { shows } = useSettingsSearchFilter();
  const serving = remoteAccess?.state === "serving";
  // A failure that appears once the section has painted is announced; one already there as it opens is not.
  const hasPainted = useHasPainted();
  // The Rotate key button unmounts when serving ends and Open System Settings when Remote Login turns on, either while
  // it holds focus.
  useSectionRefocus([serving, loginOn]);

  if (!remoteAccess) return <></>;
  const { allowed, failure, browser_port: browserPort } = remoteAccess;
  // A daemon that predates the list sends none.
  const addresses: readonly string[] = remoteAccess.server_addresses ?? [];

  const setAllowed = (value: boolean) => {
    // Turning it on is asked first and sent by the dialog; the switch shows the daemon's own state meanwhile.
    if (value) {
      setConfirmingAllow(true);
      return undefined;
    }
    return request({ type: "set_remote_access", allowed: false }).catch(alertFailure({ title: t("settings.remoteAccess.allow.failedTitle") }));
  };

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

  const portRow = browserPort !== null && (
    <SettingRow
      compact
      searchId="remote-access.port"
      label={t("settings.remoteAccess.port.label")}
      description={
        <>
          {t("settings.remoteAccess.port.description")}
          <span className="block"><Message id="settings.remoteAccess.port.current" params={{ port: String(browserPort) }} /></span>
        </>
      }
    >
      <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => setChangingPort(true)}>
        {t("settings.remoteAccess.port.change")}
      </Button>
    </SettingRow>
  );

  // What each way in shows, so that a search can tell which tab holds what it found.
  const appShown = remoteAccess.connection !== null && shows("remote-access.connection");
  const browserShown = (remoteAccess.forward_command !== null && shows("remote-access.forward")) || (remoteAccess.address !== null && shows("remote-access.address"))
    || (browserPort !== null && shows("remote-access.port")) || shows("remote-access.rotate");
  // A search that finds rows in one tab only shows that one, whichever the user left selected.
  const shownTab: WayIn = tab === "app" && !appShown && browserShown ? "browser" : tab === "browser" && !browserShown && appShown ? "app" : tab;

  // A tab with nothing to show cannot be chosen: while a search leaves it empty (it can be chosen again once the search
  // is over, or finds something in both), or, for the application's, when the daemon gives no connection details.
  const disabledTabs: WayIn[] = appShown === browserShown ? [] : appShown ? ["browser"] : ["app"];

  return (
    <>
      <SettingsLead><Message id="settings.remoteAccess.lead" /></SettingsLead>
      <LeadRows ruledBelow={serving && (appShown || browserShown)}>
        {/* First, since both ways in log in over SSH. */}
        <SettingRow
          compact
          searchId="remote-access.remote-login"
          label={t(sshServer.label)}
          description={
            <>
              <Message id={sshServer.description} params={sshServerNames(chip)} />
              {/* Where it stands is a line of its own, not part of the searched description; a state on is the badge at
                  the row's end instead. */}
              {loginOn !== true && (
                <span className="block"><Message id={loginOn === false ? sshServer.off : sshServer.unknown} params={sshParams(loginOn === false ? "off" : "unknown", true)} /></span>
              )}
            </>
          }
        >
          {loginOn === true ? (
            <StatusBadge size="lg" color="success" variant="soft" separated={false} className="inline-flex items-center gap-1">
              <CircleCheck size={16} aria-hidden="true" />
              {t("settings.remoteAccess.remoteLogin.on")}
            </StatusBadge>
          ) : (
            <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => void openRemoteLoginSettings()}>
              {t("settings.remoteAccess.remoteLogin.openSettings")}
            </Button>
          )}
        </SettingRow>
        <SettingRow
          compact
          searchId="remote-access.allow"
          label={t("settings.remoteAccess.allow.label")}
          description={t(allowed ? "settings.remoteAccess.allow.ways.on" : "settings.remoteAccess.allow.ways.off")}
        >
          <SettingSwitch label={t("settings.remoteAccess.allow.label")} isSelected={allowed} onChange={setAllowed} />
        </SettingRow>
        {failure && (
          <SettingsSearchBlock id="remote-access.failed">
            <div className="groove-b py-4">
              <ErrorAlert
                announce={hasPainted}
                title={t("settings.remoteAccess.failed.titleWays")}
                details={<DiagnosticBlock>{failure.detail}</DiagnosticBlock>}
              >
                <Message id={FAILURE_REASON[failure.reason]} />
              </ErrorAlert>
            </div>
          </SettingsSearchBlock>
        )}
        {/* The port is the user's setting whether or not the daemon serves: while it does not, it stays here under the
            switch, to be set before turning remote access on; while it does, it is a part of the Browser tab. */}
        {!serving && portRow}
      </LeadRows>
      {serving && (appShown || browserShown) && (
        <Tabs variant="underline" selectedKey={shownTab} disabledKeys={disabledTabs} onSelectionChange={(key) => setTab(key as WayIn)} className="gap-0">
          <TextTabStrip
            underline
            label={t("settings.remoteAccess.tabs.label")}
            tabs={[
              { id: "app", label: t("settings.remoteAccess.tabs.app") },
              { id: "browser", label: t("settings.remoteAccess.tabs.browser") },
            ]}
          />
          <SettingsTabPanel id="app">
            {remoteAccess.connection !== null && (
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
                    <CopyCommand text={remoteAccess.connection.text} copyText={t("settings.remoteAccess.connection.copy")} failedText={t("settings.remoteAccess.connection.copyFailed")} />
                    <QrCode rows={remoteAccess.connection.qr} label={t("settings.remoteAccess.connection.qr")} />
                  </div>
                }
              />
            )}
          </SettingsTabPanel>
          <SettingsTabPanel id="browser">
            {remoteAccess.forward_command !== null && (
              <SettingRow
                searchId="remote-access.forward"
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
                    command={remoteAccess.forward_command}
                    addresses={addresses}
                    copyText={t("settings.remoteAccess.forward.copy")}
                    failedText={t("settings.remoteAccess.forward.copyFailed")}
                  />
                }
              />
            )}
            {remoteAccess.address !== null && (
              <SettingRow
                searchId="remote-access.address"
                label={t("settings.remoteAccess.address.label")}
                description={t("settings.remoteAccess.address.description")}
                below={<CopyCommand text={remoteAccess.address} copyText={t("settings.remoteAccess.address.copy")} failedText={t("settings.remoteAccess.address.copyFailed")} />}
              />
            )}
            {portRow}
            <SettingRow
              compact
              searchId="remote-access.rotate"
              label={t("settings.remoteAccess.rotate.label")}
              description={t("settings.remoteAccess.rotate.description")}
            >
              <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => setConfirmingRotate(true)}>
                {t("settings.remoteAccess.rotate.label")}
              </Button>
            </SettingRow>
          </SettingsTabPanel>
        </Tabs>
      )}
      {/* The state is the row's description too; this says a change of it, made in the system's settings while the
          user was out of the window or as they come back. Where it cannot be told nothing is said. */}
      <AttendedStatusAnnouncer text={loginOn === undefined ? undefined : t(loginOn ? sshServer.announcedOn : sshServer.announcedOff)} />
      {confirmingAllow && (
        <ConfirmDialog
          title={t("settings.remoteAccess.confirm.title")}
          errorTitle={t("settings.remoteAccess.allow.failedTitle")}
          message={
            <>
              <p>{t("settings.remoteAccess.confirm.access")}</p>
              <p>{t("settings.remoteAccess.confirm.key")}</p>
              <p>{t("settings.remoteAccess.confirm.ssh")}</p>
              {browserPort !== null && (
                <>
                  <p>{t("settings.remoteAccess.confirm.port")}</p>
                  <p>{t("settings.remoteAccess.confirm.portKey")}</p>
                </>
              )}
            </>
          }
          confirmLabel={t("settings.remoteAccess.confirm.confirm")}
          destructive
          onConfirm={async () => {
            await request({ type: "set_remote_access", allowed: true });
            setConfirmingAllow(false);
          }}
          onCancel={() => setConfirmingAllow(false)}
        />
      )}
      {changingPort && browserPort !== null && (
        <PortDialog
          initialPort={browserPort}
          onSubmit={async (port) => {
            await request({ type: "set_remote_access_port", port });
          }}
          onClose={() => setChangingPort(false)}
        />
      )}
      {confirmingRotate && (
        <ConfirmDialog
          title={t("settings.remoteAccess.rotate.confirmTitle")}
          errorTitle={t("settings.remoteAccess.rotate.failedTitle")}
          message={t("settings.remoteAccess.rotate.confirmMessage")}
          confirmLabel={t("settings.remoteAccess.rotate.confirm")}
          onConfirm={async () => {
            await request({ type: "rotate_owner_key" });
            setConfirmingRotate(false);
          }}
          onCancel={() => setConfirmingRotate(false)}
        />
      )}
    </>
  );
}

/** The dialog that changes the browser port on Windows: one field, a whole number from 1 to 65535, sent on Save; the
 * dialog closes once the daemon has taken it, and shows the failure otherwise. A port already in use is no failure of
 * the request: the section says so once the daemon has tried it. */
function PortDialog({ initialPort, onSubmit, onClose }: {
  initialPort: number;
  onSubmit: (port: number) => Promise<void>;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const [value, setValue] = useState(String(initialPort));
  const { busy, run } = useDialogAction(undefined, t("settings.remoteAccess.port.failedTitle"));
  const { shown, attempt } = useSubmitValidation();
  const port = parsePort(value);
  const valueError = port === undefined ? t("settings.remoteAccess.port.invalid") : undefined;

  const submit = () => {
    if (!attempt(valueError) || port === undefined) return;
    void run(async () => {
      await onSubmit(port);
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
