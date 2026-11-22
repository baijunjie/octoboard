import React, { useId } from "react";

import { FormCard } from "../components/card/FormCard";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { RemoteDaemonEntry } from "../platform";
import { SettingRow, SettingSwitch } from "../settings/SettingRow";
import { useRemoteDevices } from "./RemoteDevices";

/**
 * The card of a saved remote daemon that says whether this computer's own key is installed on its server, and turns
 * signing in with it on or off. The switch is on while the key is installed, and also while it is not but the next
 * connection would ask before installing it; it is off once the owner said no to that question or turned it off here.
 * Its line says which: installed, and for which user and host; not installed yet, and that the next connection asks;
 * or, off, what turning it on does. Turning it off where a key is installed asks first, since that removes the key
 * from the server and from this computer (`RemoteDevicesProvider`); turning it on installs nothing by itself. The
 * switch is on in two of the three states, so its line is its accessible description: a screen reader that lands on
 * the switch hears which of them it is.
 */
export function RemoteDeviceKeyCard({ entry }: { entry: RemoteDaemonEntry }): React.ReactElement {
  const t = useT();
  const descriptionId = useId();
  const { setDeviceKey } = useRemoteDevices();
  const installed = entry.auth === "device";
  const on = installed || !entry.deviceKey.declined;
  const label = t("remoteDaemons.form.auth.device");
  let description: React.ReactNode = t("remoteDaemons.deviceKey.off");
  if (installed) {
    description = <Message id="remoteDaemons.deviceKey.installed" params={{ destination: <InlineReference monospace dir="ltr" wrap>{`${entry.user}@${entry.host}`}</InlineReference> }} />;
  } else if (on) {
    description = t("remoteDaemons.deviceKey.asks");
  }
  return (
    <FormCard>
      {/* The row's own padding is the card's. */}
      <div className="-my-4">
        <SettingRow label={label} description={<span id={descriptionId}>{description}</span>} compact>
          <SettingSwitch label={label} describedBy={descriptionId} isSelected={on} onChange={(enabled) => setDeviceKey(entry, enabled)} />
        </SettingRow>
      </div>
    </FormCard>
  );
}
