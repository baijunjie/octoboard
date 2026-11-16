import { useEffect, useState } from "react";

import { useStableHandler } from "../components/hooks/useLatest";
import type { Location } from "../navigation/history";
import type { RemoteDaemonsCapability } from "../platform";
import { remoteDevicePreference } from "../remoteDaemons/remoteDevicePreference";
import { useRemoteDaemons } from "../remoteDaemons/useRemoteDaemons";

/**
 * Whether the Remote devices page shows, and the device it shows, for the application's own window (`capability`
 * is absent in any other window, which has no such page).
 *
 * The page is a page of its own beside a console's sessions. It is where the user went (the rail's button, a
 * menu's Remote Devices item, a question a connection left for it), and it shows over the console and the session
 * that were current then: it is gone as soon as either changes, in the very render that changes it, by whatever
 * way (a console's avatar, a session chosen from a notification, Back), so each of them leaves it as it would leave
 * any other page. The device shown is the one last chosen while it is still saved, else the first saved; choosing
 * one is remembered per daemon (`remember`).
 *
 * The page is also keyed on whether the local daemon can be used (`offline`). A page opened while it could not be is
 * gone once it can. A page opened while it could stays, re-anchored to what is current then, whenever the daemon
 * becomes unusable or usable again, since remote work does not depend on it; the form on it is kept.
 */
export function useRemoteDevicesPage({ capability, consoleId, sessionId, offline }: {
  capability: RemoteDaemonsCapability | undefined;
  /** The current console and the selected session, which the page shows over. */
  consoleId: string | undefined;
  sessionId: string | undefined;
  /** The local daemon cannot be used, so there are no consoles and sessions to show over. */
  offline: boolean;
}) {
  const list = useRemoteDaemons(capability);
  const [page, setPage] = useState<{ console: string | undefined; session: string | undefined; offline: boolean; openedOffline: boolean }>();
  const [chosen, setChosen] = useState(() => remoteDevicePreference.get());
  // A new device's form is open in the content area, which the top bar's trail does not name a device for.
  const [adding, setAdding] = useState(false);
  // A page opened while the daemon was usable stays when that changes (remote work does not depend on it), taking the
  // anchor of what is current then; one opened while it was not is gone once it can be used.
  const reanchors = page !== undefined && !page.openedOffline && page.offline !== offline;
  const shown =
    capability !== undefined &&
    page !== undefined &&
    (reanchors || (page.offline === offline && page.console === consoleId && page.session === sessionId));
  const device = (list.entries.find((entry) => entry.id === chosen) ?? list.entries[0])?.id;
  useEffect(() => {
    if (!page) return;
    if (!shown) setPage(undefined);
    else if (reanchors) setPage({ console: consoleId, session: sessionId, offline, openedOffline: false });
  });
  const remember = (id: string) => {
    setChosen(id);
    remoteDevicePreference.set(id);
  };
  return {
    list,
    shown,
    device,
    adding,
    setAdding,
    remember,
    /** Shows the page over what is current now. */
    show: useStableHandler(() => setPage((current) => (shown ? current : { console: consoleId, session: sessionId, offline, openedOffline: offline }))),
    leave: () => setPage(undefined),
    /** Sets whether the page shows, and the device it shows, to what a history entry says. */
    follow: (target: Location) => {
      if (target.devices === undefined) {
        setPage(undefined);
        return;
      }
      setPage({ console: target.console, session: target.session, offline, openedOffline: offline });
      if (target.devices) remember(target.devices);
    },
  };
}
