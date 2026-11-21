import React from "react";

import { AttendedStatusAnnouncer } from "../components/announce/AttendedStatusAnnouncer";
import { useHasPainted } from "../components/motion/useHasPainted";
import { Message, useT } from "../i18n/react";
import { daemonOs } from "../i18n/platformTerms";
import { useDaemonStore } from "../store";
import { anyServing, hasBrowserPort, servedBrowserAddress, servedConnection } from "./accessState";
import { BrowserAccessGroup } from "./BrowserAccessGroup";
import { OtherComputersGroup } from "./OtherComputersGroup";
import { RotateKeyRow } from "./RotateKeyRow";
import { SettingsGroup } from "./SettingsGroup";
import { SettingsLead } from "./SettingsLead";
import { sshServerWords } from "./sshServerWords";
import { useRemoteLogin } from "./useRemoteLogin";
import { useSectionRefocus } from "./useSectionRefocus";

/**
 * Access to this computer's own daemon, in three groups by where it is used: `BrowserAccessGroup` (This computer's
 * browser, only where the daemon has a browser port), `OtherComputersGroup` (the SSH server and the ways in from another
 * computer) and Key, Rotate key, shown while either way in serves, since the one owner key is shared (`RotateKeyRow`).
 * Listed only in the application's window (`remoteAccessOffered`). The switches are not settings of the configuration
 * document: the daemon keeps them apart (`AccessSwitchRow`). The section holds what the groups share: the lead, which
 * speaks of the browser only where the daemon has a browser port, whether Remote Login is on (a change of it, which
 * the user makes in System Settings, is announced when the user returns to the window), and the focus handoff for the
 * controls that go away with the daemon's state.
 */
export function RemoteAccessSection(): React.ReactElement {
  const t = useT();
  const remoteAccess = useDaemonStore((s) => s.remoteAccess);
  // The section is in the local window only, so the daemon whose section it is runs on this computer.
  const sshServer = sshServerWords(daemonOs());
  const loginOn = useRemoteLogin();
  // A failure that appears once the section has painted is announced; one already there as it opens is not.
  const hasPainted = useHasPainted();
  // What unmounts under focus: Rotate key when serving ends, Open System Settings when Remote Login turns on, the
  // browser group's local address controls when the port stops serving, and the connection details' with remote access.
  useSectionRefocus([anyServing(remoteAccess), loginOn, servedBrowserAddress(remoteAccess) !== null, servedConnection(remoteAccess) !== null]);

  if (!remoteAccess) return <></>;
  const browserPort = hasBrowserPort(remoteAccess);

  return (
    <>
      <SettingsLead><Message id={browserPort ? "settings.remoteAccess.lead" : "settings.remoteAccess.leadApp"} /></SettingsLead>
      {hasBrowserPort(remoteAccess) && <BrowserAccessGroup remoteAccess={remoteAccess} announce={hasPainted} />}
      <OtherComputersGroup remoteAccess={remoteAccess} loginOn={loginOn} announce={hasPainted} />
      {anyServing(remoteAccess) && (
        <SettingsGroup title="settings.remoteAccess.group.key">
          <RotateKeyRow browser={browserPort} />
        </SettingsGroup>
      )}
      {/* Announce changes made in System Settings when the user returns to the window. */}
      <AttendedStatusAnnouncer text={loginOn === undefined ? undefined : t(loginOn ? sshServer.announcedOn : sshServer.announcedOff)} />
    </>
  );
}
