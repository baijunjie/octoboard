import { Server } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { remoteDaemonsOffered } from "../remoteDaemons/remoteDaemonsOffered";
import { useRemoteDevices } from "../remoteDaemons/RemoteDevices";
import { ChromeButton } from "./ChromeButton";
import { RailCountBadge } from "./RailCountBadge";

/** The id of the button, which focus is handed to by identity (`focusNewConsole`). */
export const REMOTE_DEVICES_BUTTON_ID = "rail-remote-devices";

/**
 * The rail's Remote devices button, under the consoles and a separator (`RailSeparator`), in the application's own window
 * (`remoteDaemonsOffered`): a press shows the Remote devices page in place of the session UI, and a press while the page
 * shows does nothing, since the page is left by going to another page (a console's avatar, Back). While the page shows
 * the button is drawn with the current fills and reports `aria-current`, and no console's avatar is ringed as current. It is a plain
 * rail button with a tooltip, no menu: what the page lists is the saved remote daemons. A badge counts the connections
 * held, so the rail tells at a glance that something is connected. Disconnecting is the menu bar icon's tab's, or closing
 * the daemon's window.
 */
export function RailRemoteDevices({ shown, onPress }: { shown: boolean; onPress: () => void }): React.ReactElement | null {
  return remoteDaemonsOffered(usePlatform()) ? <RemoteDevicesButton shown={shown} onPress={onPress} /> : null;
}

function RemoteDevicesButton({ shown, onPress }: { shown: boolean; onPress: () => void }): React.ReactElement {
  const t = useT();
  const held = useRemoteDevices().statuses.length;
  return (
    <ChromeButton
      id={REMOTE_DEVICES_BUTTON_ID}
      label={held > 0 ? t("rail.remoteDevices.count", { count: held }) : t("remoteDevices.label")}
      tooltipPlacement="end"
      current={shown}
      onPress={onPress}
      badge={held > 0 ? <RailCountBadge count={held} overflowLabel={t("rail.countBadgeOverflow")} color="success" /> : undefined}
    >
      <Server className="size-4" />
    </ChromeButton>
  );
}
