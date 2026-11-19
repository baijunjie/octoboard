import type { Location } from "../navigation/history";
import type { RemoteDaemonsCapability } from "../platform";
import { remoteDevicePreference } from "../remoteDaemons/remoteDevicePreference";
import { useRemoteDaemons } from "../remoteDaemons/useRemoteDaemons";
import { useAnchoredPage } from "./useAnchoredPage";

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
 *
 * While a form on the page holds changes that are not saved (`held`) the page also stays, re-anchored, whenever what
 * it was anchored to changes without the user asking (the session under it deleted, the daemon becoming usable): such
 * a change never takes the form away. A page the user leaves asks first, and a confirmed leave calls `leave` or
 * `follow` itself, so `held` never brings a left page back.
 *
 * The anchoring, the choice kept per daemon and whether a new device's form is open (`adding`) are
 * `useAnchoredPage`'s; this hook adds the terms of the daemon being usable or not.
 */
export function useRemoteDevicesPage({ capability, consoleId, sessionId, offline, held }: {
  capability: RemoteDaemonsCapability | undefined;
  /** The current console and the selected session, which the page shows over. */
  consoleId: string | undefined;
  sessionId: string | undefined;
  /** The local daemon cannot be used, so there are no consoles and sessions to show over. */
  offline: boolean;
  /** A form on the page holds changes that are not saved. */
  held: boolean;
}) {
  const list = useRemoteDaemons(capability);
  // A page opened while the daemon was usable stays when that changes (remote work does not depend on it), taking the
  // anchor of what is current then; one opened while it was not is gone once it can be used, unless a form on it holds
  // changes, which then stays as an ordinary page.
  const page = useAnchoredPage({
    consoleId,
    sessionId,
    held,
    available: capability !== undefined,
    preference: remoteDevicePreference,
    stamp: (reanchoring) => ({ offline, openedOffline: reanchoring ? false : offline }),
    matches: (anchor) => anchor.offline === offline,
    moves: (anchor) => !anchor.openedOffline && anchor.offline !== offline,
  });
  const device = (list.entries.find((entry) => entry.id === page.chosen) ?? list.entries[0])?.id;
  return {
    list,
    shown: page.shown,
    device,
    adding: page.adding,
    startAdd: page.startAdd,
    cancelAdd: page.cancelAdd,
    select: page.select,
    show: page.show,
    leave: page.leave,
    follow: (target: Location) => page.follow(target, target.devices),
  };
}
